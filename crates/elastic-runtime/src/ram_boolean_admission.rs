//! BE14a Boolean eligibility for the existing capacity-admission runtime.
//!
//! This module adds one fail-closed Boolean gate in front of the existing
//! [`CapacityAdmissionControllerV1`]. The gate answers only whether the current
//! RAM observation establishes enough bytes for the request reserve plus one
//! work unit. It does not choose concurrency, validate an adapter mutation, or
//! authorize actuation. When the gate is `True`, the historical capacity
//! controller still performs numeric width selection and the ordinary trusted
//! `OBSERVE -> PLAN -> VALIDATE -> ACT -> VERIFY -> COMMIT / ROLLBACK` path.
//!
//! The stable predicate `elastic.ram::capacity-sufficient` is derived from the
//! built-in `free-capacity` observation signal, interpreted as **bytes** for
//! this contract. Missing, unavailable, stale, structurally unrepresentable, or
//! inexact (> 2^53) byte evidence remains `Unknown` and cannot reach actuation.

use std::time::{Duration, Instant};

use elastic_adapters::ConcurrencyPermits;
use elastic_core::resource::{DimensionId, ObservationSignalId};
use elastic_core::{
    BoolExpr, BooleanGuard, FreshnessSnapshot, GuardFactSource, GuardScope, GuardedResourceSpec,
    ObservationEpoch, PlannerEpoch, PredicateKey, PredicateRegistry, ResourceGeneration,
    TransitionMechanism, TruthValue,
};
use elastic_eir::{lower_guarded, EirGuardedResource};
use serde::{Deserialize, Serialize};

use crate::{
    capture_decision_trace, BooleanGuardPreplanner, CapabilityPredicate,
    CapacityAdmissionControllerV1, CapacityAdmissionReportV1, CapacityAdmissionRequestV1,
    CapacityStateV1, FactResourceBinding, FactSnapshot, FactSourceId, Observation,
    ObservationSnapshot, ObservationSource, ObservationThresholdPredicate,
    PredicateEvaluationInput, PredicateEvaluator, ThresholdComparison,
};

/// Namespace of the stable BE14a RAM-capacity predicate.
pub const RAM_CAPACITY_PREDICATE_NAMESPACE: &str = "elastic.ram";
/// Name of the stable BE14a RAM-capacity predicate.
pub const RAM_CAPACITY_PREDICATE_NAME: &str = "capacity-sufficient";
/// Unit of the numeric source signal used by BE14a.
pub const RAM_CAPACITY_SOURCE_UNIT: &str = "bytes";
/// Largest unsigned integer that is exactly representable by `f64`.
const MAX_EXACT_F64_INTEGER_U64: u64 = 1_u64 << 53;

/// Stable key used by the BE14a RAM-capacity guard.
///
/// Its semantics are: the current fresh RAM observation establishes
/// `available_memory_bytes >= reserve_memory_bytes + memory_bytes_per_trial`.
/// The request carries the exact integer threshold; the durable report retains
/// that request beside the decision trace.
pub fn ram_capacity_predicate_key() -> PredicateKey {
    PredicateKey::new(
        RAM_CAPACITY_PREDICATE_NAMESPACE,
        RAM_CAPACITY_PREDICATE_NAME,
    )
    .expect("static BE14a PredicateKey is valid")
}

/// Durable explanatory Boolean evidence attached to one admission attempt.
///
/// When present, `decision_trace_json` is the bounded strict `DecisionTrace/v1`
/// payload. `None` means immutable plan/environment identity rejected the request
/// before Boolean evaluation. Trace data is explanatory/replay evidence only and
/// cannot authorize physical actuation.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BooleanRamCapacityEvidenceV1 {
    pub schema_version: u16,
    pub predicate_key: String,
    pub source_signal: String,
    pub source_unit: String,
    pub required_memory_bytes: Option<u64>,
    pub truth: String,
    pub decision_trace_json: Option<String>,
}

