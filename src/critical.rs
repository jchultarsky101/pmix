//! Which tolerances a substitute part has to hold (ADR 0014).
//!
//! A drawing states every tolerance as an equal. They are not equal: a
//! bore held to ±0.005 is the fit, and a ±0.5 on an overall length is
//! the stock it was cut from. A person sourcing a replacement reads the
//! first and ignores the second, and nothing in the PMI document made
//! that ordering visible — a consumer had to know to look.
//!
//! **The ordering is by width, not by importance.** A narrow zone is
//! narrow whoever drew it; whether it *matters* depends on the
//! assembly, which is not in the file. So this ranks and does not
//! judge, and the field is called what it is measured by.
//!
//! **What a width means differs by kind**, so each is stated:
//!
//! - A plus-minus tolerance is its upper deviation less its lower.
//! - A pair of limits is the upper less the lower.
//! - A geometric tolerance is the width of its zone.
//! - An ISO 286 fit states a grade rather than a width, and the grade
//!   is the tightness: IT6 is tighter than IT7 whatever the size. Those
//!   are ranked among themselves, ahead of anything wider, and carry
//!   the code rather than a made-up number.

use serde::{Deserialize, Serialize};

use crate::model::{Dimension, DimensionTolerance, GeometricTolerance, Measure, PmiDocument};

/// One tolerance, and what makes it tight.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Critical {
    /// The record's id in the PMI document.
    pub id: String,
    /// Whether it came from a dimension or a geometric tolerance.
    pub source: Source,
    /// What it controls, as the file displays it — `⌀12.5 ±0.05`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// How wide the tolerance is, where it states a width.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<Measure>,
    /// The ISO 286 code, where the tolerance is a fit rather than a
    /// width — `H7`, `g6`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fit: Option<String>,
    /// The nominal the tolerance is on, where there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nominal: Option<Measure>,
    /// The features it applies to, by their ids in the PMI document.
    pub features: Vec<String>,
}

/// Where a ranked tolerance came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Dimension,
    GeometricTolerance,
}

/// The tightest tolerances in a document, tightest first.
///
/// `limit` caps how many come back; pass `usize::MAX` for all of them.
/// Records stating no width and no fit are left out, because there is
/// nothing to rank them by — they are still in the PMI document.
pub fn tightest(document: &PmiDocument, limit: usize) -> Vec<Critical> {
    let mut found: Vec<Critical> = Vec::new();
    for d in &document.semantic.dimensions {
        if let Some(c) = from_dimension(d) {
            found.push(c);
        }
    }
    for t in &document.semantic.tolerances {
        if let Some(c) = from_tolerance(t) {
            found.push(c);
        }
    }

    found.sort_by(|a, b| {
        rank(a)
            .partial_cmp(&rank(b))
            .unwrap_or(std::cmp::Ordering::Equal)
            // Two tolerances of one width are ordered by id, so that two
            // readings of one file agree (ADR 0004).
            .then_with(|| a.id.cmp(&b.id))
    });
    found.truncate(limit);
    found
}

/// What a record is ranked by: its width, or for a fit, a number
/// standing for its grade.
///
/// A fit states no width — the width depends on the nominal size — so it
/// cannot be compared against one directly. Grades are ranked among
/// themselves and put ahead of every stated width, because a part
/// dimensioned to an ISO fit is a part whose fit is the point. The
/// number is an ordering device and never appears in the output.
fn rank(c: &Critical) -> f64 {
    match (&c.fit, &c.width) {
        (Some(code), _) => grade_of(code).unwrap_or(20) as f64 / 1e6,
        (None, Some(w)) => w.value.abs(),
        (None, None) => f64::INFINITY,
    }
}

/// The IT grade an ISO 286 code names: the digits in `H7`, `g6`, `js9`.
fn grade_of(code: &str) -> Option<u32> {
    let digits: String = code.chars().filter(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// A width, rounded as every other measured number in these documents
/// is (ADR 0004).
///
/// Subtracting two deviations is exact in neither binary nor decimal:
/// `0.05 - -0.1` comes out as 0.15000000000000002, which is not a
/// tolerance anybody wrote.
fn width(upper: &Measure, lower: &Measure) -> Measure {
    let q = crate::fingerprint::identity_quantum();
    let raw = upper.value - lower.value;
    let value = crate::identity::num(raw, q).parse().unwrap_or(raw);
    Measure::new(value, upper.unit.clone())
}

fn from_dimension(d: &Dimension) -> Option<Critical> {
    let (width, fit) = match &d.tolerance {
        Some(DimensionTolerance::PlusMinus { lower, upper }) => (Some(width(upper, lower)), None),
        Some(DimensionTolerance::LimitsAndFits { zone, grade, .. }) => {
            (None, Some(format!("{zone}{grade}")))
        }
        // No tolerance, so a pair of limits is the only thing left that
        // states a width; a dimension with neither is not ranked.
        None => (
            Some(width(&d.limits.as_ref()?.upper, &d.limits.as_ref()?.lower)),
            None,
        ),
    };
    Some(Critical {
        id: d.meta.id.clone(),
        source: Source::Dimension,
        text: d.text.clone(),
        width,
        fit,
        nominal: d.value.clone(),
        features: d.features.clone(),
    })
}

fn from_tolerance(t: &GeometricTolerance) -> Option<Critical> {
    let value = t.value.clone()?;
    Some(Critical {
        id: t.meta.id.clone(),
        source: Source::GeometricTolerance,
        text: t.text.clone(),
        // A geometric tolerance's value *is* the width of its zone.
        width: Some(value),
        fit: None,
        nominal: None,
        features: t.features.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_grade_is_read_from_its_code() {
        assert_eq!(grade_of("H7"), Some(7));
        assert_eq!(grade_of("js9"), Some(9));
        assert_eq!(grade_of("g6"), Some(6));
        assert_eq!(grade_of(""), None);
    }

    /// A fit is ranked by its grade and ahead of any stated width,
    /// because a part dimensioned to one is a part whose fit is the
    /// point.
    #[test]
    fn a_tighter_grade_ranks_ahead_of_a_looser_one_and_of_a_width() {
        let fit = |code: &str| Critical {
            id: code.into(),
            source: Source::Dimension,
            text: None,
            width: None,
            fit: Some(code.into()),
            nominal: None,
            features: Vec::new(),
        };
        let width = |w: f64| Critical {
            id: format!("w{w}"),
            source: Source::Dimension,
            text: None,
            width: Some(Measure::new(w, "mm")),
            fit: None,
            nominal: None,
            features: Vec::new(),
        };
        assert!(rank(&fit("H6")) < rank(&fit("H7")));
        assert!(rank(&fit("H7")) < rank(&width(0.001)));
        assert!(rank(&width(0.01)) < rank(&width(0.1)));
    }

    /// A negative deviation is a width, not a lesser one — and the
    /// subtraction is rounded, because `0.05 - -0.1` is
    /// 0.15000000000000002 and nobody wrote that on a drawing.
    #[test]
    fn a_width_is_measured_however_the_deviations_are_signed() {
        let w = |lower: f64, upper: f64| {
            width(&Measure::new(upper, "mm"), &Measure::new(lower, "mm")).value
        };
        assert_eq!(w(-0.05, 0.05), 0.1);
        assert_eq!(w(-0.1, 0.05), 0.15);
        assert_eq!(w(-0.2, 0.0), 0.2);
    }
}
