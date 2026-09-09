# Synthetic fixtures

Hand-built STEP files, each written to make one behaviour checkable in
isolation. They are not exports of any real design and hold no
proprietary data: part numbers and values are invented.

## PMI and properties

| File | What it is for |
| --- | --- |
| `dimension_basics.stp` | Dimensions with tolerances, and the units they are stated in |
| `dimension_basics_inches.stp` | The same design in inches, which must key identically |
| `tolerance_datum_basics.stp` | Geometric tolerances, datums, and datum reference frames |
| `presentation_basics.stp` | Annotations, styles, and saved views |
| `property_basics.stp` | Named properties on a part |
| `assembly_properties.stp` | Components stating the same property names, which collide unless the part is part of the key |
| `datum_targets.stp` | A datum established by targets, none of which the file gives geometry |

## Feature recognition

One plate, and three single deliberate changes to it. Each is a question
people bring to a pair of CAD files, and the point of the feature
document is that answering it takes one field (ADR 0011, ADR 0012).

| File | How it differs from the baseline |
| --- | --- |
| `plate_one_hole.stp` | The baseline: a 40 × 30 × 10 plate with one Ø8 through hole |
| `plate_hole_larger.stp` | The hole is bored to Ø10. Nothing else changes |
| `plate_hole_moved.stp` | The hole moves 5mm in X. Nothing else changes |
| `plate_hole_split.stp` | The bore is written as two half-cylinders, which is how most exporters write one. The same hole, so the same id |
| `plate_one_hole_inches.stp` | The same design stated in inches. Nothing changes at all, and the documents must be equal |

The plate is deliberately the simplest solid that still exercises the
rules: the hole's mouth has to be an inner loop of the faces it opens
onto, or a through hole would read as blind, and the six planar sides
have to come out as unassigned, or the accounting promise would not be
tested by anything.
