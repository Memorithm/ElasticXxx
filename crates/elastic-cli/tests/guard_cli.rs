//! Process-level BE10b guard CLI qualification.

use std::fs;
use std::process::Command;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

fn fixture_path() -> std::path::PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "elastic-guard-cli-{}-{stamp}.json",
        std::process::id()
    ));
    fs::write(
        &path,
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
        }"#,
    )
    .unwrap();
    path
}

fn temp_file(label: &str, bytes: &[u8]) -> std::path::PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "elastic-guard-cli-{label}-{}-{stamp}.json",
        std::process::id()
    ));
    fs::write(&path, bytes).unwrap();
    path
}

fn run(args: &[&str]) -> serde_json::Value {
    let output = Command::new(env!("CARGO_BIN_EXE_elastic-cli"))
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn payload(output: &serde_json::Value) -> &serde_json::Value {
    // EvidenceEnvelope v1 flattens the command payload into the root object.
    output
}

#[test]
fn check_list_and_fingerprint_are_machine_readable_and_non_actuating() {
    let path = fixture_path();
    let path_text = path.to_str().unwrap();

    for command in ["guard-check", "guard-list", "guard-fingerprint"] {
        let output = run(&[command, path_text]);
        let payload = payload(&output);
        assert_eq!(payload["command"], command);
        assert_eq!(payload["read_only"], true);
        assert_eq!(payload["actuation_authorized"], false);
    }

    fs::remove_file(path).unwrap();
}

#[test]
fn exact_guard_analysis_is_bounded_machine_readable_and_non_actuating() {
    let path = fixture_path();
    let output = run(&[
        "guard-analyze",
        path.to_str().unwrap(),
        "--max-variables",
        "4",
        "--max-assignments",
        "16",
    ]);
    let output = payload(&output);
    assert_eq!(output["command"], "guard-analyze");
    assert_eq!(
        output["analysis_semantics"],
        "fully-grounded-boolean-exhaustive-v1"
    );
    assert_eq!(output["solver_backend"], "dependency-free-exact-oracle");
    assert_eq!(output["limits"]["max_variables"], 4);
    assert_eq!(output["limits"]["max_assignments"], 16);
    assert_eq!(output["guards"][0]["satisfiable"], true);
    assert_eq!(output["guards"][0]["contradiction"], false);
    assert_eq!(output["guards"][0]["tautology"], false);
    assert_eq!(output["read_only"], true);
    assert_eq!(output["actuation_authorized"], false);
    assert_eq!(output["trusted_validation_performed"], false);
    fs::remove_file(path).unwrap();
}

#[test]
fn eval_and_explain_preserve_true_false_unknown_semantics() {
    let path = fixture_path();
    let path_text = path.to_str().unwrap();

    let explicit_true = run(&[
        "guard-eval",
        path_text,
        "--fact",
        "elastic.test::healthy=true",
    ]);
    assert_eq!(payload(&explicit_true)["guards"][0]["truth"], "true");

    let missing = run(&["guard-explain", path_text]);
    let missing = payload(&missing);
    assert_eq!(missing["guards"][0]["truth"], "unknown");
    assert_eq!(missing["facts"][0]["truth"], "unknown");
    assert_eq!(missing["facts"][0]["explicit"], false);
    assert_eq!(missing["missing_fact_semantics"], "unknown");
    assert_eq!(missing["actuation_authorized"], false);

    fs::remove_file(path).unwrap();
}

#[test]
fn duplicate_or_undeclared_fact_assignment_fails_closed() {
    let path = fixture_path();
    let path_text = path.to_str().unwrap();

    let duplicate = Command::new(env!("CARGO_BIN_EXE_elastic-cli"))
        .args([
            "guard-eval",
            path_text,
            "--fact",
            "elastic.test::healthy=true",
            "--fact",
            "elastic.test::healthy=false",
        ])
        .output()
        .unwrap();
    assert!(!duplicate.status.success());

    let undeclared = Command::new(env!("CARGO_BIN_EXE_elastic-cli"))
        .args([
            "guard-eval",
            path_text,
            "--fact",
            "elastic.test::missing=true",
        ])
        .output()
        .unwrap();
    assert!(!undeclared.status.success());

    fs::remove_file(path).unwrap();
}

#[test]
fn guarded_plan_dry_run_reports_survivor_rejection_and_unknown_without_actuation() {
    let operator = temp_file(
        "operator",
        br#"{
          "version":1,
          "resources":[{"adapter":"ram","id":"ram","host_total":4096,"min":512,"max":4096,"initial":1024,"max_step":2048}],
          "controllers":[{"resource":"ram","planner":{"kind":"first-grounded"},"forecaster":{"kind":"current-state"},"cadence":{"kind":"one-shot"},"mode":"plan-only"}]
        }"#,
    );

    let cases = [
        (
            "true",
            r#"{"kind":"builtin","name":"free-capacity"}"#,
            "greater-than",
            0.0,
            (1, 0, 0),
            "candidate",
        ),
        (
            "false",
            r#"{"kind":"builtin","name":"free-capacity"}"#,
            "less-than",
            0.0,
            (0, 1, 0),
            "no-candidate",
        ),
        (
            "unknown",
            r#"{"kind":"custom","name":"missing-signal"}"#,
            "greater-than",
            0.0,
            (0, 0, 1),
            "insufficient-evidence",
        ),
    ];

    for (label, signal, comparison, threshold, counts, outcome) in cases {
        let body = format!(
            r#"{{
              "schema_version":1,
              "predicates":[{{
                "kind":"observation-threshold",
                "key":{{"namespace":"elastic.test","name":"gate"}},
                "signal":{signal},
                "comparison":"{comparison}",
                "threshold":{threshold},
                "unit":"bytes",
                "max_age_ms":5000
              }}],
              "guards":[{{
                "scope":{{"kind":"resource"}},
                "expression":{{"op":"atom","predicate":{{"namespace":"elastic.test","name":"gate"}}}}
              }}]
            }}"#
        );
        let guard = temp_file(label, body.as_bytes());
        let output = run(&[
            "guard-plan-dry-run",
            "--operator-config",
            operator.to_str().unwrap(),
            "--guard-config",
            guard.to_str().unwrap(),
            "--resource",
            "ram",
        ]);
        let output = payload(&output);
        assert_eq!(output["command"], "guard-plan-dry-run");
        assert_eq!(output["pruning"]["eligible"], counts.0);
        assert_eq!(output["pruning"]["rejected"], counts.1);
        assert_eq!(output["pruning"]["unknown"], counts.2);
        assert_eq!(output["numeric_outcome"]["kind"], outcome);
        assert_eq!(output["read_only"], true);
        assert_eq!(output["actuation_authorized"], false);
        assert_eq!(output["trusted_validation_performed"], false);
        assert_eq!(output["runtime_cycle_executed"], false);
        fs::remove_file(guard).unwrap();
    }

    fs::remove_file(operator).unwrap();
}

