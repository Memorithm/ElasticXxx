use std::error::Error;
use std::io::{Error as IoError, ErrorKind};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};

mod commands;
mod config_run;
use commands::*;
use config_run::run_config;

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
    #[arg(value_name = "ID", required_unless_present = "config", conflicts_with = "config")]
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
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Commands::Inspect { id } => inspect(&id),
        Commands::Observe { id } => observe(&id),
        Commands::Plan { id } => plan(&id),
        Commands::Validate { id, ram } => validate(&id, ram.into()),
        Commands::Apply { id, ram } => apply(&id, ram.into()),
        Commands::Run { args } => args.execute(),
        Commands::Watch {
            id,
            ram,
            interval_ms,
            max_cycles,
        } => watch(&id, ram.into(), interval_ms, max_cycles),
        Commands::Explain { id } => explain(&id),
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
}
