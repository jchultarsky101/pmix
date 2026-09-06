# ADR 0003: Presentation layer of the PMI data model

- **Status:** Proposed, 2026-09-06
- **Deciders:** Julian Chultarsky
- **Depends on:** [ADR 0001](0001-step-parsing-strategy.md), [ADR 0002](0002-semantic-pmi-model.md)

## Context

ADR 0002 defined the semantic layer and left the *presentation* layer, the
PMI a human sees, to this record. The presentation layer matters for three
reasons:

1. **It is the only PMI in many files.** Every AP203 export, older AP214
   exports, tessellated-geometry exports such as NIST FTC 08, and JT files
   through version 10.0 carry graphical annotations only. Without this
   layer `pmix` would report nothing for them.
2. **It anchors the semantic layer.** AP242 links each semantic record to
   the callout that displays it through
   `draughting_model_item_association`. Those links fill the
   `presentation` field that every semantic record already carries and are
   how a reader knows *which* frame on screen a tolerance record is.
3. **Saved views are design intent.** Which annotations are shown together,
   from which direction, is part of the model-based definition.

What the NIST corpus (17 AP242 files, all five NIST test series) actually
contains, as counted on 2026-09-06:

| Construct | Count | Notes |
| --------- | ----- | ----- |
| `draughting_callout` | 840 | one per displayed annotation; groups its occurrences |
| `tessellated_annotation_occurrence` | 778 | glyphs and lines as triangles and polylines; dominant form |
| `annotation_placeholder_occurrence[_with_leader_line]` | 355 | position, box, and leader points, no glyphs |
| `annotation_curve_occurrence` | 22 | classic polyline form, mostly supplemental geometry |
| `annotation_plane` | 693 | almost one per callout |
| `draughting_model_item_association[_with_placeholder]` | 2,145 | semantic-to-presentation links, 661 named "PMI representation to presentation link" |
| `camera_model_d3` / `model_geometric_view` | 130 / 52 | saved views: `MBD_A`, `Front`, `Isometric`, ... |
| `coordinates_list` | 5,625 | tessellation data; thousands of points per file |
| `descriptive_representation_item('equivalent unicode string', ...)` | 284 in 4 files | the only explicit annotation text in the corpus |
| `styled_item` / `colour_rgb` / `presentation_layer_assignment` | 8,098 / 316 / 173 | styling |

Two facts shape the design. First, **annotation text is almost never
explicit in STEP**: glyphs are tessellated triangles, and only one CAD
system in the corpus writes the equivalent Unicode string. Second,
**tessellation is both huge and unstable**: every re-export re-tessellates,
so raw coordinates would dominate a diff with noise while carrying no
design change.

## Decision

### Records

```text
presentation
├── annotations[]   Annotation
└── views[]         SavedView
```

Annotation planes are not a separate list. In the corpus they are nearly
one-to-one with callouts, and in JT every annotation carries its own 2D
frame, so the plane is inlined as a placement on each annotation.

**Annotation**: one displayed callout. In STEP that is a
`draughting_callout` and its contents; an occurrence outside any callout is
its own annotation.

