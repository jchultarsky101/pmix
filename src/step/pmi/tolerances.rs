//! Geometric tolerances (feature control frames).

use super::features::feature_for;
use super::measures::{measure_with_unit, qualified_item};
use super::{Ctx, source_ref};
use crate::model::{
    GeometricTolerance, Measure, Meta, ToleranceKind, ToleranceModifier, UnitBasis, Unmapped, Zone,
    ZoneForm,
};
use crate::step::p21::{Id, Instance, Parameter};

/// The fifteen tolerance type keywords, with their canonical kinds.
const KINDS: &[(&str, &str)] = &[
    ("ANGULARITY_TOLERANCE", "angularity"),
    ("CIRCULAR_RUNOUT_TOLERANCE", "circular_runout"),
    ("COAXIALITY_TOLERANCE", "coaxiality"),
    ("CONCENTRICITY_TOLERANCE", "concentricity"),
    ("CYLINDRICITY_TOLERANCE", "cylindricity"),
    ("FLATNESS_TOLERANCE", "flatness"),
    ("LINE_PROFILE_TOLERANCE", "line_profile"),
    ("PARALLELISM_TOLERANCE", "parallelism"),
    ("PERPENDICULARITY_TOLERANCE", "perpendicularity"),
    ("POSITION_TOLERANCE", "position"),
    ("ROUNDNESS_TOLERANCE", "roundness"),
    ("STRAIGHTNESS_TOLERANCE", "straightness"),
    ("SURFACE_PROFILE_TOLERANCE", "surface_profile"),
    ("SYMMETRY_TOLERANCE", "symmetry"),
    ("TOTAL_RUNOUT_TOLERANCE", "total_runout"),
];

/// Is this instance a geometric tolerance, in either instance form?
pub(crate) fn is_tolerance(inst: &Instance) -> bool {
    inst.has_type("GEOMETRIC_TOLERANCE") || KINDS.iter().any(|(k, _)| inst.has_type(k))
}

/// Build a [`GeometricTolerance`] for every tolerance in the exchange.
/// Upper segments of composite frames are built before their lower
/// segments so the lower's id can include its parent.
pub(crate) fn walk(ctx: &mut Ctx<'_>) {
    let ex = ctx.ex;
    let mut targets: Vec<Id> = ex
        .instances()
        .filter(|i| is_tolerance(i))
        .map(|i| i.id)
        .collect();
    tracing::debug!(count = targets.len(), "tolerance entities");
    let lowers: std::collections::HashSet<Id> = ctx
        .tolerance_relationships
        .iter()
        .map(|(_, _, related, _)| *related)
        .collect();
    targets.sort_by_key(|id| lowers.contains(id));
    for id in targets {
        build(ctx, id);
    }
}

/// Attributes of the `geometric_tolerance` supertype plus the datum set,
/// located in either instance form.
struct Core<'p> {
    magnitude: Option<&'p Parameter>,
    toleranced: Option<&'p Parameter>,
    datum_refs: Option<&'p Parameter>,
}

fn core(inst: &Instance) -> Core<'_> {
    if let Some(seg) = inst.segment("GEOMETRIC_TOLERANCE") {
        let p = &seg.parameters;
        return Core {
            magnitude: p.get(2),
            toleranced: p.get(3),
            datum_refs: inst.attr("GEOMETRIC_TOLERANCE_WITH_DATUM_REFERENCE", 0),
        };
    }
    // Simple instance of a subtype: (name, description, magnitude,
    // toleranced_shape_aspect[, datum_system]).
    let p = inst.parameters();
    Core {
        magnitude: p.get(2),
        toleranced: p.get(3),
        datum_refs: p.get(4),
    }
}

