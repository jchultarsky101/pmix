# ADR 0012: Describing how two models differ

- **Status:** Accepted, 2026-09-09
- **Deciders:** Julian Chultarsky
- **Depends on:** [ADR 0004](0004-identity.md), [ADR 0005](0005-diff.md), [ADR 0011](0011-feature-recognition.md)

## Context

The question people actually bring to a pair of CAD files is not whether
they are identical. It is what changed, in words: *both parts have a hole
there, but one is a larger diameter*; *the mounting holes do not line up,
one is five millimetres off*.

`pmix diff` cannot answer either. It matches records by id, and an id is
an exact fingerprint (ADR 0004) quantised to a thousandth of a
millimetre. A hole whose diameter changed and a hole that moved both
produce a fingerprint that matches nothing, so both come out the same
way: one record removed, one record added. Identity answers *whether* two
things are the same. It has no notion of *near*, so it can never answer
*how different*.

Two difficulties sit behind the question.

- **Correspondence.** Deciding which hole in one model is "the same hole"
  as one in the other, when the two are not identical, is a matching
  problem over things that only partly agree.
- **Undecidability.** One hole moved five millimetres and one hole
  deleted with another added five millimetres away are the same B-rep.
  No amount of geometry separates them. A tool that always reports
  "moved" is guessing, and guessing quietly is the failure mode this
  project exists to avoid.

There is also a way to be wrong at scale: a model exported from a
different origin has *every* feature displaced, and a naive comparison
reports hundreds of differences where the truth is one.

## Decision

**`pmix` extracts the base data for a comparison; it does not author the
comparison.** Judging that two holes are the same hole moved, and saying
so in a sentence, belongs to a consumer — a language model, a reviewer, a
downstream tool. That is the part rules are bad at and the part where a
wrong answer must stay visibly an opinion. What `pmix` owes that consumer
is a description complete and specific enough to reason over.

Four things follow, and they are requirements on the features document
(ADR 0011), not on `diff`.

**Features carry parameters, not just identity.** A recognised hole
states its diameter, its depth, whether it is through or blind, its axis,
and where it sits — in canonical units. "Ø4.5 became Ø5.0" is not
recoverable from two fingerprints, however good the consumer is.

**Every face is accounted for.** ADR 0011 already requires this, and
comparison is why it matters most: a consumer handed only what was
recognised cannot tell *absent* from *not recognised*, and will read
silence as a change.

**What matches exactly is marked as matching exactly.** Where fingerprints
agree the answer is already certain and free. Saying so shrinks the set
needing judgment to the part that actually needs it.

**Placement is stated once.** A rigid transform fitted from the exact
matches, reported as a single fact when it explains the rest, instead of
the same displacement repeated across every feature in the part.

### What this refuses

`pmix` will not choose between "moved" and "removed and added". It states
where both things are and leaves the weighing to the consumer. Offering
nearest-candidate pairings with their distances is acceptable as
advisory data; presenting one as the conclusion is not.

## Alternatives considered

- **Author the narrative in `pmix`.** Rules for move, resize, and
  renumber. Rejected: the interesting cases are the undecidable ones, and
  phrasing a difference for a human is the part a language model does
  better than a rule table.
- **Leave comparison on identity alone.** That is the reported gap. Exact
  identity remains the fast path and the certain answer, but it cannot be
  the only one.
- **Hand the consumer raw B-rep.** Hundreds of faces per part with no
  vocabulary. The consumer would have to recognise features itself,
  without determinism and without a way to check it.
- **Fuzzy-match inside `diff` and report a confidence.** A number attached
  to a guess reads as a measurement. Distances and positions are facts;
  a confidence would not be.

## Consequences

- The features document is the input to comparison, so its parameters and
  its ids are load-bearing from the first version rather than additions
  later.
- Comparison of geometry needs a home — a section of `pmix diff` or a
  mode of `pmix features`. Deferred until the document exists.
- Determinism matters more, not less: a language model consuming a noisy
  document amplifies the noise into confident prose.
- This needs fixtures whose answer is known — one part exported twice
  with a single deliberate change (a diameter, a position, a hole
  removed), so that what the document says about the difference can be
  checked.
