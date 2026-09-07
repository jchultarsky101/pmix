# pmix

[![CI](https://github.com/jchultarsky101/pmix/actions/workflows/ci.yml/badge.svg)](https://github.com/jchultarsky101/pmix/actions/workflows/ci.yml)
<!-- Restore when the crate is published:
[![crates.io](https://img.shields.io/crates/v/pmix.svg)](https://crates.io/crates/pmix)
[![docs.rs](https://img.shields.io/docsrs/pmix)](https://docs.rs/pmix)
-->
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE.md)

**pmix** is a command-line tool that reads a 3D model file (STEP or JT),
extracts the Product Manufacturing Information (PMI) embedded in it, and
writes that information to a JSON file in a stable, comparable form.

The end goal is to run `pmix` on two or more models and diff the results, so
you can answer questions like "did the tolerances change between revision B
and revision C?" without opening a CAD package.

> **Status: early development.** `pmix extract` reads STEP AP242 files and
> emits both layers of the model: the semantic layer (units, features,
> dimensions with tolerances, geometric tolerances with zones, modifiers and
> composites, datums with targets, datum reference frames) and the
> presentation layer (annotations with text, plane, leaders, style, geometry
> summary and links to the semantic records; saved views). It extracts every
> such entity in the NIST test corpus. Identity across exports, `pmix diff`,
> and JT are not there yet. Nothing is published to crates.io yet. See the
> [roadmap](#roadmap).

## What is PMI?

PMI is the set of annotations that turn a bare geometric model into a
manufacturing specification: dimensions and their tolerances, geometric
dimensioning and tolerancing (GD&T) feature control frames, datums, surface
finish symbols, and notes. In a model-based definition (MBD) workflow this
data lives inside the 3D file rather than on a 2D drawing.

## Supported formats

| Format | Standard            | Extensions               | Status  |
| ------ | ------------------- | ------------------------ | ------- |
| STEP   | ISO 10303 (AP242)   | `.stp`, `.step`, `.p21`  | planned |
| JT     | ISO 14306           | `.jt`                    | planned |

## Installation

Once published, `pmix` will be installable from crates.io:

```bash
cargo install pmix
```

Until then, build from source (requires Rust 1.85 or newer):

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

Every array is sorted by id. Ids are *identity keys*
([ADR 0004](docs/adr/0004-identity.md)): they name which design element a
record is, so the same tolerance keeps its id across re-exports even when
its value changes. Datums, datum systems, and saved views get readable ids
(`datum:A`, `dsys:A|B|C`, `view:MBD_A`); features, dimensions,
tolerances, and annotations get hashed ids anchored on the B-rep geometry
they apply to. Measures are kept in the unit the file declares. Anything
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

API documentation will be available on [docs.rs](https://docs.rs/pmix) once
the crate is published.

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
- [ ] `pmix diff`
- [ ] JT reader: PMI Manager segment
- [ ] Publish to crates.io

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