#[test]
fn guarded_plan_dry_run_does_not_materialize_huge_ram_commitment() {
    let operator = temp_file(
        "huge-operator",
        br#"{
          "version":1,
          "resources":[{"adapter":"ram","id":"ram","host_total":17592186044416,"min":1073741824,"max":17592186044416,"initial":8796093022208,"max_step":1073741824}],
          "controllers":[{"resource":"ram","planner":{"kind":"first-grounded"},"forecaster":{"kind":"current-state"},"cadence":{"kind":"one-shot"},"mode":"plan-only"}]
        }"#,
    );
    let guard = fixture_path();

    let output = run(&[
        "guard-plan-dry-run",
        "--operator-config",
        operator.to_str().unwrap(),
        "--guard-config",
        guard.to_str().unwrap(),
        "--resource",
        "ram",
    ]);
    let output = payload(&output);
    assert_eq!(output["read_only"], true);
    assert_eq!(output["actuation_authorized"], false);
    assert_eq!(
        output["observation_source"],
        "operator-config-declared-initial-state"
    );

    fs::remove_file(operator).unwrap();
    fs::remove_file(guard).unwrap();
}

#[test]
fn guarded_plan_dry_run_consumes_embedded_operator_policy_and_rejects_ambiguity() {
    let operator = temp_file(
        "embedded-operator",
        br#"{
          "version":1,
          "resources":[{"adapter":"ram","id":"ram","host_total":4096,"min":512,"max":4096,"initial":1024,"max_step":2048}],
          "controllers":[{
            "resource":"ram",
            "planner":{"kind":"first-grounded"},
            "forecaster":{"kind":"current-state"},
            "cadence":{"kind":"one-shot"},
            "mode":"plan-only",
            "guard_config":{
              "schema_version":1,
              "predicates":[{
                "kind":"observation-threshold",
                "key":{"namespace":"elastic.test","name":"gate"},
                "signal":{"kind":"builtin","name":"free-capacity"},
                "comparison":"greater-than",
                "threshold":0.0,
                "unit":"bytes",
                "max_age_ms":5000
              }],
              "guards":[{
                "scope":{"kind":"resource"},
                "expression":{"op":"atom","predicate":{"namespace":"elastic.test","name":"gate"}}
              }]
            }
          }]
        }"#,
    );

    let output = run(&[
        "guard-plan-dry-run",
        "--operator-config",
        operator.to_str().unwrap(),
        "--resource",
        "ram",
    ]);
    let output = payload(&output);
    assert_eq!(output["guard_config_source"], "operator-config");
    assert_eq!(output["pruning"]["eligible"], 1);
    assert_eq!(output["actuation_authorized"], false);
    assert_eq!(output["runtime_cycle_executed"], false);

    let external = fixture_path();
    let ambiguous = Command::new(env!("CARGO_BIN_EXE_elastic-cli"))
        .args([
            "guard-plan-dry-run",
            "--operator-config",
            operator.to_str().unwrap(),
            "--guard-config",
            external.to_str().unwrap(),
            "--resource",
            "ram",
        ])
        .output()
        .unwrap();
    assert!(!ambiguous.status.success());
    assert!(String::from_utf8_lossy(&ambiguous.stderr).contains("ambiguous"));

    fs::remove_file(operator).unwrap();
    fs::remove_file(external).unwrap();
}

