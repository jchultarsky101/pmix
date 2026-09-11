//! Patterns in the features a body holds (ADR 0014).
//!
//! Four holes are four facts. That they sit at the corners of a 24 × 14
//! rectangle is a fifth, and it is the one that answers the question a
//! substitute part has to pass: *will it bolt where the old one bolted?*
//! Nothing else in either document answers it, and a consumer asked to
//! derive it from four position vectors will derive it afresh every
//! time, sometimes differently.
//!
//! **This is a recognition, not a reading.** Nothing in the file says
//! "bolt circle". Each pattern therefore states the rule that produced
//! it, in the same posture as a recognised feature: what it rests on is
//! part of the answer.
//!
//! **Both readings, where both hold.** Four holes at the corners of a
//! square are a grid *and* a bolt circle, and neither reading is wrong.
//! ADR 0011 settled what to do about that for features — report both and
//! state the overlap — and the same applies here.
//!
//! A pattern needs three members. Two of anything lie on a line at an
//! even pitch, which makes "linear pattern" true of every pair and
//! useful about none of them.

use std::collections::BTreeMap;

use crate::model::ContentId;

use super::model::{Feature, Kind, Pattern, PatternKind};

/// The fewest members a pattern can have.
const LEAST: usize = 3;

/// Kinds worth looking for a pattern among. A fillet or a chamfer runs
/// along an edge rather than sitting at a place, so a pattern of them
/// would not mean what a pattern of holes means.
fn patternable(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Hole | Kind::Counterbore | Kind::Countersink | Kind::Boss
    )
}

