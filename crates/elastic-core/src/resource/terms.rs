//! Extensible typed identifiers for the general resource model.
//!
//! Built-in semantics are expressed as enum variants so that core code can
//! match on them without string comparisons. Downstream crates may define
//! additional identifiers through validated `custom` constructors, which keeps
//! the set open without a closed-world "every future hardware" enum and without
//! making raw strings carry core semantics.
//!
//! Ordering is total and deterministic: built-in terms order by declaration,
//! custom terms order after every built-in term by their text. Collections of
//! terms therefore normalize identically in every process.

use super::error::{ResourceSpecError, TermKind};
use std::fmt;

/// Shared representation of an extensible term: one built-in variant or one
/// validated custom text.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Term<B> {
    /// A built-in semantic known to this crate.
    Builtin(B),
    /// A downstream-defined extension with a non-empty canonical text.
    Custom(String),
}

/// Generate one extensible identifier type plus its built-in enum.
///
/// The generated type exposes:
/// - associated constants named after each built-in variant;
/// - [`Term::Builtin`] construction via `<Type>::builtin`;
/// - open-set extension via `<Type>::custom`, which rejects blank texts;
/// - total ordering (built-ins by declaration, customs after, lexicographic);
/// - `Display`/`as_str` over the canonical text.
macro_rules! extensible_term {
    (
        $(#[$type_meta:meta])*
        $name:ident, $builtin_name:ident {
            $( $(#[$variant_meta:meta])* $variant:ident as $constant:ident = $canonical:expr ),* $(,)?
        }
        $term_kind:expr
    ) => {
        /// Built-in semantics known to the Elastic core.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum $builtin_name {
            $(
                $(#[$variant_meta])*
                $variant,
            )*
        }

        impl $builtin_name {
            /// Canonical text of this built-in term.
            #[must_use]
            pub const fn canonical(self) -> &'static str {
                match self {
                    $(Self::$variant => $canonical,)*
                }
            }
        }

        impl fmt::Display for $builtin_name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.canonical())
            }
        }

        $(#[$type_meta])*
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(Term<$builtin_name>);

        impl $name {
            $(
                #[doc = concat!("The built-in `", stringify!($variant), "` term (canonical text: `", $canonical, "`).")]
                pub const $constant: Self = Self(Term::Builtin($builtin_name::$variant));
            )*

            /// Wrap a built-in semantic.
            #[must_use]
            pub const fn builtin(builtin: $builtin_name) -> Self {
                Self(Term::Builtin(builtin))
            }

            /// Define an open-set extension term.
            ///
            /// The text must not be empty or blank. Custom terms never shadow
            /// built-in semantics; they always order after every built-in
            /// term of the same type.
            ///
            /// # Errors
            ///
            /// Returns [`ResourceSpecError::InvalidCustomTerm`] when the text
            /// is empty or only whitespace.
            pub fn custom(text: impl Into<String>) -> Result<Self, ResourceSpecError> {
                let text = text.into();
                if text.trim().is_empty() {
                    return Err(ResourceSpecError::InvalidCustomTerm {
                        term_kind: $term_kind,
                    });
                }
                Ok(Self(Term::Custom(text)))
            }

            /// The built-in semantic, if this term is not a custom extension.
            #[must_use]
            pub const fn builtin_part(&self) -> Option<$builtin_name> {
                match &self.0 {
                    Term::Builtin(builtin) => Some(*builtin),
                    Term::Custom(_) => None,
                }
            }

            /// The extension text, if this term is not built-in.
            #[must_use]
            pub fn custom_text(&self) -> Option<&str> {
                match &self.0 {
                    Term::Builtin(_) => None,
                    Term::Custom(text) => Some(text.as_str()),
                }
            }

            /// Canonical text of this term.
            #[must_use]
            pub fn as_str(&self) -> &str {
                match &self.0 {
                    Term::Builtin(builtin) => builtin.canonical(),
                    Term::Custom(text) => text.as_str(),
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

extensible_term! {
    /// Identifier of an elastic dimension: one axis along which a resource may
    /// legally change.
    DimensionId, BuiltinDimension {
        /// Amount of backing storage or units held.
        Capacity as CAPACITY = "capacity",
        /// Degree of simultaneous access or execution admitted.
        Concurrency as CONCURRENCY = "concurrency",
        /// Physical or hierarchical placement class of the materialization.
        Residency as RESIDENCY = "residency",
        /// Nearness to consumers or peers.
        Locality as LOCALITY = "locality",
        /// Mathematical/numerical representation contract.
        Representation as REPRESENTATION = "representation",
        /// Numerical precision or fidelity class.
        Precision as PRECISION = "precision",
        /// Degree of internal decomposition into parallel work.
        Parallelism as PARALLELISM = "parallelism",
        /// Distribution of work or requests over routes.
        Routing as ROUTING = "routing",
        /// Level of duplication retained for availability or reuse.
        Redundancy as REDUNDANCY = "redundancy",
        /// Durability of the realization across failures or restarts.
        Persistence as PERSISTENCE = "persistence",
        /// Whether and how the realization can be regenerated from a source.
        Recomputability as RECOMPUTABILITY = "recomputability",
        /// Share of transfer capacity assigned.
        Bandwidth as BANDWIDTH = "bandwidth",
        /// Energy or power envelope consumed.
        Energy as ENERGY = "energy",
    }
    TermKind::Dimension
}

extensible_term! {
    /// Identifier of an optimization objective: something the runtime may try
    /// to improve. Objectives are deliberately distinct from invariants; they
    /// never authorize violating a preserved property.
    ObjectiveId, BuiltinObjective {
        /// End-to-end responsiveness of operations on the resource.
        Latency as LATENCY = "latency",
        /// Sustained operation rate.
        Throughput as THROUGHPUT = "throughput",
        /// Resident size of the materialization.
        MemoryFootprint as MEMORY_FOOTPRINT = "memory-footprint",
        /// Energy or power consumption.
        Energy as ENERGY = "energy",
        /// Cost paid when moving between materializations.
        MigrationCost as MIGRATION_COST = "migration-cost",
        /// Resistance of performance to perturbation and thrash.
        Stability as STABILITY = "stability",
    }
    TermKind::Objective
}

extensible_term! {
    /// Semantic class of a resource, mirroring the provisional whitepaper
    /// taxonomy (stock, capacity, rate, exclusive, shared, state,
    /// representation, configuration).
    ResourceClassId, BuiltinResourceClass {
        /// A counted pool of indistinguishable units.
        Stock as STOCK = "stock",
        /// A bounded amount of usable headroom.
        CapacityResource as CAPACITY_RESOURCE = "capacity-resource",
        /// A sustainable rate of service.
        Rate as RATE = "rate",
        /// Held by at most one consumer at a time.
        Exclusive as EXCLUSIVE = "exclusive",
        /// Safely shareable under declared discipline.
        Shared as SHARED = "shared",
        /// Carries state that survives across operations.
        Stateful as STATEFUL = "stateful",
        /// Data whose mathematical representation may change.
        Representational as REPRESENTATIONAL = "representational",
        /// Named settings that may be reconfigured.
        Configurational as CONFIGURATIONAL = "configurational",
    }
    TermKind::ResourceClass
}

extensible_term! {
    /// Runtime observation that may inform adaptation decisions for the
    /// declaring resource.
    ObservationSignalId, BuiltinObservationSignal {
        /// Remaining free capacity of the hosting context.
        FreeCapacity as FREE_CAPACITY = "free-capacity",
        /// Fraction of the resource currently in use.
        Utilization as UTILIZATION = "utilization",
        /// Depth of pending work queues.
        QueueDepth as QUEUE_DEPTH = "queue-depth",
        /// Sampled latency measurements.
        LatencySample as LATENCY_SAMPLE = "latency-sample",
        /// Distance to thermal limits.
        ThermalMargin as THERMAL_MARGIN = "thermal-margin",
        /// Instantaneous energy or power draw.
        EnergyRate as ENERGY_RATE = "energy-rate",
        /// Availability or topology changes of hosting elements.
        TopologyChange as TOPOLOGY_CHANGE = "topology-change",
    }
    TermKind::ObservationSignal
}

/// Stable logical identity of a resource.
///
/// Logical identity is independent of any physical realization: the same
/// logical resource may change residency, representation, or other elastic
/// dimensions while remaining this resource. The identifier is user-chosen and
/// must be non-empty.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LogicalResourceId(String);

impl LogicalResourceId {
    /// Construct a non-empty logical resource identifier.
    ///
    /// # Errors
    ///
    /// Returns [`ResourceSpecError::EmptyResourceId`] when the text is empty
    /// or only whitespace.
    pub fn new(value: impl Into<String>) -> Result<Self, ResourceSpecError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(ResourceSpecError::EmptyResourceId);
        }
        Ok(Self(value))
    }

    /// Borrow the identifier text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for LogicalResourceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Identifier of an externally defined semantic contract that an invariant may
/// uphold (for example a KV-cache reuse contract).
///
/// The Elastic core cannot interpret the contract's contents; it records that
/// the declaration promises the contract holds. Contract interpretation belongs
/// to the resource adapter that maps declarations onto executable checks.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContractId(String);

impl ContractId {
    /// Construct a non-empty contract identifier.
    ///
    /// # Errors
    ///
    /// Returns [`ResourceSpecError::InvalidCustomTerm`] with
    /// [`TermKind::Contract`] when the text is empty or only whitespace.
    pub fn new(value: impl Into<String>) -> Result<Self, ResourceSpecError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(ResourceSpecError::InvalidCustomTerm {
                term_kind: TermKind::Contract,
            });
        }
        Ok(Self(value))
    }

    /// Borrow the contract identifier text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ContractId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