fn build(ctx: &mut Ctx<'_>, id: Id) {
    let ex = ctx.ex;
    let Some(inst) = ex.get(id) else { return };
    let mut unmapped: Vec<Unmapped> = Vec::new();
    let mut source_refs = vec![source_ref(id)];

    let kind = KINDS
        .iter()
        .find(|(k, _)| inst.has_type(k))
        .map(|(_, c)| ToleranceKind::parse(c))
        .unwrap_or_else(|| ToleranceKind::Other(inst.type_key().to_ascii_lowercase()));
    if matches!(kind, ToleranceKind::Other(_)) {
        ctx.warn(format!("tolerance #{id} has no recognised type"), Some(id));
    }
    let Core {
        magnitude,
        toleranced,
        datum_refs,
    } = core(inst);

    // Magnitude.
    let mut value = None;
    let mut decimal_places = None;
    if let Some(m) = ctx.deref(magnitude, "magnitude", id) {
        if m.has_type("MEASURE_REPRESENTATION_ITEM") {
            if let Some(q) = qualified_item(ctx, m) {
                value = Some(q.measure);
                decimal_places = q.decimal_places;
                for t in q.type_qualifiers {
                    unmapped.push(Unmapped {
                        attribute: "type_qualifier".into(),
                        raw: t,
                    });
                }
            }
        } else {
            value = measure_with_unit(ctx, m);
        }
        if value.is_some() {
            ctx.consume(m.id);
            source_refs.push(source_ref(m.id));
        }
    }

    // Toleranced feature.
    let mut features = Vec::new();
    match toleranced.and_then(Parameter::as_ref) {
        Some(sa) => match feature_for(ctx, sa) {
            Some(f) => features.push(f),
            None => ctx.warn(
                format!("tolerance #{id}: feature #{sa} unresolved"),
                Some(id),
            ),
        },
        None => ctx.warn(
            format!("tolerance #{id}: missing toleranced shape aspect"),
            Some(id),
        ),
    }

    // Datum system.
    let mut datum_system = None;
    let mut refs = Vec::new();
    if let Some(p) = datum_refs {
        p.collect_refs(&mut refs);
    }
    for r in refs {
        match ctx.datum_system_ids.get(&r).cloned().flatten() {
            Some(ds) if datum_system.is_none() => datum_system = Some(ds),
            Some(ds) => unmapped.push(Unmapped {
                attribute: "extra_datum_system".into(),
                raw: ds,
            }),
            None => {
                let raw = ex
                    .get(r)
                    .map(ToString::to_string)
                    .unwrap_or_else(|| source_ref(r));
                unmapped.push(Unmapped {
                    attribute: "datum_reference".into(),
                    raw,
                });
            }
        }
    }

    // Modifiers and the mix-in supertypes.
    let mut modifiers: Vec<ToleranceModifier> = inst
        .attr("GEOMETRIC_TOLERANCE_WITH_MODIFIERS", 0)
        .and_then(Parameter::as_list)
        .map(|l| {
            l.iter()
                .filter_map(Parameter::as_enum)
                .map(tolerance_modifier)
                .collect()
        })
        .unwrap_or_default();
    let maximum_value = inst
        .attr("GEOMETRIC_TOLERANCE_WITH_MAXIMUM_TOLERANCE", 0)
        .and_then(|p| resolve_measure(ctx, p, "maximum_upper_tolerance", id, &mut source_refs));
    let unequally_disposed = inst
        .attr("UNEQUALLY_DISPOSED_GEOMETRIC_TOLERANCE", 0)
        .and_then(|p| resolve_measure(ctx, p, "displacement", id, &mut source_refs));
    if unequally_disposed.is_some() && !modifiers.contains(&ToleranceModifier::UnequallyDisposed) {
        modifiers.push(ToleranceModifier::UnequallyDisposed);
    }
    let unit_basis = inst
        .attr("GEOMETRIC_TOLERANCE_WITH_DEFINED_UNIT", 0)
        .and_then(|p| resolve_measure(ctx, p, "unit_size", id, &mut source_refs))
        .map(|length| {
            let area = inst.segment("GEOMETRIC_TOLERANCE_WITH_DEFINED_AREA_UNIT");
            let area_type = area
                .and_then(|s| s.parameters.first())
                .and_then(Parameter::as_enum)
                .map(str::to_ascii_lowercase);
            let width = area
                .and_then(|s| s.parameters.get(1))
                .and_then(|p| resolve_measure(ctx, p, "second_unit_size", id, &mut source_refs));
            UnitBasis {
                length,
                width,
                area_type,
            }
        });

    // Zone.
    let mut zone: Option<Zone> = None;
    for zid in ctx.zones.get(&id).cloned().unwrap_or_default() {
        let Some(z) = ex.get(zid) else { continue };
        let mut zone_rec = zone.take().unwrap_or_default();
        if let Some(form) = ctx.deref(z.parameters().get(5), "form", zid) {
            let name = form
                .parameters()
                .first()
                .and_then(Parameter::as_str)
                .unwrap_or_default();
            let f = zone_form(name);
            match &zone_rec.form {
                None => zone_rec.form = Some(f),
                Some(existing) if *existing != f => unmapped.push(Unmapped {
                    attribute: "extra_zone_form".into(),
                    raw: name.to_owned(),
                }),
                _ => {}
            }
            ctx.consume(form.id);
        }
        for did in ctx.zone_definitions.get(&zid).cloned().unwrap_or_default() {
            let Some(d) = ex.get(did) else { continue };
            if d.has_type("PROJECTED_ZONE_DEFINITION") {
                let p = d.parameters();
                zone_rec.projected = p.get(3).and_then(|m| {
                    resolve_measure(ctx, m, "projected_length", did, &mut source_refs)
                });
                if let Some(end) = p.get(2).and_then(Parameter::as_ref) {
                    unmapped.push(Unmapped {
                        attribute: "projection_end".into(),
                        raw: source_ref(end),
                    });
                }
                ctx.consume(did);
            } else if d.has_type("RUNOUT_ZONE_DEFINITION") {
                if let Some(o) = ctx.deref(d.parameters().get(2), "orientation", did) {
                    zone_rec.runout_angle = o
                        .parameters()
                        .first()
                        .and_then(|m| resolve_measure(ctx, m, "angle", o.id, &mut source_refs));
                    ctx.consume(o.id);
                }
                ctx.consume(did);
            } else {
                unmapped.push(Unmapped {
                    attribute: "zone_definition".into(),
                    raw: d.to_string(),
                });
            }
        }
        ctx.consume(zid);
        source_refs.push(source_ref(zid));
        zone = Some(zone_rec);
    }

    // Composite relationship: this tolerance is the lower segment.
    let mut composite_of = None;
    for (rel_id, relating, related, name) in ctx.tolerance_relationships.clone() {
        if related != id {
            continue;
        }
        if name.eq_ignore_ascii_case("composite") {
            match ctx.tolerance_ids.get(&relating).cloned().flatten() {
                Some(upper) => {
                    composite_of = Some(upper);
                    ctx.consume(rel_id);
                    source_refs.push(source_ref(rel_id));
                }
                None => ctx.warn(
                    format!("#{rel_id}: composite parent #{relating} was not built"),
                    Some(rel_id),
                ),
            }
        } else {
            ctx.consume(rel_id);
            unmapped.push(Unmapped {
                attribute: "tolerance_relationship".into(),
                raw: format!("{name} -> {}", source_ref(relating)),
            });
        }
    }

    let text = render(
        ctx,
        &kind,
        value.as_ref(),
        zone.as_ref(),
        &modifiers,
        datum_system.as_deref(),
    );
    let mods_key: Vec<&str> = modifiers.iter().map(|m| m.as_str()).collect();
    let rid = ctx.ids.make(
        "tol",
        &[
            kind.as_str(),
            &value.as_ref().map(Measure::canonical).unwrap_or_default(),
            &zone
                .as_ref()
                .map(|z| {
                    format!(
                        "{}/{}/{}",
                        z.form.as_ref().map(|f| f.as_str()).unwrap_or(""),
                        z.projected
                            .as_ref()
                            .map(Measure::canonical)
                            .unwrap_or_default(),
                        z.runout_angle
                            .as_ref()
                            .map(Measure::canonical)
                            .unwrap_or_default()
                    )
                })
                .unwrap_or_default(),
            &unequally_disposed
                .as_ref()
                .map(Measure::canonical)
                .unwrap_or_default(),
            &maximum_value
                .as_ref()
                .map(Measure::canonical)
                .unwrap_or_default(),
            &unit_basis
                .as_ref()
                .map(|u| {
                    format!(
                        "{}/{}/{}",
                        u.length.canonical(),
                        u.width.as_ref().map(Measure::canonical).unwrap_or_default(),
                        u.area_type.as_deref().unwrap_or("")
                    )
                })
                .unwrap_or_default(),
            &mods_key.join(","),
            datum_system.as_deref().unwrap_or(""),
            &features.join(","),
            composite_of.as_deref().unwrap_or(""),
        ],
    );
    ctx.consume(id);
    ctx.tolerance_ids.insert(id, Some(rid.clone()));
    ctx.tolerances.push(GeometricTolerance {
        meta: Meta {
            id: rid,
            source_refs,
            unmapped,
            ..Default::default()
        },
        kind,
        value,
        zone,
        unequally_disposed,
        maximum_value,
        unit_basis,
        modifiers,
        datum_system,
        features,
        affected_plane: None,
        composite_of,
        decimal_places,
        text: Some(text),
    });
}

