//! Strict versioned operator configuration for Boolean guards.
//!
//! Durable configuration uses stable [`PredicateKey`] identities rather than
//! process-local `PredicateId` values. Lowering is deterministic and produces
//! the same public [`BooleanGuard`] and [`ObservationThresholdPredicate`]
//! semantics used by programmatic callers. Configuration is descriptive and
//! non-actuating.

use std::collections::BTreeSet;
use std::time::Duration;

use elastic_core::resource::{DimensionId, ObservationSignalId};
use elastic_core::{
    BoolExpr, BooleanGuard, GuardScope, PredicateKey, PredicateRegistry, TransitionMechanism,
    MAX_BOOLEAN_EXPR_DEPTH, MAX_CANONICAL_EXPRESSION_NODES, MAX_REGISTERED_PREDICATES,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{ObservationThresholdPredicate, ThresholdComparison};

/// First durable Boolean guard-configuration schema.
pub const GUARD_CONFIG_SCHEMA_V1: u32 = 1;
/// Maximum accepted encoded guard configuration.
pub const MAX_GUARD_CONFIG_BYTES: usize = 256 * 1024;
/// Maximum guards in one configuration document.
pub const MAX_GUARD_CONFIG_GUARDS: usize = 256;
/// Maximum byte length of a unit or custom term carried by configuration.
pub const MAX_GUARD_CONFIG_TERM_BYTES: usize = 64;
/// Maximum expression nodes in one configured guard.
pub const MAX_GUARD_CONFIG_EXPR_NODES: usize = MAX_CANONICAL_EXPRESSION_NODES;

/// Strict versioned Boolean guard configuration.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuardConfigV1 {
    pub schema_version: u32,
    pub predicates: Vec<PredicateConfigV1>,
    pub guards: Vec<GuardRuleConfigV1>,
}

impl GuardConfigV1 {
    /// Decode bounded JSON, validate all durable identities, and reject future schemas.
    pub fn from_bounded_json(bytes: &[u8]) -> Result<Self, GuardConfigError> {
        validate_json_preallocation_bounds(bytes)?;
        let config: Self = serde_json::from_slice(bytes)
            .map_err(|error| GuardConfigError::Decode(error.to_string()))?;
        config.validate()?;
        Ok(config)
    }

    /// Encode validated configuration as deterministic compact JSON.
    pub fn to_bounded_json(&self) -> Result<String, GuardConfigError> {
        self.validate()?;
        let encoded = serde_json::to_string(self)
            .map_err(|error| GuardConfigError::Encode(error.to_string()))?;
        if encoded.len() > MAX_GUARD_CONFIG_BYTES {
            return Err(GuardConfigError::TooLarge {
                max_bytes: MAX_GUARD_CONFIG_BYTES,
                actual_bytes: encoded.len(),
            });
        }
        Ok(encoded)
    }

    /// Validate the complete schema without performing observation or actuation.
    pub fn validate(&self) -> Result<(), GuardConfigError> {
        if self.schema_version != GUARD_CONFIG_SCHEMA_V1 {
            return Err(GuardConfigError::UnsupportedSchema {
                actual: self.schema_version,
                supported: GUARD_CONFIG_SCHEMA_V1,
            });
        }
        if self.predicates.len() > MAX_REGISTERED_PREDICATES {
            return Err(GuardConfigError::TooManyPredicates {
                max: MAX_REGISTERED_PREDICATES,
                actual: self.predicates.len(),
            });
        }
        if self.guards.len() > MAX_GUARD_CONFIG_GUARDS {
            return Err(GuardConfigError::TooManyGuards {
                max: MAX_GUARD_CONFIG_GUARDS,
                actual: self.guards.len(),
            });
        }

        let mut keys = BTreeSet::new();
        for predicate in &self.predicates {
            predicate.validate()?;
            let key = predicate.key().to_core()?;
            if !keys.insert(key.clone()) {
                return Err(GuardConfigError::DuplicatePredicate(key.to_string()));
            }
        }
        for guard in &self.guards {
            guard.validate(&keys)?;
        }
        Ok(())
    }

