# ADR 0009: Reading JT files

- **Status:** Accepted, 2026-09-07
- **Deciders:** Julian Chultarsky
- **Depends on:** [ADR 0001](0001-step-parsing-strategy.md), [ADR 0003](0003-presentation-pmi-model.md)

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
- **`pmix extract` still refuses JT** until the PMI walker lands. Claiming
  to extract and returning nothing would read as "this file has no PMI",
  which is worse than an honest refusal. `pmix inspect` works now.

## Alternatives considered

- **Bind to TKJT or PyOpenJt.** Ruled out by GPL-2.0; the project is MIT.
- **A commercial SDK.** Paid, closed, and unavailable to contributors.
- **Convert JT to STEP with an external tool first.** Moves the problem to
  a tool the user must buy, and loses exactly the PMI associations the
  comparison depends on.
- **Decode geometry too.** Multiplies the work for data the tool does not
  compare.

## Consequences

- `pmix inspect` reads JT files and reports the header, the segment
  inventory, and the elements of the segments it decodes.
- The `unknown` list will gain JT segment types that are recognised but not
  decoded, so nothing is silently ignored.
- `docs/test-data.md` records the NIST assembly as a verified PMI-bearing
  JT fixture.
