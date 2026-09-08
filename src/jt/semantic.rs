//! Mapping JT PMI onto the model of ADR 0002.
//!
//! JT records the meaning of an annotation in the properties of its PMI
//! entity, not in dedicated fields: a dimension's magnitude is the
//! `value` property, its deviations are `lowerDelta` and `upperDelta`,
//! and a feature control frame's datum references are the `label`
//! properties beneath its tolerance compartments. The property keys and
//! their enumerations are those of the specification's PMI property
//! list.
//!
//! Producing systems vary the keys around a common shape, so this module
//! matches by suffix wherever it can rather than by exact key. Every
//! property is also kept verbatim on the record, so nothing a file says
//! is lost even where this reader has no field for it.

use std::collections::BTreeMap;

use crate::model::{
    Compartment, ContentId, Datum, DatumModifier, DatumRef, DatumSystem, Dimension, DimensionKind,
    DimensionQualifier, DimensionSubtype, DimensionTolerance, GeometricTolerance, Layer, Measure,
    Meta, Note, NoteKind, Origin, Other, Semantic, ToleranceKind, ToleranceModifier, Unknown,
    Unmapped,
};

use super::pmi::{Entity, EntityKind, PmiManager};

/// Entity kinds that become [`Dimension`] records.
fn is_dimension(kind: &EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Dimension | EntityKind::CalloutDimension | EntityKind::ChamferDimension
    )
}

/// Entity kinds that become [`GeometricTolerance`] records.
fn is_tolerance(kind: &EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::FeatureControlFrame | EntityKind::CompositeFeatureControlFrame
    )
}

/// Entity kinds that become [`Note`] records.
fn is_note(kind: &EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Note
            | EntityKind::FaceAttributeNote
            | EntityKind::ModelViewLabelNote
            | EntityKind::BalloonNote
            | EntityKind::CoordinateNote
            | EntityKind::AttributeNote
            | EntityKind::BundleOrDressingNote
            | EntityKind::WeldNote
    )
}

/// Entity kinds that carry no PMI of their own: drawing furniture, the
/// styling of a view, and the placement of a component.
fn is_presentation_only(kind: &EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::ReferenceGeometry
            | EntityKind::Centreline
            | EntityKind::CircleCentre
            | EntityKind::Crosshatch
            | EntityKind::CuttingPlaneSymbol
            | EntityKind::ModelViewStyle
            | EntityKind::PartTransform
            | EntityKind::Section
    )
}

/// The dimension `type` enumeration of the PMI property list.
fn dimension_subtype(code: Option<&str>) -> Option<DimensionSubtype> {
    Some(match code?.trim() {
        "0" => DimensionSubtype::CurveLength,
        "1" => DimensionSubtype::Linear,
        "2" => DimensionSubtype::parse("angle"),
        "3" => DimensionSubtype::Radius,
        other => DimensionSubtype::Other(other.to_owned()),
    })
}

/// The `characteristic` enumeration of the PMI property list.
pub(crate) fn tolerance_kind(code: Option<&str>) -> ToleranceKind {
    match code.map(str::trim) {
        Some("0") => ToleranceKind::LineProfile,
        Some("1") => ToleranceKind::CircularRunout,
        Some("2") => ToleranceKind::Perpendicularity,
        Some("3") => ToleranceKind::Position,
        Some("4") => ToleranceKind::TotalRunout,
        Some("5") => ToleranceKind::SurfaceProfile,
        Some("6") => ToleranceKind::Cylindricity,
        Some("7") => ToleranceKind::Symmetry,
        Some("8") => ToleranceKind::Angularity,
        Some("9") => ToleranceKind::Parallelism,
        Some("10") => ToleranceKind::Concentricity,
        Some("11") => ToleranceKind::Flatness,
        Some("12") => ToleranceKind::Roundness,
        Some("13") => ToleranceKind::Straightness,
        Some("14") => ToleranceKind::parse("axis_intersection"),
        Some(other) => ToleranceKind::Other(other.to_owned()),
        None => ToleranceKind::Other(String::new()),
    }
}

/// The `modifier` enumeration of the PMI property list, which states the
/// material condition a tolerance or a datum reference applies at.
/// `rfs` is the regardless-of-feature-size default and adds nothing.
fn material_condition(code: Option<&str>) -> Option<&'static str> {
    match code.map(str::trim) {
        Some("0") => Some("least_material"),
        Some("1") => Some("maximum_material"),
        _ => None,
    }
}

