use std::ffi::OsString;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, ExitCode, Stdio};

use clap::{Parser, Subcommand};
use serde_json::{json, Value};

const DIAGNOSTIC_SCHEMA: &str = "elastic-diagnostics/v1";

#[derive(Parser, Debug)]
#[command(
    name = "cargo elastic",
    bin_name = "cargo elastic",
    about = "Static checks and developer tooling for the Elastic embedded language"
)]
struct CargoElasticCli {
    #[command(subcommand)]
    command: CargoElasticCommand,
}

#[derive(Subcommand, Debug)]
enum CargoElasticCommand {
    /// Run `cargo check` and emit stable Elastic diagnostics as JSON.
    Check {
        /// Cargo manifest to check. Defaults to Cargo's current-directory discovery.
        #[arg(long)]
        manifest_path: Option<PathBuf>,
        /// Restrict checking to one package.
        #[arg(short = 'p', long)]
        package: Option<String>,
        /// Ask Cargo to check all targets.
        #[arg(long)]
        all_targets: bool,
    },
}

fn main() -> ExitCode {
    let args = normalized_args(std::env::args_os().collect());
    let cli = CargoElasticCli::parse_from(args);
    match cli.command {
        CargoElasticCommand::Check {
            manifest_path,
            package,
            all_targets,
        } => match run_check(manifest_path, package, all_targets) {
            Ok(true) => ExitCode::SUCCESS,
            Ok(false) => ExitCode::from(1),
            Err(error) => {
                let report = json!({
                    "schema": DIAGNOSTIC_SCHEMA,
                    "command": "check",
                    "cargo_success": false,
                    "elastic_diagnostic_count": 0,
                    "diagnostics": [],
                    "tool_error": error,
                });
                println!("{report}");
                ExitCode::from(2)
            }
        },
    }
}

fn normalized_args(mut args: Vec<OsString>) -> Vec<OsString> {
    // Cargo custom subcommands may invoke `cargo-elastic elastic ...`; direct
    // execution uses `cargo-elastic ...`. Accept both shapes deterministically.
    if args.get(1).is_some_and(|arg| arg == "elastic") {
        args.remove(1);
    }
    args
}

fn run_check(
    manifest_path: Option<PathBuf>,
    package: Option<String>,
    all_targets: bool,
) -> Result<bool, String> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let mut command = Command::new(cargo);
    command
        .arg("check")
        .arg("--message-format=json")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    if let Some(path) = manifest_path {
        command.arg("--manifest-path").arg(path);
    }
    if let Some(package) = package {
        command.arg("--package").arg(package);
    }
    if all_targets {
        command.arg("--all-targets");
    }

    let mut child = command
        .spawn()
        .map_err(|error| format!("failed to start cargo check: {error}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "cargo check stdout pipe was unavailable".to_owned())?;
    let mut diagnostics = Vec::new();
    for line in BufReader::new(stdout).lines() {
        let line = line.map_err(|error| format!("failed to read cargo JSON output: {error}"))?;
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            eprintln!("cargo-elastic: ignoring non-JSON cargo output: {line}");
            continue;
        };
        if let Some(diagnostic) = extract_elastic_diagnostic(&message) {
            diagnostics.push(diagnostic);
        }
    }
    let status = child
        .wait()
        .map_err(|error| format!("failed waiting for cargo check: {error}"))?;
    let report = json!({
        "schema": DIAGNOSTIC_SCHEMA,
        "command": "check",
        "cargo_success": status.success(),
        "elastic_diagnostic_count": diagnostics.len(),
        "diagnostics": diagnostics,
    });
    println!("{report}");
    Ok(status.success())
}

fn extract_elastic_diagnostic(message: &Value) -> Option<Value> {
    if message.get("reason")?.as_str()? != "compiler-message" {
        return None;
    }
    let diagnostic = message.get("message")?;
    let text = diagnostic.get("message")?.as_str()?;
    let code = extract_stable_code(text)?;
    let human = strip_stable_code(text).unwrap_or(text);
    let primary = diagnostic
        .get("spans")?
        .as_array()?
        .iter()
        .find(|span| span.get("is_primary").and_then(Value::as_bool) == Some(true));
    let span = primary.map(|span| {
        json!({
            "file": span.get("file_name").and_then(Value::as_str),
            "line_start": span.get("line_start").and_then(Value::as_u64),
            "column_start": span.get("column_start").and_then(Value::as_u64),
            "line_end": span.get("line_end").and_then(Value::as_u64),
            "column_end": span.get("column_end").and_then(Value::as_u64),
        })
    });
    Some(json!({
        "code": code,
        "level": diagnostic.get("level").and_then(Value::as_str).unwrap_or("error"),
        "message": human,
        "primary_span": span,
    }))
}

fn extract_stable_code(message: &str) -> Option<&str> {
    let message = message.trim_start();
    let rest = message.strip_prefix('[')?;
    let end = rest.find(']')?;
    let code = &rest[..end];
    (code.starts_with("ELX-") && code.len() <= 64).then_some(code)
}

fn strip_stable_code(message: &str) -> Option<&str> {
    let message = message.trim_start();
    let rest = message.strip_prefix('[')?;
    let end = rest.find(']')?;
    Some(rest[end + 1..].trim_start())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cargo_subcommand_prefix_is_optional() {
        let direct = normalized_args(vec!["cargo-elastic".into(), "check".into()]);
        assert_eq!(
            direct,
            vec![OsString::from("cargo-elastic"), OsString::from("check")]
        );
        let via_cargo = normalized_args(vec![
            "cargo-elastic".into(),
            "elastic".into(),
            "check".into(),
        ]);
        assert_eq!(
            via_cargo,
            vec![OsString::from("cargo-elastic"), OsString::from("check")]
        );
    }

    #[test]
    fn rustc_json_preserves_elastic_code_and_primary_span() {
        let input = json!({
            "reason": "compiler-message",
            "message": {
                "message": "[ELX-LANG-0003] policy guard references undeclared predicate alias `missing`",
                "level": "error",
                "spans": [{
                    "file_name": "src/main.rs",
                    "line_start": 12,
                    "line_end": 12,
                    "column_start": 30,
                    "column_end": 37,
                    "is_primary": true
                }]
            }
        });
        let diagnostic = extract_elastic_diagnostic(&input).unwrap();
        assert_eq!(diagnostic["code"], "ELX-LANG-0003");
        assert_eq!(
            diagnostic["message"],
            "policy guard references undeclared predicate alias `missing`"
        );
        assert_eq!(diagnostic["primary_span"]["file"], "src/main.rs");
        assert_eq!(diagnostic["primary_span"]["line_start"], 12);
    }

    #[test]
    fn unrelated_rustc_diagnostic_is_not_relabelled() {
        let input = json!({
            "reason": "compiler-message",
            "message": {"message": "mismatched types", "level": "error", "spans": []}
        });
        assert!(extract_elastic_diagnostic(&input).is_none());
    }
}
