//! Measures with units and qualified representation items.

use super::Ctx;
use super::units::unit_name;
use crate::model::Measure;
use crate::step::p21::{Instance, Parameter};

/// A measure item from a representation, with its qualifiers.
#[derive(Debug, Clone)]
pub(crate) struct Qualified {
    /// `representation_item.name`, e.g. `"nominal value"`.
    pub name: String,
    pub measure: Measure,
    /// From `VALUE_FORMAT_TYPE_QUALIFIER('NR2 1.2')`.
    pub decimal_places: Option<u8>,
    /// From `TYPE_QUALIFIER('maximum')` and the like.
    pub type_qualifiers: Vec<String>,
}

/// Segments that carry `(value_component, unit_component)`.
const MEASURE_SEGMENTS: &[&str] = &[
    "MEASURE_WITH_UNIT",
    "LENGTH_MEASURE_WITH_UNIT",
    "PLANE_ANGLE_MEASURE_WITH_UNIT",
    "AREA_MEASURE_WITH_UNIT",
    "MASS_MEASURE_WITH_UNIT",
    "RATIO_MEASURE_WITH_UNIT",
];

/// The value and unit of a `measure_with_unit` (simple or complex).
pub(crate) fn measure_with_unit(ctx: &mut Ctx<'_>, inst: &Instance) -> Option<Measure> {
    let seg = MEASURE_SEGMENTS
        .iter()
        .filter_map(|k| inst.segment(k))
        .find(|s| s.parameters.len() >= 2)?;
    let value = seg.parameters[0].as_f64();
    let unit_ref = seg.parameters[1].as_ref();
    let (Some(value), Some(unit_ref)) = (value, unit_ref) else {
        ctx.warn(
            format!("#{} has a measure without a numeric value or unit", inst.id),
            Some(inst.id),
        );
        return None;
    };
    let unit = unit_name(ctx, unit_ref).unwrap_or_default();
    Some(Measure::new(value, unit))
}

/// A `measure_representation_item`, with its qualifiers resolved and the
/// qualifier instances consumed.
pub(crate) fn qualified_item(ctx: &mut Ctx<'_>, inst: &Instance) -> Option<Qualified> {
    let measure = measure_with_unit(ctx, inst)?;
    let name = inst
        .attr("REPRESENTATION_ITEM", 0)
        .and_then(Parameter::as_str)
        .unwrap_or_default()
        .to_owned();
    let mut q = Qualified {
        name,
        measure,
        decimal_places: None,
        type_qualifiers: Vec::new(),
    };
    let ex = ctx.ex;
    let qualifiers: Vec<_> = inst
        .attr("QUALIFIED_REPRESENTATION_ITEM", 0)
        .and_then(Parameter::as_list)
        .map(|l| l.iter().filter_map(Parameter::as_ref).collect())
        .unwrap_or_default();
    for qid in qualifiers {
        let Some(qi) = ex.get(qid) else {
            ctx.warn(
                format!("qualifier #{qid} of #{} is undefined", inst.id),
                Some(inst.id),
            );
            continue;
        };
        if let Some(fmt) = qi
            .attr("VALUE_FORMAT_TYPE_QUALIFIER", 0)
            .and_then(Parameter::as_str)
        {
            match decimal_places(fmt) {
                Some(d) => q.decimal_places = Some(d),
                None => ctx.warn(
                    format!("unrecognised value format `{fmt}` on #{qid}"),
                    Some(qid),
                ),
            }
        } else if let Some(t) = qi.attr("TYPE_QUALIFIER", 0).and_then(Parameter::as_str) {
            q.type_qualifiers.push(t.trim().to_ascii_lowercase());
        } else {
            ctx.warn(
                format!(
                    "unrecognised qualifier #{qid} ({}) on #{}",
                    qi.type_key(),
                    inst.id
                ),
                Some(qid),
            );
            continue;
        }
        ctx.consume(qid);
    }
    Some(q)
}

/// Decimal places from an ISO 13584-42 value format such as `NR2 1.2` or
/// `NR2S 0.3` (the digit after the dot).
fn decimal_places(fmt: &str) -> Option<u8> {
    let spec = fmt.split_whitespace().last()?;
    let (_, frac) = spec.split_once('.')?;
    frac.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::decimal_places;

    #[test]
    fn parses_value_formats() {
        assert_eq!(decimal_places("NR2 1.2"), Some(2));
        assert_eq!(decimal_places("NR2S 0.3"), Some(3));
        assert_eq!(decimal_places("NR2 3.0"), Some(0));
        assert_eq!(decimal_places("NR1 3"), None);
        assert_eq!(decimal_places(""), None);
    }
}
