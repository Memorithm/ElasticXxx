//! Stage B real-model fixed-baseline execution-readiness harness.
//!
//! This module consumes the frozen SmolLM2 preregistration JSON and prepares
//! deterministic, fail-closed measurement hooks. It does **not** download
//! model/dataset artifacts, invoke NNIS, unlock final-test, invent metrics, or
//! authorize an elastic allocator/search.
//!
//! CI and local dry-runs validate protocol integrity and candidate admission
//! only. Operator evidence collection against NNIS at the pinned revision
//! remains out of band.

use serde_json::{Map, Value};
use std::fmt;

/// Machine-readable Stage B preregistration schema identity.
pub const STAGE_B_PREREGISTRATION_SCHEMA: &str =
    "elastic-bit-allocation-stage-b-preregistration-v1";

/// Machine-readable Stage B measurement / dry-run report schema identity.
pub const STAGE_B_MEASUREMENT_SCHEMA: &str = "elastic-bit-allocation-stage-b-measurement-v1";

/// Frozen HuggingFace model revision.
pub const PINNED_MODEL_REVISION: &str = "93efa2f097d58c2a74874c7e644dbc9b0cee75a2";

/// Frozen source-model SHA-256.
pub const PINNED_MODEL_SHA256: &str =
    "80521b40281d6ce74e35c9282c22539e75aa0ac8578892b2a59955ef78d55da1";

/// Frozen tokenizer SHA-256.
pub const PINNED_TOKENIZER_SHA256: &str =
    "9ca9acddb6525a194ec8ac7a87f24fbba7232a9a15ffa1af0c1224fcd888e47c";

/// Frozen WikiText dataset revision.
pub const PINNED_DATASET_REVISION: &str = "b08601e04326c79dfdd32d625aee71d232d685c3";

/// Frozen NNIS revision that owns the runtime harness surfaces.
pub const PINNED_NNIS_REVISION: &str = "b9d2f1e74bb68dfa90ca499a24ce857d15e7fb02";

const TRAIN_PARQUET_SHA256: &str =
    "e83889baabc497075506f91975be5fac0d45c5290b6b20582c8cd1e853d0c9f7";
const VALIDATION_PARQUET_SHA256: &str =
    "204929b7ff9d6184953f867dedb860e40aa69c078fc1e54b3baaa8fb28511c4c";
const TEST_PARQUET_SHA256: &str =
    "5f1bea067869d04849c0f975a2b29c4ff47d867f484f5010ea5e861eab246d91";

/// Errors raised while validating Stage B readiness gates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StageBError {
    /// JSON could not be parsed.
    InvalidJson(String),
    /// A required identity or research gate drifted.
    ProtocolViolation(String),
    /// Final-test access was requested while still locked.
    FinalTestLocked,
    /// Allocator/search work was requested while still NO-GO.
    AllocatorUnauthorized,
    /// A candidate cannot be admitted under the frozen rules.
    CandidateInadmissible(String),
}

impl fmt::Display for StageBError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidJson(message) => write!(f, "invalid Stage B JSON: {message}"),
            Self::ProtocolViolation(message) => write!(f, "Stage B protocol violation: {message}"),
            Self::FinalTestLocked => write!(
                f,
                "Stage B final-test remains locked until a follow-up threshold record is frozen"
            ),
            Self::AllocatorUnauthorized => {
                write!(f, "Stage B elastic allocator/search remains unauthorized")
            }
            Self::CandidateInadmissible(message) => {
                write!(f, "Stage B candidate inadmissible: {message}")
            }
        }
    }
}

impl std::error::Error for StageBError {}

/// Data partition roles frozen by the preregistration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum StageBPartition {
    /// Train-derived calibration blocks; fitting allowed.
    Calibration,
    /// Validation-derived development blocks; no fitting; threshold selection later.
    Development,
    /// Test-derived confirmatory blocks; locked in preregistration v1.
    FinalTest,
}

impl StageBPartition {
    /// Stable protocol spelling.
    #[must_use]
    pub const fn canonical_name(self) -> &'static str {
        match self {
            Self::Calibration => "calibration",
            Self::Development => "development",
            Self::FinalTest => "final_test",
        }
    }
}

/// Fixed baseline families required before any allocator GO.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum StageBBaselineFamily {
    /// Mandatory dense reference against the pinned checkpoint.
    DenseReference,
    /// Verified fixed 4-bit full-model baseline.
    Fixed4Bit,
    /// Verified fixed <=2-bit full-model baseline.
    Fixed2BitOrLower,
    /// One structural family: sparse.
    StructuralSparse,
    /// One structural family: low-rank.
    StructuralLowRank,
    /// One structural family: codebook.
    StructuralCodebook,
    /// One structural family: heterogeneous residual.
    StructuralHeterogeneousResidual,
}

