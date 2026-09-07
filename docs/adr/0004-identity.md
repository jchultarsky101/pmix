# ADR 0004: Identity of records across exports

- **Status:** Proposed, 2026-09-07
- **Deciders:** Julian Chultarsky
- **Depends on:** [ADR 0002](0002-semantic-pmi-model.md), [ADR 0003](0003-presentation-pmi-model.md)

## Context

`pmix diff` must say "the position tolerance on this hole changed from
⌀0.1 to ⌀0.2", not "one tolerance vanished and another appeared". That
needs an `id` that survives re-export: the same design element in two
files must get the same id even when its value, its text, or every entity
number around it changed. ADR 0002 deferred the recipe and used a content
hash in the meantime, which is exactly wrong for this purpose: any change
to a record changes its id.

What the files offer as anchors, checked against the NIST corpus and the
four re-export pairs available (CTC 04, FTC 08, FTC 11, STC 09 in two
editions each):

| Candidate anchor | Finding |
| ---------------- | ------- |
| Entity numbers (`#1752`) | Renumbered on every export. Useless. |
| `id_attribute` values | Embed entity numbers (`SA #4088 on entity #4078`). Useless. |
| Vendor names (`Position.1`, `Feature Control Frame (49)`) | Persistent in some CAD systems, sequentially renumbered in others, absent in AP203. A hint at best. |
| B-rep geometry of the feature | Coordinates identical across the three edition re-labellings, and identical for the part body in the STC 09 re-export (median nearest-neighbour distance 0, differences confined to non-body geometry). Stable as long as the design is not moved or remodelled. |
| Datum letters | Design intent by definition. Stable unless the designer changes them, in which case a change *is* the news. |
| Saved view names (`MBD_A`) | Design intent. Stable. |
| Tolerance kind, dimension kind and subtype | Changing these is, from the designer's view, replacing the callout. Stable enough to anchor on. |

Two more facts shape the design. First, the units of two exports of the
same part can differ (the corpus mixes inch and millimetre files), and
absolute coordinates differ with them. Second, a part may carry two
identical callouts on the same feature, so identity cannot be assumed
unique.

## Decision

### Identity is separate from content

Every record's `id` is built from its **identity key**: the attributes
that name *which* design element it is. Everything else is **content** and
is compared by the diff. The rule for deciding which side an attribute
falls on: *if a designer edited this attribute, would they say they changed
the callout or replaced it?* Values, tolerances, modifiers, datum
references, text, placement, and geometry summaries are content. Kind,
the feature(s) a record applies to, datum letters, view names, and
composite structure are identity.

### Identity keys

