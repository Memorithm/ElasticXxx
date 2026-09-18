//! Solver-neutral boundary for optional bounded symbolic analysis.
//!
//! This module is analysis-only. It deliberately has no validation, actuation,
//! commit, or publication method. `Unknown`, `Timeout`, and `ResourceLimit`
//! remain explicit non-results.

use core::fmt;

use crate::BoolExpr;

pub const DEFAULT_SYMBOLIC_TIMEOUT_MILLIS: u64 = 1_000;
pub const DEFAULT_SYMBOLIC_MAX_CLAUSES: u64 = 100_000;
pub const DEFAULT_SYMBOLIC_MAX_NODES: u64 = 1_000_000;
pub const DEFAULT_SYMBOLIC_MAX_MEMORY_BYTES: u64 = 256 * 1024 * 1024;
pub const DEFAULT_SYMBOLIC_SEED: u64 = 0;

/// Resource category exhausted by a backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SymbolicResource {
    Clauses,
    Nodes,
    Memory,
    Other,
}

/// Solver-neutral result of one bounded satisfiability query.
///
/// `Sat` is static-analysis evidence only; it is never actuation permission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SymbolicBackendResult {
    Sat,
    Unsat,
    Unknown,
    Timeout,
    ResourceLimit(SymbolicResource),
}

impl SymbolicBackendResult {
    #[must_use]
    pub const fn is_conclusive(self) -> bool {
        matches!(self, Self::Sat | Self::Unsat)
    }

    #[must_use]
    pub const fn is_non_result(self) -> bool {
        matches!(self, Self::Unknown | Self::Timeout | Self::ResourceLimit(_))
    }
}

/// Finite deterministic resource policy supplied to a symbolic backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SymbolicBackendConfig {
    timeout_millis: u64,
    max_clauses: u64,
    max_nodes: u64,
    max_memory_bytes: u64,
    seed: u64,
}

impl SymbolicBackendConfig {
    pub const fn new(
        timeout_millis: u64,
        max_clauses: u64,
        max_nodes: u64,
        max_memory_bytes: u64,
        seed: u64,
    ) -> Result<Self, SymbolicBackendConfigError> {
        if timeout_millis == 0 {
            return Err(SymbolicBackendConfigError::ZeroTimeout);
        }
        if max_clauses == 0 {
            return Err(SymbolicBackendConfigError::ZeroClauseLimit);
        }
        if max_nodes == 0 {
            return Err(SymbolicBackendConfigError::ZeroNodeLimit);
        }
        if max_memory_bytes == 0 {
            return Err(SymbolicBackendConfigError::ZeroMemoryLimit);
        }
        Ok(Self {
            timeout_millis,
            max_clauses,
            max_nodes,
            max_memory_bytes,
            seed,
        })
    }

    #[must_use]
    pub const fn timeout_millis(self) -> u64 {
        self.timeout_millis
    }
    #[must_use]
    pub const fn max_clauses(self) -> u64 {
        self.max_clauses
    }
    #[must_use]
    pub const fn max_nodes(self) -> u64 {
        self.max_nodes
    }
    #[must_use]
    pub const fn max_memory_bytes(self) -> u64 {
        self.max_memory_bytes
    }
    #[must_use]
    pub const fn seed(self) -> u64 {
        self.seed
    }
}

impl Default for SymbolicBackendConfig {
    fn default() -> Self {
        Self {
            timeout_millis: DEFAULT_SYMBOLIC_TIMEOUT_MILLIS,
            max_clauses: DEFAULT_SYMBOLIC_MAX_CLAUSES,
            max_nodes: DEFAULT_SYMBOLIC_MAX_NODES,
            max_memory_bytes: DEFAULT_SYMBOLIC_MAX_MEMORY_BYTES,
            seed: DEFAULT_SYMBOLIC_SEED,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SymbolicBackendConfigError {
    ZeroTimeout,
    ZeroClauseLimit,
    ZeroNodeLimit,
    ZeroMemoryLimit,
}

impl fmt::Display for SymbolicBackendConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let field = match self {
            Self::ZeroTimeout => "timeout_millis",
            Self::ZeroClauseLimit => "max_clauses",
            Self::ZeroNodeLimit => "max_nodes",
            Self::ZeroMemoryLimit => "max_memory_bytes",
        };
        write!(f, "symbolic backend requires non-zero {field}")
    }
}

impl std::error::Error for SymbolicBackendConfigError {}

/// Optional SAT backend for static analysis only.
pub trait SymbolicBackend {
    /// Stable backend identifier for diagnostics.
    fn backend_id(&self) -> &'static str;

    /// Solve one SAT query under explicit finite limits.
    ///
    /// Implementations must preserve inconclusive states as non-results.
    fn solve(&self, expression: &BoolExpr, config: SymbolicBackendConfig) -> SymbolicBackendResult;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scripted(SymbolicBackendResult);

    impl SymbolicBackend for Scripted {
        fn backend_id(&self) -> &'static str {
            "scripted"
        }
        fn solve(&self, _: &BoolExpr, _: SymbolicBackendConfig) -> SymbolicBackendResult {
            self.0
        }
    }

    #[test]
    fn defaults_are_finite_and_seeded() {
        let c = SymbolicBackendConfig::default();
        assert!(c.timeout_millis() > 0 && c.max_clauses() > 0);
        assert!(c.max_nodes() > 0 && c.max_memory_bytes() > 0);
        assert_eq!(c.seed(), DEFAULT_SYMBOLIC_SEED);
    }

    #[test]
    fn zero_limits_fail_closed() {
        assert_eq!(
            SymbolicBackendConfig::new(0, 1, 1, 1, 0),
            Err(SymbolicBackendConfigError::ZeroTimeout)
        );
        assert_eq!(
            SymbolicBackendConfig::new(1, 0, 1, 1, 0),
            Err(SymbolicBackendConfigError::ZeroClauseLimit)
        );
        assert_eq!(
            SymbolicBackendConfig::new(1, 1, 0, 1, 0),
            Err(SymbolicBackendConfigError::ZeroNodeLimit)
        );
        assert_eq!(
            SymbolicBackendConfig::new(1, 1, 1, 0, 0),
            Err(SymbolicBackendConfigError::ZeroMemoryLimit)
        );
    }

    #[test]
    fn inconclusive_results_stay_non_results() {
        for result in [
            SymbolicBackendResult::Unknown,
            SymbolicBackendResult::Timeout,
            SymbolicBackendResult::ResourceLimit(SymbolicResource::Memory),
        ] {
            let observed =
                Scripted(result).solve(&BoolExpr::Const(true), SymbolicBackendConfig::default());
            assert!(observed.is_non_result());
            assert!(!observed.is_conclusive());
        }
    }

    #[test]
    fn sat_and_unsat_are_analysis_conclusions_only() {
        for result in [SymbolicBackendResult::Sat, SymbolicBackendResult::Unsat] {
            let backend = Scripted(result);
            assert_eq!(backend.backend_id(), "scripted");
            assert!(backend
                .solve(&BoolExpr::Const(true), SymbolicBackendConfig::default())
                .is_conclusive());
        }
    }
}
