use elastic::{
    Tdi93C3FactsV1, Tdi93C3PredicateV1, TruthValue, TDI93_C3_INTEROP_SCHEMA_V1,
    TDI93_C3_PREDICATE_COUNT_V1,
};

#[test]
fn tdi93_representation_adapter_is_available_through_the_public_facade() {
    assert_eq!(
        TDI93_C3_INTEROP_SCHEMA_V1,
        "tdi9.3.elasticxxx-c3-carrier.v1"
    );
    let values = [Some(false); TDI93_C3_PREDICATE_COUNT_V1];
    let facts = Tdi93C3FactsV1::from_optional(values);
    assert_eq!(
        facts.get(Tdi93C3PredicateV1::VerifierAbsent),
        TruthValue::False
    );
    assert_eq!(
        facts.try_binary(),
        Some([false; TDI93_C3_PREDICATE_COUNT_V1])
    );
}
