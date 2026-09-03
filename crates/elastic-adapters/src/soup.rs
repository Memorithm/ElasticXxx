//! Versioned SOUP run-resource planning boundary.
//!
//! This module models the resource knobs that ElasticXxx may plan for a SOUP
//! training run without importing SOUP's training semantics. It is deliberately
//! pre-execution only: v1 validates a proposed configuration envelope and maps
//! its adaptive axes into the generic Elastic resource model. It does not edit
//! `soup.yaml`, launch SOUP, or claim that batch size / layer streaming can be
//! changed safely in the middle of a training step.
//!
//! The v1 contract is qualified against SOUP v0.73.3 commit
//! `05b646523727925990530667e7012ede50bd30b2`. Unknown upstream revisions fail
//! closed until separately reviewed.

use elastic_core::resource::{
    AdmissibleTransition, CapabilityRequirement, ContractId, DimensionId, Invariant,
    InvariantKind, LogicalResourceId, ObservationSignalId, ResourceClassId, ResourceSpec,
    ResourceSpecError,
};
use elastic_core::TransitionMechanism;
use std::fmt;

/// ElasticXxx's first versioned SOUP resource-planning contract.
pub const SOUP_RESOURCE_PLAN_V1: &str = "elastic.soup.run-resource-plan@1.0.0";

/// SOUP revision qualified for [`SoupRunResourcePlanV1`].
pub const SOUP_QUALIFIED_UPSTREAM_COMMIT: &str =
    "05b646523727925990530667e7012ede50bd30b2";

/// Hub resource-declaration contract carried by the published SOUP components.
///
/// This constant records interoperability identity only. ElasticXxx does not
/// implement Hub placement semantics.
pub const SOUP_HUB_RESOURCE_CONTRACT_V1: &str = "hub.ml.resource-requirements@1.0.0";

/// Minimum number of SOUP layer-streaming VRAM buffers at the qualified revision.
pub const SOUP_MIN_STREAM_BUFFERS: u8 = 2;
/// Maximum number of SOUP layer-streaming VRAM buffers at the qualified revision.
pub const SOUP_MAX_STREAM_BUFFERS: u8 = 8;
/// Default number of SOUP layer-streaming VRAM buffers at the qualified revision.
pub const SOUP_DEFAULT_STREAM_BUFFERS: u8 = 2;

/// Tasks explicitly admitted by SOUP's layer-streaming planner at the qualified revision.
pub const SOUP_STREAM_TASKS: &[&str] = &["sft", "dpo", "orpo", "simpo", "kto"];

/// Batch-size choice accepted by SOUP training.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoupBatchSize {
    /// Delegate the concrete fit decision to SOUP's own batch-size preflight.
    Auto,
    /// Request one fixed positive per-device batch size.
    Fixed(u32),
}

/// SOUP strategy for resolving [`SoupBatchSize::Auto`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoupAutoBatchStrategy {
    /// SOUP chooses probe on CUDA and static estimation on CPU.
    Auto,
    /// Use SOUP's static memory-fit formula.
    Static,
    /// Use SOUP's real OOM try/halve probe.
    Probe,
}

/// Storage tier requested for the frozen streamed base.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoupStreamSource {
    /// Let SOUP resolve RAM vs disk from its own preflight.
    Auto,
    /// Keep the frozen base in host RAM.
    Ram,
    /// Use SOUP's disk overflow tier when its own media checks admit it.
    Disk,
}

/// Layer-streaming part of a SOUP resource plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SoupLayerStreamingV1 {
    source: SoupStreamSource,
    buffers: u8,
}

impl SoupLayerStreamingV1 {
    /// Construct a validated layer-streaming choice.
    ///
    /// # Errors
    ///
    /// Rejects buffer counts outside the SOUP v0.73.3 contract `[2, 8]`.
    pub fn new(source: SoupStreamSource, buffers: u8) -> Result<Self, SoupContractError> {
        if !(SOUP_MIN_STREAM_BUFFERS..=SOUP_MAX_STREAM_BUFFERS).contains(&buffers) {
            return Err(SoupContractError::InvalidStreamBuffers { buffers });
        }
        Ok(Self { source, buffers })
    }

