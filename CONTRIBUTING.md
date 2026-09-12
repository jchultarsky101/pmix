# Contributing to pmix

Thank you for your interest in contributing. This document explains how to
get set up, what we expect from a change, and how the review process works.

## Ways to contribute

- **Report bugs** and **request features** through
  [GitHub issues](https://github.com/jchultarsky101/pmix/issues). Please use the
  issue templates; they ask for the details we need.
- **Share sample files.** Publicly shareable STEP or JT files that contain
  PMI are the single most useful thing you can give this project. Open an
  issue describing the file, the CAD system that produced it, and the licence
  it can be redistributed under.
- **Improve documentation.** Typos, unclear wording, and missing examples are
  all fair game.
- **Submit code.** See below.

## Development setup

You need a stable Rust toolchain at or above the minimum supported version
declared in `Cargo.toml` (`rust-version`). Install it with
[rustup](https://rustup.rs/).

```bash
git clone https://github.com/jchultarsky101/pmix.git
cd pmix
cargo build
cargo test
```

Before opening a pull request, run the same checks CI runs — all four,
with the same environment, because two of them only fail when warnings
are denied:

```bash
cargo fmt --all -- --check
RUSTFLAGS="-D warnings" cargo clippy --all-targets --all-features -- -D warnings
RUSTFLAGS="-D warnings" cargo test --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --document-private-items --all-features
```

The `RUSTDOCFLAGS` on the last one is not decoration. Without it the doc
build *passes* on a rustdoc link to a private item, and CI fails on it —
which is exactly how a pull request in this repository went red with the
other three green.

## Making changes

1. Open an issue first for anything beyond a small fix, so we can agree on the
   approach before you invest time in it.
2. Fork the repository and create a branch from `main`.
3. Keep pull requests focused. One logical change per PR is much easier to
   review than a grab bag.
4. Add or update tests for any behaviour you change. Integration tests for the
   CLI live in `tests/`; unit tests live next to the code they cover.
5. Keep the documentation in step with the code, in the same pull request.
   Update `README.md` for any change to usage, flags, or output; add an
   entry under `## [Unreleased]` in `CHANGELOG.md` for user-visible changes;
   keep rustdoc on public items current; and record design decisions as
   architecture decision records under `docs/adr/`. A change is not complete
   until its documentation is.
6. Make sure the checks above pass locally.
7. Open the pull request and fill in the template.

### Commit messages

Write commit messages in the imperative mood ("Add JT header parser", not
"Added" or "Adds"). The first line should be a short summary under about 72
characters; add a blank line and more detail below if the change needs it.

### Code style

- `rustfmt` and `clippy` with the settings in this repository are the style
  guide. If clippy is wrong for a specific case, silence it locally with an
  `#[allow]` and a comment explaining why.
- Public items need documentation comments. `cargo doc` must build without
  warnings.
- Prefer returning errors over panicking. The library must not panic on
  malformed input; a corrupt model file is an expected condition, not a bug.
- Keep the JSON output deterministic. Anything that ends up in the output must
  be sorted or otherwise ordered independently of the input file's internal
  entity numbering.

## Review process

A maintainer will review your pull request, usually within a week. Reviews
may ask for changes; that is a normal part of the process, not a rejection.
Once approved and green in CI, a maintainer will merge it.

## Releases

Releases are cut by the maintainers, in this order:

1. Bump the version in `Cargo.toml`, and the `> **Status: X.Y.Z.**`
   paragraph at the top of `README.md` — check that what it *says* is
   still true, not only that the number matches. It sat at 0.13.0 for
   three releases, describing a tool that had since grown two
   subcommands.
2. Move `## [Unreleased]` in `CHANGELOG.md` into a new dated section, with
   a short paragraph saying what the release is *for*, and add the
   comparison link at the foot of the file.
3. Open a pull request for the two. Run the four checks above locally
   first — the docs build included — and merge once CI is green.
4. Push a `vX.Y.Z` tag. cargo-dist builds the binaries and installers and
   attaches them to the GitHub release
   ([ADR 0006](docs/adr/0006-distribution.md)). **The tag is what
   publishes** — nothing is released until it is pushed.
5. **Announce it.** Post a short note to the team's Slack channel: the
   version, the release link, a few bullets of what changed, and any
   action users must take. Mention breaking changes explicitly and say
   "no action needed" when there are none.

Step 5 is the one that gets forgotten, because step 4 makes the release
*exist* and it is easy to stop there. A release nobody is told about is a
release nobody uses.

## Licence

By contributing you agree that your contributions will be licensed under the
[MIT License](LICENSE.md) that covers the project.
