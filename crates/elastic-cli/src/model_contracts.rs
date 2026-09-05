//! CLI tooling for building and validating persisted model-execution controller contracts.

use std::error::Error;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use elastic::{
    ModelExecutionCapabilitiesWireV1, ModelExecutionControllerContractsV1,
    ModelExecutionControllerContractsWireV1, ModelExecutionEnvelopePolicyWireV1,
    ModelExecutionProfileSetWireV1,
};
use serde_json::{json, Value};

use crate::evidence::print_json;

type CommandResult = Result<(), Box<dyn Error>>;

/// Build one strict aggregate controller-contract bundle from the historical
/// three validated wire documents and materialize it as a new JSON file.
pub(crate) fn build_contracts(
    capabilities: &Path,
    profiles: &Path,
    policy: &Path,
    output: &Path,
) -> CommandResult {
    let capabilities = fs::read_to_string(capabilities)?;
    let profiles = fs::read_to_string(profiles)?;
    let policy = fs::read_to_string(policy)?;
    let rendered = build_document(&capabilities, &profiles, &policy)?;

    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    file.write_all(rendered.as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

/// Revalidate one aggregate bundle and print a bounded, non-actuating summary.
pub(crate) fn validate_contracts(input: &Path) -> CommandResult {
    let json = fs::read_to_string(input)?;
    print_json(validate_document(&json)?)
}

fn build_document(
    capabilities_json: &str,
    profiles_json: &str,
    policy_json: &str,
) -> Result<String, Box<dyn Error>> {
    let capabilities: ModelExecutionCapabilitiesWireV1 = serde_json::from_str(capabilities_json)?;
    let profiles: ModelExecutionProfileSetWireV1 = serde_json::from_str(profiles_json)?;
    let policy: ModelExecutionEnvelopePolicyWireV1 = serde_json::from_str(policy_json)?;

    let contracts =
        ModelExecutionControllerContractsWireV1::from_wire_parts(capabilities, profiles, policy)
            .into_validated()?;
    Ok(contracts.to_pretty_json()?)
}

fn validate_document(input: &str) -> Result<Value, Box<dyn Error>> {
    let contracts = ModelExecutionControllerContractsV1::from_json(input)?;
    Ok(json!({
        "command": "model-contracts-validate",
        "actuating": false,
        "valid": true,
        "provider_id": contracts.profiles().provider_id(),
        "model_revision": contracts.profiles().model_revision(),
        "capability_fingerprint": contracts.capabilities().fingerprint().to_string(),
        "profile_set_fingerprint": contracts.profiles().fingerprint().to_string(),
        "policy_fingerprint": contracts.policy().fingerprint().to_string(),
        "capacity_unit": contracts.policy().capacity_unit(),
        "profile_count": contracts.profiles().profiles().len(),
        "rule_count": contracts.policy().rules().len(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use elastic::{
        ModelExecutionCapabilitiesV1, ModelExecutionEnvelopePolicyV1, ModelExecutionEnvelopeRuleV1,
        ModelExecutionProfileEnvelopeV1, ModelExecutionProfileSetV1, ModelExecutionProfileV1,
    };

    fn split_documents() -> (String, String, String) {
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
                    0,
                    10_000,
                    ModelExecutionProfileEnvelopeV1::new(2, 5_000, 5_000).unwrap(),
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
    fn build_document_materializes_reusable_valid_bundle() {
        let (capabilities, profiles, policy) = split_documents();
        let built = build_document(&capabilities, &profiles, &policy).unwrap();
        let replayed = ModelExecutionControllerContractsV1::from_json(&built).unwrap();

        assert_eq!(replayed.profiles().provider_id(), "reference-backend");
        assert_eq!(replayed.profiles().profiles().len(), 2);
        assert_eq!(replayed.policy().rules().len(), 2);
    }

    #[test]
    fn validation_summary_reports_bound_identities() {
        let (capabilities, profiles, policy) = split_documents();
        let built = build_document(&capabilities, &profiles, &policy).unwrap();
        let summary = validate_document(&built).unwrap();

        assert_eq!(summary["valid"], true);
        assert_eq!(summary["actuating"], false);
        assert_eq!(summary["provider_id"], "reference-backend");
        assert_eq!(summary["capacity_unit"], "bytes");
        assert_eq!(summary["profile_count"], 2);
        assert_eq!(summary["rule_count"], 2);
    }

    #[test]
    fn mixed_split_identity_fails_before_bundle_is_written() {
        let (capabilities, _, _) = split_documents();
        let foreign_capabilities = ModelExecutionCapabilitiesV1::new(
            "foreign-backend",
            "model-rev-a",
            64,
            vec![1, 2, 4],
            vec![2_500, 5_000, 10_000],
            vec![2_500, 5_000, 10_000],
        )
        .unwrap();
        let foreign_profiles = ModelExecutionProfileSetV1::new(
            &foreign_capabilities,
            vec![ModelExecutionProfileV1::new("full", 0, 4, 10_000, 10_000).unwrap()],
        )
        .unwrap();
        let foreign_policy = ModelExecutionEnvelopePolicyV1::new(
            &foreign_profiles,
            "bytes",
            vec![ModelExecutionEnvelopeRuleV1::new(
                "full",
                0,
                0,
                10_000,
                ModelExecutionProfileEnvelopeV1::new(4, 10_000, 10_000).unwrap(),
            )
            .unwrap()],
        )
        .unwrap();

        let error = build_document(
            &capabilities,
            &serde_json::to_string(&foreign_profiles.to_wire()).unwrap(),
            &serde_json::to_string(&foreign_policy.to_wire()).unwrap(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("provider"));
    }
}