    /// Requested source tier.
    #[must_use]
    pub const fn source(&self) -> SoupStreamSource {
        self.source
    }

    /// Number of pre-allocated layer buffers.
    #[must_use]
    pub const fn buffers(&self) -> u8 {
        self.buffers
    }
}

impl Default for SoupLayerStreamingV1 {
    fn default() -> Self {
        Self {
            source: SoupStreamSource::Auto,
            buffers: SOUP_DEFAULT_STREAM_BUFFERS,
        }
    }
}

/// Validated pre-execution resource plan for one SOUP training run.
///
/// The object carries only knobs whose resource meaning is established by the
/// qualified SOUP revision. Quantization, dtype, model semantics, reward
/// semantics and optimizer choices are intentionally outside this contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoupRunResourcePlanV1 {
    task: String,
    batch_size: SoupBatchSize,
    auto_batch_strategy: SoupAutoBatchStrategy,
    streaming: Option<SoupLayerStreamingV1>,
}

impl SoupRunResourcePlanV1 {
    /// Validate an external SOUP resource-plan envelope against the qualified
    /// upstream identity.
    ///
    /// # Errors
    ///
    /// Fails closed on an unknown upstream revision, blank task, fixed batch
    /// size zero, invalid stream buffers, or a streaming task outside SOUP's
    /// explicit allowlist.
    pub fn from_external(
        upstream_commit: &str,
        task: impl Into<String>,
        batch_size: SoupBatchSize,
        auto_batch_strategy: SoupAutoBatchStrategy,
        streaming: Option<SoupLayerStreamingV1>,
    ) -> Result<Self, SoupContractError> {
        if upstream_commit != SOUP_QUALIFIED_UPSTREAM_COMMIT {
            return Err(SoupContractError::UnsupportedUpstreamRevision {
                revision: upstream_commit.to_owned(),
            });
        }

        let task = task.into();
        let task = task.trim();
        if task.is_empty() {
            return Err(SoupContractError::BlankTask);
        }
        if matches!(batch_size, SoupBatchSize::Fixed(0)) {
            return Err(SoupContractError::InvalidBatchSize { batch_size: 0 });
        }
        if streaming.is_some() && !SOUP_STREAM_TASKS.contains(&task) {
            return Err(SoupContractError::StreamingUnsupportedForTask {
                task: task.to_owned(),
            });
        }

        Ok(Self {
            task: task.to_owned(),
            batch_size,
            auto_batch_strategy,
            streaming,
        })
    }

    /// Build a plan pinned to the qualified SOUP revision.
    pub fn qualified(
        task: impl Into<String>,
        batch_size: SoupBatchSize,
        auto_batch_strategy: SoupAutoBatchStrategy,
        streaming: Option<SoupLayerStreamingV1>,
    ) -> Result<Self, SoupContractError> {
        Self::from_external(
            SOUP_QUALIFIED_UPSTREAM_COMMIT,
            task,
            batch_size,
            auto_batch_strategy,
            streaming,
        )
    }

    /// SOUP task identity carried by this plan.
    #[must_use]
    pub fn task(&self) -> &str {
        &self.task
    }

    /// Batch-size choice.
    #[must_use]
    pub const fn batch_size(&self) -> SoupBatchSize {
        self.batch_size
    }

    /// SOUP-owned auto-batch resolution strategy.
    #[must_use]
    pub const fn auto_batch_strategy(&self) -> SoupAutoBatchStrategy {
        self.auto_batch_strategy
    }

    /// Optional layer-streaming choice.
    #[must_use]
    pub const fn streaming(&self) -> Option<SoupLayerStreamingV1> {
        self.streaming
    }

