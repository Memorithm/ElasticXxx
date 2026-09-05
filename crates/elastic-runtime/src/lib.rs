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
//! - forecasters project that evidence without fabricating unavailable facts;
//! - `TransitionPlanner` proposals flow through the planning contract;
//! - adapters provide the trusted boundary for physical effects;
//! - the control loop coordinates one-shot or bounded periodic evaluation.

#![forbid(unsafe_code)]

pub mod actuation;
pub mod attempt;
pub mod cancellation;
pub mod clock;
pub mod commit;
pub mod config;
pub mod configured_controller;
pub mod configured_forecaster;
pub mod control_loop;
pub mod controller;
pub mod error;
pub mod events;
pub mod evidence;
pub mod forecast;
pub mod forecast_runtime;
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
pub mod reference_adapters;
pub mod registry;
pub mod runtime;
pub mod transaction;
pub mod verification;

pub use actuation::Actuation;
pub use attempt::{CycleAttempt, CycleFailure};
pub use cancellation::CancellationToken;
pub use clock::{RuntimeClock, SystemClock};
pub use commit::{CommitRecord, RollbackRecord};
pub use config::{Cadence, PlannerConfig, RuntimeConfig, RuntimeMode};
pub use configured_controller::{
    ConfiguredController, ConfiguredPlanner, ConfiguredResource, ConfiguredResourceState,
};
pub use configured_forecaster::ConfiguredForecaster;
pub use controller::Controller;
pub use error::RuntimeError;
pub use events::{NoopEventSink, RuntimeEvent, RuntimeEventKind, RuntimeEventSink};
pub use evidence::{
    EvidenceCommand, EvidenceDiff, EvidenceEnvelope, EvidenceError, EvidenceEvent,
    EvidenceEventKind, EvidenceSchema, EvidenceSummary, EVIDENCE_SCHEMA_V1, MAX_EVIDENCE_BYTES,
    MAX_EVIDENCE_COLLECTION_ITEMS, MAX_EVIDENCE_DEPTH, MAX_EVIDENCE_DIFF_PATHS, MAX_EVIDENCE_NODES,
    MAX_EVIDENCE_RESOURCE_ID_BYTES, MAX_EVIDENCE_STRING_BYTES,
};
pub use forecast::{CurrentStateForecaster, EwmaForecaster, Forecast, ForecastStatus, Forecaster};
pub use forecast_runtime::{
    ForecastController, ForecastCycleAttempt, ForecastCycleFailure, ForecastCycleResult,
    ForecastRunAttempt, ForecastRunFailure, ForecastRunResult, ForecastRuntime,
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
    ConcurrencyPermitsObserver, HostMemoryObserver, ObserverSet, RamBudgetObserver,
    RuntimeTimingObserver,
};
pub use operator_config::{
    CadenceConfig, ControllerConfig, ExecutionModeConfig, ForecasterSelection, OperatorConfig,
    PlannerSelection, ResourceConfig, OPERATOR_CONFIG_VERSION,
};
pub use plan::{InvariantCheck, Plan, ValidatedPlan};
pub use reference_adapters::{TransactionalConcurrency, TransactionalRam};
pub use registry::{RegisteredResource, ResourceRegistry};
pub use runtime::{CycleResult, LoopStopReason, RunResult, Runtime};
pub use transaction::TransactionalActuator;
pub use verification::VerificationResult;
