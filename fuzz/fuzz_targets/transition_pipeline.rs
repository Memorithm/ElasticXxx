#![no_main]

use arbitrary::Unstructured;
use libfuzzer_sys::fuzz_target;

use elastic_core::{
    CapabilitySet, EvidenceKind, EvidenceToken, IssuerId, RepresentationEpoch, RepresentationId,
    RepresentationState, TargetContract, TransitionAttestations, TransitionMechanism,
};
use elastic_kv::{
    build_epoch_delta, validate_reusable_attention_view, KvAttentionViewPolicy, KvPageDescriptor,
    KvPageId, KvTargetMaterialization, PageTransition,
};

const MECHANISMS: [TransitionMechanism; 3] = [
    TransitionMechanism::Reinterpret,
    TransitionMechanism::Reencode,
    TransitionMechanism::Recompute,
];

struct Generator<'a> {
    u: Unstructured<'a>,
}

impl<'a> Generator<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            u: Unstructured::new(data),
        }
    }

    fn byte(&mut self, modulus: u8) -> u8 {
        let raw = self.u.arbitrary::<u8>().unwrap_or(0);
        if modulus == 0 { 0 } else { raw % modulus }
    }

    fn boolean(&mut self) -> bool {
        self.byte(2) == 1
    }

    fn identifier(&mut self, tag: u8) -> RepresentationId {
        let len = usize::from(self.u.arbitrary::<u8>().map(|raw| raw % 41).unwrap_or(0));
        let mut bytes = Vec::with_capacity(len + 1);
        bytes.push(tag);
        if let Ok(slice) = self.u.bytes(len) {
            bytes.extend_from_slice(slice);
        }
        let text = String::from_utf8_lossy(&bytes).into_owned();
        RepresentationId::new(text).unwrap_or_else(|_| RepresentationId::new("fallback").unwrap())
    }

    fn state(&mut self) -> RepresentationState {
        let id = self.identifier(b's');
        let schema_version = self.u.arbitrary::<u32>().unwrap_or(1);
        let epoch = match self.byte(8) {
            0 => RepresentationEpoch::new(u64::MAX),
            n => RepresentationEpoch::new(u64::from(n)),
        };
        RepresentationState::new(id, schema_version, epoch)
    }

    fn capability_set(&mut self, must_support: &[&RepresentationState]) -> CapabilitySet {
        let mut caps = CapabilitySet::new();
        for tag in 0..self.byte(5) {
            caps.insert(self.identifier(b'c'), u32::from(tag));
        }
        for state in must_support {
            caps.insert(state.id.clone(), state.schema_version);
        }
        caps
    }

    fn descriptor(&mut self, page: u64) -> KvPageDescriptor {
        let key_transform_scope = match self.byte(3) {
            0 => elastic_kv::KeyTransformScope::Raw,
            1 => elastic_kv::KeyTransformScope::TokenStable,
            _ => elastic_kv::KeyTransformScope::QueryDependent,
        };
        let key_encoding_pipeline = match self.byte(4) {
            0 => elastic_kv::KeyEncodingPipeline::Raw,
            1 => elastic_kv::KeyEncodingPipeline::TransformThenCodec,
            2 => elastic_kv::KeyEncodingPipeline::CodecThenTransform,
            _ => elastic_kv::KeyEncodingPipeline::FusedDeclared,
        };
        let recovery_source = match self.byte(4) {
            0 => elastic_kv::KvRecoverySource::None,
            1 => elastic_kv::KvRecoverySource::StoredCanonicalRaw,
            2 => elastic_kv::KvRecoverySource::ModelRecompute,
            _ => elastic_kv::KvRecoverySource::ExternalStableSource,
        };
        let precision_text = String::from_utf8_lossy(self.u.bytes(4).unwrap_or(b"prec"))
            .trim()
            .to_owned();
        KvPageDescriptor {
            page: KvPageId::new(page),
            representation: self.state(),
            precision: elastic_kv::KvPrecision::Custom(precision_text),
            residency: elastic_kv::KvResidency::Host,
            key_transform_scope,
            key_encoding_pipeline,
            recovery_source,
        }
    }
}

