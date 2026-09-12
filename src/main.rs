use std::collections::{BTreeMap, BTreeSet};
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

    /// Recognise manufacturing features in a model's geometry.
    ///
    /// Reads holes, counterbores, countersinks, and bosses from the
    /// B-rep and writes them with their diameters, depths, and
    /// positions, in millimetres whatever the file declared. Every face
    /// that went into no feature is listed too, so that what was not
    /// recognised cannot be mistaken for what is not there.
    ///
    /// Given two or more models, compares them instead: what pairs
    /// exactly, what is left over on each side, and a displacement
    /// stated once where one explains the lot. It will not decide
    /// whether a feature moved or was removed and another added, because
    /// geometry cannot tell those apart. Exit status is 0 when nothing
    /// differs, 1 when something does, 2 on error.
    ///
    /// This is not PMI: nothing in the file states it. It is a
    /// description of the shape, meant to be compared with another.
    Features {
        /// One model to describe, or two or more to compare. Every input
        /// after the first is compared against it.
        #[arg(num_args = 1.., required = true)]
        inputs: Vec<PathBuf>,

        /// Where to write the output. Defaults to standard output.
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Emit JSON instead of text.
        #[arg(long)]
        json: bool,

        /// Emit compact JSON instead of pretty-printed JSON.
        #[arg(long, requires = "json")]
        compact: bool,
    },

    /// Read a model's product structure: its parts and its assembly tree.
    ///
    /// Says what the file *contains* rather than what it states or what
    /// its shapes are: which parts it names, at what revision, how many
    /// times each is used, and where each occurrence sits. Placements
    /// are in millimetres whatever the file declared.
    ///
    /// This is what makes a component addressable — a body with a part
    /// number and a revision beside it is something a person or a model
    /// can look up. See ADR 0014.
    Product {
        /// Path to the model (.stp, .step, or .jt).
        input: PathBuf,

        /// Where to write the output. Defaults to standard output.
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Emit JSON instead of text.
        #[arg(long)]
        json: bool,

        /// Emit compact JSON instead of pretty-printed JSON.
        #[arg(long, requires = "json")]
        compact: bool,
    },

    /// Describe a part: what it is, how big it is, and what it is made of.
    ///
    /// Gathers all three documents into one answer — identity and
    /// revision, each body's size and the features and patterns
    /// recognised in it, and the material, mass and finish the file
    /// states, lifted out of its own properties into named fields.
    ///
    /// This is the view to reach for when the question is what a part
    /// *is*, rather than what its shape is or what its file says. Use
    /// `pmix product` first to see what parts a file holds.
    Describe {
        /// Path to the model (.stp, .step, or .jt).
        input: PathBuf,

        /// Only this part, by its id or its part number.
        #[arg(short, long)]
        part: Option<String>,

        /// Where to write the output. Defaults to standard output.
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Emit JSON instead of text.
        #[arg(long)]
        json: bool,

        /// Emit compact JSON instead of pretty-printed JSON.
        #[arg(long, requires = "json")]
        compact: bool,
    },

    /// Compare the PMI of two or more models.
    ///
    /// Inputs are model files (extracted on the fly) or JSON documents
    /// written by `pmix extract`. Every input after the first is compared
    /// against the first. Exit status is 0 when nothing differs, 1 when
    /// something does, 2 on error.
    Diff {
        /// Two or more inputs; the first is the baseline.
        #[arg(num_args = 2.., required = true)]
        inputs: Vec<PathBuf>,

        /// Emit JSON instead of text.
        #[arg(long)]
        json: bool,
    },

    /// Serve the documents to a language model over standard input and
    /// output, as a Model Context Protocol server.
    ///
    /// One JSON-RPC message per line in, one per line out; logs go to
    /// standard error. The tools read files and change nothing. Point a
    /// client at `pmix mcp` and it will list them. See ADR 0013.
    Mcp,

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
        Ok(code) => code,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::from(2)
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

fn run(cli: Cli) -> Result<ExitCode> {
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
        )
        .map(|()| ExitCode::SUCCESS),
        Command::Features {
            inputs,
            output,
            json,
            compact,
        } => features(inputs, output, json, compact),
        Command::Product {
            input,
            output,
            json,
            compact,
        } => product(input, output, json, compact),
        Command::Describe {
            input,
            part,
            output,
            json,
            compact,
        } => describe(input, part, output, json, compact),
        Command::Diff { inputs, json } => diff(inputs, json),
        Command::Mcp => {
            tracing::info!("serving over standard input and output");
            let stdin = std::io::stdin();
            let stdout = std::io::stdout();
            pmix::mcp::serve(stdin.lock(), stdout.lock()).context("the server stopped")?;
            Ok(ExitCode::SUCCESS)
        }
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
        )
        .map(|()| ExitCode::SUCCESS),
    }
}

