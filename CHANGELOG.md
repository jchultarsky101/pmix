# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- ADR 0001 recording the STEP parsing strategy, and the NIST MBE PMI AP242
  test corpus under `tests/fixtures/nist`.
- Project scaffolding: library and CLI crate layout, CI workflow, issue and
  pull request templates, contribution guidelines, and code of conduct.
- `pmix extract` command with `--output` and `--compact` options.
- `--verbose` flag and `RUST_LOG` support for diagnostic logging via `tracing`.
- Input format detection for STEP (`.stp`, `.step`, `.p21`) and JT (`.jt`).
- Versioned JSON data model (`schema_version` 1) for extracted PMI.

[Unreleased]: https://github.com/jchultarsky101/pmix/compare/main...HEAD
