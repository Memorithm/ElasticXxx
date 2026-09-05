use std::error::Error;
use std::io::{Error as IoError, ErrorKind};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};

mod commands;
mod config_run;
mod evidence;
mod model_plan;
use commands::*;
use config_run::{run_config, run_config_to_file};
use evidence::{diff, replay};
use model_plan::{model_plan, ModelPlanOptions};

#[derive(Parser)]
#[command(name = "elastic", about = "Elastic runtime CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Clone, Copy, Debug, Args)]
struct RamArgs {
    /// Operator-supplied maximum memory available to this resource, in bytes.
    #[arg(long)]
    host_total: u64,
    /// Minimum permitted commitment, in bytes.
    #[arg(long)]
    min: u64,
    /// Maximum permitted commitment, in bytes.
    #[arg(long)]
    max: u64,
    /// Initial real allocation, in bytes.
    #[arg(long)]
    initial: u64,
    /// Requested target commitment, in bytes.
    #[arg(long)]
    target: u64,
    /// Optional maximum absolute resize step, in bytes.
    #[arg(long)]
    max_step: Option<u64>,
}

impl From<RamArgs> for RamCommandOptions {
    fn from(args: RamArgs) -> Self {
        Self {
            host_total: args.host_total,
            min: args.min,
            max: args.max,
            initial: args.initial,
            target: args.target,
            max_step: args.max_step,
        }
    }
}

#[derive(Clone, Copy, Debug, Args)]
struct AdaptiveRamArgs {
    /// Operator-supplied maximum memory available to this resource, in bytes.
    #[arg(long)]
    host_total: u64,
    /// Minimum permitted commitment, in bytes.
    #[arg(long)]
    min: u64,
    /// Maximum permitted commitment, in bytes.
    #[arg(long)]
    max: u64,
    /// Initial real allocation, in bytes.
    #[arg(long)]
    initial: u64,
    /// Optional maximum absolute resize step, in bytes.
    #[arg(long)]
    max_step: Option<u64>,
    /// Desired free-memory fraction of the configured host total.
    #[arg(long)]
    headroom: f64,
    /// Fractional deadband around the desired headroom.
    #[arg(long, default_value_t = 0.0)]
    deadband: f64,
}

impl From<AdaptiveRamArgs> for AdaptiveRamOptions {
    fn from(args: AdaptiveRamArgs) -> Self {
        Self {
            host_total: args.host_total,
            min: args.min,
            max: args.max,
            initial: args.initial,
            max_step: args.max_step,
            headroom: args.headroom,
            deadband: args.deadband,
        }
    }
}

/// Sources accepted by `elastic run`.
///
/// The historical inline RAM form remains supported. A versioned operator
/// configuration is an exclusive alternative and may optionally select one
/// configured resource; without `--resource`, all configured controllers run
/// in canonical resource-id order.
#[derive(Debug, Args)]
struct RunArgs {
    #[arg(
        value_name = "ID",
        required_unless_present = "config",
        conflicts_with = "config"
    )]
    id: Option<String>,

    /// Versioned JSON operator configuration.
    #[arg(
        long,
        value_name = "FILE",
        conflicts_with_all = [
            "id",
            "host_total",
            "min",
            "max",
            "initial",
            "max_step",
            "headroom",
            "deadband"
        ]
    )]
    config: Option<PathBuf>,

    /// Run only this configured resource. By default all controllers run.
    #[arg(long, value_name = "ID", requires = "config")]
    resource: Option<String>,

    #[arg(long, required_unless_present = "config")]
    host_total: Option<u64>,
    #[arg(long, required_unless_present = "config")]
    min: Option<u64>,
    #[arg(long, required_unless_present = "config")]
    max: Option<u64>,
    #[arg(long, required_unless_present = "config")]
    initial: Option<u64>,
    #[arg(long)]
    max_step: Option<u64>,
    #[arg(long, required_unless_present = "config")]
    headroom: Option<f64>,
    #[arg(long)]
    deadband: Option<f64>,
}

