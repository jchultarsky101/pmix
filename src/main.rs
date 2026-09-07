use std::collections::BTreeSet;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{ArgAction, Parser, Subcommand};
use tracing_subscriber::EnvFilter;

use pmix::step::p21::{self, Exchange, Id, Instance};

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

        /// Include full annotation geometry (coordinates and triangles)
        /// instead of only the summary.
        #[arg(long)]
        presentation_geometry: bool,
    },

    /// Explore the raw entity graph of a STEP file.
    ///
    /// With no options, prints the header, a summary, and a count of every
    /// entity type. Use --entity to dump specific instances (and what they
    /// reference, up to --depth), or --type to list all instances of a type.
    Inspect {
        /// Path to the STEP file.
        input: PathBuf,

        /// Dump these instances (`#12` or `12`). Repeatable.
        #[arg(short, long = "entity", value_name = "ID", value_parser = parse_id)]
        entities: Vec<Id>,

        /// When dumping instances, also dump what they reference, this many
        /// levels deep.
        #[arg(short, long, default_value_t = 0)]
        depth: usize,

        /// List every instance having a segment of this entity type
        /// (case-insensitive). Repeatable.
        #[arg(short, long = "type", value_name = "KEYWORD")]
        types: Vec<String>,

        /// List only complex-instance type combinations.
        #[arg(long)]
        complex: bool,

        /// Show parser diagnostics.
        #[arg(long)]
        diagnostics: bool,

        /// Emit JSON instead of text.
        #[arg(long)]
        json: bool,
    },
}

fn parse_id(s: &str) -> std::result::Result<Id, String> {
    s.trim_start_matches('#')
        .parse()
        .map_err(|_| format!("`{s}` is not an instance id like `#12`"))
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
            presentation_geometry,
        } => extract(
            input,
            output,
            compact,
            pmix::ExtractOptions {
                presentation_geometry,
            },
        ),
        Command::Inspect {
            input,
            entities,
            depth,
            types,
            complex,
            diagnostics,
            json,
        } => inspect(
            input,
            InspectOptions {
                entities,
                depth,
                types,
                complex,
                diagnostics,
                json,
            },
        ),
    }
}

fn extract(
    input: PathBuf,
    output: Option<PathBuf>,
    compact: bool,
    options: pmix::ExtractOptions,
) -> Result<()> {
    tracing::info!(input = %input.display(), "extracting PMI");
    let document = pmix::extract_with(&input, &options)
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

struct InspectOptions {
    entities: Vec<Id>,
    depth: usize,
    types: Vec<String>,
    complex: bool,
    diagnostics: bool,
    json: bool,
}

fn inspect(input: PathBuf, opts: InspectOptions) -> Result<()> {
    let bytes =
        std::fs::read(&input).with_context(|| format!("failed to read `{}`", input.display()))?;
    tracing::info!(input = %input.display(), bytes = bytes.len(), "parsing");
    let ex = p21::parse_bytes(&bytes)
        .with_context(|| format!("failed to parse `{}`", input.display()))?;
    tracing::debug!(
        instances = ex.len(),
        diagnostics = ex.diagnostics.len(),
        "parsed"
    );

    let stdout = std::io::stdout();
    let mut out = std::io::BufWriter::new(stdout.lock());

    let selecting = !opts.entities.is_empty() || !opts.types.is_empty();
    if selecting {
        // Collect the closure of requested instances, breadth-first.
        let mut selected: BTreeSet<Id> = BTreeSet::new();
        let mut frontier: Vec<Id> = opts.entities.clone();
        for t in &opts.types {
            frontier.extend(ex.of_type(t).map(|i| i.id));
        }
        for id in &opts.entities {
            if ex.get(*id).is_none() {
                eprintln!("warning: no instance #{id}");
            }
        }
        for _ in 0..=opts.depth {
            let mut next = Vec::new();
            for id in frontier.drain(..) {
                if selected.insert(id) {
                    if let Some(inst) = ex.get(id) {
                        next.extend(inst.references());
                    }
                }
            }
            frontier = next;
        }
        let instances: Vec<&Instance> = selected.iter().filter_map(|id| ex.get(*id)).collect();
        if opts.json {
            serde_json::to_writer_pretty(&mut out, &instances)?;
            writeln!(out)?;
        } else {
            for inst in instances {
                writeln!(out, "{inst}")?;
            }
        }
    } else if opts.json {
        serde_json::to_writer_pretty(&mut out, &Summary::new(&ex, &opts))?;
        writeln!(out)?;
    } else {
        print_summary(&mut out, &ex, &opts)?;
    }

    if opts.diagnostics && !opts.json {
        writeln!(out)?;
        if ex.diagnostics.is_empty() {
            writeln!(out, "no diagnostics")?;
        }
        for d in &ex.diagnostics {
            writeln!(out, "{d}")?;
        }
    }
    out.flush()?;
    Ok(())
}

#[derive(serde::Serialize)]
struct Summary<'a> {
    header: &'a p21::Header,
    instances: usize,
    complex_instances: usize,
    entity_types: Vec<TypeCount>,
    complex_types: Vec<TypeCount>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    diagnostics: Vec<&'a p21::Diagnostic>,
}

#[derive(serde::Serialize)]
struct TypeCount {
    keyword: String,
    count: usize,
}

impl<'a> Summary<'a> {
    fn new(ex: &'a Exchange, opts: &InspectOptions) -> Self {
        let to_tc = |(keyword, count)| TypeCount { keyword, count };
        Self {
            header: &ex.header,
            instances: ex.len(),
            complex_instances: ex.instances().filter(|i| i.is_complex()).count(),
            entity_types: if opts.complex {
                Vec::new()
            } else {
                ex.type_counts().into_iter().map(to_tc).collect()
            },
            complex_types: ex.complex_type_counts().into_iter().map(to_tc).collect(),
            diagnostics: if opts.diagnostics {
                ex.diagnostics.iter().collect()
            } else {
                Vec::new()
            },
        }
    }
}

fn print_summary(out: &mut impl Write, ex: &Exchange, opts: &InspectOptions) -> Result<()> {
    let h = &ex.header;
    writeln!(out, "file name:     {}", h.file_name().unwrap_or("-"))?;
    writeln!(out, "time stamp:    {}", h.time_stamp().unwrap_or("-"))?;
    writeln!(
        out,
        "preprocessor:  {}",
        h.preprocessor_version().unwrap_or("-")
    )?;
    writeln!(
        out,
        "origin system: {}",
        h.originating_system().unwrap_or("-")
    )?;
    writeln!(out, "schema:        {}", h.schema_identifiers().join(", "))?;
    for d in h.description() {
        writeln!(out, "description:   {d}")?;
    }
    let complex = ex.instances().filter(|i| i.is_complex()).count();
    writeln!(out)?;
    writeln!(
        out,
        "{} instances ({complex} complex), {} entity types, {} diagnostics",
        ex.len(),
        ex.type_counts().len(),
        ex.diagnostics.len()
    )?;

    if !opts.complex {
        writeln!(out)?;
        writeln!(out, "entity types:")?;
        for (keyword, count) in ex.type_counts() {
            writeln!(out, "{count:>8}  {keyword}")?;
        }
    }
    let complex_types = ex.complex_type_counts();
    if !complex_types.is_empty() {
        writeln!(out)?;
        writeln!(out, "complex instance combinations:")?;
        for (key, count) in complex_types {
            writeln!(out, "{count:>8}  {}", key.replace('+', " + "))?;
        }
    }
    Ok(())
}