/// One BE14a Boolean-gated admission result plus the existing runtime report.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BooleanRamCapacityAdmissionReportV1 {
    pub schema_version: u16,
    pub guard: BooleanRamCapacityEvidenceV1,
    pub admission: CapacityAdmissionReportV1,
}

/// RAM-capacity Boolean front-end for the existing admission controller.
///
/// The wrapper owns no scheduler and no second actuator. False/Unknown evidence
/// stops before numeric width selection. True evidence delegates to
/// [`CapacityAdmissionControllerV1::admit`], which remains authoritative for
/// width calculation, trusted validation, physical permit mutation,
/// verification, commit, and rollback.
pub struct BooleanRamCapacityAdmissionControllerV1 {
    inner: CapacityAdmissionControllerV1,
    guarded_resource: EirGuardedResource,
    max_width: u32,
    expected_plan_id: String,
    expected_environment_id: String,
    next_observation_epoch: u64,
    resource_generation: u64,
}

impl BooleanRamCapacityAdmissionControllerV1 {
    /// Construct a Boolean-gated capacity controller over the same declaration
    /// and trusted permit runtime used by the unguarded controller.
    pub fn new(
        id: &str,
        max_width: u32,
        initial_width: u32,
        expected_plan_id: &str,
        expected_environment_id: &str,
    ) -> Result<Self, String> {
        let inner = CapacityAdmissionControllerV1::new(
            id,
            max_width,
            initial_width,
            expected_plan_id,
            expected_environment_id,
        )?;
        let predicate = ram_capacity_predicate_key();
        let registry =
            PredicateRegistry::from_keys([predicate.clone()]).map_err(|error| error.to_string())?;
        let predicate_id = registry
            .id(&predicate)
            .expect("registry contains the BE14a RAM predicate");
        let guard = BooleanGuard::new(
            GuardScope::Transition {
                mechanism: TransitionMechanism::Reinterpret,
                dimension: DimensionId::CONCURRENCY,
            },
            registry,
            BoolExpr::atom(predicate_id),
        )
        .map_err(|error| error.to_string())?;
        let declaration = ConcurrencyPermits::declaration(id).map_err(|error| error.to_string())?;
        let guarded_spec = GuardedResourceSpec::new(declaration, vec![guard])
            .map_err(|error| error.to_string())?;
        let guarded_resource = lower_guarded(&guarded_spec).map_err(|error| error.to_string())?;

        Ok(Self {
            inner,
            guarded_resource,
            max_width,
            expected_plan_id: expected_plan_id.to_owned(),
            expected_environment_id: expected_environment_id.to_owned(),
            next_observation_epoch: 1,
            resource_generation: 1,
        })
    }

    /// Shared permit handle used by the embedding executor after admission.
    pub fn permits(&self) -> crate::TransactionalConcurrency {
        self.inner.permits()
    }

