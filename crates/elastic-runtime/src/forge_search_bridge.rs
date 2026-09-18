//! Fail-closed Forge -> Elastic Boolean policy candidate bridge.
//!
//! Forge may propose candidate guard/constraint forms, but it does not own
//! Elastic runtime semantics. This module accepts a bounded versioned proposal,
//! validates producer identity syntax, then revalidates every guard and
//! pseudo-Boolean constraint with Elastic's canonical types. The resulting value
//! is descriptive policy data only: it exposes no actuation API and carries no
//! verification or promotion authority.

use std::collections::BTreeSet;

use elastic_core::{
    PredicateKey, PseudoBooleanConstraintDeclaration, PseudoBooleanRelation, PseudoBooleanScale,
    WeightedPredicateKey, MAX_BOOLEAN_EXPR_DEPTH, MAX_PSEUDO_BOOLEAN_TERMS,
};
use elastic_eir::MAX_EIR_PSEUDO_BOOLEAN_CONSTRAINTS;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{GuardConfigError, GuardConfigV1, LoweredGuardConfigV1, PredicateKeyConfigV1};

/// First Elastic-owned interchange schema for Forge-origin Boolean policy candidates.
pub const FORGE_SEARCH_CANDIDATE_SCHEMA_V1: u32 = 1;
/// Exact repository identity accepted as the Forge producer for schema v1.
pub const FORGE_SEARCH_PRODUCER_REPOSITORY_V1: &str = "Memorithm/Forge";
/// Maximum encoded candidate document accepted before JSON allocation.
pub const MAX_FORGE_SEARCH_CANDIDATE_BYTES: usize = 384 * 1024;
/// Additional wrapper depth permitted around an already-bounded guard expression.
pub const MAX_FORGE_SEARCH_CANDIDATE_JSON_DEPTH: usize = MAX_BOOLEAN_EXPR_DEPTH + 16;

/// Forge provenance carried with a proposed policy.
///
/// These fields are syntax-checked provenance, not authentication. Elastic does
/// not treat a Forge fingerprint as executed evidence or destination approval.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForgeCandidateSourceV1 {
    pub repository: String,
    pub commit_id: String,
    pub candidate_id: String,
    pub source_sha256: String,
    pub envelope_fingerprint: String,
}

/// One integer-weighted stable predicate proposed by Forge.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForgeWeightedPredicateV1 {
    pub predicate: PredicateKeyConfigV1,
    /// Canonical base-10 `i128` string. JSON numbers are deliberately avoided.
    pub weight: String,
}

/// Pseudo-Boolean relation exposed by the bridge wire schema.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ForgePseudoBooleanRelationV1 {
    LessOrEqual,
    GreaterOrEqual,
    Equal,
}

impl From<ForgePseudoBooleanRelationV1> for PseudoBooleanRelation {
    fn from(value: ForgePseudoBooleanRelationV1) -> Self {
        match value {
            ForgePseudoBooleanRelationV1::LessOrEqual => Self::LessOrEqual,
            ForgePseudoBooleanRelationV1::GreaterOrEqual => Self::GreaterOrEqual,
            ForgePseudoBooleanRelationV1::Equal => Self::Equal,
        }
    }
}

/// One Forge-proposed pseudo-Boolean declaration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForgePseudoBooleanConstraintV1 {
    pub terms: Vec<ForgeWeightedPredicateV1>,
    pub relation: ForgePseudoBooleanRelationV1,
    /// Canonical base-10 `i128` string.
    pub threshold: String,
    pub unit: String,
    pub quantum: u64,
}

/// Versioned Forge-origin policy candidate accepted by Elastic for revalidation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForgeSearchCandidateV1 {
    pub schema_version: u32,
    pub source: ForgeCandidateSourceV1,
    pub guard_config: GuardConfigV1,
    #[serde(default)]
    pub constraints: Vec<ForgePseudoBooleanConstraintV1>,
}

impl ForgeSearchCandidateV1 {
    /// Decode a bounded candidate and validate it with Elastic-owned semantics.
    pub fn from_bounded_json(bytes: &[u8]) -> Result<Self, ForgeSearchCandidateError> {
        validate_json_preallocation_bounds(bytes)?;
        let candidate: Self = serde_json::from_slice(bytes)
            .map_err(|error| ForgeSearchCandidateError::Decode(error.to_string()))?;
        candidate.validate()?;
        Ok(candidate)
    }

