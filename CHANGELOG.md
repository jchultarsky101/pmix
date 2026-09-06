# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- ADR 0003 (proposed): presentation layer of the PMI data model.
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

[Unreleased]: https://github.com/jchultarsky101/pmix/compare/main...HEAD
