//! Process-level BE10b guard CLI qualification.

use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

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