    /// Validate producer syntax and every Elastic guard/constraint declaration.
    pub fn validate(&self) -> Result<(), ForgeSearchCandidateError> {
        if self.schema_version != FORGE_SEARCH_CANDIDATE_SCHEMA_V1 {
            return Err(ForgeSearchCandidateError::UnsupportedSchema {
                actual: self.schema_version,
                supported: FORGE_SEARCH_CANDIDATE_SCHEMA_V1,
            });
        }
        validate_source(&self.source)?;
        self.guard_config
            .validate()
            .map_err(ForgeSearchCandidateError::GuardConfig)?;
        if self.constraints.len() > MAX_EIR_PSEUDO_BOOLEAN_CONSTRAINTS {
            return Err(ForgeSearchCandidateError::TooManyConstraints {
                actual: self.constraints.len(),
                maximum: MAX_EIR_PSEUDO_BOOLEAN_CONSTRAINTS,
            });
        }

        let declared = self
            .guard_config
            .predicates
            .iter()
            .map(|predicate| predicate.key().to_core())
            .collect::<Result<BTreeSet<PredicateKey>, GuardConfigError>>()
            .map_err(ForgeSearchCandidateError::GuardConfig)?;
        for constraint in &self.constraints {
            constraint.to_core(&declared)?;
        }
        Ok(())
    }

    /// Revalidate and lower to canonical Elastic policy types without actuation.
    pub fn revalidate(&self) -> Result<ForgeRevalidatedPolicyV1, ForgeSearchCandidateError> {
        self.validate()?;
        let guard_config = self
            .guard_config
            .lower()
            .map_err(ForgeSearchCandidateError::GuardConfig)?;
        let declared = self
            .guard_config
            .predicates
            .iter()
            .map(|predicate| predicate.key().to_core())
            .collect::<Result<BTreeSet<PredicateKey>, GuardConfigError>>()
            .map_err(ForgeSearchCandidateError::GuardConfig)?;
        let constraints = self
            .constraints
            .iter()
            .map(|constraint| constraint.to_core(&declared))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ForgeRevalidatedPolicyV1 {
            guard_config,
            constraints,
        })
    }
}

