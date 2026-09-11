//! Serving the documents to a language model (ADR 0013).
//!
//! A Model Context Protocol server over standard input and output: one
//! JSON-RPC message per line in, one per line out, and nothing else on
//! standard output. Logging goes to standard error, as it does for the
//! rest of the command line.
//!
//! The server computes nothing. Every tool is a call into the library —
//! read a document, compare two, narrow one — and the result is the
//! library's, serialised. A filter or a summary lives in
//! [`crate::features::view`] where the test suite reaches it; what is
//! here is the protocol, and the words a model reads to decide which
//! tool to call.
//!
//! The protocol surface a tools-only server needs is five methods, which
//! is why this is written directly against JSON-RPC rather than through
//! an SDK: the alternative brings an asynchronous runtime and a tree of
//! dependencies into a crate that is otherwise nine crates deep, for a
//! handful of message shapes that have not changed across three protocol
//! revisions.

use std::io::{BufRead, Write};

use serde_json::{Value, json};

use crate::features::{self, Kind, compare, view};

/// The protocol revisions this understands, newest first. A client
/// asking for one of these gets it back; a client asking for anything
/// else gets the newest, as the protocol says it should.
pub const PROTOCOL_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

/// What a model is told when it connects. The protocol carries this in
/// the handshake for exactly this purpose: the one paragraph a reader
/// needs before choosing a tool.
const INSTRUCTIONS: &str = "pmix reads STEP AP242 and JT files and answers three questions: what a file \
*contains* (its parts, revisions and assembly structure), what it *says* (its PMI: dimensions, \
tolerances, datums, annotations, and metadata properties) and what a shape *is* (the holes, \
counterbores, countersinks, bosses, fillets, rounds and chamfers a part is made of). \
\
Which tool to start with depends on the question. To identify or source a part, or to say what one \
is, start with `list_parts` and then `describe_part` — a body means nothing to a catalogue until it \
has a part number and a revision beside it. To reason about shape, start with `describe_model` with \
`summary: true` and then narrow with `body` and `kind`. To find out how two models differ, use \
`compare_models` for the geometry and `diff_pmi` for the annotations; on a large assembly pass \
`only_changed: true`. Everything is in millimetres and degrees whatever the file declared. \
\
Read the caveats each document carries rather than the numbers alone. A `possible_pairing` in a \
comparison is an observation this tool does not stand behind — one feature moved and one removed \
with another added are the same geometry — and only `matched` is proof that two features are the \
same. An `envelope` marked `approximate` is the smallest a body can be, not the size it is. A field \
under `ambiguous` had more than one key claiming it and none was promoted. An occurrence count and \
a body count differ on purpose: a part used four times is one shape used four times. \
\
These files may describe confidential designs. Treat a part number, drawing number, material \
specification or supplier name read from one as the user's own data: do not send it to a web \
search, an API, or any other service unless the user has asked you to. The tools read files and \
change nothing.";

