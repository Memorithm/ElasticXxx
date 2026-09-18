//! Trusted BE14d transaction bridge for one KV representation transition.
//!
//! The Boolean capacity guard in [`crate::boolean_admission`] is planning
//! evidence only. This module deliberately does not turn a `True` guard into
//! actuation authority. Instead it adapts a concrete KV backend to the existing
//! [`elastic_runtime::TransactionalActuator`] contract so that trusted
//! validation, prepare, actuation, verification, commit, and rollback continue
//! through the generic ElasticXxx runtime state machine.
//!
//! Concrete caches retain ownership of physical storage, codec semantics, and
//! verification. The adapter binds the exact source page and validated
//! [`KvTransitionPlan`], rejects source drift, and verifies descriptor identity
//! around backend-owned semantic verification and rollback.

use elastic_core::resource::{DimensionId, LogicalResourceId};
use elastic_eir::{EirResource, Fingerprint, PlanOutcome};
use elastic_runtime::{
    Actuation, CommitRecord, InvariantCheck, Plan, RollbackRecord, RuntimeError,
    TransactionalActuator, ValidatedPlan, VerificationResult,
};

use crate::{KvPageDescriptor, KvTransitionPlan};

/// Concrete KV backend boundary used by the generic ElasticXxx transaction.
///
/// Implementations own the physical cache and therefore remain authoritative
/// for domain-specific invariant checks and semantic verification. Returning an
/// empty invariant-check vector is valid only when the bound runtime resource
/// declares no invariant applicable to the candidate; the generic runtime will
/// otherwise reject validation fail-closed.
pub trait KvTransitionBackendV1 {
    /// Stable backend identity used in runtime audit records.
    fn name(&self) -> &str;

    /// Read the currently authoritative descriptor for `page`.
    fn read_page(&self, page: crate::KvPageId) -> Result<KvPageDescriptor, String>;

    /// Perform trusted action-time validation for the exact transition.
    ///
    /// Returned checks are fed unchanged to the generic runtime validator.
    fn validate_transition(
        &self,
        runtime_plan: &Plan,
        source: &KvPageDescriptor,
        target: &KvPageDescriptor,
        transition: &KvTransitionPlan,
    ) -> Result<Vec<InvariantCheck>, String>;

    /// Prepare backend resources without making the target visible.
    fn prepare_transition(
        &mut self,
        source: &KvPageDescriptor,
        target: &KvPageDescriptor,
        transition: &KvTransitionPlan,
    ) -> Result<(), String>;

    /// Atomically re-check the authoritative source state and action-time
    /// physical feasibility, then apply the prepared transition.
    ///
    /// The source comparison and mutation must share the backend's concurrency
    /// boundary. A stale `source` must fail without overwriting a concurrent KV
    /// mutation. This method is the last trusted check before physical effect.
    fn apply_transition_if_source(
        &mut self,
        source: &KvPageDescriptor,
        target: &KvPageDescriptor,
        transition: &KvTransitionPlan,
    ) -> Result<(), String>;

    /// Verify backend-specific semantics for the expected target state.
    fn verify_transition(
        &self,
        target: &KvPageDescriptor,
        transition: &KvTransitionPlan,
    ) -> Result<VerificationResult, String>;

    /// Restore the exact source state after a failed/inconclusive verification
    /// or failed commit path.
    fn restore_page(&mut self, source: &KvPageDescriptor) -> Result<(), String>;
}

/// Build the descriptor that a representation-only transition is permitted to
/// expose.
///
/// BE14d owns the representation axis, so page identity, precision, and
/// residency are preserved. Precision/residency changes belong to their own
/// declared dimensions and cannot be smuggled through this adapter.
#[must_use]
pub fn kv_transition_target_descriptor(
    source: &KvPageDescriptor,
    transition: &KvTransitionPlan,
) -> KvPageDescriptor {
    KvPageDescriptor {
        page: source.page,
        representation: transition.representation.to.clone(),
        precision: source.precision.clone(),
        residency: source.residency.clone(),
        key_transform_scope: transition.target_key_transform_scope,
        key_encoding_pipeline: transition.target_key_encoding_pipeline,
        recovery_source: transition.target_recovery_source,
    }
}

