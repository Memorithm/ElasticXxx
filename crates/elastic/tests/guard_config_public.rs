//! BE10a public-facade contract: durable guard configuration must be usable
//! without depending on internal ElasticXxx crates.

use elastic::prelude::*;

#[test]
fn public_facade_decodes_validates_and_lowers_stable_key_guard_config() {
    let input = br#"{
      "schema_version": 1,
      "predicates": [
        {
          "kind": "observation-threshold",
          "key": {"namespace": "elastic.ram", "name": "pressure-ok"},
          "signal": {"kind": "builtin", "name": "utilization"},
          "comparison": "less-or-equal",
          "threshold": 0.80,
          "unit": "fraction",
          "max_age_ms": 500
        }
      ],
      "guards": [
        {
          "scope": {
            "kind": "transition",
            "mechanism": "reinterpret",
            "dimension": {"kind": "builtin", "name": "capacity"}
          },
          "expression": {
            "op": "atom",
            "predicate": {"namespace": "elastic.ram", "name": "pressure-ok"}
          }
        }
      ]
    }"#;

    let config = GuardConfigV1::from_bounded_json(input).expect("valid public config");
    assert_eq!(config.schema_version, GUARD_CONFIG_SCHEMA_V1);

    let lowered = config.lower().expect("public lowering");
    assert_eq!(lowered.registry().len(), 1);
    assert_eq!(lowered.predicates()[0].unit(), "fraction");
    assert_eq!(lowered.guards().len(), 1);
    assert_eq!(
        lowered.guards()[0].scope(),
        &GuardScope::Transition {
            mechanism: TransitionMechanism::Reinterpret,
            dimension: DimensionId::CAPACITY,
        }
    );
}

#[test]
fn public_facade_rejects_unknown_nested_fields() {
    let input = br#"{
      "schema_version": 1,
      "predicates": [
        {
          "kind": "observation-threshold",
          "key": {"namespace": "elastic.ram", "name": "pressure-ok", "id": 4},
          "signal": {"kind": "builtin", "name": "utilization"},
          "comparison": "less-than",
          "threshold": 0.8,
          "unit": "fraction",
          "max_age_ms": 500
        }
      ],
      "guards": []
    }"#;

    assert!(matches!(
        GuardConfigV1::from_bounded_json(input),
        Err(GuardConfigError::Decode(_))
    ));
}