    /// Lower durable stable-key configuration to public runtime/core semantics.
    ///
    /// No observer is sampled and no physical effect is performed.
    pub fn lower(&self) -> Result<LoweredGuardConfigV1, GuardConfigError> {
        self.validate()?;
        let keys = self
            .predicates
            .iter()
            .map(|predicate| predicate.key().to_core())
            .collect::<Result<Vec<_>, _>>()?;
        let registry = PredicateRegistry::from_keys(keys.clone())
            .map_err(|error| GuardConfigError::Lowering(error.to_string()))?;

        let predicates = self
            .predicates
            .iter()
            .map(PredicateConfigV1::lower)
            .collect::<Result<Vec<_>, _>>()?;
        let guards = self
            .guards
            .iter()
            .map(|guard| guard.lower(&registry))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(LoweredGuardConfigV1 {
            registry,
            predicates,
            guards,
        })
    }
}

/// One configured predicate. Additional kinds must be versioned explicitly.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum PredicateConfigV1 {
    ObservationThreshold {
        key: PredicateKeyConfigV1,
        signal: ObservationSignalConfigV1,
        comparison: ThresholdComparisonConfigV1,
        threshold: f64,
        unit: String,
        max_age_ms: u64,
    },
}

impl PredicateConfigV1 {
    #[must_use]
    pub const fn key(&self) -> &PredicateKeyConfigV1 {
        match self {
            Self::ObservationThreshold { key, .. } => key,
        }
    }

    fn validate(&self) -> Result<(), GuardConfigError> {
        match self {
            Self::ObservationThreshold {
                key,
                signal,
                threshold,
                unit,
                ..
            } => {
                key.to_core()?;
                signal.to_core()?;
                validate_bounded_token("unit", unit)?;
                if !threshold.is_finite() {
                    return Err(GuardConfigError::NonFiniteThreshold(key.display()));
                }
            }
        }
        Ok(())
    }

    fn lower(&self) -> Result<ConfiguredThresholdPredicateV1, GuardConfigError> {
        match self {
            Self::ObservationThreshold {
                key,
                signal,
                comparison,
                threshold,
                unit,
                max_age_ms,
            } => {
                let key = key.to_core()?;
                let signal = signal.to_core()?;
                let evaluator = ObservationThresholdPredicate::new(
                    key.clone(),
                    signal.clone(),
                    (*comparison).into(),
                    *threshold,
                    Duration::from_millis(*max_age_ms),
                )
                .map_err(|error| GuardConfigError::Lowering(error.to_string()))?;
                Ok(ConfiguredThresholdPredicateV1 {
                    key,
                    signal,
                    unit: unit.clone(),
                    evaluator,
                })
            }
        }
    }
}

/// Stable durable predicate key. No compact `PredicateId` is serialized.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PredicateKeyConfigV1 {
    pub namespace: String,
    pub name: String,
}

impl PredicateKeyConfigV1 {
    pub fn to_core(&self) -> Result<PredicateKey, GuardConfigError> {
        PredicateKey::new(self.namespace.clone(), self.name.clone())
            .map_err(|error| GuardConfigError::InvalidPredicateKey(error.to_string()))
    }

    fn display(&self) -> String {
        format!("{}::{}", self.namespace, self.name)
    }
}

/// Stable observation-signal identity preserving builtin/custom distinction.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ObservationSignalConfigV1 {
    Builtin {
        name: BuiltinObservationSignalConfigV1,
    },
    Custom {
        name: String,
    },
}

impl ObservationSignalConfigV1 {
    pub fn to_core(&self) -> Result<ObservationSignalId, GuardConfigError> {
        match self {
            Self::Builtin { name } => Ok((*name).into()),
            Self::Custom { name } => {
                validate_bounded_token("custom observation signal", name)?;
                ObservationSignalId::custom(name.clone())
                    .map_err(|error| GuardConfigError::InvalidTerm(error.to_string()))
            }
        }
    }
}

/// Built-in observation signals accepted by schema v1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BuiltinObservationSignalConfigV1 {
    FreeCapacity,
    Utilization,
    QueueDepth,
    LatencySample,
    ThermalMargin,
    EnergyRate,
    TopologyChange,
}

impl From<BuiltinObservationSignalConfigV1> for ObservationSignalId {
    fn from(value: BuiltinObservationSignalConfigV1) -> Self {
        match value {
            BuiltinObservationSignalConfigV1::FreeCapacity => Self::FREE_CAPACITY,
            BuiltinObservationSignalConfigV1::Utilization => Self::UTILIZATION,
            BuiltinObservationSignalConfigV1::QueueDepth => Self::QUEUE_DEPTH,
            BuiltinObservationSignalConfigV1::LatencySample => Self::LATENCY_SAMPLE,
            BuiltinObservationSignalConfigV1::ThermalMargin => Self::THERMAL_MARGIN,
            BuiltinObservationSignalConfigV1::EnergyRate => Self::ENERGY_RATE,
            BuiltinObservationSignalConfigV1::TopologyChange => Self::TOPOLOGY_CHANGE,
        }
    }
}

