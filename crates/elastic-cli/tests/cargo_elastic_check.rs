use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

fn temp_project(name: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "elastic-cargo-check-{name}-{}-{id}",
        std::process::id()
    ));
    if path.exists() {
        fs::remove_dir_all(&path).expect("remove stale cargo-elastic test project");
    }
    fs::create_dir_all(path.join("src")).expect("create cargo-elastic test project");
    path
}

fn elastic_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("elastic-cli lives below crates")
        .join("elastic")
        .canonicalize()
        .expect("canonical public elastic crate path")
}

#[test]
fn check_reports_stable_elastic_code_and_rustc_primary_span() {
    let project = temp_project("unknown-target");
    let elastic = elastic_path();
    fs::write(
        project.join("Cargo.toml"),
        format!(
            r#"[package]
name = "elastic-diagnostic-fixture"
version = "0.0.0"
edition = "2021"

[dependencies]
elastic = {{ package = "memorithm-elastic", path = {:?} }}
"#,
            elastic
        ),
    )
    .unwrap();
    fs::write(
        project.join("src/main.rs"),
        r#"use elastic::prelude::*;

elastic! {
    document broken {
        resource ram {
            class(configurational);
            allow(capacity);
        }
        policy bad {
            id("bad");
            version(1, 0, 0);
            target(missing);
        }
    }
}

fn main() {}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cargo-elastic"))
        .arg("check")
        .arg("--manifest-path")
        .arg(project.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", project.join("target"))
        .output()
        .expect("run cargo-elastic check");
    assert!(
        !output.status.success(),
        "invalid policy must fail cargo check"
    );

    let stdout = String::from_utf8(output.stdout).expect("cargo-elastic stdout is UTF-8");
    let report: Value = serde_json::from_str(stdout.trim()).unwrap_or_else(|error| {
        panic!("cargo-elastic stdout was not one JSON report: {error}; stdout={stdout:?}")
    });
    assert_eq!(report["schema"], "elastic-diagnostics/v1");
    assert_eq!(report["command"], "check");
    assert_eq!(report["cargo_success"], false);
    assert!(report["elastic_diagnostic_count"].as_u64().unwrap() >= 1);

    let diagnostics = report["diagnostics"].as_array().unwrap();
    let diagnostic = diagnostics
        .iter()
        .find(|entry| entry["code"] == "ELX-LANG-0001")
        .expect("unknown target diagnostic must retain ELX-LANG-0001");
    assert!(diagnostic["message"]
        .as_str()
        .unwrap()
        .contains("targets unknown document resource `missing`"));
    let file = diagnostic["primary_span"]["file"].as_str().unwrap();
    assert!(
        file.ends_with("src/main.rs"),
        "unexpected primary file: {file}"
    );
    assert_eq!(diagnostic["primary_span"]["line_start"], 12);
    assert!(diagnostic["primary_span"]["column_start"].as_u64().unwrap() > 0);

    fs::remove_dir_all(project).expect("remove cargo-elastic test project");
}
