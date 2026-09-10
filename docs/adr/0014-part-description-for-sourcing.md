# ADR 0014: Describing a part well enough to source a substitute

- **Status:** Proposed, 2026-09-10
- **Deciders:** Julian Chultarsky
- **Depends on:** [ADR 0007](0007-properties.md), [ADR 0011](0011-feature-recognition.md), [ADR 0012](0012-geometric-comparison.md), [ADR 0013](0013-mcp-server.md)

## Context

ADR 0013 gave a language model a way to ask what a file says and what a
shape is. The question it was built for was comparison: what changed
between two revisions. A second question has arrived that the same
transport should answer — **given this assembly, find a viable substitute
for it or for any of its components** — and the documents behind the
transport do not hold enough to answer it.

Sourcing is not a geometry question. It is an identity question with a
geometric constraint attached: *this part is a 40 × 20 × 6 mm stamped
steel bracket, part number 12345 rev C, with four Ø5.5 holes on a 30 mm
square pattern — what else is that?* `pmix` today can state the four
holes. It can state nothing else in that sentence.

### What is missing, precisely

**Who the part is.** `FeatureDocument.bodies` is a flat list. A body's
`name` is taken from the name on its shell or its surface
([`src/features/step.rs`](../../src/features/step.rs)), which is an
accident of the exporter, not a designation anybody would search for.
Product identity *is* read — `PRODUCT`, `PRODUCT_DEFINITION`,
`PRODUCT_DEFINITION_FORMATION` all appear in the property reader — but
only far enough to attribute a property to a part (ADR 0007). The
assembly structure itself, `NEXT_ASSEMBLY_USAGE_OCCURRENCE` and the
per-occurrence transform, is not read at all, and neither is JT's
equivalent in the LSG. So there is no bill of materials: no list of
unique parts, no quantities, no where-used, and no join from a body back
to the part it realises.

**How big it is.** No envelope, no volume, no surface area, no mass. A
feature states a Ø4.5 hole 12 mm deep; nothing states how big the thing
holding the hole is. Size is the first filter in any catalogue search and
the document cannot supply it.

**What it is made of.** No material, in either document. AP242 can state
one properly; most exporters instead write a property called `MATERIAL`
or `MATNR` or `Werkstoff`, which the property reader carries through
undistinguished among a hundred others.

**Whether a candidate would fit.** This one is derived rather than read.
Four holes are stated individually; that they form a 30 mm square pattern
is arithmetic nobody does. ADR 0013 put naming a bolt circle on the
model's side of the line, and for *interpretation* that is right — but a
model asked to search the web for a bracket needs the pattern as a datum,
not as something to re-derive from four position vectors every time it is
asked.

### The failure this invites

ADR 0012 recorded that a model consuming a noisy document turns the noise
into confident prose. A model consuming a document with a *hole* in it
does something worse: it fills the hole. Asked to source a part it has
been given a shape for and no identity, it will produce a plausible part
number. Every addition below is therefore also a defence — the way to
stop a model inventing an identity is to give it the real one, or to make
the absence of one explicit.

### The risk this creates

This workflow ends with an identifier leaving the machine. A part number,
a material spec, and an envelope typed into a web search are a
disclosure, and the models this project is aimed at are customer models.
That is a new class of consequence for a tool that until now only read
files, and the decision below has to account for it rather than leave it
to whoever wires the model up.

## Decision

Four stages, in the order a sourcing question needs them. Each stage is
useful on its own and none depends on the next; the order is by how much
of the question it unblocks, not by convenience.

Three principles hold across all of them.

**A document holds what a file says; anything derived says that it is
derived.** ADR 0011 drew this line for geometry and it holds here.
A material name read from a property is data. A bolt circle computed from
four hole axes is a *recognised pattern*, named as such, in the same
posture as a recognised feature: stated with the rule that produced it,
never mixed in among things the file declared.

**Product structure gets its own document.** The PMI document and the
features document are separate on purpose, because what a file *says* and
what a shape *is* are different questions with different schemas that
move at different rates (ADR 0011). A bill of materials is a third
question — what the file *contains* — and belongs in a third document
with its own schema version. The join between the three is by part id, and
supplying that join is itself part of the work: today a body cannot be
traced to a product at all.