/// Trusted adapter binding one source KV page to one exact validated target.
pub struct TransactionalKvPageV1<B> {
    backend: B,
    resource: LogicalResourceId,
    resource_fingerprint: Fingerprint,
    source: KvPageDescriptor,
    target: KvPageDescriptor,
    transition: KvTransitionPlan,
    prepared: bool,
    actuated: bool,
}

impl<B: KvTransitionBackendV1> TransactionalKvPageV1<B> {
    /// Bind a backend to an exact representation-only transition.
    pub fn new(
        backend: B,
        resource: &EirResource,
        source: KvPageDescriptor,
        transition: KvTransitionPlan,
        capabilities: &elastic_core::CapabilitySet,
        attestations: elastic_core::TransitionAttestations,
    ) -> Result<Self, RuntimeError> {
        source
            .validate_descriptor()
            .map_err(|error| RuntimeError::configuration(error.to_string()))?;
        if transition.representation.from != source.representation {
            return Err(RuntimeError::configuration(
                "KV transaction source descriptor does not match transition source representation",
            ));
        }

        // `KvTransitionPlan` is a public explanatory value, not an unforgeable
        // authorization token. Re-run the authoritative structural validator at
        // this trust boundary from the exact supplied target contract and
        // provenance-carrying capabilities/attestations before binding it.
        let authoritative = source
            .validate_reusable_representation_change(
                transition.representation.to.clone(),
                transition.representation.mechanism,
                capabilities,
                attestations,
                crate::KvTargetMaterialization::new(
                    transition.target_key_transform_scope,
                    transition.target_key_encoding_pipeline,
                    transition.target_recovery_source,
                ),
            )
            .map_err(|error| RuntimeError::configuration(error.to_string()))?;
        if authoritative != transition {
            return Err(RuntimeError::configuration(
                "KV transaction plan does not match authoritative structural validation",
            ));
        }

        let target = kv_transition_target_descriptor(&source, &authoritative);
        target
            .validate_descriptor()
            .map_err(|error| RuntimeError::configuration(error.to_string()))?;
        if target.cache_compatibility() != authoritative.compatibility {
            return Err(RuntimeError::configuration(
                "KV transaction target descriptor disagrees with transition compatibility",
            ));
        }
        Ok(Self {
            backend,
            resource: resource.identity().clone(),
            resource_fingerprint: resource.fingerprint(),
            source,
            target,
            transition: authoritative,
            prepared: false,
            actuated: false,
        })
    }

    /// Exact target descriptor that may become visible after a verified commit.
    #[must_use]
    pub const fn target(&self) -> &KvPageDescriptor {
        &self.target
    }

    /// Read-only access to the concrete backend.
    #[must_use]
    pub const fn backend(&self) -> &B {
        &self.backend
    }

    /// Consume the adapter and recover the concrete backend.
    #[must_use]
    pub fn into_backend(self) -> B {
        self.backend
    }

    fn require_runtime_binding(&self, plan: &Plan) -> Result<(), RuntimeError> {
        if plan.resource.identity() != &self.resource {
            return Err(RuntimeError::validation(
                "KV runtime plan targets a different logical resource",
            ));
        }
        if plan.resource.fingerprint() != self.resource_fingerprint {
            return Err(RuntimeError::validation(
                "KV runtime plan resource fingerprint differs from the bound EIR contract",
            ));
        }
        let candidate = plan.candidate().ok_or_else(|| {
            RuntimeError::validation("KV transactional adapter requires a transition candidate")
        })?;
        if candidate.dimension() != &DimensionId::REPRESENTATION
            || candidate.mechanism() != self.transition.representation.mechanism
        {
            return Err(RuntimeError::validation(
                "KV runtime candidate does not match the bound representation transition",
            ));
        }
        if !candidate.is_declared_in(&plan.resource) || !candidate.capability_grounded() {
            return Err(RuntimeError::validation(
                "KV runtime candidate is not declared and capability-grounded",
            ));
        }
        match &plan.outcome {
            PlanOutcome::Candidate(_) => Ok(()),
            _ => Err(RuntimeError::validation(
                "KV runtime plan outcome is not an executable candidate",
            )),
        }
    }