/// Flags that state a tolerance modifier when set, and the modifier they
/// name. `allAround` and `allOver` sit on the entity itself; the rest are
/// written under the tolerance compartment they qualify.
const TOLERANCE_FLAGS: &[(&str, &str)] = &[
    ("allAround", "all_around"),
    ("allOver", "all_over"),
    ("commonZone", "common_zone"),
    ("reciprocityRequirement", "reciprocity"),
    ("tangentPlane", "tangent_plane"),
    ("unequal", "unequally_disposed"),
];

/// The modifiers a feature control frame states, in a stable order.
fn tolerance_modifiers(entity: &Entity) -> Vec<ToleranceModifier> {
    let mut out: Vec<ToleranceModifier> = TOLERANCE_FLAGS
        .iter()
        .filter(|(flag, _)| {
            entity
                .properties
                .iter()
                .any(|(k, v)| k.rsplit('.').next() == Some(*flag) && v.trim() != "0")
        })
        .map(|(_, name)| ToleranceModifier::parse(name))
        .collect();
    // The material condition the tolerance itself applies at, as opposed
    // to the one on a datum reference.
    out.extend(
        entity
            .ending_with(".modifier")
            .iter()
            .filter(|(path, _)| !path.contains("Datum"))
            .filter_map(|(_, code)| material_condition(Some(code)))
            .map(ToleranceModifier::parse),
    );
    out.dedup();
    out
}

/// Properties this reader turns into fields of a record, and so does not
/// repeat as unmapped attributes.
const CONSUMED: &[&str] = &[
    "Description",
    "basic",
    "characteristic",
    "deviation",
    "featureOfSize",
    "grade",
    "isReference",
    "label",
    "lowerDelta",
    "allAround",
    "allOver",
    "commonZone",
    "modifier",
    "name",
    "reciprocityRequirement",
    "tangentPlane",
    "unequal",
    "precision",
    "type",
    "upperDelta",
    "value",
];

/// Properties that say how an annotation is drawn rather than what it
/// means. They belong to the presentation layer of ADR 0003, which the JT
/// reader does not fill yet, so the semantic records leave them out
/// instead of burying the design data under them.
const STYLE: &[&str] = &[
    "accountabilityId",
    "alignment",
    "angleFormat",
    "angleNumerator",
    "appendedTextSpaceFactor",
    "attachmentType",
    "blanked",
    "bold",
    "chamferSeparatorCapital",
    "chamferSpacing",
    "commaAsDecimal",
    "datumOnLeader",
    "diameterPlacement",
    "dimensionLeadingZero",
    "dimensionLineBetweenArrows",
    "dimensionLineTrim",
    "dimensionTrailingZero",
    "documentation",
    "dualDimensionLineCenter",
    "fcfTextUnderline",
    "flag",
    "font",
    "fraction",
    "fractionSize",
    "inFront",
    "inspectionDisplay",
    "italic",
    "justification",
    "leadingZero",
    "limitDisplay",
    "limitFitOrder",
    "limitFitParenthesis",
    "lineFactor",
    "majorAngle",
    "manual",
    "narrowLeaderAngle",
    "narrowOffset",
    "notToScale",
    "panZoom",
    "parameter",
    "parameterLineFactor",
    "parameterSpaceFactor",
    "radialPlacement",
    "referenceContent",
    "singleSideFirst",
    "singleSideLength",
    "singleSided",
    "spaceFactor",
    "statisticalPlacement",
    "strikethrough",
    "style",
    "textAspect",
    "textThickness",
    "toleranceAngleFormat",
    "toleranceDisplay",
    "toleranceLeadingZero",
    "tolerancePrecision",
    "toleranceTextSpaceFactor",
    "toleranceTrailingZero",
    "trailingZero",
    "zeroToleranceDisplay",
];

/// Key fragments that mark a whole family of drawing properties.
const STYLE_PARTS: &[&str] = &[
    "Leader",
    "Text",
    "text",
    "colour",
    "Colour",
    "Color",
    "width",
    "Symbol",
    "symbol",
    "Display",
    "Plane",
    "Origin",
    "origin",
    "position",
    "LAYER",
    "JTTK",
    "____JtTk",
    "GeneralAttribute",
    "NX_PMI",
];