fn all_attestations() -> TransitionAttestations {
    TransitionAttestations::none()
        .attest_semantic_equivalence()
        .attest_reencoder_available()
        .attest_recompute_source_available()
}

fuzz_target!(|data: &[u8]| {
    let mut generator = Generator::new(data);

    let from = generator.state();
    let to = generator.state();
    let mechanism = MECHANISMS[usize::from(generator.byte(3))];
    let caps = generator.capability_set(&[&from, &to]);
    let attestations = all_attestations();

    let transition = elastic_core::RepresentationTransition {
        from: from.clone(),
        to: to.clone(),
        mechanism,
    };
    let _ = transition.validate(&caps, attestations);

    let new_contract = TargetContract::New {
        id: generator.identifier(b'd'),
        schema_version: generator.u.arbitrary().unwrap_or(1),
    };
    for contract in [TargetContract::Same, new_contract] {
        if let Ok(derived) = from.derive_target(contract, mechanism) {
            let derived_transition = elastic_core::RepresentationTransition {
                from: from.clone(),
                to: derived,
                mechanism,
            };
            let structurally_sound = caps.supports(&derived_transition.to)
                && derived_transition.to.epoch.get() >= derived_transition.from.epoch.get();
            if !structurally_sound {
                continue;
            }
            let outcome = derived_transition.validate(&caps, TransitionAttestations::none());
            match mechanism {
                TransitionMechanism::Reinterpret
                    if !derived_transition.from.same_contract(&derived_transition.to) =>
                {
                    assert_eq!(
                        outcome,
                        Err(elastic_core::TransitionError::MissingSemanticEquivalenceAttestation)
                    );
                }
                TransitionMechanism::Reinterpret => assert_eq!(outcome, Ok(())),
                TransitionMechanism::Reencode => assert_eq!(
                    outcome,
                    Err(elastic_core::TransitionError::MissingReencoderAttestation)
                ),
                TransitionMechanism::Recompute => assert_eq!(
                    outcome,
                    Err(elastic_core::TransitionError::MissingRecomputeSourceAttestation)
                ),
            }
            assert_eq!(
                derived_transition.validate(&caps, all_attestations()),
                Ok(())
            );
        }
    }

    let issuer = IssuerId::new("fuzz-issuer").unwrap_or_else(|_| IssuerId::new("fb").unwrap());
    let token = EvidenceToken::issue(issuer, EvidenceKind::ReencoderAvailable, &transition);
    assert!(token.matches(&transition));

    let page_count = usize::from(generator.byte(9));
    let pages: Vec<KvPageDescriptor> = (0..page_count as u64)
        .map(|index| generator.descriptor(index))
        .collect();
    let policy = if generator.boolean() {
        KvAttentionViewPolicy::per_page_representation()
    } else {
        KvAttentionViewPolicy::homogeneous()
    };
    if let Ok(summary) = validate_reusable_attention_view(&pages, policy) {
        assert_eq!(summary.page_count, pages.len());
    }

    if let Some(source) = pages.first() {
        if source.validate_descriptor().is_ok() && source.key_transform_is_query_independent() {
            let target_state = generator.state();
            let plan_caps = generator.capability_set(&[&target_state]);
            if let Ok(plan) = source.validate_reusable_representation_change(
                target_state,
                mechanism,
                &plan_caps,
                attestations,
                KvTargetMaterialization::new(
                    source.key_transform_scope,
                    source.key_encoding_pipeline,
                    source.recovery_source,
                ),
            ) {
                let transitions: Vec<PageTransition> = (0..page_count as u64)
                    .map(|index| PageTransition {
                        page: KvPageId::new(index),
                        plan: plan.clone(),
                    })
                    .collect();
                if let Ok(delta) = build_epoch_delta(&transitions) {
                    assert!(delta.to.epoch.get() > delta.from.epoch.get());
                    assert_eq!(delta.len(), transitions.len());
                }
            }
        }
    }
});
