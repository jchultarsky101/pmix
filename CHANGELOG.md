# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- The bitlength codec read the wrong number of bits for a field-width
  change, so any vector using its adaptive path decoded to wrong values.
  The specification's prose and its code sample disagree on the width;
  real files settle it. Nothing released was affected, because the
  topology table this codec serves has not been in a release.

### Added

- **A part's own metadata is now extracted from JT** (`pmix::jt::meta`):
  material, density, volume, mass units, Young's modulus, part name, and
  the system that wrote the file. These live in the metadata segments,
  which the reader previously listed but never opened. Values keep the
  type the file gives them, and a number written as digits is read as a
  number so `pmix diff` can compare it.
- A reader for JT's compressed integer packets (`pmix::jt::codec`), which
  is what every table-shaped JT segment is built from. The null and
  bitlength codecs are implemented; a packet using one of the other three
  is reported with the byte it was found at rather than guessed at.
- Face identifiers and the two per-face flags are read from the smart
  topology table, which is what lets a PMI association be resolved to a
  face (ADR 0010).
- A reader for the smart topology table (`pmix::jt::stt`, ADR 0010),
  which abstracts a part's precise B-rep without needing a Parasolid
  reader. `pmix inspect` now reports each part's bodies, faces, and
  edges. What each compressed vector after the counts means is not
  established yet, so they are counted rather than interpreted.

## [0.2.1] - 2026-09-08

### Changed

- Two properties that state the same name and value are recorded once
  rather than once per part. Until a property can be attributed to the
  part that states it, the repetition carried no information. This makes
  a JT document smaller even though it now carries more.

- Reading a model is about **1.85x faster**, and around 2.5x on the files
  that carry the most annotation geometry. The output is unchanged: every
  fixture produces byte-identical JSON, ids included.
  - Summarising what an annotation draws no longer builds a string for
    every coordinate before hashing them. It hashes as it goes, through
    the new `pmix::model::ContentHasher`, and formats into one reused
    buffer.
  - Coordinates are formatted by fixed-point arithmetic rather than the
    general float formatter, which was the reader's single largest cost.
    The general formatter still handles anything outside the range where
    that is exact, and a test checks the two agree over several hundred
    thousand values.
  - An annotation with one occurrence no longer summarises the same
    geometry twice, once for the occurrence and once for the aggregate.
  - The Part 21 lexer no longer allocates a string per real number to
    normalise forms such as `1.` that Rust's float parser rejects. It
    tries the number as written first, which is nearly always enough.

## [0.2.0] - 2026-09-07

The headline is JT. `pmix extract` and `pmix diff` now accept `.jt` files
and produce the same document as for STEP, so a JT model can be compared
the way a STEP model already could.

### Added

- **JT PMI extraction** (ADR 0009). Dimensions with their values, plus and
  minus deviations and ISO fits; geometric tolerances with material
  conditions and datum reference frames; datums. A callout that nests
  several measurements, such as a hole and thread note, becomes one record
  per measurement.
- **The JT presentation layer** (ADR 0003): annotations with their kind,
  plane, style, and a summary of the lines that draw them, and saved views
  with their camera and the annotations each one shows.
  `--presentation-geometry` gives the coordinates for JT as it does for
  STEP.
- **The JT file structure reader** (`pmix::jt`): header, table of contents,
  segments, and XZ decompression of the segments that carry PMI, plus a
  walker over their element streams. `pmix inspect` reads JT files and
  reports the header, the segment inventory, and those elements. Geometry
  segments are listed but never decoded.
- **JT model units**, read from the scene graph's
  `JT_PROP_MEASUREMENT_UNITS` property, so every JT measure states the unit
  the file declares. The scene graph's other properties, such as part
  names, become the document's `properties`.
- Model views, PMI associations, and the CAD tags that resolve them, which
  is what ties an annotation to the views it appears in.
- **Identity keys for JT** (ADR 0004), so a JT id names which callout a
  record is rather than what it currently says. Changing a value, a
  tolerance, or a callout's text leaves the id alone, and `pmix diff`
  reports such an edit as a change rather than as a removal and an
  addition.
- **Ids shared between the formats.** Datums, datum reference frames, and
  saved views get the same id from a STEP file and a JT file of one
  design: `datum:A`, `dsys:A|B|C`, `view:Top`. Dimensions and geometric
  tolerances do not yet, because the STEP reader anchors them on B-rep
  fingerprints and the JT reader has no B-rep to fingerprint; ADR 0004
  records what would close that.
- **A `properties` section** (ADR 0007): named values that are neither PMI
  nor geometry, such as part numbers, revisions, suppliers, prices, and the
  CAx-IF validation properties. Values are typed (text, integer, number,
  measure with unit, boolean), a property attaches to the whole part or to
  one PMI record, and product-level properties get readable ids such as
  `prop:Part_Number`. `pmix diff` compares user properties and ignores
  validation ones, which are derived from the PMI they describe.
- Descriptive property values that are exactly a number's own rendering are
  read as integers or numbers, so counts and prices written as strings
  become comparable (ADR 0008). Values whose formatting carries meaning,
  such as `007`, `2.50`, and `1e5`, stay text.
- Derived units (`derived_unit`), so areas and volumes resolve as `mm2` and
  `mm3` instead of being reported as unrecognised.
- The NIST MTC assembly as the JT fixture, public domain, which settles
  that it carries PMI.

### Changed

- The machinery that turns an identity key into an id is shared by the
  readers (`pmix::identity`), so both use one vocabulary of prefixes and
  one collision rule. JT tolerance ids change prefix from `gtol` to `tol`
  and annotation ids from `anno` to `ann` to match STEP.