    fn require_source_state(&self, stage: &str) -> Result<(), RuntimeError> {
        let current = self
            .backend
            .read_page(self.source.page)
            .map_err(|error| RuntimeError::validation(format!("{stage}: {error}")))?;
        if current != self.source {
            return Err(RuntimeError::validation(format!(
                "{stage}: KV source page drifted from the bound descriptor"
            )));
        }
        Ok(())
    }
}

impl<B: KvTransitionBackendV1> TransactionalActuator for TransactionalKvPageV1<B> {
    fn name(&self) -> &str {
        self.backend.name()
    }

    fn validate(&self, plan: &Plan) -> Result<Vec<InvariantCheck>, RuntimeError> {
        self.require_runtime_binding(plan)?;
        self.require_source_state("action-time validation")?;
        self.backend
            .validate_transition(plan, &self.source, &self.target, &self.transition)
            .map_err(RuntimeError::validation)
    }

    fn prepare(&mut self, plan: &ValidatedPlan) -> Result<Actuation, RuntimeError> {
        if !plan.validated {
            return Err(RuntimeError::validation(
                "KV transaction cannot prepare an unvalidated plan",
            ));
        }
        self.require_runtime_binding(&plan.plan)?;
        self.require_source_state("prepare")?;
        self.backend
            .prepare_transition(&self.source, &self.target, &self.transition)
            .map_err(RuntimeError::actuation)?;
        self.prepared = true;
        Ok(Actuation::new(plan.clone(), None, self.backend.name()))
    }

    fn actuate(&mut self, actuation: &Actuation) -> Result<(), RuntimeError> {
        if !self.prepared || !actuation.is_valid() || actuation.adapter_name != self.backend.name()
        {
            return Err(RuntimeError::actuation(
                "KV transaction actuation is not bound to the prepared validated adapter state",
            ));
        }
        // Re-read immediately before the physical boundary for a cheap
        // fail-closed guard, then require the backend itself to compare source
        // + feasibility atomically with the mutation.
        self.require_source_state("pre-actuation")?;
        self.backend
            .apply_transition_if_source(&self.source, &self.target, &self.transition)
            .map_err(RuntimeError::actuation)?;
        self.actuated = true;
        Ok(())
    }

    fn verify(&self, actuation: &Actuation) -> Result<VerificationResult, RuntimeError> {
        if !self.actuated || !actuation.is_valid() || actuation.adapter_name != self.backend.name()
        {
            return Err(RuntimeError::verification(
                "KV transaction verification is not bound to an applied actuation",
            ));
        }
        let current = self
            .backend
            .read_page(self.source.page)
            .map_err(RuntimeError::verification)?;
        if current != self.target {
            return Ok(VerificationResult::Fail {
                detail: "KV backend descriptor does not match the bound target".into(),
            });
        }
        self.backend
            .verify_transition(&self.target, &self.transition)
            .map_err(RuntimeError::verification)
    }

