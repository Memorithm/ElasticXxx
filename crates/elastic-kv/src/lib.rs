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

/// Ordered pipeline used to materialize stored K.
///
/// The order is intentionally part of the cache contract. ElasticXxx must not
/// assume that a numerical codec/quantizer commutes with a geometric or other
/// key transform. A concrete representation may prove such equivalence, but
/// that proof belongs at a trusted adapter boundary rather than in this enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyEncodingPipeline {
    /// No key transform has been materialized; K is stored in its raw domain.
    Raw,
    /// Apply the token-stable/query-dependent key transform, then the numerical
    /// codec (which may be the identity codec).
    TransformThenCodec,
    /// Apply the numerical codec/domain conversion before the key transform.
    CodecThenTransform,
    /// A fused implementation whose semantics are declared by the
    /// representation contract. No commutation with either ordered form is
    /// implied.
    FusedDeclared,
}

/// Descriptive source retained for future rematerialization.
///
/// This is metadata only. It never self-authorizes `Reencode` or `Recompute`;
/// the corresponding trusted-boundary `TransitionAttestations` remain required.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KvRecoverySource {
    /// No independent source is declared.
    None,
    /// A canonical/raw K source is retained independently of this materialized
    /// representation and may support a later epoch re-encode.
    StoredCanonicalRaw,
    /// The model/runtime can regenerate K from an upstream model state.
    ModelRecompute,
    /// A runtime-specific stable external source can regenerate the page.
    ExternalStableSource,
}

/// Conservative cache/rematerialization classification for one materialization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KvCacheCompatibility {
    /// Reusable across future queries only under the current validated
    /// representation contract/epoch and the caller's remaining provenance
    /// checks. No independent rematerialization source is declared.
    CacheInvariant,
    /// A canonical/raw source is retained, so another representation epoch may
    /// be produced by an explicitly validated re-encode.
    EpochReencodable,
    /// A future materialization requires recomputation from a declared upstream
    /// source; metadata alone does not authorize that recomputation.
    RecomputableOnly,
    /// K depends on a future query/context and is not a generally reusable
    /// cache state.
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
    /// Ordered materialization pipeline for K.
    pub key_encoding_pipeline: KeyEncodingPipeline,
    /// Descriptive source available for later rematerialization.
    pub recovery_source: KvRecoverySource,
}

impl KvPageDescriptor {
    /// Validate the local relationship between transform scope and encoding
    /// pipeline. This does not establish cross-query reuse or provenance.
    pub fn validate_descriptor(&self) -> Result<(), KvTransitionError> {
        validate_scope_pipeline(self.key_transform_scope, self.key_encoding_pipeline)
    }

    /// Whether the stored key transform is independent of a future query.
    ///
    /// This is a **necessary but not sufficient** condition for cross-query KV
    /// reuse. A caller must still validate derivation provenance, model/schema
    /// compatibility, epochs/generations, positional context, semantic contract,
    /// and any other domain-specific reuse requirements before reusing a page.
    pub const fn key_transform_is_query_independent(&self) -> bool {
        !matches!(self.key_transform_scope, KeyTransformScope::QueryDependent)
    }

    /// Conservative classification of the current page's reuse/rematerialization
    /// properties.
    ///
    /// A positive rematerialization class describes available metadata only;
    /// transition execution still requires capabilities and attestations.
    pub const fn cache_compatibility(&self) -> KvCacheCompatibility {
        if matches!(self.key_transform_scope, KeyTransformScope::QueryDependent) {
            return KvCacheCompatibility::QueryDependent;
        }
        match self.recovery_source {
            KvRecoverySource::StoredCanonicalRaw => KvCacheCompatibility::EpochReencodable,
            KvRecoverySource::ModelRecompute | KvRecoverySource::ExternalStableSource => {
                KvCacheCompatibility::RecomputableOnly
            }
            KvRecoverySource::None => KvCacheCompatibility::CacheInvariant,
        }
    }

