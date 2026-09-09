# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.7.0] - 2026-09-09

A design keys the same way whatever units it states. A fingerprint used
to use the file's own units, so a plane at one inch and the same plane at
25.4 millimetres were different faces, and every dimension on them a
different dimension. Everything keyed on geometry is now stated in
millimetres and degrees, and `pmix diff` compares a measure in the unit
it means rather than the unit it is written in.

### Added

- **Units are normalised.** A design does not change when it is exported
  in inches rather than millimetres, so its ids no longer do either.
  Everything keyed on geometry — surfaces, edges, vertices, annotation
  planes and bounding boxes — is stated in millimetres and degrees
  whatever the file declares, and `pmix diff` compares a measure in the
  unit it means rather than the unit it is written in, so 1 inch and
  25.4 mm are not a change. Five of the seventeen NIST fixtures state
  inches, so this was not hypothetical.

### Changed

- Records in a file stating anything but millimetres and degrees change
  id, which is the point: they now agree with the same design stated in
  millimetres. Files already in millimetres are unaffected.

## [0.6.0] - 2026-09-09

A JT callout now reaches the edges it applies to, not only the faces. A
length is usually measured between two edges rather than between two
faces, so this is most of what was still missing: 71 of the test file's
78 dimensions now reach their geometry rather than 63, and all 16
tolerances rather than 15.

### Added

- **A JT callout now reaches the edges it applies to, not only the
  faces.** An edge carries a tag in the topology table's attribute
  section just as a face does, and is fingerprinted as its curve kind
  and its two ends by the same shared recipe the STEP reader uses. A
  length is often measured between two edges, so this matters: 71 of the
  test file's 78 dimensions now reach their geometry rather than 63, and
  all 16 tolerances rather than 15. Every edge a callout names is found.

## [0.5.0] - 2026-09-09

The headline is that a property now says which part states it, on both
readers. An assembly states the same property names of every component,
and without the part in the key they were one record with
content-ordered suffixes: on a 105-component assembly, 3017 of 3117
properties carried a suffix and the largest group was 106 deep, so
adding one component reshuffled up to 106 ids.

Property ids change in files that name a part. A file describing one
unnamed part keys exactly as before, and nothing outside `properties`
changes.

### Added

- **A STEP property now says which part states it.** An assembly states
  the same property names of every component, so without the part they
  were one record with content-ordered suffixes: on a 105-component
  assembly, 3017 of 3117 properties carried a suffix and the largest
  group was 106 deep, so adding one component reshuffled up to 106 ids.
  A property attached to an assembly occurrence is named by that
  occurrence, which is what tells two uses of one product apart. The JT
  reader already worked this way.
- `pmix diff` pairs properties by name where no id pairs them, for the
  case where two readers state a property's group differently. Such a
  pairing is reported as `matched: "name"` and rendered as
  `(matched by name)`; it never changes an id.

### Fixed

- **A JT property key's trailing `::` is no longer part of its name.**
  The specification defines it as marking the property visible to a
  viewer, so keeping it made the same property two records and put a JT
  decoration in a name a STEP file also states.

### Changed

- A STEP property id reads as `prop:core.Part_Number` where the part and
  the name both allow it, and is hashed otherwise. Property ids change
  in files that name a part; a file describing one unnamed part keys
  exactly as before.

## [0.4.0] - 2026-09-08

The headline is that a JT dimension or tolerance is now anchored on the
geometry it is about rather than on where it happens to be drawn. The
whole smart topology table is read, a PMI callout resolves to the B-rep
faces it applies to, and those faces are fingerprinted by the same recipe
the STEP reader uses — which now lives in one place that both readers
call. JT 9 files can also be read.

**JT ids change in this release.** Re-extract rather than reusing saved
JSON. STEP output is unchanged, byte for byte, across every fixture.

### Added

- **A property now says which part states it**, in a new `part` field, so
  an assembly's materials and volumes can be told apart. The scene
  graph's late loaded property atoms say which node owns which segment,
  and a part names itself on its node; where the owning node is an
  unnamed child, the metadata segment's own name is used instead. The two
  together name every segment in the test file.
- The part is part of a property's identity, so two parts made of
  different materials are two records rather than one. A part saying the
  same thing twice is still recorded once.
