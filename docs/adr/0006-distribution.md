# ADR 0006: Distribution through GitHub releases

- **Status:** Accepted, 2026-09-07
- **Deciders:** Julian Chultarsky

## Context

0.1.0 was prepared for crates.io. Before publishing, the decision was
taken to hold off: the base functionality (JT input, unit normalisation,
assemblies) should work well before the crate name is claimed with a
release that users will `cargo install` and judge. Users still need a way
to install the tool on Windows, macOS, and Linux without a Rust toolchain.

## Decision

- **crates.io publishing is postponed** until the base functionality is
  complete. The `Cargo.toml` metadata stays publish-ready so nothing has
  to be reworked then.
- **Binaries and installers come from GitHub releases**, built by
  [cargo-dist](https://opensource.axo.dev/cargo-dist/) in CI on every
  `v*` tag: archives for macOS (Apple Silicon and Intel), Linux (x86-64
  and ARM64), and Windows (x86-64 and ARM64), a shell installer script for
  macOS and Linux, a PowerShell installer script for Windows, and the
  `pmix-update` updater binary that upgrades an installation in place from
  the latest release.
- **Releases are cut by tagging**: bump the version in `Cargo.toml`, close
  the changelog section, merge, tag `vX.Y.Z`, push the tag. The workflow
  builds, tests, and publishes the release with its artifacts and the
  changelog section as notes.

## Consequences

- `dist-workspace.toml` and `.github/workflows/release.yml` are generated
  by cargo-dist and must be regenerated with `dist init` when its
  configuration changes; the CI workflow is checked against the
  configuration on every run.
- The README's installation section leads with the installer scripts and
  binaries; `cargo install pmix` returns when publishing resumes.
- Adding a target is a one-line change to the configuration.
