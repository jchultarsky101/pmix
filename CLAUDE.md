# pmix — project rules for Claude Code

`pmix` is a Rust CLI and library that extracts PMI (Product Manufacturing
Information) from STEP AP242 and, later, JT files into comparable JSON.
Read [docs/adr/](docs/adr/) before changing architecture.

## Rules

- **Keep documentation in step with code.** Every change that alters
  behaviour, CLI flags, JSON output, module layout, or a design decision must
  update the affected docs in the same PR: `README.md` (usage, roadmap),
  `CHANGELOG.md` (`[Unreleased]`), rustdoc on public items, `docs/adr/` for
  design decisions, and `tests/fixtures/*/README.md` for fixture changes. A
  PR is not done until its docs are.
- **Design decisions go in ADRs**, not in README or commit messages. One
  numbered file per decision under `docs/adr/`, added to the index there.
  README links to ADRs; it does not restate them.
- **Libraries:** clap, thiserror, tracing. Add other crates only with a
  reason in the PR description.
- **Never drop PMI silently.** Unmapped content becomes an explicit
  `unknown` record. Parsers report diagnostics and keep going; they do not
  panic on malformed input.
- **JSON output must be deterministic** (sorted, independent of source
  entity numbering).
- **`main` is PR-only** with all CI checks required. Squash-merge. Run
  `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`
  before pushing.
- **Test against the NIST corpus** in `tests/fixtures/nist` for every
  reader change.
