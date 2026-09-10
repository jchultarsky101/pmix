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

### What is read

The whole table. The counts, the twenty-three compressed vectors of the
topology, the checksum, the geometry counts, and every analytic surface
and curve the geometry describes, each attached to the face or edge it
belongs to. `pmix inspect` reports, per part, the counts and what its
faces and edges lie on.

The counts are trustworthy: they are plain integers, and they check
against each other. Every part in the test file states one body, at least
as many shells as regions, at least as many loops as faces, and exactly
twice as many coedges as edges, which is what a closed solid requires.

The topology is a fixed chain of twenty-three vectors: two for the
bodies, two for the regions, four for the shells, five for the faces,
three for the loops, two for the coedges, and five for the edges. Reading
exactly that many lands on the geometry counts, and those check the
reading: a B-rep has one surface per face and one curve per edge, and
every part in the test file agrees on both.

The strongest check is the walk. Starting from the faces and following
the start indices down to the edges reaches every loop exactly once,
every coedge exactly once, and every edge exactly twice, on all eight
parts. A start index that is wrong by one anywhere breaks that.

### What the predictors required

**A predictor does not apply to the whole vector: the first four values
are primers.** The prose describes each predictor as though it applied
throughout, and the format's own decoder in annex B does not — it copies
the first four residuals through untouched and predicts only from the
fifth.

This was shipped wrong in 0.3.0 and the symptom was quiet. Accumulating
from the second value gives a vector that is still ascending and still
distinct, so it passes for a face identifier; the face identifiers that
release reported were in fact start-loop indices, and no test could tell.
What catches it is following the chain: with the whole vector
accumulated, the start indices overshoot the sections they point into,
and the walk from faces to edges reaches loops that do not exist. With
four primers every start index in every part lands in range and the walk
is exact.

### What the face and edge sections required

**The face and edge sections each hold one vector more than the
specification's figure shows, and both are the kind of geometry the
entity lies on.** A face's vector uses the documented surface types
(0 plane through 4 torus) and an edge's the documented curve types
(0 line, 1 circle, 2 ellipse); values beyond those are the kinds the
table has no closed form for. They account for exactly the entities left
undescribed: on every part, the faces stating an analytic kind number
exactly the described surfaces, and the same holds for edges and curves.

This also makes a face useful even when its surface is a spline: the
reader can still say what kind of face it is.

### Which face each surface belongs to

**The index a described surface carries is the position of its face, and
a curve's is the position of its edge.**

An earlier investigation ruled this out and was wrong, because it was
reasoning about indices decoded with the whole vector accumulated. Once
the primers were right the values changed, and every one of them fell
inside the face count. The three readings that were ruled out then are no
longer the same three, so the note recording them has been removed rather
than corrected.

Three readings remained, and they are distinguishable because the face
identifiers are a permutation of the face positions rather than the
identity:

- the index is the face's **position**;
- the index is the face's **identifier**;
- the surface's own position in the array is the face's position.

They are told apart geometrically, not structurally. A curve bounding a
face lies on that face's surface, so a wrong mapping puts a curve off its
surface by millimetres. Measuring every analytic curve bounding an
analytic face across all eight parts: the first reading puts 2998 of 2998
on the surface to within a nanometre, and the other two put 37% and 63%
on it. That check is a test, and it is written so no tolerance could let a
wrong mapping through.

A wrong mapping here would be worse than none, because it would produce
fingerprints that look reasonable and silently match the wrong callouts.
That is why it is verified against geometry rather than argued from
structure.

**Points** are not read. They are quantised rather than exact, and
nothing needs them yet; edges name their vertices by index already. They
are stepped over to reach the attributes, and the figure's branch around
the quantiser covers the coordinates too: a part with nothing written
explicitly has neither, and its precision follows the flags directly.

### Which faces a callout applies to

An association names a B-rep face by an **index into the PMI CAD tag
list**, not by a face group number. The specification allows both, but
every file seen so far uses the tag. So the tag pool after the CAD tag
indices is read as well (section 10.2.16); the sixty-four bit tags are
not, because no file has written one, and a tag that was not read names
nothing rather than the wrong face.

The tags themselves are persistent identifiers from the originating
system, and the topology table's **attribute section** carries one per
face. That section is not reached without stepping over the body
attributes, whose layout is another figures-versus-reality problem: the
file writes two checksum blocks where the figure shows one, and two
fields the figure does not show at all. Those two are stepped over
unnamed rather than guessed at. The landing point is checked instead:
the face identifier count that follows must equal the face count, and a
packet of exactly that many distinct values must follow it. Searching
the same region for anything satisfying those constraints finds the same
offset on all eight parts, which is what says the walk lands where it
should.

**Edges carry tags too**, in the edge attribute section after the faces,
and a callout names one the same way. Reaching them meant walking past
the face checksums and what the figure calls the face moniker table.
What the file writes there is not that table: a count of faces, a packet
of one more value than there are faces, a word per face, and two more
packets. Those are stepped over unnamed rather than guessed at, and the
landing point is checked instead — searching the region for an edge
identifier count followed by that many distinct values finds exactly the
same offset on all eight parts. Every edge then carries a distinct tag,
and every edge a callout names is an edge some part has, all 75 of them.

An edge is fingerprinted as its curve kind and its two ends, sorted, by
the same shared recipe the STEP reader uses. That matters because a
length is often measured between two edges rather than between two
faces: anchoring edges took the test file from 63 of 78 dimensions
reaching their geometry to 71, and tolerances from 15 of 16 to all 16.

**The nth tag belongs to the nth face**, because the table already
stores its faces in face group order. It is worth being explicit about
what was ruled out. Section 11.13 says a face group numbers a body's
faces by increasing face identifier, and the table has a Face Identifier
vector, so ordering the faces by it looks like the rule. It is not: that
vector is a permutation of the positions and belongs to a different
numbering. Both readings resolve to a real face of the right part, so
neither can be told from the other by structure.

Geometry tells them apart. A callout's leader ends on what it points at,
which the file states in millimetres against a table stated in metres.
Counting the leaders that land on a face their callout names, over the
faces where the two readings disagree: by position 74, by identifier 14.
The reading also has to survive a machinist's reading of the result, and
does — a callout that says THRU reaches a cylinder, and a counterbore
callout reaches two cylinders and a plane, at radii that make sense for
the fastener.

Most of the leaders that land on nothing are not a failure of the
mapping: their distance is exactly the radius of a cylinder the callout
names, because a leader to a hole is drawn to the hole's axis rather
than to its wall. The test allows for that.

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
- A face can now be fingerprinted: it names its surface, its loops, and
  through them its edges and their curves. That is what ADR 0004's recipe
  needs from the JT side.
- A JT annotation reaches the faces and edges it applies to, each face
  its surface, each edge its curve, and each of them a fingerprint by the
  recipe the STEP reader uses. A JT dimension, tolerance, and datum is anchored on the geometry
  it is about rather than on where it is drawn.
- The point geometry stays unread. Vertices come from the curves instead,
  which is exact where the quantised points would not be.
- What remains for cross-format identity is one design exported to both
  formats **with the JT written with precise geometry**; a JT exported as
  tessellation alone carries no topology segment and nothing to
  fingerprint. Since ADR 0011 it need not carry PMI either, because a
  recognised feature is keyed on geometry alone. Everything either reader
  can do without such a pair is done.