impl StageBBaselineFamily {
    /// Stable protocol spelling.
    #[must_use]
    pub const fn canonical_name(self) -> &'static str {
        match self {
            Self::DenseReference => "dense-reference",
            Self::Fixed4Bit => "fixed-4bit",
            Self::Fixed2BitOrLower => "fixed-2bit-or-lower",
            Self::StructuralSparse => "structural-sparse",
            Self::StructuralLowRank => "structural-low-rank",
            Self::StructuralCodebook => "structural-codebook",
            Self::StructuralHeterogeneousResidual => "structural-heterogeneous-residual",
        }
    }

    /// True when this family is the mandatory dense reference.
    #[must_use]
    pub const fn is_dense_reference(self) -> bool {
        matches!(self, Self::DenseReference)
    }
}

/// Closed Stage B candidate slate prepared by the readiness harness.
pub const STAGE_B_FIXED_CANDIDATE_SLATE: &[StageBBaselineFamily] = &[
    StageBBaselineFamily::DenseReference,
    StageBBaselineFamily::Fixed4Bit,
    StageBBaselineFamily::Fixed2BitOrLower,
    StageBBaselineFamily::StructuralSparse,
    StageBBaselineFamily::StructuralLowRank,
    StageBBaselineFamily::StructuralCodebook,
    StageBBaselineFamily::StructuralHeterogeneousResidual,
];

/// Why a metric field is empty in a dry-run or blocked campaign.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MetricStatus {
    /// Measurement was not attempted by this harness path.
    NotExecuted {
        /// Human-readable reason retained for evidence.
        reason: &'static str,
    },
    /// Backend/runtime does not expose the counter.
    Unsupported {
        /// Human-readable reason retained for evidence.
        reason: &'static str,
    },
    /// Candidate or partition is blocked by a research gate.
    Blocked {
        /// Human-readable reason retained for evidence.
        reason: &'static str,
    },
}

impl MetricStatus {
    fn wire_kind(&self) -> &'static str {
        match self {
            Self::NotExecuted { .. } => "not_executed",
            Self::Unsupported { .. } => "unsupported",
            Self::Blocked { .. } => "blocked",
        }
    }

    fn reason(&self) -> &'static str {
        match self {
            Self::NotExecuted { reason }
            | Self::Unsupported { reason }
            | Self::Blocked { reason } => reason,
        }
    }
}

/// Admission decision for one fixed candidate under the frozen protocol.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CandidateAdmission {
    /// Dense reference is planned for operator-run measurement; dry-run does not fabricate values.
    PlannedDenseReference,
    /// Required family cannot run because the owning backend lacks a qualified path.
    BlockedMissingBackendCapability {
        /// Stable reason string.
        reason: &'static str,
    },
}

impl CandidateAdmission {
    fn wire_kind(&self) -> &'static str {
        match self {
            Self::PlannedDenseReference => "planned_dense_reference",
            Self::BlockedMissingBackendCapability { .. } => "blocked_missing_backend_capability",
        }
    }

    fn reason(&self) -> &'static str {
        match self {
            Self::PlannedDenseReference => {
                "dense reference is mandatory and must be measured by an operator against pinned NNIS"
            }
            Self::BlockedMissingBackendCapability { reason } => reason,
        }
    }
}

/// Validated Stage B preregistration identities and research gates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StageBPreregistration {
    /// Exact model repository.
    pub model_repository: String,
    /// Exact model revision.
    pub model_revision: String,
    /// Exact source-model SHA-256.
    pub source_model_sha256: String,
    /// Exact tokenizer SHA-256.
    pub tokenizer_sha256: String,
    /// Exact dataset repository.
    pub dataset_repository: String,
    /// Exact dataset revision.
    pub dataset_revision: String,
    /// Exact NNIS revision.
    pub nnis_revision: String,
    /// Final-test remains locked.
    pub final_test_locked: bool,
    /// Allocator/search remains unauthorized.
    pub allocator_unauthorized: bool,
    /// Low-bit full-model runtime is not qualified at preregistration.
    pub low_bit_runtime_qualified: bool,
    /// Structural full-model runtime is not qualified at preregistration.
    pub structural_runtime_qualified: bool,
}

