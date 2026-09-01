use std::error::Error;
use std::fs;
use std::path::Path;

use crate::evidence::{print_json, EVIDENCE_SCHEMA};
use elastic_runtime::{
    CancellationToken, ConfiguredController, ConfiguredResourceState, Forecast, OperatorConfig,
    RuntimeEvent,
};
use serde_json::{json, Value};

type CommandResult = Result<(), Box<dyn Error>>;

/// Execute one versioned JSON operator configuration.
///
/// When `resource` is omitted, every configured controller is materialized and
/// run in canonical resource-id order. Each controller remains its own trusted
/// transaction boundary; this command does not invent a cross-resource atomic
/// transaction.
pub fn run_config(path: &Path, resource: Option<&str>) -> CommandResult {
    let contents = fs::read_to_string(path)?;
    let config: OperatorConfig = serde_json::from_str(&contents)?;
    let output = execute_operator_config(&config, resource)?;
    print_json(output)
}

fn execute_operator_config(
    config: &OperatorConfig,
    resource: Option<&str>,
) -> Result<Value, Box<dyn Error>> {
    config.validate()?;
    let mut controllers = match resource {
        Some(resource_id) => vec![config.build_controller(resource_id)?],
        None => config.build_controllers()?,
    };

    let mut executions = Vec::with_capacity(controllers.len());
    for controller in &mut controllers {
        executions.push(execute_controller(controller)?);
    }

    Ok(json!({
        "command": "run",
        "source": "operator-config",
        "evidence_schema": EVIDENCE_SCHEMA,
        "config_version": config.version,
        "selected_resource": resource,
        "controllers": executions,
    }))
}

fn execute_controller(controller: &mut ConfiguredController) -> Result<Value, Box<dyn Error>> {
    let resource_id = controller.resource().identity().as_str().to_owned();
    let cancellation = CancellationToken::new();
    let result = controller.run(&cancellation)?;
    let final_state = controller.actuator().state()?;

    Ok(json!({
        "resource_id": resource_id,
        "stop_reason": format!("{:?}", result.stop_reason),
        "final_state": render_resource_state(final_state),
        "cycles": result.cycles.iter().enumerate().map(|(index, cycle)| {
            let transaction = &cycle.transaction;
            json!({
                "index": index,
                "forecast": cycle.forecast.as_ref().map(render_forecast),
                "candidate_target": transaction.plan.as_ref().and_then(|validated| {
                    validated.plan.candidate().and_then(|candidate| candidate.magnitude())
                }),
                "validated": transaction.plan.as_ref().is_some_and(|validated| validated.validated),
                "committed": transaction.commit.is_some(),
                "rolled_back": transaction.rollback.is_some(),
                "verification": transaction.verification.as_ref().map(|verification| format!("{verification:?}")),
                "events": cycle.events().map(render_event).collect::<Vec<_>>(),
            })
        }).collect::<Vec<_>>(),
        "events": result.events.iter().map(render_event).collect::<Vec<_>>(),
    }))
}

fn render_forecast(forecast: &Forecast) -> Value {
    json!({
        "status": format!("{:?}", forecast.status),
        "method": forecast.method,
        "horizon_ms": forecast.horizon.as_millis(),
        "confidence": forecast.confidence,
        "detail": forecast.detail,
        "context": forecast.planning_context().map(|context| {
            context
                .iter()
                .map(|(signal, value)| json!({
                    "signal": signal.to_string(),
                    "value": value,
                }))
                .collect::<Vec<_>>()
        }),
    })
}

fn render_event(event: &RuntimeEvent) -> Value {
    json!({
        "kind": format!("{:?}", event.kind),
        "details": event.details,
    })
}

