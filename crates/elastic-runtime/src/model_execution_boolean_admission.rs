//! BE14c fail-closed Boolean screening for model-execution envelope rules.
//!
//! This module is deliberately planning-only. It evaluates the existing
//! `FREE_CAPACITY` and `UTILIZATION` telemetry against provider-declared model
//! envelope rules before the existing numeric/profile planner is invoked.
//! `False` prunes a rule, `Unknown` blocks preference resolution whenever that
//! unknown rule could precede a later match, and `True` only admits the existing
//! [`ModelExecutionAdaptivePlannerV1`] to re-resolve the same policy. Boolean
//! evidence is explanatory and never authorizes actuation.

use std::time::{Duration, Instant};

use elastic_adapters::{
    ModelExecutionAdaptivePlannerV1, ModelExecutionEnvelopePolicyV1, ModelExecutionProfileSetV1,
};
use elastic_core::resource::ObservationSignalId;
use elastic_core::{PredicateKey, TruthValue};
use elastic_eir::{EirResource, PlanOutcome, PlanningContext, TransitionPlanner};
use serde::{Deserialize, Serialize};

use crate::ObservationSnapshot;

/// Stable namespace for BE14c rule-threshold predicates.
pub const MODEL_EXECUTION_RULE_PREDICATE_NAMESPACE: &str = "elastic.model-execution";
/// Unit carried by the runtime `UTILIZATION` observation.
pub const MODEL_EXECUTION_UTILIZATION_SOURCE_UNIT: &str = "fraction-0-to-1";
/// Unit used by provider-declared utilization thresholds.
pub const MODEL_EXECUTION_UTILIZATION_THRESHOLD_UNIT: &str = "basis-points";
/// Symbolic unit marker for `FREE_CAPACITY`; the concrete unit comes from policy.
pub const MODEL_EXECUTION_FREE_CAPACITY_SOURCE_UNIT: &str = "policy-capacity-unit";
/// Freshness envelope for live model-execution resource telemetry.
pub const MODEL_EXECUTION_BOOLEAN_MAX_AGE: Duration = Duration::from_secs(1);

const MAX_EXACT_F64_INTEGER: f64 = 9_007_199_254_740_992.0;

/// Stable key for a provider rule's free-capacity threshold.
pub fn model_execution_rule_free_capacity_predicate_key(rule_rank: u32) -> PredicateKey {
    PredicateKey::new(
        MODEL_EXECUTION_RULE_PREDICATE_NAMESPACE,
        format!("rule-{rule_rank}-free-capacity"),
    )
    .expect("numeric BE14c rule rank produces a valid PredicateKey")
}

/// Stable key for a provider rule's utilization threshold.
pub fn model_execution_rule_utilization_predicate_key(rule_rank: u32) -> PredicateKey {
    PredicateKey::new(
        MODEL_EXECUTION_RULE_PREDICATE_NAMESPACE,
        format!("rule-{rule_rank}-utilization"),
    )
    .expect("numeric BE14c rule rank produces a valid PredicateKey")
}

/// Durable explanatory evidence for one provider rule screened by BE14c.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BooleanModelExecutionRuleEvidenceV1 {
    pub rule_id: String,
    pub rule_rank: u32,
    pub free_capacity_predicate_key: String,
    pub utilization_predicate_key: String,
    pub min_free_capacity: u64,
    pub max_utilization_bps: u16,
    pub free_capacity_truth: String,
    pub utilization_truth: String,
    pub combined_truth: String,
}

/// Fail-closed result of Boolean rule screening before numeric/profile ranking.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum BooleanModelExecutionScreenOutcomeV1 {
    /// The first provider-preferred rule with complete `True` evidence.
    Selected { rule_id: String, rule_rank: u32 },
    /// All provider rules were conclusively `False`.
    NoMatchingRule,
    /// At least one potentially preferred rule had unusable evidence.
    InsufficientEvidence {
        blocking_rule_id: String,
        blocking_rule_rank: u32,
    },
}