/// One candidate slot in a Stage B measurement campaign.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StageBCandidatePlan {
    /// Required baseline family.
    pub family: StageBBaselineFamily,
    /// Admission decision.
    pub admission: CandidateAdmission,
    /// Exact serialized bits/value status (never fabricated).
    pub serialized_bits_per_value: MetricStatus,
    /// Exact resident bits/value status (never fabricated).
    pub resident_bits_per_value: MetricStatus,
    /// Mean next-token NLL status (never fabricated).
    pub mean_next_token_nll: MetricStatus,
    /// Derived perplexity status (never fabricated).
    pub perplexity: MetricStatus,
    /// Request-total latency median status (never fabricated).
    pub request_total_ms_median: MetricStatus,
    /// Request-total latency p95 status (never fabricated).
    pub request_total_ms_p95: MetricStatus,
    /// Conversion/materialization time status (never fabricated).
    pub conversion_or_materialization_ms: MetricStatus,
    /// Peak temporary memory status (never fabricated).
    pub peak_temporary_memory_bytes: MetricStatus,
    /// Memory traffic counter status (explicit unsupported rather than zero).
    pub memory_traffic_counters: MetricStatus,
}

/// Schema-versioned dry-run / readiness report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StageBDryRunReport {
    /// Measurement schema identity.
    pub schema: &'static str,
    /// Bound preregistration schema identity.
    pub preregistration_schema: &'static str,
    /// Campaign mode for this report.
    pub mode: &'static str,
    /// Partition selected for this campaign plan.
    pub partition: StageBPartition,
    /// Validated workload identities.
    pub identities: StageBPreregistration,
    /// Dense reference remains mandatory.
    pub dense_reference_required: bool,
    /// Operator must still run NNIS for real evidence.
    pub operator_execution_required: bool,
    /// Operator runtime path retained from the preregistration.
    pub operator_runtime_path: &'static str,
    /// Acceptance thresholds remain unfrozen.
    pub acceptance_thresholds_frozen: bool,
    /// Final-test execution remains unauthorized.
    pub final_test_execution_authorized: bool,
    /// Allocator/search remains unauthorized.
    pub elastic_allocator_or_search_authorized: bool,
    /// Planned / blocked candidate slots.
    pub candidates: Vec<StageBCandidatePlan>,
}

impl StageBDryRunReport {
    /// Stable JSON report with explicit non-numeric metric statuses.
    #[must_use]
    pub fn to_json_value(&self) -> Value {
        let mut root = Map::new();
        root.insert("schema".into(), Value::String(self.schema.into()));
        root.insert(
            "preregistration_schema".into(),
            Value::String(self.preregistration_schema.into()),
        );
        root.insert("mode".into(), Value::String(self.mode.into()));
        root.insert(
            "partition".into(),
            Value::String(self.partition.canonical_name().into()),
        );
        root.insert(
            "dense_reference_required".into(),
            Value::Bool(self.dense_reference_required),
        );
        root.insert(
            "operator_execution_required".into(),
            Value::Bool(self.operator_execution_required),
        );
        root.insert(
            "operator_runtime_path".into(),
            Value::String(self.operator_runtime_path.into()),
        );
        root.insert(
            "acceptance_thresholds_frozen".into(),
            Value::Bool(self.acceptance_thresholds_frozen),
        );
        root.insert(
            "final_test_execution_authorized".into(),
            Value::Bool(self.final_test_execution_authorized),
        );
        root.insert(
            "elastic_allocator_or_search_authorized".into(),
            Value::Bool(self.elastic_allocator_or_search_authorized),
        );
        root.insert(
            "identities".into(),
            Value::Object(identity_object(&self.identities)),
        );
        root.insert(
            "candidates".into(),
            Value::Array(
                self.candidates
                    .iter()
                    .map(candidate_object)
                    .collect::<Vec<_>>(),
            ),
        );
        // Explicit non-claims retained in every dry-run artifact.
        root.insert("real_model_result_claimed".into(), Value::Bool(false));
        root.insert("performance_claimed".into(), Value::Bool(false));
        root.insert("scientific_novelty_claimed".into(), Value::Bool(false));
        root.insert("fabricated_numeric_metrics".into(), Value::Bool(false));
        Value::Object(root)
    }

    /// Canonical pretty-printed JSON suitable for operator inspection and tests.
    #[must_use]
    pub fn canonical_json(&self) -> String {
        serde_json::to_string_pretty(&self.to_json_value())
            .expect("Stage B dry-run report must serialize")
    }
}

fn identity_object(identities: &StageBPreregistration) -> Map<String, Value> {
    let mut object = Map::new();
    object.insert(
        "model_repository".into(),
        Value::String(identities.model_repository.clone()),
    );
    object.insert(
        "model_revision".into(),
        Value::String(identities.model_revision.clone()),
    );
    object.insert(
        "source_model_sha256".into(),
        Value::String(identities.source_model_sha256.clone()),
    );
    object.insert(
        "tokenizer_sha256".into(),
        Value::String(identities.tokenizer_sha256.clone()),
    );
    object.insert(
        "dataset_repository".into(),
        Value::String(identities.dataset_repository.clone()),
    );
    object.insert(
        "dataset_revision".into(),
        Value::String(identities.dataset_revision.clone()),
    );
    object.insert(
        "nnis_revision".into(),
        Value::String(identities.nnis_revision.clone()),
    );
    object.insert(
        "final_test_locked".into(),
        Value::Bool(identities.final_test_locked),
    );
    object.insert(
        "allocator_unauthorized".into(),
        Value::Bool(identities.allocator_unauthorized),
    );
    object.insert(
        "low_bit_runtime_qualified".into(),
        Value::Bool(identities.low_bit_runtime_qualified),
    );
    object.insert(
        "structural_runtime_qualified".into(),
        Value::Bool(identities.structural_runtime_qualified),
    );
    object
}

