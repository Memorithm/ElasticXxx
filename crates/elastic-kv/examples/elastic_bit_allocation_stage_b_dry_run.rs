//! Fail-closed Stage B dry-run entrypoint.
//!
//! Consumes the frozen SmolLM2 preregistration JSON, enforces pinned identities,
//! and emits a schema-versioned measurement plan with explicit not-executed /
//! blocked metric hooks. This does not run NNIS, unlock final-test, invent
//! numeric observations, or authorize an elastic allocator.

use elastic_kv::run_stage_b_dry_run;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

fn default_manifest_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../research/elastic-bit-allocation-stage-b-smollm2-v1.json")
}

fn main() -> ExitCode {
    let path = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(default_manifest_path);

    let json = match fs::read_to_string(&path) {
        Ok(value) => value,
        Err(error) => {
            eprintln!(
                "failed to read Stage B manifest {}: {error}",
                path.display()
            );
            return ExitCode::FAILURE;
        }
    };

    match run_stage_b_dry_run(&json) {
        Ok(report) => {
            println!("{}", report.canonical_json());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("Stage B dry-run failed closed: {error}");
            ExitCode::FAILURE
        }
    }
}