fn product(input: PathBuf, output: Option<PathBuf>, json: bool, compact: bool) -> Result<ExitCode> {
    tracing::info!(input = %input.display(), "reading product structure");
    let document = pmix::product::read_path(&input)
        .with_context(|| format!("failed to read `{}`", input.display()))?;
    let text = match (json, compact) {
        (true, true) => serde_json::to_string(&document)?,
        (true, false) => serde_json::to_string_pretty(&document)?,
        (false, _) => render_product(&document),
    };
    match output {
        Some(path) => std::fs::write(&path, text)
            .with_context(|| format!("failed to write `{}`", path.display()))?,
        None => print!("{text}"),
    }
    Ok(ExitCode::SUCCESS)
}

/// The assembly as an indented tree, each part named as a person would
/// name it, with the occurrences beneath the part that contains them.
fn render_product(document: &pmix::product::ProductDocument) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{} ({}), {} part{}, {} occurrence{}",
        document.source.file_name,
        document.source.format,
        document.parts.len(),
        if document.parts.len() == 1 { "" } else { "s" },
        document.relations.len(),
        if document.relations.len() == 1 {
            ""
        } else {
            "s"
        }
    );

    let named = |id: &str| -> String {
        let Some(part) = document.parts.iter().find(|p| p.id == id) else {
            return id.to_owned();
        };
        let mut label = part
            .number
            .clone()
            .or_else(|| part.name.clone())
            .unwrap_or_else(|| part.id.clone());
        if let (Some(number), Some(name)) = (&part.number, &part.name) {
            label = format!("{number} ({name})");
        }
        let label = match &part.revision {
            Some(rev) => format!("{label} rev {rev}"),
            None => label,
        };
        match part.bodies.len() {
            0 => label,
            1 => format!("{label} — {}", part.bodies[0]),
            n => format!("{label} — {n} bodies"),
        }
    };

    // A subassembly used twice is printed twice, because that is what
    // the assembly holds; the part list above it says it once.
    fn walk(
        out: &mut String,
        document: &pmix::product::ProductDocument,
        named: &impl Fn(&str) -> String,
        part: &str,
        depth: usize,
        seen: &mut Vec<String>,
    ) {
        use std::fmt::Write as _;
        if seen.contains(&part.to_owned()) {
            let _ = writeln!(out, "{:indent$}... cycle", "", indent = depth * 2 + 2);
            return;
        }
        seen.push(part.to_owned());
        for r in document.relations.iter().filter(|r| r.parent == part) {
            let where_at = match &r.placement {
                Some(p) => format!(" at {},{},{}", p.origin[0], p.origin[1], p.origin[2]),
                None => String::new(),
            };
            let _ = writeln!(
                out,
                "{:indent$}{}{}{}",
                "",
                named(&r.child),
                r.name
                    .as_deref()
                    .map(|n| format!(" [{n}]"))
                    .unwrap_or_default(),
                where_at,
                indent = depth * 2 + 2
            );
            walk(out, document, named, &r.child, depth + 1, seen);
        }
        seen.pop();
    }

    for root in &document.roots {
        let _ = writeln!(out, "\n{}", named(root));
        walk(&mut out, document, &named, root, 0, &mut Vec::new());
    }

    if !document.unattached.is_empty() {
        let _ = writeln!(
            out,
            "\n{} bod{} under no part:",
            document.unattached.len(),
            if document.unattached.len() == 1 {
                "y"
            } else {
                "ies"
            }
        );
        for u in &document.unattached {
            let _ = writeln!(out, "  {}: {}", u.body, u.reason);
        }
    }

    let unused: Vec<&pmix::product::Part> = document
        .parts
        .iter()
        .filter(|p| p.occurrences == 0 && !document.roots.contains(&p.id))
        .collect();
    if !unused.is_empty() {
        let _ = writeln!(out, "\nnamed but not placed:");
        for p in unused {
            let _ = writeln!(out, "  {}", named(&p.id));
        }
    }

    for d in &document.diagnostics {
        let _ = writeln!(out, "\nnote: {}", d.message);
    }
    out
}

fn describe(
    input: PathBuf,
    part: Option<String>,
    output: Option<PathBuf>,
    json: bool,
    compact: bool,
) -> Result<ExitCode> {
    tracing::info!(input = %input.display(), "describing");
    let product = pmix::product::read_path(&input)
        .with_context(|| format!("failed to read `{}`", input.display()))?;
    // A part's material is in the PMI document and its size is in the
    // features document; what a part *is* needs all three (ADR 0014).
    let pmi = pmix::load(&input).ok();
    let shapes = pmix::features::read_path(&input).ok();
    let mut parts = pmix::product::summarise(&product, pmi.as_ref(), shapes.as_ref());
    // The tolerances a substitute has to hold, tightest first. Stated
    // for the file rather than per part: a dimension names the features
    // it controls, and those are not attributed to a part (ADR 0007).
    let critical = pmi
        .as_ref()
        .map(|d| pmix::critical::tightest(d, 8))
        .unwrap_or_default();
    // Thread designations, read out of whatever text carried them: a
    // note, a dimension, a validation property (ADR 0014).
    let threads = pmi
        .as_ref()
        .map(pmix::threads::in_document)
        .unwrap_or_default();

    if let Some(wanted) = &part {
        parts.retain(|p| &p.id == wanted || p.number.as_deref() == Some(wanted.as_str()));
        if parts.is_empty() {
            anyhow::bail!(
                "`{}` names no part `{wanted}`; run `pmix product {}` to see what it holds",
                input.display(),
                input.display()
            );
        }
    }

    let text = match (json, compact) {
        (true, true) => serde_json::to_string(&parts)?,
        (true, false) => serde_json::to_string_pretty(&parts)?,
        (false, _) => render_parts(&product, &parts, shapes.as_ref(), &critical, &threads),
    };
    match output {
        Some(path) => std::fs::write(&path, text)
            .with_context(|| format!("failed to write `{}`", path.display()))?,
        None => print!("{text}"),
    }
    Ok(ExitCode::SUCCESS)
}