    /// Whether the plan changes the base-model residency axis.
    #[must_use]
    pub const fn uses_layer_streaming(&self) -> bool {
        self.streaming.is_some()
    }

    /// Map the external resource knobs into a generic Elastic declaration.
    ///
    /// Batch size is a capacity axis. Layer streaming adds a residency axis.
    /// Both are configuration decisions governed by the external SOUP v1
    /// contract; this declaration does not itself mutate a SOUP process.
    pub fn resource_spec(
        &self,
        resource_id: impl Into<String>,
    ) -> Result<ResourceSpec, SoupContractError> {
        let resource_id = LogicalResourceId::new(resource_id.into())?;
        let contract = ContractId::new(SOUP_RESOURCE_PLAN_V1)?;

        let mut builder = ResourceSpec::builder(ResourceClassId::CONFIGURATIONAL, resource_id)
            .allow(DimensionId::CAPACITY)
            .preserve(Invariant::new(InvariantKind::PreserveIdentity))
            .preserve(Invariant::new(InvariantKind::UpholdContract(contract.clone())))
            .admit(AdmissibleTransition::new(
                TransitionMechanism::Reinterpret,
                DimensionId::CAPACITY,
            ))
            .require_capability(CapabilityRequirement::new(
                TransitionMechanism::Reinterpret,
                DimensionId::CAPACITY,
            ))
            .observe(ObservationSignalId::FREE_CAPACITY)
            .observe(ObservationSignalId::UTILIZATION)
            .label("external.contract", SOUP_RESOURCE_PLAN_V1)
            .label("external.upstream_commit", SOUP_QUALIFIED_UPSTREAM_COMMIT)
            .label("external.task", self.task.clone());

        if self.streaming.is_some() {
            builder = builder
                .allow(DimensionId::RESIDENCY)
                .admit(AdmissibleTransition::new(
                    TransitionMechanism::Reinterpret,
                    DimensionId::RESIDENCY,
                ))
                .require_capability(CapabilityRequirement::new(
                    TransitionMechanism::Reinterpret,
                    DimensionId::RESIDENCY,
                ));
        }

        Ok(builder.build()?)
    }
}

/// Fail-closed errors at the SOUP/ElasticXxx contract boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SoupContractError {
    /// The external plan names a SOUP revision that v1 has not qualified.
    UnsupportedUpstreamRevision { revision: String },
    /// SOUP task identity is empty after trimming.
    BlankTask,
    /// A fixed SOUP batch size must be positive.
    InvalidBatchSize { batch_size: u32 },
    /// SOUP layer streaming requires 2..=8 buffers at the qualified revision.
    InvalidStreamBuffers { buffers: u8 },
    /// SOUP's layer-streaming planner does not admit this task.
    StreamingUnsupportedForTask { task: String },
    /// The generic Elastic resource declaration rejected the mapped envelope.
    ResourceSpec(ResourceSpecError),
}

impl fmt::Display for SoupContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedUpstreamRevision { revision } => write!(
                f,
                "SOUP revision {revision:?} is not qualified by {SOUP_RESOURCE_PLAN_V1}; expected {SOUP_QUALIFIED_UPSTREAM_COMMIT}"
            ),
            Self::BlankTask => write!(f, "SOUP task must not be blank"),
            Self::InvalidBatchSize { batch_size } => write!(
                f,
                "SOUP fixed batch size must be >= 1; got {batch_size}"
            ),
            Self::InvalidStreamBuffers { buffers } => write!(
                f,
                "SOUP stream buffers must be in [{SOUP_MIN_STREAM_BUFFERS}, {SOUP_MAX_STREAM_BUFFERS}]; got {buffers}"
            ),
            Self::StreamingUnsupportedForTask { task } => write!(
                f,
                "SOUP layer streaming is not qualified for task {task:?}; supported tasks: {}",
                SOUP_STREAM_TASKS.join(", ")
            ),
            Self::ResourceSpec(error) => write!(f, "invalid Elastic resource mapping: {error}"),
        }
    }
}

