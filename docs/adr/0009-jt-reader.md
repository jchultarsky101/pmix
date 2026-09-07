# ADR 0009: Reading JT files

- **Status:** Accepted, 2026-09-07
- **Deciders:** Julian Chultarsky
- **Depends on:** [ADR 0001](0001-step-parsing-strategy.md), [ADR 0002](0002-semantic-pmi-model.md), [ADR 0003](0003-presentation-pmi-model.md)
- **Amended:** 2026-09-07, with what building the PMI walker established

## Context

JT (ISO 14306) is the second format `pmix` must read. Unlike STEP it is
binary, so the questions ADR 0001 answered for STEP have to be answered
again with different evidence.

**The specification is available.** Siemens publishes the JT format
reference free of charge and without registration. Revision D of the
version 10.6 description, February 2026, is a 6.6 MB PDF.

**Existing implementations cannot be used.** The two open-source JT
readers, Open Cascade's JT Assistant with its TKJT toolkit, and PyOpenJt,
are both GPL-2.0. Linking either into an MIT-licensed tool would force the
whole project to GPL. The alternatives, Siemens' own JT Open Toolkit and
the various commercial SDKs, are paid and closed. So there is no middle
path of the kind ADR 0001 weighed for STEP: a reader has to be written.

**A licence-clean test file exists and contains PMI.** The NIST MTC
assembly, public domain, is JT 10.5 written by NX. Its 107 segments
include 14 PMI Data segments and 30 Meta Data segments, which settles a
question `docs/test-data.md` had recorded as unverified.

**The structure was confirmed against that file**, not just read from the
specification. A 109-byte header (80-byte version string, byte order flag,
reserved field, table of contents offset, and the scene graph segment
identifier), a table of contents of 32-byte entries, and segments carrying
a header of identifier, type, and length. Segment payloads in this file are
XZ compressed, announced by a compression flag of 3 and an algorithm byte
of 3, and they begin with the XZ magic number rather than raw LZMA.

## Decision

**Write a pure-Rust reader**, for the same reasons ADR 0001 gave for STEP,
with licence incompatibility making the choice forced rather than merely
preferable.

**Scope it to PMI.** JT carries tessellated geometry, several B-rep
flavours, and levels of detail, none of which `pmix` needs. The reader
parses the file structure and then only the segments that carry PMI and
metadata, types 3 and 4. Geometry segments are listed in the inventory and
never decoded. This keeps a large format tractable.

**Add one dependency, `lzma-rs`**, a pure-Rust LZMA, LZMA2, and XZ codec.
Compression is unavoidable because JT 10 segments are XZ compressed, and a
pure-Rust decoder preserves ADR 0006's property that the tool builds and
cross-compiles for every release target with no C toolchain. Binding
liblzma through `xz2` would have broken that.

**Deliver in increments**, as the STEP reader was: the file structure
first, which is JT's equivalent of the Part 21 parser, then the PMI
Manager element, then the mapping onto the model that ADR 0003 already
describes.

### Limits, deliberately

- **Little-endian only.** The header carries a byte order flag, but every
  file available is little-endian and untested code for the other case
  would be a liability. A big-endian file is rejected with a clear error.
- **Version 10 verified.** The reader parses the header of any version and
  reports what it finds, but only 10.x is tested. Older layouts differ and
  will be added when a test file for them exists.
- **The presentation layer comes later.** `pmix extract` fills the
  semantic layer of ADR 0002 from JT and leaves the presentation layer of
  ADR 0003 empty, so annotations and saved views are not compared yet.
  The data is there: the PMI element carries model views and the 2D
  geometry of every annotation, and the reader already walks past both.

## What the file format turned out to require

Three things were not apparent from the specification and are recorded
here because they are the reason the reader is shaped as it is.

**The meaning is in the properties.** A JT PMI entity has almost no typed
fields. Its magnitude, its deviations, its material condition, and the
datums it references are all key and value strings on the entity, named
by the specification's PMI property list. The reader is therefore a
mapping from property keys onto the model rather than a walk over a
typed structure, and it matches keys by their last segment wherever it
can: writers vary the path around a common shape, so NX writes
`ToleranceCompartment[0].PrimaryDatum.Reference[0].label` where the
specification lists `Primary.Datum[0].label`.

**Style outnumbers meaning.** A single dimension carries around 150
properties, most of them about how it is drawn: fonts, colours, arrow
geometry, leader routing. Those belong to the presentation layer, so the
semantic records leave them out rather than bury the design data under
them. The reader keeps a list of the drawing properties it recognises and
records everything else it did not read as `unmapped`, so a property that
carries meaning cannot be lost by being unlisted.

**A callout is several dimensions.** A hole and thread note states no
value of its own; its measurements are nested under `ParameterDimension`
entries. Each of those becomes its own dimension record, because a hole's
diameter changing between two revisions is exactly what the tool exists
to report.

Two things the file said contradicted the specification's figures and
were taken from the file. A property atom's hidden flag is one byte, not
four; parsing it as four breaks the element a few hundred bytes in. And a
scene graph holds two element streams one after another, the nodes and
then the property atoms, rather than the single stream the figure shows.

### Where the units come from

JT states no unit on a PMI value. The model's unit is a string property
of the scene graph, `JT_PROP_MEASUREMENT_UNITS`, so the reader reads the
property table to find it and every measure carries it. A file that does
not declare it produces a diagnostic and measures with no unit, rather
than a guess. Angles are degrees, which is what the PMI property list
specifies.

JT also does not say whether a dimension measures a size or a location,
which ADR 0002's model requires. The `featureOfSize` flag settles it when
set; otherwise a radius or a diameter counts as a size, because it can be
nothing else.

## Alternatives considered

- **Bind to TKJT or PyOpenJt.** Ruled out by GPL-2.0; the project is MIT.
- **A commercial SDK.** Paid, closed, and unavailable to contributors.
- **Convert JT to STEP with an external tool first.** Moves the problem to
  a tool the user must buy, and loses exactly the PMI associations the
  comparison depends on.
- **Decode geometry too.** Multiplies the work for data the tool does not
  compare.

## Consequences

- `pmix extract` and `pmix diff` accept JT files and produce the same
  document as for STEP, so the two formats are comparable in principle.
  They are not yet comparable in practice: identity (ADR 0004) is
  anchored on B-rep geometry fingerprints that the JT reader does not
  compute, so ids match within a format but not across the two.
- `pmix inspect` reads JT files and reports the header, the segment
  inventory, and the elements of the segments it decodes.
- The `unknown` list will gain JT segment types that are recognised but not
  decoded, so nothing is silently ignored.
- `docs/test-data.md` records the NIST assembly as a verified PMI-bearing
  JT fixture.