impl ForgePseudoBooleanConstraintV1 {
    fn to_core(
        &self,
        declared: &BTreeSet<PredicateKey>,
    ) -> Result<PseudoBooleanConstraintDeclaration, ForgeSearchCandidateError> {
        if self.terms.len() > MAX_PSEUDO_BOOLEAN_TERMS {
            return Err(ForgeSearchCandidateError::TooManyTerms {
                actual: self.terms.len(),
                maximum: MAX_PSEUDO_BOOLEAN_TERMS,
            });
        }
        let threshold = parse_canonical_i128("threshold", &self.threshold)?;
        let scale = PseudoBooleanScale::new(self.unit.clone(), self.quantum)
            .map_err(|error| ForgeSearchCandidateError::Constraint(error.to_string()))?;
        let terms = self
            .terms
            .iter()
            .map(|term| {
                let key = term
                    .predicate
                    .to_core()
                    .map_err(ForgeSearchCandidateError::GuardConfig)?;
                if !declared.contains(&key) {
                    return Err(ForgeSearchCandidateError::UnknownPredicate(key.to_string()));
                }
                let weight = parse_canonical_i128("weight", &term.weight)?;
                WeightedPredicateKey::new(key, weight)
                    .map_err(|error| ForgeSearchCandidateError::Constraint(error.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        PseudoBooleanConstraintDeclaration::new(terms, self.relation.into(), threshold, scale)
            .map_err(|error| ForgeSearchCandidateError::Constraint(error.to_string()))
    }
}

/// Canonical Elastic-owned result of candidate revalidation.
///
/// It intentionally contains no actuator, permit, verification verdict, or
/// promotion bit. Executed evidence remains authoritative downstream.
pub struct ForgeRevalidatedPolicyV1 {
    guard_config: LoweredGuardConfigV1,
    constraints: Vec<PseudoBooleanConstraintDeclaration>,
}

impl ForgeRevalidatedPolicyV1 {
    #[must_use]
    pub const fn guard_config(&self) -> &LoweredGuardConfigV1 {
        &self.guard_config
    }

    #[must_use]
    pub fn constraints(&self) -> &[PseudoBooleanConstraintDeclaration] {
        &self.constraints
    }
}

/// Fail-closed Forge-search bridge errors.
#[derive(Debug, Error, PartialEq)]
pub enum ForgeSearchCandidateError {
    #[error("Forge candidate is {actual} bytes; maximum is {maximum}")]
    TooLarge { actual: usize, maximum: usize },
    #[error("Forge candidate JSON nesting exceeds maximum depth {maximum}")]
    JsonTooDeep { maximum: usize },
    #[error("failed to decode Forge candidate: {0}")]
    Decode(String),
    #[error("unsupported Forge candidate schema {actual}; supported schema is {supported}")]
    UnsupportedSchema { actual: u32, supported: u32 },
    #[error("Forge candidate producer repository must be Memorithm/Forge")]
    InvalidRepository,
    #[error("Forge candidate commit id must be 40 lowercase hexadecimal characters")]
    InvalidCommitId,
    #[error("Forge candidate {0} must be 64 lowercase hexadecimal characters")]
    InvalidSha256(&'static str),
    #[error("Elastic guard revalidation failed: {0}")]
    GuardConfig(GuardConfigError),
    #[error("Forge candidate contains {actual} constraints; maximum is {maximum}")]
    TooManyConstraints { actual: usize, maximum: usize },
    #[error("Forge candidate constraint contains {actual} terms; maximum is {maximum}")]
    TooManyTerms { actual: usize, maximum: usize },
    #[error("Forge candidate {field} is not a canonical base-10 i128: {value}")]
    NonCanonicalInteger { field: &'static str, value: String },
    #[error("Forge candidate constraint references undeclared predicate '{0}'")]
    UnknownPredicate(String),
    #[error("Elastic constraint revalidation failed: {0}")]
    Constraint(String),
}

fn validate_source(source: &ForgeCandidateSourceV1) -> Result<(), ForgeSearchCandidateError> {
    if source.repository != FORGE_SEARCH_PRODUCER_REPOSITORY_V1 {
        return Err(ForgeSearchCandidateError::InvalidRepository);
    }
    if source.commit_id.len() != 40 || !is_lower_hex(&source.commit_id) {
        return Err(ForgeSearchCandidateError::InvalidCommitId);
    }
    for (field, value) in [
        ("candidate_id", source.candidate_id.as_str()),
        ("source_sha256", source.source_sha256.as_str()),
        ("envelope_fingerprint", source.envelope_fingerprint.as_str()),
    ] {
        if value.len() != 64 || !is_lower_hex(value) {
            return Err(ForgeSearchCandidateError::InvalidSha256(field));
        }
    }
    Ok(())
}

fn is_lower_hex(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn parse_canonical_i128(
    field: &'static str,
    value: &str,
) -> Result<i128, ForgeSearchCandidateError> {
    let parsed =
        value
            .parse::<i128>()
            .map_err(|_| ForgeSearchCandidateError::NonCanonicalInteger {
                field,
                value: value.to_owned(),
            })?;
    if parsed.to_string() != value {
        return Err(ForgeSearchCandidateError::NonCanonicalInteger {
            field,
            value: value.to_owned(),
        });
    }
    Ok(parsed)
}

fn validate_json_preallocation_bounds(bytes: &[u8]) -> Result<(), ForgeSearchCandidateError> {
    if bytes.len() > MAX_FORGE_SEARCH_CANDIDATE_BYTES {
        return Err(ForgeSearchCandidateError::TooLarge {
            actual: bytes.len(),
            maximum: MAX_FORGE_SEARCH_CANDIDATE_BYTES,
        });
    }
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for byte in bytes.iter().copied() {
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
                if depth > MAX_FORGE_SEARCH_CANDIDATE_JSON_DEPTH {
                    return Err(ForgeSearchCandidateError::JsonTooDeep {
                        maximum: MAX_FORGE_SEARCH_CANDIDATE_JSON_DEPTH,
                    });
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
    use crate::{
        GuardExprConfigV1, GuardRuleConfigV1, GuardScopeConfigV1, PredicateConfigV1,
        ThresholdComparisonConfigV1, GUARD_CONFIG_SCHEMA_V1,
    };

    fn key(name: &str) -> PredicateKeyConfigV1 {
        PredicateKeyConfigV1 {
            namespace: "elastic.forge-test".to_owned(),
            name: name.to_owned(),
        }
    }

    fn candidate() -> ForgeSearchCandidateV1 {
        let eligible = key("eligible");
        ForgeSearchCandidateV1 {
            schema_version: FORGE_SEARCH_CANDIDATE_SCHEMA_V1,
            source: ForgeCandidateSourceV1 {
                repository: FORGE_SEARCH_PRODUCER_REPOSITORY_V1.to_owned(),
                commit_id: "a".repeat(40),
                candidate_id: "b".repeat(64),
                source_sha256: "c".repeat(64),
                envelope_fingerprint: "d".repeat(64),
            },
            guard_config: GuardConfigV1 {
                schema_version: GUARD_CONFIG_SCHEMA_V1,
                predicates: vec![PredicateConfigV1::ObservationThreshold {
                    key: eligible.clone(),
                    signal: crate::ObservationSignalConfigV1::Builtin {
                        name: crate::BuiltinObservationSignalConfigV1::FreeCapacity,
                    },
                    comparison: ThresholdComparisonConfigV1::GreaterOrEqual,
                    threshold: 1024.0,
                    unit: "bytes".to_owned(),
                    max_age_ms: 1_000,
                }],
                guards: vec![GuardRuleConfigV1 {
                    scope: GuardScopeConfigV1::Resource,
                    expression: GuardExprConfigV1::Atom {
                        predicate: eligible.clone(),
                    },
                }],
            },
            constraints: vec![ForgePseudoBooleanConstraintV1 {
                terms: vec![ForgeWeightedPredicateV1 {
                    predicate: eligible,
                    weight: "1".to_owned(),
                }],
                relation: ForgePseudoBooleanRelationV1::LessOrEqual,
                threshold: "1".to_owned(),
                unit: "count".to_owned(),
                quantum: 1,
            }],
        }
    }

    #[test]
    fn forge_candidate_is_revalidated_into_elastic_types() {
        let candidate = candidate();
        candidate.validate().unwrap();
        let policy = candidate.revalidate().unwrap();
        assert_eq!(policy.guard_config().guards().len(), 1);
        assert_eq!(policy.constraints().len(), 1);
        assert_eq!(policy.constraints()[0].threshold(), 1);
    }

    #[test]
    fn bounded_json_roundtrip_revalidates_candidate() {
        let encoded = serde_json::to_vec(&candidate()).unwrap();
        let decoded = ForgeSearchCandidateV1::from_bounded_json(&encoded).unwrap();
        assert_eq!(decoded, candidate());
    }

    #[test]
    fn forge_identity_is_syntax_checked_but_not_authority() {
        let mut value = candidate();
        value.source.repository = "example/other".to_owned();
        assert_eq!(
            value.validate(),
            Err(ForgeSearchCandidateError::InvalidRepository)
        );

        let mut value = candidate();
        value.source.candidate_id = "ABC".to_owned();
        assert_eq!(
            value.validate(),
            Err(ForgeSearchCandidateError::InvalidSha256("candidate_id"))
        );
    }

    #[test]
    fn constraints_must_reference_elastic_declared_predicates() {
        let mut value = candidate();
        value.constraints[0].terms[0].predicate = key("undeclared");
        assert!(matches!(
            value.validate(),
            Err(ForgeSearchCandidateError::UnknownPredicate(_))
        ));
    }

    #[test]
    fn integer_wire_values_are_canonical_and_zero_weight_fails_closed() {
        let mut value = candidate();
        value.constraints[0].threshold = "01".to_owned();
        assert!(matches!(
            value.validate(),
            Err(ForgeSearchCandidateError::NonCanonicalInteger {
                field: "threshold",
                ..
            })
        ));

        let mut value = candidate();
        value.constraints[0].terms[0].weight = "0".to_owned();
        assert!(matches!(
            value.validate(),
            Err(ForgeSearchCandidateError::Constraint(_))
        ));
    }

    #[test]
    fn future_schema_and_oversized_input_fail_closed() {
        let mut value = candidate();
        value.schema_version += 1;
        assert!(matches!(
            value.validate(),
            Err(ForgeSearchCandidateError::UnsupportedSchema { .. })
        ));

        let bytes = vec![b' '; MAX_FORGE_SEARCH_CANDIDATE_BYTES + 1];
        assert!(matches!(
            ForgeSearchCandidateV1::from_bounded_json(&bytes),
            Err(ForgeSearchCandidateError::TooLarge { .. })
        ));
    }
}