- Summarising what an annotation draws is shared too (`pmix::geometry`), so
  a geometry summary means the same thing whichever format produced it.
- A document written by 0.1.1 no longer loads, because the document gained
  a required `properties` array. Re-extract from the model file rather than
  reusing a saved JSON. Before 1.0 the document format is not stable.

### Known limits

- Cross-format matching covers datums, datum reference frames, and saved
  views, not dimensions or geometric tolerances.
- Assemblies are not modelled, so each part of one states its own datum A
  and the ids are told apart by a suffix (ADR 0004).
- JT is read little-endian only, and only version 10 is tested (ADR 0009).

## [0.1.1] - 2026-09-07

First release with prebuilt binaries.

### Added

- Prebuilt binaries, installer scripts, and the `pmix-update` updater for
  macOS (Apple Silicon and Intel), Linux (x86-64 and ARM64), and Windows
  (x86-64; Windows on ARM runs it under emulation), built by cargo-dist on every release tag (ADR 0006). crates.io publishing is
  postponed until the base functionality is complete.

## [0.1.0] - 2026-09-07

First release: STEP AP242 extraction of both PMI layers, identity across
exports, and `pmix diff`.

### Added

- `pmix diff` (ADR 0005): compares two or more models or extracted JSON
  documents by identity, reports added, removed, and changed records with
  the fields that changed, excludes extraction-only fields, prints text or
  `--json`, and exits 0/1/2 for same/different/error.
- `pmix::load` reads either a model file or a JSON document.
- Identity keys (ADR 0004): ids now name the design element rather than
  hashing content. Features are anchored on a B-rep geometry fingerprint
  (surface kind and placement plus a split-invariant span) and shape aspects on
  the same geometry merge into one feature; datums, datum systems, and
  saved views get readable ids (`datum:A`, `dsys:A|B|C`, `view:MBD_A`);
  dimensions, tolerances, and annotations key on kind and the features or
  records they apply to; collisions are ordered by content, never by entity
  numbering. Ids are assigned in a finalisation pass after all walkers.
- Earlier NIST builds of four models under `tests/fixtures/nist/previous`
  and a test that ids survive re-export.
- ADR 0004: identity of records across exports.
- Presentation layer (ADR 0003) in the model and the STEP reader:
  annotations from draughting callouts and standalone occurrences with
  kind, label, text (explicit from the PMI validation property or rendered
  from the linked semantic record), plane, placeholder box and leader
  lines, style, geometry summary with bounding box and content hash, parts,
  and links to semantic records and features; saved views from cameras with
  projection, clipping planes, and the annotations they show. Semantic
  records now carry their `presentation` links.
- `pmix extract --presentation-geometry` to include full annotation
  coordinates and triangles.
- Dimensions now carry a rendered `text` such as `⌀35 -0.2/+0` or `[40]`.
- Synthetic fixture `presentation_basics` with expected JSON.
- ADR 0003: presentation layer of the PMI data model.
- STEP AP242 walkers for geometric tolerances (all fifteen types in simple
  and complex instance form, magnitude with decimal places, modifiers,
  tolerance zones with projection and runout angle, unequally disposed and
  maximum values, per-unit basis, composite frames, rendered text such as
  `⌖ ⌀0.1 Ⓟ10 Ⓜ | A | B Ⓜ | C`) and for datums (datum features, point,
  line, circle, rectangle, circular-line and area targets with placement and
  size, datum reference frames with per-datum modifiers and common datums).
  Every tolerance, datum, target, and datum system in the NIST corpus is
  extracted.
- Synthetic fixture `tolerance_datum_basics` with expected JSON.
- Semantic data model (`pmix::model`) implementing ADR 0002: features,
  datums, datum systems, dimensions, geometric tolerances, notes, and an
  `unknown` list for content that is recognised but not mapped.
- STEP AP242 reader (`pmix::step::pmi`) with walkers for units, features
  (shape aspects resolved to B-rep faces), and dimensions with values,
  limits, plus-minus and limits-and-fits tolerances, qualifiers, and
  modifiers. `pmix extract` now produces output for STEP files.
- Synthetic fixtures under `tests/fixtures/synthetic` with expected JSON.
- ADR 0002: semantic layer of the PMI data model.
- `docs/test-data.md`: test-data strategy and a licence-checked survey of
  public STEP and JT sources.
- STEP Part 21 parser (`pmix::step::p21`): header, simple and complex
  instances, typed values, all string escapes, edition-3 section skipping,
  and error recovery with line-numbered diagnostics. Parses the entire NIST
  AP242 corpus without diagnostics.
- `pmix inspect` command to explore a STEP file's entity graph: type counts,
  complex-instance combinations, per-type listing, entity dumps with
  reference depth, diagnostics, and JSON output.
- `CLAUDE.md` with project rules, including keeping documentation in step
  with code changes.
- ADR 0001 recording the STEP parsing strategy, and the NIST MBE PMI AP242
  test corpus under `tests/fixtures/nist`.
- Project scaffolding: library and CLI crate layout, CI workflow, issue and
  pull request templates, contribution guidelines, and code of conduct.
- `pmix extract` command with `--output` and `--compact` options.
- `--verbose` flag and `RUST_LOG` support for diagnostic logging via `tracing`.
- Input format detection for STEP (`.stp`, `.step`, `.p21`) and JT (`.jt`).
- Versioned JSON data model (`schema_version` 1) for extracted PMI.

[Unreleased]: https://github.com/jchultarsky101/pmix/compare/v0.2.1...HEAD
[0.2.1]: https://github.com/jchultarsky101/pmix/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/jchultarsky101/pmix/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/jchultarsky101/pmix/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/jchultarsky101/pmix/releases/tag/v0.1.0