impl RunArgs {
    fn execute(self) -> Result<(), Box<dyn Error>> {
        if let Some(config) = self.config {
            return run_config(&config, self.resource.as_deref());
        }

        let id = required(self.id, "resource ID")?;
        let options = AdaptiveRamOptions {
            host_total: required(self.host_total, "--host-total")?,
            min: required(self.min, "--min")?,
            max: required(self.max, "--max")?,
            initial: required(self.initial, "--initial")?,
            max_step: self.max_step,
            headroom: required(self.headroom, "--headroom")?,
            deadband: self.deadband.unwrap_or(0.0),
        };
        run(&id, options)
    }
}

#[derive(Debug, Args)]
struct HubRunArgs {
    /// Versioned JSON operator configuration.
    #[arg(long, value_name = "FILE")]
    config: PathBuf,

    /// Run only this configured resource. By default all controllers run.
    #[arg(long, value_name = "ID")]
    resource: Option<String>,

    /// New path that will receive one elastic-runtime-evidence-v1 artifact.
    #[arg(long, value_name = "FILE")]
    evidence_output: PathBuf,
}

impl HubRunArgs {
    fn execute(self) -> Result<(), Box<dyn Error>> {
        run_config_to_file(
            &self.config,
            self.resource.as_deref(),
            &self.evidence_output,
        )
    }
}

#[derive(Debug, Args)]
struct ModelPlanArgs {
    /// Preferred aggregate model-execution controller-contracts JSON.
    #[arg(
        long,
        value_name = "FILE",
        conflicts_with_all = ["capabilities", "profiles", "policy"]
    )]
    contracts: Option<PathBuf>,

    /// Strict model-execution capabilities JSON contract.
    #[arg(
        long,
        value_name = "FILE",
        required_unless_present = "contracts",
        conflicts_with = "contracts"
    )]
    capabilities: Option<PathBuf>,

    /// Strict correlated profile-set JSON contract bound to `--capabilities`.
    #[arg(
        long,
        value_name = "FILE",
        required_unless_present = "contracts",
        conflicts_with = "contracts"
    )]
    profiles: Option<PathBuf>,

    /// Strict resource-envelope policy JSON contract bound to `--profiles`.
    #[arg(
        long,
        value_name = "FILE",
        required_unless_present = "contracts",
        conflicts_with = "contracts"
    )]
    policy: Option<PathBuf>,

    /// Backend-owned capacity-unit identity used by this snapshot.
    #[arg(long)]
    capacity_unit: String,

    /// Observed free capacity in `--capacity-unit`.
    #[arg(long)]
    free_capacity: u64,

    /// Observed utilization in integer basis points, 0..=10000.
    #[arg(long)]
    utilization_bps: u16,

    /// Currently active correlated profile preference rank.
    #[arg(long)]
    current_profile_rank: u32,
}

impl ModelPlanArgs {
    fn execute(self) -> Result<(), Box<dyn Error>> {
        model_plan(ModelPlanOptions {
            contracts: self.contracts.as_deref(),
            capabilities: self.capabilities.as_deref(),
            profiles: self.profiles.as_deref(),
            policy: self.policy.as_deref(),
            capacity_unit: &self.capacity_unit,
            free_capacity: self.free_capacity,
            utilization_bps: self.utilization_bps,
            current_profile_rank: self.current_profile_rank,
        })
    }
}

fn required<T>(value: Option<T>, name: &str) -> Result<T, Box<dyn Error>> {
    value.ok_or_else(|| {
        IoError::new(
            ErrorKind::InvalidInput,
            format!("missing required inline run argument {name}"),
        )
        .into()
    })
}