/// Whether a property describes how an annotation is drawn.
pub(crate) fn is_style(key: &str) -> bool {
    // Nested keys such as `ParameterDimension[0].fraction` name the same
    // setting as the bare key, so judge the last segment too.
    let leaf = key.rsplit('.').next().unwrap_or(key);
    STYLE.contains(&leaf) || STYLE_PARTS.iter().any(|s| key.contains(s))
}

/// Whether a property was read into a field of the record.
fn is_consumed(key: &str) -> bool {
    CONSUMED.contains(&key.rsplit('.').next().unwrap_or(key))
}

/// Attributes worth keeping on a record: everything the reader did not
/// read into a field and that is not purely about drawing.
fn unmapped(entity: &Entity) -> Vec<Unmapped> {
    let mut out: Vec<Unmapped> = entity
        .properties
        .iter()
        .filter(|(k, v)| !v.is_empty() && !is_consumed(k) && !is_style(k))
        .map(|(k, v)| Unmapped {
            attribute: k.clone(),
            raw: v.clone(),
        })
        .collect();
    out.sort_by(|a, b| a.attribute.cmp(&b.attribute));
    out.dedup_by(|a, b| a.attribute == b.attribute);
    out
}

/// Build the shared fields of a record.
fn meta(
    ids: &mut ContentId,
    prefix: &str,
    parts: &[&str],
    entity: &Entity,
    manager: usize,
) -> Meta {
    Meta {
        id: ids.make(prefix, parts),
        origin: Origin::Semantic,
        presentation: Vec::new(),
        unmapped: unmapped(entity),
        source_refs: vec![source_ref(manager, entity)],
    }
}

/// The datum references of a feature control frame, in compartment order.
///
/// Writers disagree on the key: the specification lists
/// `Primary.Datum[0].label` and `ToleranceCompartment[0].DatumGroup
/// .Reference[0].label`, while NX writes `ToleranceCompartment[0]
/// .PrimaryDatum.Reference[0].label`. All of them end in `.label` and name
/// their precedence, so match on that.
fn datum_references(entity: &Entity) -> Vec<Vec<DatumRef>> {
    let mut order: [Vec<DatumRef>; 3] = Default::default();
    for (path, label) in entity.ending_with(".label") {
        if label.is_empty() {
            continue;
        }
        let rank = if path.contains("Primary") {
            0
        } else if path.contains("Secondary") {
            1
        } else if path.contains("Tertiary") {
            2
        } else {
            continue;
        };
        if order[rank].iter().any(|d| d.datum == label) {
            continue;
        }
        // The material condition sits beside the label, under the same path.
        let modifiers = material_condition(entity.property(&format!("{path}.modifier")))
            .map(|m| vec![DatumModifier::parse(m)])
            .unwrap_or_default();
        order[rank].push(DatumRef {
            datum: label.to_owned(),
            modifiers,
            modifier_values: Vec::new(),
        });
    }
    order.into_iter().filter(|c| !c.is_empty()).collect()
}

/// A property of `entity` under `prefix`, which is empty for the entity's
/// own properties and `ParameterDimension[0].` and so on for the
/// measurements a callout nests inside itself.
fn under<'a>(entity: &'a Entity, prefix: &str, key: &str) -> Option<&'a str> {
    entity.property(&format!("{prefix}{key}"))
}

fn number_under(entity: &Entity, prefix: &str, key: &str) -> Option<f64> {
    under(entity, prefix, key)?.trim().parse().ok()
}

fn flag_under(entity: &Entity, prefix: &str, key: &str) -> bool {
    under(entity, prefix, key).is_some_and(|v| v.trim() != "0")
}

/// The parameter numbers a callout dimension uses, in order.
fn parameter_indices(entity: &Entity) -> Vec<usize> {
    let mut found: Vec<usize> = entity
        .properties
        .iter()
        .filter_map(|(k, _)| {
            k.strip_prefix("ParameterDimension[")?
                .split(']')
                .next()?
                .parse()
                .ok()
        })
        .collect();
    found.sort_unstable();
    found.dedup();
    found
}

