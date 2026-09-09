//! Reading a STEP geometry item as a fingerprint (ADR 0004).
//!
//! The recipe itself lives in [`crate::fingerprint`], because the JT
//! reader has to produce the same string for the same face. What this
//! module does is turn a STEP instance into what that recipe wants: a
//! surface, and the vertices of the face on it.

use std::collections::HashSet;

use super::datums::placement;
use crate::fingerprint::{self, identity_quantum as shared_quantum};
use crate::identity::triple;
use crate::model::Placement;
use crate::step::p21::{Exchange, Id, Instance, Parameter};

/// The identity quantum: a fixed 1e-3 model units. It is deliberately not
/// derived from the file's uncertainty, which differs between exports of
/// the same design and would make ids depend on export settings.
pub(crate) fn identity_quantum(_ex: &Exchange) -> f64 {
    shared_quantum()
}

fn axis_of(p: &Placement) -> [f64; 3] {
    p.axis
        .as_ref()
        .map(|d| fingerprint::normalise([d.x, d.y, d.z]))
        .unwrap_or([0.0, 0.0, 1.0])
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
    let carrier = surface
        .map(|s| surface_of(ex, s))
        .unwrap_or_else(|| fingerprint::Surface::Other("face".into()));
    Fingerprint {
        key: fingerprint::face_key(&carrier, &verts, q),
        points: verts,
    }
}

/// The surface a STEP instance describes, as the shared recipe wants it.
///
/// A surface whose kind is known but whose placement the file leaves out
/// is named without being located, which keeps such a face apart from
/// other kinds without pretending to know where it is.
fn surface_of(ex: &Exchange, s: &Instance) -> fingerprint::Surface {
    let p = s.parameters();
    let pl = p
        .get(1)
        .and_then(Parameter::as_ref)
        .and_then(|i| ex.get(i))
        .and_then(|i| placement(ex, i));
    let number = |at: usize| p.get(at).and_then(Parameter::as_f64).unwrap_or(0.0);
    let is = |t: &str| s.has_type(t);
    match pl {
        Some(pl) if is("PLANE") => fingerprint::Surface::Plane {
            origin: pl.origin,
            axis: axis_of(&pl),
        },
        Some(pl) if is("CYLINDRICAL_SURFACE") => fingerprint::Surface::Cylinder {
            origin: pl.origin,
            axis: axis_of(&pl),
            radius: number(2),
        },
        Some(pl) if is("CONICAL_SURFACE") => fingerprint::Surface::Cone {
            origin: pl.origin,
            axis: axis_of(&pl),
            radius: number(2),
            semi_angle: number(3),
        },
        Some(pl) if is("SPHERICAL_SURFACE") => fingerprint::Surface::Sphere {
            origin: pl.origin,
            radius: number(2),
        },
        Some(pl) if is("TOROIDAL_SURFACE") || is("DEGENERATE_TOROIDAL_SURFACE") => {
            fingerprint::Surface::Torus {
                origin: pl.origin,
                axis: axis_of(&pl),
                major_radius: number(2),
                minor_radius: number(3),
            }
        }
        _ => fingerprint::Surface::Other(named(s)),
    }
}

/// What a surface is called when it is not located, or not one of the
/// kinds the recipe has a closed form for.
fn named(s: &Instance) -> String {
    for (t, name) in [
        ("PLANE", "plane"),
        ("CYLINDRICAL_SURFACE", "cylinder"),
        ("CONICAL_SURFACE", "cone"),
        ("SPHERICAL_SURFACE", "sphere"),
        ("TOROIDAL_SURFACE", "torus"),
        ("DEGENERATE_TOROIDAL_SURFACE", "torus"),
    ] {
        if s.has_type(t) {
            return name.into();
        }
    }
    s.type_key().to_ascii_lowercase()
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
