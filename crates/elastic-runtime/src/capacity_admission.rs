//! Bounded admission of independent work through the actual permit controller.
//!
//! The caller owns sensors and must provide their measured age and provenance.
//! This module does not probe a host, reserve physical RAM, schedule work, or
//! change workload semantics. A permit width is applied by the existing runtime
//! and must be enforced by the caller's executor using the shared permit handle.

use elastic_adapters::ConcurrencyPermits;
use elastic_core::{resource::DimensionId, TransitionMechanism};
use elastic_eir::{EirResource, PlanOutcome, TransitionCandidate, TransitionPlanner};
use serde::{Deserialize, Serialize};

use crate::{
    CycleAttempt, PlannerConfig, Runtime, RuntimeConfig, RuntimeMode, TransactionalConcurrency,
};

/// Measured capacity state. Unknown capacity must not be converted into zero or
/// an optimistic default. Units are CPU execution slots and bytes.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "kebab-case", deny_unknown_fields)]
pub enum CapacityStateV1 {
    Available {
        cpu_slots: u32,
        available_memory_bytes: u64,
    },
    Unavailable {
        reason: String,
    },
    Unknown {
        reason: String,
    },
}

/// A caller-supplied physical observation and explicit freshness boundary.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CapacityObservationV1 {
    pub observation_id: String,
    pub sensor: String,
    pub environment_id: String,
    pub age_milliseconds: u64,
    pub capacity: CapacityStateV1,
}

/// Static per-work-unit resource envelope; no quality/precision parameters.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CapacityAdmissionRequestV1 {
    pub schema_version: u16,
    pub plan_id: String,
    pub max_concurrency: u32,
    pub memory_bytes_per_trial: u64,
    pub reserve_memory_bytes: u64,
    pub max_age_milliseconds: u64,
    pub observation: CapacityObservationV1,
}

/// Complete bounded decision, including failed attempts and actual final width.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapacityAdmissionReportV1 {
    pub schema_version: u16,
    pub request: CapacityAdmissionRequestV1,
    pub status: String,
    pub reason: String,
    pub proposed_width: Option<u32>,
    pub previous_width: u32,
    pub final_width: u32,
    /// None means the failed runtime returned no authoritative commit state.
    pub committed: Option<bool>,
    /// None means rollback state is unknown after a failed cycle.
    pub rolled_back: Option<bool>,
    pub verification: Option<String>,
    pub events: Vec<String>,
}

fn sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

impl CapacityAdmissionRequestV1 {
    /// Validate shape and units before any permit state is created or mutated.
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1
            || !sha256(&self.plan_id)
            || !sha256(&self.observation.observation_id)
            || !sha256(&self.observation.environment_id)
            || !(1..=256).contains(&self.max_concurrency)
            || self.memory_bytes_per_trial == 0
            || !(1..=60_000).contains(&self.max_age_milliseconds)
            || self.observation.sensor.is_empty()
            || self.observation.sensor.len() > 256
            || self.observation.sensor.chars().any(char::is_control)
        {
            return Err("invalid capacity admission schema, identity, units or bounds".into());
        }
        match &self.observation.capacity {
            CapacityStateV1::Unknown { reason } | CapacityStateV1::Unavailable { reason }
                if reason.is_empty()
                    || reason.len() > 256
                    || reason.chars().any(char::is_control) =>
            {
                return Err("invalid capacity availability reason".into());
            }
            _ => {}
        }
        Ok(())
    }
}

struct TargetWidth(u32);
impl TransitionPlanner for TargetWidth {
    fn propose_transition(&self, resource: &EirResource) -> PlanOutcome {
        match resource.transitions().iter().find(|entry| {
            entry.transition().mechanism() == TransitionMechanism::Reinterpret
                && entry.transition().dimension() == &DimensionId::CONCURRENCY
                && entry.capability_grounded()
        }) {
            Some(entry) => PlanOutcome::Candidate(
                TransitionCandidate::from_admitted(entry).with_magnitude(self.0 as u64),
            ),
            None => PlanOutcome::Unsupported,
        }
    }
}

/// Capacity controller whose clones of the permit handle share real holder state.
pub struct CapacityAdmissionControllerV1 {
    permits: TransactionalConcurrency,
    runtime: Runtime,
    resource: EirResource,
    max_width: u32,
    expected_plan_id: String,
    expected_environment_id: String,
}

