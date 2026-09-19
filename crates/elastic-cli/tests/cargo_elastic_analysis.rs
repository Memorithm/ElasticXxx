use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

fn temp_config(name: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "elastic-cargo-analysis-{name}-{}-{id}.json",
        std::process::id()
    ));
    fs::write(
        &path,
        r#"{
          "schema_version":1,
          "predicates":[{
            "kind":"observation-threshold",
            "key":{"namespace":"elastic.analysis","name":"a"},
            "signal":{"kind":"builtin","name":"utilization"},
            "comparison":"less-or-equal","threshold":0.8,"unit":"fraction","max_age_ms":500
          }],
          "guards":[
            {"scope":{"kind":"resource"},"expression":{"op":"const","value":false}},
            {"scope":{"kind":"resource"},"expression":{"op":"const","value":true}},
            {"scope":{"kind":"resource"},"expression":{"op":"atom","predicate":{"namespace":"elastic.analysis","name":"a"}}},
            {"scope":{"kind":"resource"},"expression":{"op":"atom","predicate":{"namespace":"elastic.analysis","name":"a"}}}
          ]
        }"#,
    )
    .unwrap();
    path
}

#[test]
fn analyze_reports_strong_kleene_findings_with_stable_codes() {
    let config = temp_config("analyze");
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-elastic"))
        .args(["analyze", "--config"])
        .arg(&config)
        .output()
        .expect("run cargo-elastic analyze");
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schema"], "elastic-analysis/v1");
    assert_eq!(report["semantics"], "strong-kleene-exact-v1");
    let diagnostics = report["diagnostics"].as_array().unwrap();
    assert!(diagnostics.iter().any(|d| d["code"] == "ELX-ANALYZE-0001"));
    assert!(diagnostics.iter().any(|d| d["code"] == "ELX-ANALYZE-0002"));
    assert!(diagnostics.iter().any(|d| d["code"] == "ELX-ANALYZE-0003"));
    assert_eq!(report["guards"][0]["dead_guard"]["holds"], true);
    assert_eq!(report["guards"][1]["tautology"]["holds"], true);
    fs::remove_file(config).unwrap();
}

#[test]
fn graph_json_and_dot_preserve_stable_predicate_to_guard_edges() {
    let config = temp_config("graph");
    let json_output = Command::new(env!("CARGO_BIN_EXE_cargo-elastic"))
        .args(["graph", "--config"])
        .arg(&config)
        .output()
        .expect("run cargo-elastic graph json");
    assert!(json_output.status.success());
    let report: Value = serde_json::from_slice(&json_output.stdout).unwrap();
    assert_eq!(report["schema"], "elastic-guard-graph/v1");
    assert_eq!(report["predicates"][0], "elastic.analysis::a");
    let edges = report["edges"].as_array().unwrap();
    assert_eq!(edges.len(), 2);
    assert!(edges.iter().any(|edge| edge["guard"] == 2));
    assert!(edges.iter().any(|edge| edge["guard"] == 3));

    let dot_output = Command::new(env!("CARGO_BIN_EXE_cargo-elastic"))
        .args(["graph", "--config"])
        .arg(&config)
        .args(["--format", "dot"])
        .output()
        .expect("run cargo-elastic graph dot");
    assert!(dot_output.status.success());
    let dot = String::from_utf8(dot_output.stdout).unwrap();
    assert!(dot.contains("digraph elastic_guards"));
    assert!(dot.contains("elastic.analysis::a"));
    assert!(dot.contains("-> \"g:2\""));
    fs::remove_file(config).unwrap();
}