/// Resolve a parameter that references a measure (simple or qualified),
/// consuming it.
fn resolve_measure(
    ctx: &mut Ctx<'_>,
    p: &Parameter,
    what: &str,
    from: Id,
    source_refs: &mut Vec<String>,
) -> Option<Measure> {
    let m = ctx.deref(Some(p), what, from)?;
    let measure = if m.has_type("MEASURE_REPRESENTATION_ITEM") {
        qualified_item(ctx, m).map(|q| q.measure)
    } else {
        measure_with_unit(ctx, m)
    };
    if measure.is_some() {
        ctx.consume(m.id);
        source_refs.push(source_ref(m.id));
    }
    measure
}

/// Map an AP242 `geometric_tolerance_modifier` enumeration value.
fn tolerance_modifier(e: &str) -> ToleranceModifier {
    let canonical = match e {
        "MAXIMUM_MATERIAL_REQUIREMENT" => "maximum_material",
        "LEAST_MATERIAL_REQUIREMENT" => "least_material",
        "FREE_STATE" => "free_state",
        "TANGENT_PLANE" => "tangent_plane",
        "STATISTICAL_TOLERANCE" => "statistical",
        "COMMON_ZONE" => "common_zone",
        "ANY_CROSS_SECTION" => "any_cross_section",
        "CIRCLE" | "CIRCLE_A" => "circle",
        "RECIPROCITY_REQUIREMENT" => "reciprocity",
        "SEPARATE_REQUIREMENT" => "separate_requirement",
        "EACH_RADIAL_ELEMENT" => "each_radial_element",
        "LINE_ELEMENT" => "line_element",
        "NOT_CONVEX" => "not_convex",
        "MAJOR_DIAMETER" => "major_diameter",
        "MINOR_DIAMETER" => "minor_diameter",
        "PITCH_DIAMETER" => "pitch_diameter",
        "UNEQUALLY_DISPOSED" => "unequally_disposed",
        "ALL_AROUND" => "all_around",
        "ALL_OVER" => "all_over",
        other => return ToleranceModifier::Other(other.to_ascii_lowercase()),
    };
    ToleranceModifier::parse(canonical)
}

