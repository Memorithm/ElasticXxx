//! Typed durable runtime-evidence contract for ElasticXxx.
//!
//! Version 1 deliberately preserves the existing flat CLI JSON shape while
//! moving schema ownership, bounded ingestion, semantic validation, summaries,
//! and deterministic diffing into the public runtime library.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

pub const EVIDENCE_SCHEMA_V1: &str = "elastic-runtime-evidence-v1";
pub const MAX_EVIDENCE_BYTES: usize = 1024 * 1024;
pub const MAX_EVIDENCE_DEPTH: usize = 32;
pub const MAX_EVIDENCE_NODES: usize = 32_768;
pub const MAX_EVIDENCE_COLLECTION_ITEMS: usize = 8_192;
pub const MAX_EVIDENCE_STRING_BYTES: usize = 64 * 1024;
pub const MAX_EVIDENCE_RESOURCE_ID_BYTES: usize = 256;
pub const MAX_EVIDENCE_DIFF_PATHS: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvidenceSchema {
    #[serde(rename = "elastic-runtime-evidence-v1")]
    V1,
}

impl EvidenceSchema {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::V1 => EVIDENCE_SCHEMA_V1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EvidenceCommand {
    Inspect,
    Observe,
    Plan,
    Doctor,
    Validate,
    Apply,
    Run,
    Watch,
    Explain,
    Replay,
    Diff,
}

impl EvidenceCommand {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Inspect => "inspect",
            Self::Observe => "observe",
            Self::Plan => "plan",
            Self::Doctor => "doctor",
            Self::Validate => "validate",
            Self::Apply => "apply",
            Self::Run => "run",
            Self::Watch => "watch",
            Self::Explain => "explain",
            Self::Replay => "replay",
            Self::Diff => "diff",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvidenceEventKind {
    ObservationCollected,
    ForecastGenerated,
    PlanSelected,
    PlanRejected,
    InvariantChecked,
    PlanValidated,
    ActuationPrepared,
    ActuationApplied,
    VerificationPerformed,
    CommitExecuted,
    RollbackExecuted,
    CycleStarted,
    CycleCompleted,
    ControlLoopStarted,
    ControlLoopStopped,
    CancellationObserved,
    ErrorEncountered,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceEvent {
    pub kind: EvidenceEventKind,
    pub details: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EvidenceEnvelope {
    pub evidence_schema: EvidenceSchema,
    pub command: EvidenceCommand,
    #[serde(flatten)]
    payload: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvidenceSummary {
    pub command: EvidenceCommand,
    pub resource_ids: Vec<String>,
    pub event_count: usize,
    pub commit_count: usize,
    pub rollback_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvidenceDiff {
    pub equal: bool,
    pub changed_paths: Vec<String>,
    pub truncated: bool,
}

#[derive(Debug, Error)]
pub enum EvidenceError {
    #[error("invalid runtime evidence: {0}")]
    Invalid(String),
    #[error("runtime evidence exceeds bounds: {0}")]
    Bounds(String),
    #[error("invalid runtime evidence JSON: {0}")]
    Json(#[from] serde_json::Error),
}

impl EvidenceEnvelope {
    /// Capture one freshly produced evidence object. The v1 schema marker is
    /// inserted by the shared library before strict parsing and validation.
    pub fn capture(mut value: Value) -> Result<Self, EvidenceError> {
        let object = value.as_object_mut().ok_or_else(|| {
            EvidenceError::Invalid("evidence root must be a JSON object".to_owned())
        })?;
        match object.get("evidence_schema") {
            None => {
                object.insert(
                    "evidence_schema".to_owned(),
                    Value::String(EVIDENCE_SCHEMA_V1.to_owned()),
                );
            }
            Some(Value::String(schema)) if schema == EVIDENCE_SCHEMA_V1 => {}
            Some(Value::String(schema)) => {
                return Err(EvidenceError::Invalid(format!(
                    "unsupported evidence schema {schema:?}"
                )));
            }
            Some(_) => {
                return Err(EvidenceError::Invalid(
                    "evidence_schema must be a string".to_owned(),
                ));
            }
        }
        Self::from_value(value)
    }

    /// Parse a previously captured v1 evidence document from bounded bytes.
    pub fn from_slice(bytes: &[u8]) -> Result<Self, EvidenceError> {
        if bytes.len() > MAX_EVIDENCE_BYTES {
            return Err(EvidenceError::Bounds(format!(
                "{} bytes exceeds maximum {}",
                bytes.len(),
                MAX_EVIDENCE_BYTES
            )));
        }
        Self::from_value(serde_json::from_slice(bytes)?)
    }

    /// Parse and validate an already-materialized JSON value.
    pub fn from_value(value: Value) -> Result<Self, EvidenceError> {
        validate_shape(&value)?;
        let envelope: Self = serde_json::from_value(value)?;
        envelope.validate()?;
        Ok(envelope)
    }

    pub fn payload(&self) -> &BTreeMap<String, Value> {
        &self.payload
    }

    pub fn validate(&self) -> Result<(), EvidenceError> {
        let value = self.to_value_unchecked()?;
        validate_shape(&value)?;
        validate_semantics(&value)?;
        Ok(())
    }

    pub fn to_value(&self) -> Result<Value, EvidenceError> {
        self.validate()?;
        self.to_value_unchecked()
    }

    pub fn to_pretty_json(&self) -> Result<String, EvidenceError> {
        Ok(serde_json::to_string_pretty(&self.to_value()?)?)
    }

    pub fn summary(&self) -> Result<EvidenceSummary, EvidenceError> {
        self.validate()?;
        let value = self.to_value_unchecked()?;
        let mut resource_ids = BTreeSet::new();
        let mut event_count = 0usize;
        let mut commit_count = 0usize;
        let mut rollback_count = 0usize;
        collect_summary(
            &value,
            &mut resource_ids,
            &mut event_count,
            &mut commit_count,
            &mut rollback_count,
        )?;
        Ok(EvidenceSummary {
            command: self.command,
            resource_ids: resource_ids.into_iter().collect(),
            event_count,
            commit_count,
            rollback_count,
        })
    }

    pub fn diff(&self, other: &Self) -> Result<EvidenceDiff, EvidenceError> {
        let left = self.to_value()?;
        let right = other.to_value()?;
        let mut changed_paths = Vec::new();
        let mut truncated = false;
        collect_diff_paths(
            &left,
            &right,
            "$".to_owned(),
            &mut changed_paths,
            &mut truncated,
        );
        Ok(EvidenceDiff {
            equal: changed_paths.is_empty() && !truncated,
            changed_paths,
            truncated,
        })
    }

    fn to_value_unchecked(&self) -> Result<Value, EvidenceError> {
        Ok(serde_json::to_value(self)?)
    }
}

fn validate_shape(value: &Value) -> Result<(), EvidenceError> {
    fn visit(value: &Value, depth: usize, nodes: &mut usize) -> Result<(), EvidenceError> {
        if depth > MAX_EVIDENCE_DEPTH {
            return Err(EvidenceError::Bounds(format!(
                "nesting depth exceeds {MAX_EVIDENCE_DEPTH}"
            )));
        }
        *nodes = nodes.saturating_add(1);
        if *nodes > MAX_EVIDENCE_NODES {
            return Err(EvidenceError::Bounds(format!(
                "node count exceeds {MAX_EVIDENCE_NODES}"
            )));
        }
        match value {
            Value::String(value) => {
                if value.len() > MAX_EVIDENCE_STRING_BYTES {
                    return Err(EvidenceError::Bounds(format!(
                        "string exceeds {MAX_EVIDENCE_STRING_BYTES} bytes"
                    )));
                }
            }
            Value::Array(values) => {
                if values.len() > MAX_EVIDENCE_COLLECTION_ITEMS {
                    return Err(EvidenceError::Bounds(format!(
                        "array exceeds {MAX_EVIDENCE_COLLECTION_ITEMS} items"
                    )));
                }
                for value in values {
                    visit(value, depth + 1, nodes)?;
                }
            }
            Value::Object(object) => {
                if object.len() > MAX_EVIDENCE_COLLECTION_ITEMS {
                    return Err(EvidenceError::Bounds(format!(
                        "object exceeds {MAX_EVIDENCE_COLLECTION_ITEMS} entries"
                    )));
                }
                for (key, value) in object {
                    if key.len() > MAX_EVIDENCE_STRING_BYTES {
                        return Err(EvidenceError::Bounds(format!(
                            "object key exceeds {MAX_EVIDENCE_STRING_BYTES} bytes"
                        )));
                    }
                    visit(value, depth + 1, nodes)?;
                }
            }
            Value::Null | Value::Bool(_) | Value::Number(_) => {}
        }
        Ok(())
    }

    let mut nodes = 0usize;
    visit(value, 0, &mut nodes)
}

fn validate_semantics(value: &Value) -> Result<(), EvidenceError> {
    let object = value
        .as_object()
        .ok_or_else(|| EvidenceError::Invalid("evidence root must be a JSON object".to_owned()))?;
    if object.get("evidence_schema").and_then(Value::as_str) != Some(EVIDENCE_SCHEMA_V1) {
        return Err(EvidenceError::Invalid(format!(
            "evidence_schema must equal {EVIDENCE_SCHEMA_V1:?}"
        )));
    }
    if object.get("command").and_then(Value::as_str).is_none() {
        return Err(EvidenceError::Invalid(
            "evidence root requires a command string".to_owned(),
        ));
    }
    validate_value_semantics(value)
}

fn validate_value_semantics(value: &Value) -> Result<(), EvidenceError> {
    match value {
        Value::Array(values) => {
            for value in values {
                validate_value_semantics(value)?;
            }
        }
        Value::Object(object) => {
            if let Some(resource_id) = object.get("resource_id") {
                validate_resource_id(resource_id)?;
            }
            if let Some(events) = object.get("events") {
                validate_events(events)?;
            }
            validate_transaction_flags(object)?;
            if let Some(controllers) = object.get("controllers") {
                validate_controller_identities(controllers)?;
            }
            for value in object.values() {
                validate_value_semantics(value)?;
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
    Ok(())
}

fn validate_resource_id(value: &Value) -> Result<&str, EvidenceError> {
    let resource_id = value.as_str().ok_or_else(|| {
        EvidenceError::Invalid("resource_id must be a string when present".to_owned())
    })?;
    if resource_id.is_empty()
        || resource_id.len() > MAX_EVIDENCE_RESOURCE_ID_BYTES
        || resource_id.chars().any(char::is_control)
    {
        return Err(EvidenceError::Invalid(
            "resource_id is empty, oversized, or contains control characters".to_owned(),
        ));
    }
    Ok(resource_id)
}

fn validate_events(value: &Value) -> Result<(), EvidenceError> {
    let events = value
        .as_array()
        .ok_or_else(|| EvidenceError::Invalid("events must be a JSON array".to_owned()))?;
    for event in events {
        let parsed: EvidenceEvent = serde_json::from_value(event.clone())
            .map_err(|error| EvidenceError::Invalid(format!("invalid runtime event: {error}")))?;
        if parsed.details.len() > MAX_EVIDENCE_STRING_BYTES {
            return Err(EvidenceError::Bounds(format!(
                "event details exceed {MAX_EVIDENCE_STRING_BYTES} bytes"
            )));
        }
    }
    Ok(())
}

fn validate_transaction_flags(
    object: &serde_json::Map<String, Value>,
) -> Result<(), EvidenceError> {
    let committed = optional_bool(object, "committed")?;
    let rolled_back = optional_bool(object, "rolled_back")?;
    if committed == Some(true) && rolled_back == Some(true) {
        return Err(EvidenceError::Invalid(
            "one transaction cannot be both committed and rolled_back".to_owned(),
        ));
    }
    if committed.is_none() && rolled_back.is_none() {
        return Ok(());
    }
    let Some(events) = object.get("events").and_then(Value::as_array) else {
        return Err(EvidenceError::Invalid(
            "transaction outcome flags require an events array".to_owned(),
        ));
    };
    let has_commit = event_kind_present(events, "CommitExecuted");
    let has_rollback = event_kind_present(events, "RollbackExecuted");
    if committed == Some(true) && !has_commit {
        return Err(EvidenceError::Invalid(
            "committed=true requires CommitExecuted evidence".to_owned(),
        ));
    }
    if committed == Some(false) && has_commit {
        return Err(EvidenceError::Invalid(
            "committed=false contradicts CommitExecuted evidence".to_owned(),
        ));
    }
    if rolled_back == Some(true) && !has_rollback {
        return Err(EvidenceError::Invalid(
            "rolled_back=true requires RollbackExecuted evidence".to_owned(),
        ));
    }
    if rolled_back == Some(false) && has_rollback {
        return Err(EvidenceError::Invalid(
            "rolled_back=false contradicts RollbackExecuted evidence".to_owned(),
        ));
    }
    Ok(())
}

fn optional_bool(
    object: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<bool>, EvidenceError> {
    object
        .get(key)
        .map(|value| {
            value.as_bool().ok_or_else(|| {
                EvidenceError::Invalid(format!("{key} must be boolean when present"))
            })
        })
        .transpose()
}

fn event_kind_present(events: &[Value], expected: &str) -> bool {
    events.iter().any(|event| {
        event
            .as_object()
            .and_then(|event| event.get("kind"))
            .and_then(Value::as_str)
            == Some(expected)
    })
}

fn validate_controller_identities(value: &Value) -> Result<(), EvidenceError> {
    let controllers = value
        .as_array()
        .ok_or_else(|| EvidenceError::Invalid("controllers must be a JSON array".to_owned()))?;
    let mut identities = BTreeSet::new();
    for controller in controllers {
        let object = controller.as_object().ok_or_else(|| {
            EvidenceError::Invalid("each controller evidence entry must be an object".to_owned())
        })?;
        let resource_id = object
            .get("resource_id")
            .ok_or_else(|| {
                EvidenceError::Invalid(
                    "each controller evidence entry requires resource_id".to_owned(),
                )
            })
            .and_then(validate_resource_id)?;
        if !identities.insert(resource_id.to_owned()) {
            return Err(EvidenceError::Invalid(format!(
                "duplicate controller resource identity {resource_id:?}"
            )));
        }
    }
    Ok(())
}

fn collect_summary(
    value: &Value,
    resource_ids: &mut BTreeSet<String>,
    event_count: &mut usize,
    commit_count: &mut usize,
    rollback_count: &mut usize,
) -> Result<(), EvidenceError> {
    match value {
        Value::Array(values) => {
            for value in values {
                collect_summary(
                    value,
                    resource_ids,
                    event_count,
                    commit_count,
                    rollback_count,
                )?;
            }
        }
        Value::Object(object) => {
            if let Some(resource_id) = object.get("resource_id") {
                resource_ids.insert(validate_resource_id(resource_id)?.to_owned());
            }
            if let Some(events) = object.get("events") {
                let events = events.as_array().ok_or_else(|| {
                    EvidenceError::Invalid("events must be a JSON array".to_owned())
                })?;
                *event_count = event_count.saturating_add(events.len());
                for event in events {
                    let kind = event
                        .as_object()
                        .and_then(|event| event.get("kind"))
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            EvidenceError::Invalid(
                                "runtime event requires a string kind".to_owned(),
                            )
                        })?;
                    match kind {
                        "CommitExecuted" => *commit_count = commit_count.saturating_add(1),
                        "RollbackExecuted" => *rollback_count = rollback_count.saturating_add(1),
                        _ => {}
                    }
                }
            }
            for (key, value) in object {
                if key != "events" {
                    collect_summary(
                        value,
                        resource_ids,
                        event_count,
                        commit_count,
                        rollback_count,
                    )?;
                }
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
    Ok(())
}

fn collect_diff_paths(
    left: &Value,
    right: &Value,
    path: String,
    changed_paths: &mut Vec<String>,
    truncated: &mut bool,
) {
    if changed_paths.len() >= MAX_EVIDENCE_DIFF_PATHS {
        *truncated = true;
        return;
    }
    match (left, right) {
        (Value::Object(left), Value::Object(right)) => {
            let keys: BTreeSet<&str> = left
                .keys()
                .map(String::as_str)
                .chain(right.keys().map(String::as_str))
                .collect();
            for key in keys {
                let child_path = format!("{path}.{}", json_string(key));
                match (left.get(key), right.get(key)) {
                    (Some(left), Some(right)) => {
                        collect_diff_paths(left, right, child_path, changed_paths, truncated)
                    }
                    _ => push_diff_path(child_path, changed_paths, truncated),
                }
                if *truncated {
                    break;
                }
            }
        }
        (Value::Array(left), Value::Array(right)) => {
            if left.len() != right.len() {
                push_diff_path(format!("{path}.length"), changed_paths, truncated);
            }
            for (index, (left, right)) in left.iter().zip(right).enumerate() {
                collect_diff_paths(
                    left,
                    right,
                    format!("{path}[{index}]"),
                    changed_paths,
                    truncated,
                );
                if *truncated {
                    break;
                }
            }
        }
        _ if left != right => push_diff_path(path, changed_paths, truncated),
        _ => {}
    }
}

fn push_diff_path(path: String, changed_paths: &mut Vec<String>, truncated: &mut bool) {
    if changed_paths.len() < MAX_EVIDENCE_DIFF_PATHS {
        changed_paths.push(path);
    } else {
        *truncated = true;
    }
}

fn json_string(value: &str) -> String {
    serde_json::to_string(value).expect("JSON string serialization cannot fail")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn event(kind: &str) -> Value {
        json!({"kind": kind, "details": "fixture"})
    }

    #[test]
    fn capture_preserves_flat_v1_shape_and_round_trips() {
        let envelope = EvidenceEnvelope::capture(json!({
            "command": "observe",
            "resource_id": "ram",
            "all_signals_valid": true,
            "observations": []
        }))
        .unwrap();
        let value = envelope.to_value().unwrap();
        assert_eq!(value["evidence_schema"], EVIDENCE_SCHEMA_V1);
        assert_eq!(value["command"], "observe");
        assert_eq!(value["resource_id"], "ram");
        assert!(value.get("payload").is_none());
        let encoded = serde_json::to_vec(&value).unwrap();
        assert_eq!(EvidenceEnvelope::from_slice(&encoded).unwrap(), envelope);
    }

    #[test]
    fn every_current_cli_command_is_a_typed_v1_command() {
        for command in [
            "inspect", "observe", "plan", "doctor", "validate", "apply", "run", "watch", "explain",
            "replay", "diff",
        ] {
            let envelope = EvidenceEnvelope::capture(json!({"command": command})).unwrap();
            assert_eq!(envelope.command.as_str(), command);
        }
    }

    #[test]
    fn unknown_schema_and_command_fail_closed() {
        let bad_schema = br#"{"evidence_schema":"v2","command":"run"}"#;
        assert!(EvidenceEnvelope::from_slice(bad_schema).is_err());
        let bad_command = br#"{"evidence_schema":"elastic-runtime-evidence-v1","command":"erase"}"#;
        assert!(EvidenceEnvelope::from_slice(bad_command).is_err());
    }

    #[test]
    fn byte_depth_and_collection_bounds_fail_closed() {
        let oversized = vec![b' '; MAX_EVIDENCE_BYTES + 1];
        assert!(matches!(
            EvidenceEnvelope::from_slice(&oversized),
            Err(EvidenceError::Bounds(_))
        ));

        let mut deep = Value::Null;
        for _ in 0..=MAX_EVIDENCE_DEPTH + 1 {
            deep = json!([deep]);
        }
        let value = json!({
            "evidence_schema": EVIDENCE_SCHEMA_V1,
            "command": "run",
            "deep": deep
        });
        assert!(matches!(
            EvidenceEnvelope::from_value(value),
            Err(EvidenceError::Bounds(_))
        ));
    }

    #[test]
    fn duplicate_controller_resource_identities_fail_closed() {
        let value = json!({
            "evidence_schema": EVIDENCE_SCHEMA_V1,
            "command": "run",
            "controllers": [
                {"resource_id":"ram","events":[]},
                {"resource_id":"ram","events":[]}
            ]
        });
        let error = EvidenceEnvelope::from_value(value).unwrap_err();
        assert!(error.to_string().contains("duplicate controller"));
    }

    #[test]
    fn impossible_transaction_outcomes_fail_closed() {
        let both = json!({
            "evidence_schema": EVIDENCE_SCHEMA_V1,
            "command": "apply",
            "resource_id": "ram",
            "committed": true,
            "rolled_back": true,
            "events": [event("CommitExecuted"), event("RollbackExecuted")]
        });
        assert!(EvidenceEnvelope::from_value(both).is_err());

        let contradiction = json!({
            "evidence_schema": EVIDENCE_SCHEMA_V1,
            "command": "apply",
            "resource_id": "ram",
            "committed": false,
            "rolled_back": false,
            "events": [event("CommitExecuted")]
        });
        assert!(EvidenceEnvelope::from_value(contradiction).is_err());
    }

    #[test]
    fn malformed_or_unknown_event_fails_closed() {
        let malformed = json!({
            "evidence_schema": EVIDENCE_SCHEMA_V1,
            "command": "run",
            "events": [{"kind":"UnknownEvent","details":"fixture"}]
        });
        assert!(EvidenceEnvelope::from_value(malformed).is_err());
    }

    #[test]
    fn summary_is_typed_and_resource_order_is_deterministic() {
        let value = json!({
            "evidence_schema": EVIDENCE_SCHEMA_V1,
            "command": "run",
            "controllers": [
                {"resource_id":"z","events":[event("CycleCompleted")]},
                {"resource_id":"a","events":[event("CommitExecuted")]}
            ]
        });
        let summary = EvidenceEnvelope::from_value(value)
            .unwrap()
            .summary()
            .unwrap();
        assert_eq!(summary.command, EvidenceCommand::Run);
        assert_eq!(summary.resource_ids, ["a", "z"]);
        assert_eq!(summary.event_count, 2);
        assert_eq!(summary.commit_count, 1);
        assert_eq!(summary.rollback_count, 0);
    }

    #[test]
    fn diff_ignores_object_order_but_preserves_array_order() {
        let left = EvidenceEnvelope::from_slice(
            br#"{"command":"run","evidence_schema":"elastic-runtime-evidence-v1","object":{"a":1,"b":2},"items":[1,2]}"#,
        )
        .unwrap();
        let reordered = EvidenceEnvelope::from_slice(
            br#"{"items":[1,2],"object":{"b":2,"a":1},"evidence_schema":"elastic-runtime-evidence-v1","command":"run"}"#,
        )
        .unwrap();
        assert!(left.diff(&reordered).unwrap().equal);

        let changed = EvidenceEnvelope::from_slice(
            br#"{"command":"run","evidence_schema":"elastic-runtime-evidence-v1","object":{"a":1,"b":2},"items":[2,1]}"#,
        )
        .unwrap();
        assert_eq!(
            left.diff(&changed).unwrap().changed_paths,
            ["$.\"items\"[0]", "$.\"items\"[1]"]
        );
    }
}