| Record | Identity key | Content (examples) |
| ------ | ------------ | ------------------ |
| Feature | `kind` + sorted geometry fingerprints of its faces, edges, and vertices + sorted member ids; `name` only when there is no geometry and no members | name |
| Datum | `label` (+ owning product when the file has several) | features, targets |
| DatumSystem | ordered datum ids per compartment, `-` inside common compartments, `\|` between | modifiers, modifier values, text |
| Dimension | `kind` + `subtype` + feature ids in order + `directed` + path id | value, limits, tolerance, qualifier, modifiers, decimal places, text |
| GeometricTolerance | `kind` + feature ids + `composite_of` (the upper segment's id) | value, zone, modifiers, datum system, unequally disposed, maximum, unit basis, text |
| Note, Other | `kind` + feature ids + text | attributes |
| Annotation | if it links to semantic records: sorted semantic ids + `kind`; otherwise `kind` + plane placement rounded to the coarse quantum + geometry bounding box rounded to the coarse quantum | text, label, style, leaders, placeholder, exact plane, geometry summary |
| SavedView | `name` | camera, clipping, annotations, default |

### Geometry fingerprint

The anchor for features. Per B-rep entity, in model units, every number
rounded to the *identity quantum* `q`:

- **Face:** surface kind, then surface parameters that fix it in space:
  plane: unit normal and signed distance from the origin; cylinder and
  cone: axis direction, the axis point closest to the origin, radius and
  semi-angle; sphere: centre and radius; torus: centre, axis, both radii;
  swept and B-spline surfaces: none. Then the face's vertex set: count and
  bounding box of the `vertex_point`s reachable through its bounds.
- **Edge:** curve kind and both end points.
- **Vertex or point:** the point.
- **Curve set or other supplemental geometry:** bounding box.

Directions are sign-normalised so that a flipped face normal does not
change the key. The identity quantum is `max(file uncertainty, 1e-3 model
units)`; the coarse quantum used for unlinked annotations is `100 × q`.

Why not the face's full boundary? Vertex bounding boxes distinguish every
face in the corpus that shares a surface with another, at a fraction of
the cost, and the diff has a fallback (below) for the rare miss.

### Id form

`prefix:` followed by the identity key when it is short and printable, or
by its FNV-1a hash otherwise:

| Prefix | Readable form | Example |
| ------ | ------------- | ------- |
| `datum` | yes | `datum:A` |
| `dsys` | yes | `dsys:A\|B\|C`, `dsys:A-B\|C` |
| `view` | yes | `view:MBD_A` |
| `feat`, `dim`, `tol`, `note`, `ann` | hashed | `tol:3f9a1c0b7d2e6a48` |

### Collisions

Two records with the same identity key (two identical frames on one
feature, two saved views with one name) get the suffixes `-1`, `-2`, ...
in ascending order of their content hash, never in entity order. The diff
therefore matches duplicates consistently as long as their content is
unchanged, and reports duplicates whose content both changed as a removal
and an addition.

### Assignment

Ids are assigned in a finalisation pass after all walkers have run, in
dependency order: features, datums, datum systems, dimensions, tolerances
(upper segments before lower), notes, annotations, views. Walkers build
records with source entity references only; the pass computes keys, resolves
collisions, rewrites cross references, and sorts every array by id.

### What is out of scope here

- **Matching across unit systems.** An inch export and a millimetre export
  of the same part get different feature ids. The diff may normalise units
  before matching; that is its decision.
- **Moved or remodelled parts.** If the body is transformed, every feature
  id changes. A fallback that matches by kind, datum context, and relative
  position belongs to the diff, not to identity.
- **Assemblies.** Files with several product definition shapes are not yet
  handled by the readers; when they are, identity keys gain the owning
  product's identifier and datum ids become `datum:<product>/A`.
- **JT.** JT features reference XT B-rep faces. The fingerprint recipe
  applies once the JT reader parses that B-rep; until then JT features are
  keyed on their text and the reader marks their ids as text-derived.

## Alternatives considered

- **Content hashes (the interim scheme).** Deterministic and simple, but
  any edit produces a new id, so a diff degenerates into remove-plus-add.
- **Source entity numbers or `id_attribute`.** Renumbered on every export in
  every CAD system in the corpus.
- **Vendor names.** Would work for CATIA-style persistent names and fail
  for NX-style sequential labels; a scheme that works for some vendors and
  silently mismatches for others is worse than one that ignores names.
- **Full boundary geometry per face.** More discriminating than vertex
  bounding boxes but requires evaluating B-spline edges, and the corpus
  shows no case where the cheaper key collides.
- **Identity by position only** (round the annotation's plane origin).
  Works for presentation-only files and is kept as the fallback for
  unlinked annotations, but a designer who drags a callout would see it
  reported as replaced.

## Consequences

- The reader gains a finalisation pass and a geometry-fingerprint module;
  walkers stop assigning ids at build time. Every synthetic expectation is
  regenerated.
- `pmix diff` can rely on id equality as its primary match and treat the
  rest as content comparison. Its own ADR covers fallback matching, unit
  normalisation, and output.
- A new corpus test extracts each re-export pair and requires every
  semantic id from one edition to be present in the other, with the STC 09
  pair recorded as the baseline for how far a real re-export drifts.
- Users can read datum, datum system, and view ids directly in the JSON;
  the hashed ids remain opaque, and the `source_refs` field remains the
  way back to the file.