/// Durable BE14c screening report. It is evidence only and cannot authorize actuation.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BooleanModelExecutionScreenReportV1 {
    pub schema_version: u16,
    pub capacity_unit: String,
    pub free_capacity_source_unit: String,
    pub utilization_source_unit: String,
    pub utilization_threshold_unit: String,
    pub outcome: BooleanModelExecutionScreenOutcomeV1,
    pub rules: Vec<BooleanModelExecutionRuleEvidenceV1>,
}

/// Planning-only BE14c bridge. The existing numeric planner remains authoritative.
#[derive(Clone, Debug)]
pub struct BooleanModelExecutionPreplannerV1 {
    policy: ModelExecutionEnvelopePolicyV1,
    numeric: ModelExecutionAdaptivePlannerV1,
}

impl BooleanModelExecutionPreplannerV1 {
    /// Bind the Boolean preplanner to exactly the same policy/profile contracts
    /// as the existing numeric planner.
    pub fn new(
        policy: ModelExecutionEnvelopePolicyV1,
        profiles: ModelExecutionProfileSetV1,
    ) -> Result<Self, String> {
        let numeric = ModelExecutionAdaptivePlannerV1::new(policy.clone(), profiles)
            .map_err(|error| error.to_string())?;
        Ok(Self { policy, numeric })
    }

    /// Exact backend-owned capacity unit used by `FREE_CAPACITY`.
    #[must_use]
    pub fn capacity_unit(&self) -> &str {
        self.policy.capacity_unit()
    }

    /// Screen provider rules from fresh explicit telemetry without planning or actuation.
    #[must_use]
    pub fn screen(
        &self,
        context: &PlanningContext,
        observations: &ObservationSnapshot,
        now: Instant,
    ) -> BooleanModelExecutionScreenReportV1 {
        let mut rules = Vec::with_capacity(self.policy.rules().len());
        let mut first_unknown = None;
        let mut selected = None;

        for rule in self.policy.rules() {
            let free = evaluate_free_capacity(context, observations, now, rule.min_free_capacity());
            let utilization =
                evaluate_utilization(context, observations, now, rule.max_utilization_bps());
            let combined = kleene_and(free, utilization);
            rules.push(BooleanModelExecutionRuleEvidenceV1 {
                rule_id: rule.rule_id().to_owned(),
                rule_rank: rule.preference_rank(),
                free_capacity_predicate_key: model_execution_rule_free_capacity_predicate_key(
                    rule.preference_rank(),
                )
                .to_string(),
                utilization_predicate_key: model_execution_rule_utilization_predicate_key(
                    rule.preference_rank(),
                )
                .to_string(),
                min_free_capacity: rule.min_free_capacity(),
                max_utilization_bps: rule.max_utilization_bps(),
                free_capacity_truth: truth_text(free).to_owned(),
                utilization_truth: truth_text(utilization).to_owned(),
                combined_truth: truth_text(combined).to_owned(),
            });

            match combined {
                TruthValue::False => {}
                TruthValue::Unknown => {
                    if first_unknown.is_none() {
                        first_unknown = Some((rule.rule_id().to_owned(), rule.preference_rank()));
                    }
                }
                TruthValue::True => {
                    if first_unknown.is_none() {
                        selected = Some((rule.rule_id().to_owned(), rule.preference_rank()));
                    }
                    break;
                }
            }
        }

        let outcome = if let Some((rule_id, rule_rank)) = first_unknown {
            BooleanModelExecutionScreenOutcomeV1::InsufficientEvidence {
                blocking_rule_id: rule_id,
                blocking_rule_rank: rule_rank,
            }
        } else if let Some((rule_id, rule_rank)) = selected {
            BooleanModelExecutionScreenOutcomeV1::Selected { rule_id, rule_rank }
        } else {
            BooleanModelExecutionScreenOutcomeV1::NoMatchingRule
        };

        BooleanModelExecutionScreenReportV1 {
            schema_version: 1,
            capacity_unit: self.policy.capacity_unit().to_owned(),
            free_capacity_source_unit: MODEL_EXECUTION_FREE_CAPACITY_SOURCE_UNIT.to_owned(),
            utilization_source_unit: MODEL_EXECUTION_UTILIZATION_SOURCE_UNIT.to_owned(),
            utilization_threshold_unit: MODEL_EXECUTION_UTILIZATION_THRESHOLD_UNIT.to_owned(),
            outcome,
            rules,
        }
    }