**What is absent is stated.** A part with no material property says so
with an explicit absence, not by omitting a field. `unassigned` in the
features document is the precedent: *not recognised* must never be
readable as *not there*, and *not stated by the file* must never be
readable as *unconstrained*.

### Stage 1 — identity and structure

A product document holding, per part: the product id, name and
description; the revision from `PRODUCT_DEFINITION_FORMATION`; the
occurrences of that part in the assembly, each with its own occurrence
name and placement; the quantity; and the ids of the bodies that realise
it. The tree is stated as parent/child relations, not as a nested
structure, so that a shared subassembly appears once and is referenced
twice.

Also read here, because they are identity and they are cheap once the
product entities are being walked: the owning organisation and the
designer (`PERSON_AND_ORGANIZATION`), approval state and date
(`APPROVAL`), and the security classification (`SECURITY_CLASSIFICATION`).
The last of these is not decoration — see *Disclosure*, below.

This stage alone changes what the question can be. "Find substitutes for
this assembly" becomes a list of parts with names and quantities rather
than an undifferentiated pile of bodies.

### Stage 2 — size, mass, and material

Per body and per part, and once for the assembly:

- **Envelope.** The axis-aligned bounding box in model coordinates, and a
  minimal box giving overall length × width × height sorted descending —
  the three numbers a catalogue asks for. The axis-aligned box alone is
  not enough, because it describes how the part happened to be oriented
  when it was exported.
- **Volume and surface area**, computed from the B-rep. Stated as
  measured facts.
- **Mass**, *only* when the file states a mass or a density. A mass
  inferred from a volume and a guessed density is a fabrication with a
  unit attached, and this project does not emit those. Volume is stated;
  the model may reason from it if it wants to, in the open.
- **Centroid**, and principal axes where the computation is already paid
  for by the volume integral.
- **Material**, promoted to a named field from wherever it came —
  AP242's material designation where present, otherwise a property whose
  key matches a known alias — with the raw key and raw string always
  retained beside it, because the alias table will be wrong.

That last clause generalises to the whole stage. Alias tables built
against `tests/fixtures/nist` will match nothing on a real Creo or NX or
CATIA export; this project has already learned that lesson once with
geometry rules. So promotion is additive and never lossy: a promoted
field carries its provenance, and a key that matched nothing stays
exactly where it was, visible.

A curated identity block follows the same rule — `part_number`,
`revision`, `description`, `material`, `mass`, `finish`, `standard`,
`supplier`, `manufacturer_part_number`, each naming the raw key it was
promoted from.

### Stage 3 — interface and class

Two recognitions, both derived, both stated as recognitions.

**Hole patterns.** Group holes by diameter and by the face they enter,
then recognise bolt circles (count, pitch circle diameter, angular
clocking of the first hole) and linear and rectangular patterns (counts
and pitches). Everything this needs — each hole's axis, position,
diameter and `through` flag — is already in the features document; the
work is arithmetic and a rule about what counts as a pattern. This is the
single most useful derived item for sourcing, because *will it bolt where
the old one bolted* is the question a substitute has to pass.

**Threads.** Read AP242 thread PMI where a file states it, and parse
designations (`M6x1`, `1/4-20 UNC`, class `6H`/`6g`) out of notes and
dimension texts. Where neither exists, a hole whose diameter matches a
tap-drill size may be flagged as *possibly tapped* — an observation, in
the posture ADR 0012 established for `possible_pairings`: named so that
it cannot be mistaken for a conclusion, and carrying what it rests on.

**Shape class** rounds the stage out: solid of revolution, constant-wall
shell with bends, prismatic, or free-form, together with a surface
inventory (counts per surface kind, largest planar face area) that is
half-present already in `UnassignedFace.surface`. A class is what gives a
model the vocabulary to search with — "stamped steel bracket" rather than
"a body with 47 faces" — and for sheet metal it also yields thickness and
bend radii, which are themselves catalogue terms.

### Stage 4 — the rest of the sentence

Smaller items, each cheap once the stages above exist:

- **Surface finish and treatment**: AP242 surface texture requirements,
  and finish and plating callouts in notes.
- **Fit-critical dimensions**: a ranking of dimensions by tolerance
  tightness, and any ISO fit codes. A substitute has to hold *these*, and
  the current document states all tolerances as equals.

### What stays out

- **Colour, appearance, and layers.** Occasionally they encode a
  subassembly. Mostly they encode a viewing preference, and a sourcing
  answer built on one would be built on sand.