    /// Validate a representation change intended to remain generally reusable
    /// across future queries.
    ///
    /// Representation changes are delegated to `elastic-core`; the KV layer
    /// adds cache-specific conditions:
    ///
    /// - query-dependent K cannot become a generally reusable cache state;
    /// - changing transform scope or encoding order creates a new
    ///   materialization and cannot be represented as a byte reinterpretation;
    /// - transform/codec order is explicit rather than assumed commutative.
    ///
    /// Successful validation here does **not** by itself prove complete
    /// cross-query reuse compatibility; provenance and other domain contracts
    /// must be validated separately.
    pub fn validate_reusable_representation_change(
        &self,
        target: RepresentationState,
        mechanism: TransitionMechanism,
        capabilities: &CapabilitySet,
        attestations: TransitionAttestations,
        target_key_transform_scope: KeyTransformScope,
        target_key_encoding_pipeline: KeyEncodingPipeline,
    ) -> Result<KvTransitionPlan, KvTransitionError> {
        self.validate_descriptor()?;
        validate_scope_pipeline(target_key_transform_scope, target_key_encoding_pipeline)?;

        if matches!(target_key_transform_scope, KeyTransformScope::QueryDependent) {
            return Err(KvTransitionError::QueryDependentCacheRepresentation);
        }

        let key_materialization_changes = self.key_transform_scope != target_key_transform_scope
            || self.key_encoding_pipeline != target_key_encoding_pipeline;
        if key_materialization_changes && matches!(mechanism, TransitionMechanism::Reinterpret) {
            return Err(KvTransitionError::KeyMaterializationChangeRequiresMaterialization);
        }

        let transition = RepresentationTransition {
            from: self.representation.clone(),
            to: target,
            mechanism,
        };
        transition.validate(capabilities, attestations)?;
        let compatibility = match mechanism {
            TransitionMechanism::Reinterpret => KvCacheCompatibility::CacheInvariant,
            TransitionMechanism::Reencode => KvCacheCompatibility::EpochReencodable,
            TransitionMechanism::Recompute => KvCacheCompatibility::RecomputableOnly,
        };
        Ok(KvTransitionPlan {
            representation: transition,
            target_key_transform_scope,
            target_key_encoding_pipeline,
            compatibility,
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
    /// Encoding order after the transition.
    pub target_key_encoding_pipeline: KeyEncodingPipeline,
    /// Conservative rematerialization class implied by the mechanism.
    pub compatibility: KvCacheCompatibility,
}

/// Policy controlling whether one logical attention view may contain pages from
/// different representation contracts/epochs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KvAttentionViewPolicy {
    allow_per_page_representation: bool,
}

impl KvAttentionViewPolicy {
    /// Require one representation contract, epoch, transform scope and encoding
    /// pipeline across the full logical view.
    pub const fn homogeneous() -> Self {
        Self {
            allow_per_page_representation: false,
        }
    }

    /// Permit mixed representation contracts/epochs because the consuming
    /// kernel/runtime explicitly declares per-page representation dispatch.
    /// Query-dependent K remains disallowed for a reusable view.
    pub const fn per_page_representation() -> Self {
        Self {
            allow_per_page_representation: true,
        }
    }

    /// Whether the consumer explicitly supports per-page representation state.
    pub const fn allows_per_page_representation(self) -> bool {
        self.allow_per_page_representation
    }
}

/// Result of validating a reusable logical attention view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KvAttentionViewSummary {
    /// Number of validated pages.
    pub page_count: usize,
    /// Common representation when the view is homogeneous. `None` means the
    /// caller explicitly enabled per-page representation dispatch and at least
    /// two representation states differ.
    pub homogeneous_representation: Option<RepresentationState>,
}

/// Validate that pages can participate in one reusable logical attention view.
///
/// The default/homogeneous policy rejects mixed contracts or materialization
/// epochs. This makes geometry/representation switches atomic at the logical
/// view boundary unless a concrete kernel explicitly supports per-page
/// descriptors. Residency and precision are intentionally not required to match
/// here because a future execution layer may support heterogeneous placement or
/// codecs independently of the representation-state rule.
pub fn validate_reusable_attention_view(
    pages: &[KvPageDescriptor],
    policy: KvAttentionViewPolicy,
) -> Result<KvAttentionViewSummary, KvTransitionError> {
    let Some(first) = pages.first() else {
        return Err(KvTransitionError::EmptyAttentionView);
    };
    first.validate_descriptor()?;
    if !first.key_transform_is_query_independent() {
        return Err(KvTransitionError::QueryDependentCacheRepresentation);
    }

    let mut homogeneous = true;
    for page in &pages[1..] {
        page.validate_descriptor()?;
        if !page.key_transform_is_query_independent() {
            return Err(KvTransitionError::QueryDependentCacheRepresentation);
        }

        if page.representation != first.representation
            || page.key_transform_scope != first.key_transform_scope
            || page.key_encoding_pipeline != first.key_encoding_pipeline
        {
            homogeneous = false;
        }

        if !policy.allows_per_page_representation() {
            if !page.representation.same_contract(&first.representation) {
                return Err(KvTransitionError::MixedRepresentationContract);
            }
            if page.representation.epoch != first.representation.epoch {
                return Err(KvTransitionError::MixedRepresentationEpoch);
            }
            if page.key_transform_scope != first.key_transform_scope {
                return Err(KvTransitionError::MixedKeyTransformScope);
            }
            if page.key_encoding_pipeline != first.key_encoding_pipeline {
                return Err(KvTransitionError::MixedKeyEncodingPipeline);
            }
        }
    }

    Ok(KvAttentionViewSummary {
        page_count: pages.len(),
        homogeneous_representation: homogeneous.then(|| first.representation.clone()),
    })
}

/// Summary for a set of page transitions that may be committed atomically.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KvAtomicTransitionSummary {
    /// Number of page transitions in the batch.
    pub page_count: usize,
    /// Common source representation state.
    pub from: RepresentationState,
    /// Common target representation state.
    pub to: RepresentationState,
}

