//! Read-only CLI frontend for the library-owned runtime-evidence contract.

use std::error::Error;
use std::fs::File;
use std::io::{Error as IoError, ErrorKind, Read};
use std::path::Path;

use elastic::{EvidenceEnvelope, MAX_EVIDENCE_BYTES};
use serde_json::{json, Value};

type CommandResult = Result<(), Box<dyn Error>>;

pub(crate) fn print_json(value: Value) -> CommandResult {
    let envelope = EvidenceEnvelope::capture(value)?;
    println!("{}", envelope.to_pretty_json()?);
    Ok(())
}

pub(crate) fn replay(path: &Path) -> CommandResult {
    let evidence = read_evidence(path)?;
    let summary = evidence.summary()?;
    print_json(json!({
        "command": "replay",
        "replayed_command": summary.command.as_str(),
        "resource_ids": summary.resource_ids,
        "event_count": summary.event_count,
        "commit_count": summary.commit_count,
        "rollback_count": summary.rollback_count,
        "valid": true,
    }))
}

pub(crate) fn diff(left: &Path, right: &Path) -> CommandResult {
    let left_evidence = read_evidence(left)?;
    let right_evidence = read_evidence(right)?;
    let left_summary = left_evidence.summary()?;
    let right_summary = right_evidence.summary()?;
    let diff = left_evidence.diff(&right_evidence)?;
    print_json(json!({
        "command": "diff",
        "left_command": left_summary.command.as_str(),
        "right_command": right_summary.command.as_str(),
        "equal": diff.equal,
        "changed_paths": diff.changed_paths,
        "truncated": diff.truncated,
    }))
}

fn read_evidence(path: &Path) -> Result<EvidenceEnvelope, Box<dyn Error>> {
    let metadata = std::fs::metadata(path).map_err(|error| {
        IoError::new(
            error.kind(),
            format!("cannot inspect evidence file '{}': {error}", path.display()),
        )
    })?;
    if !metadata.is_file() {
        return Err(IoError::new(
            ErrorKind::InvalidData,
            format!("evidence path '{}' is not a regular file", path.display()),
        )
        .into());
    }
    if metadata.len() > MAX_EVIDENCE_BYTES as u64 {
        return Err(IoError::new(
            ErrorKind::InvalidData,
            format!(
                "evidence file '{}' exceeds {} bytes",
                path.display(),
                MAX_EVIDENCE_BYTES
            ),
        )
        .into());
    }
    let file = File::open(path)?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take((MAX_EVIDENCE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_EVIDENCE_BYTES {
        return Err(IoError::new(
            ErrorKind::InvalidData,
            "evidence file grew beyond the bounded ingestion limit while reading",
        )
        .into());
    }
    Ok(EvidenceEnvelope::from_slice(&bytes)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(label: &str) -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "elastic-evidence-{label}-{}-{stamp}.json",
            std::process::id()
        ))
    }

    #[test]
    fn legacy_v1_file_is_parsed_by_shared_contract() {
        let path = temp_path("legacy");
        fs::write(
            &path,
            br#"{"evidence_schema":"elastic-runtime-evidence-v1","command":"observe","resource_id":"ram","observations":[]}"#,
        )
        .unwrap();
        let evidence = read_evidence(&path).unwrap();
        assert_eq!(evidence.summary().unwrap().resource_ids, ["ram"]);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn oversized_file_is_rejected_before_json_parse() {
        let path = temp_path("oversized");
        fs::write(&path, vec![b' '; MAX_EVIDENCE_BYTES + 1]).unwrap();
        let error = read_evidence(&path).unwrap_err();
        assert!(error.to_string().contains("exceeds"));
        fs::remove_file(path).unwrap();
    }
}
