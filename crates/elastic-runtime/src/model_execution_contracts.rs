//! Persistable, fail-closed contract bundle for model-execution controllers.
//!
//! The individual capabilities, correlated-profile, and envelope-policy wire
//! contracts already own their semantic validation. This module only sequences
//! that validation so a persisted controller contract bundle cannot silently mix
//! declarations from different providers, model revisions, or fingerprints.

use elastic_adapters::{
    ModelExecutionCapabilitiesV1, ModelExecutionCapabilitiesWireV1, ModelExecutionEnvelopePolicyV1,
    ModelExecutionEnvelopePolicyWireV1, ModelExecutionProfileSetV1, ModelExecutionProfileSetWireV1,
};
use serde::{Deserialize, Serialize};

use crate::RuntimeError;

/// Versioned aggregate contract for the declarations required by one adaptive
/// model-execution controller.
pub const MODEL_EXECUTION_CONTROLLER_CONTRACTS_V1: &str =
    "elastic.model-execution.controller-contracts@1.0.0";

/// JSON media type for [`MODEL_EXECUTION_CONTROLLER_CONTRACTS_V1`].
pub const MODEL_EXECUTION_CONTROLLER_CONTRACTS_MEDIA_TYPE_V1: &str =
    "application/vnd.elastic.model-execution-controller-contracts.v1+json";
/// Maximum encoded bytes accepted for one persisted controller-contract bundle.
pub const MAX_MODEL_EXECUTION_CONTROLLER_CONTRACTS_BYTES: usize = 256 * 1024;
/// Maximum raw JSON container nesting accepted before bundle deserialization.
pub const MAX_MODEL_EXECUTION_CONTROLLER_CONTRACTS_JSON_DEPTH: usize = 32;

/// Strict persisted form of one controller contract bundle.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ModelExecutionControllerContractsWireV1 {
    contract: String,
    capabilities: ModelExecutionCapabilitiesWireV1,
    profiles: ModelExecutionProfileSetWireV1,
    policy: ModelExecutionEnvelopePolicyWireV1,
}

impl ModelExecutionControllerContractsWireV1 {
    /// Aggregate already-versioned wire declarations under the controller bundle
    /// contract. Semantic identity is checked only by [`Self::into_validated`].
    #[must_use]
    pub fn from_wire_parts(
        capabilities: ModelExecutionCapabilitiesWireV1,
        profiles: ModelExecutionProfileSetWireV1,
        policy: ModelExecutionEnvelopePolicyWireV1,
    ) -> Self {
        Self {
            contract: MODEL_EXECUTION_CONTROLLER_CONTRACTS_V1.to_owned(),
            capabilities,
            profiles,
            policy,
        }
    }

    /// Revalidate the complete persisted identity chain.
    ///
    /// # Errors
    ///
    /// Fails closed for an unsupported aggregate contract, malformed base
    /// capabilities, stale/mismatched correlated profiles, or stale/mismatched
    /// envelope policy.
    pub fn into_validated(self) -> Result<ModelExecutionControllerContractsV1, RuntimeError> {
        if self.contract != MODEL_EXECUTION_CONTROLLER_CONTRACTS_V1 {
            return Err(RuntimeError::configuration(format!(
                "unsupported model-execution controller contract {:?}; expected {:?}",
                self.contract, MODEL_EXECUTION_CONTROLLER_CONTRACTS_V1
            )));
        }

        let capabilities = self
            .capabilities
            .into_validated()
            .map_err(|error| RuntimeError::configuration(error.to_string()))?;
        let profiles = self
            .profiles
            .into_validated(&capabilities)
            .map_err(|error| RuntimeError::configuration(error.to_string()))?;
        let policy = self
            .policy
            .into_validated(&profiles)
            .map_err(|error| RuntimeError::configuration(error.to_string()))?;

        Ok(ModelExecutionControllerContractsV1 { profiles, policy })
    }
}

/// Fully revalidated declarations required to construct one adaptive model
/// execution controller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelExecutionControllerContractsV1 {
    profiles: ModelExecutionProfileSetV1,
    policy: ModelExecutionEnvelopePolicyV1,
}

