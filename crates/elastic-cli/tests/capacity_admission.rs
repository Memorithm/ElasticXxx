use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Stdio};

fn request() -> Value {
    json!({"schema_version":1,"plan_id":"a".repeat(64),"max_concurrency":4,
           "memory_bytes_per_trial":100,"reserve_memory_bytes":100,"max_age_milliseconds":100,
           "observation":{"observation_id":"b".repeat(64),"sensor":"process-contract-fixture/v1",
                          "environment_id":"c".repeat(64),"age_milliseconds":0,
                          "capacity":{"status":"available","cpu_slots":3,"available_memory_bytes":350}}})
}
fn run(input: &[u8]) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_elastic-cli"))
        .arg("admit-capacity")
        .args([
            "--expected-plan-id",
            &"a".repeat(64),
            "--expected-environment-id",
            &"c".repeat(64),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}
#[test]
fn cli_runs_shared_controller_and_emits_rejected_decisions_with_failure_exit() {
    let mut req = request();
    let output = run(&serde_json::to_vec(&req).unwrap());
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["request"], req);
    assert_eq!(report["final_width"], 2);
    assert_eq!(report["committed"], true);
    assert_eq!(report["verification"], "Pass");
    req["observation"]["age_milliseconds"] = json!(101);
    let output = run(&serde_json::to_vec(&req).unwrap());
    assert_eq!(output.status.code(), Some(2));
    let rejected: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rejected["reason"], "stale-observation");
    assert_eq!(rejected["committed"], false);
    assert_eq!(rejected["final_width"], 4);
    req = request();
    req["observation"]["environment_id"] = json!("d".repeat(64));
    let output = run(&serde_json::to_vec(&req).unwrap());
    assert_eq!(output.status.code(), Some(2));
    let rejected: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rejected["reason"], "environment-identity-mismatch");
    assert_eq!(rejected["committed"], false);
}
#[test]
fn cli_rejects_ambiguous_unknown_oversized_and_overflowed_input() {
    let raw = serde_json::to_string(&request()).unwrap();
    for invalid in [
        raw.replacen('{', "{\"schema_version\":1,", 1),
        raw.replacen('{', "{\"unexpected\":1,", 1),
        raw.replace(
            "\"memory_bytes_per_trial\":100",
            "\"memory_bytes_per_trial\":18446744073709551616",
        ),
        " ".repeat(16385),
    ] {
        let output = run(invalid.as_bytes());
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
    }
}
