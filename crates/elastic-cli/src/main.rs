use std::error::Error;
use std::io::{Error as IoError, ErrorKind};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};

mod capacity_admission;
mod commands;
mod config_run;
mod evidence;
mod guard_analysis;
mod guard_cli;
mod model_contracts;
mod model_plan;
use commands::*;
use config_run::{run_config, run_config_to_file};
use evidence::{diff, replay};
use guard_analysis::analyze as guard_analyze;
use guard_cli::{
    check as guard_check, eval as guard_eval, explain as guard_explain,
    fingerprint as guard_fingerprint, list as guard_list, plan_dry_run as guard_plan_dry_run,
};
use model_contracts::{build_contracts, validate_contracts};
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

#[derive(Debug, Subcommand)]
enum ModelContractsCommand {
    /// Build one strict aggregate controller-contract bundle as a new JSON file.
    Build {
        #[arg(long, value_name = "FILE")]
        capabilities: PathBuf,
        #[arg(long, value_name = "FILE")]
        profiles: PathBuf,
        #[arg(long, value_name = "FILE")]
        policy: PathBuf,
        #[arg(long, value_name = "FILE")]
        output: PathBuf,
    },
    /// Revalidate one aggregate controller-contract bundle without actuation.
    Validate { input: PathBuf },
}

