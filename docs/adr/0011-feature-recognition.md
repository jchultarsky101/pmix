# ADR 0011: Recognising manufacturing features from geometry

- **Status:** Accepted, 2026-09-09
- **Deciders:** Julian Chultarsky
- **Depends on:** [ADR 0004](0004-identity.md), [ADR 0010](0010-jt-precise-geometry.md)

## Context

`pmix` reads a part's precise geometry already. The JT reader decodes the
smart topology table into faces with their surfaces, loops, coedges,
edges with their curves, and vertices, and every edge in the test file is
shared by exactly two faces, so the face adjacency graph that feature
recognition works over is derivable directly. The STEP reader walks the
same structure per face to fingerprint it.

None of that is used for anything but anchoring PMI. Meanwhile the PMI
work is short of data: no corpus available to this project carries any
PMI beyond the seventeen NIST files. The two hundred customer models are
AP242 with full B-rep and no PMI at all, so there is nothing else they
can be used for.

**What the data looks like.** Across a sample of those models:

| surface | share |
| --- | --- |
| plane | 63.4% |
| cylinder | 20.7% |
| cone | 15.7% |
| B-spline | 0.2% |

Essentially everything is analytic. In the NIST JT fixture, of 939 faces,
72 cylinders are bounded only by circles, 17 are tori, and 53 are cones.

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

**Reuse identity.** A recognised feature is fingerprinted by the shared
recipe (ADR 0004), over the faces it is made of, so two exports of one
design name the same hole the same way and `pmix diff` can compare
feature sets.

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
- Ralph's two hundred models become a test corpus for something.
- The recognised feature set is a claim about a shape, so it needs
  checking against shapes whose answer is known. The synthetic fixtures
  are the place for that: a part with one through hole must yield one
  through hole and no leftovers.
