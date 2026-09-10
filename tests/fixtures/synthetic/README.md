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

## Product structure

The committed public corpus holds no assemblies at all: not one of the 17
NIST files states a `NEXT_ASSEMBLY_USAGE_OCCURRENCE`, so nothing there
exercises the product reader (ADR 0014). These two files carry that
coverage on their own. Each states a full advanced B-rep for every
component, so the same files serve the body-to-part join.

| File | What it is for |
| --- | --- |
| `assembly_two_parts.stp` | Two different parts, each placed once, at different revisions. The simplest bill of materials that is not a single part |
| `assembly_repeated_part.stp` | One part used three times at three placements. A reader that collapses occurrences reports one pin here instead of three, which is the failure this file exists to catch |

Both are written by hand rather than exported, which means they encode
what the reader expects rather than confirming it against a real writer.
Verify product-structure changes against real exports as well.

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

Comparing two documents needs more than one hole, so that a body having
moved can be told from a hole having moved:

| File | What it is for |
| --- | --- |
| `plate_four_holes.stp` | Four mounting holes; the baseline for comparisons |
| `plate_four_holes_shifted.stp` | The whole plate moved 5mm in X. Nothing keeps its id and one displacement explains all four |
| `plate_four_holes_one_moved.stp` | One hole moved 5mm. Three holes keep their ids, which is proof the body did not move |

Blends and chamfers need shapes with corners rather than holes:

| File | What it is for |
| --- | --- |
| `plate_corner_round.stp` | A plate with one outside corner rounded at R5, tangent to both faces it joins |
| `plate_inside_fillet.stp` | An L-shaped plate whose inside corner is filleted at R5. The same blend, the other way round |
| `shaft_chamfered.stp` | A bar with its top edge chamfered at 45°. The cone sits beside a shaft, not a bore, which is what makes it a chamfer and not a countersink |

The plate is deliberately the simplest solid that still exercises the
rules: the hole's mouth has to be an inner loop of the faces it opens
onto, or a through hole would read as blind, and the six planar sides
have to come out as unassigned, or the accounting promise would not be
tested by anything.
