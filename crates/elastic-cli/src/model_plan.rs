//! Non-actuating CLI frontend for qualified model-execution planning contracts.

use std::error::Error;
use std::fs;
use std::io::{Error as IoError, ErrorKind};
use std::path::Path;

use elastic::{
    ModelExecutionCapabilitiesWireV1, ModelExecutionEnvelopePolicyWireV1,
    ModelExecutionHardwarePlannerV1, ModelExecutionHardwareSelectionV1,
    ModelExecutionProfilePlanV1, ModelExecutionProfileSetV1, ModelExecutionProfileSetWireV1,
    ModelExecutionResourceSnapshotV1,
};
use serde_json::{json, Value};

use crate::evidence::print_json;

type CommandResult = Result<(), Box<dyn Error>>;

/// Inputs for one non-actuating model-execution planning decision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ModelPlanOptions<'a> {
    pub capabilities: &'a Path,
    pub profiles: &'a Path,
    pub policy: &'a Path,
    pub capacity_unit: &'a str,
    pub free_capacity: u64,
    pub utilization_bps: u16,
    pub current_profile_rank: u32,
}

/// Load strict versioned contracts, revalidate their identity chain, and print
/// one deterministic non-actuating model-execution planning result.
pub(crate) fn model_plan(options: ModelPlanOptions<'_>) -> CommandResult {
    let capabilities = fs::read_to_string(options.capabilities)?;
    let profiles = fs::read_to_string(options.profiles)?;
    let policy = fs::read_to_string(options.policy)?;
    let value = plan_documents(
        &capabilities,
        &profiles,
        &policy,
        options.capacity_unit,
        options.free_capacity,
        options.utilization_bps,
        options.current_profile_rank,
    )?;
    print_json(value)
}

fn plan_documents(
    capabilities_json: &str,
    profiles_json: &str,
    policy_json: &str,
    capacity_unit: &str,
    free_capacity: u64,
    utilization_bps: u16,
    current_profile_rank: u32,
) -> Result<Value, Box<dyn Error>> {
    let capabilities_wire: ModelExecutionCapabilitiesWireV1 =
        serde_json::from_str(capabilities_json)?;
    let capabilities = capabilities_wire.into_validated()?;

    let profiles_wire: ModelExecutionProfileSetWireV1 = serde_json::from_str(profiles_json)?;
    let profiles = profiles_wire.into_validated(&capabilities)?;

    let policy_wire: ModelExecutionEnvelopePolicyWireV1 = serde_json::from_str(policy_json)?;
    let policy = policy_wire.into_validated(&profiles)?;

    if profile_by_rank(&profiles, current_profile_rank).is_none() {
        return Err(IoError::new(
            ErrorKind::InvalidInput,
            format!(
                "current model-execution profile rank {current_profile_rank} is not published by the validated profile set"
            ),
        )
        .into());
    }

    let snapshot =
        ModelExecutionResourceSnapshotV1::new(capacity_unit, free_capacity, utilization_bps)?;
    let selection = ModelExecutionHardwarePlannerV1.select(&policy, &profiles, &snapshot)?;

    let base = json!({
        "command": "model-plan",
        "actuating": false,
        "provider_id": profiles.provider_id(),
        "model_revision": profiles.model_revision(),
        "capability_fingerprint": profiles.capability_fingerprint().to_string(),
        "profile_set_fingerprint": profiles.fingerprint().to_string(),
        "policy_fingerprint": policy.fingerprint().to_string(),
        "snapshot": {
            "capacity_unit": snapshot.capacity_unit(),
            "free_capacity": snapshot.free_capacity(),
            "utilization_bps": snapshot.utilization_bps(),
        },
        "current_profile_rank": current_profile_rank,
    });

    match selection {
        ModelExecutionHardwareSelectionV1::Selected {
            rule_id,
            rule_rank,
            plan,
        } => {
            let outcome = if plan.preference_rank() == current_profile_rank {
                "no-change"
            } else {
                "selected"
            };
            Ok(merge_selection(base, outcome, &rule_id, rule_rank, &plan))
        }
        ModelExecutionHardwareSelectionV1::NoMatchingRule => {
            let mut object = base
                .as_object()
                .cloned()
                .expect("model-plan base is an object");
            object.insert("outcome".to_owned(), json!("no-candidate"));
            Ok(Value::Object(object))
        }
        ModelExecutionHardwareSelectionV1::NoFeasibleProfile { rule_id } => Err(IoError::new(
            ErrorKind::InvalidData,
            format!(
                "matched model-execution rule {rule_id:?} produced no feasible correlated profile"
            ),
        )
        .into()),
    }
}