impl std::error::Error for SoupContractError {}

impl From<ResourceSpecError> for SoupContractError {
    fn from(value: ResourceSpecError) -> Self {
        Self::ResourceSpec(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qualified_resident_plan_maps_only_capacity() {
        let plan = SoupRunResourcePlanV1::qualified(
            "grpo",
            SoupBatchSize::Auto,
            SoupAutoBatchStrategy::Probe,
            None,
        )
        .unwrap();
        let spec = plan.resource_spec("soup-train").unwrap();

        assert!(spec.is_elastic(&DimensionId::CAPACITY));
        assert!(!spec.is_elastic(&DimensionId::RESIDENCY));
        assert_eq!(spec.label("external.contract"), Some(SOUP_RESOURCE_PLAN_V1));
        assert_eq!(
            spec.label("external.upstream_commit"),
            Some(SOUP_QUALIFIED_UPSTREAM_COMMIT)
        );
    }

    #[test]
    fn qualified_streaming_plan_maps_capacity_and_residency() {
        let streaming = SoupLayerStreamingV1::new(SoupStreamSource::Auto, 2).unwrap();
        let plan = SoupRunResourcePlanV1::qualified(
            "sft",
            SoupBatchSize::Fixed(1),
            SoupAutoBatchStrategy::Auto,
            Some(streaming),
        )
        .unwrap();
        let spec = plan.resource_spec("soup-streamed-train").unwrap();

        assert!(spec.is_elastic(&DimensionId::CAPACITY));
        assert!(spec.is_elastic(&DimensionId::RESIDENCY));
        assert!(spec.admits(TransitionMechanism::Reinterpret, &DimensionId::RESIDENCY));
    }

    #[test]
    fn unknown_upstream_revision_fails_closed() {
        let error = SoupRunResourcePlanV1::from_external(
            "future",
            "sft",
            SoupBatchSize::Auto,
            SoupAutoBatchStrategy::Auto,
            None,
        )
        .unwrap_err();
        assert!(matches!(
            error,
            SoupContractError::UnsupportedUpstreamRevision { .. }
        ));
    }

    #[test]
    fn zero_fixed_batch_is_rejected() {
        let error = SoupRunResourcePlanV1::qualified(
            "sft",
            SoupBatchSize::Fixed(0),
            SoupAutoBatchStrategy::Static,
            None,
        )
        .unwrap_err();
        assert_eq!(
            error,
            SoupContractError::InvalidBatchSize { batch_size: 0 }
        );
    }

    #[test]
    fn stream_buffer_bounds_match_qualified_soup_contract() {
        assert!(SoupLayerStreamingV1::new(SoupStreamSource::Ram, 2).is_ok());
        assert!(SoupLayerStreamingV1::new(SoupStreamSource::Disk, 8).is_ok());
        assert_eq!(
            SoupLayerStreamingV1::new(SoupStreamSource::Auto, 1).unwrap_err(),
            SoupContractError::InvalidStreamBuffers { buffers: 1 }
        );
        assert_eq!(
            SoupLayerStreamingV1::new(SoupStreamSource::Auto, 9).unwrap_err(),
            SoupContractError::InvalidStreamBuffers { buffers: 9 }
        );
    }

    #[test]
    fn rollout_task_streaming_is_rejected_without_weakening_resident_plan() {
        let stream = SoupLayerStreamingV1::default();
        let error = SoupRunResourcePlanV1::qualified(
            "grpo",
            SoupBatchSize::Auto,
            SoupAutoBatchStrategy::Auto,
            Some(stream),
        )
        .unwrap_err();
        assert_eq!(
            error,
            SoupContractError::StreamingUnsupportedForTask {
                task: "grpo".to_owned()
            }
        );

        assert!(SoupRunResourcePlanV1::qualified(
            "grpo",
            SoupBatchSize::Auto,
            SoupAutoBatchStrategy::Auto,
            None,
        )
        .is_ok());
    }
}
