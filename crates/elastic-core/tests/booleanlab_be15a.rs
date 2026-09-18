use std::collections::BTreeMap;

use elastic_core::{
    BoolExpr, CompiledGuard, ExactBooleanOracle, FactSet, MultiwordCompiledGuard, MultiwordFactSet,
    PredicateId, SymbolicBackend, SymbolicBackendConfig, SymbolicBackendResult, TruthValue,
};
const FIXTURE: &str = include_str!("data/booleanlab/exact-vectors-v1.tsv");
const MANIFEST: &str = include_str!("data/booleanlab/exact-vectors-v1.manifest");
const EXPECTED_GENERATOR_COMMIT: &str = "7fd929a62cfc219a525b21ff02801fbc8bcb012e";
const MAX_INPUT_BITS: u32 = 8;
const MAX_RPN_TOKENS: usize = 64;

#[derive(Debug)]
struct Vector<'a> {
    case_id: &'a str,
    input_bits: u32,
    expression_rpn: &'a str,
    truth_table: &'a str,
    satisfiable: bool,
    tautology: bool,
    contradiction: bool,
}

fn manifest() -> BTreeMap<&'static str, &'static str> {
    MANIFEST
        .lines()
        .map(|line| {
            line.split_once('=')
                .expect("manifest line must be key=value")
        })
        .collect()
}

fn parse_bool(value: &str) -> bool {
    match value {
        "true" => true,
        "false" => false,
        other => panic!("invalid fixture Boolean {other:?}"),
    }
}

fn vectors() -> Vec<Vector<'static>> {
    let mut lines = FIXTURE.lines();
    let header = lines.next().expect("fixture header");
    assert_eq!(
        header,
        "schema_version\tcase_id\tshape_class\tinput_bits\texpression_rpn\ttruth_table_row_major\talgebraic_degree\tnonlinearity\tbalanced\tcorrelation_immunity\tsatisfiable\ttautology\tcontradiction\tbooleanlab_fingerprint"
    );

    lines
        .map(|line| {
            let fields: Vec<_> = line.split('\t').collect();
            assert_eq!(fields.len(), 14, "malformed fixture row: {line}");
            assert_eq!(fields[0], "1", "unsupported fixture schema");
            let input_bits = fields[3].parse().expect("bounded input bit count");
            assert!(input_bits <= MAX_INPUT_BITS);
            Vector {
                case_id: fields[1],
                input_bits,
                expression_rpn: fields[4],
                truth_table: fields[5],
                satisfiable: parse_bool(fields[10]),
                tautology: parse_bool(fields[11]),
                contradiction: parse_bool(fields[12]),
            }
        })
        .collect()
}

fn pop(stack: &mut Vec<BoolExpr>, token: &str) -> BoolExpr {
    stack
        .pop()
        .unwrap_or_else(|| panic!("RPN stack underflow at {token:?}"))
}

fn parse_rpn(source: &str, input_bits: u32) -> BoolExpr {
    let tokens: Vec<_> = source.split_whitespace().collect();
    assert!(tokens.len() <= MAX_RPN_TOKENS);
    let mut stack = Vec::with_capacity(tokens.len());
    for token in tokens {
        match token {
            "true" => stack.push(BoolExpr::Const(true)),
            "false" => stack.push(BoolExpr::Const(false)),
            "not" => {
                let value = pop(&mut stack, token);
                stack.push(BoolExpr::negate(value));
            }
            "and" | "or" | "xor" | "implies" => {
                let rhs = pop(&mut stack, token);
                let lhs = pop(&mut stack, token);
                let expr = match token {
                    "and" => BoolExpr::all([lhs, rhs]),
                    "or" => BoolExpr::any([lhs, rhs]),
                    "xor" => BoolExpr::Xor(Box::new(lhs), Box::new(rhs)),
                    "implies" => BoolExpr::Implies(Box::new(lhs), Box::new(rhs)),
                    _ => unreachable!(),
                };
                stack.push(expr);
            }
            predicate if predicate.starts_with('p') => {
                let index: u32 = predicate[1..]
                    .parse()
                    .unwrap_or_else(|_| panic!("invalid predicate token {predicate:?}"));
                assert!(index < input_bits, "predicate outside declared domain");
                stack.push(BoolExpr::atom(PredicateId::new(index)));
            }
            other => panic!("unknown RPN token {other:?}"),
        }
    }
    assert_eq!(stack.len(), 1, "RPN expression must produce one value");
    stack.pop().unwrap()
}

