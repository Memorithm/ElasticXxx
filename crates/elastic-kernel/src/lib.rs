//! Generic kernel-realization planning contracts for ElasticXxx.
//!
//! This crate models an executable kernel as a first-class elastic resource:
//! one logical computation ([`elastic_core::LogicalResourceId`]) may have
//! many physical realizations ([`RealizationIdentity`]), and the runtime
//! chooses among them according to capability facts, declared contracts, and
//! objective priorities — while every transition preserves the declared
//! invariants.
//!
//! Responsibility boundaries (mirroring the workspace architecture):
//!
//! - this crate is **generic**: it knows about workgroup limits, subgroup
//!   declarations, feature tri-states, requirements, evidence, objectives,
//!   and honest outcomes. It knows nothing about attention, WGSL, WGPU,
//!   CUDA, or any vendor;
//! - domain adapters (today: FLAT-ATTENTION) translate their concrete kernel
//!   variants into [`KernelCandidate`] records and their device discovery
//!   into [`CapabilitySnapshot`] values;
//! - the semantic core (`elastic-core`) remains untouched by compute-domain
//!   concepts; where core vocabulary fits ([`ObjectiveId`],
//!   [`ContractId`], fingerprints), this crate reuses it instead of
//!   duplicating it.
//!
//! Honesty guarantees:
//!
//! - outcomes are exactly `{selected, no candidate, insufficient evidence,
//!   unsupported}` — see [`plan`] and [`SelectionOutcome`];
//! - unknown capabilities are never treated as present or absent
//!   ([`FeatureSupport::Unknown`], [`RejectionReason::FeatureUnknown`]);
//! - measured facts, static estimates, and unknown evidence are distinct
//!   types ([`Evidence`]); a guessed latency can never pose as a
//!   measurement;
//! - selection decisions are auditable end to end through
//!   [`SelectionRecord`], including per-candidate rejection reasons and
//!   deterministic fingerprints;
//! - realization switching follows its own lifecycle
//!   ([`lifecycle`]) instead of misusing the data-transition taxonomy.

#![forbid(unsafe_code)]

pub mod candidate;
pub mod capability;
pub mod lifecycle;
pub mod planner;
pub mod requirements;

pub use candidate::{
    Evidence, EvidenceTier, EvidenceUnit, KernelCandidate, MeasuredQuantity, ObjectiveEvidence,
    RealizationIdentity, StaticQuantity,
};
pub use capability::{
    BindingLimits, CapabilityError, CapabilitySnapshot, Feature, FeatureSupport, LimitKind,
    SubgroupSupport, WorkgroupLimits,
};
pub use lifecycle::{
    Activated, CommittedRealization, Proposed, RolledBackRealization, StageAttestations,
    StageFailure, StageRejection, Validated, Verified,
};
pub use planner::{
    plan, CandidateRejection, DecisiveEvidence, EvidenceShortfall, RejectedReason,
    SelectionOutcome, SelectionPolicy, SelectionRecord, UnsupportedReason, PLANNER_VERSION,
};
pub use requirements::{
    FeatureRequirement, KernelRequirements, RejectionReason as CapabilityRejectionReason,
    RequirementsError,
};
