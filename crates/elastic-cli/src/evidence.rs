//! Deterministic runtime-evidence inspection utilities.

use std::collections::BTreeSet;
use std::error::Error;
use std::fs;
use std::io::{Error as IoError, ErrorKind};
use std::path::Path;

use serde_json::{json, Value};

pub(crate) const EVIDENCE_SCHEMA: &str = "elastic-runtime-evidence-v1";
const MAX_DIFF_PATHS: usize = 512;

type CommandResult = Result<(), Box<dyn Error>>;

/// Print one machine-readable evidence record.
pub(crate) fn print_json(mut value: Value) -> CommandResult {
    if let Value::Object(object) = &mut value {
        object
            .entry("evidence_schema".to_owned())
            .or_insert_with(|| Value::String(EVIDENCE_SCHEMA.to_owned()));
    }
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

/// Validate and summarize a previously captured CLI evidence record.
///
/// Replay is deliberately read-only: it parses and validates the evidence
/// envelope but never rebuilds an adapter, planner, actuator, or controller.
pub(crate) fn replay(path: &Path) -> CommandResult {
    let evidence = read_json(path)?;
    let summary = validate_evidence(&evidence)?;

    print_json(json!({
        "command": "replay",
        "replayed_command": summary.command,
        "resource_ids": summary.resource_ids,
        "event_count": summary.event_count,
        "commit_count": summary.commit_count,
        "rollback_count": summary.rollback_count,
        "valid": true,
    }))
}

/// Compare two captured evidence records using canonical JSON ordering.
///
/// Object key order is ignored, while array order remains meaningful because
/// cycle and event order are part of the runtime evidence contract.
pub(crate) fn diff(left: &Path, right: &Path) -> CommandResult {
    let left_value = read_json(left)?;
    let right_value = read_json(right)?;
    let left_summary = validate_evidence(&left_value)?;
    let right_summary = validate_evidence(&right_value)?;
    let mut changed_paths = Vec::new();
    let mut truncated = false;
    collect_diff_paths(
        &left_value,
        &right_value,
        "$".to_owned(),
        &mut changed_paths,
        &mut truncated,
    );

    print_json(json!({
        "command": "diff",
        "left_command": left_summary.command,
        "right_command": right_summary.command,
        "equal": changed_paths.is_empty() && !truncated,
        "changed_paths": changed_paths,
        "truncated": truncated,
    }))
}

fn read_json(path: &Path) -> Result<Value, Box<dyn Error>> {
    let contents = fs::read_to_string(path).map_err(|error| {
        IoError::new(
            error.kind(),
            format!("cannot read evidence file '{}': {error}", path.display()),
        )
    })?;
    serde_json::from_str(&contents).map_err(|error| {
        IoError::new(
            ErrorKind::InvalidData,
            format!("invalid JSON evidence file '{}': {error}", path.display()),
        )
        .into()
    })
}

#[derive(Debug)]
struct EvidenceSummary {
    command: String,
    resource_ids: Vec<String>,
    event_count: usize,
    commit_count: usize,
    rollback_count: usize,
}

fn validate_evidence(value: &Value) -> Result<EvidenceSummary, Box<dyn Error>> {
    let object = value.as_object().ok_or_else(|| {
        IoError::new(
            ErrorKind::InvalidData,
            "evidence root must be a JSON object",
        )
    })?;
    let command = object
        .get("command")
        .and_then(Value::as_str)
        .filter(|command| !command.is_empty())
        .ok_or_else(|| {
            IoError::new(
                ErrorKind::InvalidData,
                "evidence root requires a non-empty string command",
            )
        })?
        .to_owned();
    let schema = object
        .get("evidence_schema")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            IoError::new(
                ErrorKind::InvalidData,
                "evidence root requires an evidence_schema string",
            )
        })?;
    if schema != EVIDENCE_SCHEMA {
        return Err(IoError::new(
            ErrorKind::InvalidData,
            format!("unsupported evidence schema '{schema}'"),
        )
        .into());
    }

    let mut summary = EvidenceSummary {
        command,
        resource_ids: Vec::new(),
        event_count: 0,
        commit_count: 0,
        rollback_count: 0,
    };
    visit_evidence(value, &mut summary)?;
    summary.resource_ids.sort_unstable();
    summary.resource_ids.dedup();
    Ok(summary)
}

