use std::error::Error;
use std::io::{Error as IoError, ErrorKind};

use elastic_core::{resource::DimensionId, TransitionMechanism};
use elastic_eir::{
    EirResource, FirstGroundedPlanner, PlanOutcome, TransitionCandidate, TransitionPlanner,
};
use elastic_runtime::control_loop::{collect_observations, observe_and_plan};
use elastic_runtime::plan::{plan_with_context, validate_with_checks};
use elastic_runtime::{
    HostMemoryObserver, Observation, Observer, Runtime, RuntimeConfig, RuntimeMode,
    TransactionalActuator, TransactionalRam,
};
use serde_json::{json, Value};

type CommandResult = Result<(), Box<dyn Error>>;

#[derive(Clone, Copy, Debug)]
pub struct RamCommandOptions {
    pub host_total: u64,
    pub min: u64,
    pub max: u64,
    pub initial: u64,
    pub target: u64,
    pub max_step: Option<u64>,
}

struct CapacityTargetPlanner {
    target: u64,
}

impl TransitionPlanner for CapacityTargetPlanner {
    fn propose_transition(&self, resource: &EirResource) -> PlanOutcome {
        let Some(admitted) = resource.transitions().iter().find(|admitted| {
            admitted.transition().mechanism() == TransitionMechanism::Reinterpret
                && admitted.transition().dimension() == &DimensionId::CAPACITY
                && admitted.capability_grounded()
        }) else {
            return PlanOutcome::Unsupported;
        };
        PlanOutcome::Candidate(
            TransitionCandidate::from_admitted(admitted).with_magnitude(self.target),
        )
    }
}

pub fn inspect(id: &str) -> CommandResult {
    let config = config_for(id)?;
    print_json(json!({
        "command": "inspect",
        "resource_id": config.resource_spec.resource_id().as_str(),
        "resource_spec": format!("{:?}", config.resource_spec),
        "eir": format!("{:?}", config.ir_resource),
        "planner_config": format!("{:?}", config.planner_config),
        "cadence": format!("{:?}", config.cadence),
        "mode": format!("{:?}", config.mode),
        "dry_run": config.dry_run,
    }))
}

pub fn observe(id: &str) -> CommandResult {
    let config = config_for(id)?;
    let (_context, snapshot) = collect_observations(&HostMemoryObserver);

    print_json(json!({
        "command": "observe",
        "resource_id": config.resource_spec.resource_id().as_str(),
        "all_signals_valid": snapshot.all_signals_valid,
        "observations": render_observations(snapshot.iter()),
    }))
}

pub fn plan(id: &str) -> CommandResult {
    let config = config_for(id)?;
    let (snapshot, plan) = observe_and_plan(
        &FirstGroundedPlanner,
        &config.ir_resource,
        &HostMemoryObserver,
    )?;

    print_json(json!({
        "command": "plan",
        "resource_id": config.resource_spec.resource_id().as_str(),
        "outcome": plan.outcome.to_string(),
        "reasoning": plan.reasoning,
        "candidate_target": plan.candidate().and_then(|candidate| candidate.magnitude()),
        "observations": render_observations(snapshot.iter()),
    }))
}

pub fn validate(id: &str, options: RamCommandOptions) -> CommandResult {
    let adapter = transactional_ram(id, options)?;
    let resource = adapter.ir()?;
    let (context, snapshot) = adapter.observe();
    let plan = plan_with_context(
        &CapacityTargetPlanner {
            target: options.target,
        },
        &resource,
        &context,
    );
    let checks = adapter.validate(&plan)?;
    let validated = validate_with_checks(plan, checks);

    print_json(json!({
        "command": "validate",
        "resource_id": resource.identity().as_str(),
        "target": options.target,
        "validated": validated.validated,
        "invariant_checks": validated.invariant_checks.iter().map(|check| json!({
            "invariant": check.invariant.to_string(),
            "holds": check.holds,
            "detail": check.detail,
        })).collect::<Vec<_>>(),
        "observations": render_observations(snapshot.iter()),
    }))
}

pub fn apply(id: &str, options: RamCommandOptions) -> CommandResult {
    let adapter = transactional_ram(id, options)?;
    let observer = adapter.clone();
    let mut actuator = adapter.clone();
    let resource = adapter.ir()?;
    let runtime = Runtime::new(RuntimeConfig {
        mode: RuntimeMode::Apply,
        dry_run: false,
        ..RuntimeConfig::default()
    });
    let result = runtime.cycle(
        &resource,
        &CapacityTargetPlanner {
            target: options.target,
        },
        &observer,
        &mut actuator,
    )?;

    print_json(json!({
        "command": "apply",
        "resource_id": resource.identity().as_str(),
        "target": options.target,
        "committed_bytes": adapter.committed()?,
        "committed": result.commit.is_some(),
        "rolled_back": result.rollback.is_some(),
        "verification": result.verification.as_ref().map(|verification| format!("{verification:?}")),
        "events": result.events.iter().map(|event| json!({
            "kind": format!("{:?}", event.kind),
            "details": event.details,
        })).collect::<Vec<_>>(),
    }))
}