/// Where the ids and identity keys of new records are collected.
pub struct Naming<'a> {
    ids: &'a mut ContentId,
    keys: &'a mut Keys,
}

/// One measurement to be turned into a record: which entity states it,
/// which element that entity is in, and where it sits on the part.
pub struct Measured<'a> {
    entity: &'a Entity,
    manager: usize,
    /// Empty for the entity's own measurement, `ParameterDimension[0].`
    /// and so on for the ones a callout nests inside itself.
    prefix: &'a str,
    anchor: &'a str,
    description: &'a str,
    length: Option<&'a str>,
}

/// Add a dimension record for the measurement under `prefix`, if it
/// states a value. A dimension with no value is a callout that only
/// frames the parameters nested inside it.
fn push_dimension(out: &mut Semantic, into: &mut Naming<'_>, at: Measured<'_>) {
    let Measured {
        entity,
        manager,
        prefix,
        anchor,
        description,
        length,
    } = at;
    let Naming { ids, keys } = into;
    let Some(value) = number_under(entity, prefix, "value") else {
        return;
    };
    let subtype =
        dimension_subtype(under(entity, prefix, "type")).unwrap_or(DimensionSubtype::Linear);
    let angular = subtype == DimensionSubtype::parse("angle");
    let unit = if angular { "deg" } else { length.unwrap_or("") };
    let value = Measure::new(value, unit);

    // A fit is stated as its deviation letter and tolerance grade; a
    // deviation alone is a plain plus and minus tolerance.
    let tolerance = match (
        under(entity, prefix, "deviation"),
        under(entity, prefix, "grade"),
    ) {
        (Some(zone), Some(grade)) if !zone.is_empty() => Some(DimensionTolerance::LimitsAndFits {
            form: String::new(),
            zone: zone.to_owned(),
            grade: grade.to_owned(),
        }),
        _ => match (
            number_under(entity, prefix, "lowerDelta"),
            number_under(entity, prefix, "upperDelta"),
        ) {
            // Both zero means no tolerance was stated, not a tolerance of
            // nothing: JT writes the pair whether or not one applies.
            (Some(lower), Some(upper)) if lower != 0.0 || upper != 0.0 => {
                Some(DimensionTolerance::PlusMinus {
                    lower: Measure::new(lower, unit),
                    upper: Measure::new(upper, unit),
                })
            }
            _ => None,
        },
    };

    let qualifier = if flag_under(entity, prefix, "basic") {
        Some(DimensionQualifier::Basic)
    } else if flag_under(entity, prefix, "isReference") {
        Some(DimensionQualifier::Reference)
    } else {
        None
    };
    // JT states whether a dimension measures a feature of size, which is
    // what separates a size from a location. Producing systems leave the
    // flag clear on dimensions whose own kind already settles it, so a
    // radius or a diameter counts as a size either way.
    let sized = flag_under(entity, prefix, "featureOfSize")
        || matches!(
            subtype,
            DimensionSubtype::Radius
                | DimensionSubtype::Diameter
                | DimensionSubtype::SphericalRadius
                | DimensionSubtype::SphericalDiameter
        );
    let text = under(entity, prefix, "name")
        .filter(|n| !n.is_empty())
        .unwrap_or(description);

    let kind = match (angular, sized) {
        (true, true) => DimensionKind::AngularSize,
        (true, false) => DimensionKind::AngularLocation,
        (false, true) => DimensionKind::Size,
        (false, false) => DimensionKind::Location,
    };
    let meta = meta(
        ids,
        "dim",
        &[subtype.as_str(), &value.canonical(), text, prefix],
        entity,
        manager,
    );
    // Which slot of a callout this is belongs to identity: a callout
    // states its diameter and its depth in a fixed order.
    keys.insert(
        meta.id.clone(),
        [
            kind.as_str(),
            subtype.as_str(),
            anchor,
            prefix.trim_end_matches('.'),
        ]
        .join("|"),
    );
    out.dimensions.push(Dimension {
        meta,
        kind,
        subtype,
        value: Some(value),
        limits: None,
        tolerance,
        qualifier,
        modifiers: Vec::new(),
        features: Vec::new(),
        directed: false,
        orientation: None,
        path: None,
        decimal_places: number_under(entity, prefix, "precision")
            .and_then(|p| u8::try_from(p as i64).ok()),
        text: (!text.is_empty()).then(|| text.to_owned()),
    });
}

