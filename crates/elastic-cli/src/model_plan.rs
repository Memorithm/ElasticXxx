//! Non-actuating CLI frontend for qualified model-execution planning contracts.

use std::error::Error;
use std::fs;
use std::io::{Error as IoError, ErrorKind};
use std::path::Path;

use elastic::{
    ModelExecutionCapabilitiesWireV1, ModelExecutionControllerContractsV1,
    ModelExecutionEnvelopePolicyWireV1, ModelExecutionHardwarePlannerV1,
    ModelExecutionHardwareSelectionV1, ModelExecutionProfilePlanV1, ModelExecutionProfileSetV1,
    ModelExecutionProfileSetWireV1, ModelExecutionResourceSnapshotV1,
};
use serde_json::{json, Value};

use crate::evidence::print_json;

type CommandResult = Result<(), Box<dyn Error>>;

/// Inputs for one non-actuating model-execution planning decision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ModelPlanOptions<'a> {
    /// Preferred aggregate controller-contract bundle.
    pub contracts: Option<&'a Path>,
    /// Historical split capability contract source.
    pub capabilities: Option<&'a Path>,
    /// Historical split correlated profile-set contract source.
    pub profiles: Option<&'a Path>,
    /// Historical split envelope-policy contract source.
    pub policy: Option<&'a Path>,
    pub capacity_unit: &'a str,
    pub free_capacity: u64,
    pub utilization_bps: u16,
    pub current_profile_rank: u32,
}

/// Load strict versioned contracts, revalidate their identity chain, and print
/// one deterministic non-actuating model-execution planning result.
pub(crate) fn model_plan(options: ModelPlanOptions<'_>) -> CommandResult {
    let contracts = load_contracts(&options)?;
    let value = plan_validated_contracts(
        &contracts,
        options.capacity_unit,
        options.free_capacity,
        options.utilization_bps,
        options.current_profile_rank,
    )?;
    print_json(value)
}

fn load_contracts(
    options: &ModelPlanOptions<'_>,
) -> Result<ModelExecutionControllerContractsV1, Box<dyn Error>> {
    match (
        options.contracts,
        options.capabilities,
        options.profiles,
        options.policy,
    ) {
        (Some(path), None, None, None) => {
            let json = fs::read_to_string(path)?;
            Ok(ModelExecutionControllerContractsV1::from_json(&json)?)
        }
        (None, Some(capabilities), Some(profiles), Some(policy)) => {
            let capabilities = fs::read_to_string(capabilities)?;
            let profiles = fs::read_to_string(profiles)?;
            let policy = fs::read_to_string(policy)?;
            validate_split_documents(&capabilities, &profiles, &policy)
        }
        _ => Err(IoError::new(
            ErrorKind::InvalidInput,
            "model-plan requires either --contracts or the complete --capabilities/--profiles/--policy set",
        )
        .into()),
    }
}

fn validate_split_documents(
    capabilities_json: &str,
    profiles_json: &str,
    policy_json: &str,
) -> Result<ModelExecutionControllerContractsV1, Box<dyn Error>> {
    let capabilities_wire: ModelExecutionCapabilitiesWireV1 =
        serde_json::from_str(capabilities_json)?;
    let capabilities = capabilities_wire.into_validated()?;

    let profiles_wire: ModelExecutionProfileSetWireV1 = serde_json::from_str(profiles_json)?;
    let profiles = profiles_wire.into_validated(&capabilities)?;

    let policy_wire: ModelExecutionEnvelopePolicyWireV1 = serde_json::from_str(policy_json)?;
    let policy = policy_wire.into_validated(&profiles)?;

    Ok(ModelExecutionControllerContractsV1::new(profiles, policy)?)
}

fn plan_validated_contracts(
    contracts: &ModelExecutionControllerContractsV1,
    capacity_unit: &str,
    free_capacity: u64,
    utilization_bps: u16,
    current_profile_rank: u32,
) -> Result<Value, Box<dyn Error>> {
    let profiles = contracts.profiles();
    let policy = contracts.policy();

    if profile_by_rank(profiles, current_profile_rank).is_none() {
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
    let selection = ModelExecutionHardwarePlannerV1.select(policy, profiles, &snapshot)?;

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

#[cfg(test)]
fn plan_documents(
    capabilities_json: &str,
    profiles_json: &str,
    policy_json: &str,
    capacity_unit: &str,
    free_capacity: u64,
    utilization_bps: u16,
    current_profile_rank: u32,
) -> Result<Value, Box<dyn Error>> {
    let contracts = validate_split_documents(capabilities_json, profiles_json, policy_json)?;
    plan_validated_contracts(
        &contracts,
        capacity_unit,
        free_capacity,
        utilization_bps,
        current_profile_rank,
    )
}

#[cfg(test)]
fn plan_bundle_document(
    contracts_json: &str,
    capacity_unit: &str,
    free_capacity: u64,
    utilization_bps: u16,
    current_profile_rank: u32,
) -> Result<Value, Box<dyn Error>> {
    let contracts = ModelExecutionControllerContractsV1::from_json(contracts_json)?;
    plan_validated_contracts(
        &contracts,
        capacity_unit,
        free_capacity,
        utilization_bps,
        current_profile_rank,
    )
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

    fn native_contracts() -> ModelExecutionControllerContractsV1 {
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
        ModelExecutionControllerContractsV1::new(profiles, policy).unwrap()
    }

    fn documents() -> (String, String, String) {
        let contracts = native_contracts();
        (
            serde_json::to_string(&contracts.capabilities().to_wire()).unwrap(),
            serde_json::to_string(&contracts.profiles().to_wire()).unwrap(),
            serde_json::to_string(&contracts.policy().to_wire()).unwrap(),
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
    fn aggregate_bundle_produces_same_selection_as_split_contracts() {
        let contracts = native_contracts();
        let bundle = contracts.to_pretty_json().unwrap();
        let bundled = plan_bundle_document(&bundle, "bytes", 3_000, 8_000, 0).unwrap();
        let (capabilities, profiles, policy) = documents();
        let split =
            plan_documents(&capabilities, &profiles, &policy, "bytes", 3_000, 8_000, 0).unwrap();

        assert_eq!(bundled, split);
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

    #[test]
    fn malformed_bundle_fails_before_planning() {
        let error = plan_bundle_document("{}", "bytes", 3_000, 8_000, 0).unwrap_err();
        assert!(error.to_string().contains("controller contracts JSON"));
    }
}