/// What each part is, as a person would want to read it.
fn render_parts(
    document: &pmix::product::ProductDocument,
    parts: &[pmix::product::PartSummary],
    shapes: Option<&pmix::features::FeatureDocument>,
    critical: &[pmix::critical::Critical],
    threads: &[pmix::threads::Thread],
) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{} ({}), {} part{}",
        document.source.file_name,
        document.source.format,
        parts.len(),
        if parts.len() == 1 { "" } else { "s" }
    );
    if let Some(e) = &document.envelope {
        let _ = writeln!(
            out,
            "overall {} \u{d7} {} \u{d7} {} mm{}",
            e.size[0],
            e.size[1],
            e.size[2],
            if e.approximate { " (at least)" } else { "" }
        );
    }

    for part in parts {
        let name = part
            .number
            .clone()
            .or_else(|| part.name.clone())
            .unwrap_or_else(|| part.id.clone());
        let _ = write!(out, "\n{name}");
        if let Some(rev) = &part.revision {
            let _ = write!(out, " rev {rev}");
        }
        let _ = writeln!(out, "  \u{d7}{}  {}", part.occurrences.max(1), part.id);
        if let Some(d) = &part.description {
            let _ = writeln!(out, "  {d}");
        }
        // The marking sits directly under the number, always: a reader
        // must not see one without the other (ADR 0014).
        match &part.classification {
            Some(c) => {
                let _ = writeln!(
                    out,
                    "  {:<14} {}{}",
                    "classified",
                    c.level.as_deref().unwrap_or("(level not stated)"),
                    c.purpose
                        .as_deref()
                        .map(|p| format!(" — {p}"))
                        .unwrap_or_default()
                );
            }
            None => {
                let _ = writeln!(out, "  {:<14} not stated", "classification");
            }
        }
        if !part.categories.is_empty() {
            let _ = writeln!(out, "  {:<14} {}", "category", part.categories.join(", "));
        }
        for who in &part.people {
            let named = match (&who.organisation, &who.person) {
                (Some(o), Some(p)) => format!("{p} ({o})"),
                (Some(o), None) => o.clone(),
                (None, Some(p)) => p.clone(),
                (None, None) => "(unnamed)".into(),
            };
            let _ = writeln!(out, "  {:<14} {named}", who.role);
        }
        for a in &part.approvals {
            let mut line = a.status.clone();
            if let Some(d) = &a.date {
                line.push_str(&format!(" on {d}"));
            }
            if let Some(l) = &a.level {
                line.push_str(&format!(": {l}"));
            }
            for b in &a.by {
                let named = b.person.clone().or_else(|| b.organisation.clone());
                if let Some(n) = named {
                    line.push_str(&format!(" — {n}, {}", b.role));
                }
            }
            let _ = writeln!(out, "  {:<14} {line}", "approval");
        }
        for a in &part.attributes {
            let unit = a.unit.as_deref().unwrap_or("");
            let _ = writeln!(
                out,
                "  {:<14} {}{}   (from {})",
                a.field,
                a.value.canonical(),
                unit,
                a.from
            );
        }
        for a in &part.ambiguous {
            let _ = writeln!(
                out,
                "  {:<14} not stated: {} keys disagree ({})",
                a.field,
                a.candidates.len(),
                a.candidates
                    .iter()
                    .map(|c| c.key.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        if part.other_properties > 0 {
            let _ = writeln!(
                out,
                "  {:<14} {} more, see `pmix extract`",
                "properties", part.other_properties
            );
        }
        for body in &part.bodies {
            let _ = write!(out, "  body {}", body.id);
            if let Some(n) = &body.name {
                let _ = write!(out, " ({n})");
            }
            if let Some(e) = &body.envelope {
                let _ = write!(
                    out,
                    ": {} \u{d7} {} \u{d7} {} mm{}",
                    e.size[0],
                    e.size[1],
                    e.size[2],
                    if e.approximate { " (at least)" } else { "" }
                );
            }
            let _ = writeln!(out);
            if let Some(class) = body.shape_class {
                let kinds: Vec<String> = body
                    .surfaces
                    .counts
                    .iter()
                    .map(|(k, n)| format!("{n} {k}"))
                    .collect();
                let _ = writeln!(out, "    {} of {}", class.name(), kinds.join(", "));
            }
            for p in &body.patterns {
                let _ = writeln!(out, "    {}", pattern_line(p));
            }
            if !body.features.is_empty() {
                let kinds: Vec<String> = body
                    .features
                    .iter()
                    .map(|(k, n)| format!("{n} {k}"))
                    .collect();
                let _ = writeln!(out, "    features: {}", kinds.join(", "));
            }
            if body.unassigned_faces > 0 {
                let _ = writeln!(out, "    {} face(s) no rule claimed", body.unassigned_faces);
            }
        }
    }
    // A file can state geometry and never define a product for it — a
    // part exported on its own often does. The shapes are still worth
    // describing; what is missing is the part number beside them, and
    // saying that is better than an empty answer.
    if parts.is_empty() {
        if let Some(shapes) = shapes.filter(|s| !s.bodies.is_empty()) {
            let _ = writeln!(
                out,
                "\nThis file states no part, so these shapes have no part number:"
            );
            for body in &shapes.bodies {
                let _ = write!(out, "\n  body {}", body.id);
                if let Some(n) = &body.name {
                    let _ = write!(out, " ({n})");
                }
                if let Some(e) = &body.envelope {
                    let _ = write!(
                        out,
                        ": {} \u{d7} {} \u{d7} {} mm{}",
                        e.size[0],
                        e.size[1],
                        e.size[2],
                        if e.approximate { " (at least)" } else { "" }
                    );
                }
                let _ = writeln!(out);
                for p in &body.patterns {
                    let _ = writeln!(out, "    {}", pattern_line(p));
                }
                let mut kinds: std::collections::BTreeMap<&str, usize> = Default::default();
                for f in &body.features {
                    *kinds.entry(f.kind.name()).or_default() += 1;
                }
                if !kinds.is_empty() {
                    let listed: Vec<String> =
                        kinds.iter().map(|(k, n)| format!("{n} {k}")).collect();
                    let _ = writeln!(out, "    features: {}", listed.join(", "));
                }
            }
        }
    }

    if !threads.is_empty() {
        let _ = writeln!(out, "\nthreads:");
        for t in threads {
            let mut parts: Vec<String> = Vec::new();
            if let Some(n) = t.count {
                parts.push(format!("{n}\u{d7}"));
            }
            parts.push(t.designation.clone());
            if let Some(d) = t.major_diameter {
                parts.push(format!("\u{2300}{d}"));
            }
            if let Some(p) = t.pitch {
                parts.push(format!("pitch {p}"));
            }
            if let Some(tpi) = t.threads_per_inch {
                parts.push(format!("{tpi} tpi"));
            }
            let _ = writeln!(
                out,
                "  {:<28} {:<12} from {} {}",
                parts.join(" "),
                t.standard.name(),
                t.found_in.kind,
                t.found_in.id
            );
        }
    }

    if !critical.is_empty() {
        let _ = writeln!(
            out,
            "\ntightest tolerances{}:",
            if parts.len() > 1 {
                " (for the file; a dimension does not say which part it is on)"
            } else {
                ""
            }
        );
        for c in critical {
            let held = match (&c.fit, &c.width) {
                (Some(fit), _) => fit.clone(),
                (None, Some(w)) => format!("{}{}", w.value, w.unit),
                (None, None) => String::new(),
            };
            let _ = writeln!(
                out,
                "  {:<10} {:<24} {}",
                held,
                c.text.as_deref().unwrap_or(""),
                c.id
            );
        }
    }

    // When nothing was named, the line above already said why; listing
    // every body again as "unattached" would be the same fact twice.
    if !parts.is_empty() {
        for u in &document.unattached {
            let _ = writeln!(out, "\nunattached {}: {}", u.body, u.reason);
        }
    }
    for d in &document.diagnostics {
        let _ = writeln!(out, "\nnote: {}", d.message);
    }
    out
}

fn diff(inputs: Vec<PathBuf>, json: bool) -> Result<ExitCode> {
    let baseline = pmix::load(&inputs[0])
        .with_context(|| format!("failed to load `{}`", inputs[0].display()))?;
    let mut reports = Vec::new();
    for path in &inputs[1..] {
        let doc =
            pmix::load(path).with_context(|| format!("failed to load `{}`", path.display()))?;
        reports.push(pmix::diff::diff(&baseline, &doc));
    }
    let differs = reports.iter().any(|r| !r.is_empty());
    let stdout = std::io::stdout();
    let mut out = std::io::BufWriter::new(stdout.lock());
    if json {
        if reports.len() == 1 {
            serde_json::to_writer_pretty(&mut out, &reports[0])?;
        } else {
            serde_json::to_writer_pretty(&mut out, &reports)?;
        }
        writeln!(out)?;
    } else {
        for (i, r) in reports.iter().enumerate() {
            if i > 0 {
                writeln!(out)?;
                writeln!(out, "{}", "=".repeat(60))?;
                writeln!(out)?;
            }
            write!(out, "{r}")?;
        }
    }
    out.flush()?;
    Ok(if differs {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    })
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

fn features(
    inputs: Vec<PathBuf>,
    output: Option<PathBuf>,
    json: bool,
    compact: bool,
) -> Result<ExitCode> {
    let read = |path: &PathBuf| {
        tracing::info!(input = %path.display(), "recognising features");
        pmix::features::read_path(path)
            .with_context(|| format!("failed to read `{}`", path.display()))
    };
    let first = read(&inputs[0])?;

    let mut differs = false;
    let write = |text: String| -> Result<()> {
        match &output {
            Some(path) => std::fs::write(path, text)
                .with_context(|| format!("failed to write `{}`", path.display())),
            None => {
                print!("{text}");
                Ok(())
            }
        }
    };

    if inputs.len() == 1 {
        let text = match (json, compact) {
            (true, true) => serde_json::to_string(&first)?,
            (true, false) => serde_json::to_string_pretty(&first)?,
            (false, _) => render_features(&first),
        };
        write(text)?;
        return Ok(ExitCode::SUCCESS);
    }

    let mut rendered = String::new();
    let mut documents = Vec::new();
    for path in &inputs[1..] {
        let other = read(path)?;
        let comparison = pmix::features::compare::compare(&first, &other);
        if !comparison.summary.identical() {
            differs = true;
        }
        if json {
            documents.push(comparison);
        } else {
            rendered.push_str(&render_comparison(&comparison));
        }
    }
    let text = match (json, compact) {
        (true, true) => serde_json::to_string(&documents)?,
        (true, false) => serde_json::to_string_pretty(&documents)?,
        (false, _) => rendered,
    };
    write(text)?;
    Ok(if differs {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    })
}

/// One feature on a line, as a comparison shows it.
fn feature_line(f: &pmix::features::Feature) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(d) = f.shape.diameter {
        parts.push(format!("\u{2300}{d}"));
    }
    if let Some(r) = f.shape.radius {
        parts.push(format!("R{r}"));
    }
    if let Some(v) = f.shape.depth {
        parts.push(format!("{v} deep"));
    }
    if let Some(v) = f.shape.length {
        parts.push(format!("{v} long"));
    }
    if let Some(a) = f.shape.angle {
        parts.push(format!("{a}\u{b0}"));
    }
    if let Some(p) = f.shape.position {
        parts.push(format!("at {},{},{}", p[0], p[1], p[2]));
    }
    format!("{:<12} {}", f.kind.name(), parts.join(", "))
}

/// A comparison as lines meant to be read.
///
/// Ordered by how much weight each part carries: what is certain first,
/// what is measured next, what is merely observed after that, and the
/// leftovers last.
/// One pattern on a line: what it is, and the numbers a catalogue would
/// state it by.
fn pattern_line(p: &pmix::features::Pattern) -> String {
    use pmix::features::PatternKind;
    let mut parts: Vec<String> = vec![format!("{}\u{d7}", p.count)];
    if let Some(d) = p.diameter {
        parts.push(format!("\u{2300}{d}"));
    }
    match p.kind {
        PatternKind::BoltCircle => {
            if let Some(pcd) = p.pitch_circle_diameter {
                parts.push(format!("on \u{2300}{pcd} PCD"));
            }
            if let Some(c) = p.clocking {
                parts.push(format!("from {c}\u{b0}"));
            }
        }
        PatternKind::Row => {
            if let Some(pitch) = p.pitch.first() {
                parts.push(format!("at {pitch} pitch"));
            }
        }
        PatternKind::Grid => {
            if let [across, down] = p.counts[..] {
                parts.push(format!("as {across}\u{d7}{down}"));
            }
            if let [a, b] = p.pitch[..] {
                parts.push(format!("at {a}\u{d7}{b}"));
            }
        }
    }
    if !p.overlaps.is_empty() {
        parts.push(format!("(also read {} other way(s))", p.overlaps.len()));
    }
    format!("{:<12} {}", p.kind.name(), parts.join(" "))
}

fn render_comparison(c: &pmix::features::compare::Comparison) -> String {
    use pmix::features::compare::Paired;
    use std::fmt::Write as _;
    let mut out = String::new();
    let mut showed_candidate = false;
    let _ = writeln!(
        out,
        "{} \u{2192} {}",
        c.baseline.file_name, c.compared.file_name
    );
    for b in &c.bodies {
        let how = match b.paired {
            Some(Paired::Id) => " (identical)".to_owned(),
            Some(Paired::Faces) => {
                format!(" (paired on {} shared faces)", b.paired_on.unwrap_or(0))
            }
            Some(Paired::Shapes) => " (paired on being the same shapes)".to_owned(),
            Some(Paired::Name) => " (paired on name alone)".to_owned(),
            None => " (no counterpart)".to_owned(),
        };
        let _ = writeln!(
            out,
            "\n{}{}{}",
            b.baseline.as_deref().unwrap_or("-"),
            b.compared
                .as_deref()
                .filter(|id| Some(*id) != b.baseline.as_deref())
                .map(|id| format!(" \u{2192} {id}"))
                .unwrap_or_default(),
            how
        );
        let _ = writeln!(
            out,
            "  {} matched, {} only in baseline, {} only in compared",
            b.matched.len(),
            b.only_baseline.len(),
            b.only_compared.len()
        );
        if let Some(d) = b.placement {
            let _ = writeln!(
                out,
                "  placement: nothing kept its place and all {} sit {},{},{} from their \
                 counterparts, so one move explains the body",
                b.only_baseline.len(),
                d[0],
                d[1],
                d[2]
            );
            // The displacement is the whole of the difference, so listing
            // every feature again would be stating it once per feature,
            // which is what the placement line exists to replace.
            continue;
        } else if b.same_shapes_moved {
            let _ = writeln!(
                out,
                "  the same {} shapes are present but in different places, and no single move \
                 puts them there",
                b.only_baseline.len()
            );
        }
        let paired: BTreeMap<&str, &pmix::features::compare::PossiblePairing> = b
            .possible_pairings
            .iter()
            .map(|c| (c.baseline.as_str(), c))
            .collect();
        for f in &b.only_baseline {
            let _ = writeln!(out, "  - {}", feature_line(f));
            if let Some(cand) = paired.get(f.id.as_str()) {
                if let Some(other) = b.only_compared.iter().find(|x| x.id == cand.compared) {
                    let _ = writeln!(out, "  + {}", feature_line(other));
                }
                let how: Vec<String> = cand
                    .differs
                    .iter()
                    .map(|ch| format!("{} {} \u{2192} {}", ch.field, ch.from, ch.to))
                    .collect();
                // What the pairing rests on comes from the data now, not
                // from this renderer deciding what to call it.
                let mut place = cand.paired_on.describe().to_owned();
                if cand.distance > 0.0 {
                    place.push_str(&format!(", {} apart", cand.distance));
                }
                // An empty list would read as a difference with nothing
                // different about it, so say which way it fell instead.
                let how = if how.is_empty() {
                    "differs in nothing reported here".to_owned()
                } else {
                    how.join(", ")
                };
                let _ = writeln!(out, "      possible pairing: {place}; {how}");
                showed_candidate = true;
            }
        }
        for f in &b.only_compared {
            if b.possible_pairings.iter().any(|c| c.compared == f.id) {
                continue;
            }
            let _ = writeln!(out, "  + {}", feature_line(f));
        }
    }
    let s = &c.summary;
    let _ = writeln!(
        out,
        "\nsummary: {} matched, {} only in baseline, {} only in compared; \
         bodies {} paired, {} only in baseline, {} only in compared",
        s.matched,
        s.only_baseline,
        s.only_compared,
        s.bodies_paired,
        s.bodies_only_baseline,
        s.bodies_only_compared
    );
    // The caution travels in the document, so print what it says rather
    // than a second copy of it that could drift (ADR 0013).
    if showed_candidate {
        for note in &c.notes {
            let _ = writeln!(out, "\n{}", note.note);
        }
    }
    out
}

/// The features of a document as lines meant to be read.
///
/// Every body reports what was recognised and how much of it was not, in
/// that order, because a count of unrecognised faces is what tells the
/// reader how much weight the rest can carry.
fn render_features(document: &pmix::features::FeatureDocument) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{} ({}), {} bod{}",
        document.source.file_name,
        document.source.format,
        document.bodies.len(),
        if document.bodies.len() == 1 {
            "y"
        } else {
            "ies"
        }
    );
    for body in &document.bodies {
        let _ = writeln!(
            out,
            "\n{}{}: {} faces, {} in features, {} unassigned",
            body.id,
            body.name
                .as_deref()
                .map(|n| format!(" ({n})"))
                .unwrap_or_default(),
            body.faces.total,
            body.faces.in_features,
            body.faces.unassigned
        );
        if let Some(class) = body.shape_class {
            let kinds: Vec<String> = body
                .surfaces
                .counts
                .iter()
                .map(|(k, n)| format!("{n} {k}"))
                .collect();
            let _ = writeln!(out, "  {} of {}", class.name(), kinds.join(", "));
        }
        for p in &body.patterns {
            let _ = writeln!(out, "  {}  {}", pattern_line(p), p.id);
        }
        for f in &body.features {
            let mut parts: Vec<String> = Vec::new();
            if let Some(d) = f.shape.diameter {
                parts.push(format!("\u{2300}{d}"));
            }
            // A blend is named by its radius, never by a diameter.
            if let Some(r) = f.shape.radius {
                parts.push(format!("R{r}"));
            }
            if let Some(depth) = f.shape.depth {
                parts.push(format!("{depth} deep"));
            }
            if let Some(length) = f.shape.length {
                parts.push(format!("{length} long"));
            }
            match f.shape.through {
                Some(true) => parts.push("through".to_owned()),
                Some(false) => parts.push("blind".to_owned()),
                None => {}
            }
            if let Some(a) = f.shape.angle {
                parts.push(format!("{a}\u{b0}"));
            }
            if let Some(p) = f.shape.position {
                parts.push(format!("at {},{},{}", p[0], p[1], p[2]));
            }
            // Two chamfers on one hole share a position, because a line
            // has one closest point to the origin however much of it a
            // face covers. The span along the axis is what tells them
            // apart, so it is shown rather than left to the JSON.
            if let Some(e) = f.shape.extent {
                parts.push(format!("span {}..{}", e[0], e[1]));
            }
            let _ = writeln!(
                out,
                "  {:<12} {:<62} {}",
                f.kind.name(),
                parts.join(", "),
                f.id
            );
        }
        if !body.unassigned.is_empty() {
            let mut by_kind: BTreeMap<&str, usize> = BTreeMap::new();
            for u in &body.unassigned {
                *by_kind.entry(u.surface.as_str()).or_default() += 1;
            }
            let listed: Vec<String> = by_kind.iter().map(|(k, n)| format!("{n} {k}")).collect();
            let _ = writeln!(out, "  unassigned:  {}", listed.join(", "));
        }
    }
    for d in &document.diagnostics {
        let _ = writeln!(out, "\n! {}", d.message);
    }
    out
}