fn fact_sets(input_bits: u32, assignment: u32) -> (FactSet, MultiwordFactSet) {
    let mut scalar = FactSet::new();
    let mut multiword = MultiwordFactSet::new(input_bits).expect("bounded fixture capacity");
    for index in 0..input_bits {
        let value = if assignment & (1 << index) != 0 {
            TruthValue::True
        } else {
            TruthValue::False
        };
        let id = PredicateId::new(index);
        scalar.set(id, value).unwrap();
        multiword.set(id, value).unwrap();
    }
    (scalar, multiword)
}

struct ExactTestSymbolicBackend;

impl SymbolicBackend for ExactTestSymbolicBackend {
    fn backend_id(&self) -> &'static str {
        "be15a-exact-test-oracle"
    }

    fn solve(
        &self,
        expression: &BoolExpr,
        _config: SymbolicBackendConfig,
    ) -> SymbolicBackendResult {
        match ExactBooleanOracle::default().satisfiability(expression) {
            Ok(report) if report.is_satisfiable() => SymbolicBackendResult::Sat,
            Ok(_) => SymbolicBackendResult::Unsat,
            Err(_) => SymbolicBackendResult::Unknown,
        }
    }
}

#[test]
fn snapshot_integrity_and_provenance_are_pinned() {
    let manifest = manifest();
    assert_eq!(
        manifest["schema"],
        "booleanlab.elasticxxx-exact-vectors-manifest.v1"
    );
    assert_eq!(manifest["vector_schema_version"], "1");
    assert_eq!(manifest["generator_repository"], "Memorithm/BooleanLab");
    assert_eq!(manifest["generator_commit"], EXPECTED_GENERATOR_COMMIT);
    assert_eq!(
        manifest["fixture_sha256"],
        "b27f14593c908a5d3ccdb2d410d64d3420d1f9c42da593621e9ca7a6f339dd59"
    );
    assert_eq!(manifest["row_order"], "p0-least-significant-assignment-bit");
    assert_eq!(manifest["claim_boundary"], "exact-test-vectors-only");
}

#[test]
fn booleanlab_vectors_match_all_elastic_evaluation_surfaces() {
    let vectors = vectors();
    assert!(!vectors.is_empty());

    for vector in vectors {
        let expression = parse_rpn(vector.expression_rpn, vector.input_bits);
        let compiled = CompiledGuard::compile(&expression).unwrap();
        let multiword = MultiwordCompiledGuard::compile(&expression, vector.input_bits).unwrap();
        let assignments = 1_u32 << vector.input_bits;
        assert_eq!(vector.truth_table.len(), assignments as usize);

        for assignment in 0..assignments {
            let expected = match vector.truth_table.as_bytes()[assignment as usize] {
                b'0' => TruthValue::False,
                b'1' => TruthValue::True,
                other => panic!("invalid truth-table byte {other:?}"),
            };
            let (facts, multiword_facts) = fact_sets(vector.input_bits, assignment);
            assert_eq!(
                expression.evaluate(&facts).unwrap(),
                expected,
                "generic evaluator mismatch for {} assignment {assignment}",
                vector.case_id
            );
            assert_eq!(
                compiled.evaluate(&facts).unwrap(),
                expected,
                "compiled evaluator mismatch for {} assignment {assignment}",
                vector.case_id
            );
            assert_eq!(
                multiword.evaluate(&multiword_facts).unwrap(),
                expected,
                "multiword evaluator mismatch for {} assignment {assignment}",
                vector.case_id
            );
        }

        let oracle = ExactBooleanOracle::default();
        assert_eq!(
            oracle.satisfiability(&expression).unwrap().is_satisfiable(),
            vector.satisfiable,
            "satisfiability mismatch for {}",
            vector.case_id
        );
        assert_eq!(
            oracle.tautology(&expression).unwrap().holds(),
            vector.tautology,
            "tautology mismatch for {}",
            vector.case_id
        );
        assert_eq!(
            !vector.satisfiable, vector.contradiction,
            "fixture contradiction metadata is inconsistent for {}",
            vector.case_id
        );
        let symbolic =
            ExactTestSymbolicBackend.solve(&expression, SymbolicBackendConfig::default());
        let expected_symbolic = if vector.satisfiable {
            SymbolicBackendResult::Sat
        } else {
            SymbolicBackendResult::Unsat
        };
        assert_eq!(
            symbolic, expected_symbolic,
            "symbolic boundary mismatch for {}",
            vector.case_id
        );
    }
}