- **Kinematics.** AP242 XML carries it; nothing in the sourcing question
  asks for it yet.
- **A density table.** See *mass*, above.
- **Searching, and judging a candidate.** See *What the model is for*.

### What the model is for

There is more on each side of the boundary ADR 0013 drew, and the
boundary itself sits where it sat. `pmix` supplies data; every judgment
in a sourcing answer stays on the model's side.

| the model does | because |
| --- | --- |
| recognises "M6×1, 20 long, socket head" as a DIN 912 / ISO 4762 | that is catalogue knowledge, not anything in the file |
| decides 6061-T6 is an acceptable stand-in for 6082-T6 here | it depends on the application, which the file does not state |
| searches, and reads what it finds | `pmix` reads files and nothing else (ADR 0013) |
| decides a candidate fits | `pmix` states the envelope and the bolt pattern; whether the difference matters is engineering judgment |
| notices that the file states no material, and calls that a gap | the document states the absence; deciding it is disqualifying is a judgment |

`pmix` gains no network access and no catalogue. It is the source, and it
does not become an opinion.

### Disclosure

Because this workflow's natural end is an identifier in a web search, two
things follow.

The security classification read in stage 1 is **stated prominently in
every document that carries a part number**, so that a model cannot see
the identifier without also seeing the marking. A tool that hands out a
proprietary part number in a field called `part_number` and buries
"proprietary" among ninety other properties has helped leak it.

And the MCP server's `instructions` — the paragraph a client shows a
model before it picks a tool — says plainly that these documents may
describe confidential designs and that identifiers from them are not to
be sent anywhere without the user asking for it. ADR 0013 already used
that field to carry a caution that a transport must not be able to strip.
This is the second such caution and it belongs in the same place.

## Alternatives considered

- **Bolt it all onto the features document.** Simplest, and wrong: a bill
  of materials is not a description of a shape, it would tie the
  schema version of the geometry to the schema version of the product
  structure, and the two will not change together.
- **Emit a natural-language part description directly** — one paragraph
  per part, ready to search with. Tempting, and it makes `pmix` an
  opinion (ADR 0013). The prose would also be untestable in the way the
  numbers are not.
- **Infer mass from a density table keyed on the material string.**
  Would make almost every part state a mass. Also makes `pmix` state a
  number no file said, derived from a string match, in kilograms. No.
- **Let the model derive bolt patterns from hole positions.** It can, and
  it does so afresh on every question, sometimes differently. A pattern
  is a local geometric fact given the holes; that puts it on this side of
  the line drawn in ADR 0011.
- **Do stage 2 first**, since envelopes are easy and identity is not.
  Rejected: an envelope attached to an anonymous body is not searchable.
  Identity is what makes the rest of it addressable.

## Consequences

- **A third document to produce, and a fourth schema version to track.**
  PMI, features, and comparison already move independently (ADR 0013).
  One more version is the price of not tying the geometry schema to the
  product schema, and the two will not change together.
- **Bodies gain a join to products**, which they do not have today. This
  is the part of stage 1 with real reading work in it, in both readers,
  and it is what the whole plan rests on.
- **The MCP server gains a tool.** `describe_model` is deliberately
  geometry-only and should stay that way; a sourcing question wants
  identity, envelope, material and interface for one part in a single
  call. That is a new tool, shaped like the existing ones: narrow,
  filtered, and computing nothing the library does not.
- **Derived data enlarges the surface on which `pmix` can be wrong.** A
  misrecognised bolt circle is a sentence a model will repeat. Each
  recognition therefore states its rule and its inputs, as features
  already do, and a pattern that a rule cannot settle is left unstated
  rather than guessed.
- **The alias tables will be incomplete on real files**, permanently.
  Designing promotion as additive and lossless is what keeps that a
  degradation rather than a defect: an unmatched key still reaches the
  model, just unpromoted.
- **The JT reader has to answer the same questions.** Product structure
  lives in the LSG, materials in part attributes. A JT exported without
  precise geometry can still supply stage 1 identity, which makes it more
  useful for sourcing than it is for comparison.
- **This is the first ADR whose subject is something the model does with
  the data outside this machine.** The disclosure clause exists because
  of that, and it will not be the last time this project has to decide
  what it says to a consumer it does not control.
