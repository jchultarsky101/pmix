//! Dimensions: `dimensional_size`, `dimensional_location`, and their
//! angular and `_with_path` variants, with values and tolerances.

use super::features::feature_for;
use super::measures::{measure_with_unit, qualified_item};
use super::{Ctx, source_ref};
use crate::model::{
    Dimension, DimensionKind, DimensionModifier, DimensionQualifier, DimensionSubtype,
    DimensionTolerance, Limits, Measure, Meta, Unmapped,
};
use crate::step::p21::{Id, Instance, Parameter};

/// Dimension entity keywords the walker understands, in both simple and
/// complex-instance form.
const DIMENSION_KEYWORDS: &[&str] = &[
    "DIMENSIONAL_SIZE",
    "DIMENSIONAL_SIZE_WITH_PATH",
    "DIMENSIONAL_SIZE_WITH_DATUM_FEATURE",
    "ANGULAR_SIZE",
    "DIMENSIONAL_LOCATION",
    "DIMENSIONAL_LOCATION_WITH_PATH",
    "DIRECTED_DIMENSIONAL_LOCATION",
    "ANGULAR_LOCATION",
];

/// The attributes of a dimension entity, located in the instance.
struct Shape<'p> {
    kind: DimensionKind,
    name: Option<&'p Parameter>,
    features: Vec<Option<&'p Parameter>>,
    angle_selection: Option<&'p Parameter>,
    description: Option<&'p Parameter>,
    path: Option<&'p Parameter>,
    directed: bool,
}

/// Locate a dimension's attributes. A simple instance of a subtype carries
/// every inherited attribute in one flat list, so the layout depends on the
/// keyword; a complex instance carries each supertype's attributes in its
/// own segment.
fn shape(inst: &Instance) -> Option<Shape<'_>> {
    use DimensionKind::*;
    if !inst.is_complex() {
        let seg = inst.segments.first()?;
        let p = &seg.parameters;
        let g = |i: usize| p.get(i);
        let s = match seg.keyword.as_str() {
            "DIMENSIONAL_SIZE" => Shape {
                kind: Size,
                name: g(1),
                features: vec![g(0)],
                angle_selection: None,
                description: None,
                path: None,
                directed: false,
            },
            "DIMENSIONAL_SIZE_WITH_PATH" => Shape {
                kind: Size,
                name: g(1),
                features: vec![g(0)],
                angle_selection: None,
                description: None,
                path: g(2),
                directed: false,
            },
            // shape_aspect (name, description, of_shape, product_definitional) + (applies_to, name)
            "DIMENSIONAL_SIZE_WITH_DATUM_FEATURE" => Shape {
                kind: Size,
                name: g(5),
                features: vec![g(4)],
                angle_selection: None,
                description: None,
                path: None,
                directed: false,
            },
            "ANGULAR_SIZE" => Shape {
                kind: AngularSize,
                name: g(1),
                features: vec![g(0)],
                angle_selection: g(2),
                description: None,
                path: None,
                directed: false,
            },
            "DIMENSIONAL_LOCATION" => Shape {
                kind: Location,
                name: g(0),
                features: vec![g(2), g(3)],
                angle_selection: None,
                description: g(1),
                path: None,
                directed: false,
            },
            "DIRECTED_DIMENSIONAL_LOCATION" => Shape {
                kind: Location,
                name: g(0),
                features: vec![g(2), g(3)],
                angle_selection: None,
                description: g(1),
                path: None,
                directed: true,
            },
            "DIMENSIONAL_LOCATION_WITH_PATH" => Shape {
                kind: Location,
                name: g(0),
                features: vec![g(2), g(3)],
                angle_selection: None,
                description: g(1),
                path: g(4),
                directed: false,
            },
            "ANGULAR_LOCATION" => Shape {
                kind: AngularLocation,
                name: g(0),
                features: vec![g(2), g(3)],
                angle_selection: g(4),
                description: g(1),
                path: None,
                directed: false,
            },
            _ => return None,
        };
        return Some(s);
    }
    let path = [
        "DIMENSIONAL_SIZE_WITH_PATH",
        "DIMENSIONAL_LOCATION_WITH_PATH",
    ]
    .iter()
    .find_map(|k| inst.attr(k, 0));
    let directed = inst.has_type("DIRECTED_DIMENSIONAL_LOCATION");
    let s = if let Some(seg) = inst.segment("ANGULAR_LOCATION") {
        let p = &seg.parameters;
        Shape {
            kind: AngularLocation,
            name: p.first(),
            features: vec![p.get(2), p.get(3)],
            angle_selection: p.get(4),
            description: p.get(1),
            path,
            directed,
        }
    } else if let Some(seg) = inst
        .segment("DIMENSIONAL_LOCATION")
        .or_else(|| inst.segment("DIRECTED_DIMENSIONAL_LOCATION"))
    {
        let p = &seg.parameters;
        Shape {
            kind: Location,
            name: p.first(),
            features: vec![p.get(2), p.get(3)],
            angle_selection: None,
            description: p.get(1),
            path,
            directed,
        }
    } else if let Some(seg) = inst.segment("ANGULAR_SIZE") {
        let p = &seg.parameters;
        Shape {
            kind: AngularSize,
            name: p.get(1),
            features: vec![p.first()],
            angle_selection: p.get(2),
            description: None,
            path,
            directed,
        }
    } else {
        let seg = inst.segment("DIMENSIONAL_SIZE")?;
        let p = &seg.parameters;
        Shape {
            kind: Size,
            name: p.get(1),
            features: vec![p.first()],
            angle_selection: None,
            description: None,
            path,
            directed,
        }
    };
    Some(s)
}

