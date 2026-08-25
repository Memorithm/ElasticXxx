//! Example C — end-to-end representational resource flow.
//!
//! Logical KV resource
//!   → declare representation elastic + preserve reuse contract
//!   → declare allowed representations
//!   → lower to normalized EIR and validate
//!   → map into the existing representation transition contract
//!   → `VersionFrontier` propose → validate → commit / rollback
//!
//! Layer separation: this example performs **planning metadata** work and
//! **structural validation** only. No physical re-encoding is simulated or
//! claimed; a real codec execution belongs to a trusted adapter boundary that
//! would supply the same attestations from actual capability discovery.

use elastic::prelude::*;
use elastic_core::resource::RepresentationalDeclaration;
use elastic_core::{
    CapabilitySet, EvidenceKind, EvidenceToken, IssuerId, TransitionAttestations, VersionFrontier,
};
use elastic_kv::{
    build_epoch_delta, KeyEncodingPipeline, KeyTransformScope, KvPageDescriptor, KvPageId,
    KvPrecision, KvRecoverySource, KvResidency, KvTargetMaterialization, PageTransition,
};

#[derive(ElasticResource)]
#[elastic(
    class(representational),
    id("session-kv"),
    allow(representation),
    preserve(contract("kv.reuse-contract") along representation),
    optimize(latency),
    admit(reencode @ representation),
    admit(reinterpret @ representation),
    capability(reencode @ representation),
    observe(free_capacity)
)]
struct SessionKv;

fn page(id: u64) -> KvPageDescriptor {
    KvPageDescriptor {
        page: KvPageId::new(id),
        representation: RepresentationState::new(
            RepresentationId::new("kv.raw").unwrap(),
            1,
            RepresentationEpoch::new(4),
        ),
        precision: KvPrecision::F16,
        residency: KvResidency::Accelerator,
        key_transform_scope: KeyTransformScope::TokenStable,
        key_encoding_pipeline: KeyEncodingPipeline::TransformThenCodec,
        recovery_source: KvRecoverySource::StoredCanonicalRaw,
    }
}

use elastic_core::{RepresentationEpoch, RepresentationId, RepresentationState};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Declaration: what may change, what must hold, which targets exist.
    let declaration = RepresentationalDeclaration::new(
        SessionKv::resource_spec()?,
        vec![
            (RepresentationId::new("kv.raw")?, 1),
            (RepresentationId::new("kv.int4.grouped")?, 1),
        ],
    )?;

    // 2. Normalized Elastic intent → EIR → validation.
    let document = lower(declaration.spec())?;
    assert_eq!(document.schema_version().get(), 1);
    println!("eir: {} {}", document, document.fingerprint());

    // 3. Current materialization of two pages of the logical resource.
    let pages = [page(1), page(2)];

    // 4. Map intent into the existing representation transition contract:
    //    derive the admissible target state (epoch policy comes from core).
    let target = declaration.derive_target(
        &pages[0].representation,
        &RepresentationId::new("kv.int4.grouped")?,
        1,
        TransitionMechanism::Reencode,
    )?;

    // 5. The trusted boundary discovers capabilities and issues evidence for
    //    exactly this transition; tokens bind claims to the fingerprint.
    let mut capabilities = CapabilitySet::new();
    capabilities.insert(target.id.clone(), target.schema_version);
    let probe = elastic_core::RepresentationTransition {
        from: pages[0].representation.clone(),
        to: target.clone(),
        mechanism: TransitionMechanism::Reencode,
    };
    let issuer = IssuerId::new("trusted-validator")?;
    let token = EvidenceToken::issue(issuer, EvidenceKind::ReencoderAvailable, &probe);
    let attestations = TransitionAttestations::from_evidence([&token], &probe);

    // 6. KV adapter-level structural validation for a reusable view change,
    //    consuming the provenance-carrying evidence.
    let mut plans = Vec::new();
    for descriptor in &pages {
        plans.push(descriptor.validate_reusable_representation_change(
            target.clone(),
            TransitionMechanism::Reencode,
            &capabilities,
            attestations,
            KvTargetMaterialization::new(
                KeyTransformScope::TokenStable,
                KeyEncodingPipeline::TransformThenCodec,
                KvRecoverySource::StoredCanonicalRaw,
            ),
        )?);
    }

    // 7. VersionFrontier lifecycle on the logical resource:
    //    propose → validate → commit (or rollback).
    let mut frontier = VersionFrontier::new(pages[0].representation.clone());
    let staged = declaration.propose_on(
        &mut frontier,
        &RepresentationId::new("kv.int4.grouped")?,
        1,
        TransitionMechanism::Reencode,
    )?;
    frontier.validate_pending(&capabilities, attestations)?;
    let committed = frontier.commit(&capabilities, attestations)?.clone();
    println!("committed logical view: {committed} (staged {staged})");

    // Rollback path demonstration: an unsupported target never touches the
    // committed state.
    let rejected = declaration.propose_on(
        &mut frontier,
        &RepresentationId::new("kv.fp8.exotic")?,
        1,
        TransitionMechanism::Reencode,
    );
    assert!(rejected.is_err());
    assert_eq!(frontier.committed(), &committed);

    // 8. Deterministic epoch delta over the migrated pages.
    let transitions: Vec<_> = pages
        .iter()
        .zip(plans.iter())
        .map(|(descriptor, plan)| PageTransition {
            page: descriptor.page,
            plan: plan.clone(),
        })
        .collect();
    let delta = build_epoch_delta(&transitions)?;
    println!(
        "delta: {} → {} pages={:?}",
        delta.from,
        delta.to,
        delta
            .migrated_pages
            .iter()
            .map(|p| p.get())
            .collect::<Vec<_>>()
    );

    println!("NOTE: no physical re-encoding was performed or claimed; steps above are planning metadata plus structural validation.");
    Ok(())
}