fn metric_object(status: &MetricStatus) -> Value {
    let mut object = Map::new();
    object.insert("status".into(), Value::String(status.wire_kind().into()));
    object.insert("reason".into(), Value::String(status.reason().into()));
    object.insert("value".into(), Value::Null);
    Value::Object(object)
}

fn candidate_object(candidate: &StageBCandidatePlan) -> Value {
    let mut object = Map::new();
    object.insert(
        "family".into(),
        Value::String(candidate.family.canonical_name().into()),
    );
    object.insert(
        "admission".into(),
        Value::String(candidate.admission.wire_kind().into()),
    );
    object.insert(
        "admission_reason".into(),
        Value::String(candidate.admission.reason().into()),
    );
    object.insert(
        "serialized_bits_per_value".into(),
        metric_object(&candidate.serialized_bits_per_value),
    );
    object.insert(
        "resident_bits_per_value".into(),
        metric_object(&candidate.resident_bits_per_value),
    );
    object.insert(
        "mean_next_token_nll".into(),
        metric_object(&candidate.mean_next_token_nll),
    );
    object.insert("perplexity".into(), metric_object(&candidate.perplexity));
    object.insert(
        "request_total_ms_median".into(),
        metric_object(&candidate.request_total_ms_median),
    );
    object.insert(
        "request_total_ms_p95".into(),
        metric_object(&candidate.request_total_ms_p95),
    );
    object.insert(
        "conversion_or_materialization_ms".into(),
        metric_object(&candidate.conversion_or_materialization_ms),
    );
    object.insert(
        "peak_temporary_memory_bytes".into(),
        metric_object(&candidate.peak_temporary_memory_bytes),
    );
    object.insert(
        "memory_traffic_counters".into(),
        metric_object(&candidate.memory_traffic_counters),
    );
    Value::Object(object)
}

fn require_object<'a>(
    value: &'a Value,
    where_: &str,
) -> Result<&'a Map<String, Value>, StageBError> {
    value
        .as_object()
        .ok_or_else(|| StageBError::ProtocolViolation(format!("{where_} must be an object")))
}

fn require_str<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    where_: &str,
) -> Result<&'a str, StageBError> {
    object
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| StageBError::ProtocolViolation(format!("{where_}.{key} must be a string")))
}

fn require_bool(object: &Map<String, Value>, key: &str, where_: &str) -> Result<bool, StageBError> {
    object
        .get(key)
        .and_then(Value::as_bool)
        .ok_or_else(|| StageBError::ProtocolViolation(format!("{where_}.{key} must be a bool")))
}

fn require_u64(object: &Map<String, Value>, key: &str, where_: &str) -> Result<u64, StageBError> {
    object
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| StageBError::ProtocolViolation(format!("{where_}.{key} must be an integer")))
}

fn require_exact_keys(
    object: &Map<String, Value>,
    expected: &[&str],
    where_: &str,
) -> Result<(), StageBError> {
    let mut actual: Vec<&str> = object.keys().map(String::as_str).collect();
    actual.sort_unstable();
    let mut expected_sorted = expected.to_vec();
    expected_sorted.sort_unstable();
    if actual != expected_sorted {
        return Err(StageBError::ProtocolViolation(format!(
            "{where_} keys differ: expected {expected_sorted:?}, got {actual:?}"
        )));
    }
    Ok(())
}

fn require_eq_str(actual: &str, expected: &str, where_: &str) -> Result<(), StageBError> {
    if actual != expected {
        return Err(StageBError::ProtocolViolation(format!(
            "{where_} drifted: expected {expected:?}, got {actual:?}"
        )));
    }
    Ok(())
}

fn require_eq_bool(actual: bool, expected: bool, where_: &str) -> Result<(), StageBError> {
    if actual != expected {
        return Err(StageBError::ProtocolViolation(format!(
            "{where_} drifted: expected {expected}, got {actual}"
        )));
    }
    Ok(())
}