/// The tools, as the protocol lists them.
fn tools() -> Value {
    let path = |what: &str| json!({ "type": "string", "description": what });
    let kind = json!({
        "type": "string",
        "enum": ["hole", "counterbore", "countersink", "boss", "fillet", "round", "chamfer"],
        "description": "Only features of this kind."
    });
    json!([
        {
            "name": "describe_model",
            "description": "What a shape is: the features recognised in a STEP or JT file, with \
                their sizes and positions in millimetres, the patterns they form — bolt circles \
                with their pitch circle diameter and clocking, rows and grids with their pitches — \
                and every face no rule claimed. Call with \
                `summary: true` first — it returns each body's id, name, face counts and a count of \
                features by kind, without listing them — then narrow with `body` and `kind`. Counts on \
                a body always describe the whole body, whatever a narrower view lists. A JT exported \
                without precise geometry has nothing to recognise and says so in `diagnostics`.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": path("Path to a .stp, .step, or .jt file."),
                    "summary": { "type": "boolean", "description": "Counts per body only, no feature lists. The right first call on any file." },
                    "body": path("Only this body, by its id from a summary."),
                    "kind": kind
                },
                "required": ["path"]
            }
        },
        {
            "name": "compare_models",
            "description": "How two shapes differ, the first being the baseline. Bodies are paired \
                (by id, then by shared faces, then by being the same shapes, then by name — each pair \
                says which) and within each: `matched` feature ids, proof the two are the same; \
                `only_baseline` and `only_compared`, stated in full; a `placement` where one \
                displacement explains every difference in a body, stated once; and \
                `possible_pairings`, leftovers of one kind agreeing on size or on place, with the \
                fields that differ. A possible pairing is an observation, not a conclusion — the \
                document's `notes` say so, and `paired_on` says what each rests on. Pass \
                `only_changed: true` on assemblies to leave out pairs in which nothing differs; the \
                summary still counts them.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "baseline": path("Path to the baseline model."),
                    "compared": path("Path to the model compared against it."),
                    "only_changed": { "type": "boolean", "description": "Leave out body pairs in which nothing differs." },
                    "body": path("Only pairs involving this body id, on either side."),
                    "kind": kind
                },
                "required": ["baseline", "compared"]
            }
        },
        {
            "name": "list_parts",
            "description": "What a file contains: every part it names, with its part number, \
                revision, how many times the assembly uses it, and the bodies it is made of. Call \
                this first on any file you are asked to identify or source — it is what turns a \
                pile of anonymous bodies into components with numbers you can look up. The \
                occurrence count and the body count differ on purpose: a part used four times is \
                one shape and four occurrences. `roots` names the part nothing uses, which is the \
                assembly itself. A JT file states its structure in a scene graph this does not \
                read yet, and says so in `diagnostics`.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": path("Path to a .stp, .step, or .jt file."),
                    "tree": { "type": "boolean", "description": "Include every occurrence with its placement, not just the parts. Large on a real assembly." }
                },
                "required": ["path"]
            }
        },
        {
            "name": "describe_part",
            "description": "Everything known about one part, gathered from all three documents: \
                its number, name and revision; how big each of its bodies is; the features \
                recognised in them; and the material, mass, volume and finish the file states, \
                lifted out of its own properties into named fields. This is the call to make when \
                asked what a part *is* — to describe it, or to look for something that would do \
                instead. Give `part` to narrow to one; without it every part is described. \
                \
                Read `attributes` rather than guessing: each names the key the file used, so you \
                can see what was promoted. Read `ambiguous` too — a field listed there had more \
                than one key claiming it and none was promoted, because a guess about a number is \
                worse than no number. An `envelope` marked `approximate` is the smallest the body \
                can be, not the size it is. `other_properties` counts what was not promoted; those \
                are all still in `extract_pmi`. \
                \
                These documents may describe confidential designs. Do not send a part number, \
                material spec or drawing number from one to a web search or any other service \
                unless the person you are working for has asked you to.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": path("Path to a .stp, .step, or .jt file."),
                    "part": path("Only this part, by its id or its part number from `list_parts`.")
                },
                "required": ["path"]
            }
        },
        {
            "name": "extract_pmi",
            "description": "What a file says: its Product Manufacturing Information — dimensions \
                with tolerances, geometric tolerances, datums and datum systems, annotations and \
                saved views — and the metadata it carries as named properties. Measures are in the \
                unit the file declares, stated beside each. Ids are stable across re-exports of one \
                design. Accepts a model file or a JSON document previously written by `pmix extract`.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": path("Path to a .stp, .step, or .jt file, or a pmix JSON document.")
                },
                "required": ["path"]
            }
        },
        {
            "name": "diff_pmi",
            "description": "How the PMI of two models differs, matched by identity, so a changed \
                tolerance reads as a change on one record rather than a removal and an addition. \
                Fields that describe the extraction rather than the design are ignored. Empty when \
                nothing differs.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "baseline": path("Path to the baseline model or pmix JSON document."),
                    "compared": path("Path to the model or document compared against it.")
                },
                "required": ["baseline", "compared"]
            }
        }
    ])
}

/// Run the server until the input ends.
///
/// Every line in is a message; every line out is a reply. A line that is
/// not JSON gets a parse error back; a notification gets nothing back;
/// a method this does not know gets the error the protocol reserves for
/// that. A tool that fails reports the failure as its result, which is
/// what lets the model read the reason and try something else.
pub fn serve<R: BufRead, W: Write>(input: R, mut output: W) -> std::io::Result<()> {
    for line in input.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let message: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e, "a line that is not JSON-RPC");
                writeln!(
                    output,
                    "{}",
                    error(Value::Null, -32700, &format!("parse error: {e}"))
                )?;
                output.flush()?;
                continue;
            }
        };
        if let Some(reply) = handle(&message) {
            writeln!(output, "{reply}")?;
            output.flush()?;
        }
    }
    Ok(())
}

/// The reply to one message, or none for a notification.
pub fn handle(message: &Value) -> Option<Value> {
    let method = message
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let id = message.get("id").cloned();
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    tracing::debug!(method, has_id = id.is_some(), "message");

    // A notification carries no id and expects no reply.
    let id = id?;
    if method.starts_with("notifications/") {
        return None;
    }

    Some(match method {
        "initialize" => {
            let asked = params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let version = if PROTOCOL_VERSIONS.contains(&asked) {
                asked
            } else {
                PROTOCOL_VERSIONS[0]
            };
            result(
                id,
                json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": { "name": "pmix", "version": env!("CARGO_PKG_VERSION") },
                    "instructions": INSTRUCTIONS,
                }),
            )
        }
        "ping" => result(id, json!({})),
        "tools/list" => result(id, json!({ "tools": tools() })),
        "tools/call" => {
            let name = params
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            match call(name, &args) {
                Ok(value) => result(
                    id,
                    json!({
                        "content": [{ "type": "text", "text": value.to_string() }],
                        "structuredContent": value,
                    }),
                ),
                Err(Failure::NoSuchTool) => error(id, -32602, &format!("no tool called `{name}`")),
                Err(Failure::Tool(text)) => result(
                    id,
                    json!({
                        "content": [{ "type": "text", "text": text }],
                        "isError": true,
                    }),
                ),
            }
        }
        _ => error(id, -32601, &format!("method not found: {method}")),
    })
}