impl ModelExecutionControllerContractsV1 {
    /// Bind an already-validated profile set and policy while rechecking their
    /// exact identity through the policy's strict wire contract.
    ///
    /// # Errors
    ///
    /// Returns a configuration error if the policy does not belong to the exact
    /// supplied profile set.
    pub fn new(
        profiles: ModelExecutionProfileSetV1,
        policy: ModelExecutionEnvelopePolicyV1,
    ) -> Result<Self, RuntimeError> {
        let policy = policy
            .to_wire()
            .into_validated(&profiles)
            .map_err(|error| RuntimeError::configuration(error.to_string()))?;
        Ok(Self { profiles, policy })
    }

    /// Parse and fully revalidate a strict JSON bundle through public
    /// preallocation bounds.
    ///
    /// # Errors
    ///
    /// Returns a configuration error for an oversized/deep document, malformed
    /// JSON, unknown fields, an unsupported bundle contract, or any nested
    /// semantic identity failure.
    pub fn from_bounded_json(input: &[u8]) -> Result<Self, RuntimeError> {
        validate_json_preallocation_bounds(input)?;
        let wire: ModelExecutionControllerContractsWireV1 =
            serde_json::from_slice(input).map_err(|error| {
                RuntimeError::configuration(format!(
                    "invalid model-execution controller contracts JSON: {error}"
                ))
            })?;
        wire.into_validated()
    }

    /// Backward-compatible string entry point routed through the same bounded
    /// decoder as untrusted byte input.
    pub fn from_json(input: &str) -> Result<Self, RuntimeError> {
        Self::from_bounded_json(input.as_bytes())
    }

    /// Exact capabilities implied by the validated correlated profile set.
    #[must_use]
    pub const fn capabilities(&self) -> &ModelExecutionCapabilitiesV1 {
        self.profiles.capabilities()
    }

    /// Exact correlated profile set.
    #[must_use]
    pub const fn profiles(&self) -> &ModelExecutionProfileSetV1 {
        &self.profiles
    }

    /// Exact backend-owned resource envelope policy.
    #[must_use]
    pub const fn policy(&self) -> &ModelExecutionEnvelopePolicyV1 {
        &self.policy
    }

    /// Convert the validated bundle back to the strict aggregate wire form.
    #[must_use]
    pub fn to_wire(&self) -> ModelExecutionControllerContractsWireV1 {
        ModelExecutionControllerContractsWireV1::from_wire_parts(
            self.capabilities().to_wire(),
            self.profiles.to_wire(),
            self.policy.to_wire(),
        )
    }

    /// Serialize the strict aggregate envelope as compact bounded JSON.
    pub fn to_bounded_json(&self) -> Result<String, RuntimeError> {
        let encoded = serde_json::to_string(&self.to_wire()).map_err(|error| {
            RuntimeError::configuration(format!(
                "could not serialize model-execution controller contracts: {error}"
            ))
        })?;
        validate_json_preallocation_bounds(encoded.as_bytes())?;
        Ok(encoded)
    }

    /// Serialize the strict aggregate envelope as pretty JSON while preserving
    /// the same persisted byte bound accepted by the decoder.
    ///
    /// # Errors
    ///
    /// Returns a configuration error if serialization unexpectedly fails or
    /// the pretty document would exceed the public persisted-input bound.
    pub fn to_pretty_json(&self) -> Result<String, RuntimeError> {
        let encoded = serde_json::to_string_pretty(&self.to_wire()).map_err(|error| {
            RuntimeError::configuration(format!(
                "could not serialize model-execution controller contracts: {error}"
            ))
        })?;
        validate_json_preallocation_bounds(encoded.as_bytes())?;
        Ok(encoded)
    }

    /// Consume the bundle into the two validated declarations required by the
    /// assembled controller. Capabilities remain embedded in `profiles`.
    #[must_use]
    pub fn into_execution_parts(
        self,
    ) -> (ModelExecutionProfileSetV1, ModelExecutionEnvelopePolicyV1) {
        (self.profiles, self.policy)
    }
}

