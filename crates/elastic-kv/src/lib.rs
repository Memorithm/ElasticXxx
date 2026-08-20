//! KV-cache representation contracts for ElasticXxx.
//!
//! This crate contains metadata and transition validation only. It does not own
//! a cache implementation and deliberately does not depend on FLAT, EPG, CUDA,
//! WGPU, or SciRust. Concrete runtimes can map their page/tile types to these
//! contracts.

#![forbid(unsafe_code)]

use elastic_core::{
    CapabilitySet, RepresentationState, RepresentationTransition, TransitionError,
    TransitionFacts, TransitionMechanism,
};
use std::fmt;

/// Numerical precision of a materialized KV page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KvPrecision {
    /// IEEE binary32.
    F32,
    /// Brain floating point 16.
    Bf16,
    /// IEEE binary16.
    F16,
    /// Signed 8-bit quantized representation.
    Int8,
    /// Signed 4-bit quantized representation.
    Int4,
    /// Runtime-defined precision/codec identifier.
    Custom(String),
}

/// Physical residence class of a KV page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KvResidency {
    /// Accelerator-local memory such as GPU VRAM.
    Accelerator,
    /// Host RAM.
    Host,
    /// Persistent or remote backing storage.
    Backing,
    /// Runtime-defined residence class.
    Custom(String),
}

/// How the stored key relates to positional/structural transformation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyTransformScope {
    /// Raw key; positional transformation is deferred to attention execution.
    Raw,
    /// Key is transformed using metadata intrinsic to the token/page and can be
    /// reused by future queries without knowing those queries.
    TokenStable,
    /// Stored key depends on a query or other future context and therefore is
    /// not a generally reusable KV-cache materialization.
    QueryDependent,
}

/// Versioned metadata for one materialized KV page/tile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KvPageDescriptor {
    /// Mathematical/numerical representation contract and epoch.
    pub representation: RepresentationState,
    /// Stored precision.
    pub precision: KvPrecision,
    /// Current residence class.
    pub residency: KvResidency,
    /// Positional/structural transform scope of K.
    pub key_transform_scope: KeyTransformScope,
    /// Whether trusted raw/reconstructible state exists outside this materialization.
    pub reconstructible: bool,
}

impl KvPageDescriptor {
    /// Whether this page can safely be reused for arbitrary future queries.
    pub const fn reusable_for_future_queries(&self) -> bool {
        !matches!(self.key_transform_scope, KeyTransformScope::QueryDependent)
    }

    /// Validate a change of representation for this page.
    ///
    /// Representation changes are delegated to `elastic-core`; the KV layer
    /// adds the cache-specific rule that a query-dependent pre-transform is not
    /// a generally reusable cache state.
    pub fn validate_representation_change(
        &self,
        target: RepresentationState,
        mechanism: TransitionMechanism,
        capabilities: &CapabilitySet,
        mut facts: TransitionFacts,
        target_key_transform_scope: KeyTransformScope,
    ) -> Result<KvTransitionPlan, KvTransitionError> {
        if matches!(target_key_transform_scope, KeyTransformScope::QueryDependent) {
            return Err(KvTransitionError::QueryDependentCacheRepresentation);
        }
        if self.reconstructible {
            facts.recompute_source_available = true;
        }
        let transition = RepresentationTransition {
            from: self.representation.clone(),
            to: target,
            mechanism,
        };
        transition.validate(capabilities, facts)?;
        Ok(KvTransitionPlan {
            representation: transition,
            target_key_transform_scope,
        })
    }
}

/// Validated cache-level representation transition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KvTransitionPlan {
    /// Core representation transition.
    pub representation: RepresentationTransition,
    /// Key-transform scope after the transition.
    pub target_key_transform_scope: KeyTransformScope,
}

/// KV-specific transition validation failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KvTransitionError {
    /// Core ElasticXxx transition contract failed.
    Core(TransitionError),
    /// Query-dependent transformed keys cannot be committed as a generally
    /// reusable KV-cache representation.
    QueryDependentCacheRepresentation,
}

impl From<TransitionError> for KvTransitionError {
    fn from(value: TransitionError) -> Self {
        Self::Core(value)
    }
}

impl fmt::Display for KvTransitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Core(error) => write!(f, "{error}"),
            Self::QueryDependentCacheRepresentation => write!(
                f,
                "query-dependent transformed keys are not a reusable KV-cache representation"
            ),
        }
    }
}

impl std::error::Error for KvTransitionError {}

#[cfg(test)]
mod tests {
    use super::*;
    use elastic_core::{RepresentationEpoch, RepresentationId};

    fn state(name: &str, epoch: u64) -> RepresentationState {
        RepresentationState::new(
            RepresentationId::new(name).unwrap(),
            1,
            RepresentationEpoch::new(epoch),
        )
    }

    #[test]
    fn token_stable_geometry_change_can_be_reencoded() {
        let page = KvPageDescriptor {
            representation: state("epg.so2", 4),
            precision: KvPrecision::Int4,
            residency: KvResidency::Host,
            key_transform_scope: KeyTransformScope::TokenStable,
            reconstructible: true,
        };
        let target = state("epg.so4.structural", 5);
        let mut caps = CapabilitySet::new();
        caps.insert(target.id.clone(), target.schema_version);
        let plan = page
            .validate_representation_change(
                target,
                TransitionMechanism::Reencode,
                &caps,
                TransitionFacts {
                    reencoder_available: true,
                    ..TransitionFacts::default()
                },
                KeyTransformScope::TokenStable,
            )
            .unwrap();
        assert_eq!(plan.target_key_transform_scope, KeyTransformScope::TokenStable);
    }

    #[test]
    fn query_dependent_target_is_rejected_as_cache_state() {
        let page = KvPageDescriptor {
            representation: state("epg.so2", 1),
            precision: KvPrecision::F16,
            residency: KvResidency::Accelerator,
            key_transform_scope: KeyTransformScope::Raw,
            reconstructible: true,
        };
        let target = state("epg.dynamic", 2);
        let mut caps = CapabilitySet::new();
        caps.insert(target.id.clone(), target.schema_version);
        assert_eq!(
            page.validate_representation_change(
                target,
                TransitionMechanism::Recompute,
                &caps,
                TransitionFacts::default(),
                KeyTransformScope::QueryDependent,
            ),
            Err(KvTransitionError::QueryDependentCacheRepresentation)
        );
    }
}