/// Durable comparison operator for one threshold predicate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThresholdComparisonConfigV1 {
    LessThan,
    LessOrEqual,
    GreaterThan,
    GreaterOrEqual,
}

impl From<ThresholdComparisonConfigV1> for ThresholdComparison {
    fn from(value: ThresholdComparisonConfigV1) -> Self {
        match value {
            ThresholdComparisonConfigV1::LessThan => Self::LessThan,
            ThresholdComparisonConfigV1::LessOrEqual => Self::LessOrEqual,
            ThresholdComparisonConfigV1::GreaterThan => Self::GreaterThan,
            ThresholdComparisonConfigV1::GreaterOrEqual => Self::GreaterOrEqual,
        }
    }
}

/// One stable-key guard declaration.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuardRuleConfigV1 {
    pub scope: GuardScopeConfigV1,
    pub expression: GuardExprConfigV1,
}

impl GuardRuleConfigV1 {
    fn validate(&self, predicates: &BTreeSet<PredicateKey>) -> Result<(), GuardConfigError> {
        self.scope.to_core()?;
        let mut nodes = 0;
        self.expression.validate(predicates, 0, &mut nodes)
    }

    fn lower(&self, registry: &PredicateRegistry) -> Result<BooleanGuard, GuardConfigError> {
        let scope = self.scope.to_core()?;
        let expression = self.expression.lower(registry)?;
        BooleanGuard::new(scope, registry.clone(), expression)
            .map_err(|error| GuardConfigError::Lowering(error.to_string()))
    }
}

/// Durable guard scope.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum GuardScopeConfigV1 {
    Resource,
    Dimension {
        dimension: DimensionConfigV1,
    },
    Transition {
        mechanism: TransitionMechanismConfigV1,
        dimension: DimensionConfigV1,
    },
}

impl GuardScopeConfigV1 {
    pub fn to_core(&self) -> Result<GuardScope, GuardConfigError> {
        match self {
            Self::Resource => Ok(GuardScope::Resource),
            Self::Dimension { dimension } => Ok(GuardScope::Dimension(dimension.to_core()?)),
            Self::Transition {
                mechanism,
                dimension,
            } => Ok(GuardScope::Transition {
                mechanism: (*mechanism).into(),
                dimension: dimension.to_core()?,
            }),
        }
    }
}

/// Stable dimension identity preserving builtin/custom distinction.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum DimensionConfigV1 {
    Builtin { name: BuiltinDimensionConfigV1 },
    Custom { name: String },
}

impl DimensionConfigV1 {
    pub fn to_core(&self) -> Result<DimensionId, GuardConfigError> {
        match self {
            Self::Builtin { name } => Ok((*name).into()),
            Self::Custom { name } => {
                validate_bounded_token("custom dimension", name)?;
                DimensionId::custom(name.clone())
                    .map_err(|error| GuardConfigError::InvalidTerm(error.to_string()))
            }
        }
    }
}

/// Built-in elastic dimensions accepted by schema v1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BuiltinDimensionConfigV1 {
    Capacity,
    Concurrency,
    Residency,
    Locality,
    Representation,
    Precision,
    Parallelism,
    Routing,
    Redundancy,
    Persistence,
    Recomputability,
    Bandwidth,
    Energy,
}

impl From<BuiltinDimensionConfigV1> for DimensionId {
    fn from(value: BuiltinDimensionConfigV1) -> Self {
        match value {
            BuiltinDimensionConfigV1::Capacity => Self::CAPACITY,
            BuiltinDimensionConfigV1::Concurrency => Self::CONCURRENCY,
            BuiltinDimensionConfigV1::Residency => Self::RESIDENCY,
            BuiltinDimensionConfigV1::Locality => Self::LOCALITY,
            BuiltinDimensionConfigV1::Representation => Self::REPRESENTATION,
            BuiltinDimensionConfigV1::Precision => Self::PRECISION,
            BuiltinDimensionConfigV1::Parallelism => Self::PARALLELISM,
            BuiltinDimensionConfigV1::Routing => Self::ROUTING,
            BuiltinDimensionConfigV1::Redundancy => Self::REDUNDANCY,
            BuiltinDimensionConfigV1::Persistence => Self::PERSISTENCE,
            BuiltinDimensionConfigV1::Recomputability => Self::RECOMPUTABILITY,
            BuiltinDimensionConfigV1::Bandwidth => Self::BANDWIDTH,
            BuiltinDimensionConfigV1::Energy => Self::ENERGY,
        }
    }
}

