//! User-facing facade for ElasticXxx.
//!
//! Downstream applications should depend on this crate rather than importing
//! implementation crates directly. The facade re-exports the typed resource
//! declaration API, deterministic EIR lowering, the operational runtime, and
//! reviewed adapter boundaries.

#![forbid(unsafe_code)]

pub mod boolean;
mod guard_macro;

pub use boolean::{predicate, ElasticGuard, ElasticGuardError, ElasticPredicates};
pub use elastic_adapters::{
    actuate_if_fresh, model_execution_current_profile_rank_signal,
    model_execution_profile_dimension, ActuationGateError, AdapterError, ConcurrencyPermits,
    HeadroomPlanner, ModelExecutionAdaptivePlannerV1, ModelExecutionAtomicProfileError,
    ModelExecutionAtomicProfilePlannerV1, ModelExecutionBasisPointAxis,
    ModelExecutionCapabilitiesV1, ModelExecutionCapabilitiesWireV1, ModelExecutionContractError,
    ModelExecutionEnvelopeError, ModelExecutionEnvelopePolicyV1,
    ModelExecutionEnvelopePolicyWireV1, ModelExecutionEnvelopeRuleV1,
    ModelExecutionEnvelopeRuleWireV1, ModelExecutionHardwarePlannerV1,
    ModelExecutionHardwareSelectionV1, ModelExecutionProfileEnvelopeV1, ModelExecutionProfileError,
    ModelExecutionProfilePlanV1, ModelExecutionProfilePlanWireV1, ModelExecutionProfileSelectionV1,
    ModelExecutionProfileSelectorV1, ModelExecutionProfileSetV1, ModelExecutionProfileSetWireV1,
    ModelExecutionProfileV1, ModelExecutionProfileWireV1, ModelExecutionResourcePlanV1,
    ModelExecutionResourcePlanWireV1, ModelExecutionResourceSnapshotV1, PlannerConfigError,
    RamBudget, SoupAutoBatchStrategy, SoupBatchSize, SoupBatchSizeWireV1, SoupContractError,
    SoupLayerStreamingV1, SoupLayerStreamingWireV1, SoupRunResourcePlanV1,
    SoupRunResourcePlanWireV1, SoupStreamSource, ThresholdPlanner,
    MODEL_EXECUTION_ACTIVATION_BUDGET_DIMENSION, MODEL_EXECUTION_ACTIVE_EXPERTS_DIMENSION,
    MODEL_EXECUTION_ATOMIC_PROFILE_V1, MODEL_EXECUTION_BASIS_POINTS_FULL,
    MODEL_EXECUTION_CAPABILITIES_MEDIA_TYPE_V1, MODEL_EXECUTION_CAPABILITIES_V1,
    MODEL_EXECUTION_CURRENT_PROFILE_RANK_SIGNAL, MODEL_EXECUTION_ENVELOPE_POLICY_MEDIA_TYPE_V1,
    MODEL_EXECUTION_ENVELOPE_POLICY_V1, MODEL_EXECUTION_EXPERT_WIDTH_DIMENSION,
    MODEL_EXECUTION_PROFILE_DIMENSION, MODEL_EXECUTION_PROFILE_PLAN_MEDIA_TYPE_V1,
    MODEL_EXECUTION_PROFILE_PLAN_V1, MODEL_EXECUTION_PROFILE_SET_MEDIA_TYPE_V1,
    MODEL_EXECUTION_PROFILE_SET_V1, MODEL_EXECUTION_RESOURCE_PLAN_MEDIA_TYPE_V1,
    MODEL_EXECUTION_RESOURCE_PLAN_V1, SOUP_DEFAULT_STREAM_BUFFERS, SOUP_HUB_RESOURCE_CONTRACT_V1,
    SOUP_MAX_STREAM_BUFFERS, SOUP_MIN_STREAM_BUFFERS, SOUP_QUALIFIED_UPSTREAM_COMMIT,
    SOUP_RESOURCE_PLAN_MEDIA_TYPE_V1, SOUP_RESOURCE_PLAN_V1, SOUP_STREAM_TASKS,
};
pub use elastic_core::control::{
    FreshnessSnapshot, ObservationEpoch, PlannerEpoch, RecommendationContext,
    RecommendationFreshnessError, ResourceGeneration,
};
pub use elastic_core::resource;
pub use elastic_core::resource::{
    AdmissibleTransition, BuiltinDimension, BuiltinObjective, BuiltinObservationSignal,
    BuiltinResourceClass, CapabilityRequirement, ContractId, DimensionId, Invariant, InvariantKind,
    LogicalResourceId, ObjectiveId, ObservationSignalId, ResourceClassId, ResourceSpec,
    ResourceSpecBuilder, ResourceSpecError,
};
pub use elastic_core::{
    BoolExpr, BoolExprFingerprint, BooleanGuard, CanonicalizationError, CompiledGuard, FactMask,
    FactSet, GuardBindingError, GuardFactSource, GuardScope, GuardedResourceSpec,
    InvariantPredicateBinding, LogicError, PredicateComponent, PredicateComponentError,
    PredicateId, PredicateKey, PredicateRegistry, PredicateRegistryError, TransitionGuard,
    TransitionMechanism, TruthValue, BOOLEAN_EXPRESSION_SCHEMA_V1, BOOLEAN_PREDICATE_SCHEMA_V1,
    FAST_PREDICATE_CAPACITY, MAX_BOOLEAN_EXPR_DEPTH, MAX_CANONICAL_EXPRESSION_NODES,
    MAX_PREDICATE_COMPONENT_BYTES, MAX_REGISTERED_PREDICATES,
};
pub use elastic_eir::PlanningSubsetError;
pub use elastic_eir::{
    evaluate_transition_guards, lower, lower_guarded, prune_transition_candidates, EirDocument,
    EirDocumentBuilder, EirGuard, EirGuardedResource, EirPredicate, EirResource, Fingerprint,
    FirstGroundedPlanner, GuardedTransitionOutcome, PlanOutcome, PlanningContext,
    RejectedTransition, TransitionCandidate, TransitionPlanner, TransitionPruningReport,
    UnknownTransition, EIR_BOOLEAN_GUARD_SCHEMA_VERSION,
};
pub use elastic_macros::ElasticResource;
pub use elastic_runtime::{
    capture_decision_trace, capture_guarded_planning_trace, fact_snapshot_fingerprint,
    observation_source_for, planning_context_fingerprint, precheck_plan_invariants,
    EvidenceCommand, EvidenceDiff, EvidenceEnvelope, EvidenceError, EvidenceEvent,
    EvidenceEventKind, EvidenceSchema, EvidenceSummary, EVIDENCE_SCHEMA_V1, MAX_EVIDENCE_BYTES,
    MAX_EVIDENCE_COLLECTION_ITEMS, MAX_EVIDENCE_DEPTH, MAX_EVIDENCE_DIFF_PATHS, MAX_EVIDENCE_NODES,
    MAX_EVIDENCE_RESOURCE_ID_BYTES, MAX_EVIDENCE_STRING_BYTES,
};
pub use elastic_runtime::{
    Actuation, BooleanGuardPlanner, BooleanGuardPreplanner, BuiltinDimensionConfigV1,
    BuiltinObservationSignalConfigV1, Cadence, CadenceConfig, CancellationToken,
    CandidateDecisionTrace, CapabilityPredicate, CommitRecord, ConcurrencyPermitsObserver,
    ConfiguredController, ConfiguredForecaster, ConfiguredPlanner, ConfiguredPlanningView,
    ConfiguredResource, ConfiguredResourceState, ConfiguredThresholdPredicateV1, Controller,
    ControllerConfig, CurrentStateForecaster, CycleAttempt, CycleFailure, CycleResult,
    DecisionReplayError, DecisionStopReason, DecisionTrace, DecisionTraceChange,
    DecisionTraceChangeKind, DecisionTraceDiff, DecisionTraceError, DimensionConfigV1,
    EwmaForecaster, ExecutionModeConfig, FactDerivationError, FactFreshnessError,
    FactResourceBinding, FactSnapshot, FactSnapshotFingerprint, FactSourceId,
    FixedModelExecutionTransitionPolicyV1, Forecast, ForecastController, ForecastCycleAttempt,
    ForecastCycleFailure, ForecastCycleResult, ForecastRunAttempt, ForecastRunFailure,
    ForecastRunResult, ForecastRuntime, ForecastStatus, Forecaster, ForecasterSelection,
    GuardConfigError, GuardConfigV1, GuardExprConfigV1, GuardPlannerTarget, GuardPreplannerError,
    GuardRuleConfigV1, GuardScopeConfigV1, GuardedPlanningDecision, GuardedPlanningOutcomeTrace,
    GuardedPlanningTrace, HostMemoryObserver, InvariantCheck, InvariantPrecheckEntry,
    InvariantPrecheckError, InvariantPrecheckReport, InvariantPrecheckStatus,
    InvariantPrecheckTraceSummary, LoopStopReason, LoweredGuardConfigV1,
    ModelExecutionActuationEvidenceV1, ModelExecutionControllerContractsV1,
    ModelExecutionControllerContractsWireV1, ModelExecutionControllerV1,
    ModelExecutionCycleEvidenceV1, ModelExecutionForecastEvidenceV1,
    ModelExecutionForecastStatusEvidenceV1, ModelExecutionInvariantEvidenceV1,
    ModelExecutionObservationEvidenceV1, ModelExecutionObservationSnapshotEvidenceV1,
    ModelExecutionObserverBundleV1, ModelExecutionPlanEvidenceV1,
    ModelExecutionPlanOutcomeEvidenceV1, ModelExecutionProfileBackendV1,
    ModelExecutionResourceObserverV1, ModelExecutionResourceTelemetrySampleV1,
    ModelExecutionResourceTelemetryV1, ModelExecutionRollbackEvidenceV1,
    ModelExecutionRunEvidenceAttemptV1, ModelExecutionRunEvidenceFailureV1,
    ModelExecutionRunEvidenceResultV1, ModelExecutionSelectedProfileEvidenceV1,
    ModelExecutionSignalEvidenceV1, ModelExecutionTransitionModeV1,
    ModelExecutionTransitionPolicyV1, ModelExecutionVerificationEvidenceV1, NoopEventSink,
    Observation, ObservationFreshnessPredicate, ObservationPresencePredicate,
    ObservationSignalConfigV1, ObservationSnapshot, ObservationSource,
    ObservationThresholdPredicate, Observer, ObserverSet, OperatorConfig, Plan, PlannerConfig,
    PlannerSelection, PlanningContextFingerprint, PredicateConfigV1, PredicateEvaluationInput,
    PredicateEvaluator, PredicateKeyConfigV1, PredicateTraceEntry, RamBudgetObserver,
    RegisteredResource, RejectedCandidateTrace, ResourceConfig, ResourceRegistry, RollbackRecord,
    RunResult, Runtime, RuntimeClock, RuntimeConfig, RuntimeError, RuntimeEvent, RuntimeEventKind,
    RuntimeEventSink, RuntimeMode, RuntimeTimingObserver, SystemClock, ThresholdComparison,
    ThresholdComparisonConfigV1, TransactionalActuator, TransactionalConcurrency,
    TransactionalModelExecution, TransactionalRam, TransitionGuardedModelExecutionBackendError,
    TransitionGuardedModelExecutionBackendV1, TransitionMechanismConfigV1, UnknownCandidateTrace,
    ValidatedPlan, VerificationResult, DECISION_TRACE_SCHEMA_V1, GUARD_CONFIG_SCHEMA_V1,
    MAX_DECISION_TRACE_BYTES, MAX_FACTS_PER_SNAPSHOT, MAX_FACT_SOURCE_ID_BYTES,
    MAX_GUARD_CONFIG_BYTES, MAX_GUARD_CONFIG_EXPR_NODES, MAX_GUARD_CONFIG_GUARDS,
    MAX_GUARD_CONFIG_TERM_BYTES, MODEL_EXECUTION_CONTROLLER_CONTRACTS_MEDIA_TYPE_V1,
    MODEL_EXECUTION_CONTROLLER_CONTRACTS_V1, MODEL_EXECUTION_CYCLE_EVIDENCE_MEDIA_TYPE_V1,
    MODEL_EXECUTION_CYCLE_EVIDENCE_V1, OPERATOR_CONFIG_VERSION,
};
pub use elastic_runtime::{
    CapacityAdmissionControllerV1, CapacityAdmissionReportV1, CapacityAdmissionRequestV1,
    CapacityObservationV1, CapacityStateV1,
};

