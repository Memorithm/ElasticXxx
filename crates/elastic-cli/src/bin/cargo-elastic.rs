use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use clap::{Parser, Subcommand, ValueEnum};
use elastic::{
    BoolExpr, ElasticDiagnosticCode, ExactKleeneOracle, ExactOracleLimits, FactSet, GuardConfigV1,
    KleenePropertyReport, KleeneSatisfiabilityReport, PredicateId, PredicateRegistry, TruthValue,
    DEFAULT_EXACT_ORACLE_ASSIGNMENTS, DEFAULT_EXACT_ORACLE_VARIABLES, MAX_GUARD_CONFIG_BYTES,
};
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
    /// Run bounded exact strong-Kleene analysis over a versioned guard configuration.
    Analyze {
        #[arg(long, value_name = "FILE")]
        config: PathBuf,
        #[arg(long, default_value_t = DEFAULT_EXACT_ORACLE_VARIABLES)]
        max_variables: usize,
        #[arg(long, default_value_t = DEFAULT_EXACT_ORACLE_ASSIGNMENTS)]
        max_assignments: usize,
    },
    /// Emit the structural predicate-to-guard graph without evaluating any policy.
    Graph {
        #[arg(long, value_name = "FILE")]
        config: PathBuf,
        #[arg(long, value_enum, default_value_t = GraphFormat::Json)]
        format: GraphFormat,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum GraphFormat {
    Json,
    Dot,
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
        CargoElasticCommand::Analyze {
            config,
            max_variables,
            max_assignments,
        } => tool_result(
            run_analyze(&config, max_variables, max_assignments),
            "analyze",
        ),
        CargoElasticCommand::Graph { config, format } => {
            tool_result(run_graph(&config, format), "graph")
        }
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

fn tool_result(result: Result<(), String>, command: &str) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            println!(
                "{}",
                json!({
                    "schema": "elastic-tool-error/v1",
                    "command": command,
                    "tool_error": error,
                    "read_only": true,
                    "actuation_authorized": false,
                })
            );
            ExitCode::from(2)
        }
    }
}

fn read_guard_config(path: &Path) -> Result<GuardConfigV1, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("cannot stat guard config {}: {error}", path.display()))?;
    let bytes = usize::try_from(metadata.len())
        .map_err(|_| "guard config length does not fit usize".to_owned())?;
    if bytes > MAX_GUARD_CONFIG_BYTES {
        return Err(format!(
            "guard config is {bytes} bytes; maximum is {MAX_GUARD_CONFIG_BYTES}"
        ));
    }
    let raw = fs::read(path)
        .map_err(|error| format!("cannot read guard config {}: {error}", path.display()))?;
    GuardConfigV1::from_bounded_json(&raw).map_err(|error| error.to_string())
}