/// Durable transition mechanism.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TransitionMechanismConfigV1 {
    Reinterpret,
    Reencode,
    Recompute,
}

impl From<TransitionMechanismConfigV1> for TransitionMechanism {
    fn from(value: TransitionMechanismConfigV1) -> Self {
        match value {
            TransitionMechanismConfigV1::Reinterpret => Self::Reinterpret,
            TransitionMechanismConfigV1::Reencode => Self::Reencode,
            TransitionMechanismConfigV1::Recompute => Self::Recompute,
        }
    }
}

/// Stable-key Boolean expression wire form.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case", deny_unknown_fields)]
pub enum GuardExprConfigV1 {
    Const { value: bool },
    Atom { predicate: PredicateKeyConfigV1 },
    Not { expression: Box<Self> },
    All { expressions: Vec<Self> },
    Any { expressions: Vec<Self> },
    Xor { left: Box<Self>, right: Box<Self> },
    Implies { left: Box<Self>, right: Box<Self> },
}

impl GuardExprConfigV1 {
    fn validate(
        &self,
        predicates: &BTreeSet<PredicateKey>,
        depth: usize,
        nodes: &mut usize,
    ) -> Result<(), GuardConfigError> {
        if depth > MAX_BOOLEAN_EXPR_DEPTH {
            return Err(GuardConfigError::ExpressionTooDeep {
                max_depth: MAX_BOOLEAN_EXPR_DEPTH,
            });
        }
        *nodes = nodes.saturating_add(1);
        if *nodes > MAX_GUARD_CONFIG_EXPR_NODES {
            return Err(GuardConfigError::ExpressionTooLarge {
                max_nodes: MAX_GUARD_CONFIG_EXPR_NODES,
            });
        }
        match self {
            Self::Const { .. } => Ok(()),
            Self::Atom { predicate } => {
                let key = predicate.to_core()?;
                if predicates.contains(&key) {
                    Ok(())
                } else {
                    Err(GuardConfigError::UnknownPredicate(key.to_string()))
                }
            }
            Self::Not { expression } => expression.validate(predicates, depth + 1, nodes),
            Self::All { expressions } | Self::Any { expressions } => expressions
                .iter()
                .try_for_each(|expression| expression.validate(predicates, depth + 1, nodes)),
            Self::Xor { left, right } | Self::Implies { left, right } => {
                left.validate(predicates, depth + 1, nodes)?;
                right.validate(predicates, depth + 1, nodes)
            }
        }
    }

    fn lower(&self, registry: &PredicateRegistry) -> Result<BoolExpr, GuardConfigError> {
        match self {
            Self::Const { value } => Ok(BoolExpr::Const(*value)),
            Self::Atom { predicate } => {
                let key = predicate.to_core()?;
                let id = registry
                    .id(&key)
                    .ok_or_else(|| GuardConfigError::UnknownPredicate(key.to_string()))?;
                Ok(BoolExpr::atom(id))
            }
            Self::Not { expression } => Ok(BoolExpr::negate(expression.lower(registry)?)),
            Self::All { expressions } => expressions
                .iter()
                .map(|expression| expression.lower(registry))
                .collect::<Result<Vec<_>, _>>()
                .map(BoolExpr::all),
            Self::Any { expressions } => expressions
                .iter()
                .map(|expression| expression.lower(registry))
                .collect::<Result<Vec<_>, _>>()
                .map(BoolExpr::any),
            Self::Xor { left, right } => Ok(BoolExpr::Xor(
                Box::new(left.lower(registry)?),
                Box::new(right.lower(registry)?),
            )),
            Self::Implies { left, right } => Ok(BoolExpr::Implies(
                Box::new(left.lower(registry)?),
                Box::new(right.lower(registry)?),
            )),
        }
    }
}