fn render_resource_state(state: ConfiguredResourceState) -> Value {
    match state {
        ConfiguredResourceState::Ram { committed_bytes } => json!({
            "kind": "ram",
            "committed_bytes": committed_bytes,
        }),
        ConfiguredResourceState::Concurrency { width } => json!({
            "kind": "concurrency",
            "width": width,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHIPPED_EXAMPLE: &str = include_str!("../../../docs/config/operator-v1.example.json");

    fn apply_ram_json() -> &'static str {
        r#"{
          "version": 1,
          "resources": [{
            "adapter": "ram",
            "id": "ram",
            "host_total": 4096,
            "min": 512,
            "max": 4096,
            "initial": 1024,
            "max_step": 2048
          }],
          "controllers": [{
            "resource": "ram",
            "planner": {
              "kind": "headroom",
              "headroom_fraction": 0.5,
              "deadband_fraction": 0.0
            },
            "forecaster": {
              "kind": "ewma",
              "alpha": 0.5,
              "horizon_ms": 1000
            },
            "cadence": { "kind": "one-shot" },
            "mode": "apply"
          }]
        }"#
    }

    #[test]
    fn shipped_example_parses_materializes_and_remains_non_actuating() {
        let config: OperatorConfig = serde_json::from_str(SHIPPED_EXAMPLE).unwrap();
        let output = execute_operator_config(&config, None).unwrap();

        assert_eq!(output["config_version"], 1);
        assert_eq!(output["controllers"][0]["resource_id"], "ram-budget");
        assert_eq!(
            output["controllers"][0]["final_state"]["committed_bytes"],
            1_048_576
        );
        assert_eq!(output["controllers"][0]["cycles"][0]["committed"], false);
        assert_eq!(output["controllers"][0]["cycles"][0]["validated"], true);
    }

    #[test]
    fn json_config_materializes_and_executes_verified_pipeline() {
        let config: OperatorConfig = serde_json::from_str(apply_ram_json()).unwrap();
        let output = execute_operator_config(&config, None).unwrap();

        assert_eq!(output["config_version"], 1);
        assert_eq!(output["controllers"][0]["resource_id"], "ram");
        assert_eq!(
            output["controllers"][0]["final_state"]["committed_bytes"],
            2048
        );
        assert_eq!(output["controllers"][0]["cycles"][0]["committed"], true);
        assert_eq!(
            output["controllers"][0]["cycles"][0]["forecast"]["method"],
            "ewma"
        );
    }

    #[test]
    fn explicit_resource_selection_rejects_unknown_controller() {
        let config: OperatorConfig = serde_json::from_str(apply_ram_json()).unwrap();
        let error = execute_operator_config(&config, Some("missing")).unwrap_err();
        assert!(error.to_string().contains("no configured controller"));
    }

    #[test]
    fn all_controllers_execute_in_canonical_resource_order() {
        let json = r#"{
          "version": 1,
          "resources": [
            {"adapter":"ram","id":"z-ram","host_total":4096,"min":512,"max":4096,"initial":1024,"max_step":2048},
            {"adapter":"ram","id":"a-ram","host_total":4096,"min":512,"max":4096,"initial":1024,"max_step":2048}
          ],
          "controllers": [
            {
              "resource":"z-ram",
              "planner":{"kind":"headroom","headroom_fraction":0.5,"deadband_fraction":0.0},
              "forecaster":{"kind":"current-state"},
              "cadence":{"kind":"one-shot"},
              "mode":"plan-only"
            },
            {
              "resource":"a-ram",
              "planner":{"kind":"headroom","headroom_fraction":0.5,"deadband_fraction":0.0},
              "forecaster":{"kind":"current-state"},
              "cadence":{"kind":"one-shot"},
              "mode":"plan-only"
            }
          ]
        }"#;
        let config: OperatorConfig = serde_json::from_str(json).unwrap();
        let output = execute_operator_config(&config, None).unwrap();

        assert_eq!(output["controllers"][0]["resource_id"], "a-ram");
        assert_eq!(output["controllers"][1]["resource_id"], "z-ram");
    }
}
