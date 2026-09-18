//! Bounded, read-only BE11 exact analysis over configured guards.
//!
//! This command consumes the same strict stable-key guard configuration as the
//! BE10 operator surfaces and lowers through the public `elastic` facade. It
//! performs only dependency-free exhaustive analysis. No result from this module
//! is validation, actuation, commit, publication, or transition authorization.

use std::error::Error;
use std::path::Path;

use elastic::{
    ExactBooleanOracle, ExactOracleLimits, ExactPropertyReport, ExactSatisfiabilityReport, FactSet,
    GuardConfigV1, PredicateRegistry, TruthValue, MAX_GUARD_CONFIG_BYTES,
};
use serde_json::{json, Value};

use crate::evidence::print_json;
use crate::guard_cli::read_bounded_file;

type CommandResult = Result<(), Box<dyn Error>>;

pub(crate) fn analyze(path: &Path, max_variables: usize, max_assignments: usize) -> CommandResult {
    let bytes = read_bounded_file(path, "guard config", MAX_GUARD_CONFIG_BYTES)?;
    let config = GuardConfigV1::from_bounded_json(&bytes)?;
    let lowered = config.lower()?;
    let limits = ExactOracleLimits::new(max_variables, max_assignments)?;
    let oracle = ExactBooleanOracle::new(limits);

    let mut guards = Vec::with_capacity(lowered.guards().len());
    for (index, guard) in lowered.guards().iter().enumerate() {
        let sat = oracle.satisfiability(guard.expression())?;
        let contradiction = oracle.contradiction(guard.expression())?;
        let tautology = oracle.tautology(guard.expression())?;
        guards.push(json!({
            "index": index,
            "scope": guard.scope().to_string(),
            "expression_fingerprint": guard.fingerprint().to_string(),
            "satisfiable": sat.is_satisfiable(),
            "contradiction": contradiction.holds(),
            "tautology": tautology.holds(),
            "satisfiability": render_satisfiability(sat, lowered.registry()),
            "contradiction_check": render_property(contradiction, lowered.registry()),
            "tautology_check": render_property(tautology, lowered.registry()),
        }));
    }

    let mut pairs = Vec::new();
    for lhs in 0..lowered.guards().len() {
        for rhs in (lhs + 1)..lowered.guards().len() {
            let left = lowered.guards()[lhs].expression();
            let right = lowered.guards()[rhs].expression();
            let left_implies_right = oracle.implication(left, right)?;
            let right_implies_left = oracle.implication(right, left)?;
            let equivalent = oracle.equivalence(left, right)?;
            let mutually_exclusive = oracle.mutual_exclusion(left, right)?;
            pairs.push(json!({
                "lhs": lhs,
                "rhs": rhs,
                "lhs_implies_rhs": render_property(left_implies_right, lowered.registry()),
                "rhs_implies_lhs": render_property(right_implies_left, lowered.registry()),
                "equivalent": render_property(equivalent, lowered.registry()),
                "mutually_exclusive": render_property(mutually_exclusive, lowered.registry()),
            }));
        }
    }

    print_json(json!({
        "command": "guard-analyze",
        "schema_version": config.schema_version,
        "analysis_semantics": "fully-grounded-boolean-exhaustive-v1",
        "limits": {
            "max_variables": limits.max_variables(),
            "max_assignments": limits.max_assignments(),
        },
        "guards": guards,
        "pairs": pairs,
        "read_only": true,
        "actuation_authorized": false,
        "trusted_validation_performed": false,
        "solver_backend": "dependency-free-exact-oracle",
    }))
}

fn render_satisfiability(report: ExactSatisfiabilityReport, registry: &PredicateRegistry) -> Value {
    json!({
        "variables": report.variables(),
        "assignment_space": report.assignment_space(),
        "assignments_evaluated": report.assignments_evaluated(),
        "witness": report.witness().map(|facts| render_facts(&facts, registry)),
    })
}

fn render_property(report: ExactPropertyReport, registry: &PredicateRegistry) -> Value {
    json!({
        "holds": report.holds(),
        "variables": report.variables(),
        "assignment_space": report.assignment_space(),
        "assignments_evaluated": report.assignments_evaluated(),
        "counterexample": report
            .counterexample()
            .map(|facts| render_facts(&facts, registry)),
    })
}

fn render_facts(facts: &FactSet, registry: &PredicateRegistry) -> Value {
    let entries = registry
        .iter()
        .filter_map(|(id, key)| {
            let value = facts.get(id).ok()?;
            if value == TruthValue::Unknown {
                return None;
            }
            Some(json!({
                "predicate": key.to_string(),
                "truth": match value {
                    TruthValue::True => "true",
                    TruthValue::False => "false",
                    TruthValue::Unknown => unreachable!("filtered above"),
                },
            }))
        })
        .collect::<Vec<_>>();
    Value::Array(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> &'static [u8] {
        br#"{
          "schema_version":1,
          "predicates":[{
            "kind":"observation-threshold",
            "key":{"namespace":"elastic.test","name":"a"},
            "signal":{"kind":"builtin","name":"utilization"},
            "comparison":"less-or-equal","threshold":0.8,"unit":"fraction","max_age_ms":500
          },{
            "kind":"observation-threshold",
            "key":{"namespace":"elastic.test","name":"b"},
            "signal":{"kind":"builtin","name":"free-capacity"},
            "comparison":"greater-or-equal","threshold":1.0,"unit":"bytes","max_age_ms":500
          }],
          "guards":[
            {"scope":{"kind":"resource"},"expression":{"op":"atom","predicate":{"namespace":"elastic.test","name":"a"}}},
            {"scope":{"kind":"resource"},"expression":{"op":"all","expressions":[
              {"op":"atom","predicate":{"namespace":"elastic.test","name":"a"}},
              {"op":"atom","predicate":{"namespace":"elastic.test","name":"b"}}
            ]}}
          ]
        }"#
    }

    #[test]
    fn exact_reports_preserve_stable_keys() {
        let config = GuardConfigV1::from_bounded_json(fixture()).unwrap();
        let lowered = config.lower().unwrap();
        let oracle = ExactBooleanOracle::default();
        let sat = oracle
            .satisfiability(lowered.guards()[1].expression())
            .unwrap();
        let rendered = render_satisfiability(sat, lowered.registry());
        let witness = rendered["witness"].as_array().unwrap();
        assert!(witness
            .iter()
            .any(|entry| entry["predicate"] == "elastic.test::a"));
        assert!(witness
            .iter()
            .any(|entry| entry["predicate"] == "elastic.test::b"));
    }

    #[test]
    fn pairwise_analysis_detects_one_way_implication() {
        let config = GuardConfigV1::from_bounded_json(fixture()).unwrap();
        let lowered = config.lower().unwrap();
        let oracle = ExactBooleanOracle::default();
        let a = lowered.guards()[0].expression();
        let a_and_b = lowered.guards()[1].expression();
        assert!(oracle.implication(a_and_b, a).unwrap().holds());
        assert!(!oracle.implication(a, a_and_b).unwrap().holds());
        assert!(!oracle.equivalence(a, a_and_b).unwrap().holds());
    }
}
