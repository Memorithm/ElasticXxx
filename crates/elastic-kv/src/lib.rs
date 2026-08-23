//! KV-cache representation contracts for ElasticXxx.
//!
//! This crate contains metadata and transition validation only. It does not own
//! a cache implementation and deliberately does not depend on FLAT, EPG, CUDA,
//! WGPU, or SciRust. Concrete runtimes can map their page/tile types to these
//! contracts.

#![forbid(unsafe_code)]

use elastic_core::{
    CapabilitySet, RepresentationState, RepresentationTransition, TransitionAttestations,
    TransitionError, TransitionMechanism,
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
    /// Key is transformed only with metadata intrinsic to the token/page; this
    /// transform is not itself query-dependent. Full cross-query reuse still
    /// requires provenance and all other compatibility checks to succeed.
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
    /// Whether reconstructible state is declared to exist outside this
    /// materialization. This metadata bit does not itself authorize recompute;
    /// validation still requires an explicit trusted-boundary attestation.
    pub reconstructible: bool,
}

impl KvPageDescriptor {
    /// Whether the stored key transform is independent of a future query.
    ///
    /// This is a **necessary but not sufficient** condition for cross-query KV
    /// reuse. A caller must still validate derivation provenance, model/schema
    /// compatibility, epochs/generations, positional context, semantic contract,
    /// and any other domain-specific reuse requirements before reusing a page.
    pub const fn key_transform_is_query_independent(&self) -> bool {
        !matches!(self.key_transform_scope, KeyTransformScope::QueryDependent)
    }

    /// Validate a representation change intended to remain generally reusable
    /// across future queries.
    ///
    /// Representation changes are delegated to `elastic-core`; the KV layer
    /// adds one cache-specific necessary condition: a query-dependent
    /// pre-transform cannot be committed as a generally reusable cache state.
    /// Successful validation here does **not** by itself prove complete
    /// cross-query reuse compatibility; provenance and other domain contracts
    /// must be validated separately.
    ///
    /// `attestations` must come from the trusted runtime/adapter boundary. In
    /// particular, `self.reconstructible` is descriptive metadata and is not
    /// automatically promoted into a recompute-source attestation.
    pub fn validate_reusable_representation_change(
        &self,
        target: RepresentationState,
        mechanism: TransitionMechanism,
        capabilities: &CapabilitySet,
        attestations: TransitionAttestations,
        target_key_transform_scope: KeyTransformScope,
    ) -> Result<KvTransitionPlan, KvTransitionError> {
        if matches!(
            target_key_transform_scope,
            KeyTransformScope::QueryDependent
        ) {
            return Err(KvTransitionError::QueryDependentCacheRepresentation);
        }
        let transition = RepresentationTransition {
            from: self.representation.clone(),
            to: target,
            mechanism,
        };
        transition.validate(capabilities, attestations)?;
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
    fn query_independent_transform_is_only_a_local_predicate() {
        let raw = KvPageDescriptor {
            representation: state("raw", 1),
            precision: KvPrecision::F16,
            residency: KvResidency::Accelerator,
            key_transform_scope: KeyTransformScope::Raw,
            reconstructible: true,
        };
        let query_dependent = KvPageDescriptor {
            key_transform_scope: KeyTransformScope::QueryDependent,
            ..raw.clone()
        };

        assert!(raw.key_transform_is_query_independent());
        assert!(!query_dependent.key_transform_is_query_independent());
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
            .validate_reusable_representation_change(
                target,
                TransitionMechanism::Reencode,
                &caps,
                TransitionAttestations::default().attest_reencoder_available(),
                KeyTransformScope::TokenStable,
            )
            .unwrap();
        assert_eq!(
            plan.target_key_transform_scope,
            KeyTransformScope::TokenStable
        );
    }

    #[test]
    fn reconstructible_metadata_does_not_self_authorize_recompute() {
        let page = KvPageDescriptor {
            representation: state("raw", 1),
            precision: KvPrecision::F16,
            residency: KvResidency::Host,
            key_transform_scope: KeyTransformScope::Raw,
            reconstructible: true,
        };
        let target = state("compressed", 2);
        let mut caps = CapabilitySet::new();
        caps.insert(target.id.clone(), target.schema_version);

        assert!(matches!(
            page.validate_reusable_representation_change(
                target,
                TransitionMechanism::Recompute,
                &caps,
                TransitionAttestations::default(),
                KeyTransformScope::Raw,
            ),
            Err(KvTransitionError::Core(
                TransitionError::MissingRecomputeSourceAttestation
            ))
        ));
    }

    #[test]
    fn query_dependent_target_is_rejected_as_reusable_cache_state() {
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
            page.validate_reusable_representation_change(
                target,
                TransitionMechanism::Recompute,
                &caps,
                TransitionAttestations::default(),
                KeyTransformScope::QueryDependent,
            ),
            Err(KvTransitionError::QueryDependentCacheRepresentation)
        );
    }
}