#[derive(Subcommand)]
enum Commands {
    /// Inspect the normalized declaration and runtime configuration.
    Inspect { id: String },
    /// Collect real host observations with explicit provenance.
    Observe { id: String },
    /// Produce an auditable, non-actuating plan.
    Plan { id: String },
    /// Validate and select a qualified model-execution profile without actuation.
    ModelPlan {
        #[command(flatten)]
        args: ModelPlanArgs,
    },
    /// Check runtime prerequisites without mutating state.
    Doctor { id: String },
    /// Validate an explicit RAM target through the trusted adapter boundary.
    Validate {
        id: String,
        #[command(flatten)]
        ram: RamArgs,
    },
    /// Apply an explicit RAM target transactionally and verify the result.
    Apply {
        id: String,
        #[command(flatten)]
        ram: RamArgs,
    },
    /// Run an inline RAM controller or a versioned operator configuration.
    Run {
        #[command(flatten)]
        args: RunArgs,
    },
    /// Run a versioned operator configuration and materialize bounded evidence.
    HubRun {
        #[command(flatten)]
        args: HubRunArgs,
    },
    /// Run a bounded periodic adaptive controller.
    Watch {
        id: String,
        #[command(flatten)]
        ram: AdaptiveRamArgs,
        /// Milliseconds between completed cycles. Must be greater than zero.
        #[arg(long)]
        interval_ms: u64,
        /// Maximum number of cycles. Must be greater than zero.
        #[arg(long)]
        max_cycles: u64,
    },
    /// Explain the planner outcome and its observation evidence.
    Explain { id: String },
    /// Validate a captured JSON evidence record without actuating resources.
    Replay { input: PathBuf },
    /// Compare two captured JSON evidence records deterministically.
    Diff { left: PathBuf, right: PathBuf },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Commands::Inspect { id } => inspect(&id),
        Commands::Observe { id } => observe(&id),
        Commands::Plan { id } => plan(&id),
        Commands::ModelPlan { args } => args.execute(),
        Commands::Doctor { id } => doctor(&id),
        Commands::Validate { id, ram } => validate(&id, ram.into()),
        Commands::Apply { id, ram } => apply(&id, ram.into()),
        Commands::Run { args } => args.execute(),
        Commands::HubRun { args } => args.execute(),
        Commands::Watch {
            id,
            ram,
            interval_ms,
            max_cycles,
        } => watch(&id, ram.into(), interval_ms, max_cycles),
        Commands::Explain { id } => explain(&id),
        Commands::Replay { input } => replay(&input),
        Commands::Diff { left, right } => diff(&left, &right),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("elastic: {error}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doctor_syntax_parses_with_resource_id() {
        let cli = Cli::try_parse_from(["elastic", "doctor", "default"]).unwrap();

        assert!(matches!(
            cli.command,
            Commands::Doctor { id } if id == "default"
        ));
    }

    #[test]
    fn model_plan_split_syntax_remains_supported() {
        let cli = Cli::try_parse_from([
            "elastic",
            "model-plan",
            "--capabilities",
            "capabilities.json",
            "--profiles",
            "profiles.json",
            "--policy",
            "policy.json",
            "--capacity-unit",
            "bytes",
            "--free-capacity",
            "3000",
            "--utilization-bps",
            "8000",
            "--current-profile-rank",
            "0",
        ])
        .unwrap();

        match cli.command {
            Commands::ModelPlan { args } => {
                assert!(args.contracts.is_none());
                assert_eq!(args.capabilities, Some(PathBuf::from("capabilities.json")));
                assert_eq!(args.profiles, Some(PathBuf::from("profiles.json")));
                assert_eq!(args.policy, Some(PathBuf::from("policy.json")));
                assert_eq!(args.capacity_unit, "bytes");
                assert_eq!(args.free_capacity, 3000);
                assert_eq!(args.utilization_bps, 8000);
                assert_eq!(args.current_profile_rank, 0);
            }
            _ => panic!("expected model-plan command"),
        }
    }

    #[test]
    fn model_plan_accepts_aggregate_contract_bundle() {
        let cli = Cli::try_parse_from([
            "elastic",
            "model-plan",
            "--contracts",
            "model-contracts.json",
            "--capacity-unit",
            "bytes",
            "--free-capacity",
            "3000",
            "--utilization-bps",
            "8000",
            "--current-profile-rank",
            "0",
        ])
        .unwrap();

        match cli.command {
            Commands::ModelPlan { args } => {
                assert_eq!(args.contracts, Some(PathBuf::from("model-contracts.json")));
                assert!(args.capabilities.is_none());
                assert!(args.profiles.is_none());
                assert!(args.policy.is_none());
            }
            _ => panic!("expected model-plan command"),
        }
    }