fn visit_evidence(value: &Value, summary: &mut EvidenceSummary) -> Result<(), Box<dyn Error>> {
    match value {
        Value::Array(values) => {
            for value in values {
                visit_evidence(value, summary)?;
            }
        }
        Value::Object(object) => {
            if let Some(resource_id) = object.get("resource_id") {
                let resource_id = resource_id.as_str().ok_or_else(|| {
                    IoError::new(
                        ErrorKind::InvalidData,
                        "resource_id must be a string when present",
                    )
                })?;
                if resource_id.is_empty() {
                    return Err(IoError::new(
                        ErrorKind::InvalidData,
                        "resource_id must not be empty",
                    )
                    .into());
                }
                summary.resource_ids.push(resource_id.to_owned());
            }

            if let Some(events) = object.get("events") {
                validate_events(events, summary)?;
            }

            for (key, value) in object {
                if key != "events" {
                    visit_evidence(value, summary)?;
                }
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
    Ok(())
}

fn validate_events(value: &Value, summary: &mut EvidenceSummary) -> Result<(), Box<dyn Error>> {
    let events = value
        .as_array()
        .ok_or_else(|| IoError::new(ErrorKind::InvalidData, "events must be a JSON array"))?;
    for event in events {
        let object = event.as_object().ok_or_else(|| {
            IoError::new(ErrorKind::InvalidData, "each event must be a JSON object")
        })?;
        let kind = object
            .get("kind")
            .and_then(Value::as_str)
            .filter(|kind| !kind.is_empty())
            .ok_or_else(|| {
                IoError::new(
                    ErrorKind::InvalidData,
                    "each event requires a non-empty string kind",
                )
            })?;
        if object.get("details").and_then(Value::as_str).is_none() {
            return Err(
                IoError::new(ErrorKind::InvalidData, "each event requires string details").into(),
            );
        }
        summary.event_count += 1;
        match kind {
            "CommitExecuted" => summary.commit_count += 1,
            "RollbackExecuted" => summary.rollback_count += 1,
            _ => {}
        }
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
    if changed_paths.len() >= MAX_DIFF_PATHS {
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
                        collect_diff_paths(left, right, child_path, changed_paths, truncated);
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
    if changed_paths.len() < MAX_DIFF_PATHS {
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

    fn event(kind: &str) -> Value {
        json!({"kind": kind, "details": "fixture"})
    }

    #[test]
    fn evidence_validation_collects_nested_runtime_facts() {
        let value = json!({
            "command": "run",
            "evidence_schema": EVIDENCE_SCHEMA,
            "resource_id": "ram",
            "cycles": [{
                "resource_id": "ram",
                "committed": true,
                "events": [event("CommitExecuted"), event("VerificationPerformed")]
            }],
            "events": [event("CycleCompleted"), event("RollbackExecuted")]
        });

        let summary = validate_evidence(&value).unwrap();
        assert_eq!(summary.command, "run");
        assert_eq!(summary.resource_ids, ["ram"]);
        assert_eq!(summary.event_count, 4);
        assert_eq!(summary.commit_count, 1);
        assert_eq!(summary.rollback_count, 1);
    }

    #[test]
    fn invalid_event_shape_fails_closed() {
        let value = json!({
            "command": "apply",
            "evidence_schema": EVIDENCE_SCHEMA,
            "events": [{"kind": "CommitExecuted", "details": 1}]
        });

        let error = validate_evidence(&value).unwrap_err();
        assert!(error.to_string().contains("string details"));
    }

    #[test]
    fn diff_ignores_object_order_but_not_arrays() {
        let left = json!({"command":"run","evidence_schema":EVIDENCE_SCHEMA,"object":{"a":1,"b":2},"items":[1,2]});
        let reordered = json!({"items":[1,2],"object":{"b":2,"a":1},"evidence_schema":EVIDENCE_SCHEMA,"command":"run"});
        let changed = json!({"command":"run","evidence_schema":EVIDENCE_SCHEMA,"object":{"a":1,"b":2},"items":[2,1]});
        let mut paths = Vec::new();
        let mut truncated = false;

        collect_diff_paths(
            &left,
            &reordered,
            "$".to_owned(),
            &mut paths,
            &mut truncated,
        );
        assert!(paths.is_empty());
        collect_diff_paths(&left, &changed, "$".to_owned(), &mut paths, &mut truncated);
        assert_eq!(paths, ["$.\"items\"[0]", "$.\"items\"[1]"]);
    }
}
