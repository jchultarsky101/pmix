# ADR 0002: Semantic layer of the PMI data model

- **Status:** Accepted, 2026-09-06
- **Deciders:** Julian Chultarsky
- **Depends on:** [ADR 0001](0001-step-parsing-strategy.md)

## Context

ADR 0001 committed to a two-layer intermediate model: a *presentation* layer
(what a human sees: callouts, text, polylines, planes, views) and a
*semantic* layer (what a machine can reason about: dimensions, tolerances,
datums, and the features they apply to). Both STEP and JT readers must
produce it, and `pmix diff` must compare it across exports. This record
defines the semantic layer and the document skeleton it lives in. The
presentation layer and the identity scheme get their own records.

What the model has to absorb:

- **STEP AP242** encodes semantic PMI as an entity graph following the
  CAx-IF *Recommended Practices for the Representation and Presentation of
  PMI (AP242)* v4.1. Values are measures with units and qualifiers, tolerance
  types are fifteen entity subtypes combined with "mix-in" supertypes in
  complex instances, datum reference frames are ordered compartments with
  per-datum modifiers, and features are `shape_aspect`s linked to B-rep faces.
- **JT** (through 10.0) stores PMI as typed 2D annotations: a kind code, text
  strings, polylines, an annotation plane, associations to B-rep entities,
  and a flat key/value property bag. Structured values are often only
  recoverable from the text. JT 10.5 and later add more semantics; the
  specification is public but not yet studied.
- **OpenCascade's** `XCAFDimTolObjects` is the most mature existing
  normalised model of AP242 PMI. Its enumerations are borrowed here; its
  silent dropping of unmapped content is not.
- **Diffing** needs records that are self-describing, deterministically
  ordered, and free of anything that changes between two exports of the same
  design (entity numbers, file paths, timestamps).

## Decision

### Document skeleton

```text
PmiDocument
├── schema_version            integer, bumped on incompatible change
├── source                    file name, format, schema, writer, timestamp
├── units                     length and angle units declared by the file
├── semantic                  this record
│   ├── features[]
│   ├── datums[]
│   ├── datum_systems[]
│   ├── dimensions[]
│   ├── tolerances[]
│   ├── notes[]
│   └── other[]
├── presentation              separate record; callouts, planes, views
├── unknown[]                 recognised but unmapped content, never dropped
└── diagnostics[]             reader warnings
```

`source` and `diagnostics` describe the extraction, not the design, and are
excluded from comparison by default.

### Rules that apply to every semantic record

1. **`id`**: an opaque string, unique within the document, derived from the
   record's content rather than from source entity numbers. Cross references
   between records use ids. The derivation recipe, including how feature
   identity survives re-export, is deferred to the identity ADR. Until then,
   readers use a content hash of the record with `id`, `source_refs`, and
   `presentation` fields excluded, with an ordinal suffix on collision.
2. **`source_refs`**: the source entities the record came from, as strings
   such as `"#1752"` for STEP or a segment/element path for JT. Present for
   debugging and traceability, excluded from comparison.
3. **`origin`**: `semantic` when the structured fields came from semantic
   entities, `text` when they were inferred from annotation text (the normal
   case for JT and for STEP files with presentation-only PMI). Lets a diff
   distinguish "the value changed" from "the confidence changed".
4. **`presentation`**: ids of the presentation-layer annotations that display
   this record, from `draughting_model_item_association` in STEP or PMI
   associations in JT. Empty when the link is absent.
5. **`unmapped`**: attributes the reader recognised but could not interpret,
   each as `{attribute, raw}`. A record with an unrecognised modifier keeps
   the modifier here rather than losing it.
6. **Enumerations serialise as strings.** Known values use the canonical
   snake_case names listed below; an unrecognised source value is kept
   verbatim, so nothing is normalised into "other" without the original
   being visible.
7. **Measures are never converted.** A `Measure` is `{value, unit}` in the
   unit the file declares (`mm`, `in`, `deg`, `rad`, ...). Comparison across
   unit systems is the diff's problem, with the unit explicit in both inputs.
   Decimal places, when the file states them, are carried as
   `decimal_places` so formatting intent is not lost.
8. **Ordering is deterministic:** every array sorted by `id`, object keys in
   declaration order, no dependence on source entity numbering.

### Records