fn operator_fixture_bytes() -> &'static [u8] {
    br#"{
      "version":1,
      "resources":[{"adapter":"ram","id":"ram","host_total":4096,"min":512,"max":4096,"initial":1024,"max_step":2048}],
      "controllers":[{"resource":"ram","planner":{"kind":"first-grounded"},"forecaster":{"kind":"current-state"},"cadence":{"kind":"one-shot"},"mode":"plan-only"}]
    }"#
}

fn free_capacity_guard_bytes() -> &'static [u8] {
    br#"{
      "schema_version":1,
      "predicates":[{
        "kind":"observation-threshold",
        "key":{"namespace":"elastic.test","name":"gate"},
        "signal":{"kind":"builtin","name":"free-capacity"},
        "comparison":"greater-than",
        "threshold":0.0,
        "unit":"bytes",
        "max_age_ms":5000
      }],
      "guards":[{
        "scope":{"kind":"resource"},
        "expression":{"op":"atom","predicate":{"namespace":"elastic.test","name":"gate"}}
      }]
    }"#
}

fn library_plan_summary(
    operator_bytes: &[u8],
    guard_bytes: &[u8],
    resource_id: &str,
) -> (usize, usize, usize, &'static str, String) {
    let operator = elastic::OperatorConfig::from_bounded_json(operator_bytes).unwrap();
    operator.validate().unwrap();
    let view = operator.build_planning_view(resource_id).unwrap();
    let guard_config = elastic::GuardConfigV1::from_bounded_json(guard_bytes).unwrap();
    let lowered = guard_config.lower().unwrap();
    let guarded_spec =
        elastic::GuardedResourceSpec::new(view.resource_spec().clone(), lowered.guards().to_vec())
            .unwrap();
    let guarded = elastic::lower_guarded(&guarded_spec).unwrap();

    let context = view.context().clone();
    let now = Instant::now();
    let observations = elastic::ObservationSnapshot::new(now, view.observations().to_vec());
    let input = elastic::PredicateEvaluationInput::new(&context, &observations, now);
    let evaluators = lowered
        .predicates()
        .iter()
        .map(|predicate| predicate.evaluator() as &dyn elastic::PredicateEvaluator)
        .collect::<Vec<_>>();
    let observation_epoch = elastic::ObservationEpoch::new(1);
    let planner_epoch = elastic::PlannerEpoch::new(1);
    let generation = elastic::ResourceGeneration::new(0);
    let resource = guarded.resource().identity().clone();
    let facts = elastic::FactSnapshot::derive(
        elastic::FactSourceId::new("elastic-cli:guard-plan-dry-run").unwrap(),
        observation_epoch,
        Some(elastic::FactResourceBinding::new(
            resource.clone(),
            generation,
        )),
        &input,
        &evaluators,
    )
    .unwrap();
    let freshness = elastic::FreshnessSnapshot::new(planner_epoch, observation_epoch)
        .with_resource_generation(resource, generation);
    let planner = elastic::BooleanGuardPlanner::new(view.planner());
    let decision = planner
        .propose_transition_detailed_with_context(&guarded, &context, &facts, &freshness)
        .unwrap();
    let trace = elastic::capture_guarded_planning_trace(
        &guarded,
        &context,
        &facts,
        &freshness,
        &decision,
        &[],
    )
    .unwrap();
    let outcome = match trace.planning_outcome() {
        elastic::GuardedPlanningOutcomeTrace::Candidate(_) => "candidate",
        elastic::GuardedPlanningOutcomeTrace::NoCandidate => "no-candidate",
        elastic::GuardedPlanningOutcomeTrace::Unsupported => "unsupported",
        elastic::GuardedPlanningOutcomeTrace::InsufficientEvidence { .. } => {
            "insufficient-evidence"
        }
    };
    (
        trace.decision_trace().eligible().len(),
        trace.decision_trace().rejected().len(),
        trace.decision_trace().unknown().len(),
        outcome,
        trace.planning_context_fingerprint().to_string(),
    )
}