    /// Apply the BE14a Boolean RAM gate before the historical numeric admission
    /// and trusted runtime cycle.
    ///
    /// Structural request errors remain errors. Missing/unavailable/stale RAM
    /// evidence becomes `Unknown`, while fresh measured insufficiency becomes
    /// `False`; neither can invoke numeric admission or actuation.
    pub fn admit(
        &mut self,
        request: CapacityAdmissionRequestV1,
    ) -> Result<BooleanRamCapacityAdmissionReportV1, String> {
        request.validate()?;
        if request.max_concurrency > self.max_width {
            return Err("request exceeds immutable controller maximum".into());
        }
        if request.plan_id != self.expected_plan_id {
            return self.identity_rejection(request, "plan-identity-mismatch");
        }
        if request.observation.environment_id != self.expected_environment_id {
            return self.identity_rejection(request, "environment-identity-mismatch");
        }

        let epoch = self.next_epoch()?;
        let generation = ResourceGeneration::new(self.resource_generation);
        let now = Instant::now();
        let (context, observations, required_memory_bytes) = ram_observation_inputs(&request, now);
        let input = PredicateEvaluationInput::new(&context, &observations, now);
        let predicate_key = ram_capacity_predicate_key();
        let facts = if let Some(required) =
            required_memory_bytes.filter(|value| *value <= MAX_EXACT_F64_INTEGER_U64)
        {
            let evaluator = ObservationThresholdPredicate::new(
                predicate_key.clone(),
                ObservationSignalId::FREE_CAPACITY,
                ThresholdComparison::GreaterOrEqual,
                required as f64,
                Duration::from_millis(request.max_age_milliseconds),
            )
            .map_err(|error| error.to_string())?;
            FactSnapshot::derive(
                FactSourceId::new("elastic-runtime:be14a-ram-capacity")
                    .map_err(|error| error.to_string())?,
                epoch,
                Some(FactResourceBinding::new(
                    self.guarded_resource.resource().identity().clone(),
                    generation,
                )),
                &input,
                &[&evaluator as &dyn PredicateEvaluator],
            )
            .map_err(|error| error.to_string())?
        } else {
            let evaluator = CapabilityPredicate::new(predicate_key.clone(), None);
            FactSnapshot::derive(
                FactSourceId::new("elastic-runtime:be14a-ram-capacity")
                    .map_err(|error| error.to_string())?,
                epoch,
                Some(FactResourceBinding::new(
                    self.guarded_resource.resource().identity().clone(),
                    generation,
                )),
                &input,
                &[&evaluator as &dyn PredicateEvaluator],
            )
            .map_err(|error| error.to_string())?
        };
        let freshness = FreshnessSnapshot::new(PlannerEpoch::new(epoch.get()), epoch)
            .with_resource_generation(
                self.guarded_resource.resource().identity().clone(),
                generation,
            );
        let truth = facts.truth(&predicate_key);
        let pruning = BooleanGuardPreplanner
            .prune(&self.guarded_resource, &facts, &freshness)
            .map_err(|error| error.to_string())?;

        let mut admission = match truth {
            TruthValue::True
                if pruning.contains_eligible(
                    TransitionMechanism::Reinterpret,
                    &DimensionId::CONCURRENCY,
                ) =>
            {
                self.inner.admit(request)?
            }
            TruthValue::False => self.blocked_report(request, "boolean-ram-capacity-false")?,
            TruthValue::Unknown => self.blocked_report(request, "boolean-ram-capacity-unknown")?,
            TruthValue::True => {
                return Err(
                    "BE14a guard was true but the declared transition was not eligible".into(),
                )
            }
        };

        let selected = admission
            .proposed_width
            .filter(|width| *width > 0)
            .and_then(|width| {
                self.guarded_resource
                    .resource()
                    .transitions()
                    .iter()
                    .find(|entry| {
                        entry.transition().mechanism() == TransitionMechanism::Reinterpret
                            && entry.transition().dimension() == &DimensionId::CONCURRENCY
                    })
                    .map(|entry| {
                        elastic_eir::TransitionCandidate::from_admitted(entry)
                            .with_magnitude(u64::from(width))
                    })
            });
        let trace = capture_decision_trace(
            &self.guarded_resource,
            &facts,
            &freshness,
            selected.as_ref(),
        )
        .map_err(|error| error.to_string())?;
        let decision_trace_json = trace.to_bounded_json().map_err(|error| error.to_string())?;

        if admission.committed == Some(true) && admission.final_width != admission.previous_width {
            self.resource_generation = self
                .resource_generation
                .checked_add(1)
                .ok_or_else(|| "BE14a resource generation exhausted".to_owned())?;
        }
        // A Boolean block has no runtime event by construction. Keep the base
        // report explicit rather than fabricating validation/rollback evidence.
        if truth != TruthValue::True {
            admission.verification = None;
            admission.events.clear();
        }

        Ok(BooleanRamCapacityAdmissionReportV1 {
            schema_version: 1,
            guard: BooleanRamCapacityEvidenceV1 {
                schema_version: 1,
                predicate_key: predicate_key.to_string(),
                source_signal: ObservationSignalId::FREE_CAPACITY.as_str().to_owned(),
                source_unit: RAM_CAPACITY_SOURCE_UNIT.to_owned(),
                required_memory_bytes,
                truth: truth_text(truth).to_owned(),
                decision_trace_json: Some(decision_trace_json),
            },
            admission,
        })
    }

