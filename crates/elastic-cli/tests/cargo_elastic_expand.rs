use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

fn temp_source(name: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "elastic-expand-{name}-{}-{id}.rs",
        std::process::id()
    ));
    fs::write(
        &path,
        r#"use elastic::prelude::*;

elastic! {
    pub resource pool {
        class(stateful);
        id("expanded-pool");
        allow(concurrency);
        observe(queue_depth);
    }
}

mod nested {
    elastic::elastic! {
        resource cache {
            class(representational);
            allow(representation);
        }
    }
}
"#,
    )
    .unwrap();
    path
}

#[test]
fn expand_uses_shared_expander_and_emits_stable_rust_without_nightly() {
    let source = temp_source("rust");
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-elastic"))
        .args(["expand", "--source"])
        .arg(&source)
        .output()
        .expect("run cargo-elastic expand");
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("pub mod pool"));
    assert!(stdout.contains("pub fn resource_spec"));
    assert!(stdout.contains("expanded-pool"));
    assert!(stdout.contains("mod cache"));
    assert!(!stdout.contains("RUSTC_BOOTSTRAP"));
    fs::remove_file(source).unwrap();
}

#[test]
fn expand_json_reports_each_elastic_invocation() {
    let source = temp_source("json");
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-elastic"))
        .args(["expand", "--source"])
        .arg(&source)
        .args(["--format", "json"])
        .output()
        .expect("run cargo-elastic expand json");
    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schema"], "elastic-expansion/v1");
    assert_eq!(report["elastic_invocation_count"], 2);
    assert_eq!(report["expander"], "shared-elastic-language-syntax");
    assert_eq!(report["rustc_nightly_required"], false);
    assert!(report["expansions"][0]["rust"]
        .as_str()
        .unwrap()
        .contains("pub mod pool"));
    fs::remove_file(source).unwrap();
}