impl CapacityAdmissionControllerV1 {
    /// Create a bounded controller; this performs no work and allocates no RAM
    /// budget. Returned errors retain underlying adapter diagnostics.
    /// Expected identities come from the embedding control plane, independently
    /// of the observation. Rebind by constructing a new controller explicitly.
    pub fn new(
        id: &str,
        max_width: u32,
        initial_width: u32,
        expected_plan_id: &str,
        expected_environment_id: &str,
    ) -> Result<Self, String> {
        if !(1..=256).contains(&max_width)
            || !sha256(expected_plan_id)
            || !sha256(expected_environment_id)
        {
            return Err("invalid immutable admission maximum or expected identities".into());
        }
        let declaration = ConcurrencyPermits::new(id, max_width as usize, initial_width as usize)
            .map_err(|e| e.to_string())?;
        let resource = declaration.ir().clone();
        let runtime = Runtime::new(RuntimeConfig {
            resource_spec: declaration.spec().clone(),
            ir_resource: resource.clone(),
            planner_config: PlannerConfig::None,
            mode: RuntimeMode::Apply,
            dry_run: false,
            max_cycles: 1,
            ..RuntimeConfig::default()
        });
        let permits = TransactionalConcurrency::new(id, max_width as usize, initial_width as usize)
            .map_err(|e| e.to_string())?;
        Ok(Self {
            permits,
            runtime,
            resource,
            max_width,
            expected_plan_id: expected_plan_id.into(),
            expected_environment_id: expected_environment_id.into(),
        })
    }

    /// Shared handle for actual admission by the embedding executor. It refuses
    /// acquisition above the committed width and refuses to strand live holders.
    pub fn permits(&self) -> TransactionalConcurrency {
        self.permits.clone()
    }