    fn next_epoch(&mut self) -> Result<ObservationEpoch, String> {
        let epoch = ObservationEpoch::new(self.next_observation_epoch);
        self.next_observation_epoch = self
            .next_observation_epoch
            .checked_add(1)
            .ok_or_else(|| "BE14a observation epoch exhausted".to_owned())?;
        Ok(epoch)
    }

    fn blocked_report(
        &self,
        request: CapacityAdmissionRequestV1,
        reason: &str,
    ) -> Result<CapacityAdmissionReportV1, String> {
        let width = self
            .inner
            .permits()
            .width()
            .map_err(|error| error.to_string())? as u32;
        Ok(CapacityAdmissionReportV1 {
            schema_version: 1,
            request,
            status: "rejected".into(),
            reason: reason.into(),
            proposed_width: None,
            previous_width: width,
            final_width: width,
            committed: Some(false),
            rolled_back: Some(false),
            verification: None,
            events: Vec::new(),
        })
    }

    fn identity_rejection(
        &self,
        request: CapacityAdmissionRequestV1,
        reason: &str,
    ) -> Result<BooleanRamCapacityAdmissionReportV1, String> {
        let admission = self.blocked_report(request, reason)?;
        Ok(BooleanRamCapacityAdmissionReportV1 {
            schema_version: 1,
            guard: BooleanRamCapacityEvidenceV1 {
                schema_version: 1,
                predicate_key: ram_capacity_predicate_key().to_string(),
                source_signal: ObservationSignalId::FREE_CAPACITY.as_str().to_owned(),
                source_unit: RAM_CAPACITY_SOURCE_UNIT.to_owned(),
                required_memory_bytes: None,
                truth: "not-evaluated".into(),
                decision_trace_json: None,
            },
            admission,
        })
    }
}

fn ram_observation_inputs(
    request: &CapacityAdmissionRequestV1,
    now: Instant,
) -> (
    elastic_eir::PlanningContext,
    ObservationSnapshot,
    Option<u64>,
) {
    let required = request
        .reserve_memory_bytes
        .checked_add(request.memory_bytes_per_trial);
    let mut context = elastic_eir::PlanningContext::new();
    let mut observations = Vec::new();
    if let CapacityStateV1::Available {
        available_memory_bytes,
        ..
    } = request.observation.capacity
    {
        if available_memory_bytes <= MAX_EXACT_F64_INTEGER_U64 {
            if let Some(timestamp) =
                now.checked_sub(Duration::from_millis(request.observation.age_milliseconds))
            {
                let value = available_memory_bytes as f64;
                context = context.observe(ObservationSignalId::FREE_CAPACITY, value);
                observations.push(Observation::from_source(
                    ObservationSource::host(request.observation.sensor.clone()),
                    ObservationSignalId::FREE_CAPACITY,
                    value,
                    timestamp,
                ));
            }
        }
    }
    (
        context,
        ObservationSnapshot::new(now, observations),
        required,
    )
}

