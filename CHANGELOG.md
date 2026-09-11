# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **`pmix product`**, a third document saying what a file *contains*
  (ADR 0014): the parts it names, their revisions, and every occurrence
  of each with the placement that puts it where it sits. Occurrences are
  stated one by one rather than rolled into a quantity, because two uses
  of one part sit in different places; each part counts its own uses so
  that nothing has to tally the list. Sorted, content-keyed, and in
  millimetres whatever the file declared.

  A part keys on its number, name and revision, which is what a person
  would call it — not on its geometry, because a subassembly has none.
  Where a file states none of the three, the id can only rest on the
  order the file lists them in, and the document says so rather than
  letting a reader discover it by diffing two exports. One NIST model
  does exactly this.

  JT files return an empty document with a diagnostic: the scene graph's
  node hierarchy is not read yet.

- **Each body is joined to the part it is the shape of.** A part names
  its bodies by the ids `pmix features` gives them, so the two documents
  join, and both go through one entry point so a body cannot have two
  ids. One body however many times the part is used — the shape is
  stated once and the occurrence count says how often it appears. A body
  that reaches no product definition is listed as unattached with the
  reason, because a file can state geometry it never defines a product
  for and silence would make that look like a part with no shape.

- **Every body states how big it is.** An axis-aligned box and the
  overall size that box implies, largest dimension first so that the
  three numbers do not depend on how the part happened to be oriented
  when it was exported. It is the first thing a catalogue asks for and
  the last thing `pmix` could say.

  A box is decided by extremes, and for the shapes this reads the
  extremes are all on something the B-rep states: a vertex, a full
  circle, or a sphere. An arc bulges past its own endpoints but never
  outside the circle it lies on, so an arc that stays inside the box
  everything else made — a rounded corner, the mouth of a hole — leaves
  the measurement exact. Where a face has no closed form at all, the box
  is the smallest the body can be rather than the size it is, and
  `approximate` says which of the two is being given.

  Volume and surface area are *not* computed: those need the trimmed
  patch of each face, which this B-rep does not hold. ADR 0014 is amended
  accordingly.

- **`list_parts` and `describe_part`**, two new Model Context Protocol
  tools (ADR 0013, ADR 0014). `list_parts` gives a model the bill of
  materials — which parts, what revision, how many of each — and
  `describe_part` gathers all three documents into one answer to "what
  is this part": its number and revision, how big each body is, the
  features recognised in them, and the material, mass and finish the
  file states. `describe_model` stays geometry-only.

  The server's instructions now say that these files may describe
  confidential designs and that a part number or material spec read from
  one is not to be sent to a web search or any other service unless the
  user asked. That paragraph is the one thing a client shows a model
  before it picks a tool, and it is where a caution cannot be stripped
  by a transport that renders only the fields it understands.

- **Material, mass, volume and the rest are promoted out of the
  properties that carry them**, into named fields that say which key
  they came from. Matching is by what a key *says* — ordinary words for
  the thing — rather than by a table of keys copied from one exporter.

  Nothing is lost: an unmatched key stays exactly where it was, and each
  part counts what was not promoted. Where several keys claim one field
  the values decide it — agreement promotes once, disagreement promotes
  nothing and shows every candidate, because a guess about a material or
  a mass is worse than none. On a real corpus that is the difference
  between silently reporting a bounding-box volume as the part's volume
  and saying that the file states two volumes.

### Changed

- **What a part has to state before a model can source a substitute for
  it** is recorded in [ADR 0014](docs/adr/0014-part-description-for-sourcing.md),
  proposed: product structure and part identity first, then envelope,
  volume and material, then the hole patterns and threads a replacement
  has to match. No behaviour changes yet — the roadmap and the ADR index
  name the stages.

## [0.13.0] - 2026-09-10

`pmix mcp` serves the documents to a language model over the Model
Context Protocol, so a model can ask what a file says, what a shape is,
and how two models differ, and reason about the answers — the consumer
ADR 0012 had in mind. Four tools, each a call into the library and
nothing more; the server computes nothing.

### Added

- **`pmix mcp`**, a Model Context Protocol server over standard input and
  output (ADR 0013), so a language model can ask what a file says and what
  a shape is, and reason about how two models differ. Four tools —
  `describe_model`, `compare_models`, `extract_pmi`, `diff_pmi` — each a
  call into the library and nothing more. The server computes nothing.

  The tools are shaped for a context window: `describe_model` with
  `summary: true` returns each body's counts without listing a feature,
  and `body`, `kind`, and `only_changed` narrow what follows. A view never
  changes what a number means — counts describe the whole body whatever
  the view lists — and each carries the filter that produced it. Those
  views live in the library (`features::view`), where they are tested.

  Written directly against JSON-RPC rather than through an SDK: a
  tools-only server needs five methods, and the alternative brings an
  asynchronous runtime and a tree of dependencies into a crate that is
  nine crates deep. So there was nothing to gate behind a Cargo feature,
  and there is none.

  The handshake tells the model, in the protocol's own `instructions`
  field, the one thing it must know before reading a comparison: a
  possible pairing is an observation the tool does not stand behind.

  A tool that fails names the path it failed on, so a model that passed
  two is told which. [`docs/mcp.md`](docs/mcp.md) is the guide: setup
  in Claude Code and Claude Desktop, each tool with its arguments and
  what it returns, how to read a comparison, and driving the server by
  hand.

