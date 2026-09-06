//! Datums, datum targets, and datum systems (reference frames).

use std::collections::HashMap;

use super::features::feature_for;
use super::measures::{measure_with_unit, qualified_item};
use super::{Ctx, source_ref};
use crate::model::{
    Compartment, Datum, DatumModifier, DatumRef, DatumSystem, DatumTarget, DatumTargetKind,
    Direction, Measure, Meta, Placement, Unmapped,
};
use crate::step::p21::{Id, Instance, Parameter};

/// Build every [`Datum`] (with its targets) and then every [`DatumSystem`].
pub(crate) fn walk(ctx: &mut Ctx<'_>) {
    let ex = ctx.ex;
    let datums: Vec<Id> = ex.of_type("DATUM").map(|i| i.id).collect();
    tracing::debug!(count = datums.len(), "datum entities");
    for id in datums {
        build_datum(ctx, id);
    }
    let systems: Vec<Id> = ex.of_type("DATUM_SYSTEM").map(|i| i.id).collect();
    tracing::debug!(count = systems.len(), "datum systems");
    for id in systems {
        build_system(ctx, id);
    }
}

fn build_datum(ctx: &mut Ctx<'_>, id: Id) {
    let ex = ctx.ex;
    let Some(inst) = ex.get(id) else { return };
    let mut source_refs = vec![source_ref(id)];
    let label = inst
        .attr("DATUM", 4)
        .and_then(Parameter::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_owned();
    if label.is_empty() {
        ctx.warn(format!("datum #{id} has no identification"), Some(id));
    }

    let mut features = Vec::new();
    let mut targets = Vec::new();
    for (rel_id, relating) in ctx.datum_links.get(&id).cloned().unwrap_or_default() {
        let Some(rel_inst) = ex.get(relating) else {
            continue;
        };
        if rel_inst.has_type("DATUM_TARGET") || rel_inst.has_type("PLACED_DATUM_TARGET_FEATURE") {
            if let Some(t) = build_target(ctx, rel_inst, &label) {
                targets.push(t);
                ctx.consume(rel_id);
                source_refs.push(source_ref(rel_id));
            }
        } else if let Some(f) = feature_for(ctx, relating) {
            features.push(f);
            ctx.consume(rel_id);
            source_refs.push(source_ref(rel_id));
        }
    }
    targets.sort_by(|a, b| a.label.cmp(&b.label));

    let mut feature_keys = features.clone();
    feature_keys.sort();
    let target_keys: Vec<String> = targets
        .iter()
        .map(|t| format!("{}:{}", t.label, t.kind))
        .collect();
    let rid = ctx.ids.make(
        "datum",
        &[&label, &feature_keys.join(","), &target_keys.join(",")],
    );
    ctx.consume(id);
    ctx.datum_ids.insert(id, Some(rid.clone()));
    ctx.datum_labels.insert(rid.clone(), label.clone());
    ctx.datums.push(Datum {
        meta: Meta {
            id: rid,
            source_refs,
            ..Default::default()
        },
        label,
        features,
        targets,
    });
}

fn build_target(ctx: &mut Ctx<'_>, inst: &Instance, datum_label: &str) -> Option<DatumTarget> {
    let ex = ctx.ex;
    let seg = inst
        .segment("PLACED_DATUM_TARGET_FEATURE")
        .or_else(|| inst.segment("DATUM_TARGET"))?;
    let p = &seg.parameters;
    let description = p
        .get(1)
        .and_then(Parameter::as_str)
        .unwrap_or_default()
        .trim();
    let target_id = p
        .get(4)
        .and_then(Parameter::as_str)
        .unwrap_or_default()
        .trim();
    let kind = match description.to_ascii_lowercase().as_str() {
        "point" => DatumTargetKind::Point,
        "line" => DatumTargetKind::Line,
        "rectangle" => DatumTargetKind::Rectangle,
        "circle" => DatumTargetKind::Circle,
        "circular curve" | "circular line" => DatumTargetKind::CircularLine,
        "area" => DatumTargetKind::Area,
        other => DatumTargetKind::Other(other.to_owned()),
    };
    let mut unmapped = Vec::new();
    let mut target = DatumTarget {
        label: format!("{datum_label}{target_id}"),
        kind,
        diameter: None,
        length: None,
        width: None,
        placement: None,
        feature: None,
        unmapped: Vec::new(),
    };

    // The feature the target sits on, or the target's own geometry.
    let mut feature = None;
    for (rel_id, feat) in ctx
        .target_features
        .get(&inst.id)
        .cloned()
        .unwrap_or_default()
    {
        match feature_for(ctx, feat) {
            Some(f) if feature.is_none() => {
                feature = Some(f);
                ctx.consume(rel_id);
            }
            Some(f) => unmapped.push(Unmapped {
                attribute: "extra_feature".into(),
                raw: f,
            }),
            None => {}
        }
    }
    if feature.is_none() && ctx.geometry_usage.contains_key(&inst.id) {
        feature = feature_for(ctx, inst.id);
    }
    target.feature = feature;

    // Placement and size through property_definition ->
    // shape_definition_representation -> shape_representation_with_parameters.
    for (pd_id, sdr_id, rep_id) in ctx
        .property_reprs
        .get(&inst.id)
        .cloned()
        .unwrap_or_default()
    {
        let Some(rep) = ex.get(rep_id) else { continue };
        if !rep.has_type("SHAPE_REPRESENTATION_WITH_PARAMETERS") {
            continue;
        }
        ctx.consume(pd_id);
        ctx.consume(sdr_id);
        ctx.consume(rep_id);
        let items: Vec<Id> = rep
            .parameters()
            .get(1)
            .and_then(Parameter::as_list)
            .map(|l| l.iter().filter_map(Parameter::as_ref).collect())
            .unwrap_or_default();
        for item_id in items {
            let Some(item) = ex.get(item_id) else {
                continue;
            };
            if item.has_type("AXIS2_PLACEMENT_3D") {
                target.placement = placement(ex, item);
                ctx.consume(item_id);
            } else if item.has_type("MEASURE_REPRESENTATION_ITEM") {
                let Some(q) = qualified_item(ctx, item) else {
                    continue;
                };
                ctx.consume(item_id);
                match q.name.trim().to_ascii_lowercase().as_str() {
                    "target diameter" => target.diameter = Some(q.measure),
                    "target length" => target.length = Some(q.measure),
                    "target width" => target.width = Some(q.measure),
                    other => unmapped.push(Unmapped {
                        attribute: format!("measure:{other}"),
                        raw: item.to_string(),
                    }),
                }
            } else {
                unmapped.push(Unmapped {
                    attribute: "representation_item".into(),
                    raw: item.to_string(),
                });
            }
        }
    }
    target.unmapped = unmapped;
    ctx.consume(inst.id);
    Some(target)
}

/// Decode an `axis2_placement_3d`.
pub(crate) fn placement(ex: &crate::step::p21::Exchange, inst: &Instance) -> Option<Placement> {
    let p = inst.parameters();
    let point = ex.get(p.get(1)?.as_ref()?)?;
    let origin = triple(point.parameters().get(1)?)?;
    let dir = |i: usize| -> Option<Direction> {
        let d = ex.get(p.get(i)?.as_ref()?)?;
        let [x, y, z] = triple(d.parameters().get(1)?)?;
        Some(Direction { x, y, z })
    };
    Some(Placement {
        origin,
        axis: dir(2),
        ref_direction: dir(3),
    })
}

fn triple(p: &Parameter) -> Option<[f64; 3]> {
    let l = p.as_list()?;
    Some([
        l.first()?.as_f64()?,
        l.get(1)?.as_f64()?,
        l.get(2)?.as_f64()?,
    ])
}

fn build_system(ctx: &mut Ctx<'_>, id: Id) {
    let ex = ctx.ex;
    let Some(inst) = ex.get(id) else { return };
    let mut source_refs = vec![source_ref(id)];
    let mut unmapped = Vec::new();
    let constituents: Vec<Id> = inst
        .attr("DATUM_SYSTEM", 4)
        .and_then(Parameter::as_list)
        .map(|l| l.iter().filter_map(Parameter::as_ref).collect())
        .unwrap_or_default();

    let mut compartments = Vec::new();
    for cid in constituents {
        let Some(c) = ex.get(cid) else {
            ctx.warn(
                format!("datum system #{id} references undefined compartment #{cid}"),
                Some(id),
            );
            continue;
        };
        if !c.has_type("DATUM_REFERENCE_COMPARTMENT") {
            unmapped.push(Unmapped {
                attribute: "constituent".into(),
                raw: c.to_string(),
            });
            continue;
        }
        let p = c.parameters();
        let base = p.get(4);
        let mut compartment = Compartment {
            datums: Vec::new(),
            common: false,
        };
        match base {
            Some(Parameter::Reference(datum_id)) => {
                if let Some(r) = datum_ref(ctx, *datum_id, p.get(5), cid, &mut unmapped) {
                    compartment.datums.push(r);
                }
            }
            Some(Parameter::Typed { keyword, value }) if keyword == "COMMON_DATUM_LIST" => {
                compartment.common = true;
                let mut elems = Vec::new();
                value.collect_refs(&mut elems);
                for eid in elems {
                    let Some(e) = ex.get(eid) else { continue };
                    let ep = e.parameters();
                    if let Some(datum_id) = ep.get(4).and_then(Parameter::as_ref) {
                        if let Some(r) = datum_ref(ctx, datum_id, ep.get(5), eid, &mut unmapped) {
                            compartment.datums.push(r);
                            ctx.consume(eid);
                            source_refs.push(source_ref(eid));
                        }
                    }
                }
                // Modifiers on the compartment itself apply to the group.
                if let Some(mods) = p.get(5).filter(|m| !m.is_unset()) {
                    unmapped.push(Unmapped {
                        attribute: "common_datum_modifiers".into(),
                        raw: mods.to_string(),
                    });
                }
            }
            other => unmapped.push(Unmapped {
                attribute: "compartment_base".into(),
                raw: other.map(ToString::to_string).unwrap_or_default(),
            }),
        }
        ctx.consume(cid);
        source_refs.push(source_ref(cid));
        compartments.push(compartment);
    }

    let text = render(&compartments, &ctx.datum_labels);
    let datum_keys: Vec<String> = compartments
        .iter()
        .flat_map(|c| c.datums.iter().map(|d| d.datum.clone()))
        .collect();
    let rid = ctx.ids.make("dsys", &[&text, &datum_keys.join(",")]);
    ctx.consume(id);
    ctx.datum_system_ids.insert(id, Some(rid.clone()));
    ctx.datum_systems.push(DatumSystem {
        meta: Meta {
            id: rid,
            source_refs,
            unmapped,
            ..Default::default()
        },
        compartments,
        text,
    });
}

/// Resolve one datum reference with its modifiers.
fn datum_ref(
    ctx: &mut Ctx<'_>,
    datum_id: Id,
    modifiers: Option<&Parameter>,
    from: Id,
    unmapped: &mut Vec<Unmapped>,
) -> Option<DatumRef> {
    let Some(Some(datum)) = ctx.datum_ids.get(&datum_id).cloned() else {
        ctx.warn(
            format!("#{from} references #{datum_id}, which is not a datum"),
            Some(from),
        );
        return None;
    };
    let mut r = DatumRef {
        datum,
        modifiers: Vec::new(),
        modifier_values: Vec::new(),
    };
    let Some(list) = modifiers.and_then(Parameter::as_list) else {
        return Some(r);
    };
    for m in list {
        match m {
            Parameter::Typed { keyword, value } if keyword == "SIMPLE_DATUM_REFERENCE_MODIFIER" => {
                if let Some(e) = value.as_enum() {
                    r.modifiers.push(datum_modifier(e));
                }
            }
            Parameter::Reference(mid) => {
                let ex = ctx.ex;
                match ex.get(*mid) {
                    Some(mv) if mv.has_type("DATUM_REFERENCE_MODIFIER_WITH_VALUE") => {
                        let p = mv.parameters();
                        if let Some(e) = p.first().and_then(Parameter::as_enum) {
                            r.modifiers.push(datum_modifier(e));
                        }
                        if let Some(v) = p
                            .get(1)
                            .and_then(|v| ctx.deref(Some(v), "modifier_value", *mid))
                        {
                            if let Some(measure) = measure_with_unit(ctx, v) {
                                r.modifier_values.push(measure);
                                ctx.consume(v.id);
                            }
                        }
                        ctx.consume(*mid);
                    }
                    _ => unmapped.push(Unmapped {
                        attribute: "datum_modifier".into(),
                        raw: m.to_string(),
                    }),
                }
            }
            other => unmapped.push(Unmapped {
                attribute: "datum_modifier".into(),
                raw: other.to_string(),
            }),
        }
    }
    Some(r)
}

/// Map an AP242 datum reference modifier enumeration value.
fn datum_modifier(e: &str) -> DatumModifier {
    let canonical = match e {
        "MAXIMUM_MATERIAL_REQUIREMENT" => "maximum_material",
        "LEAST_MATERIAL_REQUIREMENT" => "least_material",
        "REGARDLESS_OF_FEATURE_SIZE" => "regardless_of_size",
        "BASIC" => "basic",
        "TRANSLATION" => "translation",
        "PROJECTED" => "projected",
        "ORIENTATION" => "orientation",
        "POINT" => "point",
        "LINE" => "line",
        "PLANE" => "plane",
        "FREE_STATE" => "free_state",
        "CONTACTING_FEATURE" => "contacting_feature",
        "DEGREE_OF_FREEDOM_CONSTRAINT_X" => "degree_of_freedom_x",
        "DEGREE_OF_FREEDOM_CONSTRAINT_Y" => "degree_of_freedom_y",
        "DEGREE_OF_FREEDOM_CONSTRAINT_Z" => "degree_of_freedom_z",
        "DEGREE_OF_FREEDOM_CONSTRAINT_U" => "degree_of_freedom_u",
        "DEGREE_OF_FREEDOM_CONSTRAINT_V" => "degree_of_freedom_v",
        "DEGREE_OF_FREEDOM_CONSTRAINT_W" => "degree_of_freedom_w",
        "DISTANCE" | "DISTANCE_VARIABLE" => "distance",
        "MAJOR_DIAMETER" => "major_diameter",
        "MINOR_DIAMETER" => "minor_diameter",
        "PITCH_DIAMETER" => "pitch_diameter",
        "ANY_CROSS_SECTION" => "any_cross_section",
        "ANY_LONGITUDINAL_SECTION" => "any_longitudinal_section",
        "CIRCULAR_OR_CYLINDRICAL" => "circular_or_cylindrical",
        "ALL_AROUND" => "all_around",
        "ALL_OVER" => "all_over",
        other => return DatumModifier::Other(other.to_ascii_lowercase()),
    };
    DatumModifier::parse(canonical)
}

/// Render a datum reference frame as `A|B(M)|C` (`A-B` for common datums).
fn render(compartments: &[Compartment], labels: &HashMap<String, String>) -> String {
    compartments
        .iter()
        .map(|c| {
            c.datums
                .iter()
                .map(|d| {
                    let label = labels.get(&d.datum).cloned().unwrap_or_else(|| "?".into());
                    let mods: Vec<String> = d
                        .modifiers
                        .iter()
                        .map(|m| match m {
                            DatumModifier::MaximumMaterial => "M".to_owned(),
                            DatumModifier::LeastMaterial => "L".to_owned(),
                            DatumModifier::RegardlessOfSize => "S".to_owned(),
                            other => other.as_str().to_owned(),
                        })
                        .collect();
                    let values: Vec<String> =
                        d.modifier_values.iter().map(Measure::canonical).collect();
                    let mut s = label;
                    if !mods.is_empty() || !values.is_empty() {
                        s.push('(');
                        s.push_str(&mods.join(","));
                        if !values.is_empty() {
                            s.push(' ');
                            s.push_str(&values.join(","));
                        }
                        s.push(')');
                    }
                    s
                })
                .collect::<Vec<_>>()
                .join("-")
        })
        .collect::<Vec<_>>()
        .join("|")
}
