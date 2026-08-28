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
//! - the control loop coordinates one-shot or periodic evaluation.

#![forbid(unsafe_code)]

pub mod actuation;
pub mod commit;
pub mod config;
pub mod control_loop;
pub mod error;
pub mod events;
pub mod observation;
pub mod plan;
pub mod runtime;
pub mod transaction;
pub mod verification;

pub use actuation::Actuation;
pub use commit::{CommitRecord, RollbackRecord};
pub use config::RuntimeConfig;
pub use error::RuntimeError;
pub use events::{RuntimeEvent, RuntimeEventKind};
pub use observation::{Observation, ObservationSnapshot, Observer};
pub use plan::{InvariantCheck, Plan, ValidatedPlan};
pub use runtime::{CycleResult, Runtime};
pub use transaction::TransactionalActuator;
pub use verification::VerificationResult;