    #[test]
    fn model_plan_contract_sources_are_mutually_exclusive_and_complete() {
        let mixed = Cli::try_parse_from([
            "elastic",
            "model-plan",
            "--contracts",
            "model-contracts.json",
            "--capabilities",
            "capabilities.json",
            "--capacity-unit",
            "bytes",
            "--free-capacity",
            "3000",
            "--utilization-bps",
            "8000",
            "--current-profile-rank",
            "0",
        ]);
        assert!(mixed.is_err());

        let incomplete = Cli::try_parse_from([
            "elastic",
            "model-plan",
            "--capabilities",
            "capabilities.json",
            "--capacity-unit",
            "bytes",
            "--free-capacity",
            "3000",
            "--utilization-bps",
            "8000",
            "--current-profile-rank",
            "0",
        ]);
        assert!(incomplete.is_err());
    }

    #[test]
    fn configured_run_syntax_parses_without_inline_arguments() {
        let cli = Cli::try_parse_from([
            "elastic",
            "run",
            "--config",
            "docs/config/operator-v1.example.json",
            "--resource",
            "ram-budget",
        ])
        .unwrap();

        match cli.command {
            Commands::Run { args } => {
                assert_eq!(
                    args.config,
                    Some(PathBuf::from("docs/config/operator-v1.example.json"))
                );
                assert_eq!(args.resource.as_deref(), Some("ram-budget"));
                assert!(args.id.is_none());
            }
            _ => panic!("expected run command"),
        }
    }

    #[test]
    fn hub_run_requires_explicit_config_and_evidence_artifact() {
        let cli = Cli::try_parse_from([
            "elastic",
            "hub-run",
            "--config",
            "operator.json",
            "--resource",
            "ram",
            "--evidence-output",
            "runtime-evidence.json",
        ])
        .unwrap();

        match cli.command {
            Commands::HubRun { args } => {
                assert_eq!(args.config, PathBuf::from("operator.json"));
                assert_eq!(args.resource.as_deref(), Some("ram"));
                assert_eq!(args.evidence_output, PathBuf::from("runtime-evidence.json"));
            }
            _ => panic!("expected hub-run command"),
        }
    }

    #[test]
    fn historical_inline_run_syntax_still_parses() {
        let cli = Cli::try_parse_from([
            "elastic",
            "run",
            "ram",
            "--host-total",
            "4096",
            "--min",
            "512",
            "--max",
            "4096",
            "--initial",
            "1024",
            "--headroom",
            "0.5",
        ])
        .unwrap();

        match cli.command {
            Commands::Run { args } => {
                assert_eq!(args.id.as_deref(), Some("ram"));
                assert!(args.config.is_none());
                assert_eq!(args.host_total, Some(4096));
                assert_eq!(args.headroom, Some(0.5));
            }
            _ => panic!("expected run command"),
        }
    }

    #[test]
    fn configured_and_inline_run_sources_cannot_be_mixed() {
        let result = Cli::try_parse_from([
            "elastic",
            "run",
            "ram",
            "--config",
            "docs/config/operator-v1.example.json",
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn replay_and_diff_syntax_parse_with_paths() {
        let replay = Cli::try_parse_from(["elastic", "replay", "run.json"]).unwrap();
        assert!(
            matches!(replay.command, Commands::Replay { input } if input.as_path() == std::path::Path::new("run.json"))
        );

        let diff = Cli::try_parse_from(["elastic", "diff", "left.json", "right.json"]).unwrap();
        assert!(
            matches!(diff.command, Commands::Diff { left, right } if left.as_path() == std::path::Path::new("left.json") && right.as_path() == std::path::Path::new("right.json"))
        );
    }
}