/// Build a [`Dimension`] for every dimension entity in the exchange.
pub(crate) fn walk(ctx: &mut Ctx<'_>) {
    let targets: Vec<Id> = ctx
        .ex
        .instances()
        .filter(|i| DIMENSION_KEYWORDS.iter().any(|k| i.has_type(k)))
        .map(|i| i.id)
        .collect();
    tracing::debug!(count = targets.len(), "dimension entities");
    for id in targets {
        if let Some(d) = build(ctx, id) {
            ctx.dimensions.push(d);
        }
    }
}

fn build(ctx: &mut Ctx<'_>, id: Id) -> Option<Dimension> {
    let ex = ctx.ex;
    let inst = ex.get(id)?;
    let mut unmapped: Vec<Unmapped> = Vec::new();
    let mut source_refs = vec![source_ref(id)];

    let Some(Shape {
        kind,
        name,
        features: feature_params,
        angle_selection,
        description,
        path: path_param,
        directed,
    }) = shape(inst)
    else {
        ctx.warn(
            format!(
                "dimension #{id} ({}) has an unrecognised attribute layout",
                inst.type_key()
            ),
            Some(id),
        );
        return None;
    };

    let name = name.and_then(Parameter::as_str).unwrap_or_default();
    let subtype = DimensionSubtype::from_ap242_name(name);
    if let Some(desc) = description
        .and_then(Parameter::as_str)
        .filter(|s| !s.trim().is_empty())
    {
        unmapped.push(Unmapped {
            attribute: "description".into(),
            raw: desc.to_owned(),
        });
    }
    if let Some(sel) = angle_selection.and_then(Parameter::as_enum) {
        if sel != "EQUAL" {
            unmapped.push(Unmapped {
                attribute: "angle_selection".into(),
                raw: sel.to_owned(),
            });
        }
    }

    let mut features = Vec::new();
    for p in feature_params {
        match p.and_then(Parameter::as_ref) {
            Some(sa) => match feature_for(ctx, sa) {
                Some(f) => features.push(f),
                None => ctx.warn(
                    format!("dimension #{id}: feature #{sa} unresolved"),
                    Some(id),
                ),
            },
            None => ctx.warn(
                format!("dimension #{id}: missing feature reference"),
                Some(id),
            ),
        }
    }

    let path = path_param
        .and_then(Parameter::as_ref)
        .and_then(|sa| feature_for(ctx, sa));

    // Values.
    let mut value: Option<Measure> = None;
    let mut lower: Option<Measure> = None;
    let mut upper: Option<Measure> = None;
    let mut qualifier: Option<DimensionQualifier> = None;
    let mut modifiers: Vec<DimensionModifier> = Vec::new();
    let mut decimal_places: Option<u8> = None;

    for (dcr_id, sdr_id) in ctx.dimension_reprs.get(&id).cloned().unwrap_or_default() {
        ctx.consume(dcr_id);
        source_refs.push(source_ref(dcr_id));
        let Some(sdr) = ex.get(sdr_id) else {
            ctx.warn(
                format!("#{dcr_id} references undefined representation #{sdr_id}"),
                Some(dcr_id),
            );
            continue;
        };
        ctx.consume(sdr_id);
        source_refs.push(source_ref(sdr_id));
        let items: Vec<Id> = ["SHAPE_DIMENSION_REPRESENTATION", "REPRESENTATION"]
            .iter()
            .find_map(|k| sdr.attr(k, 1))
            .and_then(Parameter::as_list)
            .map(|l| l.iter().filter_map(Parameter::as_ref).collect())
            .unwrap_or_default();
        for item_id in items {
            let Some(item) = ex.get(item_id) else {
                ctx.warn(
                    format!("#{sdr_id} lists undefined item #{item_id}"),
                    Some(sdr_id),
                );
                continue;
            };
            if item.has_type("MEASURE_REPRESENTATION_ITEM") || item.has_type("MEASURE_WITH_UNIT") {
                let Some(q) = qualified_item(ctx, item) else {
                    continue;
                };
                ctx.consume(item_id);
                match q.name.trim().to_ascii_lowercase().as_str() {
                    "nominal value" | "" => {
                        if value.is_none() {
                            value = Some(q.measure.clone());
                        } else {
                            unmapped.push(Unmapped {
                                attribute: "extra_value".into(),
                                raw: item.to_string(),
                            });
                        }
                    }
                    "upper limit" => upper = Some(q.measure.clone()),
                    "lower limit" => lower = Some(q.measure.clone()),
                    other => unmapped.push(Unmapped {
                        attribute: format!("measure:{other}"),
                        raw: item.to_string(),
                    }),
                }
                if q.decimal_places.is_some() {
                    decimal_places = q.decimal_places;
                }
                for t in &q.type_qualifiers {
                    let mapped = match t.as_str() {
                        "maximum" => Some(DimensionQualifier::Maximum),
                        "minimum" => Some(DimensionQualifier::Minimum),
                        _ => None,
                    };
                    match (mapped, &qualifier) {
                        (Some(m), None) => qualifier = Some(m),
                        _ => unmapped.push(Unmapped {
                            attribute: "type_qualifier".into(),
                            raw: t.clone(),
                        }),
                    }
                }
            } else if item.has_type("DESCRIPTIVE_REPRESENTATION_ITEM") {
                ctx.consume(item_id);
                let p = item.parameters();
                let n = p
                    .first()
                    .and_then(Parameter::as_str)
                    .unwrap_or_default()
                    .trim();
                let d = p
                    .get(1)
                    .and_then(Parameter::as_str)
                    .unwrap_or_default()
                    .trim();
                match (
                    n.to_ascii_lowercase().as_str(),
                    d.to_ascii_lowercase().as_str(),
                ) {
                    ("dimensional note", "theoretical") => {
                        qualifier = Some(DimensionQualifier::Basic)
                    }
                    ("dimensional note", "auxiliary") => {
                        qualifier = Some(DimensionQualifier::Reference)
                    }
                    _ => unmapped.push(Unmapped {
                        attribute: format!("descriptive:{n}"),
                        raw: d.to_owned(),
                    }),
                }
            } else if item.has_type("COMPOUND_REPRESENTATION_ITEM") {
                ctx.consume(item_id);
                let elems: Vec<Id> = item
                    .parameters()
                    .get(1)
                    .map(|p| {
                        let mut v = Vec::new();
                        p.collect_refs(&mut v);
                        v
                    })
                    .unwrap_or_default();
                for eid in elems {
                    let Some(e) = ex.get(eid) else { continue };
                    ctx.consume(eid);
                    let d = e
                        .parameters()
                        .get(1)
                        .and_then(Parameter::as_str)
                        .unwrap_or_default();
                    modifiers.push(modifier_from_ap242(d));
                }
            } else {
                unmapped.push(Unmapped {
                    attribute: "representation_item".into(),
                    raw: item.to_string(),
                });
            }
        }
    }

    let limits = match (lower, upper) {
        (Some(lower), Some(upper)) => Some(Limits { lower, upper }),
        (Some(l), None) => {
            unmapped.push(Unmapped {
                attribute: "lower_limit_without_upper".into(),
                raw: l.canonical(),
            });
            None
        }
        (None, Some(u)) => {
            unmapped.push(Unmapped {
                attribute: "upper_limit_without_lower".into(),
                raw: u.canonical(),
            });
            None
        }
        (None, None) => None,
    };

    // Tolerance.
    let mut tolerance: Option<DimensionTolerance> = None;
    for pm_id in ctx.plus_minus.get(&id).cloned().unwrap_or_default() {
        let Some(pm) = ex.get(pm_id) else { continue };
        let Some(range) = ctx.deref(pm.parameters().first(), "range", pm_id) else {
            continue;
        };
        let t = if range.has_type("TOLERANCE_VALUE") {
            let p = range.parameters();
            let lo = ctx.deref(p.first(), "lower_bound", range.id);
            let hi = ctx.deref(p.get(1), "upper_bound", range.id);
            match (lo, hi) {
                (Some(lo), Some(hi)) => {
                    let lo_m = measure_with_unit(ctx, lo);
                    let hi_m = measure_with_unit(ctx, hi);
                    ctx.consume(lo.id);
                    ctx.consume(hi.id);
                    match (lo_m, hi_m) {
                        (Some(lower), Some(upper)) => {
                            Some(DimensionTolerance::PlusMinus { lower, upper })
                        }
                        _ => None,
                    }
                }
                _ => None,
            }
        } else if range.has_type("LIMITS_AND_FITS") {
            let p = range.parameters();
            let s = |i: usize| {
                p.get(i)
                    .and_then(Parameter::as_str)
                    .unwrap_or_default()
                    .to_owned()
            };
            if !s(3).trim().is_empty() {
                unmapped.push(Unmapped {
                    attribute: "limits_and_fits_source".into(),
                    raw: s(3),
                });
            }
            Some(DimensionTolerance::LimitsAndFits {
                form: s(0),
                zone: s(1),
                grade: s(2),
            })
        } else {
            ctx.warn(
                format!(
                    "#{pm_id} has unrecognised range #{} ({})",
                    range.id,
                    range.type_key()
                ),
                Some(pm_id),
            );
            None
        };
        // Consume the tolerance only when fully resolved, so a failure shows
        // up in the document's `unknown` list rather than vanishing.
        if let Some(t) = t {
            ctx.consume(pm_id);
            ctx.consume(range.id);
            source_refs.push(source_ref(pm_id));
            source_refs.push(source_ref(range.id));
            if tolerance.is_none() {
                tolerance = Some(t);
            } else {
                unmapped.push(Unmapped {
                    attribute: "extra_tolerance".into(),
                    raw: pm.to_string(),
                });
            }
        }
    }

    let tol_key = match &tolerance {
        Some(DimensionTolerance::PlusMinus { lower, upper }) => {
            format!("pm:{}/{}", lower.canonical(), upper.canonical())
        }
        Some(DimensionTolerance::LimitsAndFits { form, zone, grade }) => {
            format!("laf:{form}/{zone}/{grade}")
        }
        None => String::new(),
    };
    let limits_key = limits
        .as_ref()
        .map(|l| format!("{}..{}", l.lower.canonical(), l.upper.canonical()))
        .unwrap_or_default();
    let mods_key = modifiers
        .iter()
        .map(|m| m.as_str())
        .collect::<Vec<_>>()
        .join(",");
    let id_str = ctx.ids.make(
        "dim",
        &[
            kind.as_str(),
            subtype.as_str(),
            &value.as_ref().map(Measure::canonical).unwrap_or_default(),
            &limits_key,
            &tol_key,
            qualifier.as_ref().map(|q| q.as_str()).unwrap_or(""),
            &mods_key,
            &features.join(","),
            path.as_deref().unwrap_or(""),
            if directed { "directed" } else { "" },
        ],
    );

    ctx.consume(id);
    Some(Dimension {
        meta: Meta {
            id: id_str,
            source_refs,
            unmapped,
            ..Default::default()
        },
        kind,
        subtype,
        value,
        limits,
        tolerance,
        qualifier,
        modifiers,
        features,
        directed,
        orientation: None,
        path,
        decimal_places,
        text: None,
    })
}

