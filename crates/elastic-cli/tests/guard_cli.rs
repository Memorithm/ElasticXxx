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