/// Lowered threshold predicate retaining explicit configuration unit metadata.
#[derive(Clone, Debug)]
pub struct ConfiguredThresholdPredicateV1 {
    key: PredicateKey,
    signal: ObservationSignalId,
    unit: String,
    evaluator: ObservationThresholdPredicate,
}

impl ConfiguredThresholdPredicateV1 {
    #[must_use]
    pub const fn key(&self) -> &PredicateKey {
        &self.key
    }

    #[must_use]
    pub const fn signal(&self) -> &ObservationSignalId {
        &self.signal
    }

    #[must_use]
    pub fn unit(&self) -> &str {
        &self.unit
    }

    #[must_use]
    pub const fn evaluator(&self) -> &ObservationThresholdPredicate {
        &self.evaluator
    }
}

/// Fully lowered public-library representation of one guard configuration.
#[derive(Clone, Debug)]
pub struct LoweredGuardConfigV1 {
    registry: PredicateRegistry,
    predicates: Vec<ConfiguredThresholdPredicateV1>,
    guards: Vec<BooleanGuard>,
}

impl LoweredGuardConfigV1 {
    #[must_use]
    pub const fn registry(&self) -> &PredicateRegistry {
        &self.registry
    }

    #[must_use]
    pub fn predicates(&self) -> &[ConfiguredThresholdPredicateV1] {
        &self.predicates
    }

    #[must_use]
    pub fn guards(&self) -> &[BooleanGuard] {
        &self.guards
    }
}

/// Guard configuration failures are fail-closed and never authorize actuation.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum GuardConfigError {
    #[error("guard configuration is {actual_bytes} bytes; maximum is {max_bytes}")]
    TooLarge {
        max_bytes: usize,
        actual_bytes: usize,
    },
    #[error("guard configuration JSON nesting exceeds maximum depth {max_depth}")]
    JsonTooDeep { max_depth: usize },
    #[error("failed to decode guard configuration: {0}")]
    Decode(String),
    #[error("failed to encode guard configuration: {0}")]
    Encode(String),
    #[error("unsupported guard configuration schema {actual}; supported schema is {supported}")]
    UnsupportedSchema { actual: u32, supported: u32 },
    #[error("guard configuration contains {actual} predicates; maximum is {max}")]
    TooManyPredicates { max: usize, actual: usize },
    #[error("guard configuration contains {actual} guards; maximum is {max}")]
    TooManyGuards { max: usize, actual: usize },
    #[error("duplicate configured predicate '{0}'")]
    DuplicatePredicate(String),
    #[error("guard references undeclared predicate '{0}'")]
    UnknownPredicate(String),
    #[error("invalid predicate key: {0}")]
    InvalidPredicateKey(String),
    #[error("invalid configured term: {0}")]
    InvalidTerm(String),
    #[error("{field} must be non-empty, trimmed, and at most {max_bytes} bytes")]
    InvalidToken {
        field: &'static str,
        max_bytes: usize,
    },
    #[error("configured threshold for '{0}' must be finite")]
    NonFiniteThreshold(String),
    #[error("guard expression exceeds maximum depth {max_depth}")]
    ExpressionTooDeep { max_depth: usize },
    #[error("guard expression exceeds maximum {max_nodes} nodes")]
    ExpressionTooLarge { max_nodes: usize },
    #[error("guard configuration lowering failed: {0}")]
    Lowering(String),
}

fn validate_bounded_token(field: &'static str, value: &str) -> Result<(), GuardConfigError> {
    if value.is_empty()
        || value.trim() != value
        || value.len() > MAX_GUARD_CONFIG_TERM_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(GuardConfigError::InvalidToken {
            field,
            max_bytes: MAX_GUARD_CONFIG_TERM_BYTES,
        });
    }
    Ok(())
}