    fn commit(&mut self, actuation: &Actuation) -> Result<CommitRecord, RuntimeError> {
        if !self.actuated || !actuation.is_valid() || actuation.adapter_name != self.backend.name()
        {
            return Err(RuntimeError::commit(
                "KV transaction commit is not bound to an applied verified actuation",
            ));
        }
        let current = self
            .backend
            .read_page(self.source.page)
            .map_err(RuntimeError::commit)?;
        if current != self.target {
            return Err(RuntimeError::commit(
                "KV target descriptor drifted before commit",
            ));
        }
        self.prepared = false;
        self.actuated = false;
        Ok(CommitRecord::new(
            format!("kv-page:{}", self.source.page),
            "verified KV representation transition committed",
        ))
    }

    fn rollback(
        &mut self,
        actuation: &Actuation,
        _verification: &VerificationResult,
    ) -> Result<RollbackRecord, RuntimeError> {
        if !self.prepared || !actuation.is_valid() || actuation.adapter_name != self.backend.name()
        {
            return Err(RuntimeError::rollback(
                "KV transaction rollback is not bound to the prepared adapter state",
            ));
        }
        self.backend
            .restore_page(&self.source)
            .map_err(RuntimeError::rollback)?;
        let restored = self
            .backend
            .read_page(self.source.page)
            .map_err(RuntimeError::rollback)?
            == self.source;
        self.prepared = false;
        self.actuated = false;
        Ok(RollbackRecord::new(
            format!("kv-page:{}", self.source.page),
            "KV representation transition rolled back to bound source descriptor",
            restored,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use elastic_core::resource::{
        AdmissibleTransition, CapabilityRequirement, Invariant, InvariantKind, ObservationSignalId,
        ResourceClassId, ResourceSpec,
    };
    use elastic_core::{
        CapabilitySet, RepresentationEpoch, RepresentationId, RepresentationState,
        TransitionAttestations, TransitionMechanism,
    };
    use elastic_eir::{lower, FirstGroundedPlanner};
    use elastic_runtime::{Runtime, RuntimeConfig, RuntimeMode};
    use std::time::{Duration, Instant};

    use crate::{
        boolean_admission::{
            BooleanKvCapacityPreflightControllerV1, BooleanKvTransitionPreflightV1,
            KvCapacityObservationV1,
        },
        KeyEncodingPipeline, KeyTransformScope, KvPageId, KvPrecision, KvRecoverySource,
        KvResidency, KvTargetMaterialization,
    };

    #[derive(Clone)]
    struct TestBackend {
        page: KvPageDescriptor,
        fail_verify: bool,
        drift_at_apply: bool,
        prepare_count: usize,
        apply_count: usize,
        restore_count: usize,
    }

    impl TestBackend {
        fn new(page: KvPageDescriptor) -> Self {
            Self {
                page,
                fail_verify: false,
                drift_at_apply: false,
                prepare_count: 0,
                apply_count: 0,
                restore_count: 0,
            }
        }
    }

    impl KvTransitionBackendV1 for TestBackend {
        fn name(&self) -> &str {
            "be14d-test-kv-backend"
        }

        fn read_page(&self, page: KvPageId) -> Result<KvPageDescriptor, String> {
            if page != self.page.page {
                return Err("unknown test page".into());
            }
            Ok(self.page.clone())
        }

        fn validate_transition(
            &self,
            runtime_plan: &Plan,
            _source: &KvPageDescriptor,
            _target: &KvPageDescriptor,
            _transition: &KvTransitionPlan,
        ) -> Result<Vec<InvariantCheck>, String> {
            Ok(runtime_plan
                .resource
                .invariants()
                .iter()
                .cloned()
                .map(|invariant| {
                    InvariantCheck::new(
                        invariant,
                        true,
                        Some("test backend validated declared invariant".into()),
                    )
                })
                .collect())
        }

        fn prepare_transition(
            &mut self,
            _source: &KvPageDescriptor,
            _target: &KvPageDescriptor,
            _transition: &KvTransitionPlan,
        ) -> Result<(), String> {
            self.prepare_count += 1;
            Ok(())
        }

        fn apply_transition_if_source(
            &mut self,
            source: &KvPageDescriptor,
            target: &KvPageDescriptor,
            _transition: &KvTransitionPlan,
        ) -> Result<(), String> {
            if self.drift_at_apply {
                self.page.representation = state("kv.concurrent-drift", 99);
            }
            if &self.page != source {
                return Err("atomic source comparison rejected stale KV page".into());
            }
            self.apply_count += 1;
            self.page = target.clone();
            Ok(())
        }

        fn verify_transition(
            &self,
            _target: &KvPageDescriptor,
            _transition: &KvTransitionPlan,
        ) -> Result<VerificationResult, String> {
            if self.fail_verify {
                Ok(VerificationResult::Fail {
                    detail: "injected semantic verification failure".into(),
                })
            } else {
                Ok(VerificationResult::Pass)
            }
        }

        fn restore_page(&mut self, source: &KvPageDescriptor) -> Result<(), String> {
            self.restore_count += 1;
            self.page = source.clone();
            Ok(())
        }
    }

    fn state(name: &str, epoch: u64) -> RepresentationState {
        RepresentationState::new(
            RepresentationId::new(name).unwrap(),
            1,
            RepresentationEpoch::new(epoch),
        )
    }

    fn source_page() -> KvPageDescriptor {
        KvPageDescriptor {
            page: KvPageId::new(17),
            representation: state("kv.canonical", 1),
            precision: KvPrecision::F16,
            residency: KvResidency::Host,
            key_transform_scope: KeyTransformScope::TokenStable,
            key_encoding_pipeline: KeyEncodingPipeline::TransformThenCodec,
            recovery_source: KvRecoverySource::StoredCanonicalRaw,
        }
    }

    fn fixture() -> (
        ResourceSpec,
        elastic_eir::EirResource,
        KvPageDescriptor,
        KvTransitionPlan,
    ) {
        let resource_id = LogicalResourceId::new("be14d-kv-test").unwrap();
        let spec = ResourceSpec::builder(ResourceClassId::REPRESENTATIONAL, resource_id)
            .allow(DimensionId::REPRESENTATION)
            .preserve(Invariant::new(InvariantKind::PreserveContents))
            .admit(AdmissibleTransition::new(
                elastic_core::TransitionMechanism::Reencode,
                DimensionId::REPRESENTATION,
            ))
            .require_capability(CapabilityRequirement::new(
                elastic_core::TransitionMechanism::Reencode,
                DimensionId::REPRESENTATION,
            ))
            .observe(ObservationSignalId::FREE_CAPACITY)
            .build()
            .unwrap();
        let eir = lower(&spec).unwrap().resources()[0].clone();
        let source = source_page();
        let target = state("kv.reencoded", 2);
        let mut capabilities = CapabilitySet::new();
        capabilities.insert(target.id.clone(), target.schema_version);
        let plan = source
            .validate_reusable_representation_change(
                target,
                TransitionMechanism::Reencode,
                &capabilities,
                TransitionAttestations::none().attest_reencoder_available(),
                KvTargetMaterialization::new(
                    KeyTransformScope::TokenStable,
                    KeyEncodingPipeline::TransformThenCodec,
                    KvRecoverySource::StoredCanonicalRaw,
                ),
            )
            .unwrap();
        (spec, eir, source, plan)
    }

    fn runtime(spec: ResourceSpec, eir: elastic_eir::EirResource) -> Runtime {
        Runtime::new(RuntimeConfig {
            resource_spec: spec,
            ir_resource: eir,
            mode: RuntimeMode::Apply,
            dry_run: false,
            max_cycles: 1,
            ..RuntimeConfig::default()
        })
    }

    #[test]
    fn boolean_capacity_candidate_flows_into_generic_verified_transaction() {
        let (spec, eir, source, expected_transition) = fixture();
        let resource_id = spec.resource_id().clone();
        let now = Instant::now();
        let observation = KvCapacityObservationV1::measured(resource_id.clone(), 4096, now);
        let target = expected_transition.representation.to.clone();
        let mut capabilities = CapabilitySet::new();
        capabilities.insert(target.id.clone(), target.schema_version);
        let target_materialization = KvTargetMaterialization::new(
            expected_transition.target_key_transform_scope,
            expected_transition.target_key_encoding_pipeline,
            expected_transition.target_recovery_source,
        );
        let mut gate = BooleanKvCapacityPreflightControllerV1::new(
            spec.clone(),
            TransitionMechanism::Reencode,
            Duration::from_secs(1),
        )
        .unwrap();
        let transition = match gate
            .validate_candidate(
                &source,
                target,
                &capabilities,
                TransitionAttestations::none().attest_reencoder_available(),
                target_materialization,
                observation.planning_context(),
                observation.observations(),
                2048,
                now,
            )
            .unwrap()
        {
            BooleanKvTransitionPreflightV1::Candidate { report, plan } => {
                assert_eq!(report.evidence.truth, "true");
                assert_eq!(plan, expected_transition);
                plan
            }
            BooleanKvTransitionPreflightV1::Blocked(report) => {
                panic!("fresh sufficient capacity unexpectedly blocked: {report:?}")
            }
        };
        let backend = TestBackend::new(source.clone());
        let mut actuator = TransactionalKvPageV1::new(
            backend,
            &eir,
            source,
            transition,
            &capabilities,
            TransitionAttestations::none().attest_reencoder_available(),
        )
        .unwrap();
        let expected = actuator.target().clone();

        let result = runtime(spec, eir.clone())
            .cycle(&eir, &FirstGroundedPlanner, &(), &mut actuator)
            .unwrap();

        assert!(result.commit.is_some());
        assert!(result.rollback.is_none());
        assert_eq!(actuator.backend().page, expected);
        assert_eq!(actuator.backend().prepare_count, 1);
        assert_eq!(actuator.backend().apply_count, 1);
        assert_eq!(actuator.backend().restore_count, 0);
    }

    #[test]
    fn forged_public_transition_is_revalidated_before_backend_binding() {
        let (_spec, eir, source, mut transition) = fixture();
        transition.representation.mechanism = TransitionMechanism::Reinterpret;
        let mut capabilities = CapabilitySet::new();
        capabilities.insert(
            transition.representation.to.id.clone(),
            transition.representation.to.schema_version,
        );
        let error = TransactionalKvPageV1::new(
            TestBackend::new(source.clone()),
            &eir,
            source,
            transition,
            &capabilities,
            TransitionAttestations::none().attest_reencoder_available(),
        )
        .err()
        .expect("forged public transition must be rejected");
        assert!(error.to_string().contains("configuration error"));
    }

    #[test]
    fn same_resource_id_with_different_eir_fingerprint_is_rejected() {
        let (_spec, bound_eir, source, transition) = fixture();
        let mut capabilities = CapabilitySet::new();
        capabilities.insert(
            transition.representation.to.id.clone(),
            transition.representation.to.schema_version,
        );
        let mut actuator = TransactionalKvPageV1::new(
            TestBackend::new(source),
            &bound_eir,
            source_page(),
            transition,
            &capabilities,
            TransitionAttestations::none().attest_reencoder_available(),
        )
        .unwrap();

        let foreign_spec = ResourceSpec::builder(
            ResourceClassId::REPRESENTATIONAL,
            bound_eir.identity().clone(),
        )
        .allow(DimensionId::REPRESENTATION)
        .admit(AdmissibleTransition::new(
            TransitionMechanism::Reencode,
            DimensionId::REPRESENTATION,
        ))
        .require_capability(CapabilityRequirement::new(
            TransitionMechanism::Reencode,
            DimensionId::REPRESENTATION,
        ))
        .observe(ObservationSignalId::FREE_CAPACITY)
        .build()
        .unwrap();
        let foreign_eir = lower(&foreign_spec).unwrap().resources()[0].clone();
        assert_ne!(foreign_eir.fingerprint(), bound_eir.fingerprint());

        let error = runtime(foreign_spec, foreign_eir.clone())
            .cycle(&foreign_eir, &FirstGroundedPlanner, &(), &mut actuator)
            .expect_err("foreign EIR with reused logical id must fail closed");
        assert!(error.to_string().contains("fingerprint differs"));
        assert_eq!(actuator.backend().prepare_count, 0);
        assert_eq!(actuator.backend().apply_count, 0);
    }

    #[test]
    fn atomic_backend_source_check_rolls_back_concurrent_drift() {
        let (spec, eir, source, transition) = fixture();
        let mut capabilities = CapabilitySet::new();
        capabilities.insert(
            transition.representation.to.id.clone(),
            transition.representation.to.schema_version,
        );
        let mut backend = TestBackend::new(source.clone());
        backend.drift_at_apply = true;
        let mut actuator = TransactionalKvPageV1::new(
            backend,
            &eir,
            source.clone(),
            transition,
            &capabilities,
            TransitionAttestations::none().attest_reencoder_available(),
        )
        .unwrap();

        let result = runtime(spec, eir.clone())
            .cycle(&eir, &FirstGroundedPlanner, &(), &mut actuator)
            .unwrap();

        assert!(result.commit.is_none());
        assert!(matches!(
            result.verification,
            Some(VerificationResult::Inconclusive { .. })
        ));
        assert!(result
            .rollback
            .as_ref()
            .is_some_and(|record| record.invariants_restored));
        assert_eq!(actuator.backend().page, source);
        assert_eq!(actuator.backend().apply_count, 0);
        assert_eq!(actuator.backend().restore_count, 1);
    }

    #[test]
    fn failed_backend_semantic_verification_rolls_back_exact_source() {
        let (spec, eir, source, transition) = fixture();
        let mut capabilities = CapabilitySet::new();
        capabilities.insert(
            transition.representation.to.id.clone(),
            transition.representation.to.schema_version,
        );
        let mut backend = TestBackend::new(source.clone());
        backend.fail_verify = true;
        let mut actuator = TransactionalKvPageV1::new(
            backend,
            &eir,
            source.clone(),
            transition,
            &capabilities,
            TransitionAttestations::none().attest_reencoder_available(),
        )
        .unwrap();

        let result = runtime(spec, eir.clone())
            .cycle(&eir, &FirstGroundedPlanner, &(), &mut actuator)
            .unwrap();

        assert!(result.commit.is_none());
        assert!(result
            .rollback
            .as_ref()
            .is_some_and(|record| record.invariants_restored));
        assert_eq!(actuator.backend().page, source);
        assert_eq!(actuator.backend().apply_count, 1);
        assert_eq!(actuator.backend().restore_count, 1);
    }

    #[test]
    fn action_time_source_drift_fails_before_prepare_or_actuation() {
        let (spec, eir, source, transition) = fixture();
        let mut capabilities = CapabilitySet::new();
        capabilities.insert(
            transition.representation.to.id.clone(),
            transition.representation.to.schema_version,
        );
        let mut drifted = source.clone();
        drifted.representation = state("kv.external-drift", 2);
        let backend = TestBackend::new(drifted);
        let mut actuator = TransactionalKvPageV1::new(
            backend,
            &eir,
            source,
            transition,
            &capabilities,
            TransitionAttestations::none().attest_reencoder_available(),
        )
        .unwrap();

        let error = runtime(spec, eir.clone())
            .cycle(&eir, &FirstGroundedPlanner, &(), &mut actuator)
            .expect_err("source drift must fail closed before mutation");

        assert!(error.to_string().contains("source page drifted"));
        assert_eq!(actuator.backend().prepare_count, 0);
        assert_eq!(actuator.backend().apply_count, 0);
        assert_eq!(actuator.backend().restore_count, 0);
    }
}
