use clap::{Parser, Subcommand};

mod commands;
use commands::*;

#[derive(Parser)]
#[command(name = "elastic", about = "Elastic runtime CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Inspect { id: String },
    Observe { id: String },
    Plan { id: String },
    Validate { id: String },
    Apply { id: String },
    Run { id: String },
    Watch { id: String, interval_ms: Option<u64> },
    Explain { id: String },
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Inspect { id } => inspect(&id),
        Commands::Observe { id } => observe(&id),
        Commands::Plan { id } => plan(&id),
        Commands::Validate { id } => validate(&id),
        Commands::Apply { id } => apply(&id),
        Commands::Run { id } => run(&id),
        Commands::Watch { id, interval_ms } => watch(&id, interval_ms),
        Commands::Explain { id } => explain(&id),
    }
}