fn require_null(object: &Map<String, Value>, key: &str, where_: &str) -> Result<(), StageBError> {
    match object.get(key) {
        Some(Value::Null) => Ok(()),
        Some(other) => Err(StageBError::ProtocolViolation(format!(
            "{where_}.{key} must remain null before baseline evidence, got {other}"
        ))),
        None => Err(StageBError::ProtocolViolation(format!(
            "{where_}.{key} is missing"
        ))),
    }
}

/// Parse and fail-closed validate the frozen Stage B preregistration JSON.
pub fn load_stage_b_preregistration(json: &str) -> Result<StageBPreregistration, StageBError> {
    let root: Value =
        serde_json::from_str(json).map_err(|error| StageBError::InvalidJson(error.to_string()))?;
    let root = require_object(&root, "root")?;
    require_exact_keys(
        root,
        &[
            "schema",
            "status",
            "issue",
            "purpose",
            "model",
            "dataset",
            "tokenization",
            "data_partitions",
            "quality",
            "runtime",
            "hardware",
            "performance_protocol",
            "representation_accounting",
            "required_fixed_baseline_families_before_allocator_go",
            "candidate_rules",
            "acceptance_thresholds",
            "authorization",
        ],
        "root",
    )?;

    require_eq_str(
        require_str(root, "schema", "root")?,
        STAGE_B_PREREGISTRATION_SCHEMA,
        "root.schema",
    )?;
    require_eq_str(
        require_str(root, "status", "root")?,
        "preregistered_not_executed",
        "root.status",
    )?;
    if require_u64(root, "issue", "root")? != 29 {
        return Err(StageBError::ProtocolViolation(
            "root.issue must remain bound to 29".into(),
        ));
    }
    require_eq_str(
        require_str(root, "purpose", "root")?,
        "fixed_real_model_baselines_before_any_elastic_allocator_or_search",
        "root.purpose",
    )?;

    let model = require_object(root.get("model").expect("checked"), "model")?;
    require_exact_keys(
        model,
        &[
            "repository",
            "revision",
            "source_model_sha256",
            "source_weight_dtype",
            "tokenizer_sha256",
            "declared_license",
        ],
        "model",
    )?;
    require_eq_str(
        require_str(model, "repository", "model")?,
        "HuggingFaceTB/SmolLM2-135M",
        "model.repository",
    )?;
    require_eq_str(
        require_str(model, "revision", "model")?,
        PINNED_MODEL_REVISION,
        "model.revision",
    )?;
    require_eq_str(
        require_str(model, "source_model_sha256", "model")?,
        PINNED_MODEL_SHA256,
        "model.source_model_sha256",
    )?;
    require_eq_str(
        require_str(model, "source_weight_dtype", "model")?,
        "bfloat16",
        "model.source_weight_dtype",
    )?;
    require_eq_str(
        require_str(model, "tokenizer_sha256", "model")?,
        PINNED_TOKENIZER_SHA256,
        "model.tokenizer_sha256",
    )?;
    require_eq_str(
        require_str(model, "declared_license", "model")?,
        "apache-2.0",
        "model.declared_license",
    )?;

    let dataset = require_object(root.get("dataset").expect("checked"), "dataset")?;
    require_exact_keys(
        dataset,
        &[
            "repository",
            "revision",
            "config",
            "declared_license_metadata",
            "license_metadata_is_provenance_not_legal_interpretation",
            "files",
        ],
        "dataset",
    )?;
    require_eq_str(
        require_str(dataset, "repository", "dataset")?,
        "Salesforce/wikitext",
        "dataset.repository",
    )?;
    require_eq_str(
        require_str(dataset, "revision", "dataset")?,
        PINNED_DATASET_REVISION,
        "dataset.revision",
    )?;
    require_eq_str(
        require_str(dataset, "config", "dataset")?,
        "wikitext-2-raw-v1",
        "dataset.config",
    )?;
    require_eq_bool(
        require_bool(
            dataset,
            "license_metadata_is_provenance_not_legal_interpretation",
            "dataset",
        )?,
        true,
        "dataset.license_metadata_is_provenance_not_legal_interpretation",
    )?;
    let files = require_object(dataset.get("files").expect("checked"), "dataset.files")?;
    require_exact_keys(files, &["train", "validation", "test"], "dataset.files")?;
    validate_dataset_file(
        files.get("train").expect("checked"),
        "train",
        "wikitext-2-raw-v1/train-00000-of-00001.parquet",
        TRAIN_PARQUET_SHA256,
        36718,
    )?;
    validate_dataset_file(
        files.get("validation").expect("checked"),
        "validation",
        "wikitext-2-raw-v1/validation-00000-of-00001.parquet",
        VALIDATION_PARQUET_SHA256,
        3760,
    )?;
    validate_dataset_file(
        files.get("test").expect("checked"),
        "test",
        "wikitext-2-raw-v1/test-00000-of-00001.parquet",
        TEST_PARQUET_SHA256,
        4358,
    )?;

    let partitions = require_object(
        root.get("data_partitions").expect("checked"),
        "data_partitions",
    )?;
    require_exact_keys(
        partitions,
        &["calibration", "development", "final_test"],
        "data_partitions",
    )?;
    let final_test = require_object(
        partitions.get("final_test").expect("checked"),
        "data_partitions.final_test",
    )?;
    require_eq_str(
        require_str(final_test, "access", "data_partitions.final_test")?,
        "locked_until_followup_threshold_record_is_frozen",
        "data_partitions.final_test.access",
    )?;
    require_eq_bool(
        require_bool(
            final_test,
            "candidate_fitting_allowed",
            "data_partitions.final_test",
        )?,
        false,
        "data_partitions.final_test.candidate_fitting_allowed",
    )?;
    require_eq_bool(
        require_bool(
            final_test,
            "acceptance_threshold_selection_allowed",
            "data_partitions.final_test",
        )?,
        false,
        "data_partitions.final_test.acceptance_threshold_selection_allowed",
    )?;

    let quality = require_object(root.get("quality").expect("checked"), "quality")?;
    require_eq_bool(
        require_bool(quality, "dense_reference_required", "quality")?,
        true,
        "quality.dense_reference_required",
    )?;

    let runtime = require_object(root.get("runtime").expect("checked"), "runtime")?;
    require_eq_str(
        require_str(runtime, "owner_repository", "runtime")?,
        "Memorithm/NNIS",
        "runtime.owner_repository",
    )?;
    require_eq_str(
        require_str(runtime, "revision", "runtime")?,
        PINNED_NNIS_REVISION,
        "runtime.revision",
    )?;
    let low_bit_runtime_qualified = require_bool(
        runtime,
        "low_bit_full_model_runtime_qualified_at_preregistration",
        "runtime",
    )?;
    let structural_runtime_qualified = require_bool(
        runtime,
        "structural_full_model_runtime_qualified_at_preregistration",
        "runtime",
    )?;
    require_eq_bool(
        low_bit_runtime_qualified,
        false,
        "runtime.low_bit_full_model_runtime_qualified_at_preregistration",
    )?;
    require_eq_bool(
        structural_runtime_qualified,
        false,
        "runtime.structural_full_model_runtime_qualified_at_preregistration",
    )?;

    let thresholds = require_object(
        root.get("acceptance_thresholds").expect("checked"),
        "acceptance_thresholds",
    )?;
    require_eq_str(
        require_str(thresholds, "status", "acceptance_thresholds")?,
        "unfrozen_pending_dense_and_fixed_baseline_evidence_on_development_split",
        "acceptance_thresholds.status",
    )?;
    for field in [
        "max_quality_degradation",
        "min_physical_storage_improvement",
        "max_steady_state_latency_regression",
        "max_conversion_or_setup_cost_or_amortization_horizon",
        "reproducibility_tolerance_and_run_policy",
    ] {
        require_null(thresholds, field, "acceptance_thresholds")?;
    }

    let authorization =
        require_object(root.get("authorization").expect("checked"), "authorization")?;
    for key in [
        "final_test_execution_authorized",
        "elastic_allocator_or_search_authorized",
        "real_model_result_claimed",
        "performance_claimed",
        "scientific_novelty_claimed",
    ] {
        require_eq_bool(
            require_bool(authorization, key, "authorization")?,
            false,
            &format!("authorization.{key}"),
        )?;
    }

    Ok(StageBPreregistration {
        model_repository: "HuggingFaceTB/SmolLM2-135M".into(),
        model_revision: PINNED_MODEL_REVISION.into(),
        source_model_sha256: PINNED_MODEL_SHA256.into(),
        tokenizer_sha256: PINNED_TOKENIZER_SHA256.into(),
        dataset_repository: "Salesforce/wikitext".into(),
        dataset_revision: PINNED_DATASET_REVISION.into(),
        nnis_revision: PINNED_NNIS_REVISION.into(),
        final_test_locked: true,
        allocator_unauthorized: true,
        low_bit_runtime_qualified,
        structural_runtime_qualified,
    })
}

