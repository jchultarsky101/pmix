//! Unit resolution: STEP unit instances to canonical unit names.

use super::Ctx;
use crate::model::Units;
use crate::step::p21::{Id, Instance, Parameter};

/// Units from the file's `GLOBAL_UNIT_ASSIGNED_CONTEXT`.
pub(crate) fn document_units(ctx: &mut Ctx<'_>) -> Units {
    let ex = ctx.ex;
    let mut units = Units::default();
    for context in ex.of_type("GLOBAL_UNIT_ASSIGNED_CONTEXT") {
        let Some(list) = context
            .attr("GLOBAL_UNIT_ASSIGNED_CONTEXT", 0)
            .and_then(Parameter::as_list)
        else {
            continue;
        };
        for p in list {
            let Some(unit) = p.as_ref().and_then(|id| ex.get(id)) else {
                continue;
            };
            if unit.has_type("LENGTH_UNIT") && units.length.is_none() {
                units.length = unit_name(ctx, unit.id);
            } else if unit.has_type("PLANE_ANGLE_UNIT") && units.angle.is_none() {
                units.angle = unit_name(ctx, unit.id);
            }
        }
        if units.length.is_some() && units.angle.is_some() {
            break;
        }
    }
    if units.length.is_none() {
        ctx.warn("no length unit declared in a global unit context", None);
    }
    units
}

/// Canonical name for the unit instance `id`, memoised.
pub(crate) fn unit_name(ctx: &mut Ctx<'_>, id: Id) -> Option<String> {
    if let Some(cached) = ctx.unit_names.get(&id) {
        return cached.clone();
    }
    // Guard against a derived unit that reaches itself.
    ctx.unit_names.insert(id, None);
    let name = match ctx.ex.get(id) {
        Some(inst) => {
            let n = resolve(inst).or_else(|| derived(ctx, inst));
            if n.is_none() {
                ctx.warn(
                    format!("unrecognised unit #{id} ({})", inst.type_key()),
                    Some(id),
                );
            }
            n
        }
        None => {
            ctx.warn(format!("unit reference #{id} is undefined"), Some(id));
            None
        }
    };
    ctx.unit_names.insert(id, name.clone());
    name
}

/// A `derived_unit` such as mm² or kg/m³: each element's unit name
/// followed by its exponent when that is not 1, joined with `.`.
/// Millimetres squared becomes `mm2`, kilograms per cubic metre `kg.m-3`.
fn derived(ctx: &mut Ctx<'_>, inst: &Instance) -> Option<String> {
    let seg = inst.segment("DERIVED_UNIT")?;
    let ex = ctx.ex;
    let mut elements = Vec::new();
    seg.parameters.first()?.collect_refs(&mut elements);
    if elements.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    for e in elements {
        let element = ex.get(e)?;
        let p = element.parameters();
        let base = p.first().and_then(Parameter::as_ref)?;
        let name = unit_name(ctx, base)?;
        let exponent = p.get(1).and_then(Parameter::as_f64).unwrap_or(1.0);
        if (exponent - 1.0).abs() < 1e-9 {
            parts.push(name);
        } else if exponent.fract().abs() < 1e-9 {
            parts.push(format!("{name}{}", exponent as i64));
        } else {
            parts.push(format!("{name}{exponent}"));
        }
    }
    Some(parts.join("."))
}

fn resolve(inst: &Instance) -> Option<String> {
    if let Some(seg) = inst.segment("SI_UNIT") {
        let prefix = seg.parameters.first().and_then(Parameter::as_enum);
        let name = seg.parameters.get(1).and_then(Parameter::as_enum)?;
        let base = match name {
            "METRE" => "m",
            "RADIAN" => "rad",
            "STERADIAN" => "sr",
            "GRAM" => "g",
            "SECOND" => "s",
            "DEGREE_CELSIUS" => "degC",
            "KELVIN" => "K",
            "NEWTON" => "N",
            "PASCAL" => "Pa",
            other => return Some(other.to_ascii_lowercase()),
        };
        let prefix = match prefix {
            None => "",
            Some("MILLI") => "m",
            Some("CENTI") => "c",
            Some("DECI") => "d",
            Some("MICRO") => "u",
            Some("NANO") => "n",
            Some("KILO") => "k",
            Some("MEGA") => "M",
            Some(other) => return Some(format!("{}{}", other.to_ascii_lowercase(), base)),
        };
        return Some(format!("{prefix}{base}"));
    }
    if let Some(seg) = inst.segment("CONVERSION_BASED_UNIT") {
        let raw = seg.parameters.first().and_then(Parameter::as_str)?;
        let n = raw.trim().to_ascii_lowercase();
        let canonical = match n.as_str() {
            "millimetre" | "millimeter" | "millimetres" | "millimeters" | "mm" => "mm",
            "centimetre" | "centimeter" | "cm" => "cm",
            "metre" | "meter" | "m" => "m",
            "micrometre" | "micrometer" | "micron" | "um" => "um",
            "inch" | "inches" | "in" => "in",
            "foot" | "feet" | "ft" => "ft",
            "degree" | "degrees" | "deg" => "deg",
            "radian" | "radians" | "rad" => "rad",
            _ => n.as_str(),
        };
        return Some(canonical.to_owned());
    }
    None
}
