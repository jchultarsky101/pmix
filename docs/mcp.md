# Using pmix from a language model

`pmix mcp` is a [Model Context Protocol](https://modelcontextprotocol.io)
server. It lets a model — Claude in Claude Code or Claude Desktop, or any
client that speaks the protocol — ask what a CAD file *says*, what a
shape *is*, and how two models differ, and then reason about the
answers in a way `pmix` deliberately does not.

The division of labour is the point. `pmix` extracts base data it stands
behind: measured, complete, deterministic. The model supplies the
judgement: that four holes moving 5mm outboard is a bolt circle growing,
that a hole which grew still carries the position tolerance it had, that
thirty-seven differences of 0.003mm are one re-export and not thirty-seven
edits. The decisions behind that split are in [ADR 0012](adr/0012-geometric-comparison.md)
and [ADR 0013](adr/0013-mcp-server.md).

## Setting it up

The server speaks over standard input and output, so a client runs it as
a subprocess. Install `pmix` first (see the [README](../README.md#installation))
and note where the binary landed — `which pmix` on macOS or Linux,
`where pmix` on Windows. You will want the **absolute path** below.

**Claude Code and Claude Desktop keep separate configurations.** Registering
the server in one does not make it appear in the other.

### Claude Code

```bash
claude mcp add -s user pmix -- /absolute/path/to/pmix mcp
```

`-s user` makes it available in every project; leave it off to register
for the current project only. `claude mcp get pmix` should then report
*Connected*.

### Claude Desktop

Claude Desktop reads one JSON file. Open it from the application —
**Settings → Developer → Edit Config** — which creates it if it does not
exist and shows where it is:

| | |
| --- | --- |
| macOS | `~/Library/Application Support/Claude/claude_desktop_config.json` |
| Windows | `%APPDATA%\Claude\claude_desktop_config.json` |

Add `pmix` under `mcpServers`, giving the **absolute path** to the binary:

```json
{
  "mcpServers": {
    "pmix": {
      "command": "/Users/you/.cargo/bin/pmix",
      "args": ["mcp"]
    }
  }
}
```

On Windows, escape the backslashes and name the executable:

```json
"command": "C:\\Users\\you\\.cargo\\bin\\pmix.exe"
```

The absolute path is not optional. A desktop application does not
inherit the `PATH` your shell has, so `"command": "pmix"` will start
nothing, and the only sign will be a line in a log file. Nothing else is
needed: no keys, no network, no environment variables.

If the file already has servers in it — it usually does — add `pmix` as
another key inside the existing `mcpServers` object rather than adding a
second `mcpServers`. The file must stay valid JSON, so mind the comma
between entries.

Then **quit Claude Desktop and start it again**. Closing the window is
not enough; the configuration is read at launch. After relaunch, the
tools appear under the tools control in the message box (the icon that
lists connected servers), with `pmix` and its six tools beneath it. Two
good first questions:

> What parts are in `/path/to/assembly.stp`?

> Describe the shape in `/path/to/part.stp` — just the summary.

**When it does not work**, the log says why. On macOS it is
`~/Library/Logs/Claude/mcp-server-pmix.log`, with the launcher's own
account in `mcp.log` beside it; on Windows, `%APPDATA%\Claude\logs\`.
Three causes cover nearly every case:

- *spawn pmix ENOENT* — the command was not found. Use the absolute path.
- *Unexpected token* or *JSON* in the launcher's log — the config file is
  no longer valid JSON. Check the comma after the previous server.
- The server starts and a tool returns `isError: true` with *No such
  file or directory* — the path given to the tool is relative. The server
  runs with the application's working directory, not your shell's, so
  give tool calls absolute paths to model files as well.

### Any other client

Any client that speaks the protocol over stdio needs the same two facts:
the command is the absolute path to `pmix`, and its one argument is
`mcp`. The tools **read files and change nothing**; paths are resolved
by the server process, so a model can only read what the user running
the client can read.

## How a session goes

A model with a context budget should ask narrow questions, and the tools
are shaped so that it can. Which pattern to use depends on the question.

### Identifying or sourcing a part

1. **`list_parts`** on the file. Every part it names, with its number,
   revision, how many times the assembly uses it, and the bodies it is
   made of. This is what turns a pile of anonymous bodies into
   components that can be looked up. Occurrences are counted but not
   listed; pass `tree: true` when the arrangement matters.
2. **`describe_part`**, with `part` to narrow to one. Everything known
   about it from all three documents: identity and revision, how big
   each body is, the features recognised in them, and the material,
   mass, volume and finish the file states, promoted out of its own
   properties into named fields.
3. **`extract_pmi`** when the promoted fields are not enough. Every
   property is still there; `other_properties` on each part says how
   many were not promoted.

Two fields decide how much weight the answer carries. Each promoted
attribute names the key it came from, so a promotion can be checked
rather than trusted. And `ambiguous` lists any field more than one key
claimed with values that disagree — a solid volume and a bounding-box
volume both read as "volume" — where nothing is promoted, because a
guess about a number is worse than no number.

### Comparing two revisions

1. **`describe_model` with `summary: true`** on each file. This returns
   every body's id, name, face counts, and a count of features by kind —
   and nothing else. It is small even for an assembly, and it tells the
   model which body ids to ask about.
2. **`compare_models`** on the pair, with **`only_changed: true`** on an
   assembly. What comes back is ordered by how much weight it carries:
   what matched exactly, then any single displacement explaining a whole
   body, then the leftovers and what they might pair with.
3. **Narrow** into whatever needs a closer look: `compare_models` with a
   `body` and a `kind`, or `describe_model` on one body.
4. **`diff_pmi`** for the annotations, when the question is about
   tolerances rather than shape — and **`extract_pmi`** to read the
   tolerances still attached to a feature that changed.

The server tells the model this itself. The protocol's handshake carries
an `instructions` paragraph, and clients show it to the model before it
chooses a tool; it says the above, and it says the one thing a reader of
a comparison has to know, covered [below](#reading-a-comparison).

## The tools

### `list_parts`

What a file contains: every part it names, and how the assembly uses
them.

| argument | | |
| --- | --- | --- |
| `path` | required | A `.stp`, `.step`, or `.jt` file |
| `tree` | optional | Include every occurrence with its placement, not just the parts |

Each part carries its number, name, description, revision, how many
times the file uses it (`occurrences`), and the ids of the bodies it is
made of. `roots` names the part nothing uses, which is the assembly
itself. `unattached` lists any body that reached no part, with the
reason.

**The occurrence count and the body count differ on purpose.** A part
used four times is one shape used four times: one body, four
occurrences. A reader that confuses them reports an assembly of four
pins as an assembly of one.

A JT file states its structure in the scene graph's node hierarchy — a
part node is a part, an instance node is an occurrence, a geometric
transform says where — and the reader walks it, so a JT gives the same
document a STEP does. Two things differ by format and are worth
knowing. A JT part node carries its material and surface area but not
its name; the name comes from the instance node above it, with the
occurrence suffix a writer appends stripped off. And the scene graph's
transforms are in the unit the file declares, not in JT's base unit of
metres, which the topology table uses.

### `describe_part`

Everything known about one part, gathered from all three documents.

| argument | | |
| --- | --- | --- |
| `path` | required | A `.stp`, `.step`, or `.jt` file |
| `part` | optional | Only this part, by its id or its number from `list_parts` |

Identity and revision from the product document — and, where the file
states them, who owns the part, who approved it and when, its product
category, and its security classification, which is placed beside the
number on purpose. Each body's envelope,
recognised features and the patterns they form — bolt circles, rows,
grids — from the features document; and `attributes`
promoted out of the properties in the PMI document — material, mass,
volume, density, finish, supplier, and the rest — each naming the key it
came from and the unit that key declared.

`threads` lists every thread designation read out of the file's text,
each with the whole text it came from and the record that carried it.
A fastener is ordered by its thread, and this is the one place a file
states it today: semantic AP242 threads arrive with Edition 5.

`fit_critical` ranks the file's tolerances by how narrow they are,
tightest first — the ones a replacement has to hold. It is ranked by
width, not by importance, and stated for the whole file rather than per
part, because a dimension names the features it controls and those are
not attributed to a part.

Read `ambiguous` alongside `attributes`: a field there had more than one
key claiming it with values that disagree, and none was promoted. Read
`approximate` on an envelope too — a box so marked is the smallest the
body can be, not the size it is.

**These files may describe confidential designs.** A part number,
drawing number, material specification or supplier name read from one is
the user's own data. The server's `instructions` say so, and a model
should not send such an identifier to a web search or any other service
unless the user has asked it to.

### `describe_model`

What a shape is: the features recognised in a file, with their sizes and
positions in millimetres, and every face no rule claimed.

| argument | | |
| --- | --- | --- |
| `path` | required | a `.stp`, `.step`, or `.jt` file |
| `summary` | optional | `true` for counts per body only, no feature lists |
| `body` | optional | only this body, by its id from a summary |
| `kind` | optional | only `hole`, `counterbore`, `countersink`, `boss`, `fillet`, `round`, or `chamfer` |

With `summary: true` the result is an overview:

```json
{
  "source": { "file_name": "nist_mtc_assembly.jt", "format": "JT", "schema": "10.5" },
  "units": { "length": "mm", "angle": "deg", "declared_length": "mm" },
  "bodies": [
    { "id": "body:005750006b307972",
      "faces": { "total": 26, "in_features": 14, "unassigned": 12 },
      "features": { "chamfer": 4 } },
    { "id": "body:091a72cc2b7663eb",
      "faces": { "total": 32, "in_features": 11, "unassigned": 21 },
      "features": { "boss": 1, "chamfer": 3, "fillet": 7 } }
  ]
}
```

Otherwise the result is a *view* — the document, and the filter that
produced it, so the model knows what is not there:

```json
{
  "filter": { "kind": "hole" },
  "document": {
    "schema_version": 1,
    "bodies": [
      { "id": "body:558dfa20046c9426",
        "faces": { "total": 7, "in_features": 1, "unassigned": 6 },
        "features": [
          { "id": "feat:3f940ff17527096f", "kind": "hole",
            "diameter": 8.0, "depth": 10.0, "through": true,
            "axis": [0.0, 0.0, 1.0], "position": [20.0, 15.0, 0.0], "extent": [0.0, 10.0],
            "faces": ["face:c095bbb7d07c3519"] }
        ],
        "unassigned": [] }
    ]
  }
}
```

Two things to notice. The **face counts describe the whole body** — seven
faces, six of them unassigned — whatever the view lists, and here it lists
no unassigned faces because a caller asking for holes did not ask for
planes. And each feature **states the measurement it is called by** and
omits the others: a bore has `diameter` and `depth`, a blend `radius` and
`length`, a cone `angle`. Everything is millimetres and degrees whatever
the file declared, with what it declared recorded in `units`.

A JT written without precise geometry has nothing to recognise. The
result then has no bodies and a `diagnostics` entry saying so; it is not
an error, because the file is a valid JT.

### `compare_models`

How two shapes differ, the first being the baseline.

| argument | | |
| --- | --- | --- |
| `baseline` | required | the model compared *from* |
| `compared` | required | the model compared *to* |
| `only_changed` | optional | leave out body pairs in which nothing differs |
| `body` | optional | only pairs involving this body id, on either side |
| `kind` | optional | only leftovers and pairings of this kind |

The result is a view of a comparison document. For a plate whose one
hole was bored from Ø8 to Ø10:

```json
{
  "filter": {},
  "comparison": {
    "schema_version": 2,
    "bodies": [
      { "baseline": "body:558dfa20046c9426", "compared": "body:293954a0dda6bb8a",
        "paired": "faces", "paired_on": 6,
        "matched": [],
        "only_baseline": [ { "id": "feat:3f940ff17527096f", "kind": "hole", "diameter": 8.0, "…": "…" } ],
        "only_compared": [ { "id": "feat:4766db2542e0009a", "kind": "hole", "diameter": 10.0, "…": "…" } ],
        "possible_pairings": [
          { "baseline": "feat:3f940ff17527096f", "compared": "feat:4766db2542e0009a",
            "paired_on": "place", "distance": 0.0,
            "differs": [ { "field": "diameter", "from": "8", "to": "10" } ] }
        ] }
    ],
    "summary": { "matched": 0, "only_baseline": 1, "only_compared": 1,
                 "bodies_paired": 1, "bodies_only_baseline": 0, "bodies_only_compared": 0 },
    "notes": [
      { "field": "possible_pairings",
        "note": "A possible pairing is an observation, not a conclusion. …" }
    ]
  }
}
```

How to read it is the next section.

### `extract_pmi`

What a file says: its Product Manufacturing Information — dimensions with
tolerances, geometric tolerances, datums and datum systems, annotations
and saved views — and the metadata it carries as named properties.

| argument | | |
| --- | --- | --- |
| `path` | required | a model file, or a JSON document written by `pmix extract` |

Measures are in the unit the file declares, stated beside each value.
Ids are stable across re-exports of one design, which is what lets
`diff_pmi` say a tolerance changed rather than that one vanished and
another appeared. The document is described in the
[README](../README.md#output-format).

### `diff_pmi`

How the PMI of two models differs, matched by identity.

| argument | | |
| --- | --- | --- |
| `baseline` | required | a model file or `pmix` JSON document |
| `compared` | required | the same, compared against it |

Fields that describe the extraction rather than the design are ignored.
A changed tolerance reads as one record with a changed field, not a
removal and an addition. What is compared and what is deliberately
ignored is recorded in [ADR 0005](adr/0005-diff.md).

## Reading a comparison

A comparison is evidence ordered by the weight it carries, and a model
should weigh it that way.

**`matched`** is proof. Two features with the same id are the same
feature — same kind, same surface, same place, within a thousandth of a
millimetre — however the two files happened to divide their faces or
which units they declared.

**`placement`** is measured. When nothing in a body kept its id, at least
two features are left over on each side, and one displacement carries
every one of them onto a counterpart of the same shape, the body moved as
a whole, and the comparison says so once instead of once per feature.
When the shapes all have counterparts but no single displacement puts
them there, `same_shapes_moved` is set: a rotation, a different origin,
and several separate edits all look like this, and the comparison does
not guess which.

**`possible_pairings`** are observations. Two leftovers of one kind that
agree on their size *or* their place are put beside each other, with
`paired_on` saying which, `distance` saying how far apart they sit, and
`differs` naming the fields that are not the same. That is all it is. One
hole moved and one hole removed with another added somewhere else are the
same geometry, and nothing in the geometry can tell them apart. The
document says this in `notes`, keyed to the field, and the server says it
in the handshake, so that the caution reaches the model as data rather
than as a sentence it never sees.

What the model can then do — and what `pmix` will not — is settle the
question on evidence that is not geometry: the part is named the same,
its properties are unchanged, the revision note says the hole was moved.
That is judgement, and it is the model's to make.

**`only_baseline`** and **`only_compared`** are stated in full, because
they are what nothing else accounts for.

## Errors

A tool that cannot do what was asked reports that as its **result**, with
`isError: true` and a plain sentence, so the model reads the reason and
tries something else:

```json
{ "content": [ { "type": "text", "text": "`/nowhere/at/all.stp`: No such file or directory (os error 2)" } ],
  "isError": true }
```

A missing argument, a file that is not STEP or JT, a `kind` that is not
one this recognises, and a path that does not exist all come back this
way, naming what was wrong. Only a tool name that is not in the list is a
protocol error, because the client was told what exists.

## What it will not do

Everything the command line will not, since it is the same library:

- **Say that a feature moved.** See above.
- **Recognise a chamfer on a straight edge**, which is a plane, not a
  cone, and which no local rule tells from a narrow face that was always
  meant to be there. It is listed under `unassigned`.
- **Recognise pockets and slots**, which need volume decomposition
  ([ADR 0011](adr/0011-feature-recognition.md)).
- **Choose between two readings of one pattern.** Four holes on a square
  are a grid and a bolt circle; it states both and says they overlap.
- **Recognise sheet metal**, whose constant-thickness test needs each
  face matched against an offset of another. The shape classes are
  `turned`, `prismatic` and `free_form`, and a body no rule settles is
  left unclassified.
- **Read a surface finish.** It is in no file this has been tested
  against ([ADR 0014](adr/0014-part-description-for-sourcing.md)).
- **Read a thread the way AP242 Edition 5 will state one.** It reads
  designations as text, which is what files carry today.
- **Detect a rotation** as a single fact; it reports the shapes as
  present and moved, and stops.
- **Read a JT that carries no precise geometry.** It says so.
- **Read a JT whose precise geometry is Parasolid XT with no topology
  table.** The table is an optional abstraction over the B-rep and the
  only form this reads; it says how many B-rep segments it found and
  that an exporter which writes the table makes the file readable.
- **Compute a volume or a surface area.** Both need the trimmed patch of
  each face, which this reader does not hold; both are read where a file
  states them and absent where it does not
  ([ADR 0014](adr/0014-part-description-for-sourcing.md)).
- **Decide between two keys that disagree** about a part's material or
  mass. It names both.
- **Search for a substitute part, or judge one.** It supplies what a
  part is; whether something else would do is engineering judgment, and
  it is the model's to make with the user, not this tool's.
- **Write anything.**

## Trying it by hand

The server is plain JSON-RPC, one message per line, so it can be driven
from a shell. This does the handshake and one call:

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"shell","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"describe_model","arguments":{"path":"part.stp","summary":true}}}' \
  | pmix mcp
```

Standard output carries only replies; logs go to standard error, and
`pmix -v mcp` says which messages arrived and which tools ran. Every
reply's `structuredContent` is the document, and its `content[0].text` is
the same document as text for a client that reads only text.

## Design

The server computes nothing. Each tool is a call into the `pmix` library
and a serialisation of what came back; the one piece that does compute —
the narrowing by body, kind, and `only_changed` — lives in the library
(`pmix::features::view`) where the test suite reaches it. That, and why
the server is in this repository rather than its own, is
[ADR 0013](adr/0013-mcp-server.md).