const fn truth_text(value: TruthValue) -> &'static str {
    match value {
        TruthValue::True => "true",
        TruthValue::False => "false",
        TruthValue::Unknown => "unknown",
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
            observation: crate::CapacityObservationV1 {
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

    fn guarded(initial_width: u32) -> BooleanRamCapacityAdmissionControllerV1 {
        BooleanRamCapacityAdmissionControllerV1::new(
            "pool",
            4,
            initial_width,
            &"a".repeat(64),
            &"c".repeat(64),
        )
        .unwrap()
    }

    #[test]
    fn true_guard_matches_unguarded_numeric_and_runtime_result() {
        let mut baseline = CapacityAdmissionControllerV1::new(
            "pool-baseline",
            4,
            4,
            &"a".repeat(64),
            &"c".repeat(64),
        )
        .unwrap();
        let mut boolean = guarded(4);
        let expected = baseline.admit(request()).unwrap();
        let actual = boolean.admit(request()).unwrap();
        assert_eq!(actual.guard.truth, "true");
        assert_eq!(actual.guard.required_memory_bytes, Some(200));
        assert_eq!(actual.admission.proposed_width, expected.proposed_width);
        assert_eq!(actual.admission.final_width, expected.final_width);
        assert_eq!(actual.admission.committed, expected.committed);
        assert!(actual.guard.decision_trace_json.is_some());
        let decoded = crate::DecisionTrace::from_bounded_json(
            actual
                .guard
                .decision_trace_json
                .as_deref()
                .unwrap()
                .as_bytes(),
        )
        .unwrap();
        assert!(decoded.selected().is_some());
    }

    #[test]
    fn false_guard_stops_before_numeric_admission_or_actuation() {
        let mut controller = guarded(4);
        let mut req = request();
        req.observation.capacity = CapacityStateV1::Available {
            cpu_slots: 4,
            available_memory_bytes: 199,
        };
        let report = controller.admit(req).unwrap();
        assert_eq!(report.guard.truth, "false");
        assert_eq!(report.admission.reason, "boolean-ram-capacity-false");
        assert_eq!(report.admission.final_width, 4);
        assert_eq!(report.admission.committed, Some(false));
        assert!(report.admission.events.is_empty());
        let decoded = crate::DecisionTrace::from_bounded_json(
            report
                .guard
                .decision_trace_json
                .as_deref()
                .unwrap()
                .as_bytes(),
        )
        .unwrap();
        assert_eq!(decoded.rejected().len(), 1);
        assert!(decoded.selected().is_none());
    }

    #[test]
    fn missing_and_stale_ram_evidence_are_unknown_and_fail_closed() {
        for capacity in [
            CapacityStateV1::Unknown {
                reason: "sensor absent".into(),
            },
            CapacityStateV1::Unavailable {
                reason: "provider unavailable".into(),
            },
        ] {
            let mut controller = guarded(4);
            let mut req = request();
            req.observation.capacity = capacity;
            let report = controller.admit(req).unwrap();
            assert_eq!(report.guard.truth, "unknown");
            assert_eq!(report.admission.reason, "boolean-ram-capacity-unknown");
            assert_eq!(report.admission.final_width, 4);
            assert!(report.admission.events.is_empty());
        }

        let mut controller = guarded(4);
        let mut stale = request();
        stale.observation.age_milliseconds = 101;
        let report = controller.admit(stale).unwrap();
        assert_eq!(report.guard.truth, "unknown");
        assert_eq!(report.admission.final_width, 4);
        assert!(report.admission.events.is_empty());
    }

    #[test]
    fn true_guard_cannot_bypass_trusted_live_holder_validation() {
        let mut controller = guarded(2);
        let permits = controller.permits();
        permits.acquire().unwrap();
        permits.acquire().unwrap();
        let mut req = request();
        req.observation.capacity = CapacityStateV1::Available {
            cpu_slots: 1,
            available_memory_bytes: 350,
        };
        let report = controller.admit(req).unwrap();
        assert_eq!(report.guard.truth, "true");
        assert_ne!(report.admission.committed, Some(true));
        assert_eq!(report.admission.final_width, 2);
        assert!(!report.admission.events.is_empty());
        permits.release().unwrap();
        permits.release().unwrap();
    }

    #[test]
    fn values_outside_exact_numeric_observation_range_are_unknown() {
        let mut controller = guarded(4);
        let mut req = request();
        req.observation.capacity = CapacityStateV1::Available {
            cpu_slots: 4,
            available_memory_bytes: MAX_EXACT_F64_INTEGER_U64 + 1,
        };
        let report = controller.admit(req).unwrap();
        assert_eq!(report.guard.truth, "unknown");
        assert_eq!(report.admission.committed, Some(false));
        assert_eq!(report.admission.final_width, 4);
    }
}
