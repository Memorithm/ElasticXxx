//! Concrete, in-process resource adapters and reviewed ecosystem boundaries.
//!
//! An adapter is the **trusted boundary** between validated Elastic intent
//! and physical action. The local RAM/concurrency adapters deliberately remain
//! portable and dependency-free. Ecosystem boundary modules may additionally
//! validate versioned external resource-plan envelopes without claiming a
//! physical effect that the owning product does not expose safely.
//!
//! Normative adapter contract demonstrated here:
//!
//! 1. declarations are constructed through the typed surface model and are
//!    valid by construction;
//! 2. observations are plain numbers derived from adapter state plus
//!    **operator-supplied configuration** (never probed from the OS);
//! 3. planner output can be routed through [`actuate_if_fresh`] so the
//!    recommendation must explicitly track the resource and all recorded
//!    planner/observation/resource generations must still be current;
//! 4. only [`apply`](ram::RamBudget::apply) style methods act, and every
//!    proposal is re-validated against bounds, step limits, and invariants
//!    immediately before the effect;
//! 5. an adapter may refuse any action that would violate a declared
//!    invariant — planners and freshness checks cannot override refusals;
//! 6. a pre-execution ecosystem plan must stay pre-execution unless the owning
//!    product publishes a separately qualified live-actuation contract.

#![forbid(unsafe_code)]

pub mod actuation;
pub mod error;
pub mod model_execution;
pub mod model_execution_profiles;
pub mod permits;
pub mod planners;
pub mod ram;
pub mod soup;

pub use actuation::{actuate_if_fresh, ActuationGateError};
pub use error::AdapterError;
pub use model_execution::{
    ModelExecutionBasisPointAxis, ModelExecutionCapabilitiesV1, ModelExecutionCapabilitiesWireV1,
    ModelExecutionContractError, ModelExecutionResourcePlanV1, ModelExecutionResourcePlanWireV1,
    MODEL_EXECUTION_ACTIVATION_BUDGET_DIMENSION, MODEL_EXECUTION_ACTIVE_EXPERTS_DIMENSION,
    MODEL_EXECUTION_BASIS_POINTS_FULL, MODEL_EXECUTION_CAPABILITIES_MEDIA_TYPE_V1,
    MODEL_EXECUTION_CAPABILITIES_V1, MODEL_EXECUTION_EXPERT_WIDTH_DIMENSION,
    MODEL_EXECUTION_RESOURCE_PLAN_MEDIA_TYPE_V1, MODEL_EXECUTION_RESOURCE_PLAN_V1,
};
pub use model_execution_profiles::{
    ModelExecutionProfileEnvelopeV1, ModelExecutionProfileError, ModelExecutionProfilePlanV1,
    ModelExecutionProfilePlanWireV1, ModelExecutionProfileSelectionV1,
    ModelExecutionProfileSelectorV1, ModelExecutionProfileSetV1, ModelExecutionProfileSetWireV1,
    ModelExecutionProfileV1, ModelExecutionProfileWireV1,
    MODEL_EXECUTION_PROFILE_PLAN_MEDIA_TYPE_V1, MODEL_EXECUTION_PROFILE_PLAN_V1,
    MODEL_EXECUTION_PROFILE_SET_MEDIA_TYPE_V1, MODEL_EXECUTION_PROFILE_SET_V1,
};
pub use permits::ConcurrencyPermits;
pub use planners::{HeadroomPlanner, PlannerConfigError, ThresholdPlanner};
pub use ram::RamBudget;
pub use soup::{
    SoupAutoBatchStrategy, SoupBatchSize, SoupBatchSizeWireV1, SoupContractError,
    SoupLayerStreamingV1, SoupLayerStreamingWireV1, SoupRunResourcePlanV1,
    SoupRunResourcePlanWireV1, SoupStreamSource, SOUP_DEFAULT_STREAM_BUFFERS,
    SOUP_HUB_RESOURCE_CONTRACT_V1, SOUP_MAX_STREAM_BUFFERS, SOUP_MIN_STREAM_BUFFERS,
    SOUP_QUALIFIED_UPSTREAM_COMMIT, SOUP_RESOURCE_PLAN_MEDIA_TYPE_V1, SOUP_RESOURCE_PLAN_V1,
    SOUP_STREAM_TASKS,
};