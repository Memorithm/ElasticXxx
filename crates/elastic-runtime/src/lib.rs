//! Operational runtime layer for ElasticXxx.
//!
//! This crate provides the observe-forecast-plan-actuate control loop and
//! generic observer/forecaster model for Elastic resources.
//!
//! # Design
//!
//! The runtime wires together:
//! - `ResourceSpec` describes what may change and what must be preserved;
//! - EIR lowering produces a validated, fingerprinted IR node;
//! - observers produce explicit telemetry evidence;
//! - Boolean fact derivation maps explicit telemetry/capability evidence into
//!   fail-closed three-valued predicate snapshots;
//! - Boolean preplanning rejects stale facts and prunes ineligible candidates
//!   before numeric planning;
//! - invariant Boolean prechecks may reject/defer work early but never replace
//!   trusted invariant validation;
//! - decision traces bind guarded EIR, exact fact identities, and candidate
//!   outcomes for bounded explanation/replay evidence;
//! - forecasters project evidence without fabricating unavailable facts;
//! - `TransitionPlanner` proposals flow through the planning contract;
//! - adapters provide the trusted boundary for physical effects;
//! - the control loop coordinates one-shot or bounded periodic evaluation.

#![forbid(unsafe_code)]

pub mod actuation;
pub mod attempt;
pub mod batch_device_boolean_admission;
pub mod batch_device_transaction;
pub mod cancellation;
pub mod capacity_admission;
pub mod clock;
pub mod commit;
pub mod composite_plan;
pub mod composite_prepare;
pub mod composite_transaction;
pub mod concurrency_boolean_admission;
pub mod config;
pub mod configured_controller;
pub mod configured_forecaster;
pub mod control_loop;
pub mod controller;
pub mod decision_trace;
pub mod error;
pub mod events;
pub mod evidence;
pub mod fact_derivation;
pub mod forecast;
pub mod forecast_runtime;
pub mod forge_search_bridge;
pub mod guard_config;
pub mod guard_planner;
pub mod guard_preplanner;
pub mod invariant_precheck;
pub mod model_execution_boolean_admission;
pub mod model_execution_boolean_controller;
pub mod model_execution_contracts;
pub mod model_execution_controller;
pub mod model_execution_evidence;
pub mod model_execution_observer;
pub mod model_execution_transaction;
pub mod model_execution_transition;
pub mod observation;
pub mod observers;
pub mod operator_config;
pub mod plan;
pub mod ram_boolean_admission;
pub mod reference_adapters;
pub mod registry;
pub mod representation_precision_boolean_admission;
pub mod runtime;
pub mod thermal_energy_boolean_admission;
pub mod thermal_energy_transaction;
pub mod transaction;
pub mod verification;

