//! Geometry fingerprints: the anchor for feature identity (ADR 0004).
//!
//! A fingerprint is a string built from the numbers that fix a B-rep
//! entity in model space, rounded to the identity quantum, so that the
//! same face in two exports of the same design yields the same string.

use std::collections::HashSet;

use super::datums::placement;
use crate::model::Placement;
use crate::step::p21::{Exchange, Id, Instance, Parameter};

/// The identity quantum: a fixed 1e-3 model units. It is deliberately not
/// derived from the file's uncertainty, which differs between exports of
/// the same design and would make ids depend on export settings.
pub(crate) fn identity_quantum(_ex: &Exchange) -> f64 {
    1e-3
}

/// Round `v` to a multiple of `q` and print it stably.
///
/// The value is first snapped to a 1e-5 grid: re-exports print the same
/// coordinate with different precision (`-2.0315` against `-2.03149999`),
/// and without the snap such pairs would round to different multiples of
/// `q` whenever the true value sits on a rounding boundary, which
/// engineering values in round fractions of an inch often do.
pub(crate) fn num(v: f64, q: f64) -> String {
    let snapped = (v * 1e5).round() / 1e5;
    let r = (snapped / q).round() * q;
    let r = if r == 0.0 { 0.0 } else { r };
    format!("{r:.4}")
}

pub(crate) fn triple(p: [f64; 3], q: f64) -> String {
    format!("{},{},{}", num(p[0], q), num(p[1], q), num(p[2], q))
}

fn normalise(v: [f64; 3]) -> [f64; 3] {
    let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if n == 0.0 {
        v
    } else {
        [v[0] / n, v[1] / n, v[2] / n]
    }
}