/// Operational runtime surface for users that prefer an explicit namespace.
pub mod runtime {
    pub use elastic_runtime::*;
}

/// Reference in-process adapters, planners, and reviewed ecosystem boundaries.
pub mod adapters {
    pub use elastic_adapters::*;
    pub use elastic_runtime::{
        FixedModelExecutionTransitionPolicyV1, ModelExecutionControllerContractsV1,
        ModelExecutionControllerContractsWireV1, ModelExecutionControllerV1,
        ModelExecutionCycleEvidenceV1, ModelExecutionObserverBundleV1,
        ModelExecutionProfileBackendV1, ModelExecutionResourceObserverV1,
        ModelExecutionResourceTelemetrySampleV1, ModelExecutionResourceTelemetryV1,
        ModelExecutionRunEvidenceAttemptV1, ModelExecutionRunEvidenceFailureV1,
        ModelExecutionRunEvidenceResultV1, ModelExecutionTransitionModeV1,
        ModelExecutionTransitionPolicyV1, TransactionalConcurrency, TransactionalModelExecution,
        TransactionalRam, TransitionGuardedModelExecutionBackendError,
        TransitionGuardedModelExecutionBackendV1,
    };
}

/// Everything needed by a typical Elastic application.
pub mod prelude {
    pub use crate::boolean::{predicate, ElasticGuard, ElasticGuardError, ElasticPredicates};
    pub use crate::elastic_guard;
    pub use elastic_adapters::{
        model_execution_current_profile_rank_signal, model_execution_profile_dimension,
        ConcurrencyPermits, HeadroomPlanner, ModelExecutionAdaptivePlannerV1,
        ModelExecutionAtomicProfilePlannerV1, ModelExecutionCapabilitiesV1,
        ModelExecutionCapabilitiesWireV1, ModelExecutionEnvelopePolicyV1,
        ModelExecutionEnvelopeRuleV1, ModelExecutionHardwarePlannerV1,
        ModelExecutionHardwareSelectionV1, ModelExecutionProfileEnvelopeV1,
        ModelExecutionProfilePlanV1, ModelExecutionProfileSelectionV1,
        ModelExecutionProfileSelectorV1, ModelExecutionProfileSetV1, ModelExecutionProfileV1,
        ModelExecutionResourcePlanV1, ModelExecutionResourcePlanWireV1,
        ModelExecutionResourceSnapshotV1, RamBudget, SoupAutoBatchStrategy, SoupBatchSize,
        SoupBatchSizeWireV1, SoupLayerStreamingV1, SoupLayerStreamingWireV1, SoupRunResourcePlanV1,
        SoupRunResourcePlanWireV1, SoupStreamSource, ThresholdPlanner,
    };
    pub use elastic_core::resource::{
        AdmissibleTransition, CapabilityRequirement, ContractId, DimensionId, Invariant,
        InvariantKind, LogicalResourceId, ObjectiveId, ObservationSignalId, ResourceClassId,
        ResourceSpec, ResourceSpecError,
    };
    pub use elastic_core::{
        BoolExpr, BooleanGuard, CanonicalizationError, CompiledGuard, FactMask, FactSet,
        GuardBindingError, GuardScope, GuardedResourceSpec, InvariantPredicateBinding, LogicError,
        PredicateId, PredicateKey, PredicateRegistry, PredicateRegistryError, TransitionGuard,
        TransitionMechanism, TruthValue, FAST_PREDICATE_CAPACITY, MAX_BOOLEAN_EXPR_DEPTH,
    };
    pub use elastic_eir::{
        evaluate_transition_guards, lower, lower_guarded, prune_transition_candidates, EirDocument,
        EirGuardedResource, EirResource, Fingerprint, FirstGroundedPlanner, PlanningContext,
        TransitionPlanner, TransitionPruningReport,
    };
    pub use elastic_macros::ElasticResource;
    pub use elastic_runtime::{
        capture_decision_trace, capture_guarded_planning_trace, fact_snapshot_fingerprint,
        planning_context_fingerprint, precheck_plan_invariants, BooleanGuardPlanner,
        BooleanGuardPreplanner, BuiltinDimensionConfigV1, BuiltinObservationSignalConfigV1,
        CadenceConfig, CancellationToken, CapabilityPredicate, ConfiguredController,
        ConfiguredForecaster, ConfiguredPlanner, ConfiguredResource, ConfiguredResourceState,
        Controller, ControllerConfig, CurrentStateForecaster, CycleAttempt, CycleFailure,
        DecisionTrace, DecisionTraceChange, DecisionTraceChangeKind, DecisionTraceDiff,
        DecisionTraceError, DimensionConfigV1, EwmaForecaster, ExecutionModeConfig,
        FactResourceBinding, FactSnapshot, FactSourceId, FixedModelExecutionTransitionPolicyV1,
        Forecast, ForecastController, ForecastCycleAttempt, ForecastCycleFailure,
        ForecastCycleResult, ForecastRunAttempt, ForecastRunFailure, ForecastRunResult,
        ForecastRuntime, Forecaster, ForecasterSelection, GuardConfigError, GuardConfigV1,
        GuardExprConfigV1, GuardPlannerTarget, GuardPreplannerError, GuardRuleConfigV1,
        GuardScopeConfigV1, GuardedPlanningDecision, GuardedPlanningOutcomeTrace,
        GuardedPlanningTrace, HostMemoryObserver, InvariantPrecheckReport, InvariantPrecheckStatus,
        InvariantPrecheckTraceSummary, ModelExecutionControllerContractsV1,
        ModelExecutionControllerContractsWireV1, ModelExecutionControllerV1,
        ModelExecutionCycleEvidenceV1, ModelExecutionObserverBundleV1,
        ModelExecutionProfileBackendV1, ModelExecutionResourceObserverV1,
        ModelExecutionResourceTelemetrySampleV1, ModelExecutionResourceTelemetryV1,
        ModelExecutionRunEvidenceAttemptV1, ModelExecutionRunEvidenceFailureV1,
        ModelExecutionRunEvidenceResultV1, ModelExecutionTransitionModeV1,
        ModelExecutionTransitionPolicyV1, Observation, ObservationSignalConfigV1, Observer,
        OperatorConfig, PlannerSelection, PlanningContextFingerprint, PredicateConfigV1,
        PredicateEvaluationInput, PredicateEvaluator, PredicateKeyConfigV1, RamBudgetObserver,
        RegisteredResource, ResourceConfig, ResourceRegistry, Runtime, RuntimeConfig, RuntimeError,
        RuntimeMode, ThresholdComparison, ThresholdComparisonConfigV1, TransactionalActuator,
        TransactionalConcurrency, TransactionalModelExecution, TransactionalRam,
        TransitionGuardedModelExecutionBackendError, TransitionGuardedModelExecutionBackendV1,
        TransitionMechanismConfigV1, VerificationResult, DECISION_TRACE_SCHEMA_V1,
        GUARD_CONFIG_SCHEMA_V1, MODEL_EXECUTION_CONTROLLER_CONTRACTS_V1,
        MODEL_EXECUTION_CYCLE_EVIDENCE_V1, OPERATOR_CONFIG_VERSION,
    };
    pub use elastic_runtime::{
        EvidenceCommand, EvidenceDiff, EvidenceEnvelope, EvidenceError, EvidenceEvent,
        EvidenceEventKind, EvidenceSchema, EvidenceSummary, EVIDENCE_SCHEMA_V1, MAX_EVIDENCE_BYTES,
    };
}