## [0.12.0] - 2026-09-10

The one thing `pmix` emits that it declines to stand behind now says so
in the data rather than in a sentence printed beside it. A comparison's
`candidates` become `possible_pairings`, each stating what it rests on,
which is a breaking change to that document and the reason for the minor
bump. Plus the docs corrections that went with it.

### Changed

- **The caution about a possible pairing is now in the document**
  (ADR 0013), and the comparison schema version is 2. It used to be a
  sentence the CLI printed beneath its output, so anything reading the
  JSON — a language model, most obviously — saw a list that looked like
  findings and would report it as findings. That is the failure ADR 0012
  set out to avoid, reintroduced by the transport.

  It is carried three ways, so no consumer can drop it by accident:
  `candidates` is renamed **`possible_pairings`**; each one states in
  **`paired_on`** the single agreement it rests on (`size`, `place`, or
  `size_and_place`); and the document carries the sentence in **`notes`**,
  keyed to the field it is about. The text output reads that note rather
  than restating it, so the two cannot drift apart.


- **What is still needed to confirm cross-format identity is now stated
  accurately.** The roadmap, ADR 0004, ADR 0010, and `tests/cross_format.rs`
  all said it needed "a model published in both formats". Two thirds of
  that has quietly gone away: no XT B-rep parser is required, because the
  smart topology table gives the same geometry (ADR 0010), and the pair
  need not carry PMI at all, because a recognised feature is keyed on
  geometry alone (ADR 0011). What is actually needed is one design
  exported to both formats **with the JT written with precise geometry** —
  the JT exports that have reached this project were tessellation-only,
  which is a different and much easier obstacle to remove. The test's own
  doc comment was staler still: it claimed the JT reader had no B-rep to
  fingerprint, which stopped being true in 0.6.0.

## [0.11.0] - 2026-09-10

`pmix features` now compares two models as well as describing one: what
pairs exactly, a displacement stated once where one explains a whole
body, and candidates offered as observations rather than conclusions. It
still will not decide whether a feature moved or was removed with
another added, because geometry cannot tell those apart. Building the
comparison turned up three identity defects, each found because a
difference came out with nothing different about it.

### Added

- **`pmix features` compares two models** (ADR 0012). Given two or more
  inputs it sets them against each other instead of describing one:
  what pairs by id and is therefore provably the same, what is left over
  on each side stated in full, and a **displacement** reported once
  where one explains every difference in a body. Exit status is 0 when
  nothing differs and 1 when something does, so it works as a check.

  A **candidate** pairs two leftovers of one kind that agree on either
  their size or their place, and names the fields that differ — "same
  place; diameter 8 → 10", or "5 apart; position 20,15,0 → 25,15,0". It
  is an observation with its distance attached, never a conclusion: one
  feature moved and one removed with another added are the same
  geometry, and nothing here can tell them apart.

  Bodies pair on their shared *faces* rather than their shared features,
  because an edit to one feature leaves every face it did not touch
  exactly as it was, and those are the evidence.

### Fixed

- **A cone is now keyed by its apex** rather than by the radius it has
  wherever the file placed it (ADR 0004). The same cone placed further
  along its own axis states a different radius there, so two exports of
  one cone were keyed differently whenever they placed it differently.
  Across the NIST re-export pair this takes the features that pair from
  41 to 45.

- **Faces lying on one surface are taken together even when they do not
  touch.** The chamfer round a hexagonal head is six patches of one
  cone, parted by the six flats, and it was reported as six chamfers
  sharing one id. Faces on one surface whose extents lie over each other
  are one face of it; bores of one size drilled one behind the other
  follow each other along the axis instead, and stay separate.

- **A feature's angle is now right for a file stating degrees.** The
  neutral B-rep claimed millimetres and degrees while carrying radians,
  so the conversion was applied to an already-converted value.

## [0.10.0] - 2026-09-09

`pmix features` now recognises the blends as well as the holes. A fillet
fills an inside corner and a round breaks an outside one, a chamfer cuts
a corner off where a cone can prove it, and each states the measurement
it is actually called by. Across the NIST corpus this takes the share of
faces that go into some feature from about a tenth to a third.

### Added