/// Which semantic records came from which PMI entity, keyed by the
/// element the entity is in and its position there. The presentation
/// layer uses this to say what an annotation displays.
pub type Links = BTreeMap<(usize, usize), Vec<String>>;

/// What the reader knows about each record's identity, by the temporary
/// id the record carries until [`crate::jt::identity`] replaces it.
pub type Keys = BTreeMap<String, String>;

/// What one pass of the semantic walker produced.
#[derive(Debug, Default)]
pub struct Built {
    pub semantic: Semantic,
    pub links: Links,
    pub keys: Keys,
}

/// Where on the part an annotation is attached, as an identity key.
///
/// JT states no features, so this stands in for the feature references
/// that anchor a STEP record (ADR 0004). A leader terminator is a point
/// on the part that the annotation points at, which is the closest thing
/// the format gives to "what this callout is about"; an annotation drawn
/// without leaders is anchored by the plane it sits on instead.
///
/// Both are model-space geometry, so both survive a re-export of the
/// same design and both change if the design is remodelled, which is the
/// same bargain the STEP fingerprint makes.
pub fn anchor(entity: &Entity, per_metre: f64) -> String {
    let mut points: Vec<String> = entity
        .ending_with(".terminator")
        .iter()
        .filter_map(|(_, v)| point(v, 1.0))
        .collect();
    if points.is_empty() {
        // The display plane's origin is written in metres.
        if let Some(p) = entity
            .property("DisplayPlane.origin")
            .and_then(|v| point(v, per_metre))
        {
            return format!("@{p}");
        }
        return String::new();
    }
    points.sort();
    points.dedup();
    points.join(";")
}

/// Read three numbers, scale them, and round them for an identity key.
fn point(text: &str, scale: f64) -> Option<String> {
    let v: Vec<f64> = text
        .split_whitespace()
        .map(|n| n.parse().ok())
        .collect::<Option<Vec<f64>>>()?;
    (v.len() == 3).then(|| {
        crate::identity::triple(
            [v[0] * scale, v[1] * scale, v[2] * scale],
            crate::identity::COARSE_QUANTUM,
        )
    })
}

/// How a PMI entity is named in `source_refs`. User labels repeat between
/// elements, so the element has to be named as well.
pub fn source_ref(manager: usize, entity: &Entity) -> String {
    format!("pmi[{manager}]#{}", entity.user_label)
}