/// Map an AP242 `tolerance_zone_form.name` (rec. practice table 13).
fn zone_form(name: &str) -> ZoneForm {
    let canonical = match name.trim().to_ascii_lowercase().as_str() {
        "cylindrical or circular" => "cylindrical_or_circular",
        "spherical" => "spherical",
        "within a circle" => "within_circle",
        "between two concentric circles" => "between_two_concentric_circles",
        "between two equidistant curves" => "between_two_equidistant_curves",
        "within a cylinder" => "within_cylinder",
        "between two coaxial cylinders" => "between_two_coaxial_cylinders",
        "between two equidistant surfaces" => "between_two_equidistant_surfaces",
        "non uniform" => "non_uniform",
        _ => return ZoneForm::Other(name.to_owned()),
    };
    ZoneForm::parse(canonical)
}

/// Human-readable frame, e.g. `⌖ ⌀0.75 Ⓜ | A | B Ⓜ | C`.
fn render(
    ctx: &Ctx<'_>,
    kind: &ToleranceKind,
    value: Option<&Measure>,
    zone: Option<&Zone>,
    modifiers: &[ToleranceModifier],
    datum_system: Option<&str>,
) -> String {
    let symbol = match kind {
        ToleranceKind::Angularity => "∠",
        ToleranceKind::CircularRunout => "↗",
        ToleranceKind::Coaxiality | ToleranceKind::Concentricity => "◎",
        ToleranceKind::Cylindricity => "⌭",
        ToleranceKind::Flatness => "⏥",
        ToleranceKind::LineProfile => "⌒",
        ToleranceKind::Parallelism => "∥",
        ToleranceKind::Perpendicularity => "⟂",
        ToleranceKind::Position => "⌖",
        ToleranceKind::Roundness => "○",
        ToleranceKind::Straightness => "⏤",
        ToleranceKind::SurfaceProfile => "⌓",
        ToleranceKind::Symmetry => "⌯",
        ToleranceKind::TotalRunout => "⌰",
        ToleranceKind::Other(o) => o.as_str(),
    };
    let mut parts = vec![symbol.to_owned()];
    let mut val = String::new();
    match zone.and_then(|z| z.form.as_ref()) {
        Some(
            ZoneForm::CylindricalOrCircular | ZoneForm::WithinCylinder | ZoneForm::WithinCircle,
        ) => val.push('⌀'),
        Some(ZoneForm::Spherical) => val.push_str("S⌀"),
        _ => {}
    }
    if let Some(v) = value {
        val.push_str(&format!("{}", v.value));
    }
    if let Some(p) = zone.and_then(|z| z.projected.as_ref()) {
        val.push_str(&format!(" Ⓟ{}", p.value));
    }
    if !val.is_empty() {
        parts.push(val);
    }
    for m in modifiers {
        parts.push(
            match m {
                ToleranceModifier::MaximumMaterial => "Ⓜ",
                ToleranceModifier::LeastMaterial => "Ⓛ",
                ToleranceModifier::FreeState => "Ⓕ",
                ToleranceModifier::TangentPlane => "Ⓣ",
                ToleranceModifier::Statistical => "ⓈⓉ",
                ToleranceModifier::UnequallyDisposed => "Ⓤ",
                other => other.as_str(),
            }
            .to_owned(),
        );
    }
    let mut text = parts.join(" ");
    if let Some(ds) = datum_system {
        if let Some(sys) = ctx.datum_systems.iter().find(|s| s.meta.id == ds) {
            let rendered = sys
                .text
                .replace("(M)", " Ⓜ")
                .replace("(L)", " Ⓛ")
                .replace('|', " | ");
            text.push_str(" | ");
            text.push_str(&rendered);
        }
    }
    text
}