struct InspectOptions {
    entities: Vec<Id>,
    depth: usize,
    types: Vec<String>,
    complex: bool,
    diagnostics: bool,
    json: bool,
}

/// Dispatch to the reader for the file's format.
fn inspect(input: PathBuf, opts: InspectOptions) -> Result<()> {
    match pmix::Format::from_path(&input) {
        Some(pmix::Format::Jt) => inspect_jt(input, opts),
        _ => inspect_step(input, opts),
    }
}

/// Explore a JT file's structure: header, segments, and the elements of
/// the segments that carry PMI (ADR 0009).
fn inspect_jt(input: PathBuf, opts: InspectOptions) -> Result<()> {
    use pmix::jt::{Elements, Jt};

    if !opts.entities.is_empty() || opts.depth > 0 || opts.complex {
        anyhow::bail!("--entity, --depth and --complex apply to STEP files only");
    }
    let bytes =
        std::fs::read(&input).with_context(|| format!("failed to read `{}`", input.display()))?;
    tracing::info!(input = %input.display(), bytes = bytes.len(), "reading JT");
    let jt = Jt::parse(&bytes).with_context(|| format!("failed to read `{}`", input.display()))?;

    let wanted = |kind: &pmix::jt::SegmentKind| {
        opts.types.is_empty()
            || opts
                .types
                .iter()
                .any(|t| kind.as_str().eq_ignore_ascii_case(t))
    };

    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for segment in jt.segments() {
        *counts.entry(segment.kind.as_str()).or_default() += 1;
    }

    #[derive(serde::Serialize)]
    struct SegmentReport {
        kind: String,
        id: String,
        offset: u64,
        length: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        decoded_length: Option<usize>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        elements: Vec<ElementReport>,
    }
    #[derive(serde::Serialize)]
    struct ElementReport {
        object_type: String,
        base_type: u8,
        object_id: i32,
        length: usize,
    }

    let mut reports = Vec::new();
    for segment in jt.segments().iter().filter(|s| wanted(&s.kind)) {
        let mut report = SegmentReport {
            kind: segment.kind.as_str(),
            id: segment.id.to_string(),
            offset: segment.offset,
            length: segment.length,
            decoded_length: None,
            error: None,
            elements: Vec::new(),
        };
        // Segments whose elements are worth listing are decoded; geometry
        // is listed only.
        if segment.kind.carries_elements() {
            match jt.segment_data(segment) {
                Ok(data) => {
                    report.decoded_length = Some(data.len());
                    report.elements = Elements::new(&data)
                        .map(|e| ElementReport {
                            object_type: e.object_type.to_string(),
                            base_type: e.base_type,
                            object_id: e.object_id,
                            length: e.data.len(),
                        })
                        .collect();
                }
                Err(e) => report.error = Some(e.to_string()),
            }
        }
        reports.push(report);
    }

    let stdout = std::io::stdout();
    let mut out = std::io::BufWriter::new(stdout.lock());
    if opts.json {
        #[derive(serde::Serialize)]
        struct Report<'a> {
            header: &'a pmix::jt::Header,
            segments: Vec<SegmentReport>,
        }
        serde_json::to_writer_pretty(
            &mut out,
            &Report {
                header: &jt.header,
                segments: reports,
            },
        )?;
        writeln!(out)?;
    } else {
        let h = &jt.header;
        writeln!(out, "version:     {}", h.version)?;
        writeln!(out, "format:      JT {}.{}", h.major, h.minor)?;
        writeln!(out, "byte order:  {:?}", h.byte_order)?;
        writeln!(out, "toc offset:  {}", h.toc_offset)?;
        writeln!(out, "lsg segment: {}", h.lsg_segment)?;
        writeln!(out)?;
        writeln!(out, "{} segments", jt.segments().len())?;
        writeln!(out)?;
        writeln!(out, "segment types:")?;
        for (kind, count) in &counts {
            writeln!(out, "{count:>8}  {kind}")?;
        }
        let decoded: Vec<_> = reports
            .iter()
            .filter(|r| r.decoded_length.is_some())
            .collect();
        if !decoded.is_empty() {
            writeln!(out)?;
            writeln!(out, "PMI and metadata segments:")?;
            for r in decoded {
                writeln!(
                    out,
                    "  {} at {}: {} bytes, {} decoded, {} element(s)",
                    r.kind,
                    r.offset,
                    r.length,
                    r.decoded_length.unwrap_or(0),
                    r.elements.len()
                )?;
                for e in &r.elements {
                    writeln!(
                        out,
                        "      {}  base {}  id {}  {} bytes",
                        e.object_type, e.base_type, e.object_id, e.length
                    )?;
                }
            }
        }
        // The Smart Topology Table abstracts a part's precise B-rep, so
        // it says how much geometry the PMI can be attached to.
        let mut topology = Vec::new();
        for segment in jt.segments() {
            if segment.kind != pmix::jt::SegmentKind::Stt {
                continue;
            }
            let Ok(data) = jt.segment_data(segment) else {
                continue;
            };
            for element in pmix::jt::Elements::new(&data) {
                if element.object_type != pmix::jt::stt::STT_ELEMENT {
                    continue;
                }
                if let Ok(t) = pmix::jt::stt::parse(element.data) {
                    topology.push((segment.offset, t));
                }
            }
        }
        if !topology.is_empty() {
            writeln!(out)?;
            writeln!(out, "B-rep topology (from the smart topology table):")?;
            for (offset, t) in &topology {
                let body = if t.counts.bodies == 1 {
                    "body"
                } else {
                    "bodies"
                };
                writeln!(
                    out,
                    "  at {offset}: {} {body}, {} faces, {} edges",
                    t.counts.bodies, t.counts.faces, t.counts.edges,
                )?;
                match &t.geometry {
                    Some(g) => {
                        writeln!(
                            out,
                            "      {} surfaces ({} described), {} curves, {} points",
                            g.surfaces, g.represented_surfaces, g.curves, g.points
                        )?;
                        // Every face states what it lies on, so the
                        // ones the table has no closed form for are
                        // counted here too rather than going unseen.
                        let mut kinds: std::collections::BTreeMap<&str, usize> =
                            std::collections::BTreeMap::new();
                        for face in &t.faces {
                            *kinds.entry(face.surface_kind.name()).or_default() += 1;
                        }
                        if !kinds.is_empty() {
                            let listed: Vec<String> =
                                kinds.iter().map(|(k, n)| format!("{n} {k}")).collect();
                            writeln!(out, "      faces on {}", listed.join(", "))?;
                        }
                        let mut curves: std::collections::BTreeMap<&str, usize> =
                            std::collections::BTreeMap::new();
                        for edge in &t.edges {
                            *curves.entry(edge.curve_kind.name()).or_default() += 1;
                        }
                        if !curves.is_empty() {
                            let listed: Vec<String> =
                                curves.iter().map(|(k, n)| format!("{n} {k}")).collect();
                            writeln!(out, "      edges on {}", listed.join(", "))?;
                        }
                        // Without these a PMI callout cannot be tied to
                        // the geometry it applies to, so say when they
                        // are missing rather than only when they are not.
                        let tagged = t.faces.iter().filter(|f| f.tag.is_some()).count();
                        writeln!(
                            out,
                            "      {tagged} of {} faces carry a tag a callout can name",
                            t.counts.faces
                        )?;
                    }
                    None => writeln!(
                        out,
                        "      stopped after {} of its vectors, so the geometry was not reached",
                        t.vectors.len()
                    )?,
                }
                if let Some(why) = &t.stopped {
                    if opts.diagnostics {
                        writeln!(out, "      stopped: {why}")?;
                    }
                }
            }
        }

        let failed: Vec<_> = reports.iter().filter(|r| r.error.is_some()).collect();
        if !failed.is_empty() && opts.diagnostics {
            writeln!(out)?;
            for r in failed {
                writeln!(
                    out,
                    "{} at {}: {}",
                    r.kind,
                    r.offset,
                    r.error.as_deref().unwrap_or("")
                )?;
            }
        }
    }
    out.flush()?;
    Ok(())
}

fn inspect_step(input: PathBuf, opts: InspectOptions) -> Result<()> {
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
