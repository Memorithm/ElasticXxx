use elastic_core::{CapabilitySet, RepresentationEpoch, RepresentationId, RepresentationState};

fn id(value: &str) -> RepresentationId {
    RepresentationId::new(value).expect("valid test representation id")
}

#[test]
fn capability_set_preserves_versions_order_and_counts_after_partial_removal() {
    let mut caps = CapabilitySet::new();
    caps.insert(id("b"), 2);
    caps.insert(id("a"), 3);
    caps.insert(id("a"), 1);
    caps.insert(id("a"), 2);
    caps.insert(id("a"), 2); // duplicate insertion remains idempotent

    assert_eq!(caps.len(), 4);
    assert!(caps.supports_contract(&id("a"), 1));
    assert!(caps.supports_contract(&id("a"), 2));
    assert!(caps.supports_contract(&id("a"), 3));
    assert!(caps.supports_contract(&id("b"), 2));

    let state = RepresentationState::new(id("a"), 2, RepresentationEpoch::new(7));
    assert!(caps.supports(&state));

    let listed: Vec<(String, u32)> = caps
        .iter()
        .map(|(representation, version)| (representation.as_str().to_owned(), version))
        .collect();
    assert_eq!(
        listed,
        vec![
            ("a".to_owned(), 1),
            ("a".to_owned(), 2),
            ("a".to_owned(), 3),
            ("b".to_owned(), 2),
        ]
    );

    assert!(caps.remove(&id("a"), 2));
    assert!(!caps.supports_contract(&id("a"), 2));
    assert!(caps.supports_contract(&id("a"), 1));
    assert!(caps.supports_contract(&id("a"), 3));
    assert_eq!(caps.len(), 3);

    assert!(caps.remove(&id("a"), 1));
    assert!(caps.remove(&id("a"), 3));
    assert!(!caps.remove(&id("a"), 3));
    assert_eq!(caps.len(), 1);
    assert_eq!(
        caps.iter()
            .map(|(representation, version)| (representation.as_str(), version))
            .collect::<Vec<_>>(),
        vec![("b", 2)]
    );
}
