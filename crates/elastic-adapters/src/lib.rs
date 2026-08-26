//! Concrete, in-process resource adapters.
//!
//! An adapter is the **trusted boundary** between validated Elastic intent
//! and physical action. These adapters are deliberately portable and
//! dependency-free: they demonstrate the full
//! declaration → plan → freshness gate → action → verify discipline with real
//! (but local) effects — actual allocations for the RAM budget, licensed width
//! for CPU concurrency — without OS-specific discovery, NUMA migration, or
//! accelerator code.
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
//!    invariant — planners and freshness checks cannot override refusals.

#![forbid(unsafe_code)]

pub mod actuation;
pub mod error;
pub mod permits;
pub mod ram;

pub use actuation::{actuate_if_fresh, ActuationGateError};
pub use error::AdapterError;
pub use permits::ConcurrencyPermits;
pub use ram::RamBudget;
