use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};

mod commands;
use commands::*;

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
    /// Run one adaptive transactional control cycle.
    Run {
        id: String,
        #[command(flatten)]
        ram: AdaptiveRamArgs,
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
        Commands::Run { id, ram } => run(&id, ram.into()),
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