- **Fillets, rounds, and chamfers** in `pmix features` (ADR 0011). A
  blend is decided at the join rather than on the face: where a surface
  carries on smoothly through it, it is a blend, and where it leaves a
  corner, it is not. Which side the material is on then separates a
  **fillet**, filling an inside corner, from a **round**, breaking an
  outside one — the same distinction that separates a bore from a shaft.
  A cone that is tangent to neither of the faces it sits between is a
  **chamfer**; beside a bore the same cone is a countersink, and only
  what it opens into tells the two apart.

  Each feature now states the measurement it is actually called by. A
  blend has a `radius` and a `length`, a bore a `diameter` and a
  `depth`, and neither states the other's. A blend runs along an edge
  rather than into the material, so it no longer answers whether it goes
  through.

  Deciding this needed the normal of a surface at a point, which
  identity never did (`features::surface`).

  Across the NIST corpus this takes the share of faces that go into some
  feature from about a tenth to a third. It does **not** recognise a
  chamfer on a straight edge: that is a plane, not a cone, and no local
  rule separates a narrow planar strip from a narrow face that was
  always meant to be there. Those stay listed as unassigned.

## [0.9.0] - 2026-09-09

`pmix features` reads a model's geometry and describes what the part
*is*, rather than what its file *says*: the holes, counterbores,
countersinks and bosses it is made of, with their diameters, depths and
positions. It is the base data for answering the question people
actually bring to two CAD files — not whether they differ, but how.

### Added

- **`pmix features`**, a new command that recognises manufacturing
  features from a model's geometry and writes them as its own document
  (ADR 0011). It reads holes (through and blind), counterbores,
  countersinks, and bosses from the B-rep of either format, and states
  each one's diameter, depth, axis, position, and the stretch of that
  axis it occupies, in millimetres and degrees whatever the file
  declared.

  This is not PMI. Nothing in the file states it, so it is a separate
  command with its own output and its own schema version, and
  `pmix extract` is unchanged.

  Every face is accounted for: those no rule claimed are listed in the
  output rather than passed over, so that *not recognised* cannot be
  mistaken for *not there*. A file with no B-rep says so in a diagnostic
  instead of reporting an empty result.

  The point of the document is comparison (ADR 0012). A feature states
  its parameters and not only its identity, because "Ø4.5 became Ø5.0"
  cannot be recovered from two fingerprints; and the same design bored
  wider differs from its baseline in one field, moved differs in one
  field, and restated in inches does not differ at all. What `pmix`
  deliberately does not do is decide whether a hole moved or was removed
  and another added: that is not decidable from geometry, so both are
  described and the judgment is left to the reader of the document.

  A feature is identified by the surface it is the wall of, not by the
  faces it is made of, so a bore written as one cylindrical face and the
  same bore written as two halves are one feature with one id.

## [0.8.0] - 2026-09-09

A datum feature or datum target the file gives no geometry is now
anchored on the datum it establishes, the way a drawing names it. Across
the NIST corpus that takes the features with nothing to anchor on from 46
to 18, and the 18 that remain now say plainly in a diagnostic that their
ids will not survive a re-export.

### Fixed

- **A datum feature or datum target the file gives no geometry is now
  anchored on the datum it establishes**, the way a drawing names it —
  `A`, or `A` target `1`. Across the NIST corpus 26 of the 44 such
  features fell back to a CAD label or to the source entity number,
  which does not survive a re-export. The datum's letter takes
  precedence over the aspect's own name, because a name like
  `Simple Datum.5` carries an index that moves when the model is edited.
- The 18 that remain are bare shape aspects a note or a tolerance hangs
  off, for which the file states nothing at all. Their diagnostic now
  says plainly that their ids will not survive a re-export.

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

- **The features document is documented.** `pmix features --json` and its
  comparison have had their own schema versions since 0.9.0 and 0.11.0,
  and the README described only the PMI document. Both shapes are now
  shown, along with why a bore states a diameter where a blend states a
  radius.

- **`docs/test-data.md` says what data is still wanted**, in the terms
  someone could act on: one design in both formats with the JT written
  with precise geometry, not PMI, and how to check a candidate JT in one
  command.

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

[Unreleased]: https://github.com/jchultarsky101/pmix/compare/v0.13.0...HEAD
[0.13.0]: https://github.com/jchultarsky101/pmix/compare/v0.12.0...v0.13.0
[0.12.0]: https://github.com/jchultarsky101/pmix/compare/v0.11.0...v0.12.0
[0.11.0]: https://github.com/jchultarsky101/pmix/compare/v0.10.0...v0.11.0
[0.10.0]: https://github.com/jchultarsky101/pmix/compare/v0.9.0...v0.10.0
[0.9.0]: https://github.com/jchultarsky101/pmix/compare/v0.8.0...v0.9.0
[0.8.0]: https://github.com/jchultarsky101/pmix/compare/v0.7.0...v0.8.0
[0.7.0]: https://github.com/jchultarsky101/pmix/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/jchultarsky101/pmix/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/jchultarsky101/pmix/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/jchultarsky101/pmix/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/jchultarsky101/pmix/compare/v0.2.1...v0.3.0
[0.2.1]: https://github.com/jchultarsky101/pmix/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/jchultarsky101/pmix/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/jchultarsky101/pmix/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/jchultarsky101/pmix/releases/tag/v0.1.0