pub fn explain(id: &str) -> CommandResult {
    let config = config_for(id)?;
    let (snapshot, plan) = observe_and_plan(
        &FirstGroundedPlanner,
        &config.ir_resource,
        &HostMemoryObserver,
    )?;
    let candidate = plan.candidate();

    print_json(json!({
        "command": "explain",
        "resource_id": config.resource_spec.resource_id().as_str(),
        "planner": "FirstGroundedPlanner",
        "outcome": plan.outcome.to_string(),
        "reasoning": plan.reasoning,
        "candidate": candidate.map(|candidate| json!({
            "transition": format!("{:?}", candidate),
            "target": candidate.magnitude(),
            "declared_in_resource": candidate.is_declared_in(&config.ir_resource),
        })),
        "evidence": {
            "all_signals_valid": snapshot.all_signals_valid,
            "observations": render_observations(snapshot.iter()),
        },
    }))
}

fn transactional_ram(
    id: &str,
    options: RamCommandOptions,
) -> Result<TransactionalRam, Box<dyn Error>> {
    Ok(TransactionalRam::new(
        id,
        options.host_total,
        options.min,
        options.max,
        options.initial,
        options.max_step,
    )?)
}

fn config_for(id: &str) -> Result<RuntimeConfig, Box<dyn Error>> {
    let config = RuntimeConfig::default();
    let configured = config.resource_spec.resource_id().as_str();
    if id != configured {
        return Err(IoError::new(
            ErrorKind::InvalidInput,
            format!(
                "unknown resource '{id}'; the bootstrap CLI currently exposes only '{configured}'"
            ),
        )
        .into());
    }
    Ok(config)
}

fn render_observations<'a>(observations: impl Iterator<Item = &'a Observation>) -> Vec<Value> {
    observations
        .map(|observation| {
            let value = if observation.is_valid() && observation.value().is_finite() {
                Some(observation.value())
            } else {
                None
            };
            json!({
                "source": observation.source().to_string(),
                "signal": observation.signal().to_string(),
                "value": value,
                "quality": observation.quality(),
                "valid": observation.is_valid(),
                "unsupported_reason": observation.unsupported_reason(),
            })
        })
        .collect()
}

fn print_json(value: Value) -> CommandResult {
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ram_options(target: u64) -> RamCommandOptions {
        RamCommandOptions {
            host_total: 4096,
            min: 512,
            max: 4096,
            initial: 1024,
            target,
            max_step: Some(2048),
        }
    }

    #[test]
    fn bootstrap_resource_is_resolved_explicitly() {
        let config = config_for("default").expect("default resource should resolve");
        assert_eq!(config.resource_spec.resource_id().as_str(), "default");
    }

    #[test]
    fn unknown_resource_is_rejected() {
        let error = config_for("missing").expect_err("unknown resources must fail");
        assert!(error.to_string().contains("unknown resource 'missing'"));
    }

    #[test]
    fn target_planner_uses_declared_capacity_transition() {
        let adapter = transactional_ram("ram", ram_options(2048)).unwrap();
        let resource = adapter.ir().unwrap();
        let plan = plan_with_context(
            &CapacityTargetPlanner { target: 2048 },
            &resource,
            &elastic_eir::PlanningContext::new(),
        );
        assert_eq!(plan.candidate().and_then(|candidate| candidate.magnitude()), Some(2048));
    }

    #[test]
    fn invalid_target_is_rejected_before_effect() {
        let adapter = transactional_ram("ram", ram_options(8192)).unwrap();
        let resource = adapter.ir().unwrap();
        let plan = plan_with_context(
            &CapacityTargetPlanner { target: 8192 },
            &resource,
            &elastic_eir::PlanningContext::new(),
        );
        assert!(adapter.validate(&plan).is_err());
        assert_eq!(adapter.committed().unwrap(), 1024);
    }

    #[test]
    fn unsupported_observation_serializes_without_nan() {
        let observation = Observation::unsupported_from_source(
            elastic_runtime::ObservationSource::host("test"),
            elastic_core::resource::ObservationSignalId::FREE_CAPACITY,
            std::time::Instant::now(),
            "unavailable",
        );
        let rendered = render_observations(std::iter::once(&observation));
        assert!(rendered[0]["value"].is_null());
        assert_eq!(rendered[0]["valid"], false);
        assert_eq!(rendered[0]["unsupported_reason"], "unavailable");
    }
}
