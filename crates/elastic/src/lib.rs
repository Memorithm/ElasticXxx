//! User-facing facade for ElasticXxx.
//!
//! Downstream applications should depend on this crate rather than importing
//! implementation crates directly. The facade re-exports the typed resource
//! declaration API, deterministic EIR lowering, the operational runtime, and
//! reviewed adapter boundaries.

#![forbid(unsafe_code)]

pub mod boolean;
mod guard_macro;

/// Versioned KV-cache representation and transaction contracts.
///
/// This namespace exposes the reviewed `elastic-kv` boundary through the
/// single-dependency facade. Physical storage and codec semantics remain owned
/// by downstream backends implementing [`kv::KvTransitionBackendV1`]; a
/// planning or Boolean admission result never authorizes actuation by itself.
pub mod kv {
    pub use elastic_core::{
        CapabilitySet, RepresentationEpoch, RepresentationId, RepresentationState, TargetContract,
        TransitionAttestations,
    };
    pub use elastic_kv::*;
}

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
    analyze_resource_policy, BoolExpr, BoolExprFingerprint, BooleanCpuArchitecture,
    BooleanCpuFeatures, BooleanGuard, CanonicalizationError, CapabilitySet, CompiledGuard,
    ExactBooleanOracle, ExactKleeneOracle, ExactOracleError, ExactOracleLimits,
    ExactPropertyReport, ExactPseudoBooleanOracle, ExactPseudoBooleanOracleError,
    ExactPseudoBooleanReport, ExactSatisfiabilityReport, FactMask, FactSet, GuardBindingError,
    GuardFactSource, GuardScope, GuardedResourceSpec, InvariantGuardDiagnostic,
    InvariantGuardStatus, InvariantPredicateBinding, KleeneOracleError, KleenePropertyReport,
    KleeneSatisfiabilityReport, LogicError, MultiwordBatchError, MultiwordBatchScreen,
    MultiwordCompiledGuard, MultiwordFactError, MultiwordFactSet, MultiwordFactWord,
    MultiwordGuardBatch, MultiwordGuardError, MultiwordGuardPath, PredicateComponent,
    PredicateComponentError, PredicateId, PredicateKey, PredicateRegistry, PredicateRegistryError,
    PseudoBooleanBindingError, PseudoBooleanConstraint, PseudoBooleanConstraintDeclaration,
    PseudoBooleanError, PseudoBooleanRelation, PseudoBooleanScale, RepresentationEpoch,
    RepresentationId, RepresentationState, RepresentationTransition, ResourcePolicyAnalysis,
    ResourcePolicyAnalysisError, SymbolicBackend, SymbolicBackendConfig,
    SymbolicBackendConfigError, SymbolicBackendResult, SymbolicResource, TransitionAttestations,
    TransitionGuard, TransitionMechanism, TransitionPairAnalysis, TransitionPolicyAnalysis,
    TruthValue, WeightedPredicate, WeightedPredicateKey, BOOLEAN_EXPRESSION_SCHEMA_V1,
    BOOLEAN_PREDICATE_SCHEMA_V1, DEFAULT_EXACT_ORACLE_ASSIGNMENTS, DEFAULT_EXACT_ORACLE_VARIABLES,
    DEFAULT_SYMBOLIC_MAX_CLAUSES, DEFAULT_SYMBOLIC_MAX_MEMORY_BYTES, DEFAULT_SYMBOLIC_MAX_NODES,
    DEFAULT_SYMBOLIC_SEED, DEFAULT_SYMBOLIC_TIMEOUT_MILLIS, FAST_PREDICATE_CAPACITY,
    MAX_BOOLEAN_EXPR_DEPTH, MAX_CANONICAL_EXPRESSION_NODES, MAX_EXACT_ORACLE_ASSIGNMENTS,
    MAX_EXACT_ORACLE_VARIABLES, MAX_MULTIWORD_FACT_PREDICATES, MAX_MULTIWORD_FACT_WORDS,
    MAX_MULTIWORD_GUARDS_PER_BATCH, MAX_PREDICATE_COMPONENT_BYTES, MAX_PSEUDO_BOOLEAN_TERMS,
    MAX_PSEUDO_BOOLEAN_UNIT_BYTES, MAX_REGISTERED_PREDICATES,
    MAX_RESOURCE_POLICY_INVARIANT_BINDINGS, MAX_RESOURCE_POLICY_PAIRS,
    MAX_RESOURCE_POLICY_TRANSITIONS, MULTIWORD_FACT_WORD_BITS,
};
pub use elastic_eir::PlanningSubsetError;
pub use elastic_eir::{
    evaluate_transition_guards, lower, lower_constrained, lower_guarded,
    prune_transition_candidates, ConstraintLoweringError, EirConstrainedResource, EirDocument,
    EirDocumentBuilder, EirGuard, EirGuardedResource, EirPredicate, EirPseudoBooleanConstraint,
    EirPseudoBooleanTerm, EirResource, Fingerprint, FirstGroundedPlanner, GuardedTransitionOutcome,
    PlanOutcome, PlanningContext, RejectedTransition, TransitionCandidate, TransitionPlanner,
    TransitionPruningReport, UnknownTransition, EIR_BOOLEAN_GUARD_SCHEMA_VERSION,
    EIR_PSEUDO_BOOLEAN_CONSTRAINT_SCHEMA_VERSION, MAX_EIR_PSEUDO_BOOLEAN_CONSTRAINTS,
};
pub use elastic_macros::ElasticResource;
pub use elastic_runtime::{
    batch_device_capacity_predicate_key, BatchDeviceCandidateV1, BatchDeviceCapacitySampleV1,
    BatchDeviceCapacitySnapshotV1, BooleanBatchDeviceCandidateEvidenceV1,
    BooleanBatchDeviceDecisionTraceV1, BooleanBatchDeviceOutcomeV1, BooleanBatchDevicePreplannerV1,
    BooleanBatchDeviceReportV1, BATCH_DEVICE_CAPACITY_SOURCE_UNIT, BATCH_DEVICE_MAX_AGE,
    BATCH_DEVICE_PREDICATE_NAMESPACE, BOOLEAN_BATCH_DEVICE_DECISION_TRACE_SCHEMA_V1,
    MAX_BATCH_DEVICE_CANDIDATES, MAX_BATCH_DEVICE_SAMPLES, MAX_BOOLEAN_BATCH_DEVICE_TRACE_BYTES,
};
pub use elastic_runtime::{
    capture_constrained_decision_trace, capture_decision_trace, capture_guarded_planning_trace,
    fact_snapshot_fingerprint, observation_source_for, planning_context_fingerprint,
    precheck_plan_invariants, EvidenceCommand, EvidenceDiff, EvidenceEnvelope, EvidenceError,
    EvidenceEvent, EvidenceEventKind, EvidenceSchema, EvidenceSummary, EVIDENCE_SCHEMA_V1,
    MAX_EVIDENCE_BYTES, MAX_EVIDENCE_COLLECTION_ITEMS, MAX_EVIDENCE_DEPTH, MAX_EVIDENCE_DIFF_PATHS,
    MAX_EVIDENCE_NODES, MAX_EVIDENCE_RESOURCE_ID_BYTES, MAX_EVIDENCE_STRING_BYTES,
};
pub use elastic_runtime::{
    concurrency_headroom_predicate_key, BooleanConcurrencyEvidenceV1,
    BooleanConcurrencyResizeControllerV1, BooleanConcurrencyResizeReportV1,
    ConcurrencyResizeReportV1, CONCURRENCY_HEADROOM_MAX_AGE, CONCURRENCY_HEADROOM_PREDICATE_NAME,
    CONCURRENCY_HEADROOM_PREDICATE_NAMESPACE, CONCURRENCY_HEADROOM_SOURCE_UNIT,
};
pub use elastic_runtime::{
    execute_guarded_batch_device_transaction, execute_unguarded_batch_device_transaction,
    BatchDevicePlacementBackendV1, BatchDeviceTransactionBlockV1, BatchDeviceTransactionFailureV1,
    BatchDeviceTransactionStageV1, CommittedBatchDeviceSelectionV1,
    GuardedBatchDeviceTransactionOutcomeV1,
};
pub use elastic_runtime::{
    model_execution_envelope_predicate_key, BooleanModelExecutionProfileControllerV1,
    BooleanModelExecutionProfileEvidenceV1, BooleanModelExecutionProfileReportV1,
    MODEL_EXECUTION_ENVELOPE_PREDICATE_NAME, MODEL_EXECUTION_ENVELOPE_PREDICATE_NAMESPACE,
};
pub use elastic_runtime::{
    model_execution_rule_free_capacity_predicate_key,
    model_execution_rule_utilization_predicate_key, BooleanModelExecutionPreplannerV1,
    BooleanModelExecutionRuleEvidenceV1, BooleanModelExecutionScreenOutcomeV1,
    BooleanModelExecutionScreenReportV1, MODEL_EXECUTION_BOOLEAN_MAX_AGE,
    MODEL_EXECUTION_FREE_CAPACITY_SOURCE_UNIT, MODEL_EXECUTION_RULE_PREDICATE_NAMESPACE,
    MODEL_EXECUTION_UTILIZATION_SOURCE_UNIT, MODEL_EXECUTION_UTILIZATION_THRESHOLD_UNIT,
};
pub use elastic_runtime::{
    ram_capacity_predicate_key, representation_precision_floor_predicate_key,
    representation_precision_floor_signal, BooleanRamCapacityAdmissionControllerV1,
    BooleanRamCapacityAdmissionReportV1, BooleanRamCapacityAdmissionReportV2,
    BooleanRamCapacityEvidenceV1, BooleanRamCapacityEvidenceV2,
    BooleanRepresentationPrecisionCandidateEvidenceV1,
    BooleanRepresentationPrecisionCandidateTraceV1, BooleanRepresentationPrecisionOutcomeV1,
    BooleanRepresentationPrecisionPreplannerV1, BooleanRepresentationPrecisionReportV1,
    BooleanRepresentationPrecisionReportV2, CapacityAdmissionControllerV1,
    CapacityAdmissionReportV1, CapacityAdmissionRequestV1, CapacityObservationV1, CapacityStateV1,
    RAM_CAPACITY_PREDICATE_NAME, RAM_CAPACITY_PREDICATE_NAMESPACE, RAM_CAPACITY_SOURCE_UNIT,
};
pub use elastic_runtime::{
    Actuation, BooleanGuardPlanner, BooleanGuardPreplanner, BuiltinDimensionConfigV1,
    BuiltinObservationSignalConfigV1, Cadence, CadenceConfig, CancellationToken,
    CandidateDecisionTrace, CapabilityPredicate, CommitRecord, ConcurrencyPermitsObserver,
    ConfiguredController, ConfiguredForecaster, ConfiguredPlanner, ConfiguredPlanningView,
    ConfiguredResource, ConfiguredResourceState, ConfiguredThresholdPredicateV1,
    ConstrainedDecisionTrace, Controller, ControllerConfig, CurrentStateForecaster, CycleAttempt,
    CycleFailure, CycleResult, DecisionReplayError, DecisionStopReason, DecisionTrace,
    DecisionTraceChange, DecisionTraceChangeKind, DecisionTraceDiff, DecisionTraceError,
    DimensionConfigV1, EwmaForecaster, ExecutionModeConfig, FactDerivationError,
    FactFreshnessError, FactResourceBinding, FactSnapshot, FactSnapshotFingerprint, FactSourceId,
    FixedModelExecutionTransitionPolicyV1, Forecast, ForecastController, ForecastCycleAttempt,
    ForecastCycleFailure, ForecastCycleResult, ForecastRunAttempt, ForecastRunFailure,
    ForecastRunResult, ForecastRuntime, ForecastStatus, Forecaster, ForecasterSelection,
    GuardConfigError, GuardConfigV1, GuardExprConfigV1, GuardPlannerTarget, GuardPreplannerError,
    GuardRuleConfigV1, GuardScopeConfigV1, GuardedPlanningDecision, GuardedPlanningOutcomeTrace,
    GuardedPlanningTrace, HostMemoryObserver, InvariantCheck, InvariantPrecheckEntry,
    InvariantPrecheckError, InvariantPrecheckReport, InvariantPrecheckStatus,
    InvariantPrecheckTraceSummary, LinuxHwmonPowerObserver, LinuxThermalMarginObserver,
    LoopStopReason, LoweredGuardConfigV1, ModelExecutionActuationEvidenceV1,
    ModelExecutionControllerContractsV1, ModelExecutionControllerContractsWireV1,
    ModelExecutionControllerV1, ModelExecutionCycleEvidenceV1, ModelExecutionForecastEvidenceV1,
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
    PredicateEvaluator, PredicateKeyConfigV1, PredicateTraceEntry,
    PseudoBooleanConstraintTermTrace, PseudoBooleanConstraintTrace, RamBudgetObserver,
    RegisteredResource, RejectedCandidateTrace, RepresentationPrecisionCandidateV1, ResourceConfig,
    ResourceRegistry, RollbackRecord, RunResult, Runtime, RuntimeClock, RuntimeConfig,
    RuntimeError, RuntimeEvent, RuntimeEventKind, RuntimeEventSink, RuntimeMode,
    RuntimeTimingObserver, SystemClock, ThresholdComparison, ThresholdComparisonConfigV1,
    TransactionalActuator, TransactionalConcurrency, TransactionalModelExecution, TransactionalRam,
    TransitionGuardedModelExecutionBackendError, TransitionGuardedModelExecutionBackendV1,
    TransitionMechanismConfigV1, UnknownCandidateTrace, UnknownConstraintPredicateTrace,
    ValidatedPlan, VerificationResult, CONSTRAINED_DECISION_TRACE_SCHEMA_V1,
    DECISION_TRACE_SCHEMA_V1, ENERGY_RATE_SOURCE_UNIT, GUARD_CONFIG_SCHEMA_V1,
    MAX_DECISION_TRACE_BYTES, MAX_FACTS_PER_SNAPSHOT, MAX_FACT_SOURCE_ID_BYTES,
    MAX_GUARD_CONFIG_BYTES, MAX_GUARD_CONFIG_EXPR_NODES, MAX_GUARD_CONFIG_GUARDS,
    MAX_GUARD_CONFIG_TERM_BYTES, MAX_OPERATOR_CONFIG_BYTES, MAX_OPERATOR_CONFIG_JSON_DEPTH,
    MAX_REPRESENTATION_PRECISION_CANDIDATES, MODEL_EXECUTION_CONTROLLER_CONTRACTS_MEDIA_TYPE_V1,
    MODEL_EXECUTION_CONTROLLER_CONTRACTS_V1, MODEL_EXECUTION_CYCLE_EVIDENCE_MEDIA_TYPE_V1,
    MODEL_EXECUTION_CYCLE_EVIDENCE_V1, OPERATOR_CONFIG_VERSION,
    REPRESENTATION_PRECISION_FLOOR_PREDICATE_NAME, REPRESENTATION_PRECISION_FLOOR_SIGNAL_NAME,
    REPRESENTATION_PRECISION_MAX_AGE, REPRESENTATION_PRECISION_PREDICATE_NAMESPACE,
    REPRESENTATION_PRECISION_SOURCE_UNIT, THERMAL_MARGIN_SOURCE_UNIT,
};