**Feature**: a portion of the part that PMI applies to, the AP242
`shape_aspect` and the JT association target.

| Field | Type | Notes |
| ----- | ---- | ----- |
| `kind` | enum | `face`, `edge`, `vertex`, `axis`, `center_plane`, `center_point`, `center`, `apex`, `tangent`, `derived`, `all_around`, `between`, `composite` (AP242 `composite_shape_aspect`), `composite_group` (`composite_group_shape_aspect`), `parallel_offset`, `geometric_alignment`, `perpendicular_to`, `datum_target_area`, `mixed`, `all_over` (the whole part, when a tolerance targets the `product_definition_shape`) |
| `name` | string? | `shape_aspect.name` when meaningful |
| `geometry` | `GeometryRef[]` | The B-rep entities: `{kind: face\|edge\|vertex, surface: plane\|cylinder\|cone\|sphere\|torus\|bspline\|other, source_ref}` from `geometric_item_specific_usage` and `item_identified_representation_usage`. The geometry fingerprint used for identity is deferred. |
| `members` | id[] | Child features of a composite (AP242 `composite_shape_aspect`, `all_around_shape_aspect`, `between_shape_aspect`) |
| `count` | integer? | Pattern count for `n×` features when stated |

**Datum** and **DatumTarget**.

| Field | Type | Notes |
| ----- | ---- | ----- |
| `label` | string | The letter, from `datum.identification` |
| `features` | id[] | `datum_feature` shape aspects, via `shape_aspect_relationship` |
| `targets` | `DatumTarget[]` | `{label: "A1", kind: point\|line\|rectangle\|circle\|circular_line\|area, diameter: Measure?, length: Measure?, width: Measure?, placement: Placement?, feature: id?, unmapped}` from `placed_datum_target_feature`, `datum_target`, `feature_for_datum_target_relationship`, and `shape_representation_with_parameters` |

**DatumSystem**: an ordered datum reference frame.

| Field | Type | Notes |
| ----- | ---- | ----- |
| `compartments` | `Compartment[]` | In precedence order. `Compartment = {datums: DatumRef[], common: bool}`; `common` is true for `A-B` style common datums. `DatumRef = {datum: id, modifiers: DatumModifier[], modifier_values: Measure[]}` |
| `text` | string | Canonical rendering such as `A\|B(M)\|C`, for humans and for text-origin readers |

`DatumModifier`: `maximum_material`, `least_material`, `regardless_of_size`,
`basic`, `translation`, `projected`, `orientation`, `point`, `line`, `plane`,
`free_state`, `contacting_feature`, `degree_of_freedom_x` .. `_w`, `distance`,
`major_diameter`, `minor_diameter`, `pitch_diameter`, `any_cross_section`,
`any_longitudinal_section`, `circular_or_cylindrical`, `all_around`, `all_over`
(the AP242 `datum_reference_modifier_type` set).

**Dimension**.

| Field | Type | Notes |
| ----- | ---- | ----- |
| `kind` | enum | `size`, `location`, `angular_size`, `angular_location` |
| `subtype` | enum | From the `.name` string: `linear`, `diameter`, `radius`, `spherical_diameter`, `spherical_radius`, `curve_length`, `thickness`, `toroidal_major`, `toroidal_minor`, ... |
| `value` | `Measure?` | Nominal |
| `limits` | `{lower, upper}?` | Range dimensions, from `value_format_type_qualifier` limit values |
| `tolerance` | `PlusMinus \| LimitsAndFits?` | `PlusMinus = {lower: Measure, upper: Measure}`; `LimitsAndFits = {form, zone, grade}` |
| `qualifier` | enum? | `basic` (theoretical), `reference` (auxiliary), `maximum`, `minimum` |
| `modifiers` | enum[] | `controlled_radius`, `square`, `statistical`, `continuous_feature`, `two_point_size`, `local_size`, `least_squares`, `envelope`, `free_state`, ... from `descriptive_representation_item` |
| `features` | id[] | One for size, two for location |
| `directed` | bool | `true` for `directed_dimensional_location`: the order of `features` is significant |
| `orientation` | `Direction?` | For oriented locations |
| `path` | id? | Feature along which a `_with_path` dimension is measured |
| `decimal_places` | integer? | |
| `text` | string? | As displayed, e.g. `⌀12.5 ±0.05` |

