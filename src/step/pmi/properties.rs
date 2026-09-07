//! Named property values (ADR 0007).
//!
//! A `property_definition` names a category and points at what the
//! property is about; its representation lists one or more named items
//! that carry the values. Both levels matter: the definition names the
//! family (`pmi validation property`, `PLM__Part_Number`) and each item
//! names the property itself (`affected area`, `Part_Number`).

use super::units::unit_name;
use super::{Ctx, PropertyRepr, source_ref};
use crate::model::{Property, PropertyKind, PropertyValue, Unmapped};
use crate::step::p21::{Id, Instance, Parameter};

/// Property definition names whose values describe how the PMI was
/// written rather than what the design is (CAx-IF validation properties).
const VALIDATION_CATEGORIES: &[&str] = &[
    "pmi validation property",
    "geometric validation property",
    "attribute validation property",
    "tessellated validation property",
];

/// Entities a property attaches to that mean "the whole part".
const PRODUCT_LEVEL: &[&str] = &[
    "PRODUCT_DEFINITION",
    "PRODUCT_DEFINITION_SHAPE",
    "PRODUCT_DEFINITION_FORMATION",
    "PRODUCT",
];

/// Build a [`Property`] for every value item of every property definition.
pub(crate) fn walk(ctx: &mut Ctx<'_>) {
    let targets: Vec<(Id, Vec<PropertyRepr>)> = ctx
        .property_reprs
        .iter()
        .map(|(target, reprs)| (*target, reprs.clone()))
        .collect();
    tracing::debug!(count = targets.len(), "property definition targets");

    for (target, reprs) in targets {
        let (applies_to, target_note) = resolve_target(ctx, target);
        for (pd_id, pdr_id, rep_id) in reprs {
            let ex = ctx.ex;
            let (Some(pd), Some(rep)) = (ex.get(pd_id), ex.get(rep_id)) else {
                continue;
            };
            // Datum target parameters are the datum walker's business.
            if rep.has_type("SHAPE_REPRESENTATION_WITH_PARAMETERS") {
                continue;
            }
            let category = pd
                .parameters()
                .first()
                .and_then(Parameter::as_str)
                .map(str::trim)
                .unwrap_or_default()
                .to_owned();
            let kind = if VALIDATION_CATEGORIES
                .iter()
                .any(|v| category.eq_ignore_ascii_case(v))
            {
                PropertyKind::Validation
            } else {
                PropertyKind::User
            };
            let rep_name = rep
                .parameters()
                .first()
                .and_then(Parameter::as_str)
                .map(str::trim)
                .unwrap_or_default()
                .to_owned();
            let mut items = Vec::new();
            if let Some(p) = rep.parameters().get(1) {
                p.collect_refs(&mut items);
            }
            let mut built = 0usize;
            for item_id in items {
                let Some(item) = ex.get(item_id) else {
                    ctx.warn(
                        format!(
                            "property representation #{rep_id} lists undefined item #{item_id}"
                        ),
                        Some(rep_id),
                    );
                    continue;
                };
                let Some((name, value)) = value_of(ctx, item) else {
                    continue;
                };
                let ex = ctx.ex;
                let name = [name.trim(), rep_name.as_str(), category.as_str()]
                    .into_iter()
                    .find(|n| !n.is_empty())
                    .unwrap_or_default()
                    .to_owned();
                let category = (category != name).then(|| category.clone());
                let mut unmapped = Vec::new();
                if let Some(note) = &target_note {
                    unmapped.push(Unmapped {
                        attribute: "applies_to".into(),
                        raw: note.clone(),
                    });
                }
                let id = ctx.ids.make(
                    "prop",
                    &[
                        category.as_deref().unwrap_or(""),
                        &name,
                        applies_to.as_deref().unwrap_or(""),
                        &value.canonical(),
                    ],
                );
                ctx.consume(item_id);
                let _ = ex;
                ctx.properties.push(Property {
                    id,
                    name,
                    category,
                    kind,
                    value,
                    applies_to: applies_to.clone(),
                    unmapped,
                    source_refs: vec![
                        source_ref(pd_id),
                        source_ref(pdr_id),
                        source_ref(rep_id),
                        source_ref(item_id),
                    ],
                });
                built += 1;
            }
            if built > 0 {
                ctx.consume(pd_id);
                ctx.consume(pdr_id);
                ctx.consume(rep_id);
            }
        }
    }
}