fn validate_dataset_file(
    value: &Value,
    split: &str,
    path: &str,
    sha256: &str,
    rows: u64,
) -> Result<(), StageBError> {
    let object = require_object(value, &format!("dataset.files.{split}"))?;
    require_exact_keys(
        object,
        &["path", "sha256", "rows"],
        &format!("dataset.files.{split}"),
    )?;
    require_eq_str(
        require_str(object, "path", &format!("dataset.files.{split}"))?,
        path,
        &format!("dataset.files.{split}.path"),
    )?;
    require_eq_str(
        require_str(object, "sha256", &format!("dataset.files.{split}"))?,
        sha256,
        &format!("dataset.files.{split}.sha256"),
    )?;
    if require_u64(object, "rows", &format!("dataset.files.{split}"))? != rows {
        return Err(StageBError::ProtocolViolation(format!(
            "dataset.files.{split}.rows drifted"
        )));
    }
    Ok(())
}

/// Fail closed if final-test access is requested while locked.
pub fn authorize_partition_access(
    preregistration: &StageBPreregistration,
    partition: StageBPartition,
) -> Result<(), StageBError> {
    match partition {
        StageBPartition::Calibration | StageBPartition::Development => Ok(()),
        StageBPartition::FinalTest => {
            if preregistration.final_test_locked {
                Err(StageBError::FinalTestLocked)
            } else {
                // Preregistration v1 always keeps the lock; any unlocked state is a protocol bug.
                Err(StageBError::ProtocolViolation(
                    "final-test unlock is outside preregistration v1".into(),
                ))
            }
        }
    }
}