**GeometricTolerance**.

| Field | Type | Notes |
| ----- | ---- | ----- |
| `kind` | enum | `angularity`, `circular_runout`, `coaxiality`, `concentricity`, `cylindricity`, `flatness`, `line_profile`, `parallelism`, `perpendicularity`, `position`, `roundness`, `straightness`, `surface_profile`, `symmetry`, `total_runout` |
| `value` | `Measure?` | `geometric_tolerance.magnitude` |
| `zone` | `Zone?` | `{form, projected: Measure?, runout_angle: Measure?}` from `tolerance_zone_form` (rec. practice table 13 names: `cylindrical_or_circular`, `spherical`, `within_circle`, `between_two_concentric_circles`, `between_two_equidistant_curves`, `within_cylinder`, `between_two_coaxial_cylinders`, `between_two_equidistant_surfaces`, `non_uniform`), `projected_zone_definition`, `runout_zone_definition` |
| `unequally_disposed` | `Measure?` | The Ⓤ displacement |
| `maximum_value` | `Measure?` | `geometric_tolerance_with_maximum_tolerance` |
| `unit_basis` | `{length: Measure, width: Measure?, area_type: string?}?` | Per-unit tolerances, `_with_defined_unit` and `_with_defined_area_unit` |
| `modifiers` | enum[] | `maximum_material`, `least_material`, `free_state`, `tangent_plane`, `statistical`, `common_zone`, `any_cross_section`, `circle`, `reciprocity`, `separate_requirement`, `each_radial_element`, `line_element`, `not_convex`, `major_diameter`, `minor_diameter`, `pitch_diameter`, `unequally_disposed` |
| `datum_system` | id? | |
| `features` | id[] | Toleranced shape aspects |
| `affected_plane` | `Direction?` | Orientation plane / intersection plane |
| `composite_of` | id? | The parent when this is a lower segment of a composite frame (`geometric_tolerance_relationship`) |
| `decimal_places` | integer? | From the magnitude's value format qualifier |
| `text` | string? | The frame as text, e.g. `⌖ ⌀0.1 Ⓜ \| A \| B Ⓜ \| C` |

**Note**: semantic text notes and flag notes when the file marks them as
such (AP242 editable text, JT note entities): `{text, kind: general|flag,
features: id[]}`.

**Other**: a recognised PMI concept the model has no record for yet
(surface texture, weld symbols, measurement points, coordinate systems):
`{kind, attributes: map, features: id[]}`. This is the structured escape
hatch; `unknown[]` at document level is the unstructured one, carrying the
raw source record text.

### STEP AP242 mapping

| AP242 entities | Record / field |
| -------------- | -------------- |
| `dimensional_size`, `angular_size`, `dimensional_location`, `angular_location`, `directed_dimensional_location`, `*_with_path` | `Dimension` |
| `dimensional_characteristic_representation` → `shape_dimension_representation` → `measure_representation_item` (+ `qualified_representation_item`, `value_format_type_qualifier`, `type_qualifier`) | `Dimension.value`, `limits`, `qualifier`, `decimal_places` |
| `plus_minus_tolerance` → `tolerance_value` / `limits_and_fits` | `Dimension.tolerance` |
| `descriptive_representation_item` ('theoretical', 'auxiliary', modifier names, in `compound_representation_item` 'modifiers') | `Dimension.qualifier`, `modifiers` |
| `geometric_tolerance` + one of 15 subtypes, mixed with `_with_datum_reference`, `_with_modifiers`, `_with_maximum_tolerance`, `unequally_disposed_geometric_tolerance`, `_with_defined_unit`, `_with_defined_area_unit` | `GeometricTolerance` |
| `tolerance_zone`, `tolerance_zone_form`, `projected_zone_definition`, `runout_zone_definition`, `non_uniform_zone_definition` | `GeometricTolerance.zone` |
| `geometric_tolerance_relationship` | `GeometricTolerance.composite_of` |
| `datum`, `datum_feature`, `datum_target`, `placed_datum_target_feature`, `shape_representation_with_parameters` | `Datum`, `DatumTarget` |
| `datum_system`, `datum_reference_compartment`, `datum_reference_element`, `general_datum_reference`, `referenced_modified_datum`, `datum_reference_modifier_with_value` | `DatumSystem` |
| `shape_aspect`, `composite_shape_aspect`, `composite_group_shape_aspect`, `all_around_shape_aspect`, `between_shape_aspect`, `derived_shape_aspect`, `centre_of_symmetry`, `apex`, `tangent`, `geometric_item_specific_usage`, `item_identified_representation_usage` | `Feature` |
| `draughting_model_item_association` | `presentation` cross-links |
| `length_unit`, `plane_angle_unit`, `si_unit`, `conversion_based_unit`, `global_unit_assigned_context` | `units`, `Measure.unit` |

