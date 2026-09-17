//! Read-only BE10 Boolean guard operator commands.
//!
//! These commands deliberately lower through the public `elastic` facade and
//! never construct an actuator, controller, or runtime. Missing fact values are
//! evaluated as `Unknown` by the library-owned strong-Kleene guard semantics.

use std::collections::BTreeMap;
use std::error::Error;
use std::fs::File;
use std::io::{Error as IoError, ErrorKind, Read};
use std::path::Path;

use elastic::{GuardConfigV1, PredicateKey, TruthValue, MAX_GUARD_CONFIG_BYTES};
use serde_json::{json, Value};

use crate::evidence::print_json;

type CommandResult = Result<(), Box<dyn Error>>;

pub(crate) fn check(path: &Path) -> CommandResult {
    let config = read_config(path)?;
    let lowered = config.lower()?;
    print_json(json!({
        "command": "guard-check",
        "schema_version": config.schema_version,
        "valid": true,
        "predicate_count": lowered.registry().len(),
        "guard_count": lowered.guards().len(),
        "read_only": true,
        "actuation_authorized": false,
    }))
}

pub(crate) fn list(path: &Path) -> CommandResult {
    let config = read_config(path)?;
    let lowered = config.lower()?;
    let predicates = lowered
        .registry()
        .iter()
        .map(|(_, key)| key.to_string())
        .collect::<Vec<_>>();
    let guards = lowered
        .guards()
        .iter()
        .enumerate()
        .map(|(index, guard)| {
            json!({
                "index": index,
                "scope": guard.scope().to_string(),
                "expression_fingerprint": guard.fingerprint().to_string(),
            })
        })
        .collect::<Vec<_>>();

    print_json(json!({
        "command": "guard-list",
        "schema_version": config.schema_version,
        "predicates": predicates,
        "guards": guards,
        "read_only": true,
        "actuation_authorized": false,
    }))
}

pub(crate) fn fingerprint(path: &Path) -> CommandResult {
    let config = read_config(path)?;
    let lowered = config.lower()?;
    let guards = lowered
        .guards()
        .iter()
        .enumerate()
        .map(|(index, guard)| {
            json!({
                "index": index,
                "scope": guard.scope().to_string(),
                "expression_fingerprint": guard.fingerprint().to_string(),
            })
        })
        .collect::<Vec<_>>();

    print_json(json!({
        "command": "guard-fingerprint",
        "schema_version": config.schema_version,
        "fingerprint_kind": "canonical-guard-expression-v1",
        "guards": guards,
        "read_only": true,
        "actuation_authorized": false,
    }))
}

pub(crate) fn eval(path: &Path, assignments: &[String]) -> CommandResult {
    evaluate(path, assignments, false)
}

pub(crate) fn explain(path: &Path, assignments: &[String]) -> CommandResult {
    evaluate(path, assignments, true)
}

fn evaluate(path: &Path, assignments: &[String], explain: bool) -> CommandResult {
    let config = read_config(path)?;
    let lowered = config.lower()?;
    let facts = parse_facts(assignments, &lowered)?;

    let mut results = Vec::with_capacity(lowered.guards().len());
    for (index, guard) in lowered.guards().iter().enumerate() {
        let truth = guard.evaluate(&facts)?;
        results.push(json!({
            "index": index,
            "scope": guard.scope().to_string(),
            "expression_fingerprint": guard.fingerprint().to_string(),
            "truth": truth_name(truth),
        }));
    }

    let fact_view = lowered
        .registry()
        .iter()
        .map(|(_, key)| {
            json!({
                "predicate": key.to_string(),
                "truth": truth_name(facts.get(key).copied().unwrap_or(TruthValue::Unknown)),
                "explicit": facts.contains_key(key),
            })
        })
        .collect::<Vec<_>>();

    let command = if explain {
        "guard-explain"
    } else {
        "guard-eval"
    };
    let mut value = json!({
        "command": command,
        "schema_version": config.schema_version,
        "guards": results,
        "read_only": true,
        "actuation_authorized": false,
    });
    if explain {
        value["facts"] = Value::Array(fact_view);
        value["missing_fact_semantics"] = Value::String("unknown".into());
    }
    print_json(value)
}