fn validate_json_preallocation_bounds(bytes: &[u8]) -> Result<(), GuardConfigError> {
    if bytes.len() > MAX_GUARD_CONFIG_BYTES {
        return Err(GuardConfigError::TooLarge {
            max_bytes: MAX_GUARD_CONFIG_BYTES,
            actual_bytes: bytes.len(),
        });
    }

    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for byte in bytes.iter().copied() {
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'{' | b'[' => {
                depth = depth.saturating_add(1);
                if depth > MAX_BOOLEAN_EXPR_DEPTH + 8 {
                    return Err(GuardConfigError::JsonTooDeep {
                        max_depth: MAX_BOOLEAN_EXPR_DEPTH + 8,
                    });
                }
            }
            b'}' | b']' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(namespace: &str, name: &str) -> PredicateKeyConfigV1 {
        PredicateKeyConfigV1 {
            namespace: namespace.into(),
            name: name.into(),
        }
    }

    fn threshold(key: PredicateKeyConfigV1) -> PredicateConfigV1 {
        PredicateConfigV1::ObservationThreshold {
            key,
            signal: ObservationSignalConfigV1::Builtin {
                name: BuiltinObservationSignalConfigV1::Utilization,
            },
            comparison: ThresholdComparisonConfigV1::LessOrEqual,
            threshold: 0.8,
            unit: "fraction".into(),
            max_age_ms: 500,
        }
    }

    fn fixture() -> GuardConfigV1 {
        let healthy = key("elastic.ram", "healthy");
        GuardConfigV1 {
            schema_version: GUARD_CONFIG_SCHEMA_V1,
            predicates: vec![threshold(healthy.clone())],
            guards: vec![GuardRuleConfigV1 {
                scope: GuardScopeConfigV1::Transition {
                    mechanism: TransitionMechanismConfigV1::Reinterpret,
                    dimension: DimensionConfigV1::Builtin {
                        name: BuiltinDimensionConfigV1::Capacity,
                    },
                },
                expression: GuardExprConfigV1::Atom { predicate: healthy },
            }],
        }
    }

    #[test]
    fn bounded_json_roundtrip_and_lowering_preserve_stable_identity() {
        let config = fixture();
        let json = config.to_bounded_json().unwrap();
        assert!(!json.contains("predicate_id"));
        assert!(json.contains("elastic.ram"));

        let decoded = GuardConfigV1::from_bounded_json(json.as_bytes()).unwrap();
        assert_eq!(decoded, config);
        let lowered = decoded.lower().unwrap();
        assert_eq!(lowered.registry().len(), 1);
        assert_eq!(lowered.predicates()[0].unit(), "fraction");
        assert_eq!(lowered.guards().len(), 1);
        assert_eq!(
            lowered.guards()[0].scope(),
            &GuardScope::Transition {
                mechanism: TransitionMechanism::Reinterpret,
                dimension: DimensionId::CAPACITY,
            }
        );
    }

    #[test]
    fn unknown_duplicate_and_future_schema_fail_closed() {
        let unknown = br#"{"schema_version":1,"predicates":[],"guards":[],"extra":true}"#;
        assert!(matches!(
            GuardConfigV1::from_bounded_json(unknown),
            Err(GuardConfigError::Decode(_))
        ));

        let duplicate = br#"{"schema_version":1,"schema_version":1,"predicates":[],"guards":[]}"#;
        assert!(matches!(
            GuardConfigV1::from_bounded_json(duplicate),
            Err(GuardConfigError::Decode(_))
        ));

        let future = br#"{"schema_version":2,"predicates":[],"guards":[]}"#;
        assert_eq!(
            GuardConfigV1::from_bounded_json(future),
            Err(GuardConfigError::UnsupportedSchema {
                actual: 2,
                supported: 1,
            })
        );
    }

    #[test]
    fn duplicate_and_unknown_predicate_keys_are_rejected() {
        let healthy = key("elastic.ram", "healthy");
        let mut config = fixture();
        config.predicates.push(threshold(healthy));
        assert!(matches!(
            config.validate(),
            Err(GuardConfigError::DuplicatePredicate(_))
        ));

        let mut config = fixture();
        config.guards[0].expression = GuardExprConfigV1::Atom {
            predicate: key("elastic.ram", "missing"),
        };
        assert!(matches!(
            config.validate(),
            Err(GuardConfigError::UnknownPredicate(_))
        ));
    }

    #[test]
    fn nonfinite_threshold_and_unbounded_tokens_are_rejected() {
        let mut config = fixture();
        let PredicateConfigV1::ObservationThreshold { threshold, .. } = &mut config.predicates[0];
        *threshold = f64::NAN;
        assert!(matches!(
            config.validate(),
            Err(GuardConfigError::NonFiniteThreshold(_))
        ));

        let mut config = fixture();
        let PredicateConfigV1::ObservationThreshold { unit, .. } = &mut config.predicates[0];
        *unit = "x".repeat(MAX_GUARD_CONFIG_TERM_BYTES + 1);
        assert!(matches!(
            config.validate(),
            Err(GuardConfigError::InvalidToken { .. })
        ));
    }

    #[test]
    fn expression_and_encoded_input_bounds_are_checked_before_lowering() {
        let mut config = fixture();
        let atom = config.guards[0].expression.clone();
        let mut expression = atom;
        for _ in 0..=MAX_BOOLEAN_EXPR_DEPTH {
            expression = GuardExprConfigV1::Not {
                expression: Box::new(expression),
            };
        }
        config.guards[0].expression = expression;
        assert!(matches!(
            config.validate(),
            Err(GuardConfigError::ExpressionTooDeep { .. })
        ));

        let oversized = vec![b' '; MAX_GUARD_CONFIG_BYTES + 1];
        assert_eq!(
            GuardConfigV1::from_bounded_json(&oversized),
            Err(GuardConfigError::TooLarge {
                max_bytes: MAX_GUARD_CONFIG_BYTES,
                actual_bytes: oversized.len(),
            })
        );
    }

    #[test]
    fn raw_json_depth_and_nested_unknown_fields_fail_before_use() {
        let too_deep = format!(
            "{}0{}",
            "[".repeat(MAX_BOOLEAN_EXPR_DEPTH + 9),
            "]".repeat(MAX_BOOLEAN_EXPR_DEPTH + 9)
        );
        assert!(matches!(
            GuardConfigV1::from_bounded_json(too_deep.as_bytes()),
            Err(GuardConfigError::JsonTooDeep { .. })
        ));

        let nested_unknown = br#"{
          "schema_version":1,
          "predicates":[{
            "kind":"observation-threshold",
            "key":{"namespace":"elastic.ram","name":"healthy","raw_id":0},
            "signal":{"kind":"builtin","name":"utilization"},
            "comparison":"less-than",
            "threshold":0.8,
            "unit":"fraction",
            "max_age_ms":500
          }],
          "guards":[]
        }"#;
        assert!(matches!(
            GuardConfigV1::from_bounded_json(nested_unknown),
            Err(GuardConfigError::Decode(_))
        ));
    }

    #[test]
    fn predicate_and_guard_collection_limits_fail_closed() {
        let predicates = (0..=MAX_REGISTERED_PREDICATES)
            .map(|index| threshold(key("elastic.test", &format!("p{index}"))))
            .collect();
        let too_many_predicates = GuardConfigV1 {
            schema_version: 1,
            predicates,
            guards: Vec::new(),
        };
        assert_eq!(
            too_many_predicates.validate(),
            Err(GuardConfigError::TooManyPredicates {
                max: MAX_REGISTERED_PREDICATES,
                actual: MAX_REGISTERED_PREDICATES + 1,
            })
        );

        let guard = GuardRuleConfigV1 {
            scope: GuardScopeConfigV1::Resource,
            expression: GuardExprConfigV1::Const { value: true },
        };
        let too_many_guards = GuardConfigV1 {
            schema_version: 1,
            predicates: Vec::new(),
            guards: vec![guard; MAX_GUARD_CONFIG_GUARDS + 1],
        };
        assert_eq!(
            too_many_guards.validate(),
            Err(GuardConfigError::TooManyGuards {
                max: MAX_GUARD_CONFIG_GUARDS,
                actual: MAX_GUARD_CONFIG_GUARDS + 1,
            })
        );
    }

    #[test]
    fn lowering_is_independent_of_predicate_declaration_order() {
        let a = key("elastic.test", "a");
        let b = key("elastic.test", "b");
        let guard = GuardRuleConfigV1 {
            scope: GuardScopeConfigV1::Resource,
            expression: GuardExprConfigV1::All {
                expressions: vec![
                    GuardExprConfigV1::Atom {
                        predicate: b.clone(),
                    },
                    GuardExprConfigV1::Atom {
                        predicate: a.clone(),
                    },
                ],
            },
        };
        let first = GuardConfigV1 {
            schema_version: 1,
            predicates: vec![threshold(a.clone()), threshold(b.clone())],
            guards: vec![guard.clone()],
        }
        .lower()
        .unwrap();
        let second = GuardConfigV1 {
            schema_version: 1,
            predicates: vec![threshold(b), threshold(a)],
            guards: vec![guard],
        }
        .lower()
        .unwrap();

        assert_eq!(first.registry(), second.registry());
        assert_eq!(
            first.guards()[0].fingerprint(),
            second.guards()[0].fingerprint()
        );
    }
}
