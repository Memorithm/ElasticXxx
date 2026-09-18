//! Read-only BE10 Boolean guard operator commands.
//!
//! These commands deliberately lower through the public `elastic` facade.
//! The inspect/evaluate commands never construct runtime components. The guarded
//! planning dry-run uses a declaration-only planning view derived from validated
//! operator configuration; it does not construct a physical resource adapter,
//! enter a runtime cycle, or call validation/actuation. Missing fact values are
//! evaluated as `Unknown` by library-owned semantics.

use std::collections::BTreeMap;
use std::error::Error;
use std::fs::File;
use std::io::{Error as IoError, ErrorKind, Read};
use std::path::Path;
use std::time::Instant;

use elastic::{
    capture_guarded_planning_trace, lower_guarded, BooleanGuardPlanner, CandidateDecisionTrace,
    FactResourceBinding, FactSnapshot, FactSourceId, FreshnessSnapshot, GuardConfigV1,
    GuardedPlanningOutcomeTrace, GuardedResourceSpec, InvariantPrecheckStatus, ObservationEpoch,
    ObservationSnapshot, OperatorConfig, PlannerEpoch, PredicateEvaluationInput,
    PredicateEvaluator, PredicateKey, ResourceGeneration, TransitionMechanism, TruthValue,
    GUARD_CONFIG_SCHEMA_V1, MAX_GUARD_CONFIG_BYTES, MAX_OPERATOR_CONFIG_BYTES,
};
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

pub(crate) fn plan_dry_run(
    operator_config_path: &Path,
    guard_config_path: Option<&Path>,
    resource_id: &str,
) -> CommandResult {
    let operator_config = read_operator_config(operator_config_path)?;
    operator_config.validate()?;
    let view = operator_config.build_planning_view(resource_id)?;

    let embedded = view.guard_config().cloned();
    let (lowered, guard_schema_version, guard_source) = match (guard_config_path, embedded) {
        (Some(_), Some(_)) => {
            return Err(IoError::new(
                ErrorKind::InvalidInput,
                "guard policy is ambiguous: the selected resource already embeds guard_config; remove --guard-config or remove the embedded policy",
            )
            .into())
        }
        (Some(path), None) => {
            let guard_config = read_config(path)?;
            let schema_version = guard_config.schema_version;
            (guard_config.lower()?, schema_version, "external-file")
        }
        (None, Some(lowered)) => (lowered, GUARD_CONFIG_SCHEMA_V1, "operator-config"),
        (None, None) => {
            return Err(IoError::new(
                ErrorKind::InvalidInput,
                "selected resource has no embedded guard_config; provide --guard-config FILE",
            )
            .into())
        }
    };
    let guarded_spec =
        GuardedResourceSpec::new(view.resource_spec().clone(), lowered.guards().to_vec())?;
    let guarded = lower_guarded(&guarded_spec)?;

    // The dry-run derives an observation snapshot only from the validated,
    // configured initial state. It does not materialize a physical adapter and
    // never enters Runtime::cycle or any TransactionalActuator method.
    let context = view.context().clone();
    let observations = view.observations().to_vec();
    let now = Instant::now();
    let observation_snapshot = ObservationSnapshot::new(now, observations);
    let input = PredicateEvaluationInput::new(&context, &observation_snapshot, now);
    let evaluators = lowered
        .predicates()
        .iter()
        .map(|predicate| predicate.evaluator() as &dyn PredicateEvaluator)
        .collect::<Vec<_>>();

    // These counters are local identities for this one immutable dry-run
    // snapshot. They are deliberately not represented as live runtime
    // epochs or a physical resource-generation claim.
    let observation_epoch = ObservationEpoch::new(1);
    let planner_epoch = PlannerEpoch::new(1);
    let resource_generation = ResourceGeneration::new(0);
    let resource = guarded.resource().identity().clone();
    let facts = FactSnapshot::derive(
        FactSourceId::new("elastic-cli:guard-plan-dry-run")?,
        observation_epoch,
        Some(FactResourceBinding::new(
            resource.clone(),
            resource_generation,
        )),
        &input,
        &evaluators,
    )?;
    let freshness = FreshnessSnapshot::new(planner_epoch, observation_epoch)
        .with_resource_generation(resource.clone(), resource_generation);

    let planner = BooleanGuardPlanner::new(view.planner());
    let decision =
        planner.propose_transition_detailed_with_context(&guarded, &context, &facts, &freshness)?;
    let trace =
        capture_guarded_planning_trace(&guarded, &context, &facts, &freshness, &decision, &[])?;
    let decision_trace: Value = serde_json::from_str(&trace.decision_trace().to_bounded_json()?)?;
    let precheck = trace.invariant_precheck();

    print_json(json!({
        "command": "guard-plan-dry-run",
        "operator_config_version": operator_config.version,
        "guard_schema_version": guard_schema_version,
        "guard_config_source": guard_source,
        "resource_id": resource.as_str(),
        "observation_source": "operator-config-declared-initial-state",
        "freshness_identity_scope": "dry-run-local",
        "freshness": {
            "planner_epoch": planner_epoch.get(),
            "observation_epoch": observation_epoch.get(),
            "resource_generation": resource_generation.get(),
        },
        "pruning": {
            "eligible": trace.decision_trace().eligible().len(),
            "rejected": trace.decision_trace().rejected().len(),
            "unknown": trace.decision_trace().unknown().len(),
            "fingerprint": trace.pruning_report_fingerprint().to_string(),
        },
        "numeric_outcome": render_guarded_outcome(trace.planning_outcome()),
        "planning_context_fingerprint": trace.planning_context_fingerprint().to_string(),
        "invariant_precheck": {
            "status": invariant_precheck_status_name(precheck.status()),
            "entries": precheck.entries(),
            "true": precheck.true_count(),
            "false": precheck.false_count(),
            "unknown": precheck.unknown_count(),
            "authoritative_validation": false,
        },
        "decision_trace": decision_trace,
        "read_only": true,
        "actuation_authorized": false,
        "trusted_validation_performed": false,
        "runtime_cycle_executed": false,
    }))
}