/// Fail closed if allocator/search work is requested.
pub fn authorize_allocator(preregistration: &StageBPreregistration) -> Result<(), StageBError> {
    if preregistration.allocator_unauthorized {
        Err(StageBError::AllocatorUnauthorized)
    } else {
        Err(StageBError::ProtocolViolation(
            "allocator authorization is outside preregistration v1".into(),
        ))
    }
}

fn metric_for_admission(admission: &CandidateAdmission) -> MetricStatus {
    match admission {
        CandidateAdmission::PlannedDenseReference => MetricStatus::NotExecuted {
            reason:
                "operator must execute pinned NNIS dense reference; dry-run does not invent values",
        },
        CandidateAdmission::BlockedMissingBackendCapability { reason } => {
            MetricStatus::Blocked { reason }
        }
    }
}

fn admit_candidate(
    family: StageBBaselineFamily,
    preregistration: &StageBPreregistration,
) -> Result<StageBCandidatePlan, StageBError> {
    let admission = match family {
        StageBBaselineFamily::DenseReference => CandidateAdmission::PlannedDenseReference,
        StageBBaselineFamily::Fixed4Bit | StageBBaselineFamily::Fixed2BitOrLower => {
            if preregistration.low_bit_runtime_qualified {
                return Err(StageBError::ProtocolViolation(
                    "low-bit runtime qualification must not be fabricated in preregistration v1"
                        .into(),
                ));
            }
            CandidateAdmission::BlockedMissingBackendCapability {
                reason:
                    "NNIS full-model low-bit runtime is not qualified at Stage B preregistration",
            }
        }
        StageBBaselineFamily::StructuralSparse
        | StageBBaselineFamily::StructuralLowRank
        | StageBBaselineFamily::StructuralCodebook
        | StageBBaselineFamily::StructuralHeterogeneousResidual => {
            if preregistration.structural_runtime_qualified {
                return Err(StageBError::ProtocolViolation(
                    "structural runtime qualification must not be fabricated in preregistration v1"
                        .into(),
                ));
            }
            CandidateAdmission::BlockedMissingBackendCapability {
                reason:
                    "NNIS full-model structural weight runtime is not qualified at Stage B preregistration",
            }
        }
    };

    let metric = metric_for_admission(&admission);
    let memory_traffic = match &admission {
        CandidateAdmission::PlannedDenseReference => MetricStatus::NotExecuted {
            reason: "memory traffic counters remain operator/runtime-dependent and must stay explicit when unsupported",
        },
        CandidateAdmission::BlockedMissingBackendCapability { reason } => MetricStatus::Blocked {
            reason,
        },
    };

    Ok(StageBCandidatePlan {
        family,
        admission,
        serialized_bits_per_value: metric.clone(),
        resident_bits_per_value: metric.clone(),
        mean_next_token_nll: metric.clone(),
        perplexity: metric.clone(),
        request_total_ms_median: metric.clone(),
        request_total_ms_p95: metric.clone(),
        conversion_or_materialization_ms: metric.clone(),
        peak_temporary_memory_bytes: metric,
        memory_traffic_counters: memory_traffic,
    })
}

