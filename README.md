# pmix

[![CI](https://github.com/jchultarsky101/pmix/actions/workflows/ci.yml/badge.svg)](https://github.com/jchultarsky101/pmix/actions/workflows/ci.yml)
<!-- Restore when the crate is published:
[![crates.io](https://img.shields.io/crates/v/pmix.svg)](https://crates.io/crates/pmix)
[![docs.rs](https://img.shields.io/docsrs/pmix)](https://docs.rs/pmix)
-->
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE.md)

**pmix** is a command-line tool that reads a 3D model file (STEP or JT),
extracts the Product Manufacturing Information (PMI) and the metadata
embedded in it, and writes both to a JSON file in a stable, comparable
form.

Run it on two or more models and `pmix diff` answers questions like "did
the tolerances change between revision B and revision C?" or "which
components changed material?" without opening a CAD package.

`pmix features` reads the geometry instead of the annotations, and
describes the holes, counterbores, countersinks, bosses, fillets, rounds
and chamfers a part is made of, with their sizes and positions. That is the base data for
answering the question people actually bring to two CAD files: not
whether they differ, but *how* — the same hole, bored wider; the
mounting holes, five millimetres off.

> **Status: 0.9.0.** `pmix extract` reads STEP AP242 and JT files and
> emits the semantic layer (units, features, dimensions with tolerances,
> geometric tolerances with zones, modifiers and composites, datums with
> targets, datum reference frames), the presentation layer (annotations
> with text, plane, leaders, style, geometry summary and links to the
> semantic records; saved views), and the **metadata** each file carries
> as named properties. It extracts every such entity in the NIST test
> corpus, and `pmix diff` compares two or more models by identity. A JT
> dimension is anchored on the B-rep faces and edges it applies to, by
> the same recipe the STEP reader uses, and in millimetres whatever unit
> the file states, so one design keys the same way however it was
> exported. `pmix features` recognises holes, counterbores, countersinks,
> bosses, fillets, rounds and chamfers from the B-rep of either format,
> states their sizes and positions in millimetres, and lists every face
> that went into none of them. See the [roadmap](#roadmap).

## What pmix extracts

**PMI** is the set of annotations that turn a bare geometric model into a
manufacturing specification: dimensions and their tolerances, geometric
dimensioning and tolerancing (GD&T) feature control frames, datums,
surface finish symbols, and notes.

**Metadata** is everything else a CAD file records about the design as
named values: part numbers and revisions, material, mass and volume, the
system that wrote the file, and whatever else the modeller chose to
attach. A file often carries far more of this than it carries PMI, and
`pmix` extracts it all. A property says which part states it, so an
assembly's components stay apart rather than collapsing into one record.

In a model-based definition (MBD) workflow both live inside the 3D file
rather than on a 2D drawing.

## Supported formats

| Format | Standard | Extensions | Status |
| ------ | -------- | ---------- | ------ |
| STEP | ISO 10303 (AP242) | `.stp`, `.step`, `.p21` | Extraction, comparison, feature recognition |
| JT | ISO 14306 (9 and 10) | `.jt` | Extraction, comparison, feature recognition |

## Installation

Prebuilt binaries and installers for macOS, Linux, and Windows are attached
to every [GitHub release](https://github.com/jchultarsky101/pmix/releases).

macOS (Apple Silicon and Intel) and Linux (x86-64 and ARM64):

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/jchultarsky101/pmix/releases/latest/download/pmix-installer.sh | sh
```

Windows (PowerShell):

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/jchultarsky101/pmix/releases/latest/download/pmix-installer.ps1 | iex"
```

The installer also puts `pmix-update` next to `pmix`; run it to upgrade to
the latest release in place. Archives for manual installation are on the
same release page.

`pmix` is distributed as prebuilt binaries from its GitHub releases rather
than through crates.io ([ADR 0006](docs/adr/0006-distribution.md)).
Building from source needs Rust 1.85 or newer:

```bash
git clone https://github.com/jchultarsky101/pmix.git
cd pmix
cargo install --path .
```

## Usage

Extract PMI from a model and print it as JSON:

```bash
pmix extract part.stp
```

Write the output to a file, compact instead of pretty-printed:

```bash
pmix extract part.stp --output part.pmi.json --compact
```

Annotation geometry is summarised by default (counts, bounding box, and a
content hash, see [ADR 0003](docs/adr/0003-presentation-pmi-model.md)).
Include the full coordinates and triangles when you need them:

```bash
pmix extract part.stp --presentation-geometry
```

### Comparing models

Diff the PMI of two models, or of JSON documents written by `pmix extract`:

```bash
pmix diff bracket_revB.stp bracket_revC.stp
pmix diff baseline.pmi.json bracket_revC.stp --json
```

Records match by identity, so a changed tolerance value reads as a change
on the same record rather than a removal and an addition:

```text
semantic
  ~ tol:3f9a1c0b7d2e6a48  ⌖ ⌀0.1 Ⓜ | A | B | C
      text: ⌖ ⌀0.1 Ⓜ | A | B | C → ⌖ ⌀0.2 Ⓜ | A | B | C
      value.value: 0.1 → 0.2

summary: 41 unchanged, 1 changed, 0 removed, 0 added
```

The exit status is 0 when nothing differs, 1 when something does, and 2
on error, so the command works as a check in scripts. What is compared
and what is deliberately ignored is recorded in
[ADR 0005](docs/adr/0005-diff.md).

### Recognising features

`pmix features` describes what a part *is*, rather than what its file
*says*. It reads the B-rep and reports the features local rules can
prove, with the numbers a difference can be stated in:

```bash
pmix features bracket.stp
pmix features bracket.jt --json --output bracket.features.json
```

```text
bracket.stp (STEP), 1 body

body:376fae1082b3ebc9 (PartBody): 117 faces, 59 in features, 58 unassigned
  hole         ⌀25, 50 deep, through, at 160,-45,0, span -50..0    feat:0ef17061c4843423
  fillet       R5, 63.509 long, at -27.5,47.631,5, span -31.7..31.7 feat:0f4ea43335de14f9
  round        R50, 100 long, at -300,-175,0, span -100..0         feat:2525182f82703c5f
  unassigned:  58 plane
```

A bore states the diameter it is called by and a blend states its
radius; a fillet fills an inside corner and a round breaks an outside
one, which is the same distinction as a bore against a shaft.

Every face is accounted for: the ones no rule claimed are listed, so
that *not recognised* is never mistaken for *not there*. Lengths are
millimetres and angles degrees whatever the file declared, so one design
exported in inches and in millimetres gives one document.

This is not PMI — nothing in the file states it — so it is a separate
command with its own output, and `pmix extract` is unchanged. What is
recognised and what the tool refuses to guess at is in
[ADR 0011](docs/adr/0011-feature-recognition.md); why it exists and
where the judgment is deliberately left to the reader is in
[ADR 0012](docs/adr/0012-geometric-comparison.md).

### Exploring a STEP file

`pmix inspect` shows the raw entity graph of a STEP file, which is useful
when investigating what a CAD system actually exported:

```bash
pmix inspect part.stp                     # header, summary, count of every entity type
pmix inspect part.stp --complex           # only the complex-instance combinations
pmix inspect part.stp --type datum        # every instance with a DATUM segment
pmix inspect part.stp -e 1752 -d 2        # instance #1752 and what it references, 2 levels deep
pmix inspect part.stp --diagnostics       # show parser warnings and recovered errors
pmix inspect part.stp -e 1752 --json      # machine-readable output
```

The parser is tolerant: malformed records are reported and skipped, and
the rest of the file is still read.

`pmix extract` and `pmix diff` accept JT files as they do STEP files, and
produce the same document. `pmix inspect` reports a JT file's header, its
segment inventory, and the elements of the segments that carry PMI:

```bash
pmix extract assembly.jt --output pmi.json
pmix inspect assembly.jt
pmix inspect assembly.jt --type "PMI data" --json
```

Geometry segments are listed but never decoded, because `pmix` compares
PMI rather than shape ([ADR 0009](docs/adr/0009-jt-reader.md)). The
exception is the smart topology table, which abstracts a part's precise
geometry and is read in full so that JT dimensions can be anchored on the
faces they apply to ([ADR 0010](docs/adr/0010-jt-precise-geometry.md)):
each face with the surface it lies on, its loops, and the edges and
curves those run along, and the tag a PMI callout names it by.

### Logging

Turn on diagnostic logging with `-v` (debug) or `-vv` (trace), or set
`RUST_LOG` for fine-grained control. Logs go to stderr, so JSON on stdout
stays clean:

```bash
pmix -v extract part.stp > part.pmi.json
```

Run `pmix --help` or `pmix extract --help` for the full option list.

## Output format

The output is a single JSON document defined in
[ADR 0002](docs/adr/0002-semantic-pmi-model.md). The shape is versioned
through a `schema_version` field so that downstream tooling can detect
breaking changes. Abridged:

```json
{
  "schema_version": 1,
  "source": { "file_name": "part.stp", "format": "STEP", "schema": "AP242_MANAGED_MODEL_BASED_3D_ENGINEERING_MIM_LF" },
  "units": { "length": "mm", "angle": "deg" },
  "properties": [
    { "id": "prop:Part_Number", "name": "Part_Number", "category": "PLM__Part_Number",
      "kind": "user", "value": { "type": "text", "value": "SYN-004-REV-A" },
      "unmapped": [], "source_refs": ["#40", "#43", "#42", "#41"] },
    { "id": "prop:Part_Count", "name": "Part_Count", "category": "TECH__Part_Count",
      "kind": "user", "value": { "type": "integer", "value": 12 },
      "unmapped": [], "source_refs": ["#80", "#83", "#82", "#81"] },
    { "id": "prop:…", "name": "affected area", "category": "pmi validation property",
      "kind": "validation", "value": { "type": "measure", "value": 120.5, "unit": "mm2" },
      "applies_to": "tol:…", "unmapped": [], "source_refs": ["#70", "#74", "#73", "#72"] }
  ],
  "semantic": {
    "features": [
      { "id": "feat:…", "kind": "face", "name": "hole",
        "geometry": [{ "kind": "face", "surface": "cylinder", "source_ref": "#25" }],
        "members": [], "origin": "semantic", "presentation": [], "unmapped": [], "source_refs": ["#40", "#41"] }
    ],
    "dimensions": [
      { "id": "dim:…", "kind": "size", "subtype": "diameter",
        "value": { "value": 12.5, "unit": "mm" },
        "tolerance": { "type": "plus_minus", "lower": { "value": -0.05, "unit": "mm" }, "upper": { "value": 0.05, "unit": "mm" } },
        "modifiers": [], "features": ["feat:…"], "decimal_places": 2,
        "origin": "semantic", "presentation": [], "unmapped": [], "source_refs": ["#42", "#46", "#45", "#50", "#49"] }
    ],
    "datums": [
      { "id": "datum:A", "label": "A", "features": ["feat:…"],
        "targets": [{ "label": "A1", "kind": "point", "diameter": { "value": 2.0, "unit": "mm" },
                      "placement": { "origin": [10.0, 10.0, 0.0], "axis": { "x": 0.0, "y": 0.0, "z": 1.0 } }, "feature": "feat:…", "unmapped": [] }],
        "origin": "semantic", "presentation": [], "unmapped": [], "source_refs": ["#42", "#43", "#45"] }
    ],
    "datum_systems": [
      { "id": "dsys:A|B|C", "text": "A|B(M)|C",
        "compartments": [
          { "datums": [{ "datum": "datum:…", "modifiers": [], "modifier_values": [] }], "common": false },
          { "datums": [{ "datum": "datum:…", "modifiers": ["maximum_material"], "modifier_values": [] }], "common": false },
          { "datums": [{ "datum": "datum:…", "modifiers": [], "modifier_values": [] }], "common": false }
        ],
        "origin": "semantic", "presentation": [], "unmapped": [], "source_refs": ["#73", "#70", "#71", "#72"] }
    ],
    "tolerances": [
      { "id": "tol:…", "kind": "position", "text": "⌖ ⌀0.1 Ⓟ10 Ⓜ | A | B Ⓜ | C",
        "value": { "value": 0.1, "unit": "mm" },
        "zone": { "form": "cylindrical_or_circular", "projected": { "value": 10.0, "unit": "mm" } },
        "modifiers": ["maximum_material"], "datum_system": "dsys:A|B|C", "features": ["feat:…"], "decimal_places": 2,
        "origin": "semantic", "presentation": [], "unmapped": [], "source_refs": ["#84", "#82", "#86", "#87"] }
    ],
    "notes": [], "other": []
  },
  "presentation": {
    "annotations": [
      { "id": "ann:…", "kind": "position", "label": "Position.1",
        "text": "⌖ ⌀0.1 Ⓟ10 Ⓜ | A | B Ⓜ | C", "text_origin": "semantic",
        "plane": { "origin": [20.0, 20.0, 0.0], "axis": { "x": 0.0, "y": 0.0, "z": 1.0 } },
        "leaders": [],
        "geometry": { "polylines": 1, "triangles": 1, "points": 8, "bbox": { "min": [20.0, 20.0, 0.0], "max": [24.0, 22.0, 0.0] }, "hash": "…" },
        "parts": [{ "form": "tessellated", "kind": "position", "geometry": { "…": "…" }, "source_ref": "#97" }],
        "style": { "colour": "#ff0000", "line_font": "continuous", "layer": "PMI layer" },
        "semantic": ["tol:…"], "features": ["feat:…"], "views": ["view:MBD_A"],
        "attributes": {}, "unmapped": [], "source_refs": ["#98", "#97", "#99", "#100"] }
    ],
    "views": [
      { "id": "view:MBD_A", "name": "MBD_A",
        "camera": { "placement": { "origin": [0.0, 0.0, 100.0] }, "projection": "parallel", "view_plane_distance": 100.0, "view_window": [200.0, 150.0] },
        "clipping_planes": [], "annotations": ["ann:…"], "unmapped": [], "source_refs": ["#215", "#214", "#217"] }
    ]
  },
  "unknown": [],
  "diagnostics": []
}
```

The `properties` array holds named values that are neither PMI nor
geometry: part numbers, revisions, suppliers, prices, and the CAx-IF
validation properties a file writes so a consuming system can check how it
read the PMI. A property attaches to the whole part, or to one PMI record
through `applies_to`. Values written as strings are read as numbers when
nothing is lost by doing so, so a count compares as a number while a
serial such as `007` stays text. See
[ADR 0007](docs/adr/0007-properties.md) and
[ADR 0008](docs/adr/0008-numeric-property-values.md).

Every array is sorted by id. Ids are *identity keys*
([ADR 0004](docs/adr/0004-identity.md)): they name which design element a
record is, so the same tolerance keeps its id across re-exports even when
its value changes. Both readers use one scheme. Datums, datum systems, and
saved views get readable ids (`datum:A`, `dsys:A|B|C`, `view:MBD_A`), and
those are the same from a STEP file and a JT file of one design, because
their identity is design intent rather than geometry. Features,
dimensions, tolerances, and annotations get hashed ids anchored on the
geometry they apply to: the B-rep faces for STEP, and where the annotation
attaches to the part for JT, which is why those two do not yet match
across the formats. Measures are kept in the unit the file declares. Anything
the reader recognises but cannot map is reported under `unknown` rather
than dropped. The model lives in [`src/model/`](src/model/).

## Library use

The crate is split into a library and a thin CLI. The library is the intended
integration point for other tools:

```rust
use std::path::Path;

fn main() -> Result<(), pmix::Error> {
    let document = pmix::extract(Path::new("part.stp"))?;
    println!("{} annotations", document.annotations.len());
    Ok(())
}
```

API documentation will be on [docs.rs](https://docs.rs/pmix) once the crate
is published; until then, `cargo doc --open` builds it locally.

## Roadmap

- [x] Project scaffolding, CLI skeleton, versioned JSON model
- [x] Test corpus: NIST MBE PMI AP242 models ([tests/fixtures/nist](tests/fixtures/nist))
- [x] STEP Part 21 parser and `pmix inspect` for exploring entity graphs
- [x] Semantic data model (ADR 0002)
- [x] STEP AP242 reader: units, features, dimensions with tolerances
- [x] STEP AP242 reader: geometric tolerances, datums, datum targets, datum systems
- [x] Presentation data model (ADR 0003)
- [x] STEP AP242 reader: annotations (tessellated, polyline, placeholder, text), styles, links, saved views
- [x] Stable identity across exports (ADR 0004)
- [x] `pmix diff` (ADR 0005)
- [x] Product and record properties (ADR 0007)
- [x] JT reader: file structure, segments, and decompression (ADR 0009)
- [x] JT 9 files: 32-bit table of contents, ZLIB segments, and two-byte element versions (ADR 0009)
- [x] JT reader: PMI Manager element, model units, and the semantic layer
- [x] JT reader: annotations and saved views, with the PMI each view shows
- [x] One identity scheme for both formats; datums, datum frames, and views share ids across them (ADR 0004)
- [x] JT compressed integer packets and the smart topology table (ADR 0010)
- [x] JT analytic surface and curve geometry, each attached to the face or edge it belongs to (ADR 0010)
- [x] Resolving a JT callout to the B-rep faces it applies to (ADR 0010)
- [x] One fingerprint recipe for both readers, anchoring a dimension on the faces it is about (ADR 0004)
- [x] Units normalised: one design keys the same way whether it states millimetres or inches (ADR 0004)
- [ ] Confirming a JT and a STEP fingerprint agree, which needs a model published in both formats
- [x] `pmix features`: holes, counterbores, countersinks, and bosses from the B-rep of either format (ADR 0011)
- [x] Fillets, rounds, and chamfers, decided by tangency between a face and the two it joins (ADR 0011)
- [ ] Chamfers on straight edges, which are planes rather than cones and which no local rule settles (ADR 0011)
- [ ] Comparing two feature documents, once the document has been used enough to say what a comparison of it needs (ADR 0012)
- [x] Binaries and installers for macOS, Linux, and Windows from GitHub releases (ADR 0006)

## Design

The reader is a targeted, pure-Rust implementation rather than a binding to
OpenCascade or a full EXPRESS schema. The reasoning and the resulting design
rules are recorded in
[ADR 0001](docs/adr/0001-step-parsing-strategy.md).

## Contributing

Contributions are welcome. Please read [CONTRIBUTING.md](CONTRIBUTING.md)
for how to set up a development environment, run the checks, and submit a
pull request. This project follows the
[Contributor Covenant](CODE_OF_CONDUCT.md) code of conduct.

Sample STEP and JT files that contain PMI are especially valuable for testing.
If you can share one under a permissive licence, please open an issue.
[docs/test-data.md](docs/test-data.md) lists what the project already uses,
what else is available, and which sources must be avoided for licence reasons.

## License

Licensed under the [MIT License](LICENSE.md).