    /// Screen first, then delegate complete `True` evidence to the existing
    /// numeric/profile planner. `Unknown` never reaches numeric ranking.
    pub fn plan_with_evidence(
        &self,
        resource: &EirResource,
        context: &PlanningContext,
        observations: &ObservationSnapshot,
        now: Instant,
    ) -> (BooleanModelExecutionScreenReportV1, PlanOutcome) {
        let report = self.screen(context, observations, now);
        let outcome = match &report.outcome {
            BooleanModelExecutionScreenOutcomeV1::Selected { .. } => {
                self.numeric.propose_transition_with_context(resource, context)
            }
            BooleanModelExecutionScreenOutcomeV1::NoMatchingRule => PlanOutcome::NoCandidate,
            BooleanModelExecutionScreenOutcomeV1::InsufficientEvidence {
                blocking_rule_id, ..
            } => PlanOutcome::InsufficientEvidence {
                detail: format!(
                    "boolean model-execution screening lacks usable evidence for preferred rule {blocking_rule_id:?}"
                ),
            },
        };
        (report, outcome)
    }
}

fn evaluate_free_capacity(
    context: &PlanningContext,
    observations: &ObservationSnapshot,
    now: Instant,
    threshold: u64,
) -> TruthValue {
    let Some(value) = fresh_value(
        context,
        observations,
        now,
        ObservationSignalId::FREE_CAPACITY,
    ) else {
        return TruthValue::Unknown;
    };
    if !(0.0..=MAX_EXACT_F64_INTEGER).contains(&value) || value.fract() != 0.0 {
        return TruthValue::Unknown;
    }
    let observed = value as u64;
    if observed >= threshold {
        TruthValue::True
    } else {
        TruthValue::False
    }
}

fn evaluate_utilization(
    context: &PlanningContext,
    observations: &ObservationSnapshot,
    now: Instant,
    max_utilization_bps: u16,
) -> TruthValue {
    let Some(value) = fresh_value(context, observations, now, ObservationSignalId::UTILIZATION)
    else {
        return TruthValue::Unknown;
    };
    if !(0.0..=1.0).contains(&value) {
        return TruthValue::Unknown;
    }
    let observed_bps = (value * 10_000.0).round() as u16;
    if observed_bps <= max_utilization_bps {
        TruthValue::True
    } else {
        TruthValue::False
    }
}

fn fresh_value(
    context: &PlanningContext,
    observations: &ObservationSnapshot,
    now: Instant,
    signal: ObservationSignalId,
) -> Option<f64> {
    let observation = observations.get(signal.clone())?;
    if !observation.is_valid() || !observation.value().is_finite() {
        return None;
    }
    let age = now.checked_duration_since(*observation.timestamp())?;
    if age > MODEL_EXECUTION_BOOLEAN_MAX_AGE {
        return None;
    }
    let value = context.get(signal)?;
    value.is_finite().then_some(value)
}