/// Build a deterministic Stage B dry-run campaign plan.
///
/// The development partition is the only admissible evidence-planning split in
/// preregistration v1. Final-test remains locked. No numeric observations are
/// produced.
pub fn dry_run_fixed_baseline_campaign(
    preregistration: &StageBPreregistration,
    partition: StageBPartition,
) -> Result<StageBDryRunReport, StageBError> {
    authorize_partition_access(preregistration, partition)?;
    // Allocator/search remains an explicit NO-GO for every dry-run consumer.
    match authorize_allocator(preregistration) {
        Err(StageBError::AllocatorUnauthorized) => {}
        other => {
            return Err(StageBError::ProtocolViolation(format!(
                "allocator authorization gate failed open: {other:?}"
            )));
        }
    }

    let mut candidates = Vec::with_capacity(STAGE_B_FIXED_CANDIDATE_SLATE.len());
    let mut dense_seen = false;
    for family in STAGE_B_FIXED_CANDIDATE_SLATE {
        if family.is_dense_reference() {
            dense_seen = true;
        }
        candidates.push(admit_candidate(*family, preregistration)?);
    }
    if !dense_seen {
        return Err(StageBError::CandidateInadmissible(
            "dense reference is mandatory and missing from the candidate slate".into(),
        ));
    }

    Ok(StageBDryRunReport {
        schema: STAGE_B_MEASUREMENT_SCHEMA,
        preregistration_schema: STAGE_B_PREREGISTRATION_SCHEMA,
        mode: "dry_run_protocol_integrity",
        partition,
        identities: preregistration.clone(),
        dense_reference_required: true,
        operator_execution_required: true,
        operator_runtime_path:
            "Memorithm/NNIS crates/nnis-bench/examples/smollm2_e2e.rs at pinned revision",
        acceptance_thresholds_frozen: false,
        final_test_execution_authorized: false,
        elastic_allocator_or_search_authorized: false,
        candidates,
    })
}

/// Convenience entrypoint used by the example binary and integration tests.
pub fn run_stage_b_dry_run(json: &str) -> Result<StageBDryRunReport, StageBError> {
    let preregistration = load_stage_b_preregistration(json)?;
    dry_run_fixed_baseline_campaign(&preregistration, StageBPartition::Development)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FROZEN_MANIFEST: &str =
        include_str!("../../../research/elastic-bit-allocation-stage-b-smollm2-v1.json");

    #[test]
    fn frozen_manifest_loads_and_dry_runs_without_numeric_claims() {
        let report = run_stage_b_dry_run(FROZEN_MANIFEST).expect("frozen manifest must dry-run");
        assert_eq!(report.schema, STAGE_B_MEASUREMENT_SCHEMA);
        assert_eq!(report.partition, StageBPartition::Development);
        assert!(report.operator_execution_required);
        assert!(!report.final_test_execution_authorized);
        assert!(!report.elastic_allocator_or_search_authorized);
        assert!(!report.acceptance_thresholds_frozen);
        assert_eq!(report.candidates.len(), STAGE_B_FIXED_CANDIDATE_SLATE.len());
        assert!(matches!(
            report.candidates[0].admission,
            CandidateAdmission::PlannedDenseReference
        ));
        assert!(report.candidates[1..].iter().all(|candidate| matches!(
            candidate.admission,
            CandidateAdmission::BlockedMissingBackendCapability { .. }
        )));

        let json = report.to_json_value();
        assert_eq!(json["fabricated_numeric_metrics"], Value::Bool(false));
        assert_eq!(json["real_model_result_claimed"], Value::Bool(false));
        assert!(json["candidates"][0]["serialized_bits_per_value"]["value"].is_null());
        assert_eq!(
            json["candidates"][0]["serialized_bits_per_value"]["status"],
            "not_executed"
        );
        assert_eq!(
            json["candidates"][1]["mean_next_token_nll"]["status"],
            "blocked"
        );
    }

    #[test]
    fn final_test_and_allocator_remain_fail_closed() {
        let preregistration =
            load_stage_b_preregistration(FROZEN_MANIFEST).expect("manifest must validate");
        assert_eq!(
            authorize_partition_access(&preregistration, StageBPartition::FinalTest),
            Err(StageBError::FinalTestLocked)
        );
        assert_eq!(
            authorize_allocator(&preregistration),
            Err(StageBError::AllocatorUnauthorized)
        );
        assert_eq!(
            dry_run_fixed_baseline_campaign(&preregistration, StageBPartition::FinalTest),
            Err(StageBError::FinalTestLocked)
        );
    }

    #[test]
    fn identity_drift_is_rejected() {
        let mutated = FROZEN_MANIFEST.replace(PINNED_MODEL_REVISION, "0".repeat(40).as_str());
        let error = load_stage_b_preregistration(&mutated).expect_err("drift must fail");
        assert!(matches!(error, StageBError::ProtocolViolation(_)));
    }

    #[test]
    fn premature_threshold_is_rejected() {
        let mut root: Value = serde_json::from_str(FROZEN_MANIFEST).unwrap();
        root["acceptance_thresholds"]["max_quality_degradation"] = Value::from(0.01);
        let error =
            load_stage_b_preregistration(&root.to_string()).expect_err("threshold must fail");
        assert!(matches!(error, StageBError::ProtocolViolation(_)));
    }
}