/// Every pattern in `features`.
pub fn recognise(features: &[Feature], ids: &mut ContentId) -> Vec<Pattern> {
    let q = crate::fingerprint::identity_quantum();

    // Members of one pattern are alike and parallel: same kind, same
    // size, same axis. A group that differs in any of those is a set of
    // separate features that happen to be near each other.
    let mut groups: BTreeMap<String, Vec<&Feature>> = BTreeMap::new();
    for f in features.iter().filter(|f| patternable(f.kind)) {
        let (Some(axis), Some(position)) = (f.shape.axis, f.shape.position) else {
            continue;
        };
        let _ = position;
        let size = f.shape.diameter.or(f.shape.radius).unwrap_or(0.0);
        let key = format!(
            "{};{};{}",
            f.kind.name(),
            crate::identity::num(size, q),
            crate::identity::triple(axis, q)
        );
        groups.entry(key).or_default().push(f);
    }

    let mut out = Vec::new();
    for group in groups.values() {
        if group.len() < LEAST {
            continue;
        }
        let axis = group[0].shape.axis.expect("grouped on an axis");
        let (u, v) = basis(axis);
        // Where each member sits, in the plane the axis is normal to.
        let flat: Vec<[f64; 2]> = group
            .iter()
            .map(|f| {
                let p = f.shape.position.expect("grouped on a position");
                [dot(p, u), dot(p, v)]
            })
            .collect();

        let members: Vec<String> = {
            let mut m: Vec<String> = group.iter().map(|f| f.id.clone()).collect();
            m.sort();
            m
        };
        let diameter = group[0].shape.diameter;

        if let Some(mut p) = bolt_circle(&flat, u, v, axis) {
            p.features = members.clone();
            p.count = group.len();
            p.diameter = diameter;
            out.push(p);
        }
        if let Some(mut p) = row(&flat) {
            p.features = members.clone();
            p.count = group.len();
            p.diameter = diameter;
            p.axis = Some(axis);
            out.push(p);
        }
        if let Some(mut p) = grid(&flat) {
            p.features = members.clone();
            p.count = group.len();
            p.diameter = diameter;
            p.axis = Some(axis);
            out.push(p);
        }
    }

    for p in out.iter_mut() {
        p.id = ids.make("pat", &[&key_of(p)]);
    }

    // Two patterns sharing a feature are two readings of the same
    // material — four holes at the corners of a square are a grid and a
    // bolt circle — which ADR 0011 says to state rather than choose
    // between.
    let made: Vec<(String, Vec<String>)> = out
        .iter()
        .map(|p| (p.id.clone(), p.features.clone()))
        .collect();
    for (i, p) in out.iter_mut().enumerate() {
        let mut overlaps: Vec<String> = made
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .filter(|(_, (_, features))| features.iter().any(|f| p.features.contains(f)))
            .map(|(_, (id, _))| id.clone())
            .collect();
        overlaps.sort();
        overlaps.dedup();
        p.overlaps = overlaps;
    }

    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// A pattern's identity key: what it is and what it is made of.
///
/// Built from the members' ids, which are themselves content-derived
/// (ADR 0004), so a pattern is named the same way in two exports of one
/// design.
fn key_of(p: &Pattern) -> String {
    format!("{};{}", p.kind.name(), p.features.join(","))
}

/// Holes on a common circle at an even angular pitch.
fn bolt_circle(flat: &[[f64; 2]], u: [f64; 3], v: [f64; 3], axis: [f64; 3]) -> Option<Pattern> {
    let tol = 100.0 * crate::fingerprint::identity_quantum();
    let (centre, radius) = fit_circle(flat, tol)?;
    if radius <= tol {
        return None;
    }

    let mut angles: Vec<f64> = flat
        .iter()
        .map(|p| (p[1] - centre[1]).atan2(p[0] - centre[0]).to_degrees())
        .map(|a| if a < 0.0 { a + 360.0 } else { a })
        .collect();
    angles.sort_by(f64::total_cmp);

    // Evenly spaced, which is what makes it a bolt circle rather than
    // four holes that happen to be concyclic — as the corners of any
    // rectangle are.
    let step = 360.0 / angles.len() as f64;
    let even = angles
        .windows(2)
        .all(|w| ((w[1] - w[0]) - step).abs() < 0.01);
    if !even {
        return None;
    }

    let clocking = angles[0];
    let centre3 = [
        u[0] * centre[0] + v[0] * centre[1],
        u[1] * centre[0] + v[1] * centre[1],
        u[2] * centre[0] + v[2] * centre[1],
    ];
    Some(Pattern {
        kind: PatternKind::BoltCircle,
        pitch_circle_diameter: Some(round(radius * 2.0)),
        centre: Some(rounded(centre3)),
        clocking: Some(round(clocking)),
        axis: Some(axis),
        rule: format!(
            "{} alike features on one circle at an even {:.4}° pitch",
            flat.len(),
            step
        ),
        ..Pattern::empty()
    })
}

/// Holes in a straight line at an even pitch.
fn row(flat: &[[f64; 2]]) -> Option<Pattern> {
    let tol = 100.0 * crate::fingerprint::identity_quantum();
    let (a, b) = (flat[0], *flat.last()?);
    let along = unit2([b[0] - a[0], b[1] - a[1]])?;
    let across = [-along[1], along[0]];

    // On one line.
    if flat
        .iter()
        .any(|p| ((p[0] - a[0]) * across[0] + (p[1] - a[1]) * across[1]).abs() > tol)
    {
        return None;
    }

    let mut along_line: Vec<f64> = flat
        .iter()
        .map(|p| (p[0] - a[0]) * along[0] + (p[1] - a[1]) * along[1])
        .collect();
    along_line.sort_by(f64::total_cmp);

    let pitch = along_line[1] - along_line[0];
    if pitch <= tol {
        return None;
    }
    if along_line
        .windows(2)
        .any(|w| ((w[1] - w[0]) - pitch).abs() > tol)
    {
        return None;
    }

    Some(Pattern {
        kind: PatternKind::Row,
        pitch: vec![round(pitch)],
        counts: vec![flat.len()],
        rule: format!("{} alike features on one line at an even pitch", flat.len()),
        ..Pattern::empty()
    })
}

/// Holes on a rectangular lattice, filled.
fn grid(flat: &[[f64; 2]]) -> Option<Pattern> {
    let tol = 100.0 * crate::fingerprint::identity_quantum();
    if flat.len() < 4 {
        return None;
    }

    // The two shortest independent steps between members are the lattice
    // the rest has to fit.
    let mut steps: Vec<[f64; 2]> = Vec::new();
    for (i, a) in flat.iter().enumerate() {
        for b in &flat[i + 1..] {
            steps.push([b[0] - a[0], b[1] - a[1]]);
        }
    }
    steps.sort_by(|x, y| length2(*x).total_cmp(&length2(*y)));

    let u = *steps.first()?;
    let v = *steps
        .iter()
        .find(|s| (s[0] * u[1] - s[1] * u[0]).abs() > tol)?;
    // Rectangular: the two directions meet at a right angle. A skewed
    // lattice is a pattern too, but not one anybody bolts to, and a rule
    // that cannot settle a thing should not claim it (ADR 0011).
    if (u[0] * v[0] + u[1] * v[1]).abs() > tol * length2(u).sqrt() {
        return None;
    }

    // Measured from one member, then shifted so the corner of the
    // lattice is the origin. Which member it is measured from, and which
    // way round the basis points, must not change the answer.
    let from = flat[0];
    let (lu, lv) = (length2(u).sqrt(), length2(v).sqrt());
    let mut cells: Vec<(i64, i64)> = Vec::new();
    for p in flat {
        let d = [p[0] - from[0], p[1] - from[1]];
        let i = (d[0] * u[0] + d[1] * u[1]) / (lu * lu);
        let j = (d[0] * v[0] + d[1] * v[1]) / (lv * lv);
        if (i - i.round()).abs() * lu > tol || (j - j.round()).abs() * lv > tol {
            return None;
        }
        cells.push((i.round() as i64, j.round() as i64));
    }
    let base = (
        cells.iter().map(|c| c.0).min()?,
        cells.iter().map(|c| c.1).min()?,
    );
    for c in cells.iter_mut() {
        *c = (c.0 - base.0, c.1 - base.1);
    }

    let across = span(cells.iter().map(|c| c.0))?;
    let down = span(cells.iter().map(|c| c.1))?;
    if across < 2 || down < 2 || across * down != flat.len() {
        // A lattice with a gap in it is not a grid; saying it is would
        // put a hole where the part has none.
        return None;
    }

    Some(Pattern {
        kind: PatternKind::Grid,
        pitch: vec![round(lu), round(lv)],
        counts: vec![across, down],
        rule: format!("{across} by {down} alike features on a filled rectangular lattice"),
        ..Pattern::empty()
    })
}

/// How many distinct lattice positions a coordinate takes, if they run
/// from zero without a gap.
fn span(values: impl Iterator<Item = i64>) -> Option<usize> {
    let mut seen: Vec<i64> = values.collect();
    seen.sort_unstable();
    seen.dedup();
    let n = seen.len();
    (seen[0] == 0 && seen[n - 1] == n as i64 - 1).then_some(n)
}

/// The circle through a set of points, if one passes through all of
/// them.
///
/// Taken from the three points furthest apart and then checked against
/// every member, rather than fitted by least squares: a fit always
/// returns a circle, and what is wanted here is whether there *is* one.
fn fit_circle(flat: &[[f64; 2]], tol: f64) -> Option<([f64; 2], f64)> {
    let (a, b, c) = (flat[0], flat[flat.len() / 2], *flat.last()?);
    let d = 2.0 * (a[0] * (b[1] - c[1]) + b[0] * (c[1] - a[1]) + c[0] * (a[1] - b[1]));
    if d.abs() < 1e-9 {
        return None;
    }
    let sq = |p: [f64; 2]| p[0] * p[0] + p[1] * p[1];
    let centre = [
        (sq(a) * (b[1] - c[1]) + sq(b) * (c[1] - a[1]) + sq(c) * (a[1] - b[1])) / d,
        (sq(a) * (c[0] - b[0]) + sq(b) * (a[0] - c[0]) + sq(c) * (b[0] - a[0])) / d,
    ];
    let radius = (sq([a[0] - centre[0], a[1] - centre[1]])).sqrt();
    for p in flat {
        let r = (sq([p[0] - centre[0], p[1] - centre[1]])).sqrt();
        if (r - radius).abs() > tol {
            return None;
        }
    }
    Some((centre, radius))
}

/// Two perpendicular directions spanning the plane `axis` is normal to.
///
/// Built from the world axis least aligned with `axis`, so that one axis
/// always yields one basis and a pattern's clocking means the same thing
/// in two exports of a design.
fn basis(axis: [f64; 3]) -> ([f64; 3], [f64; 3]) {
    let mut least = 0;
    for i in 1..3 {
        if axis[i].abs() < axis[least].abs() {
            least = i;
        }
    }
    let mut seed = [0.0; 3];
    seed[least] = 1.0;
    let u = normalise(cross(axis, seed));
    let v = normalise(cross(axis, u));
    (u, v)
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalise(v: [f64; 3]) -> [f64; 3] {
    let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if n < 1e-12 {
        v
    } else {
        [v[0] / n, v[1] / n, v[2] / n]
    }
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn unit2(v: [f64; 2]) -> Option<[f64; 2]> {
    let n = (v[0] * v[0] + v[1] * v[1]).sqrt();
    (n > 1e-12).then(|| [v[0] / n, v[1] / n])
}

fn length2(v: [f64; 2]) -> f64 {
    v[0] * v[0] + v[1] * v[1]
}

fn round(v: f64) -> f64 {
    let q = crate::fingerprint::identity_quantum();
    crate::identity::num(v, q).parse().unwrap_or(v)
}

fn rounded(p: [f64; 3]) -> [f64; 3] {
    [round(p[0]), round(p[1]), round(p[2])]
}
