# ADR 0001: STEP parsing strategy

- **Status:** Accepted, 2026-09-06
- **Deciders:** Julian Chultarsky

## Context

`pmix` must read PMI (dimensions, geometric tolerances, datums, notes, and
their graphical presentation) from STEP AP242 files, and later from JT files,
into a JSON model stable enough to diff across exports. It ships as a Rust CLI
on Linux, macOS, and Windows, is MIT licensed, and will be published to
crates.io. Three approaches were evaluated.

### Option 1: OpenCascade Technology (OCCT)

OCCT's XDE layer has read AP242 semantic PMI since 7.0 (2016): all fifteen
tolerance types with modifiers and material conditions, datums with targets,
tessellated presentation, and saved views. It is the strongest existing
implementation.

Findings against it:

- No Rust bindings expose the PMI layer. The maintained `opencascade` crate
  wraps OCCT 7.8.1 and only the plain `STEPControl_Reader`. Roughly five
  classes and forty getters of C++ shim code would be needed.
- OCCT 7.8 and 7.9 return null tolerance magnitudes on NX and NIST AP242
  files; the fix landed in 8.0, which the Rust bindings reject. Reaching 8.0
  means forking the build crate or shipping prebuilt archives.
- OCCT never reads notes from STEP and silently drops anything it does not
  map. Silent loss is the worst failure mode for a diff tool.
- Distribution: `cargo install` would require CMake plus a C++ compiler and a
  10 to 20 minute build; prebuilt binaries are tens of megabytes per platform;
  Windows static CRT matching is unverified.
- Licence: LGPL 2.1 with an exception covering header material only. Workable
  for an open-source MIT binary, but adds notice and source-availability
  obligations.
- OCCT has no JT reader, so the JT side would be from scratch regardless.

### Option 2: Existing crates on crates.io

- No Rust crate has compiled the full AP242 EXPRESS schema (about 2,100
  entity types); `espr` overflows its stack on it and Foxtrot's full AP214
  code generation produced a 39,000-line file.
- `ruststep` is dormant, self-described as experimental, ships only AP201 and
  AP203, and has an unresolved design for complex instances, which is how
  AP242 encodes every tolerance.
- `step-io` is the only crate that extracts AP242 PMI. It is months old,
  single-author, "heals or drops" what it does not understand, and exposes no
  tolerance modifiers.
- `openbim-step` is AGPL. `truck-stepio`, `step-p21`, `iso-10303` are geometry
  or older-AP only.

### Option 3: Targeted pure-Rust reader

A Part 21 tokenizer plus an untyped entity graph is a few hundred lines. A
semantic PMI extractor then pattern-matches roughly 70 to 90 core entity
types (up to about 150 with styling and saved views), keyed on the presence of
complex-instance segments. This is how `step-io`, OCCT's own `StepDimTol`
layer, and the NIST STEP File Analyzer work in practice. The full schema is
never needed.

Ground truth exists: NIST publishes 17 AP242 PMI test files in the public
domain with expected-PMI results, and the NIST STEP File Analyzer reconstructs
every feature control frame as text, giving an oracle to compare against.

## Decision

Build a targeted, pure-Rust STEP reader (option 3). Do not depend on OCCT or
on existing STEP crates, including for the Part 21 tokenizer, because the
reader needs source spans, error recovery, and an untyped graph rather than
the serde-oriented AST those crates provide.

Design rules that follow from the decision:

1. **`Reader` trait per format.** STEP and JT readers produce the same
   intermediate model. An optional OCCT-backed reader can be added behind a
   feature flag later if a gap proves too hard to close.
2. **Two-layer model.** A *presentation* layer of callouts (kind, text,
   polylines or tessellation, annotation plane, leader points, linked faces,
   saved-view membership) and an optional *semantic* layer (dimensions,
   tolerances, datums, datum systems, feature references). STEP feeds both;
   JT through 10.0 feeds mostly presentation plus a property bag.
3. **Never drop silently.** Recognised but unmapped content becomes an
   explicit `unknown` record carrying the raw entity, so diffs surface it.
4. **Stable identity.** Entity numbers differ between exports of the same
   part. Diffing needs content-derived identifiers and geometry fingerprints
   for feature references. This is deferred to its own ADR before `pmix diff`.

## Consequences

- `cargo install pmix` works with no system dependencies; binaries stay small;
  licensing stays MIT-clean.
- The project owns the AP242 interpretation and all vendor quirks. The NIST
  corpus and expected-PMI results are the primary defence; every extractor
  change is validated against them.
- Phased delivery: fixtures and oracle; Part 21 parser and `pmix inspect`;
  semantic PMI; presentation PMI and saved views; identity and `pmix diff`;
  JT reader.