fn run_analyze(path: &Path, max_variables: usize, max_assignments: usize) -> Result<(), String> {
    let config = read_guard_config(path)?;
    let lowered = config.lower().map_err(|error| error.to_string())?;
    let limits = ExactOracleLimits::new(max_variables, max_assignments)
        .map_err(|error| error.to_string())?;
    let oracle = ExactKleeneOracle::new(limits);
    let mut diagnostics = Vec::new();
    let mut guards = Vec::with_capacity(lowered.guards().len());

    for (index, guard) in lowered.guards().iter().enumerate() {
        let expression = guard.expression();
        let sat = oracle
            .satisfiability(expression)
            .map_err(|error| error.to_string())?;
        let dead = oracle
            .contradiction(expression)
            .map_err(|error| error.to_string())?;
        let tautology = oracle
            .tautology(expression)
            .map_err(|error| error.to_string())?;
        if dead.holds() {
            diagnostics.push(analysis_diagnostic(
                ElasticDiagnosticCode::AnalysisDeadGuard,
                "warning",
                format!("guard {index} can never evaluate explicitly true"),
                json!({"guard": index}),
            ));
        }
        if tautology.holds() {
            diagnostics.push(analysis_diagnostic(
                ElasticDiagnosticCode::AnalysisTautologicalGuard,
                "warning",
                format!("guard {index} is true for every strong-Kleene assignment"),
                json!({"guard": index}),
            ));
        }
        guards.push(json!({
            "index": index,
            "scope": guard.scope().to_string(),
            "expression_fingerprint": guard.fingerprint().to_string(),
            "satisfiability": render_kleene_satisfiability(sat, lowered.registry()),
            "dead_guard": render_kleene_property(dead, lowered.registry()),
            "tautology": render_kleene_property(tautology, lowered.registry()),
        }));
    }

    let mut pairs = Vec::new();
    for lhs in 0..lowered.guards().len() {
        for rhs in (lhs + 1)..lowered.guards().len() {
            let left = lowered.guards()[lhs].expression();
            let right = lowered.guards()[rhs].expression();
            let lr = oracle
                .implication(left, right)
                .map_err(|error| error.to_string())?;
            let rl = oracle
                .implication(right, left)
                .map_err(|error| error.to_string())?;
            let equivalent = oracle
                .equivalence(left, right)
                .map_err(|error| error.to_string())?;
            let exclusive = oracle
                .mutual_exclusion(left, right)
                .map_err(|error| error.to_string())?;
            if equivalent.holds() {
                diagnostics.push(analysis_diagnostic(
                    ElasticDiagnosticCode::AnalysisEquivalentGuards,
                    "warning",
                    format!("guards {lhs} and {rhs} are exactly equivalent"),
                    json!({"lhs": lhs, "rhs": rhs}),
                ));
            } else {
                if lr.holds() {
                    diagnostics.push(analysis_diagnostic(
                        ElasticDiagnosticCode::AnalysisGuardImplication,
                        "info",
                        format!("guard {lhs} explicit-true eligibility implies guard {rhs}"),
                        json!({"lhs": lhs, "rhs": rhs}),
                    ));
                }
                if rl.holds() {
                    diagnostics.push(analysis_diagnostic(
                        ElasticDiagnosticCode::AnalysisGuardImplication,
                        "info",
                        format!("guard {rhs} explicit-true eligibility implies guard {lhs}"),
                        json!({"lhs": rhs, "rhs": lhs}),
                    ));
                }
            }
            if exclusive.holds() {
                diagnostics.push(analysis_diagnostic(
                    ElasticDiagnosticCode::AnalysisMutuallyExclusiveGuards,
                    "info",
                    format!("guards {lhs} and {rhs} are mutually exclusive"),
                    json!({"lhs": lhs, "rhs": rhs}),
                ));
            }
            pairs.push(json!({
                "lhs": lhs,
                "rhs": rhs,
                "lhs_implies_rhs": render_kleene_property(lr, lowered.registry()),
                "rhs_implies_lhs": render_kleene_property(rl, lowered.registry()),
                "equivalent": render_kleene_property(equivalent, lowered.registry()),
                "mutually_exclusive": render_kleene_property(exclusive, lowered.registry()),
            }));
        }
    }

    println!(
        "{}",
        json!({
            "schema": "elastic-analysis/v1",
            "command": "analyze",
            "semantics": "strong-kleene-exact-v1",
            "limits": {
                "max_variables": limits.max_variables(),
                "max_assignments": limits.max_assignments(),
            },
            "diagnostics": diagnostics,
            "guards": guards,
            "pairs": pairs,
            "read_only": true,
            "actuation_authorized": false,
            "trusted_validation_performed": false,
        })
    );
    Ok(())
}