fn validate_json_preallocation_bounds(input: &[u8]) -> Result<(), RuntimeError> {
    if input.len() > MAX_MODEL_EXECUTION_CONTROLLER_CONTRACTS_BYTES {
        return Err(RuntimeError::configuration(format!(
            "model-execution controller contracts JSON exceeds {} bytes",
            MAX_MODEL_EXECUTION_CONTROLLER_CONTRACTS_BYTES
        )));
    }

    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for &byte in input {
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'{' | b'[' => {
                depth = depth.saturating_add(1);
                if depth > MAX_MODEL_EXECUTION_CONTROLLER_CONTRACTS_JSON_DEPTH {
                    return Err(RuntimeError::configuration(format!(
                        "model-execution controller contracts JSON nesting exceeds {}",
                        MAX_MODEL_EXECUTION_CONTROLLER_CONTRACTS_JSON_DEPTH
                    )));
                }
            }
            b'}' | b']' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use elastic_adapters::{
        ModelExecutionCapabilitiesV1, ModelExecutionEnvelopeRuleV1,
        ModelExecutionProfileEnvelopeV1, ModelExecutionProfileV1,
    };

    fn contracts(provider: &str) -> ModelExecutionControllerContractsV1 {
        let capabilities = ModelExecutionCapabilitiesV1::new(
            provider,
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
        ModelExecutionControllerContractsV1::new(profiles, policy).unwrap()
    }

    #[test]
    fn bounded_decoder_round_trip_revalidates_identity_chain() {
        let original = contracts("reference-backend");
        let json = original.to_bounded_json().unwrap();
        assert!(json.len() <= MAX_MODEL_EXECUTION_CONTROLLER_CONTRACTS_BYTES);
        let replayed =
            ModelExecutionControllerContractsV1::from_bounded_json(json.as_bytes()).unwrap();
        assert_eq!(replayed, original);
    }

    #[test]
    fn bounded_decoder_rejects_oversized_input_before_deserialization() {
        let oversized = vec![b' '; MAX_MODEL_EXECUTION_CONTROLLER_CONTRACTS_BYTES + 1];
        let error = ModelExecutionControllerContractsV1::from_bounded_json(&oversized).unwrap_err();
        assert!(error.to_string().contains("exceeds"));
    }

    #[test]
    fn bounded_decoder_rejects_excessive_raw_json_depth() {
        let depth = MAX_MODEL_EXECUTION_CONTROLLER_CONTRACTS_JSON_DEPTH + 1;
        let nested = format!("{}0{}", "[".repeat(depth), "]".repeat(depth));
        let error =
            ModelExecutionControllerContractsV1::from_bounded_json(nested.as_bytes()).unwrap_err();
        assert!(error.to_string().contains("nesting exceeds"));
    }

    #[test]
    fn strict_json_round_trip_revalidates_identity_chain() {
        let original = contracts("reference-backend");
        let json = original.to_pretty_json().unwrap();
        let replayed = ModelExecutionControllerContractsV1::from_json(&json).unwrap();

        assert_eq!(replayed, original);
        assert_eq!(
            replayed.capabilities().fingerprint(),
            original.capabilities().fingerprint()
        );
        assert_eq!(
            replayed.profiles().fingerprint(),
            original.profiles().fingerprint()
        );
        assert_eq!(
            replayed.policy().fingerprint(),
            original.policy().fingerprint()
        );
    }

    #[test]
    fn mixed_wire_identity_fails_closed() {
        let left = contracts("provider-a");
        let right = contracts("provider-b");
        let mixed = ModelExecutionControllerContractsWireV1::from_wire_parts(
            left.capabilities().to_wire(),
            right.profiles().to_wire(),
            right.policy().to_wire(),
        );

        let error = mixed.into_validated().unwrap_err();
        assert!(matches!(error, RuntimeError::Configuration(_)));
    }

    #[test]
    fn unknown_top_level_fields_are_rejected() {
        let contracts = contracts("reference-backend");
        let mut value = serde_json::to_value(contracts.to_wire()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("unexpected".to_owned(), serde_json::json!(true));
        let json = serde_json::to_string(&value).unwrap();

        let error = ModelExecutionControllerContractsV1::from_json(&json).unwrap_err();
        assert!(matches!(error, RuntimeError::Configuration(_)));
    }
}
