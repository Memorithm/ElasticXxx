//! Integration tests for the representation-layer bridge.

use elastic_core::resource::{
    AdmissibleTransition, ContractId, DeclarationError, DimensionId, Invariant, InvariantKind,
    LogicalResourceId, RepresentationalDeclaration, ResourceClassId, ResourceSpec,
};
use elastic_core::{
    CapabilitySet, FrontierError, RepresentationEpoch, RepresentationId, RepresentationState,
    TransitionAttestations, TransitionError,
    TransitionMechanism::{Reencode, Reinterpret},
    VersionFrontier,
};

fn raw_spec() -> ResourceSpec {
    ResourceSpec::builder(
        ResourceClassId::REPRESENTATIONAL,
        LogicalResourceId::new("session-kv").unwrap(),
    )
    .allow(DimensionId::REPRESENTATION)
    .preserve(
        Invariant::new(InvariantKind::UpholdContract(
            ContractId::new("kv.reuse-contract").unwrap(),
        ))
        .along(DimensionId::REPRESENTATION),
    )
    .admit(AdmissibleTransition::new(
        Reencode,
        DimensionId::REPRESENTATION,
    ))
    .require_capability(CapabilityRequirement::new(
        Reencode,
        DimensionId::REPRESENTATION,
    ))
    .build()
    .unwrap()
}

use elastic_core::resource::CapabilityRequirement;

fn raw_state(epoch: u64) -> RepresentationState {
    RepresentationState::new(
        RepresentationId::new("kv.raw").unwrap(),
        1,
        RepresentationEpoch::new(epoch),
    )
}

fn int4_state(epoch: u64) -> RepresentationState {
    RepresentationState::new(
        RepresentationId::new("kv.int4").unwrap(),
        1,
        RepresentationEpoch::new(epoch),
    )
}

fn declaration() -> RepresentationalDeclaration {
    RepresentationalDeclaration::new(
        raw_spec(),
        vec![
            (RepresentationId::new("kv.raw").unwrap(), 1),
            (RepresentationId::new("kv.int4").unwrap(), 1),
        ],
    )
    .unwrap()
}

#[test]
fn valid_specialization_derives_targets_with_core_epoch_policy() {
    let declaration = declaration();
    let current = raw_state(7);

    let target = declaration
        .derive_target(
            &current,
            &RepresentationId::new("kv.int4").unwrap(),
            1,
            Reencode,
        )
        .unwrap();

    assert_eq!(target.id.as_str(), "kv.int4");
    assert_eq!(target.epoch.get(), 8, "contract change must advance epoch");
    assert!(declaration.supports(&target));
    assert!(declaration.supports(&current));
}

#[test]
fn specialization_requires_elastic_representation_dimension() {
    let rigid = ResourceSpec::builder(
        ResourceClassId::STATEFUL,
        LogicalResourceId::new("pool").unwrap(),
    )
    .allow(DimensionId::PARALLELISM)
    .build()
    .unwrap();
    assert_eq!(
        RepresentationalDeclaration::new(rigid, [(RepresentationId::new("x").unwrap(), 1)]),
        Err(DeclarationError::RepresentationNotElastic)
    );
}

#[test]
fn specialization_requires_allowed_contracts() {
    assert_eq!(
        RepresentationalDeclaration::new(raw_spec(), []),
        Err(DeclarationError::NoAllowedContracts)
    );
}

#[test]
fn unallowed_contracts_and_unadmitted_mechanisms_are_rejected() {
    let declaration = declaration();
    let current = raw_state(7);

    assert_eq!(
        declaration.derive_target(
            &current,
            &RepresentationId::new("kv.int8").unwrap(),
            1,
            Reencode
        ),
        Err(DeclarationError::UnsupportedRepresentation {
            id: RepresentationId::new("kv.int8").unwrap(),
            schema_version: 1,
        })
    );

    assert_eq!(
        declaration.derive_target(
            &current,
            &RepresentationId::new("kv.int4").unwrap(),
            1,
            Reinterpret,
        ),
        Err(DeclarationError::MechanismNotAdmitted {
            mechanism: Reinterpret
        })
    );

    // Wrong schema version of an otherwise-allowed id is also unsupported.
    assert!(matches!(
        declaration.derive_target(
            &current,
            &RepresentationId::new("kv.int4").unwrap(),
            2,
            Reencode
        ),
        Err(DeclarationError::UnsupportedRepresentation { .. })
    ));
}

#[test]
fn frontier_lifecycle_respects_declaration_and_trusted_capabilities() {
    let declaration = declaration();
    let mut frontier = VersionFrontier::new(raw_state(3));

    // Proposal derived from declaration admission rules.
    let target = declaration
        .propose_on(
            &mut frontier,
            &RepresentationId::new("kv.int4").unwrap(),
            1,
            Reencode,
        )
        .unwrap();

    // Structural validation fails without trusted capabilities...
    assert_eq!(
        frontier.validate_pending(&CapabilitySet::new(), TransitionAttestations::default()),
        Err(FrontierError::Core(TransitionError::UnsupportedTarget {
            id: RepresentationId::new("kv.int4").unwrap(),
            schema_version: 1,
        }))
    );

    // ...and rollback leaves the committed state untouched.
    assert!(frontier.rollback().is_some());
    assert_eq!(frontier.committed(), &raw_state(3));

    // Re-propose; this time the trusted boundary supplies capabilities and
    // attestations, and the commit advances the frontier.
    let target_again = declaration
        .propose_on(
            &mut frontier,
            &RepresentationId::new("kv.int4").unwrap(),
            1,
            Reencode,
        )
        .unwrap();
    assert_eq!(target_again, target);

    let mut caps = CapabilitySet::new();
    caps.insert(target.id.clone(), target.schema_version);
    let committed = frontier
        .commit(
            &caps,
            TransitionAttestations::default().attest_reencoder_available(),
        )
        .unwrap();
    assert_eq!(committed, &int4_state(4));
    assert!(declaration.supports(committed));
}

#[test]
fn multiple_schema_versions_of_one_representation_are_independent() {
    let spec = raw_spec();
    let declaration = RepresentationalDeclaration::new(
        spec,
        vec![
            (RepresentationId::new("kv.int4").unwrap(), 1),
            (RepresentationId::new("kv.int4").unwrap(), 2),
            (RepresentationId::new("kv.raw").unwrap(), 1),
        ],
    )
    .unwrap();

    assert_eq!(declaration.allowed_contracts().count(), 3);
    let current = raw_state(7);

    for version in [1, 2] {
        let target = declaration
            .derive_target(
                &current,
                &RepresentationId::new("kv.int4").unwrap(),
                version,
                Reencode,
            )
            .unwrap();
        assert_eq!(target.schema_version, version);
        assert!(declaration.supports(&target));
    }

    // Version 3 was never declared.
    assert_eq!(
        declaration.derive_target(
            &current,
            &RepresentationId::new("kv.int4").unwrap(),
            3,
            Reencode
        ),
        Err(DeclarationError::UnsupportedRepresentation {
            id: RepresentationId::new("kv.int4").unwrap(),
            schema_version: 3,
        })
    );
}
