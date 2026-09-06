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

> **Status: early development.** The CLI, JSON data model, and format
> detection exist. The STEP and JT readers are not implemented yet, so
> `pmix extract` currently reports the format as unsupported. Nothing is
> published to crates.io yet. See the [roadmap](#roadmap).

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
pmix extract part.jt --output part.pmi.json --compact
```

Turn on diagnostic logging with `-v` (debug) or `-vv` (trace), or set
`RUST_LOG` for fine-grained control. Logs go to stderr, so JSON on stdout
stays clean:

```bash
pmix -v extract part.stp > part.pmi.json
```

Run `pmix --help` or `pmix extract --help` for the full option list.

## Output format

The output is a single JSON document. The shape is versioned through a
`schema_version` field so that downstream tooling can detect breaking changes.

```json
{
  "schema_version": 1,
  "source": {
    "file_name": "part.stp",
    "format": "STEP"
  },
  "annotations": [
    {
      "kind": "dimension",
      "text": "⌀12.5 ±0.05"
    },
    {
      "kind": "geometric_tolerance",
      "text": "⌖ ⌀0.1 Ⓜ A B C"
    }
  ]
}
```

Collections are sorted deterministically and identifiers are derived from
content rather than from the source file's internal entity numbering, so two
extractions of semantically identical PMI produce byte-identical output. The
model will grow considerably as the readers are implemented; the current
definition lives in [`src/model.rs`](src/model.rs).

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
- [ ] STEP AP242 reader: semantic PMI (dimensions, tolerances, datums)
- [ ] STEP AP242 reader: presentation PMI (graphical annotations)
- [ ] JT reader: PMI segment
- [ ] `pmix diff` command to compare two or more extractions
- [ ] Publish to crates.io

## Contributing

Contributions are welcome. Please read [CONTRIBUTING.md](CONTRIBUTING.md)
for how to set up a development environment, run the checks, and submit a
pull request. This project follows the
[Contributor Covenant](CODE_OF_CONDUCT.md) code of conduct.

Sample STEP and JT files that contain PMI are especially valuable for testing.
If you can share one under a permissive licence, please open an issue.

## License

Licensed under the [MIT License](LICENSE.md).
