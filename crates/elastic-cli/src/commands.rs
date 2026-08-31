use std::error::Error;
use std::io::{Error as IoError, ErrorKind};

use elastic_eir::FirstGroundedPlanner;
use elastic_runtime::control_loop::{collect_observations, observe_and_plan};
use elastic_runtime::{HostMemoryObserver, Observation, RuntimeConfig};
use serde_json::{json, Value};

type CommandResult = Result<(), Box<dyn Error>>;

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
