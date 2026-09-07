# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: https://github.com/jchultarsky101/pmix/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/jchultarsky101/pmix/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/jchultarsky101/pmix/releases/tag/v0.1.0
