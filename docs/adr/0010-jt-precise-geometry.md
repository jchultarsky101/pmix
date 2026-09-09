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
nothing needs them yet; edges name their vertices by index already.

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
- Cross-format identity for dimensions and tolerances stays open. What
  remains is on the PMI side rather than the geometry side: resolving a
  face group to the faces it names, and fingerprinting those faces to the
  same recipe the STEP reader uses.