/// Operational runtime surface for users that prefer an explicit namespace.
pub mod runtime {
    pub use elastic_runtime::*;
}

/// Reference in-process adapters, planners, and reviewed ecosystem boundaries.
pub mod adapters {
    pub use elastic_adapters::*;
    pub use elastic_runtime::{
        model_execution_envelope_predicate_key, BooleanModelExecutionProfileControllerV1,
        BooleanModelExecutionProfileEvidenceV1, BooleanModelExecutionProfileReportV1,
        FixedModelExecutionTransitionPolicyV1, ModelExecutionControllerContractsV1,
        ModelExecutionControllerContractsWireV1, ModelExecutionControllerV1,
        ModelExecutionCycleEvidenceV1, ModelExecutionObserverBundleV1,
        ModelExecutionProfileBackendV1, ModelExecutionResourceObserverV1,
        ModelExecutionResourceTelemetrySampleV1, ModelExecutionResourceTelemetryV1,
        ModelExecutionRunEvidenceAttemptV1, ModelExecutionRunEvidenceFailureV1,
        ModelExecutionRunEvidenceResultV1, ModelExecutionTransitionModeV1,
        ModelExecutionTransitionPolicyV1, TransactionalConcurrency, TransactionalModelExecution,
        TransactionalRam, TransitionGuardedModelExecutionBackendError,
        TransitionGuardedModelExecutionBackendV1, MODEL_EXECUTION_ENVELOPE_PREDICATE_NAME,
        MODEL_EXECUTION_ENVELOPE_PREDICATE_NAMESPACE,
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
        analyze_resource_policy, BoolExpr, BooleanCpuArchitecture, BooleanCpuFeatures,
        BooleanGuard, CanonicalizationError, CompiledGuard, ExactBooleanOracle, ExactKleeneOracle,
        ExactOracleError, ExactOracleLimits, ExactPropertyReport, ExactPseudoBooleanOracle,
        ExactPseudoBooleanOracleError, ExactPseudoBooleanReport, ExactSatisfiabilityReport,
        FactMask, FactSet, GuardBindingError, GuardScope, GuardedResourceSpec,
        InvariantGuardDiagnostic, InvariantGuardStatus, InvariantPredicateBinding,
        KleeneOracleError, KleenePropertyReport, KleeneSatisfiabilityReport, LogicError,
        MultiwordBatchError, MultiwordBatchScreen, MultiwordCompiledGuard, MultiwordFactError,
        MultiwordFactSet, MultiwordFactWord, MultiwordGuardBatch, MultiwordGuardError,
        MultiwordGuardPath, PredicateId, PredicateKey, PredicateRegistry, PredicateRegistryError,
        PseudoBooleanBindingError, PseudoBooleanConstraint, PseudoBooleanConstraintDeclaration,
        PseudoBooleanError, PseudoBooleanRelation, PseudoBooleanScale, ResourcePolicyAnalysis,
        ResourcePolicyAnalysisError, TransitionGuard, TransitionMechanism, TransitionPairAnalysis,
        TransitionPolicyAnalysis, TruthValue, WeightedPredicate, WeightedPredicateKey,
        DEFAULT_EXACT_ORACLE_ASSIGNMENTS, DEFAULT_EXACT_ORACLE_VARIABLES, FAST_PREDICATE_CAPACITY,
        MAX_BOOLEAN_EXPR_DEPTH, MAX_EXACT_ORACLE_ASSIGNMENTS, MAX_EXACT_ORACLE_VARIABLES,
        MAX_MULTIWORD_FACT_PREDICATES, MAX_MULTIWORD_FACT_WORDS, MAX_MULTIWORD_GUARDS_PER_BATCH,
        MAX_PSEUDO_BOOLEAN_TERMS, MAX_PSEUDO_BOOLEAN_UNIT_BYTES, MULTIWORD_FACT_WORD_BITS,
    };
    pub use elastic_eir::{
        evaluate_transition_guards, lower, lower_constrained, lower_guarded,
        prune_transition_candidates, ConstraintLoweringError, EirConstrainedResource, EirDocument,
        EirGuardedResource, EirPseudoBooleanConstraint, EirPseudoBooleanTerm, EirResource,
        Fingerprint, FirstGroundedPlanner, PlanningContext, TransitionPlanner,
        TransitionPruningReport, MAX_EIR_PSEUDO_BOOLEAN_CONSTRAINTS,
    };
    pub use elastic_macros::ElasticResource;
    pub use elastic_runtime::{
        capture_constrained_decision_trace, capture_decision_trace, capture_guarded_planning_trace,
        fact_snapshot_fingerprint, model_execution_envelope_predicate_key,
        planning_context_fingerprint, precheck_plan_invariants, BooleanGuardPlanner,
        BooleanGuardPreplanner, BooleanModelExecutionProfileControllerV1,
        BooleanModelExecutionProfileEvidenceV1, BooleanModelExecutionProfileReportV1,
        BuiltinDimensionConfigV1, BuiltinObservationSignalConfigV1, CadenceConfig,
        CancellationToken, CapabilityPredicate, ConfiguredController, ConfiguredForecaster,
        ConfiguredPlanner, ConfiguredResource, ConfiguredResourceState, ConstrainedDecisionTrace,
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
        InvariantPrecheckTraceSummary, LinuxHwmonPowerObserver, LinuxThermalMarginObserver,
        ModelExecutionControllerContractsV1, ModelExecutionControllerContractsWireV1,
        ModelExecutionControllerV1, ModelExecutionCycleEvidenceV1, ModelExecutionObserverBundleV1,
        ModelExecutionProfileBackendV1, ModelExecutionResourceObserverV1,
        ModelExecutionResourceTelemetrySampleV1, ModelExecutionResourceTelemetryV1,
        ModelExecutionRunEvidenceAttemptV1, ModelExecutionRunEvidenceFailureV1,
        ModelExecutionRunEvidenceResultV1, ModelExecutionTransitionModeV1,
        ModelExecutionTransitionPolicyV1, Observation, ObservationSignalConfigV1, Observer,
        OperatorConfig, PlannerSelection, PlanningContextFingerprint, PredicateConfigV1,
        PredicateEvaluationInput, PredicateEvaluator, PredicateKeyConfigV1,
        PseudoBooleanConstraintTermTrace, PseudoBooleanConstraintTrace, RamBudgetObserver,
        RegisteredResource, ResourceConfig, ResourceRegistry, Runtime, RuntimeConfig, RuntimeError,
        RuntimeMode, ThresholdComparison, ThresholdComparisonConfigV1, TransactionalActuator,
        TransactionalConcurrency, TransactionalModelExecution, TransactionalRam,
        TransitionGuardedModelExecutionBackendError, TransitionGuardedModelExecutionBackendV1,
        TransitionMechanismConfigV1, VerificationResult, CONSTRAINED_DECISION_TRACE_SCHEMA_V1,
        DECISION_TRACE_SCHEMA_V1, ENERGY_RATE_SOURCE_UNIT, GUARD_CONFIG_SCHEMA_V1,
        MODEL_EXECUTION_CONTROLLER_CONTRACTS_V1, MODEL_EXECUTION_CYCLE_EVIDENCE_V1,
        MODEL_EXECUTION_ENVELOPE_PREDICATE_NAME, MODEL_EXECUTION_ENVELOPE_PREDICATE_NAMESPACE,
        OPERATOR_CONFIG_VERSION, THERMAL_MARGIN_SOURCE_UNIT,
    };
    pub use elastic_runtime::{
        EvidenceCommand, EvidenceDiff, EvidenceEnvelope, EvidenceError, EvidenceEvent,
        EvidenceEventKind, EvidenceSchema, EvidenceSummary, EVIDENCE_SCHEMA_V1, MAX_EVIDENCE_BYTES,
    };
}