const fn kleene_and(left: TruthValue, right: TruthValue) -> TruthValue {
    match (left, right) {
        (TruthValue::False, _) | (_, TruthValue::False) => TruthValue::False,
        (TruthValue::True, TruthValue::True) => TruthValue::True,
        _ => TruthValue::Unknown,
    }
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
    use std::time::Duration;

    use crate::{Observation, ObservationSource};
    use elastic_adapters::{
        ModelExecutionCapabilitiesV1, ModelExecutionEnvelopeRuleV1,
        ModelExecutionHardwarePlannerV1, ModelExecutionHardwareSelectionV1,
        ModelExecutionProfileEnvelopeV1, ModelExecutionProfileV1,
    };

    fn profiles() -> ModelExecutionProfileSetV1 {
        let capabilities = ModelExecutionCapabilitiesV1::new(
            "reference-backend",
            "model-rev-a",
            64,
            vec![1, 2, 4],
            vec![2_500, 5_000, 10_000],
            vec![2_500, 5_000, 10_000],
        )
        .unwrap();
        ModelExecutionProfileSetV1::new(
            &capabilities,
            vec![
                ModelExecutionProfileV1::new("full", 0, 4, 10_000, 10_000).unwrap(),
                ModelExecutionProfileV1::new("balanced", 10, 2, 5_000, 5_000).unwrap(),
                ModelExecutionProfileV1::new("minimal", 20, 1, 2_500, 2_500).unwrap(),
            ],
        )
        .unwrap()
    }

    fn policy(profiles: &ModelExecutionProfileSetV1) -> ModelExecutionEnvelopePolicyV1 {
        ModelExecutionEnvelopePolicyV1::new(
            profiles,
            "bytes",
            vec![
                ModelExecutionEnvelopeRuleV1::new(
                    "rich",
                    0,
                    8_000,
                    7_000,
                    ModelExecutionProfileEnvelopeV1::new(4, 10_000, 10_000).unwrap(),
                )
                .unwrap(),
                ModelExecutionEnvelopeRuleV1::new(
                    "balanced",
                    10,
                    2_000,
                    9_000,
                    ModelExecutionProfileEnvelopeV1::new(2, 5_000, 5_000).unwrap(),
                )
                .unwrap(),
                ModelExecutionEnvelopeRuleV1::new(
                    "survival",
                    20,
                    0,
                    10_000,
                    ModelExecutionProfileEnvelopeV1::new(1, 2_500, 2_500).unwrap(),
                )
                .unwrap(),
            ],
        )
        .unwrap()
    }

    fn evidence(
        now: Instant,
        free: f64,
        utilization: f64,
    ) -> (PlanningContext, ObservationSnapshot) {
        let context = PlanningContext::new()
            .observe(ObservationSignalId::FREE_CAPACITY, free)
            .observe(ObservationSignalId::UTILIZATION, utilization);
        let observations = ObservationSnapshot::new(
            now,
            vec![
                Observation::from_source(
                    ObservationSource::runtime("be14c-test"),
                    ObservationSignalId::FREE_CAPACITY,
                    free,
                    now,
                ),
                Observation::from_source(
                    ObservationSource::runtime("be14c-test"),
                    ObservationSignalId::UTILIZATION,
                    utilization,
                    now,
                ),
            ],
        );
        (context, observations)
    }

    #[test]
    fn complete_true_evidence_selects_first_matching_provider_rule() {
        let profiles = profiles();
        let preplanner =
            BooleanModelExecutionPreplannerV1::new(policy(&profiles), profiles).unwrap();
        let now = Instant::now();
        let (context, observations) = evidence(now, 9_000.0, 0.60);
        let report = preplanner.screen(&context, &observations, now);
        assert_eq!(
            report.outcome,
            BooleanModelExecutionScreenOutcomeV1::Selected {
                rule_id: "rich".into(),
                rule_rank: 0,
            }
        );
        assert_eq!(report.rules[0].combined_truth, "true");
    }

    #[test]
    fn false_rule_is_pruned_before_later_true_rule() {
        let profiles = profiles();
        let preplanner =
            BooleanModelExecutionPreplannerV1::new(policy(&profiles), profiles).unwrap();
        let now = Instant::now();
        let (context, observations) = evidence(now, 3_000.0, 0.80);
        let report = preplanner.screen(&context, &observations, now);
        assert_eq!(report.rules[0].combined_truth, "false");
        assert_eq!(
            report.outcome,
            BooleanModelExecutionScreenOutcomeV1::Selected {
                rule_id: "balanced".into(),
                rule_rank: 10,
            }
        );
    }

    #[test]
    fn missing_stale_and_invalid_evidence_are_unknown_and_block_ranking() {
        let profiles = profiles();
        let preplanner =
            BooleanModelExecutionPreplannerV1::new(policy(&profiles), profiles).unwrap();
        let now = Instant::now();

        let missing = preplanner.screen(
            &PlanningContext::new(),
            &ObservationSnapshot::new(now, vec![]),
            now,
        );
        assert!(matches!(
            missing.outcome,
            BooleanModelExecutionScreenOutcomeV1::InsufficientEvidence {
                blocking_rule_rank: 0,
                ..
            }
        ));

        let old = now.checked_sub(Duration::from_secs(2)).unwrap();
        let (stale_context, _) = evidence(now, 9_000.0, 0.60);
        let stale = ObservationSnapshot::new(
            now,
            vec![
                Observation::from_source(
                    ObservationSource::runtime("be14c-test"),
                    ObservationSignalId::FREE_CAPACITY,
                    9_000.0,
                    old,
                ),
                Observation::from_source(
                    ObservationSource::runtime("be14c-test"),
                    ObservationSignalId::UTILIZATION,
                    0.60,
                    old,
                ),
            ],
        );
        assert!(matches!(
            preplanner.screen(&stale_context, &stale, now).outcome,
            BooleanModelExecutionScreenOutcomeV1::InsufficientEvidence { .. }
        ));

        let invalid_context = PlanningContext::new()
            .observe(ObservationSignalId::FREE_CAPACITY, 9_000.5)
            .observe(ObservationSignalId::UTILIZATION, 1.5);
        let invalid = ObservationSnapshot::new(
            now,
            vec![
                Observation::from_source(
                    ObservationSource::runtime("be14c-test"),
                    ObservationSignalId::FREE_CAPACITY,
                    9_000.5,
                    now,
                ),
                Observation::from_source(
                    ObservationSource::runtime("be14c-test"),
                    ObservationSignalId::UTILIZATION,
                    1.5,
                    now,
                ),
            ],
        );
        assert!(matches!(
            preplanner.screen(&invalid_context, &invalid, now).outcome,
            BooleanModelExecutionScreenOutcomeV1::InsufficientEvidence { .. }
        ));
    }

    #[test]
    fn unknown_higher_priority_rule_blocks_later_true_rule() {
        let profiles = profiles();
        let preplanner =
            BooleanModelExecutionPreplannerV1::new(policy(&profiles), profiles).unwrap();
        let now = Instant::now();
        let context = PlanningContext::new()
            .observe(ObservationSignalId::FREE_CAPACITY, 9_000.0)
            .observe(ObservationSignalId::UTILIZATION, 0.80);
        let observations = ObservationSnapshot::new(
            now,
            vec![Observation::from_source(
                ObservationSource::runtime("be14c-test"),
                ObservationSignalId::FREE_CAPACITY,
                3_000.0,
                now,
            )],
        );
        let report = preplanner.screen(&context, &observations, now);
        assert!(matches!(
            report.outcome,
            BooleanModelExecutionScreenOutcomeV1::InsufficientEvidence {
                blocking_rule_rank: 0,
                ..
            }
        ));
    }

    #[test]
    fn complete_screening_matches_existing_numeric_rule_resolution() {
        let profiles = profiles();
        let policy = policy(&profiles);
        let preplanner =
            BooleanModelExecutionPreplannerV1::new(policy.clone(), profiles.clone()).unwrap();
        for (free, utilization) in [(9_000_u64, 6_000_u16), (3_000, 8_000), (500, 9_500)] {
            let now = Instant::now();
            let utilization_fraction = f64::from(utilization) / 10_000.0;
            let (context, observations) = evidence(now, free as f64, utilization_fraction);
            let report = preplanner.screen(&context, &observations, now);
            let snapshot =
                elastic_adapters::ModelExecutionResourceSnapshotV1::new("bytes", free, utilization)
                    .unwrap();
            let numeric = ModelExecutionHardwarePlannerV1
                .select(&policy, &profiles, &snapshot)
                .unwrap();
            match (report.outcome, numeric) {
                (
                    BooleanModelExecutionScreenOutcomeV1::Selected { rule_id, .. },
                    ModelExecutionHardwareSelectionV1::Selected {
                        rule_id: numeric_id,
                        ..
                    },
                ) => assert_eq!(rule_id, numeric_id),
                (
                    BooleanModelExecutionScreenOutcomeV1::NoMatchingRule,
                    ModelExecutionHardwareSelectionV1::NoMatchingRule,
                ) => {}
                (left, right) => {
                    panic!("boolean/numeric rule resolution diverged: {left:?} vs {right:?}")
                }
            }
        }
    }
}
