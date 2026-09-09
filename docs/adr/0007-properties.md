# ADR 0007: Product and record properties

- **Status:** Accepted, 2026-09-07
- **Deciders:** Julian Chultarsky
- **Depends on:** [ADR 0002](0002-semantic-pmi-model.md), [ADR 0004](0004-identity.md), [ADR 0005](0005-diff.md)

## Context

STEP files carry named property values that are neither PMI nor geometry:
part numbers, revisions, lifecycle states, suppliers, prices, and computed
quantities such as volume and surface area. `pmix` was discarding all of
them, because ADR 0002's model has nowhere to put a value that is not a
dimension, a tolerance, a datum, or an annotation.

Two corpora show what exists, and they differ instructively:

| | 200-model set (OpenCascade 7.9 writer) | NIST corpus |
| --- | --- | --- |
| Attached to | `product_definition` only | `product_definition` (556), but also `geometric_tolerance` (142), `shape_aspect` (124), `dimensional_size` (108), `characterized_item_within_representation` (545) |
| Definition names | `PLM__Part_Number`, `PLM__Revision`, ... | `pmi validation property` (1017), `semantic text` (96), `geometric validation property` (37), `attribute validation property` (9), plus user names such as `Modeled By` |
| Value items | `descriptive_representation_item` only | descriptive (996), measure (1478), integer (865), real (365), value (5) |

So properties attach either to the whole part or to an individual PMI
record, their names live at two levels (the property definition names a
*category*, the representation item names the *property*), and their
values are typed.

The CAx-IF *validation properties* are a special family: a producing system
writes the affected area of a callout, the number of presentation elements
it drew, and the Unicode string equivalent to a feature control frame, so
that a consuming system can check it interpreted the file the same way.
They are derived from the PMI, not independent design data, and there are
about sixty per file in the NIST corpus.

## Decision

### A `properties` section

A top-level array, parallel to `semantic` and `presentation`, rather than a
field on every record. Properties attach to many different things, so one
collection keeps the document shape stable, gives the diff a single
collection to compare, and avoids adding a field to record types that
usually have no properties at all.

| Field | Type | Notes |
| ----- | ---- | ----- |
| `id` | string | Identity key, see below |
| `name` | string | The representation item's name, falling back to the representation's, then the property definition's |
| `category` | string? | The property definition's name when it differs from `name`: `pmi validation property`, `PLM__Part_Number`, ... |
| `kind` | enum | `user` or `validation` |
| `value` | `PropertyValue` | Tagged by `type`: `text`, `integer`, `number`, `measure` (with `unit`), `boolean` |
| `applies_to` | id? | The record the property is about; absent means the whole part |
| `unmapped`, `source_refs` | | As in ADR 0002 |

A single property definition whose representation lists several items
yields several properties, all sharing `category` and `applies_to`. That is
how the NIST validation properties are written and it is the honest
reading: three named values, not one.

`kind` is `validation` when the category is one of the CAx-IF validation
families (`pmi validation property`, `geometric validation property`,
`attribute validation property`), otherwise `user`.

### Identity

The identity key is `category`, `name`, and `applies_to`: what the property
*is* and what it is about. The value is content, so a revision going from A
to B is a change to one property, not a replacement
([ADR 0004](0004-identity.md)). Ids are readable when the key is simple and
the property is about the whole part (`prop:Part_Number`), hashed
otherwise. Collisions take the usual content-ordered suffixes.

### Diff

`properties` is compared like any other collection, with one exclusion:
**validation properties are not compared**. They are derived from the PMI
they describe, so a changed tolerance would report twice, and their
tessellation-dependent members (affected area, element counts) change on
every re-export. This follows ADR 0005's rule that the diff reports design
changes, not extraction artefacts. A geometry change with no PMI change is
therefore not reported; `pmix` is a PMI tool, and the README says so.

### Schema version

`schema_version` stays 1. The section is additive, and ADR 0002 bumps the
version only for changes that are not backwards compatible. The array is
always emitted, empty when the file has no properties, so a consumer can
tell an old document from a new one with no properties by the key's
presence.

### The part that states a property belongs to its identity

Added 2026-09-09. An assembly states the same property names of every
component — `Material`, `Mass (g)`, `Bbox Xmax (mm)` — so without the
part in the key they are one record with content-ordered suffixes. On a
105-component assembly, 3017 of 3117 properties carried such a suffix,
the largest group being 106 deep, which meant adding one component
reshuffled up to 106 ids.

