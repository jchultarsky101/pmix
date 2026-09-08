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
The packet allows five codecs. All but the chopper are written, that
being the only one no file has yet asked for. An unimplemented codec is
reported with the byte it was found at rather than guessed at, because a
guess would produce plausible numbers rather than an error, and plausible
wrong geometry is worse than none.

### What is read so far

The whole topology, and the surfaces of the geometry after it: the
counts, the twenty-three compressed vectors, the checksum, the geometry
counts, and every analytic surface the table describes, with its
location, axis, radii, and half angle. `pmix inspect` reports the counts
and the mix of surface kinds per part.

The counts are trustworthy: they are plain integers, and they check
against each other. Every part in the test file states one body, at least
as many shells as regions, at least as many loops as faces, and exactly
twice as many coedges as edges, which is what a closed solid requires.

The topology is a fixed chain of twenty-three vectors: two for the
bodies, two for the regions, four for the shells, five for the faces,
three for the loops, two for the coedges, and five for the edges. Reading
exactly that many lands on the geometry counts, and those check the
reading: a B-rep has one surface per face and one curve per edge, and
every part in the test file agrees on both. Reading the wrong number of
vectors would put arbitrary bytes there instead.

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

**Which face each surface belongs to.** The surfaces are read but not
attached to faces, and the table does not appear to say. This was
investigated on 2026-09-08 and the obvious answers were ruled out, so
they are recorded here rather than tried again.

- **The surface index is not a face identifier.** For every part the two
  are different sets; neither contains the other.
- **The surface index is not a position among the surfaces.** If it were,
  the values it skips would be exactly the surfaces the table does not
  describe. They are not, in any part. The clearest case is the part
  whose surfaces are all described: with nothing skipped the index would
  run from zero to one less than the count, and instead it has gaps and
  runs past the end.
- **None of the five face vectors holds a surface reference.** Each was
  scored against the set of described surfaces, in both raw and
  accumulated form, over every part. The only vector whose values all
  name a described surface is the flag that is zero everywhere, which
  says nothing.

What remains consistent with all of that is the specification's own
words: the index is a mapping to the *original* B-rep surfaces. In
Parasolid XT a face references its surface, and that reference lives in
the XT data the topology table was chosen to avoid reading. So the
correspondence may simply not be in the table.

Three ways forward, none yet taken:

- **Read only the face-to-surface reference out of the XT B-rep**, rather
  than the whole format. Far less than a full XT reader, and it is the
  one thing missing.
- **Follow the edges.** An edge names the faces it separates and the
  curve it lies on, and curves carry indices in the same numbering as
  surfaces. If that numbering is consistent between the two, the edge
  vectors would tie a face to a curve and so to the numbering the
  surfaces use.
- **Find a file whose parts describe every surface.** Then a positional
  correspondence would be the only possibility left, and checkable.

**Curves and points**, which the fingerprint recipe uses for edges and
vertices. Surfaces are the part it needs first.

### What the surfaces required

**A surface type is written one above the value the specification
lists.** The table gives PLANE as 0 and TORUS as 4; files write 1 and 5.
The arrays prove it. Each surface draws a location and two directions
from shared arrays, and only the curved kinds draw radii and angles, so
the length of the radius and angle arrays is fixed by the mix of kinds.
With the documented values those arrays are far too short for the
surfaces they would have to describe; subtracting one makes every part
in the test file come out exact, all eight of them.

That the arrays are laid end to end is what makes this checkable at all:
one surface read wrongly derails every surface after it, so ending on
exactly the declared count is a strong statement that the reading is
right. A further sign: the cones come out at a half angle of exactly a
quarter of pi, which is a chamfer cut at forty-five degrees.

### What the move-to-front codec required

Nothing contradicted the specification here, but the specification says
little: there is no algorithm section for it, only a paragraph. It holds
no code text of its own, just two nested packets, the values in the
order they were first seen and the offsets that replay them against a
window of the sixteen most recent. An offset outside the window means
the value was not in it and comes from the values stream instead.

Adding it took every table to the end of its topology, which is what
made the geometry counts readable and confirmed the chain length.

### What the arithmetic codec required

Three things about it are not what the specification says, and each was
settled by reading a real file.

**The histogram follows the code text, not precedes it.** The figure can
be read either way; only one of the two produces an entry count a file
could hold.

**A value in the histogram is unsigned, though the field is typed I32.**
The specification stores a value as its distance above the table's
minimum, which cannot be negative, so reading it signed corrupts any
value whose top stored bit is set. The symptom is subtle: identifiers
came out nearly right, ascending with occasional runs that stepped
backwards.

**A histogram with no escape symbol is followed by nothing.** The figure
shows an out-of-band count and array after every arithmetic packet. In
practice they are written only when the histogram has an escape symbol
to stand in for them, which is what makes them necessary. Reading a
count that is not there consumes the next packet's header.

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