    /// Decide a width from a fresh declared snapshot and execute the existing
    /// observe/plan/validate/act/verify/commit-or-rollback cycle. Freshness is
    /// checked at entry; a remote consumer must recheck age at dispatch.
    pub fn admit(
        &mut self,
        request: CapacityAdmissionRequestV1,
    ) -> Result<CapacityAdmissionReportV1, String> {
        request.validate()?;
        if request.max_concurrency > self.max_width {
            return Err("request exceeds immutable controller maximum".into());
        }
        let previous = self.permits.width().map_err(|e| e.to_string())? as u32;
        let mut report = CapacityAdmissionReportV1 {
            schema_version: 1,
            request,
            status: "rejected".into(),
            reason: String::new(),
            proposed_width: None,
            previous_width: previous,
            final_width: previous,
            committed: Some(false),
            rolled_back: Some(false),
            verification: None,
            events: Vec::new(),
        };
        let request = &report.request;
        if request.plan_id != self.expected_plan_id {
            report.reason = "plan-identity-mismatch".into();
            return Ok(report);
        }
        if request.observation.environment_id != self.expected_environment_id {
            report.reason = "environment-identity-mismatch".into();
            return Ok(report);
        }
        if request.observation.age_milliseconds > request.max_age_milliseconds {
            report.reason = "stale-observation".into();
            return Ok(report);
        }
        let (cpu, bytes) = match &request.observation.capacity {
            CapacityStateV1::Available {
                cpu_slots,
                available_memory_bytes,
            } => (*cpu_slots, *available_memory_bytes),
            CapacityStateV1::Unknown { .. } => {
                report.reason = "capacity-unknown".into();
                return Ok(report);
            }
            CapacityStateV1::Unavailable { .. } => {
                report.reason = "capacity-unavailable".into();
                return Ok(report);
            }
        };
        let memory_width =
            bytes.saturating_sub(request.reserve_memory_bytes) / request.memory_bytes_per_trial;
        let target = u64::from(request.max_concurrency)
            .min(u64::from(cpu))
            .min(memory_width) as u32;
        report.proposed_width = Some(target);
        if target == 0 {
            report.reason = "insufficient-capacity".into();
            return Ok(report);
        }
        let observer = self.permits.clone();
        match self.runtime.cycle_attempt(
            &self.resource,
            &TargetWidth(target),
            &observer,
            &mut self.permits,
        ) {
            CycleAttempt::Completed(cycle) => {
                report.committed = Some(cycle.commit.is_some());
                report.rolled_back = Some(cycle.rollback.is_some());
                report.verification = cycle.verification.as_ref().map(|v| format!("{v:?}"));
                report.events = cycle
                    .events
                    .iter()
                    .map(|e| format!("{:?}: {}", e.kind, e.details))
                    .collect();
                report.status = if report.committed == Some(true) {
                    "admitted"
                } else {
                    "rejected"
                }
                .into();
                report.reason = if report.committed == Some(true) {
                    "verified-permit-width"
                } else {
                    "runtime-did-not-commit"
                }
                .into();
            }
            CycleAttempt::Failed(failure) => {
                report.committed = None;
                report.rolled_back = None;
                report.reason = format!("runtime-failure: {}", failure.error);
                report.events = failure
                    .events
                    .iter()
                    .map(|e| format!("{:?}: {}", e.kind, e.details))
                    .collect();
            }
        }
        report.final_width = self.permits.width().map_err(|e| e.to_string())? as u32;
        if report.committed == Some(true) && report.final_width != target {
            return Err("committed permit width disagrees with the verified target".into());
        }
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> CapacityAdmissionRequestV1 {
        CapacityAdmissionRequestV1 {
            schema_version: 1,
            plan_id: "a".repeat(64),
            max_concurrency: 4,
            memory_bytes_per_trial: 100,
            reserve_memory_bytes: 100,
            max_age_milliseconds: 100,
            observation: CapacityObservationV1 {
                observation_id: "b".repeat(64),
                environment_id: "c".repeat(64),
                sensor: "qualification-fixture/v1".into(),
                age_milliseconds: 10,
                capacity: CapacityStateV1::Available {
                    cpu_slots: 3,
                    available_memory_bytes: 350,
                },
            },
        }
    }
    #[test]
    fn actual_permits_enforce_reduced_width_and_preserve_live_holders() {
        let mut c =
            CapacityAdmissionControllerV1::new("pool", 4, 4, &"a".repeat(64), &"c".repeat(64))
                .unwrap();
        let report = c.admit(request()).unwrap();
        assert_eq!(report.committed, Some(true));
        assert_eq!(report.final_width, 2);
        let permits = c.permits();
        permits.acquire().unwrap();
        permits.acquire().unwrap();
        assert!(permits.acquire().is_err());
        let mut req = request();
        req.observation.capacity = CapacityStateV1::Available {
            cpu_slots: 1,
            available_memory_bytes: 350,
        };
        let rejected = c.admit(req.clone()).unwrap();
        assert_ne!(rejected.committed, Some(true));
        assert_eq!(rejected.final_width, 2);
        assert!(!rejected.events.is_empty());
        permits.release().unwrap();
        permits.release().unwrap();
        assert_eq!(c.admit(req).unwrap().final_width, 1);
    }
    #[test]
    fn missing_stale_exhausted_and_invalid_capacity_never_actuate() {
        let mut c =
            CapacityAdmissionControllerV1::new("pool", 4, 4, &"a".repeat(64), &"c".repeat(64))
                .unwrap();
        let mut stale = request();
        stale.observation.age_milliseconds = 101;
        assert_eq!(c.admit(stale).unwrap().reason, "stale-observation");
        for (capacity, reason) in [
            (
                CapacityStateV1::Unknown {
                    reason: "sensor absent".into(),
                },
                "capacity-unknown",
            ),
            (
                CapacityStateV1::Unavailable {
                    reason: "no device".into(),
                },
                "capacity-unavailable",
            ),
            (
                CapacityStateV1::Available {
                    cpu_slots: 4,
                    available_memory_bytes: 99,
                },
                "insufficient-capacity",
            ),
        ] {
            let mut req = request();
            req.observation.capacity = capacity;
            let r = c.admit(req).unwrap();
            assert_eq!(r.reason, reason);
            assert_eq!(r.committed, Some(false));
            assert_eq!(r.final_width, 4);
        }
        let mut invalid = request();
        invalid.memory_bytes_per_trial = 0;
        assert!(c.admit(invalid).is_err());
        let mut overflow = request();
        overflow.reserve_memory_bytes = u64::MAX;
        assert_eq!(c.admit(overflow).unwrap().reason, "insufficient-capacity");
    }

    #[test]
    fn another_plan_or_environment_cannot_raise_live_permit_width() {
        let mut c =
            CapacityAdmissionControllerV1::new("pool", 4, 1, &"a".repeat(64), &"c".repeat(64))
                .unwrap();
        for environment in [false, true] {
            let mut req = request();
            if environment {
                req.observation.environment_id = "d".repeat(64);
            } else {
                req.plan_id = "d".repeat(64);
            }
            let report = c.admit(req).unwrap();
            assert_eq!(
                report.reason,
                if environment {
                    "environment-identity-mismatch"
                } else {
                    "plan-identity-mismatch"
                }
            );
            assert_eq!(report.committed, Some(false));
            assert_eq!(report.final_width, 1);
            assert!(report.events.is_empty());
            let permits = c.permits();
            permits.acquire().unwrap();
            assert!(permits.acquire().is_err());
            permits.release().unwrap();
        }
    }
}