- The arithmetic and move-to-front codecs for JT's compressed integer
  packets. **Every part in the test file now reads its whole B-rep
  topology and every analytic surface its geometry describes**: planes,
  cylinders, cones, spheres, and tori, with their locations, axes, radii,
  and half angles. Only the chopper codec is unimplemented, and no file
  has asked for it.
- **A JT face now says which surface it lies on**, along with its loops,
  their coedges, and the edges and analytic curves those run along. Every
  start index is checked by walking the table: from the faces, every loop
  is reached once, every coedge once, and every edge twice. `pmix inspect`
  reports what a part's faces and edges lie on, including the ones with
  no closed form.
- Lines, circles, and ellipses are recovered from the curve geometry, as
  the surfaces already were.
- **A JT annotation now reaches the B-rep faces it applies to.** An
  association names a face by a tag from the originating system; the
  topology table's attribute section carries one tag per face, and the
  PMI element's CAD tag pool is read to resolve it. Every face a callout
  names in the test file is found, and a through hole callout reaches the
  cylinder it is about.
- **A JT dimension, tolerance, and datum is anchored on the geometry it
  is about.** The faces a callout names are fingerprinted by the recipe
  the STEP reader uses, which now lives in one place rather than two, and
  each becomes a `feature` record in the output. A callout whose faces
  the file does not give still falls back to where it attaches to the
  part. Whether a JT fingerprint equals the STEP fingerprint of the same
  face remains unchecked, because no model is published in both formats.
- The vertices a fingerprint spans are recovered by evaluating each
  edge's curve over the stretch of it that edge covers, rather than read
  from JT's point geometry, which is quantised. Every edge meeting at a
  vertex agrees on where it is to within a micron.
- **JT 9 files can be read.** They differ from JT 10 in four places, none
  of them in the specification: a 105-byte header stating the offset of
  the table of contents in 32 bits, 28-byte table entries, ZLIB rather
  than XZ compression, and element version numbers written as two bytes
  rather than one. A JT 9 file's scene-graph properties now extract the
  same way a JT 10 file's do. Its PMI and precise geometry are read as
  version 10 states them and are **not** verified, so a pre-10 file
  carrying either gets a diagnostic saying so.

### Changed

- A JT dimension, tolerance, and datum that reaches its geometry is
  identified by that geometry, so its id differs from the one 0.3.0 gave
  it. A datum reference now resolves to the record that carries its
  letter plainly rather than to one of the suffixed duplicates.

### Fixed

- **JT vectors that use a predictor were decoded wrongly.** A predictor
  applies from the fifth value, not the second: the first four stand for
  themselves. Everything the 0.3.0 reader called a face identifier was in
  fact a start-loop index, and the identifiers it reported for a part
  were not that part's.
- The bit reader assumed one refill always supplied enough bits, which
  holds for the 32-bit words a packet's code text is written as but not
  for the bytes a histogram is written as. Reading a field wider than a
  byte from a histogram overflowed.

## [0.3.0] - 2026-09-08

The headline is that a JT extract now carries what a part is made of, not
just the PMI drawn on it: material, volume, density, mass units, Young's
modulus, and the part's name.

### Changed

- Two properties that state the same name and value are recorded once
  rather than once per part. Until a property can be attributed to the
  part that states it, the repetition carried no information. A JT
  document is smaller for it even though it now carries more.

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
- A reader for the smart topology table (`pmix::jt::stt`, ADR 0010),
  which abstracts a part's precise B-rep without needing a Parasolid
  reader. It gives each part's bodies, faces, and edges, which
  `pmix inspect` now reports, and each face's identifier, which is what
  will let a PMI association be resolved to the face it applies to. This
  is groundwork: nothing in the extracted document depends on it yet.

## [0.2.1] - 2026-09-08

### Changed

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

[Unreleased]: https://github.com/jchultarsky101/pmix/compare/v0.7.0...HEAD
[0.7.0]: https://github.com/jchultarsky101/pmix/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/jchultarsky101/pmix/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/jchultarsky101/pmix/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/jchultarsky101/pmix/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/jchultarsky101/pmix/compare/v0.2.1...v0.3.0
[0.2.1]: https://github.com/jchultarsky101/pmix/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/jchultarsky101/pmix/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/jchultarsky101/pmix/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/jchultarsky101/pmix/releases/tag/v0.1.0
