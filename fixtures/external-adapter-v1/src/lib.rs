//! Standalone consumer of the versioned Elastic external-adapter contract.
//!
//! This crate intentionally depends only on the public `elastic` facade.

#[cfg(test)]
mod tests {
    use elastic::external_adapter_v1::{
        Actuation, CommitRecord, FirstGroundedPlanner, InvariantCheck, Observer, Plan,
        PlanningContext, RollbackRecord, Runtime, RuntimeConfig, RuntimeError, RuntimeMode,
        TransactionalActuator, ValidatedPlan, VerificationResult,
    };

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum VerificationMode {
        Pass,
        Fail,
    }

    struct FixtureObserver;

    impl Observer for FixtureObserver {
        fn observe(
            &self,
        ) -> (
            PlanningContext,
            Vec<elastic::external_adapter_v1::Observation>,
        ) {
            (PlanningContext::new(), Vec::new())
        }
    }

    struct FixtureAdapter {
        verification: VerificationMode,
        fail_commit: bool,
        misbind_name: bool,
        actuation_calls: usize,
        committed: bool,
        rolled_back: bool,
    }

    impl FixtureAdapter {
        fn passing() -> Self {
            Self {
                verification: VerificationMode::Pass,
                fail_commit: false,
                misbind_name: false,
                actuation_calls: 0,
                committed: false,
                rolled_back: false,
            }
        }
    }

    impl TransactionalActuator for FixtureAdapter {
        fn name(&self) -> &str {
            "external-v1-fixture"
        }

        fn validate(&self, plan: &Plan) -> Result<Vec<InvariantCheck>, RuntimeError> {
            Ok(plan
                .resource
                .invariants()
                .iter()
                .cloned()
                .map(|invariant| {
                    InvariantCheck::new(
                        invariant,
                        true,
                        Some("external fixture revalidated invariant".to_owned()),
                    )
                })
                .collect())
        }

        fn prepare(&mut self, plan: &ValidatedPlan) -> Result<Actuation, RuntimeError> {
            let name = if self.misbind_name {
                "foreign-adapter"
            } else {
                self.name()
            };
            Ok(Actuation::new(plan.clone(), None, name))
        }

        fn actuate(&mut self, _actuation: &Actuation) -> Result<(), RuntimeError> {
            self.actuation_calls = self.actuation_calls.saturating_add(1);
            Ok(())
        }

        fn verify(&self, _actuation: &Actuation) -> Result<VerificationResult, RuntimeError> {
            Ok(match self.verification {
                VerificationMode::Pass => VerificationResult::Pass,
                VerificationMode::Fail => VerificationResult::Fail {
                    detail: "injected external verification failure".to_owned(),
                },
            })
        }

        fn commit(&mut self, _actuation: &Actuation) -> Result<CommitRecord, RuntimeError> {
            if self.fail_commit {
                return Err(RuntimeError::commit("injected external commit failure"));
            }
            self.committed = true;
            Ok(CommitRecord::new(
                self.name(),
                "external fixture commit after verification",
            ))
        }

        fn rollback(
            &mut self,
            _actuation: &Actuation,
            _verification: &VerificationResult,
        ) -> Result<RollbackRecord, RuntimeError> {
            self.rolled_back = true;
            Ok(RollbackRecord::new(
                self.name(),
                "external fixture restored pre-actuation state",
                true,
            ))
        }
    }

    fn applying_runtime() -> Runtime {
        Runtime::new(RuntimeConfig {
            mode: RuntimeMode::Apply,
            dry_run: false,
            ..RuntimeConfig::default()
        })
    }

    #[test]
    fn facade_only_external_adapter_commits_after_verification() {
        assert_eq!(elastic::external_adapter_v1::CONTRACT_VERSION, 1);
        let runtime = applying_runtime();
        let resource = runtime.config().ir_resource.clone();
        let mut adapter = FixtureAdapter::passing();

        let result = runtime
            .cycle(
                &resource,
                &FirstGroundedPlanner,
                &FixtureObserver,
                &mut adapter,
            )
            .expect("conforming external adapter should commit");

        assert_eq!(adapter.actuation_calls, 1);
        assert!(adapter.committed);
        assert!(!adapter.rolled_back);
        assert!(result.commit.is_some());
        assert!(result.rollback.is_none());
    }

    #[test]
    fn facade_only_external_adapter_rolls_back_failed_verification() {
        let runtime = applying_runtime();
        let resource = runtime.config().ir_resource.clone();
        let mut adapter = FixtureAdapter::passing();
        adapter.verification = VerificationMode::Fail;

        let result = runtime
            .cycle(
                &resource,
                &FirstGroundedPlanner,
                &FixtureObserver,
                &mut adapter,
            )
            .expect("failed verification with restored rollback should be recoverable");

        assert_eq!(adapter.actuation_calls, 1);
        assert!(!adapter.committed);
        assert!(adapter.rolled_back);
        assert!(result.commit.is_none());
        assert!(result.rollback.is_some());
    }

    #[test]
    fn facade_only_external_adapter_rolls_back_commit_failure() {
        let runtime = applying_runtime();
        let resource = runtime.config().ir_resource.clone();
        let mut adapter = FixtureAdapter::passing();
        adapter.fail_commit = true;

        let result = runtime
            .cycle(
                &resource,
                &FirstGroundedPlanner,
                &FixtureObserver,
                &mut adapter,
            )
            .expect("commit failure with restored rollback should be recoverable");

        assert_eq!(adapter.actuation_calls, 1);
        assert!(!adapter.committed);
        assert!(adapter.rolled_back);
        assert!(result.commit.is_none());
        assert!(result.rollback.is_some());
    }

    #[test]
    fn runtime_rejects_external_adapter_identity_misbinding_before_actuation() {
        let runtime = applying_runtime();
        let resource = runtime.config().ir_resource.clone();
        let mut adapter = FixtureAdapter::passing();
        adapter.misbind_name = true;

        let error = runtime
            .cycle(
                &resource,
                &FirstGroundedPlanner,
                &FixtureObserver,
                &mut adapter,
            )
            .expect_err("misbound prepared actuation must fail closed");

        assert!(matches!(error, RuntimeError::Validation(_)));
        assert_eq!(adapter.actuation_calls, 0);
        assert!(!adapter.committed);
        assert!(!adapter.rolled_back);
    }
}