A complex instance is interpreted by the *set* of its segment keywords, never
by a single type name, because the mix-ins appear in any combination.

### JT mapping (forward-looking)

JT PMI entity kinds map onto the same records: Dimension → `Dimension`,
Feature Control Frame → `GeometricTolerance` plus `DatumSystem`, Datum
Feature Symbol → `Datum`, Datum Target → `DatumTarget`, Note → `Note`,
Surface Finish, Weld, Measurement Point, Locator, Coordinate System →
`Other`. Structured values come from the property bag where present and from
parsing the text otherwise, with `origin: text`. PMI associations populate
`features` through B-rep face references. Nothing in the semantic layer
requires an entity graph, so a text-first reader fits without special cases.

### Example

One feature control frame from the NIST corpus, position ⌀0.1 at MMC to
datums A, B at MMC, C:

```json
{
  "id": "tol:7f3a...",
  "kind": "position",
  "value": { "value": 0.1, "unit": "mm" },
  "zone": { "form": "within_cylinder" },
  "modifiers": ["maximum_material"],
  "datum_system": "dsys:a1c9...",
  "features": ["feat:2b77..."],
  "text": "⌖ ⌀0.1 Ⓜ | A | B Ⓜ | C",
  "origin": "semantic",
  "presentation": ["ann:9e02..."],
  "source_refs": ["#1752"]
}
```

with

```json
{
  "id": "dsys:a1c9...",
  "compartments": [
    { "datums": [{ "datum": "datum:A", "modifiers": [] }], "common": false },
    { "datums": [{ "datum": "datum:B", "modifiers": ["maximum_material"] }], "common": false },
    { "datums": [{ "datum": "datum:C", "modifiers": [] }], "common": false }
  ],
  "text": "A|B(M)|C",
  "origin": "semantic",
  "source_refs": ["#1751"]
}
```

### Rust representation

`pmix::model` holds plain structs and enums deriving `Serialize` and
`Deserialize`, `snake_case` field and variant names, `Option` fields skipped
when `None`, empty arrays emitted (an empty `modifiers` list is a fact).
Enumerations with a verbatim fallback are implemented as `enum Kind { Known
variants..., Other(String) }` with custom string serialisation. Readers
implement `trait Reader { fn read(&self, input: &[u8], name: &str) ->
Result<PmiDocument> }`. The existing placeholder `model.rs` is replaced;
`schema_version` stays at 1 because nothing has been released.

## Alternatives considered

- **Mirror the AP242 entity graph in JSON.** Lossless but not comparable:
  two exports of the same design differ in every entity number, and a JT
  reader could never produce it.
- **Adopt OpenCascade's `XCAFDimTolObjects` shape as-is.** Its enumerations
  are sound and are reused, but it has no place for unmapped content, no
  provenance, no text-origin distinction, and no notes.
- **Text-only records** (each callout as a string). Trivially JT-compatible
  and easy to diff, but ambiguous: `0.1` in a frame means different things
  depending on zone and modifiers, and text formatting differs by CAD
  system, so diffs would be dominated by noise.
- **Per-entity worksheets as in the NIST analyser.** Useful as an oracle;
  too close to the source schema to serve as a cross-format model.

## Consequences

- The STEP extractor is a set of walkers over the untyped Part 21 graph,
  one per record type, each keyed on segment sets, filling these structs.
- Every walker ships with a synthetic fixture and is validated against the
  NIST corpus and its oracles (see `docs/test-data.md`).
- The identity ADR must be settled before `pmix diff`; until then ids are
  content hashes, stable for identical content and not otherwise.
- The presentation ADR defines annotations, planes, and saved views, and the
  exact form of the `presentation` cross-link ids.
- JT work can begin against the same structs with no model changes, adding
  readers rather than fields.