A STEP property reaches its part two ways. One attached to a product
definition is about that product. One attached to a
`next_assembly_usage_occurrence` is about that component **as used
here**, and the occurrence's own name is what names it: two uses of one
product state the same values, so only the occurrence tells them apart.
The JT reader already scoped properties this way.

The field is left out of the key entirely when no part is named, so a
file describing one part keys exactly as it always did. An id reads
plainly as `prop:core.Part_Number` where both the part and the name
allow, and falls back to a hash where they do not.

### A JT key's trailing double colon is not part of the name

The specification (11.9.1.2) defines a trailing `::` as marking a
property **visible** to a viewing application, and its absence as
marking it hidden — a display hint, and explicitly not a security or
content distinction. It is stripped, so that marking a property visible
does not make it a different record, and so that a name a STEP file also
states is not carried with a JT decoration on it.

Keys are **case-sensitive** (11.9.1.3), so nothing is case-folded.

### What is deliberately not normalised

A JT writer may group keys with a prefix, as HOOPS Exchange does with
`Assembly Metadata/Analysis Software` where a STEP file puts the group
in `category`. The specification documents no such separator: it is one
converter's convention. Folding it into identity would let a converter's
naming habit silently change an id, so it is handled at comparison time
instead ([ADR 0005](0005-diff.md)) and never in a key.

## Alternatives considered

- **A `properties` map on every record.** Natural for the NIST validation
  properties, but most records have none, product-level properties would
  need a home anyway, and the diff would have to walk every collection.
- **Flatten each property definition into one record.** Loses the
  distinction between the three values in a NIST validation representation
  and forces an arbitrary choice of which name wins.
- **Keep only user properties and discard validation ones.** Contradicts
  the project's rule that recognised content is never dropped silently, and
  the validation properties are exactly what a conformance checker wants.
  They are captured and excluded from the diff instead.
- **Compare validation properties too.** Every re-export would report
  dozens of changes that restate PMI changes already reported.

## Consequences

- A `properties` walker runs after the PMI walkers, so it can attach
  properties to the records they built, and before the identity pass, which
  remaps `applies_to`.
- The datum-target and annotation walkers already read some property
  definitions (for target dimensions and for annotation text). Those remain
  where they are; the properties walker reports the same underlying values
  independently, because a validation string is both the annotation's text
  and a property in its own right.
- Unit resolution gains `derived_unit`, because the geometric validation
  properties are areas and volumes; without it they would be reported with
  an empty unit.
- `docs/test-data.md` documents the ignored `data/` directory, where local
  model sets can be kept for exploring the readers by hand.

## Where JT properties come from

Added 2026-09-08. A JT file states properties in two places, and the
reader takes both.

The **scene graph's property table** holds what the structure needs: the
model units, the name and unique identifier of each node, layer numbers.

The **metadata segments** hold what a part says about itself, as property
proxy elements (specification section 8.2): its material, density,
volume, mass units, and Young's modulus, its name, and the system that
wrote it. These are the properties an engineer would recognise as design
data, and they were the reason to open those segments at all.

Both are typed by this ADR's rules. A value the file types as a number
or a date keeps that type; a value the file types as text goes through
the coercion of [ADR 0008](0008-numeric-property-values.md), so a volume
written as `842.167` compares as a number while a density written as
`7.73e-06` stays text, its exponent being the kind of formatting that
rule deliberately preserves.

Properties that describe how the file was written rather than what was
designed, such as the translator version and the level-of-detail
settings, are marked as validation properties and left out of the diff.

### Which part states a property

Added 2026-09-08. A property carries the part that states it, in the
`part` field, so an assembly's materials and volumes can be told apart.

A part names itself on its scene-graph node, and the scene graph's
property table says which node points at which segment, through the late
loaded property atoms that make late loading possible in the first
place. That resolves every segment in the test file to a node.

It does not resolve every segment to a *name*, because the node that
owns a segment is often a child of the part rather than the part itself,
and children are unnamed. A metadata segment also states its own name,
and the two sources together name all thirty in the test file: nine from
the node, twenty-one from the segment.

The part is part of a property's identity, so two parts made of
different materials are two records rather than one that collides. A
part saying the same thing twice is still recorded once.

The reader does not walk the scene graph's parent and child links. Doing
so would name the remaining nodes directly rather than through the
segment, and is what the semantic records will need when assemblies are
modelled (ADR 0004). It is not needed for properties.