| Field | Type | Notes |
| ----- | ---- | ----- |
| `id` | string | Content-derived (see below) |
| `kind` | enum | The presented PMI type, from the rec. practice table 18 name on the tessellated or curve set: `position`, `flatness`, ..., `linear_dimension`, `diameter_dimension`, `radial_dimension`, `angular_dimension`, `general_dimension`, `datum`, `datum_target`, `label`, `note`, `surface_finish`, `weld`, ... Unrecognised names kept verbatim. JT maps its type codes onto the same names. |
| `label` | string? | The vendor's callout name, e.g. `Position.1`. Kept for humans; changes between exports, so the diff ignores it by default. |
| `text` | string? | The annotation's text |
| `text_origin` | enum? | `explicit` when the file carries the string, `semantic` when rendered from the linked semantic record, absent when no text is known |
| `plane` | `Placement?` | The annotation plane (`annotation_plane`, or the JT 2D frame) |
| `placeholder` | `{placement, box: [width, height]?, role: string?, text_height: Measure?}?` | From `annotation_placeholder_occurrence` |
| `leaders` | `Leader[]` | `{points: [[x, y, z]], terminator: string?}` from `annotation_to_model_leader_line` and `auxiliary_leader_line` |
| `geometry` | `GeometrySummary` | See below |
| `parts` | `Part[]` | One per occurrence in the callout: `{form: tessellated\|polyline\|placeholder\|fill_area\|text, kind, geometry: GeometrySummary}` |
| `style` | `{colour: string?, line_font: string?, line_width: Measure?, layer: string?}?` | From `styled_item`, `curve_style`, `colour_rgb` as `#rrggbb` or the predefined name, `presentation_layer_assignment` |
| `semantic` | id[] | Semantic records this annotation displays, from `draughting_model_item_association` (JT: PMI associations) |
| `features` | id[] | Features this annotation is attached to, from associations whose definition is a shape aspect, and from `characterized_item_within_representation` |
| `views` | id[] | Saved views this annotation is part of |
| `attributes` | map | Vendor properties with no home elsewhere (JT's PMI property bag). Empty for STEP. |
| `unmapped`, `source_refs` | | As in ADR 0002 |

The link is symmetric: every semantic record's `presentation` field lists
the annotations that display it.

**GeometrySummary** is the default representation of graphical geometry:

| Field | Type | Notes |
| ----- | ---- | ----- |
| `polylines` | integer | Number of polylines |
| `triangles` | integer | Number of triangles |
| `points` | integer | Total distinct coordinates |
| `bbox` | `{min: [x, y, z], max: [x, y, z]}?` | Axis-aligned, in model units |
| `hash` | string | Content hash of all coordinates, rounded to the file's uncertainty (or 1e-6 in model units), so that identical geometry hashes identically across exports and any change is detected |

Full coordinates are emitted only with `pmix extract --presentation-geometry`,
which adds `polylines: [[[x, y, z], ...]]` and `triangles: [[i, j, k], ...]`
with a `vertices` array to each part. This keeps the default document small
and the diff readable while nothing is lost on request.

**SavedView**: a `camera_model_d3` and what it shows.

| Field | Type | Notes |
| ----- | ---- | ----- |
| `id` | string | |
| `name` | string | `MBD_A`, `Front`, ... |
| `camera` | `{placement: Placement, projection: parallel\|perspective, view_plane_distance: number?, view_window: [w, h]?, front_clip: number?, back_clip: number?}` | From `camera_model_d3.view_reference_system` and `view_volume` |
| `clipping_planes` | `Placement[]` | From `camera_model_d3_multi_clipping` and its union/intersection variants; the boolean structure is kept in `unmapped` until needed |
| `annotations` | id[] | Annotations visible in the view, from the `draughting_model` the `model_geometric_view` pairs with the camera, following `mapped_item` to the annotation groups it includes |
| `default` | bool | `default_model_geometric_view` |
| `unmapped`, `source_refs` | | |

### Text

Text is filled in this order and the origin recorded:

1. An explicit string in the file: the `equivalent unicode string`
   descriptive item attached to the callout's shape aspect, a
   `text_literal` in an `annotation_text_occurrence`, or JT's string table.
2. Otherwise, when the annotation links to exactly one semantic record, the
   record's rendered text (the `text` field ADR 0002 already defines for
   dimensions and tolerances, the label for datums and targets).
3. Otherwise none. A `label` or `note` annotation whose text exists only as
   tessellated glyphs is reported with `kind`, geometry summary, and no
   text. Recognising glyphs is out of scope; the summary hash still detects
   a change.

### Presentation-only files

When the file has no semantic PMI, the semantic layer stays empty and this
layer carries everything. Deriving semantic records from annotation text
(`origin: text` in ADR 0002) is a later phase and a separate decision; this
record only guarantees the text is captured when it exists.

### STEP AP242 mapping