impl ModelContractsCommand {
    fn execute(self) -> Result<(), Box<dyn Error>> {
        match self {
            Self::Build {
                capabilities,
                profiles,
                policy,
                output,
            } => build_contracts(&capabilities, &profiles, &policy, &output),
            Self::Validate { input } => validate_contracts(&input),
        }
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
    /// Admit an independent-work capacity envelope from strict bounded stdin JSON.
    /// This controls a local permit ledger; the external executor must enforce it.
    AdmitCapacity {
        /// Independently expected workload identity (SHA-256).
        #[arg(long)]
        expected_plan_id: String,
        /// Independently expected executor environment (SHA-256).
        #[arg(long)]
        expected_environment_id: String,
    },
    /// Inspect the normalized declaration and runtime configuration.
    Inspect { id: String },
    /// Collect real host observations with explicit provenance.
    Observe { id: String },
    /// Produce an auditable, non-actuating plan.
    Plan { id: String },
    /// Build or validate persisted adaptive model-execution contract bundles.
    ModelContracts {
        #[command(subcommand)]
        command: ModelContractsCommand,
    },
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
    /// Validate a strict versioned Boolean guard configuration without actuation.
    GuardCheck { config: PathBuf },
    /// List stable predicates and canonical guards without actuation.
    GuardList { config: PathBuf },
    /// Report canonical guard-expression fingerprints without actuation.
    GuardFingerprint { config: PathBuf },
    /// Evaluate configured guards from explicit stable-key facts. Missing facts are Unknown.
    GuardEval {
        config: PathBuf,
        #[arg(long = "fact", value_name = "NAMESPACE::NAME=TRUTH")]
        facts: Vec<String>,
    },
    /// Explain three-valued guard results and the explicit/missing fact partition.
    GuardExplain {
        config: PathBuf,
        #[arg(long = "fact", value_name = "NAMESPACE::NAME=TRUTH")]
        facts: Vec<String>,
    },
    /// Run bounded exact Boolean analysis over configured guards without actuation.
    GuardAnalyze {
        config: PathBuf,
        /// Maximum distinct predicates permitted in one exact query.
        #[arg(long, default_value_t = elastic::DEFAULT_EXACT_ORACLE_VARIABLES)]
        max_variables: usize,
        /// Maximum assignments permitted in one exact query.
        #[arg(long, default_value_t = elastic::DEFAULT_EXACT_ORACLE_ASSIGNMENTS)]
        max_assignments: usize,
    },
    /// Perform guarded numeric planning against configured observations without validation or actuation.
    GuardPlanDryRun {
        #[arg(long, value_name = "FILE")]
        operator_config: PathBuf,
        /// Optional external guard policy. Omit when the selected controller embeds `guard_config`.
        #[arg(long, value_name = "FILE")]
        guard_config: Option<PathBuf>,
        #[arg(long, value_name = "ID")]
        resource: String,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Commands::AdmitCapacity {
            expected_plan_id,
            expected_environment_id,
        } => capacity_admission::run(&expected_plan_id, &expected_environment_id),
        Commands::Inspect { id } => inspect(&id),
        Commands::Observe { id } => observe(&id),
        Commands::Plan { id } => plan(&id),
        Commands::ModelContracts { command } => command.execute(),
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
        Commands::GuardCheck { config } => guard_check(&config),
        Commands::GuardList { config } => guard_list(&config),
        Commands::GuardFingerprint { config } => guard_fingerprint(&config),
        Commands::GuardEval { config, facts } => guard_eval(&config, &facts),
        Commands::GuardExplain { config, facts } => guard_explain(&config, &facts),
        Commands::GuardAnalyze {
            config,
            max_variables,
            max_assignments,
        } => guard_analyze(&config, max_variables, max_assignments),
        Commands::GuardPlanDryRun {
            operator_config,
            guard_config,
            resource,
        } => guard_plan_dry_run(&operator_config, guard_config.as_deref(), &resource),
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
    fn model_contracts_build_and_validate_syntax_parse() {
        let build = Cli::try_parse_from([
            "elastic",
            "model-contracts",
            "build",
            "--capabilities",
            "capabilities.json",
            "--profiles",
            "profiles.json",
            "--policy",
            "policy.json",
            "--output",
            "contracts.json",
        ])
        .unwrap();
        assert!(matches!(
            build.command,
            Commands::ModelContracts {
                command: ModelContractsCommand::Build { output, .. }
            } if output == PathBuf::from("contracts.json")
        ));

        let validate =
            Cli::try_parse_from(["elastic", "model-contracts", "validate", "contracts.json"])
                .unwrap();
        assert!(matches!(
            validate.command,
            Commands::ModelContracts {
                command: ModelContractsCommand::Validate { input }
            } if input == PathBuf::from("contracts.json")
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
    #[test]
    fn guard_read_only_command_syntax_uses_stable_fact_assignments() {
        let check = Cli::try_parse_from(["elastic", "guard-check", "guards.json"]).unwrap();
        assert!(
            matches!(check.command, Commands::GuardCheck { config } if config == PathBuf::from("guards.json"))
        );

        let eval = Cli::try_parse_from([
            "elastic",
            "guard-eval",
            "guards.json",
            "--fact",
            "elastic.ram::healthy=true",
        ])
        .unwrap();
        assert!(matches!(
            eval.command,
            Commands::GuardEval { config, facts }
                if config == PathBuf::from("guards.json")
                    && facts == ["elastic.ram::healthy=true"]
        ));
    }
    #[test]
    fn guard_plan_dry_run_syntax_accepts_embedded_or_external_guard_config() {
        let embedded = Cli::try_parse_from([
            "elastic",
            "guard-plan-dry-run",
            "--operator-config",
            "operator.json",
            "--resource",
            "ram",
        ])
        .unwrap();
        assert!(matches!(
            embedded.command,
            Commands::GuardPlanDryRun {
                operator_config,
                guard_config: None,
                resource,
            } if operator_config == PathBuf::from("operator.json") && resource == "ram"
        ));

        let external = Cli::try_parse_from([
            "elastic",
            "guard-plan-dry-run",
            "--operator-config",
            "operator.json",
            "--guard-config",
            "guards.json",
            "--resource",
            "ram",
        ])
        .unwrap();
        assert!(matches!(
            external.command,
            Commands::GuardPlanDryRun {
                operator_config,
                guard_config: Some(guard_config),
                resource,
            } if operator_config == PathBuf::from("operator.json")
                && guard_config == PathBuf::from("guards.json")
                && resource == "ram"
        ));
    }
}