/// Turn the PMI of one file into semantic records.
///
/// `length` is the unit the file declares its model in, which is the unit
/// every PMI measure is expressed in; `angle` is degrees, which is what
/// the PMI property list specifies for angular values.
pub fn build(managers: &[PmiManager], length: Option<&str>, unknown: &mut Vec<Unknown>) -> Built {
    let per_metre = length
        .and_then(crate::jt::property::per_metre)
        .unwrap_or(1.0);
    let mut out = Semantic::default();
    let mut links = Links::new();
    let mut keys = Keys::new();
    let mut ids = ContentId::new();
    // Datum systems are shared by the frames that reference them, so a
    // system is emitted once per distinct set of compartments.
    let mut systems: BTreeMap<String, String> = BTreeMap::new();

    for (m, manager) in managers.iter().enumerate() {
        for (e, entity) in manager.entities.iter().enumerate() {
            let mut made: Vec<String> = Vec::new();
            let kind = entity.kind.as_str();
            let anchor = anchor(entity, per_metre);
            let description = entity
                .property("Description")
                .unwrap_or_default()
                .to_owned();

            if is_dimension(&entity.kind) {
                let before = out.dimensions.len();
                // The entity's own measurement, when it has one.
                let mut naming = Naming {
                    ids: &mut ids,
                    keys: &mut keys,
                };
                push_dimension(
                    &mut out,
                    &mut naming,
                    Measured {
                        entity,
                        manager: m,
                        prefix: "",
                        anchor: &anchor,
                        description: &description,
                        length,
                    },
                );
                // A callout such as a hole and thread note carries its
                // sizes as numbered parameters rather than one value, and
                // each of those is a dimension in its own right.
                for index in parameter_indices(entity) {
                    let prefix = format!("ParameterDimension[{index}].");
                    push_dimension(
                        &mut out,
                        &mut naming,
                        Measured {
                            entity,
                            manager: m,
                            prefix: &prefix,
                            anchor: &anchor,
                            description: &description,
                            length,
                        },
                    );
                }
                made.extend(out.dimensions[before..].iter().map(|d| d.meta.id.clone()));
                if out.dimensions.len() == before {
                    unknown.push(Unknown {
                        layer: Layer::Semantic,
                        kind: kind.clone(),
                        reason: "the dimension states no value".into(),
                        source_ref: source_ref(m, entity),
                        raw: description.clone(),
                    });
                }
                links.insert((m, e), made);
                continue;
            }

            if is_tolerance(&entity.kind) {
                let unit = length.unwrap_or("");
                let kind = tolerance_kind(entity.property("characteristic"));
                let value = entity
                    .ending_with(".value")
                    .first()
                    .and_then(|(_, v)| v.trim().parse().ok())
                    .map(|v: f64| Measure::new(v, unit));
                let compartments = datum_references(entity);
                let datum_system = (!compartments.is_empty()).then(|| {
                    let text = compartments
                        .iter()
                        .map(|c| {
                            c.iter()
                                .map(|d| d.datum.as_str())
                                .collect::<Vec<_>>()
                                .join(",")
                        })
                        .collect::<Vec<_>>()
                        .join("|");
                    systems
                        .entry(text.clone())
                        .or_insert_with(|| {
                            let id = ids.make("dsys", &[&text]);
                            out.datum_systems.push(DatumSystem {
                                meta: Meta {
                                    id: id.clone(),
                                    ..Default::default()
                                },
                                compartments: compartments
                                    .iter()
                                    .map(|c| Compartment {
                                        common: c.len() > 1,
                                        datums: c.clone(),
                                    })
                                    .collect(),
                                text: text.clone(),
                            });
                            id
                        })
                        .clone()
                });
                let canonical = value.as_ref().map(Measure::canonical).unwrap_or_default();
                let system_text = datum_system.clone().unwrap_or_default();
                let meta = meta(
                    &mut ids,
                    "gtol",
                    &[kind.as_str(), &canonical, &system_text, &description],
                    entity,
                    m,
                );
                keys.insert(meta.id.clone(), [kind.as_str(), &anchor].join("|"));
                made.push(meta.id.clone());
                out.tolerances.push(GeometricTolerance {
                    meta,
                    kind,
                    value,
                    zone: None,
                    unequally_disposed: None,
                    maximum_value: None,
                    unit_basis: None,
                    modifiers: tolerance_modifiers(entity),
                    datum_system,
                    features: Vec::new(),
                    affected_plane: None,
                    composite_of: None,
                    decimal_places: None,
                    text: (!description.is_empty()).then(|| description.clone()),
                });
                links.insert((m, e), made);
                continue;
            }

            if entity.kind == EntityKind::DatumFeatureSymbol {
                let label = entity.property("label").unwrap_or_default().to_owned();
                let meta = meta(&mut ids, "datum", &[&label, &description], entity, m);
                keys.insert(meta.id.clone(), label.clone());
                made.push(meta.id.clone());
                out.datums.push(Datum {
                    meta,
                    label,
                    features: Vec::new(),
                    targets: Vec::new(),
                });
                links.insert((m, e), made);
                continue;
            }

            if is_note(&entity.kind) {
                let text = if description.is_empty() {
                    entity.texts.join(" ")
                } else {
                    description.clone()
                };
                let meta = meta(&mut ids, "note", &[&kind, &text], entity, m);
                keys.insert(meta.id.clone(), [kind.as_str(), text.as_str()].join("|"));
                made.push(meta.id.clone());
                out.notes.push(Note {
                    meta,
                    text,
                    kind: NoteKind::General,
                    features: Vec::new(),
                });
                links.insert((m, e), made);
                continue;
            }

            if is_presentation_only(&entity.kind) {
                continue;
            }

            // Anything else that carries properties is a PMI concept the
            // model has no record for yet: keep it rather than drop it.
            let attributes: BTreeMap<String, String> = entity
                .properties
                .iter()
                .filter(|(k, v)| !v.is_empty() && !is_style(k))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            if attributes.is_empty() {
                unknown.push(Unknown {
                    layer: Layer::Semantic,
                    kind: kind.clone(),
                    reason: "no PMI walker for this JT entity type".into(),
                    source_ref: source_ref(m, entity),
                    raw: String::new(),
                });
                continue;
            }
            let meta = meta(&mut ids, "other", &[&kind, &description], entity, m);
            keys.insert(
                meta.id.clone(),
                [kind.as_str(), description.as_str()].join("|"),
            );
            made.push(meta.id.clone());
            out.other.push(Other {
                meta,
                kind,
                attributes,
                features: Vec::new(),
            });
            links.insert((m, e), made);
        }
    }

    out.sort();
    Built {
        semantic: out,
        links,
        keys,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity(kind: EntityKind, props: &[(&str, &str)]) -> Entity {
        Entity {
            kind,
            user_label: 1,
            texts: Vec::new(),
            polylines: Vec::new(),
            text_polylines: Vec::new(),
            properties: props
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
            type_name: None,
            valid: true,
        }
    }

    fn build_one(entity: Entity) -> Semantic {
        let manager = PmiManager {
            entities: vec![entity],
            ..Default::default()
        };
        build(&[manager], Some("mm"), &mut Vec::new()).semantic
    }

    #[test]
    fn a_dimension_carries_its_value_and_deviations() {
        let out = build_one(entity(
            EntityKind::Dimension,
            &[
                ("type", "1"),
                ("value", "5.5"),
                ("lowerDelta", "-0.15"),
                ("upperDelta", "0.15"),
                ("featureOfSize", "1"),
                ("precision", "2"),
                ("Description", "Linear Dimension"),
            ],
        ));
        let d = &out.dimensions[0];
        assert_eq!(d.kind, DimensionKind::Size);
        assert_eq!(d.subtype, DimensionSubtype::Linear);
        assert_eq!(d.value, Some(Measure::new(5.5, "mm")));
        assert_eq!(
            d.tolerance,
            Some(DimensionTolerance::PlusMinus {
                lower: Measure::new(-0.15, "mm"),
                upper: Measure::new(0.15, "mm"),
            })
        );
        assert_eq!(d.decimal_places, Some(2));
        assert_eq!(d.text.as_deref(), Some("Linear Dimension"));
    }

    #[test]
    fn an_angular_dimension_is_measured_in_degrees() {
        let out = build_one(entity(
            EntityKind::Dimension,
            &[("type", "2"), ("value", "60")],
        ));
        let d = &out.dimensions[0];
        assert_eq!(d.value.as_ref().unwrap().unit, "deg");
        assert_eq!(d.kind, DimensionKind::AngularLocation);
    }

    #[test]
    fn a_feature_control_frame_resolves_its_datum_system() {
        let out = build_one(entity(
            EntityKind::FeatureControlFrame,
            &[
                ("characteristic", "3"),
                ("ToleranceCompartment[0].value", "0.10"),
                (
                    "ToleranceCompartment[0].PrimaryDatum.Reference[0].label",
                    "A",
                ),
                (
                    "ToleranceCompartment[0].SecondaryDatum.Reference[0].label",
                    "B",
                ),
                (
                    "ToleranceCompartment[0].TertiaryDatum.Reference[0].label",
                    "C",
                ),
            ],
        ));
        let t = &out.tolerances[0];
        assert_eq!(t.kind, ToleranceKind::Position);
        assert_eq!(t.value, Some(Measure::new(0.10, "mm")));
        let system = &out.datum_systems[0];
        assert_eq!(system.text, "A|B|C");
        assert_eq!(t.datum_system.as_ref(), Some(&system.meta.id));
        assert_eq!(system.compartments.len(), 3);
    }

    #[test]
    fn an_unlisted_characteristic_is_kept_verbatim() {
        let out = build_one(entity(
            EntityKind::FeatureControlFrame,
            &[("characteristic", "99")],
        ));
        assert_eq!(out.tolerances[0].kind, ToleranceKind::Other("99".into()));
    }

    #[test]
    fn a_callout_expands_into_the_sizes_it_states() {
        let out = build_one(entity(
            EntityKind::CalloutDimension,
            &[
                ("Description", "Hole and Thread Callout"),
                ("type", "1"),
                ("ParameterDimension[0].type", "1"),
                ("ParameterDimension[1].type", "1"),
                ("ParameterDimension[1].value", "6.6"),
                ("ParameterDimension[2].type", "3"),
                ("ParameterDimension[2].value", "3.3"),
            ],
        ));
        // The callout itself states no value, and the parameter that has
        // none is not a measurement either.
        let values: Vec<f64> = out
            .dimensions
            .iter()
            .map(|d| d.value.as_ref().unwrap().value)
            .collect();
        assert_eq!(values.len(), 2);
        assert!(values.contains(&6.6) && values.contains(&3.3));
        assert!(
            out.dimensions
                .iter()
                .all(|d| d.text.as_deref() == Some("Hole and Thread Callout"))
        );
    }

    #[test]
    fn a_dimension_with_no_value_is_reported_rather_than_dropped() {
        let manager = PmiManager {
            entities: vec![entity(EntityKind::Dimension, &[("type", "1")])],
            ..Default::default()
        };
        let mut unknown = Vec::new();
        let out = build(&[manager], Some("mm"), &mut unknown).semantic;
        assert!(out.dimensions.is_empty());
        assert_eq!(unknown.len(), 1);
        assert!(unknown[0].reason.contains("no value"), "{unknown:?}");
    }

    #[test]
    fn a_fit_is_read_as_its_deviation_and_grade() {
        let out = build_one(entity(
            EntityKind::Dimension,
            &[
                ("type", "3"),
                ("value", "4.5"),
                ("deviation", "H"),
                ("grade", "7"),
            ],
        ));
        assert_eq!(
            out.dimensions[0].tolerance,
            Some(DimensionTolerance::LimitsAndFits {
                form: String::new(),
                zone: "H".into(),
                grade: "7".into(),
            })
        );
        // A radius measures a feature of size even with the flag clear.
        assert_eq!(out.dimensions[0].kind, DimensionKind::Size);
    }

    #[test]
    fn deviations_of_zero_are_not_a_tolerance() {
        let out = build_one(entity(
            EntityKind::Dimension,
            &[
                ("type", "1"),
                ("value", "10"),
                ("lowerDelta", "0"),
                ("upperDelta", "0"),
            ],
        ));
        assert_eq!(out.dimensions[0].tolerance, None);
    }

    #[test]
    fn a_tolerance_states_its_material_condition_and_flags() {
        let out = build_one(entity(
            EntityKind::FeatureControlFrame,
            &[
                ("characteristic", "3"),
                ("ToleranceCompartment[0].value", "0.5"),
                ("ToleranceCompartment[0].modifier", "1"),
                ("ToleranceCompartment[0].commonZone", "1"),
                ("allAround", "0"),
                (
                    "ToleranceCompartment[0].PrimaryDatum.Reference[0].label",
                    "A",
                ),
                (
                    "ToleranceCompartment[0].PrimaryDatum.Reference[0].modifier",
                    "0",
                ),
            ],
        ));
        let t = &out.tolerances[0];
        assert_eq!(
            t.modifiers,
            [
                ToleranceModifier::CommonZone,
                ToleranceModifier::MaximumMaterial
            ]
        );
        // The datum reference keeps its own, separate condition.
        let datum = &out.datum_systems[0].compartments[0].datums[0];
        assert_eq!(datum.modifiers, [DatumModifier::LeastMaterial]);
    }

    #[test]
    fn a_datum_feature_symbol_becomes_a_datum() {
        let out = build_one(entity(EntityKind::DatumFeatureSymbol, &[("label", "A")]));
        assert_eq!(out.datums[0].label, "A");
    }

    #[test]
    fn drawing_furniture_produces_no_record() {
        let out = build_one(entity(
            EntityKind::Centreline,
            &[("textColour", "0x00000000")],
        ));
        assert!(out.dimensions.is_empty() && out.other.is_empty());
    }

    #[test]
    fn style_never_reaches_the_unmapped_list() {
        let out = build_one(entity(
            EntityKind::Dimension,
            &[
                ("type", "1"),
                ("value", "1"),
                ("textColour", "0x00000000"),
                ("Leader[0].arrowLength", "3.5"),
                ("inspection", "1"),
            ],
        ));
        let attributes: Vec<&str> = out.dimensions[0]
            .meta
            .unmapped
            .iter()
            .map(|u| u.attribute.as_str())
            .collect();
        assert_eq!(attributes, ["inspection"]);
    }
}