pub use actuation::Actuation;
pub use attempt::{CycleAttempt, CycleFailure};
pub use batch_device_boolean_admission::{
    batch_device_capacity_predicate_key, BatchDeviceCandidateV1, BatchDeviceCapacitySampleV1,
    BatchDeviceCapacitySnapshotV1, BooleanBatchDeviceCandidateEvidenceV1,
    BooleanBatchDeviceDecisionTraceV1, BooleanBatchDeviceOutcomeV1, BooleanBatchDevicePreplannerV1,
    BooleanBatchDeviceReportV1, BATCH_DEVICE_CAPACITY_SOURCE_UNIT, BATCH_DEVICE_MAX_AGE,
    BATCH_DEVICE_PREDICATE_NAMESPACE, BOOLEAN_BATCH_DEVICE_DECISION_TRACE_SCHEMA_V1,
    MAX_BATCH_DEVICE_CANDIDATES, MAX_BATCH_DEVICE_SAMPLES, MAX_BOOLEAN_BATCH_DEVICE_TRACE_BYTES,
};
pub use batch_device_transaction::{
    execute_guarded_batch_device_transaction, execute_unguarded_batch_device_transaction,
    BatchDevicePlacementBackendV1, BatchDeviceTransactionBlockV1, BatchDeviceTransactionFailureV1,
    BatchDeviceTransactionStageV1, CommittedBatchDeviceSelectionV1,
    GuardedBatchDeviceTransactionOutcomeV1,
};
pub use cancellation::CancellationToken;
pub use capacity_admission::{
    CapacityAdmissionControllerV1, CapacityAdmissionReportV1, CapacityAdmissionRequestV1,
    CapacityObservationV1, CapacityStateV1,
};
pub use clock::{RuntimeClock, SystemClock};
pub use commit::{CommitRecord, RollbackRecord};
pub use composite_plan::{
    CompositePlanEnvelope, CompositePlanError, COMPOSITE_PLAN_ENVELOPE_SCHEMA_V1,
    MAX_COMPOSITE_SUBPLANS,
};
pub use composite_prepare::{
    abort_composite_prepare, prepare_composite_plan, retry_composite_prepare_cleanup,
    CompositePreActState, CompositePrepareBackend, CompositePrepareCleanupFailure,
    CompositePrepareCleanupOperation, CompositePrepareFailure, CompositePrepareRecoveryEntry,
    CompositePrepareRecoveryEnvelope, CompositePrepareStage, CompositePreparedEnvelope,
    CompositePreparedSubplan, COMPOSITE_PREPARE_SCHEMA_V1,
};
pub use composite_transaction::{
    execute_composite_transaction, retry_composite_commit_cleanup,
    retry_composite_transaction_recovery, CompositeCommitCleanupEntry,
    CompositeCommitCleanupEnvelope, CompositeCommitReport, CompositeFailureDisposition,
    CompositeRecoveryFailure, CompositeResourceCommit, CompositeTransactionBackend,
    CompositeTransactionFailure, CompositeTransactionRecoveryAction,
    CompositeTransactionRecoveryEntry, CompositeTransactionRecoveryEnvelope,
    CompositeTransactionStage, COMPOSITE_TRANSACTION_SCHEMA_V1,
};
pub use concurrency_boolean_admission::{
    concurrency_headroom_predicate_key, BooleanConcurrencyEvidenceV1,
    BooleanConcurrencyResizeControllerV1, BooleanConcurrencyResizeReportV1,
    ConcurrencyResizeReportV1, CONCURRENCY_HEADROOM_MAX_AGE, CONCURRENCY_HEADROOM_PREDICATE_NAME,
    CONCURRENCY_HEADROOM_PREDICATE_NAMESPACE, CONCURRENCY_HEADROOM_SOURCE_UNIT,
};
pub use config::{Cadence, PlannerConfig, RuntimeConfig, RuntimeMode};
pub use configured_controller::{
    ConfiguredController, ConfiguredPlanner, ConfiguredPlanningView, ConfiguredResource,
    ConfiguredResourceState,
};
pub use configured_forecaster::ConfiguredForecaster;
pub use controller::Controller;
pub use decision_trace::{
    capture_constrained_decision_trace, capture_decision_trace, capture_guarded_planning_trace,
    fact_snapshot_fingerprint, CandidateDecisionTrace, ConstrainedDecisionTrace,
    DecisionReplayError, DecisionStopReason, DecisionTrace, DecisionTraceChange,
    DecisionTraceChangeKind, DecisionTraceDiff, DecisionTraceError, FactSnapshotFingerprint,
    GuardedPlanningOutcomeTrace, GuardedPlanningTrace, InvariantPrecheckTraceSummary,
    PredicateTraceEntry, PseudoBooleanConstraintTermTrace, PseudoBooleanConstraintTrace,
    RejectedCandidateTrace, UnknownCandidateTrace, UnknownConstraintPredicateTrace,
    CONSTRAINED_DECISION_TRACE_SCHEMA_V1, DECISION_TRACE_SCHEMA_V1, MAX_DECISION_TRACE_BYTES,
};
pub use error::RuntimeError;
pub use events::{NoopEventSink, RuntimeEvent, RuntimeEventKind, RuntimeEventSink};
pub use evidence::{
    EvidenceCommand, EvidenceDiff, EvidenceEnvelope, EvidenceError, EvidenceEvent,
    EvidenceEventKind, EvidenceSchema, EvidenceSummary, EVIDENCE_SCHEMA_V1, MAX_EVIDENCE_BYTES,
    MAX_EVIDENCE_COLLECTION_ITEMS, MAX_EVIDENCE_DEPTH, MAX_EVIDENCE_DIFF_PATHS, MAX_EVIDENCE_NODES,
    MAX_EVIDENCE_RESOURCE_ID_BYTES, MAX_EVIDENCE_STRING_BYTES,
};
pub use fact_derivation::{
    observation_source_for, CapabilityPredicate, FactDerivationError, FactFreshnessError,
    FactResourceBinding, FactSnapshot, FactSourceId, ObservationFreshnessPredicate,
    ObservationPresencePredicate, ObservationThresholdPredicate, PredicateEvaluationInput,
    PredicateEvaluator, ThresholdComparison, MAX_FACTS_PER_SNAPSHOT, MAX_FACT_SOURCE_ID_BYTES,
};
pub use forecast::{CurrentStateForecaster, EwmaForecaster, Forecast, ForecastStatus, Forecaster};
pub use forecast_runtime::{
    ForecastController, ForecastCycleAttempt, ForecastCycleFailure, ForecastCycleResult,
    ForecastRunAttempt, ForecastRunFailure, ForecastRunResult, ForecastRuntime,
};
pub use forge_search_bridge::{
    ForgeCandidateSourceV1, ForgePseudoBooleanConstraintV1, ForgePseudoBooleanRelationV1,
    ForgeRevalidatedPolicyV1, ForgeSearchCandidateError, ForgeSearchCandidateV1,
    ForgeWeightedPredicateV1, FORGE_SEARCH_CANDIDATE_SCHEMA_V1,
    FORGE_SEARCH_PRODUCER_REPOSITORY_V1, MAX_FORGE_SEARCH_CANDIDATE_BYTES,
    MAX_FORGE_SEARCH_CANDIDATE_JSON_DEPTH,
};
pub use guard_config::{
    BuiltinDimensionConfigV1, BuiltinObservationSignalConfigV1, ConfiguredThresholdPredicateV1,
    DimensionConfigV1, GuardConfigError, GuardConfigV1, GuardExprConfigV1, GuardRuleConfigV1,
    GuardScopeConfigV1, LoweredGuardConfigV1, ObservationSignalConfigV1, PredicateConfigV1,
    PredicateKeyConfigV1, ThresholdComparisonConfigV1, TransitionMechanismConfigV1,
    GUARD_CONFIG_SCHEMA_V1, MAX_GUARD_CONFIG_BYTES, MAX_GUARD_CONFIG_EXPR_NODES,
    MAX_GUARD_CONFIG_GUARDS, MAX_GUARD_CONFIG_TERM_BYTES,
};
pub use guard_planner::{
    planning_context_fingerprint, BooleanGuardPlanner, GuardPlannerTarget, GuardedPlanningDecision,
    PlanningContextFingerprint,
};
pub use guard_preplanner::{BooleanGuardPreplanner, GuardPreplannerError};
pub use invariant_precheck::{
    precheck_plan_invariants, InvariantPrecheckEntry, InvariantPrecheckError,
    InvariantPrecheckReport, InvariantPrecheckStatus,
};
pub use model_execution_boolean_admission::{
    model_execution_rule_free_capacity_predicate_key,
    model_execution_rule_utilization_predicate_key, BooleanModelExecutionPreplannerV1,
    BooleanModelExecutionRuleEvidenceV1, BooleanModelExecutionScreenOutcomeV1,
    BooleanModelExecutionScreenReportV1, MODEL_EXECUTION_BOOLEAN_MAX_AGE,
    MODEL_EXECUTION_FREE_CAPACITY_SOURCE_UNIT, MODEL_EXECUTION_RULE_PREDICATE_NAMESPACE,
    MODEL_EXECUTION_UTILIZATION_SOURCE_UNIT, MODEL_EXECUTION_UTILIZATION_THRESHOLD_UNIT,
};
pub use model_execution_boolean_controller::{
    model_execution_envelope_predicate_key, BooleanModelExecutionProfileControllerV1,
    BooleanModelExecutionProfileEvidenceV1, BooleanModelExecutionProfileReportV1,
    MODEL_EXECUTION_ENVELOPE_PREDICATE_NAME, MODEL_EXECUTION_ENVELOPE_PREDICATE_NAMESPACE,
};
pub use model_execution_contracts::{
    ModelExecutionControllerContractsV1, ModelExecutionControllerContractsWireV1,
    MODEL_EXECUTION_CONTROLLER_CONTRACTS_MEDIA_TYPE_V1, MODEL_EXECUTION_CONTROLLER_CONTRACTS_V1,
};
pub use model_execution_controller::{
    ModelExecutionControllerV1, ModelExecutionObserverBundleV1, ModelExecutionRunEvidenceAttemptV1,
    ModelExecutionRunEvidenceFailureV1, ModelExecutionRunEvidenceResultV1,
};
pub use model_execution_evidence::{
    ModelExecutionActuationEvidenceV1, ModelExecutionCycleEvidenceV1,
    ModelExecutionForecastEvidenceV1, ModelExecutionForecastStatusEvidenceV1,
    ModelExecutionInvariantEvidenceV1, ModelExecutionObservationEvidenceV1,
    ModelExecutionObservationSnapshotEvidenceV1, ModelExecutionPlanEvidenceV1,
    ModelExecutionPlanOutcomeEvidenceV1, ModelExecutionRollbackEvidenceV1,
    ModelExecutionSelectedProfileEvidenceV1, ModelExecutionSignalEvidenceV1,
    ModelExecutionVerificationEvidenceV1, MODEL_EXECUTION_CYCLE_EVIDENCE_MEDIA_TYPE_V1,
    MODEL_EXECUTION_CYCLE_EVIDENCE_V1,
};
pub use model_execution_observer::{
    ModelExecutionResourceObserverV1, ModelExecutionResourceTelemetrySampleV1,
    ModelExecutionResourceTelemetryV1,
};
pub use model_execution_transaction::{
    ModelExecutionProfileBackendV1, TransactionalModelExecution,
};
pub use model_execution_transition::{
    FixedModelExecutionTransitionPolicyV1, ModelExecutionTransitionModeV1,
    ModelExecutionTransitionPolicyV1, TransitionGuardedModelExecutionBackendError,
    TransitionGuardedModelExecutionBackendV1,
};
pub use observation::{Observation, ObservationSnapshot, ObservationSource, Observer};
pub use observers::{
    active_permits_signal, concurrency_capacity_signal, concurrency_width_signal,
    host_memory_available_bytes_signal, host_memory_total_bytes_signal,
    host_memory_used_bytes_signal, host_memory_utilization_signal, ram_configured_max_bytes_signal,
    ram_configured_min_bytes_signal, ram_in_use_bytes_signal, runtime_uptime_seconds_signal,
    ConcurrencyPermitsObserver, HostMemoryObserver, LinuxHwmonPowerObserver,
    LinuxThermalMarginObserver, ObserverSet, RamBudgetObserver, RuntimeTimingObserver,
    ENERGY_RATE_SOURCE_UNIT, THERMAL_MARGIN_SOURCE_UNIT,
};
pub use operator_config::{
    CadenceConfig, ControllerConfig, ExecutionModeConfig, ForecasterSelection, OperatorConfig,
    PlannerSelection, ResourceConfig, MAX_OPERATOR_CONFIG_BYTES, MAX_OPERATOR_CONFIG_JSON_DEPTH,
    OPERATOR_CONFIG_VERSION,
};
pub use plan::{InvariantCheck, Plan, ValidatedPlan};
pub use ram_boolean_admission::{
    ram_capacity_predicate_key, BooleanRamCapacityAdmissionControllerV1,
    BooleanRamCapacityAdmissionReportV1, BooleanRamCapacityAdmissionReportV2,
    BooleanRamCapacityEvidenceV1, BooleanRamCapacityEvidenceV2, RAM_CAPACITY_PREDICATE_NAME,
    RAM_CAPACITY_PREDICATE_NAMESPACE, RAM_CAPACITY_SOURCE_UNIT,
};
pub use reference_adapters::{TransactionalConcurrency, TransactionalRam};
pub use registry::{RegisteredResource, ResourceRegistry};
pub use representation_precision_boolean_admission::{
    representation_precision_floor_predicate_key, representation_precision_floor_signal,
    BooleanRepresentationPrecisionCandidateEvidenceV1,
    BooleanRepresentationPrecisionCandidateTraceV1, BooleanRepresentationPrecisionOutcomeV1,
    BooleanRepresentationPrecisionPreplannerV1, BooleanRepresentationPrecisionReportV1,
    BooleanRepresentationPrecisionReportV2, RepresentationPrecisionCandidateV1,
    MAX_REPRESENTATION_PRECISION_CANDIDATES, REPRESENTATION_PRECISION_FLOOR_PREDICATE_NAME,
    REPRESENTATION_PRECISION_FLOOR_SIGNAL_NAME, REPRESENTATION_PRECISION_MAX_AGE,
    REPRESENTATION_PRECISION_PREDICATE_NAMESPACE, REPRESENTATION_PRECISION_SOURCE_UNIT,
};
pub use runtime::{CycleResult, LoopStopReason, RunResult, Runtime};
pub use thermal_energy_boolean_admission::{
    energy_rate_within_budget_predicate_key, thermal_margin_sufficient_predicate_key,
    BooleanThermalEnergyEvidenceV1, BooleanThermalEnergyPreplannerV1, BooleanThermalEnergyReportV1,
    BooleanThermalEnergyStatusV1, ENERGY_RATE_WITHIN_BUDGET_PREDICATE_NAME, THERMAL_ENERGY_MAX_AGE,
    THERMAL_ENERGY_PREDICATE_NAMESPACE, THERMAL_MARGIN_SUFFICIENT_PREDICATE_NAME,
};
pub use thermal_energy_transaction::{
    execute_guarded_thermal_energy_transaction, execute_unguarded_thermal_energy_transaction,
    CommittedThermalEnergyTransitionV1, GuardedThermalEnergyTransactionOutcomeV1,
    ThermalEnergyTransactionBlockV1, ThermalEnergyTransactionFailureV1,
    ThermalEnergyTransactionStageV1, ThermalEnergyTransitionBackendV1,
};
pub use transaction::TransactionalActuator;
pub use verification::VerificationResult;