/// Validate the representation-state boundary for an atomic logical-view
/// transition.
///
/// Every page must start from exactly the same representation state and target
/// exactly the same representation state. A runtime may then execute pages in
/// parallel, but must expose the new logical view only after all page
/// transitions commit successfully (or use a stronger transaction protocol).
pub fn validate_atomic_transition_batch(
    plans: &[KvTransitionPlan],
) -> Result<KvAtomicTransitionSummary, KvTransitionError> {
    let Some(first) = plans.first() else {
        return Err(KvTransitionError::EmptyTransitionBatch);
    };
    for plan in &plans[1..] {
        if plan.representation.from != first.representation.from {
            return Err(KvTransitionError::MixedTransitionSource);
        }
        if plan.representation.to != first.representation.to {
            return Err(KvTransitionError::MixedTransitionTarget);
        }
    }
    Ok(KvAtomicTransitionSummary {
        page_count: plans.len(),
        from: first.representation.from.clone(),
        to: first.representation.to.clone(),
    })
}

fn validate_scope_pipeline(
    scope: KeyTransformScope,
    pipeline: KeyEncodingPipeline,
) -> Result<(), KvTransitionError> {
    match (scope, pipeline) {
        (KeyTransformScope::Raw, KeyEncodingPipeline::Raw) => Ok(()),
        (KeyTransformScope::Raw, _) => Err(KvTransitionError::RawKeyHasTransformPipeline),
        (_, KeyEncodingPipeline::Raw) => Err(KvTransitionError::TransformedKeyMissingPipeline),
        _ => Ok(()),
    }
}

