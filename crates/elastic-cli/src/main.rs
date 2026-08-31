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