/// Flip a direction so its first non-negligible component is positive.
fn sign_normalise(d: [f64; 3]) -> [f64; 3] {
    for c in d {
        if c.abs() > 1e-9 {
            return if c < 0.0 { [-d[0], -d[1], -d[2]] } else { d };
        }
    }
    d
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn axis_of(p: &Placement) -> [f64; 3] {
    p.axis
        .as_ref()
        .map(|d| normalise([d.x, d.y, d.z]))
        .unwrap_or([0.0, 0.0, 1.0])
}

/// Point on the axis through `origin` closest to the model origin.
fn closest_on_axis(origin: [f64; 3], axis: [f64; 3]) -> [f64; 3] {
    let t = dot(origin, axis);
    [
        origin[0] - t * axis[0],
        origin[1] - t * axis[1],
        origin[2] - t * axis[2],
    ]
}

fn point(inst: &Instance) -> Option<[f64; 3]> {
    let l = inst.parameters().get(1)?.as_list()?;
    Some([
        l.first()?.as_f64()?,
        l.get(1)?.as_f64()?,
        l.get(2).and_then(Parameter::as_f64).unwrap_or(0.0),
    ])
}

/// What a geometry item contributes to a feature's identity: a key that
/// fixes its carrier (surface, curve, point) in space, and the points it
/// spans. Faces contribute their surface and their vertices, so a face
/// split in two by a re-export contributes the same surface and the same
/// vertex span as the whole face did.
#[derive(Debug, Clone, Default)]
pub(crate) struct Fingerprint {
    pub key: String,
    pub points: Vec<[f64; 3]>,
}

/// Fingerprint of a geometry item referenced by a feature.
pub(crate) fn of_item(ex: &Exchange, item: &Instance, q: f64) -> Fingerprint {
    if item.has_type("ADVANCED_FACE") || item.has_type("FACE_SURFACE") || item.has_type("FACE") {
        return face(ex, item, q);
    }
    if item.has_type("ORIENTED_EDGE") {
        if let Some(e) = item
            .parameters()
            .get(3)
            .and_then(Parameter::as_ref)
            .and_then(|i| ex.get(i))
        {
            return of_item(ex, e, q);
        }
    }
    if item.has_type("EDGE_CURVE") {
        let p = item.parameters();
        let curve_kind = p
            .get(3)
            .and_then(Parameter::as_ref)
            .and_then(|i| ex.get(i))
            .map(|c| c.type_key().to_ascii_lowercase())
            .unwrap_or_default();
        let pts: Vec<[f64; 3]> = [1, 2]
            .iter()
            .filter_map(|i| {
                p.get(*i)
                    .and_then(Parameter::as_ref)
                    .and_then(|v| ex.get(v))
            })
            .filter_map(|v| vertex_point(ex, v))
            .collect();
        let mut ends: Vec<String> = pts.iter().map(|pt| triple(*pt, q)).collect();
        ends.sort();
        return Fingerprint {
            key: format!("edge/{curve_kind}/{}", ends.join("/")),
            points: pts,
        };
    }
    if item.has_type("VERTEX_POINT") {
        let pt = vertex_point(ex, item);
        return Fingerprint {
            key: pt
                .map(|pt| format!("vertex/{}", triple(pt, q)))
                .unwrap_or_else(|| "vertex".into()),
            points: pt.into_iter().collect(),
        };
    }
    if item.has_type("CARTESIAN_POINT") {
        let pt = point(item);
        return Fingerprint {
            key: pt
                .map(|pt| format!("point/{}", triple(pt, q)))
                .unwrap_or_else(|| "point".into()),
            points: pt.into_iter().collect(),
        };
    }
    // Curves (supplemental geometry): the sampled polyline's end points, so
    // trimmed curves on one basis stay distinct.
    if let Some(pl) = super::presentation::sample_curve(ex, item) {
        if let (Some(first), Some(last)) = (pl.first(), pl.last()) {
            let mut ends = [triple(*first, q), triple(*last, q)];
            ends.sort();
            return Fingerprint {
                key: format!(
                    "curve/{}/{}/{}",
                    item.type_key().to_ascii_lowercase(),
                    ends[0],
                    ends[1]
                ),
                points: pl,
            };
        }
    }
    // Anything else: type plus the points it reaches.
    let mut pts = Vec::new();
    let mut seen = HashSet::new();
    collect_points(ex, item.id, 0, &mut seen, &mut pts);
    Fingerprint {
        key: item.type_key().to_ascii_lowercase(),
        points: pts,
    }
}

fn vertex_point(ex: &Exchange, v: &Instance) -> Option<[f64; 3]> {
    let pt = ex.get(v.parameters().get(1)?.as_ref()?)?;
    point(pt)
}

fn face(ex: &Exchange, f: &Instance, q: f64) -> Fingerprint {
    let p = f.parameters();
    let surface = p.get(2).and_then(Parameter::as_ref).and_then(|i| ex.get(i));
    let key = surface
        .map(|s| surface_key(ex, s, q))
        .unwrap_or_else(|| "face".into());
    // Vertices reachable through bounds -> loop -> edges -> vertices.
    let mut verts: Vec<[f64; 3]> = Vec::new();
    let mut bounds = Vec::new();
    if let Some(b) = p.get(1) {
        b.collect_refs(&mut bounds);
    }
    for b in bounds.iter().filter_map(|i| ex.get(*i)) {
        let Some(l) = b
            .parameters()
            .get(1)
            .and_then(Parameter::as_ref)
            .and_then(|i| ex.get(i))
        else {
            continue;
        };
        let mut edges = Vec::new();
        if let Some(e) = l.parameters().get(1) {
            e.collect_refs(&mut edges);
        }
        for oe in edges.iter().filter_map(|i| ex.get(*i)) {
            let edge = if oe.has_type("ORIENTED_EDGE") {
                oe.parameters()
                    .get(3)
                    .and_then(Parameter::as_ref)
                    .and_then(|i| ex.get(i))
            } else {
                Some(oe)
            };
            let Some(edge) = edge else { continue };
            for i in [1, 2] {
                if let Some(v) = edge
                    .parameters()
                    .get(i)
                    .and_then(Parameter::as_ref)
                    .and_then(|x| ex.get(x))
                {
                    if let Some(pt) = vertex_point(ex, v) {
                        verts.push(pt);
                    }
                }
            }
        }
    }
    // The span of the face along its surface, chosen so that a face split
    // by a re-export (a cylinder cut at a new seam, a plane cut in two)
    // keeps the same span: axial extent for surfaces of revolution,
    // nothing for spheres, the vertex box otherwise.
    let span = match surface {
        Some(s)
            if s.has_type("CYLINDRICAL_SURFACE")
                || s.has_type("CONICAL_SURFACE")
                || s.has_type("TOROIDAL_SURFACE")
                || s.has_type("DEGENERATE_TOROIDAL_SURFACE") =>
        {
            axial_extent(ex, s, &verts, q)
        }
        Some(s) if s.has_type("SPHERICAL_SURFACE") => String::new(),
        _ => bbox(&verts)
            .map(|(lo, hi)| format!("/{}/{}", triple(lo, q), triple(hi, q)))
            .unwrap_or_default(),
    };
    Fingerprint {
        key: format!("{key}{span}"),
        points: verts,
    }
}

/// `[min, max]` of the vertices projected onto the surface's axis.
fn axial_extent(ex: &Exchange, s: &Instance, verts: &[[f64; 3]], q: f64) -> String {
    let Some(pl) = s
        .parameters()
        .get(1)
        .and_then(Parameter::as_ref)
        .and_then(|i| ex.get(i))
        .and_then(|i| placement(ex, i))
    else {
        return String::new();
    };
    let a = sign_normalise(axis_of(&pl));
    let c = closest_on_axis(pl.origin, a);
    let ts: Vec<f64> = verts
        .iter()
        .map(|v| dot([v[0] - c[0], v[1] - c[1], v[2] - c[2]], a))
        .collect();
    match (
        ts.iter().cloned().reduce(f64::min),
        ts.iter().cloned().reduce(f64::max),
    ) {
        (Some(lo), Some(hi)) => format!("/t{}..{}", num(lo, q), num(hi, q)),
        _ => String::new(),
    }
}

fn surface_key(ex: &Exchange, s: &Instance, q: f64) -> String {
    let p = s.parameters();
    let pl = p
        .get(1)
        .and_then(Parameter::as_ref)
        .and_then(|i| ex.get(i))
        .and_then(|i| placement(ex, i));
    let dq = 1e-3;
    if s.has_type("PLANE") {
        if let Some(pl) = pl {
            let n = sign_normalise(axis_of(&pl));
            let d = dot(n, pl.origin);
            return format!("plane/{}/{}", triple(n, dq), num(d, q));
        }
        return "plane".into();
    }
    if s.has_type("CYLINDRICAL_SURFACE") {
        if let Some(pl) = pl {
            let a = sign_normalise(axis_of(&pl));
            let c = closest_on_axis(pl.origin, a);
            let r = p.get(2).and_then(Parameter::as_f64).unwrap_or(0.0);
            return format!("cylinder/{}/{}/{}", triple(a, dq), triple(c, q), num(r, q));
        }
        return "cylinder".into();
    }
    if s.has_type("CONICAL_SURFACE") {
        if let Some(pl) = pl {
            let a = axis_of(&pl);
            let c = closest_on_axis(pl.origin, a);
            let r = p.get(2).and_then(Parameter::as_f64).unwrap_or(0.0);
            let ang = p.get(3).and_then(Parameter::as_f64).unwrap_or(0.0);
            return format!(
                "cone/{}/{}/{}/{}",
                triple(a, dq),
                triple(c, q),
                num(r, q),
                num(ang, dq)
            );
        }
        return "cone".into();
    }
    if s.has_type("SPHERICAL_SURFACE") {
        if let Some(pl) = pl {
            let r = p.get(2).and_then(Parameter::as_f64).unwrap_or(0.0);
            return format!("sphere/{}/{}", triple(pl.origin, q), num(r, q));
        }
        return "sphere".into();
    }
    if s.has_type("TOROIDAL_SURFACE") || s.has_type("DEGENERATE_TOROIDAL_SURFACE") {
        if let Some(pl) = pl {
            let a = sign_normalise(axis_of(&pl));
            let r1 = p.get(2).and_then(Parameter::as_f64).unwrap_or(0.0);
            let r2 = p.get(3).and_then(Parameter::as_f64).unwrap_or(0.0);
            return format!(
                "torus/{}/{}/{}/{}",
                triple(pl.origin, q),
                triple(a, dq),
                num(r1, q),
                num(r2, q)
            );
        }
        return "torus".into();
    }
    s.type_key().to_ascii_lowercase()
}

pub(crate) fn bbox(pts: &[[f64; 3]]) -> Option<([f64; 3], [f64; 3])> {
    if pts.is_empty() {
        return None;
    }
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in pts {
        for i in 0..3 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
    }
    Some((lo, hi))
}

fn collect_points(
    ex: &Exchange,
    id: Id,
    depth: usize,
    seen: &mut HashSet<Id>,
    out: &mut Vec<[f64; 3]>,
) {
    if depth > 4 || !seen.insert(id) || seen.len() > 5000 {
        return;
    }
    let Some(inst) = ex.get(id) else { return };
    if inst.has_type("CARTESIAN_POINT") {
        if let Some(p) = point(inst) {
            out.push(p);
        }
        return;
    }
    for r in inst.references() {
        collect_points(ex, r, depth + 1, seen, out);
    }
}