fn merge_selection(
    base: Value,
    outcome: &str,
    rule_id: &str,
    rule_rank: u32,
    plan: &ModelExecutionProfilePlanV1,
) -> Value {
    let resource_plan = plan.resource_plan();
    let mut object = base
        .as_object()
        .cloned()
        .expect("model-plan base is an object");
    object.insert("outcome".to_owned(), json!(outcome));
    object.insert(
        "matched_rule".to_owned(),
        json!({
            "id": rule_id,
            "rank": rule_rank,
        }),
    );
    object.insert(
        "selected_profile".to_owned(),
        json!({
            "id": plan.profile_id(),
            "rank": plan.preference_rank(),
            "active_experts": resource_plan.active_experts(),
            "expert_width_bps": resource_plan.expert_width_bps(),
            "activation_budget_bps": resource_plan.activation_budget_bps(),
        }),
    );
    Value::Object(object)
}

fn profile_by_rank(
    profiles: &ModelExecutionProfileSetV1,
    rank: u32,
) -> Option<&elastic::ModelExecutionProfileV1> {
    profiles
        .profiles()
        .iter()
        .find(|profile| profile.preference_rank() == rank)
}

#[cfg(test)]
mod tests {
    use super::*;
    use elastic::{
        ModelExecutionCapabilitiesV1, ModelExecutionEnvelopePolicyV1, ModelExecutionEnvelopeRuleV1,
        ModelExecutionProfileEnvelopeV1, ModelExecutionProfileSetV1, ModelExecutionProfileV1,
    };

    fn documents() -> (String, String, String) {
        let capabilities = ModelExecutionCapabilitiesV1::new(
            "reference-backend",
            "model-rev-a",
            64,
            vec![1, 2, 4],
            vec![2_500, 5_000, 10_000],
            vec![2_500, 5_000, 10_000],
        )
        .unwrap();
        let profiles = ModelExecutionProfileSetV1::new(
            &capabilities,
            vec![
                ModelExecutionProfileV1::new("full", 0, 4, 10_000, 10_000).unwrap(),
                ModelExecutionProfileV1::new("balanced", 10, 2, 5_000, 5_000).unwrap(),
                ModelExecutionProfileV1::new("minimal", 20, 1, 2_500, 2_500).unwrap(),
            ],
        )
        .unwrap();
        let policy = ModelExecutionEnvelopePolicyV1::new(
            &profiles,
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
        .unwrap();

        (
            serde_json::to_string(&capabilities.to_wire()).unwrap(),
            serde_json::to_string(&profiles.to_wire()).unwrap(),
            serde_json::to_string(&policy.to_wire()).unwrap(),
        )
    }

    #[test]
    fn constrained_snapshot_selects_balanced_profile() {
        let (capabilities, profiles, policy) = documents();
        let value =
            plan_documents(&capabilities, &profiles, &policy, "bytes", 3_000, 8_000, 0).unwrap();

        assert_eq!(value["outcome"], "selected");
        assert_eq!(value["selected_profile"]["id"], "balanced");
        assert_eq!(value["selected_profile"]["rank"], 10);
        assert_eq!(value["selected_profile"]["active_experts"], 2);
    }

    #[test]
    fn selected_current_profile_reports_no_change() {
        let (capabilities, profiles, policy) = documents();
        let value =
            plan_documents(&capabilities, &profiles, &policy, "bytes", 3_000, 8_000, 10).unwrap();

        assert_eq!(value["outcome"], "no-change");
        assert_eq!(value["selected_profile"]["id"], "balanced");
    }

    #[test]
    fn stale_or_unknown_current_profile_is_rejected() {
        let (capabilities, profiles, policy) = documents();
        let error = plan_documents(
            &capabilities,
            &profiles,
            &policy,
            "bytes",
            3_000,
            8_000,
            999,
        )
        .unwrap_err();
        assert!(error.to_string().contains("not published"));
    }

    #[test]
    fn policy_capacity_unit_is_enforced() {
        let (capabilities, profiles, policy) = documents();
        let error =
            plan_documents(&capabilities, &profiles, &policy, "mib", 3_000, 8_000, 0).unwrap_err();
        assert!(error.to_string().contains("capacity unit mismatch"));
    }
}
