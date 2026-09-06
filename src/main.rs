use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

/// Extract and compare Product Manufacturing Information (PMI) from 3D models.
#[derive(Debug, Parser)]
#[command(name = "pmix", version, about, long_about = None)]
struct Cli {
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
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Extract {
            input,
            output,
            compact,
        } => {
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