| AP242 entities | Record / field |
| -------------- | -------------- |
| `draughting_callout`, `draughting_callout_relationship` | `Annotation`; related callouts merge into one annotation with several `parts` |
| `tessellated_annotation_occurrence` → `tessellated_geometric_set` (+ `repositioned_tessellated_item`) → `tessellated_curve_set`, `complex_triangulated_surface_set`, `coordinates_list` | `parts[].form = tessellated`, `kind` from the set name, geometry |
| `annotation_curve_occurrence` → `geometric_curve_set` → `polyline`, `circle`, `trimmed_curve`, `composite_curve` | `parts[].form = polyline`, geometry (curves sampled as polylines with the segment count recorded in `unmapped`) |
| `annotation_fill_area_occurrence` | `parts[].form = fill_area` |
| `annotation_placeholder_occurrence`, `_with_leader_line`, `planar_box`, `annotation_to_model_leader_line`, `auxiliary_leader_line`, `apll_point` | `placeholder`, `leaders` |
| `annotation_text_occurrence`, `text_literal`, `composite_text` | `text` (explicit), `parts[].form = text` |
| `annotation_plane` | `plane` |
| `draughting_model_item_association`, `_with_placeholder` | `semantic`, `features`, and the reverse `presentation` links |
| `characterized_item_within_representation` | `features` |
| `descriptive_representation_item('equivalent unicode string')` via `property_definition_representation` | `text` (explicit) |
| `styled_item`, `over_riding_styled_item`, `presentation_style_assignment`, `curve_style`, `fill_area_style_colour`, `colour_rgb`, `draughting_pre_defined_colour`, `presentation_layer_assignment` | `style` |
| `camera_model_d3`, `camera_model_d3_multi_clipping[_union\|_intersection]`, `view_volume`, `model_geometric_view`, `default_model_geometric_view`, `camera_usage`, the per-view `draughting_model`, `mapped_item`, `representation_map` | `SavedView` |
| global `draughting_model`, `mechanical_design_geometric_presentation_representation` | Membership only; not a record |

### JT mapping (forward-looking)

A JT PMI entity is already shaped like an `Annotation`: its type code gives
`kind`; its string table entries give `text` with `text_origin: explicit`;
its 2D reference frame gives `plane`; its text and non-text polylines give
`geometry`, transformed from plane coordinates to model space; its
associations give `semantic` and `features`; model views give `views`; its
property bag goes to `attributes`. Nothing in the record is STEP-specific.

### Identity

Interim, as in ADR 0002: a content hash of `kind`, `text`, the plane
placement rounded to the file's uncertainty, the linked semantic ids, and
the geometry hash, with ordinal suffixes on collision. The identity ADR will
refine the recipe; annotations attached to semantic records will inherit
stability from them.

### Unknown content

Unconsumed callouts, occurrences, planes, cameras, and views are reported
in `unknown` with `layer: presentation`, as ADR 0002 requires. Styling
entities are not PMI content and are not reported when unused.

## Alternatives considered

- **Always emit full coordinates.** Lossless but unusable: tens of
  thousands of triples per file that change at every re-export. Kept as an
  opt-in flag instead.
- **Emit no geometry at all.** Small and stable, but a presentation-only
  file would then carry nothing comparable, and a moved or resized
  annotation would be invisible. The summary with a hash detects change at
  negligible size.
- **Recover text from glyph tessellation.** Would make labels and notes
  comparable in the many files with no explicit text, but it is a
  recognition problem, not an extraction one. Left for a future decision;
  the geometry hash and the semantic-rendered text cover the common cases.
- **A separate `planes` list with annotations referencing it.** Closer to
  the AP242 structure, but the corpus shows planes are nearly one per
  callout, JT has no shared planes, and inlining removes an indirection
  from every consumer.
- **Treat each occurrence as its own annotation.** Simpler to extract, but
  a feature control frame and its leader would then be two records that a
  human sees as one; the callout is the unit of display.

## Consequences

- The presentation walkers fill `annotations` and `views` and populate the
  `presentation` field on semantic records; the empty `presentation`
  placeholder in the current document skeleton is replaced by this shape.
- A new `--presentation-geometry` flag on `pmix extract`.
- The corpus test gains: every `draughting_callout` becomes an annotation,
  every association is consumed, every camera becomes a view, and the
  number of semantic records with a non-empty `presentation` matches the
  number of "PMI representation to presentation link" associations.
- Synthetic fixtures: one tessellated callout linked to a tolerance, one
  placeholder with leader line linked to a datum, one polyline annotation,
  one saved view containing them, plus the explicit-text case.
- FTC 08 (tessellated geometry, graphical PMI only) becomes a meaningful
  test: its output should list every annotation with kind and geometry
  summary and an empty semantic layer.