fn run_graph(path: &Path, format: GraphFormat) -> Result<(), String> {
    let config = read_guard_config(path)?;
    let lowered = config.lower().map_err(|error| error.to_string())?;
    let mut edges = BTreeSet::<(String, usize)>::new();
    let mut guards = Vec::with_capacity(lowered.guards().len());
    for (index, guard) in lowered.guards().iter().enumerate() {
        let mut atoms = BTreeSet::new();
        collect_atoms(guard.expression(), &mut atoms);
        for atom in atoms {
            let key = lowered.registry().key(atom).ok_or_else(|| {
                format!(
                    "guard {index} references unregistered predicate {}",
                    atom.index()
                )
            })?;
            edges.insert((key.to_string(), index));
        }
        guards.push(json!({
            "index": index,
            "scope": guard.scope().to_string(),
            "expression_fingerprint": guard.fingerprint().to_string(),
        }));
    }
    let predicates = lowered
        .registry()
        .iter()
        .map(|(_, key)| key.to_string())
        .collect::<Vec<_>>();
    match format {
        GraphFormat::Json => {
            let edge_json = edges
                .iter()
                .map(|(predicate, guard)| json!({"predicate": predicate, "guard": guard}))
                .collect::<Vec<_>>();
            println!(
                "{}",
                json!({
                    "schema": "elastic-guard-graph/v1",
                    "command": "graph",
                    "predicates": predicates,
                    "guards": guards,
                    "edges": edge_json,
                    "read_only": true,
                    "actuation_authorized": false,
                })
            );
        }
        GraphFormat::Dot => {
            println!("digraph elastic_guards {{");
            println!("  rankdir=LR;");
            for predicate in &predicates {
                println!(
                    "  \"p:{}\" [shape=ellipse,label=\"{}\"];",
                    dot_escape(predicate),
                    dot_escape(predicate)
                );
            }
            for (index, guard) in lowered.guards().iter().enumerate() {
                let label = format!("guard {index}\n{}", guard.scope());
                println!(
                    "  \"g:{index}\" [shape=box,label=\"{}\"];",
                    dot_escape(&label)
                );
            }
            for (predicate, guard) in &edges {
                println!("  \"p:{}\" -> \"g:{guard}\";", dot_escape(predicate));
            }
            println!("}}");
        }
    }
    Ok(())
}

fn analysis_diagnostic(
    code: ElasticDiagnosticCode,
    level: &str,
    message: String,
    context: Value,
) -> Value {
    json!({
        "code": code.as_str(),
        "level": level,
        "message": message,
        "context": context,
    })
}

fn render_kleene_satisfiability(
    report: KleeneSatisfiabilityReport,
    registry: &PredicateRegistry,
) -> Value {
    json!({
        "satisfiable": report.is_satisfiable(),
        "variables": report.variables(),
        "assignment_space": report.assignment_space(),
        "assignments_evaluated": report.assignments_evaluated(),
        "witness": report.witness().map(|facts| render_kleene_facts(&facts, registry)),
    })
}

fn render_kleene_property(report: KleenePropertyReport, registry: &PredicateRegistry) -> Value {
    json!({
        "holds": report.holds(),
        "variables": report.variables(),
        "assignment_space": report.assignment_space(),
        "assignments_evaluated": report.assignments_evaluated(),
        "counterexample": report.counterexample().map(|facts| render_kleene_facts(&facts, registry)),
    })
}

fn render_kleene_facts(facts: &FactSet, registry: &PredicateRegistry) -> Value {
    Value::Array(
        registry
            .iter()
            .map(|(id, key)| {
                let truth = facts.get(id).unwrap_or(TruthValue::Unknown);
                json!({
                    "predicate": key.to_string(),
                    "truth": match truth {
                        TruthValue::True => "true",
                        TruthValue::False => "false",
                        TruthValue::Unknown => "unknown",
                    },
                })
            })
            .collect(),
    )
}

fn collect_atoms(expression: &BoolExpr, atoms: &mut BTreeSet<PredicateId>) {
    match expression {
        BoolExpr::Const(_) => {}
        BoolExpr::Atom(id) => {
            atoms.insert(*id);
        }
        BoolExpr::Not(inner) => collect_atoms(inner, atoms),
        BoolExpr::All(expressions) | BoolExpr::Any(expressions) => {
            for expression in expressions {
                collect_atoms(expression, atoms);
            }
        }
        BoolExpr::Xor(left, right) | BoolExpr::Implies(left, right) => {
            collect_atoms(left, atoms);
            collect_atoms(right, atoms);
        }
    }
}

fn dot_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\"', "\\\"")
        .replace('\n', "\\n")
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