#[test]
fn guarded_plan_process_exit_codes_are_explicit() {
    let operator = temp_file("exit-operator", operator_fixture_bytes());
    let guard = temp_file("exit-guard", free_capacity_guard_bytes());

    let success = Command::new(env!("CARGO_BIN_EXE_elastic-cli"))
        .args([
            "guard-plan-dry-run",
            "--operator-config",
            operator.to_str().unwrap(),
            "--guard-config",
            guard.to_str().unwrap(),
            "--resource",
            "ram",
        ])
        .output()
        .unwrap();
    assert_eq!(success.status.code(), Some(0));

    let failure = Command::new(env!("CARGO_BIN_EXE_elastic-cli"))
        .args([
            "guard-plan-dry-run",
            "--operator-config",
            operator.to_str().unwrap(),
            "--guard-config",
            guard.to_str().unwrap(),
            "--resource",
            "missing",
        ])
        .output()
        .unwrap();
    assert_eq!(failure.status.code(), Some(2));
    assert!(!failure.status.success());

    fs::remove_file(operator).unwrap();
    fs::remove_file(guard).unwrap();
}

#[test]
fn malformed_unknown_nonfinite_and_oversized_inputs_fail_closed_in_process() {
    let malformed = temp_file("malformed-guard", b"{");
    let unknown = temp_file(
        "unknown-guard",
        br#"{
          "schema_version":1,
          "predicates":[],
          "guards":[],
          "unexpected":true
        }"#,
    );
    let nonfinite = temp_file(
        "nonfinite-guard",
        br#"{
          "schema_version":1,
          "predicates":[{
            "kind":"observation-threshold",
            "key":{"namespace":"elastic.test","name":"gate"},
            "signal":{"kind":"builtin","name":"free-capacity"},
            "comparison":"greater-than",
            "threshold":1e999,
            "unit":"bytes",
            "max_age_ms":5000
          }],
          "guards":[]
        }"#,
    );
    let oversized_guard = temp_file(
        "oversized-guard",
        &vec![b' '; elastic::MAX_GUARD_CONFIG_BYTES + 1],
    );

    for path in [&malformed, &unknown, &nonfinite, &oversized_guard] {
        let output = Command::new(env!("CARGO_BIN_EXE_elastic-cli"))
            .args(["guard-check", path.to_str().unwrap()])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2), "path: {}", path.display());
    }

    let guard = temp_file("valid-guard", free_capacity_guard_bytes());
    let unknown_operator = temp_file(
        "unknown-operator",
        br#"{
          "version":1,
          "resources":[],
          "controllers":[],
          "unexpected":true
        }"#,
    );
    let oversized_operator = temp_file(
        "oversized-operator",
        &vec![b' '; elastic::MAX_OPERATOR_CONFIG_BYTES + 1],
    );
    for path in [&unknown_operator, &oversized_operator] {
        let output = Command::new(env!("CARGO_BIN_EXE_elastic-cli"))
            .args([
                "guard-plan-dry-run",
                "--operator-config",
                path.to_str().unwrap(),
                "--guard-config",
                guard.to_str().unwrap(),
                "--resource",
                "ram",
            ])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2), "path: {}", path.display());
    }

    for path in [
        malformed,
        unknown,
        nonfinite,
        oversized_guard,
        guard,
        unknown_operator,
        oversized_operator,
    ] {
        fs::remove_file(path).unwrap();
    }
}

#[test]
fn guarded_plan_cli_matches_public_library_semantics() {
    let operator_bytes = operator_fixture_bytes();
    let guard_bytes = free_capacity_guard_bytes();
    let expected = library_plan_summary(operator_bytes, guard_bytes, "ram");
    let operator = temp_file("equivalence-operator", operator_bytes);
    let guard = temp_file("equivalence-guard", guard_bytes);

    let output = run(&[
        "guard-plan-dry-run",
        "--operator-config",
        operator.to_str().unwrap(),
        "--guard-config",
        guard.to_str().unwrap(),
        "--resource",
        "ram",
    ]);
    let output = payload(&output);
    assert_eq!(output["pruning"]["eligible"], expected.0);
    assert_eq!(output["pruning"]["rejected"], expected.1);
    assert_eq!(output["pruning"]["unknown"], expected.2);
    assert_eq!(output["numeric_outcome"]["kind"], expected.3);
    assert_eq!(output["planning_context_fingerprint"], expected.4);
    assert_eq!(output["read_only"], true);
    assert_eq!(output["actuation_authorized"], false);

    fs::remove_file(operator).unwrap();
    fs::remove_file(guard).unwrap();
}
