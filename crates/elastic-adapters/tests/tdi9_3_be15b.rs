use elastic_adapters::{
    Tdi93C3FactsV1, Tdi93C3PredicateV1, TDI93_C3_INTEROP_SCHEMA_V1, TDI93_C3_PREDICATE_COUNT_V1,
    TDI93_C3_SOURCE_COMMIT_V1,
};
use elastic_core::{BooleanGuard, GuardScope, PredicateRegistry, TruthValue};
use std::collections::BTreeMap;

const FIXTURE: &str = include_str!("data/tdi9_3/tdi9.3-c3-carrier-v1.tsv");
const MANIFEST: &str = include_str!("data/tdi9_3/tdi9.3-c3-carrier-v1.manifest");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourceClass {
    Action,
    Unrecoverable,
    Invalid,
}

#[derive(Clone, Copy, Debug)]
struct SourceRow {
    index: usize,
    values: [bool; TDI93_C3_PREDICATE_COUNT_V1],
    class: SourceClass,
    has_action_label: bool,
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

fn source_rows() -> Vec<SourceRow> {
    let mut rows = Vec::new();
    for line in FIXTURE.lines() {
        if line.starts_with('#') {
            continue;
        }
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 4, "malformed TDI fixture row: {line}");
        let index = fields[0].parse::<usize>().expect("bounded row index");
        assert_eq!(index, rows.len(), "TDI fixture rows must remain ordered");
        let bytes = fields[1].as_bytes();
        assert_eq!(bytes.len(), TDI93_C3_PREDICATE_COUNT_V1);
        let values = core::array::from_fn(|bit| match bytes[bit] {
            b'0' => false,
            b'1' => true,
            other => panic!("invalid TDI predicate byte {other:?}"),
        });
        let class = match fields[2] {
            "ACTION" => SourceClass::Action,
            "UNRECOVERABLE" => SourceClass::Unrecoverable,
            "INVALID" => SourceClass::Invalid,
            other => panic!("unsupported TDI carrier class {other:?}"),
        };
        let has_action_label = fields[3] != "-";
        assert_eq!(class == SourceClass::Action, has_action_label);
        rows.push(SourceRow {
            index,
            values,
            class,
            has_action_label,
        });
    }
    rows
}

#[test]
fn pinned_source_contract_is_non_final_and_exact() {
    let manifest = manifest();
    assert_eq!(manifest["schema"], "tdi.elasticxxx-c3-carrier-manifest.v1");
    assert_eq!(manifest["source_repository"], "Memorithm/TDI");
    assert_eq!(manifest["source_commit"], TDI93_C3_SOURCE_COMMIT_V1);
    assert_eq!(manifest["source_pr"], "505");
    assert_eq!(manifest["source_schema"], TDI93_C3_INTEROP_SCHEMA_V1);
    assert_eq!(manifest["claim_boundary"], "non-final-representation-only");
    assert_eq!(
        manifest["unknown_policy"],
        "missing-is-elastic-unknown-and-never-tdi-false"
    );
}

#[test]
fn all_grounded_tdi_rows_roundtrip_through_the_representation_adapter() {
    let rows = source_rows();
    assert_eq!(rows.len(), 512);
    let mut class_counts = [0usize; 3];

    for row in rows {
        assert!(row.index < 512);
        let adapted = Tdi93C3FactsV1::from_binary(row.values);
        assert!(!adapted.has_unknown());
        assert_eq!(adapted.try_binary(), Some(row.values));

        for predicate in Tdi93C3PredicateV1::ALL {
            let expected = if row.values[predicate.index()] {
                TruthValue::True
            } else {
                TruthValue::False
            };
            assert_eq!(adapted.get(predicate), expected);
        }

        match row.class {
            SourceClass::Action => class_counts[0] += 1,
            SourceClass::Unrecoverable => class_counts[1] += 1,
            SourceClass::Invalid => class_counts[2] += 1,
        }
        assert_eq!(row.class == SourceClass::Action, row.has_action_label);
    }

    // This validates the source snapshot partition only. ElasticXxx does not
    // reproduce or authorize TDI action semantics.
    assert_eq!(class_counts, [120, 8, 384]);
}

#[test]
fn every_missing_predicate_is_unknown_and_cannot_be_projected_to_tdi_binary() {
    let keys = Tdi93C3PredicateV1::ALL
        .into_iter()
        .map(|predicate| predicate.key().unwrap())
        .collect::<Vec<_>>();
    let registry = PredicateRegistry::from_keys(keys).unwrap();

    for row in source_rows() {
        for missing in Tdi93C3PredicateV1::ALL {
            let mut optional = row.values.map(Some);
            optional[missing.index()] = None;
            let adapted = Tdi93C3FactsV1::from_optional(optional);
            assert!(adapted.has_unknown());
            assert_eq!(adapted.get(missing), TruthValue::Unknown);
            assert_eq!(adapted.try_binary(), None);

            let missing_key = missing.key().unwrap();
            let guard = BooleanGuard::requires(
                GuardScope::Resource,
                registry.clone(),
                registry.id(&missing_key).unwrap(),
            )
            .unwrap();
            assert_eq!(
                guard.evaluate(&adapted.fact_map().unwrap()).unwrap(),
                TruthValue::Unknown,
                "row {} predicate {} must remain Unknown",
                row.index,
                missing.name()
            );
        }
    }
}