fn read_operator_config(path: &Path) -> Result<OperatorConfig, Box<dyn Error>> {
    let bytes = read_bounded_file(path, "operator config", MAX_OPERATOR_CONFIG_BYTES)?;
    Ok(OperatorConfig::from_bounded_json(&bytes)?)
}

pub(crate) fn read_bounded_file(
    path: &Path,
    label: &str,
    max_bytes: usize,
) -> Result<Vec<u8>, Box<dyn Error>> {
    let metadata = std::fs::metadata(path).map_err(|error| {
        IoError::new(
            error.kind(),
            format!("cannot inspect {label} '{}': {error}", path.display()),
        )
    })?;
    if !metadata.is_file() {
        return Err(IoError::new(
            ErrorKind::InvalidData,
            format!("{label} path '{}' is not a regular file", path.display()),
        )
        .into());
    }
    if metadata.len() > max_bytes as u64 {
        return Err(IoError::new(
            ErrorKind::InvalidData,
            format!("{label} '{}' exceeds {max_bytes} bytes", path.display()),
        )
        .into());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    File::open(path)?
        .take((max_bytes + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > max_bytes {
        return Err(IoError::new(
            ErrorKind::InvalidData,
            format!("{label} grew beyond the bounded ingestion limit while reading"),
        )
        .into());
    }
    Ok(bytes)
}

fn render_guarded_outcome(outcome: &GuardedPlanningOutcomeTrace) -> Value {
    match outcome {
        GuardedPlanningOutcomeTrace::Candidate(candidate) => json!({
            "kind": "candidate",
            "candidate": render_candidate(candidate),
        }),
        GuardedPlanningOutcomeTrace::NoCandidate => json!({"kind": "no-candidate"}),
        GuardedPlanningOutcomeTrace::Unsupported => json!({"kind": "unsupported"}),
        GuardedPlanningOutcomeTrace::InsufficientEvidence { detail } => json!({
            "kind": "insufficient-evidence",
            "detail": detail,
        }),
    }
}

fn render_candidate(candidate: &CandidateDecisionTrace) -> Value {
    json!({
        "mechanism": mechanism_name(candidate.mechanism()),
        "dimension": candidate.dimension().to_string(),
        "capability_grounded": candidate.capability_grounded(),
        "magnitude": candidate.magnitude(),
    })
}

const fn mechanism_name(mechanism: TransitionMechanism) -> &'static str {
    match mechanism {
        TransitionMechanism::Reinterpret => "reinterpret",
        TransitionMechanism::Reencode => "reencode",
        TransitionMechanism::Recompute => "recompute",
    }
}

const fn invariant_precheck_status_name(status: InvariantPrecheckStatus) -> &'static str {
    match status {
        InvariantPrecheckStatus::NoCandidate => "no-candidate",
        InvariantPrecheckStatus::InvalidCandidate => "invalid-candidate",
        InvariantPrecheckStatus::Passed => "passed-non-authoritative",
        InvariantPrecheckStatus::Rejected => "rejected",
        InvariantPrecheckStatus::InsufficientEvidence => "insufficient-evidence",
    }
}

fn read_config(path: &Path) -> Result<GuardConfigV1, Box<dyn Error>> {
    let bytes = read_bounded_file(path, "guard config", MAX_GUARD_CONFIG_BYTES)?;
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
