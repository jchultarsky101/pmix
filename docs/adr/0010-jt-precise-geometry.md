# ADR 0010: Reading JT precise geometry

- **Status:** Accepted, 2026-09-08
- **Deciders:** Julian Chultarsky
- **Depends on:** [ADR 0004](0004-identity.md), [ADR 0009](0009-jt-reader.md)

## Context

ADR 0004 anchors a STEP dimension or tolerance on the B-rep faces it
applies to, fingerprinted from their surface geometry. The JT reader has
no such anchor, so it falls back to where an annotation attaches to the
part. That is why a dimension read from STEP and the same dimension read
from JT do not share an id, which is the one gap left in cross-format
identity.

Closing it needs the geometry a JT PMI entity points at. A PMI
association names a B-rep face by face group, and a face group indexes
the faces of a body by increasing face identifier (specification section
11.13). So the reader needs, per part, the faces of its B-rep and the
surface each one lies on.

The test file offers two routes to that.

**XT B-Rep segments** hold the precise geometry as Parasolid XT. The
format is documented in the specification's annex F, across some fifty
pages covering curves, surfaces, topology, and a schema. It is the
authoritative geometry, and it is a large, self-contained format to
implement.

**Smart Topology Table segments** hold, in the specification's words, "a
lightweight abstraction of the existing precise B-Rep data". Annex H
documents it in about twenty pages: entity counts, topology, and
*analytic geometry information*. The test file carries one such table for
each of its eight parts with precise geometry.

## Decision

**Read the Smart Topology Table, not the XT B-rep.** The fingerprint
recipe in ADR 0004 wants a surface kind and the parameters that fix it in
space. That is exactly what the table's analytic geometry section holds,
and reading it costs a fraction of implementing Parasolid XT. The XT
segments stay listed and undecoded, as the other geometry segments are.

If a file carries XT B-rep but no topology table, its parts get no
feature anchor and their dimensions keep the attachment-point key. That
is a degradation, not a failure.

**Implement the compressed integer packet first.** Every vector in the
table is stored as an Int32 compressed data packet (specification section
10.2.1), so nothing in the table is readable without it. It is
implemented as its own module with its own tests, because it is used by
the ULP and LWPA segments too should those ever be read.

**Implement only the codecs that real files use, and refuse the rest.**
The packet allows five codecs. The null and bitlength codecs are written;
arithmetic, chopper, and move-to-front are not. An unimplemented codec is
reported with the byte it was found at rather than guessed at, because a
guess would produce plausible numbers rather than an error, and plausible
wrong geometry is worse than none.

### What is read so far

The counts that head the table, and a walk over the compressed vectors
after them. `pmix inspect` reports the counts per part.

The counts are trustworthy: they are plain integers, and they check
against each other. Every part in the test file states one body, at least
as many shells as regions, at least as many loops as faces, and exactly
twice as many coedges as edges, which is what a closed solid requires.

### What is not read yet, and why

**Three of the five face vectors are named, from what they contain.**
The specification's figure shows four vectors per face; this file writes
five, so position alone identifies nothing. The first holds values that
are distinct, ascending, start at zero, and outrun the face count, which
is what the specification describes a face identifier as: unique, and not
an index. Two others hold only zeros and ones, so they are the two flags;
one is clear on every face of every part, which is what an outward facing
solid gives for the orientation flag, and the other varies. The remaining
two are left unnamed. Neither is the start-loop index the figure lists
first: one is not monotonic, and the running total of the other does not
reach the loop count.

**Three of the eight tables.** They reach the arithmetic or
move-to-front codec within their first few vectors, so their faces are
not read either. The other five give up their faces.

**The geometric data section**, which is what identity actually needs.
It follows the topology in the same element, past a vector every table
reaches that uses the arithmetic codec. So the arithmetic codec is now
the thing standing between the reader and the surfaces, rather than one
of several open questions.

## Alternatives considered

- **Implement Parasolid XT.** The authoritative geometry, fully
  documented, and perhaps ten times the work of the topology table for
  the same fingerprints. Worth revisiting only if a file turns up that
  carries XT B-rep without a topology table.
- **Anchor on the face index alone.** A PMI association names a face, and
  that name could go into the identity key without any geometry. It would
  be stable within a file and renumbered between exports, which is the
  same failure that rules out STEP entity numbers (ADR 0004).
- **Wait for a matched STEP and JT pair before starting.** No such file
  is published, and the work needed to use one is the same either way.
  Building the reader first at least makes the JT anchor better than the
  attachment point, whether or not a pair ever appears.

## Consequences

- `pmix inspect` reports each part's B-rep size.
- The compressed integer packet is available to any future reader of
  JT's other table-shaped segments.
- Cross-format identity for dimensions and tolerances stays open. The
  remaining steps are the vector meanings, the two missing codecs, the
  geometric data, and linking a part's scene-graph node to both its PMI
  segment and its topology table.