/// The record a property is about: `None` for the whole part, plus a note
/// when the target is something the model has no record for.
fn resolve_target(ctx: &mut Ctx<'_>, target: Id) -> (Option<String>, Option<String>) {
    let Some(inst) = ctx.ex.get(target) else {
        return (None, Some(format!("undefined {}", source_ref(target))));
    };
    if inst.type_names().any(|t| PRODUCT_LEVEL.contains(&t)) {
        return (None, None);
    }
    let found = ctx
        .tolerance_ids
        .get(&target)
        .cloned()
        .flatten()
        .or_else(|| ctx.dimension_ids.get(&target).cloned())
        .or_else(|| ctx.datum_ids.get(&target).cloned().flatten())
        .or_else(|| ctx.datum_system_ids.get(&target).cloned().flatten())
        .or_else(|| ctx.feature_ids.get(&target).cloned().flatten());
    match found {
        Some(id) => (Some(id), None),
        None => (
            None,
            Some(format!("{} ({})", source_ref(target), inst.type_key())),
        ),
    }
}

/// The name and typed value of a representation item, if it carries one.
fn value_of(ctx: &mut Ctx<'_>, item: &Instance) -> Option<(String, PropertyValue)> {
    let named = |keyword: &str| -> String {
        item.attr(keyword, 0)
            .or_else(|| item.attr("REPRESENTATION_ITEM", 0))
            .and_then(Parameter::as_str)
            .unwrap_or_default()
            .to_owned()
    };

    if item.has_type("DESCRIPTIVE_REPRESENTATION_ITEM") {
        let value = item
            .attr("DESCRIPTIVE_REPRESENTATION_ITEM", 1)
            .and_then(Parameter::as_str)?
            .to_owned();
        return Some((
            named("DESCRIPTIVE_REPRESENTATION_ITEM"),
            PropertyValue::Text { value },
        ));
    }
    if item.has_type("INTEGER_REPRESENTATION_ITEM") {
        // Written as `1.` by some exporters, so read it as a number.
        let v = item
            .attr("INTEGER_REPRESENTATION_ITEM", 1)
            .and_then(Parameter::as_f64)?;
        return Some((
            named("INTEGER_REPRESENTATION_ITEM"),
            PropertyValue::Integer { value: v as i64 },
        ));
    }
    if item.has_type("REAL_REPRESENTATION_ITEM") {
        let v = item
            .attr("REAL_REPRESENTATION_ITEM", 1)
            .and_then(Parameter::as_f64)?;
        return Some((
            named("REAL_REPRESENTATION_ITEM"),
            PropertyValue::Number { value: v },
        ));
    }
    if item.has_type("BOOLEAN_REPRESENTATION_ITEM") {
        let v = item
            .attr("BOOLEAN_REPRESENTATION_ITEM", 1)
            .and_then(Parameter::as_enum)?;
        return Some((
            named("BOOLEAN_REPRESENTATION_ITEM"),
            PropertyValue::Boolean { value: v == "T" },
        ));
    }
    if item.has_type("VALUE_REPRESENTATION_ITEM") {
        let v = item
            .attr("VALUE_REPRESENTATION_ITEM", 1)
            .and_then(Parameter::as_f64)?;
        return Some((
            named("VALUE_REPRESENTATION_ITEM"),
            PropertyValue::Number { value: v },
        ));
    }
    if item.has_type("MEASURE_REPRESENTATION_ITEM") {
        return measure_value(ctx, item);
    }
    None
}

/// A measure item in either instance form: the simple
/// `MEASURE_REPRESENTATION_ITEM(name, TYPE(value), unit)` or the complex
/// instance whose `MEASURE_WITH_UNIT` segment carries value and unit.
fn measure_value(ctx: &mut Ctx<'_>, item: &Instance) -> Option<(String, PropertyValue)> {
    let seg = item.segment("MEASURE_REPRESENTATION_ITEM")?;
    let (value, unit_ref) = if seg.parameters.len() >= 3 {
        (
            seg.parameters.get(1).and_then(Parameter::as_f64),
            seg.parameters.get(2).and_then(Parameter::as_ref),
        )
    } else {
        let m = item.segment("MEASURE_WITH_UNIT")?;
        (
            m.parameters.first().and_then(Parameter::as_f64),
            m.parameters.get(1).and_then(Parameter::as_ref),
        )
    };
    let value = value?;
    let name = if seg.parameters.len() >= 3 {
        seg.parameters.first().and_then(Parameter::as_str)
    } else {
        item.attr("REPRESENTATION_ITEM", 0)
            .and_then(Parameter::as_str)
    }
    .unwrap_or_default()
    .to_owned();
    let unit = unit_ref.and_then(|u| unit_name(ctx, u)).unwrap_or_default();
    Some((name, PropertyValue::Measure { value, unit }))
}
