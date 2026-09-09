# ADR 0011: Recognising manufacturing features from geometry

- **Status:** Accepted, 2026-09-09
- **Deciders:** Julian Chultarsky
- **Depends on:** [ADR 0004](0004-identity.md), [ADR 0010](0010-jt-precise-geometry.md)
- **Leads to:** [ADR 0012](0012-geometric-comparison.md)

## Context

`pmix` reads a part's precise geometry already. The JT reader decodes the
smart topology table into faces with their surfaces, loops, coedges,
edges with their curves, and vertices, and every edge in the test file is
shared by exactly two faces, so the face adjacency graph that feature
recognition works over is derivable directly. The STEP reader walks the
same structure per face to fingerprint it.

None of that is used for anything but anchoring PMI.

**Why now.** The question people bring to a pair of CAD files is what
changed, in words: *both have a hole there, but one is a larger
diameter*. `pmix diff` cannot say that, because it matches on exact
identity and has no vocabulary for a shape (ADR 0012). Features are that
vocabulary. A difference can only be phrased as "Ø4.5 became Ø5.0" by
something that knows one of these is a hole and that a hole has a
diameter.

There is data for it, too. No corpus available to this project carries
any PMI beyond the seventeen NIST files, while the two hundred customer
models are AP242 with full B-rep and no PMI at all.

**What the data looks like.** Across a sample of those models:

| surface | share |
| --- | --- |
| plane | 63.4% |
| cylinder | 20.7% |
| cone | 15.7% |
| B-spline | 0.2% |

Essentially everything is analytic. In the NIST JT fixture, of 939 faces,
72 cylinders are bounded only by circles, 17 are tori, and 53 are cones.

**What the rules find.** A spike over that fixture, using nothing but
surface kind, bounding curve kind, and face orientation, separated holes
from bosses cleanly: every plate yielded holes and no bosses, every
fastener part one outward-facing cylinder and no holes.

| faces | holes | bosses |
| --- | --- | --- |
| 348 | Ø3.30 ×8, Ø6.60 ×4, Ø7.50 ×3, Ø11.00 ×4, Ø12.50 ×4 | — |
| 278 | Ø4.50 ×20 | — |
| 167 | Ø3.00 ×3, Ø4.00 ×8, Ø4.50 ×4, Ø10.00 ×1 | — |
| 35 | Ø4.50 ×10 | — |
| 32 | — | Ø7.00 × 3.58 |
| 31 | — | Ø10.00 × 5.40 |
| 22 | — | Ø8.00 × 0.30 |

The diameters cross-check against the PMI the same file states: Ø4.50 is
its through-hole callout, and Ø6.60 with Ø11.00 are the two stages of its
counterbore. Two independent readings of the file agree.

**What the field says.** Recognising features from a B-rep is a research
area going back to the 1980s: attributed adjacency graphs, volume
decomposition, hint-based methods, and more recently learned approaches.
Two of its results matter here.

- Recognising an **isolated** feature is a matter of local rules over
  surface kinds, adjacency, and orientation. It is tractable.
- Recognising **interacting** features is not settled. A hole drilled
  through a pocket belongs to both, and one solid admits several valid
  decompositions. There is no single right answer to return.

## Decision

**Add feature recognition as a separate command with its own output.**
`pmix features` reads a file and emits a document of the features it
recognises. It is not part of `pmix extract`, and a feature is not a PMI
record: the semantic and presentation layers (ADR 0002, ADR 0003) keep
describing what a file *states*, and this describes what a shape *is*.

**Recognise only what local rules can prove.** The first set is the
features that a surface kind, its adjacency, and its orientation settle
between them:

| feature | what identifies it |
| --- | --- |
| through hole | a cylinder bounded by circles, normal facing its axis, open at both ends |
| blind hole | the same, capped by a plane or a cone |
| counterbore, countersink | a coaxial cylinder or cone meeting a hole |
| fillet, round | a torus, or a cylinder tangent to the two faces it joins |
| chamfer | a cone between two faces |
| boss | a cylinder whose normal faces away from its axis |

**Account for every face.** The output says which faces went into each
feature and lists every face that went into none. A recogniser that
reports what it found and stays quiet about the rest invites the reader
to assume the rest is nothing. Silence is the failure mode this whole
project has been guarding against.

**Refuse the decomposition problem.** Where two recognised features share
a face, both are reported along with the fact that they overlap, rather
than choosing one reading. Pockets and slots of arbitrary profile need
volume decomposition and are out of scope until there is a reason and a
way to test them.

**State parameters, not only identity.** A hole carries its diameter,
depth, axis, position, and whether it is through or blind, in canonical
units. Identity says whether two things are the same; only the
parameters say how they differ, and describing the difference is what
this is for (ADR 0012).

**Reuse identity, over the surface rather than the faces.** A recognised
feature is also fingerprinted by the shared recipe (ADR 0004), applied to
the surface it is the wall of: the line it turns about, its size, and the
stretch of that line it occupies. Not the faces it is made of, because
how many of those there are is an exporter's choice and not the design's.
Most systems cut a bore into two half-cylinders meeting along two
straight edges, where JT's topology table writes one face; keying on the
faces would make those two different holes. So a bore written either way,
in either format, gets one id, and whatever agrees exactly needs no
further judgment.

The same reasoning applies one level up: a body is keyed by the distinct
surfaces it is made of, so that a re-export which splits a face does not
produce a body that pairs with nothing.

### What this is not

It is not general automated feature recognition. A file whose faces are
mostly splines will yield mostly nothing, and the output will say so
rather than guessing. The JT smart topology table is an abstraction that
describes only analytic surfaces — 107 of the NIST fixture's 939 faces
have no closed form there — so JT has blind spots that STEP does not, and
the output records which reader it came from.

## Alternatives considered

- **Fold features into the semantic layer.** They are not PMI: nothing in
  the file states them, and a reader that mixes what a file says with
  what a tool inferred makes the two indistinguishable downstream.
- **Recognise features to improve PMI anchoring first.** Attractive — a
  callout on "hole 3" reads better than one on a cylinder at a
  coordinate — but it would make PMI identity depend on inference. The
  fingerprint stays geometric. Naming a feature in the PMI output can
  come later, as a label rather than as identity.
- **A learned recogniser.** Needs a labelled corpus that does not exist
  here, and gives answers that cannot be explained to someone asking why
  a hole was missed. Rules can be read.
- **Wait for a customer to ask.** The geometry is already parsed and the
  corpus is already sitting unused; the cost of finding out is a spike.

## Consequences

- A new command, a new output document, and a new schema version for it,
  independent of the PMI document's.
- The STEP reader needs a face adjacency graph, which it does not
  currently keep; the JT reader's topology table already provides one.
- Rules have to be written against what exporters actually write, not
  against the idealised solid: a bore split across faces, and the seam
  edge where a closed face meets itself, are both the normal case in
  STEP and absent from JT. A rule that asks what bounds a face has to
  pass over the seam and take the split faces together, or it matches
  nothing in real files while passing on a hand-built one.
- Ralph's two hundred models become a test corpus for something.
- The recognised feature set is a claim about a shape, so it needs
  checking against shapes whose answer is known. The synthetic fixtures
  are the place for that: a part with one through hole must yield one
  through hole and no leftovers.