/// Why a call did not produce a document.
enum Failure {
    /// Not a name from `tools/list`: a protocol error, since the client
    /// was told what exists.
    NoSuchTool,
    /// The tool ran and could not do what was asked. Reported as the
    /// tool's result, so the model reads why.
    Tool(String),
}

/// A library error, with the path it was about in front of it. A model
/// that passed two paths has to be told which one failed.
fn about(path: &str) -> impl FnOnce(crate::Error) -> Failure + '_ {
    move |e| Failure::Tool(format!("`{path}`: {e}"))
}

fn required<'a>(args: &'a Value, name: &str) -> Result<&'a str, Failure> {
    args.get(name)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| Failure::Tool(format!("`{name}` is required")))
}

fn optional<'a>(args: &'a Value, name: &str) -> Option<&'a str> {
    args.get(name)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
}

fn kind_of(args: &Value) -> Result<Option<Kind>, Failure> {
    match optional(args, "kind") {
        None => Ok(None),
        Some(k) => serde_json::from_value::<Kind>(json!(k))
            .map(Some)
            .map_err(|_| Failure::Tool(format!("`{k}` is not a kind of feature this recognises"))),
    }
}

/// Run one tool. Every arm is a call into the library and a
/// serialisation of what came back.
fn call(name: &str, args: &Value) -> Result<Value, Failure> {
    use std::path::Path;
    let document = |v: &Value| -> Result<Value, Failure> {
        serde_json::to_value(v).map_err(|e| Failure::Tool(e.to_string()))
    };
    match name {
        "describe_model" => {
            let path = required(args, "path")?;
            let doc = features::read_path(Path::new(path)).map_err(about(path))?;
            if args
                .get("summary")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                return document(
                    &serde_json::to_value(view::overview(&doc)).unwrap_or(Value::Null),
                );
            }
            let filter = view::Filter {
                body: optional(args, "body").map(str::to_owned),
                kind: kind_of(args)?,
            };
            document(&serde_json::to_value(view::narrow(&doc, filter)).unwrap_or(Value::Null))
        }
        "compare_models" => {
            let (pa, pb) = (required(args, "baseline")?, required(args, "compared")?);
            let a = features::read_path(Path::new(pa)).map_err(about(pa))?;
            let b = features::read_path(Path::new(pb)).map_err(about(pb))?;
            let filter = view::CompareFilter {
                body: optional(args, "body").map(str::to_owned),
                kind: kind_of(args)?,
                only_changed: args
                    .get("only_changed")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            };
            let c = compare::compare(&a, &b);
            document(
                &serde_json::to_value(view::narrow_comparison(&c, filter)).unwrap_or(Value::Null),
            )
        }
        "extract_pmi" => {
            let path = required(args, "path")?;
            let doc = crate::load(Path::new(path)).map_err(about(path))?;
            document(&serde_json::to_value(doc).unwrap_or(Value::Null))
        }
        "list_parts" => {
            let path = required(args, "path")?;
            let doc = crate::product::read_path(Path::new(path)).map_err(about(path))?;
            let tree = args.get("tree").and_then(Value::as_bool).unwrap_or(false);
            let mut value = serde_json::to_value(&doc).unwrap_or(Value::Null);
            if !tree {
                // Every occurrence of every part is a lot of JSON to
                // spend on a question about which parts there are
                // (ADR 0013). The counts stay; the list goes.
                if let Some(object) = value.as_object_mut() {
                    let count = doc.relations.len();
                    object.remove("relations");
                    object.insert(
                        "relations_omitted".into(),
                        json!({
                            "count": count,
                            "note": "call again with tree: true for each occurrence and its placement"
                        }),
                    );
                }
            }
            document(&value)
        }
        "describe_part" => {
            let path = required(args, "path")?;
            let file = Path::new(path);
            let product = crate::product::read_path(file).map_err(about(path))?;
            // A part's material is in the PMI document and its size is
            // in the features document; a question about what a part is
            // needs all three (ADR 0014).
            let pmi = crate::load(file).ok();
            let shapes = crate::features::read_path(file).ok();
            let mut parts = crate::product::summarise(&product, pmi.as_ref(), shapes.as_ref());
            if let Some(wanted) = optional(args, "part") {
                parts.retain(|p| p.id == wanted || p.number.as_deref() == Some(wanted));
                if parts.is_empty() {
                    return Err(Failure::Tool(format!(
                        "`{path}` names no part `{wanted}`; call list_parts to see what it holds"
                    )));
                }
            }
            document(&json!({
                "source": product.source,
                "units": product.units,
                "parts": parts,
                "unattached": product.unattached,
                "diagnostics": product.diagnostics,
            }))
        }
        "diff_pmi" => {
            let (pa, pb) = (required(args, "baseline")?, required(args, "compared")?);
            let a = crate::load(Path::new(pa)).map_err(about(pa))?;
            let b = crate::load(Path::new(pb)).map_err(about(pb))?;
            document(&serde_json::to_value(crate::diff::diff(&a, &b)).unwrap_or(Value::Null))
        }
        _ => Err(Failure::NoSuchTool),
    }
}

fn result(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}