/// KV-specific transition/view validation failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KvTransitionError {
    /// Core ElasticXxx transition contract failed.
    Core(TransitionError),
    /// Query-dependent transformed keys cannot be committed as a generally
    /// reusable KV-cache representation.
    QueryDependentCacheRepresentation,
    /// A raw K descriptor declared a transform/codec pipeline.
    RawKeyHasTransformPipeline,
    /// A transformed K descriptor failed to declare its encoding pipeline.
    TransformedKeyMissingPipeline,
    /// Reinterpretation attempted to change key transform scope or encoding
    /// order, which requires a new materialization.
    KeyMaterializationChangeRequiresMaterialization,
    /// A reusable attention view contains no pages.
    EmptyAttentionView,
    /// Homogeneous view contains more than one representation contract.
    MixedRepresentationContract,
    /// Homogeneous view contains more than one materialization epoch.
    MixedRepresentationEpoch,
    /// Homogeneous view contains more than one key-transform scope.
    MixedKeyTransformScope,
    /// Homogeneous view contains more than one key-encoding pipeline.
    MixedKeyEncodingPipeline,
    /// Atomic transition batch contains no page transitions.
    EmptyTransitionBatch,
    /// Atomic transition batch does not share one source state.
    MixedTransitionSource,
    /// Atomic transition batch does not share one target state.
    MixedTransitionTarget,
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
            Self::RawKeyHasTransformPipeline => {
                write!(f, "raw K must use the raw key-encoding pipeline")
            }
            Self::TransformedKeyMissingPipeline => write!(
                f,
                "transformed K must declare a non-raw key-encoding pipeline"
            ),
            Self::KeyMaterializationChangeRequiresMaterialization => write!(
                f,
                "changing key transform scope or encoding order requires re-encoding or recomputation"
            ),
            Self::EmptyAttentionView => write!(f, "reusable attention view must contain a page"),
            Self::MixedRepresentationContract => write!(
                f,
                "homogeneous attention view contains mixed representation contracts"
            ),
            Self::MixedRepresentationEpoch => write!(
                f,
                "homogeneous attention view contains mixed representation epochs"
            ),
            Self::MixedKeyTransformScope => {
                write!(f, "homogeneous attention view contains mixed key-transform scopes")
            }
            Self::MixedKeyEncodingPipeline => write!(
                f,
                "homogeneous attention view contains mixed key-encoding pipelines"
            ),
            Self::EmptyTransitionBatch => write!(f, "atomic transition batch must not be empty"),
            Self::MixedTransitionSource => write!(
                f,
                "atomic transition batch contains mixed source representation states"
            ),
            Self::MixedTransitionTarget => write!(
                f,
                "atomic transition batch contains mixed target representation states"
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

    fn raw_page(name: &str, epoch: u64) -> KvPageDescriptor {
        KvPageDescriptor {
            representation: state(name, epoch),
            precision: KvPrecision::F16,
            residency: KvResidency::Accelerator,
            key_transform_scope: KeyTransformScope::Raw,
            key_encoding_pipeline: KeyEncodingPipeline::Raw,
            recovery_source: KvRecoverySource::None,
        }
    }

    #[test]
    fn query_independent_transform_is_only_a_local_predicate() {
        let raw = raw_page("raw", 1);
        let query_dependent = KvPageDescriptor {
            key_transform_scope: KeyTransformScope::QueryDependent,
            key_encoding_pipeline: KeyEncodingPipeline::TransformThenCodec,
            ..raw.clone()
        };

        assert!(raw.key_transform_is_query_independent());
        assert!(!query_dependent.key_transform_is_query_independent());
    }

    #[test]
    fn cache_compatibility_distinguishes_future_rematerialization_paths() {
        let invariant = raw_page("raw", 1);
        let canonical = KvPageDescriptor {
            recovery_source: KvRecoverySource::StoredCanonicalRaw,
            ..invariant.clone()
        };
        let recomputable = KvPageDescriptor {
            recovery_source: KvRecoverySource::ModelRecompute,
            ..invariant.clone()
        };
        let query_dependent = KvPageDescriptor {
            key_transform_scope: KeyTransformScope::QueryDependent,
            key_encoding_pipeline: KeyEncodingPipeline::FusedDeclared,
            ..invariant.clone()
        };

        assert_eq!(invariant.cache_compatibility(), KvCacheCompatibility::CacheInvariant);
        assert_eq!(
            canonical.cache_compatibility(),
            KvCacheCompatibility::EpochReencodable
        );
        assert_eq!(
            recomputable.cache_compatibility(),
            KvCacheCompatibility::RecomputableOnly
        );
        assert_eq!(
            query_dependent.cache_compatibility(),
            KvCacheCompatibility::QueryDependent
        );
    }

    #[test]
    fn descriptor_rejects_implicit_transform_codec_order() {
        let bad_raw = KvPageDescriptor {
            key_encoding_pipeline: KeyEncodingPipeline::TransformThenCodec,
            ..raw_page("raw", 1)
        };
        assert_eq!(
            bad_raw.validate_descriptor(),
            Err(KvTransitionError::RawKeyHasTransformPipeline)
        );

        let bad_transformed = KvPageDescriptor {
            key_transform_scope: KeyTransformScope::TokenStable,
            ..raw_page("token", 1)
        };
        assert_eq!(
            bad_transformed.validate_descriptor(),
            Err(KvTransitionError::TransformedKeyMissingPipeline)
        );
    }

    #[test]
    fn token_stable_geometry_change_can_be_reencoded() {
        let page = KvPageDescriptor {
            representation: state("epg.so2", 4),
            precision: KvPrecision::Int4,
            residency: KvResidency::Host,
            key_transform_scope: KeyTransformScope::TokenStable,
            key_encoding_pipeline: KeyEncodingPipeline::TransformThenCodec,
            recovery_source: KvRecoverySource::StoredCanonicalRaw,
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
                KeyEncodingPipeline::TransformThenCodec,
            )
            .unwrap();
        assert_eq!(plan.target_key_transform_scope, KeyTransformScope::TokenStable);
        assert_eq!(plan.compatibility, KvCacheCompatibility::EpochReencodable);
    }

    #[test]
    fn recovery_source_metadata_does_not_self_authorize_recompute() {
        let page = KvPageDescriptor {
            recovery_source: KvRecoverySource::ModelRecompute,
            ..raw_page("raw", 1)
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
                KeyEncodingPipeline::Raw,
            ),
            Err(KvTransitionError::Core(
                TransitionError::MissingRecomputeSourceAttestation
            ))
        ));
    }

    #[test]
    fn query_dependent_target_is_rejected_as_reusable_cache_state() {
        let page = raw_page("epg.so2", 1);
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
                KeyEncodingPipeline::FusedDeclared,
            ),
            Err(KvTransitionError::QueryDependentCacheRepresentation)
        );
    }

    #[test]
    fn changing_transform_order_cannot_be_reinterpreted() {
        let page = KvPageDescriptor {
            key_transform_scope: KeyTransformScope::TokenStable,
            key_encoding_pipeline: KeyEncodingPipeline::TransformThenCodec,
            ..raw_page("token", 3)
        };
        let target = state("token", 3);
        let mut caps = CapabilitySet::new();
        caps.insert(target.id.clone(), target.schema_version);

        assert_eq!(
            page.validate_reusable_representation_change(
                target,
                TransitionMechanism::Reinterpret,
                &caps,
                TransitionAttestations::default(),
                KeyTransformScope::TokenStable,
                KeyEncodingPipeline::CodecThenTransform,
            ),
            Err(KvTransitionError::KeyMaterializationChangeRequiresMaterialization)
        );
    }

    #[test]
    fn homogeneous_attention_view_rejects_mixed_epochs() {
        let a = raw_page("epg.so2", 7);
        let b = raw_page("epg.so2", 8);
        assert_eq!(
            validate_reusable_attention_view(&[a, b], KvAttentionViewPolicy::homogeneous()),
            Err(KvTransitionError::MixedRepresentationEpoch)
        );
    }

    #[test]
    fn explicit_per_page_policy_allows_mixed_representation_epochs() {
        let a = raw_page("epg.so2", 7);
        let b = raw_page("epg.so4", 8);
        let summary = validate_reusable_attention_view(
            &[a, b],
            KvAttentionViewPolicy::per_page_representation(),
        )
        .unwrap();
        assert_eq!(summary.page_count, 2);
        assert_eq!(summary.homogeneous_representation, None);
    }

    #[test]
    fn atomic_transition_batch_rejects_mixed_target_epochs() {
        let page = raw_page("raw", 1);
        let target_a = state("epg.so2", 2);
        let target_b = state("epg.so2", 3);
        let mut caps = CapabilitySet::new();
        caps.insert(target_a.id.clone(), target_a.schema_version);
        let attestations = TransitionAttestations::default().attest_reencoder_available();
        let plan_a = page
            .validate_reusable_representation_change(
                target_a,
                TransitionMechanism::Reencode,
                &caps,
                attestations,
                KeyTransformScope::TokenStable,
                KeyEncodingPipeline::TransformThenCodec,
            )
            .unwrap();
        let plan_b = page
            .validate_reusable_representation_change(
                target_b,
                TransitionMechanism::Reencode,
                &caps,
                attestations,
                KeyTransformScope::TokenStable,
                KeyEncodingPipeline::TransformThenCodec,
            )
            .unwrap();

        assert_eq!(
            validate_atomic_transition_batch(&[plan_a, plan_b]),
            Err(KvTransitionError::MixedTransitionTarget)
        );
    }
}
