use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{ArgAction, Parser, Subcommand};
use tracing_subscriber::EnvFilter;

/// Extract and compare Product Manufacturing Information (PMI) from 3D models.
#[derive(Debug, Parser)]
#[command(name = "pmix", version, about, long_about = None)]
struct Cli {
    /// Increase log verbosity (-v for debug, -vv for trace). Overrides RUST_LOG.
    #[arg(short, long, action = ArgAction::Count, global = true)]
    verbose: u8,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Extract PMI from a STEP or JT file and write it as JSON.
    Extract {
        /// Path to the input model (.stp, .step, or .jt).
        input: PathBuf,

        /// Where to write the JSON output. Defaults to standard output.
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Emit compact JSON instead of pretty-printed JSON.
        #[arg(long)]
        compact: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    init_tracing(cli.verbose);

    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::FAILURE
        }
    }
}

/// Configure logging. `RUST_LOG` is honoured unless `-v` flags are given,
/// in which case they take precedence. Logs go to stderr so that JSON on
/// stdout stays clean.
fn init_tracing(verbosity: u8) {
    let filter = match verbosity {
        0 => EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
        1 => EnvFilter::new("info,pmix=debug"),
        _ => EnvFilter::new("debug,pmix=trace"),
    };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_target(false)
        .init();
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Extract {
            input,
            output,
            compact,
        } => {
            tracing::info!(input = %input.display(), "extracting PMI");
            let document = pmix::extract(&input)
                .with_context(|| format!("failed to extract PMI from `{}`", input.display()))?;

            let json = if compact {
                serde_json::to_string(&document)?
            } else {
                serde_json::to_string_pretty(&document)?
            };

            match output {
                Some(path) => std::fs::write(&path, json)
                    .with_context(|| format!("failed to write `{}`", path.display()))?,
                None => println!("{json}"),
            }
            Ok(())
        }
    }
}
