//! Operational runtime layer for ElasticXxx.
//!
//! This crate provides the observe-plan-actuate control loop and generic
//! observer model for Elastic resources.
//!
//! # Design
//!
//! The runtime wires together:
//! - `ResourceSpec` describes what may change and what must be preserved;
//! - EIR lowering produces a validated, fingerprinted IR node;
//! - `TransitionPlanner` proposals flow through the planning contract;
//! - adapters provide the trusted boundary for physical effects;
//! - the control loop coordinates one-shot or bounded periodic evaluation.

#![forbid(unsafe_code)]

pub mod actuation;
pub mod cancellation;
pub mod clock;
pub mod commit;
pub mod config;
pub mod control_loop;
pub mod error;
pub mod events;
pub mod observation;
pub mod observers;
pub mod plan;
pub mod runtime;
pub mod transaction;
pub mod verification;

pub use actuation::Actuation;
pub use cancellation::CancellationToken;
pub use clock::{RuntimeClock, SystemClock};
pub use commit::{CommitRecord, RollbackRecord};
pub use config::{Cadence, PlannerConfig, RuntimeConfig, RuntimeMode};
pub use error::RuntimeError;
pub use events::{NoopEventSink, RuntimeEvent, RuntimeEventKind, RuntimeEventSink};
pub use observation::{Observation, ObservationSnapshot, ObservationSource, Observer};
pub use observers::{
    ConcurrencyPermitsObserver, HostMemoryObserver, ObserverSet, RamBudgetObserver,
    RuntimeTimingObserver, active_permits_signal, concurrency_width_signal,
    host_memory_available_bytes_signal, host_memory_total_bytes_signal,
    host_memory_used_bytes_signal, host_memory_utilization_signal, ram_configured_max_bytes_signal,
    ram_configured_min_bytes_signal, ram_in_use_bytes_signal, runtime_uptime_seconds_signal,
};
pub use plan::{InvariantCheck, Plan, ValidatedPlan};
pub use runtime::{CycleResult, LoopStopReason, RunResult, Runtime};
pub use transaction::TransactionalActuator;
pub use verification::VerificationResult;