fn read_config(path: &Path) -> Result<GuardConfigV1, Box<dyn Error>> {
    let metadata = std::fs::metadata(path).map_err(|error| {
        IoError::new(
            error.kind(),
            format!("cannot inspect guard config '{}': {error}", path.display()),
        )
    })?;
    if !metadata.is_file() {
        return Err(IoError::new(
            ErrorKind::InvalidData,
            format!(
                "guard config path '{}' is not a regular file",
                path.display()
            ),
        )
        .into());
    }
    if metadata.len() > MAX_GUARD_CONFIG_BYTES as u64 {
        return Err(IoError::new(
            ErrorKind::InvalidData,
            format!(
                "guard config '{}' exceeds {} bytes",
                path.display(),
                MAX_GUARD_CONFIG_BYTES
            ),
        )
        .into());
    }

    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    File::open(path)?
        .take((MAX_GUARD_CONFIG_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_GUARD_CONFIG_BYTES {
        return Err(IoError::new(
            ErrorKind::InvalidData,
            "guard config grew beyond the bounded ingestion limit while reading",
        )
        .into());
    }
    Ok(GuardConfigV1::from_bounded_json(&bytes)?)
}

fn parse_facts(
    assignments: &[String],
    config: &elastic::LoweredGuardConfigV1,
) -> Result<BTreeMap<PredicateKey, TruthValue>, Box<dyn Error>> {
    let mut facts = BTreeMap::new();
    for assignment in assignments {
        let (raw_key, raw_value) = assignment.split_once('=').ok_or_else(|| {
            IoError::new(
                ErrorKind::InvalidInput,
                format!(
                    "invalid --fact '{assignment}'; expected namespace::name=true|false|unknown"
                ),
            )
        })?;
        let (namespace, name) = raw_key.split_once("::").ok_or_else(|| {
            IoError::new(
                ErrorKind::InvalidInput,
                format!("invalid predicate key '{raw_key}'; expected namespace::name"),
            )
        })?;
        let key = PredicateKey::new(namespace, name)?;
        if config.registry().id(&key).is_none() {
            return Err(IoError::new(
                ErrorKind::InvalidInput,
                format!("--fact references undeclared predicate '{key}'"),
            )
            .into());
        }
        let value = match raw_value {
            "true" => TruthValue::True,
            "false" => TruthValue::False,
            "unknown" => TruthValue::Unknown,
            _ => {
                return Err(IoError::new(
                    ErrorKind::InvalidInput,
                    format!("invalid truth value '{raw_value}'; expected true, false, or unknown"),
                )
                .into())
            }
        };
        if facts.insert(key.clone(), value).is_some() {
            return Err(IoError::new(
                ErrorKind::InvalidInput,
                format!("duplicate --fact assignment for '{key}'"),
            )
            .into());
        }
    }
    Ok(facts)
}

const fn truth_name(value: TruthValue) -> &'static str {
    match value {
        TruthValue::True => "true",
        TruthValue::False => "false",
        TruthValue::Unknown => "unknown",
    }
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
            "elastic-guard-{label}-{}-{stamp}.json",
            std::process::id()
        ))
    }

    fn fixture() -> &'static [u8] {
        br#"{
          "schema_version":1,
          "predicates":[{
            "kind":"observation-threshold",
            "key":{"namespace":"elastic.test","name":"healthy"},
            "signal":{"kind":"builtin","name":"utilization"},
            "comparison":"less-or-equal",
            "threshold":0.8,
            "unit":"fraction",
            "max_age_ms":500
          }],
          "guards":[{
            "scope":{"kind":"resource"},
            "expression":{"op":"atom","predicate":{"namespace":"elastic.test","name":"healthy"}}
          }]
        }"#
    }

    #[test]
    fn fact_parser_is_stable_keyed_and_fail_closed() {
        let config = GuardConfigV1::from_bounded_json(fixture())
            .unwrap()
            .lower()
            .unwrap();
        let facts = parse_facts(&["elastic.test::healthy=true".into()], &config).unwrap();
        assert_eq!(facts.len(), 1);
        assert!(parse_facts(&["elastic.test::missing=true".into()], &config).is_err());
        assert!(parse_facts(
            &[
                "elastic.test::healthy=true".into(),
                "elastic.test::healthy=false".into()
            ],
            &config
        )
        .is_err());
    }

    #[test]
    fn bounded_reader_rejects_oversized_files() {
        let path = temp_path("oversized");
        fs::write(&path, vec![b' '; MAX_GUARD_CONFIG_BYTES + 1]).unwrap();
        assert!(read_config(&path).is_err());
        fs::remove_file(path).unwrap();
    }
}