/// Map an AP242 dimension modifier description (rec. practice tables 7
/// and 8) to a [`DimensionModifier`].
fn modifier_from_ap242(desc: &str) -> DimensionModifier {
    let d = desc.trim().to_ascii_lowercase();
    let canonical = match d.as_str() {
        "controlled radius" => "controlled_radius",
        "square" => "square",
        "statistical" | "statistical tolerance" => "statistical",
        "continuous feature" => "continuous_feature",
        "two point size" => "two_point_size",
        "local size defined by a sphere" | "local size" => "local_size",
        "least squares association criteria" => "least_squares",
        "maximum inscribed association criteria" => "maximum_inscribed",
        "minimum circumscribed association criteria" => "minimum_circumscribed",
        "circumference diameter calculated size" => "circumference_diameter",
        "area diameter calculated size" => "area_diameter",
        "volume diameter calculated size" => "volume_diameter",
        "maximum rank order size" => "maximum_size",
        "minimum rank order size" => "minimum_size",
        "average rank order size" => "average_size",
        "median rank order size" => "median_size",
        "mid range rank order size" => "mid_range_size",
        "range rank order size" => "range_of_sizes",
        "any restricted portion of feature" | "any restricted portion" => "any_restricted_portion",
        "any cross section" => "any_cross_section",
        "specific fixed cross section" => "specific_fixed_cross_section",
        "common tolerance" => "common_tolerance",
        "free state condition" | "free state" => "free_state",
        "envelope requirement" | "envelope" => "envelope",
        "between" => "between",
        _ => return DimensionModifier::Other(desc.to_owned()),
    };
    DimensionModifier::parse(canonical)
}
