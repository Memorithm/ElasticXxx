//! User-facing facade for ElasticXxx.
//!
//! Downstream applications should depend on this crate rather than importing
//! implementation crates directly. The facade re-exports the typed resource
//! declaration API, deterministic EIR lowering, the operational runtime, and
//! reviewed adapter boundaries.

#![forbid(unsafe_code)]

pub use elastic_adapters::{
    actuate_if_fresh, ActuationGateError, AdapterError, ConcurrencyPermits, HeadroomPlanner,
    PlannerConfigError, RamBudget, SoupAutoBatchStrategy, SoupBatchSize, SoupContractError,
    SoupLayerStreamingV1, SoupRunResourcePlanV1, SoupStreamSource, ThresholdPlanner,
    SOUP_DEFAULT_STREAM_BUFFERS, SOUP_HUB_RESOURCE_CONTRACT_V1, SOUP_MAX_STREAM_BUFFERS,
    SOUP_MIN_STREAM_BUFFERS, SOUP_QUALIFIED_UPSTREAM_COMMIT, SOUP_RESOURCE_PLAN_V1,
    SOUP_STREAM_TASKS,
};
pub use elastic_core::resource;
pub use elastic_core::resource::{
    AdmissibleTransition, BuiltinDimension, BuiltinObjective, BuiltinObservationSignal,
    BuiltinResourceClass, CapabilityRequirement, ContractId, DimensionId, Invariant, InvariantKind,
    LogicalResourceId, ObjectiveId, ObservationSignalId, ResourceClassId, ResourceSpec,
    ResourceSpecBuilder, ResourceSpecError,
};
pub use elastic_core::TransitionMechanism;
pub use elastic_eir::{
    lower, EirDocument, EirDocumentBuilder, EirResource, Fingerprint, FirstGroundedPlanner,
    PlanOutcome, PlanningContext, TransitionCandidate, TransitionPlanner,
};
pub use elastic_macros::ElasticResource;
pub use elastic_runtime::{
    Actuation, Cadence, CadenceConfig, CancellationToken, CommitRecord, ConcurrencyPermitsObserver,
    ConfiguredController, ConfiguredForecaster, ConfiguredPlanner, ConfiguredResource,
    ConfiguredResourceState, Controller, ControllerConfig, CurrentStateForecaster, CycleResult,
    EwmaForecaster, ExecutionModeConfig, Forecast, ForecastController, ForecastCycleResult,
    ForecastRunResult, ForecastRuntime, ForecastStatus, Forecaster, ForecasterSelection,
    HostMemoryObserver, InvariantCheck, LoopStopReason, NoopEventSink, Observation,
    ObservationSnapshot, ObservationSource, Observer, ObserverSet, OperatorConfig, Plan,
    PlannerConfig, PlannerSelection, RamBudgetObserver, RegisteredResource, ResourceConfig,
    ResourceRegistry, RollbackRecord, RunResult, Runtime, RuntimeClock, RuntimeConfig,
    RuntimeError, RuntimeEvent, RuntimeEventKind, RuntimeEventSink, RuntimeMode,
    RuntimeTimingObserver, SystemClock, TransactionalActuator, TransactionalConcurrency,
    TransactionalRam, ValidatedPlan, VerificationResult, OPERATOR_CONFIG_VERSION,
};
pub use elastic_runtime::{
    EvidenceCommand, EvidenceDiff, EvidenceEnvelope, EvidenceError, EvidenceEvent,
    EvidenceEventKind, EvidenceSchema, EvidenceSummary, EVIDENCE_SCHEMA_V1, MAX_EVIDENCE_BYTES,
    MAX_EVIDENCE_COLLECTION_ITEMS, MAX_EVIDENCE_DEPTH, MAX_EVIDENCE_DIFF_PATHS, MAX_EVIDENCE_NODES,
    MAX_EVIDENCE_RESOURCE_ID_BYTES, MAX_EVIDENCE_STRING_BYTES,
};

/// Operational runtime surface for users that prefer an explicit namespace.
pub mod runtime {
    pub use elastic_runtime::*;
}

/// Reference in-process adapters, planners, and reviewed ecosystem boundaries.
pub mod adapters {
    pub use elastic_adapters::*;
    pub use elastic_runtime::{TransactionalConcurrency, TransactionalRam};
}

/// Everything needed by a typical Elastic application.
pub mod prelude {
    pub use elastic_adapters::{
        ConcurrencyPermits, HeadroomPlanner, RamBudget, SoupAutoBatchStrategy, SoupBatchSize,
        SoupLayerStreamingV1, SoupRunResourcePlanV1, SoupStreamSource, ThresholdPlanner,
    };
    pub use elastic_core::resource::{
        AdmissibleTransition, CapabilityRequirement, ContractId, DimensionId, Invariant,
        InvariantKind, LogicalResourceId, ObjectiveId, ObservationSignalId, ResourceClassId,
        ResourceSpec, ResourceSpecError,
    };
    pub use elastic_core::TransitionMechanism;
    pub use elastic_eir::{
        lower, EirDocument, EirResource, Fingerprint, FirstGroundedPlanner, TransitionPlanner,
    };
    pub use elastic_macros::ElasticResource;
    pub use elastic_runtime::{
        CadenceConfig, CancellationToken, ConcurrencyPermitsObserver, ConfiguredController,
        ConfiguredForecaster, ConfiguredPlanner, ConfiguredResource, ConfiguredResourceState,
        Controller, ControllerConfig, CurrentStateForecaster, EwmaForecaster, ExecutionModeConfig,
        Forecast, ForecastController, ForecastCycleResult, ForecastRunResult, ForecastRuntime,
        Forecaster, ForecasterSelection, HostMemoryObserver, Observation, Observer, OperatorConfig,
        PlannerSelection, RamBudgetObserver, RegisteredResource, ResourceConfig, ResourceRegistry,
        Runtime, RuntimeConfig, RuntimeError, RuntimeMode, TransactionalActuator,
        TransactionalConcurrency, TransactionalRam, VerificationResult, OPERATOR_CONFIG_VERSION,
    };
    pub use elastic_runtime::{
        EvidenceCommand, EvidenceDiff, EvidenceEnvelope, EvidenceError, EvidenceEvent,
        EvidenceEventKind, EvidenceSchema, EvidenceSummary, EVIDENCE_SCHEMA_V1, MAX_EVIDENCE_BYTES,
    };
}
