//! Building the neutral B-rep from a STEP file's advanced B-rep.
//!
//! The PMI side of the STEP reader walks a single face at a time to
//! fingerprint it (ADR 0004). Recognition needs the whole shell and, in
//! particular, which faces meet across an edge, which the file states
//! only by two faces naming one `EDGE_CURVE`. Collecting that is what
//! this module is for.

use std::collections::BTreeMap;

use crate::fingerprint::{Scale, Surface};
use crate::step::p21::{Exchange, Id, Instance, Parameter};
use crate::step::pmi::fingerprint::{surface_of, vertex_point};

use super::brep::{CurveKind, Edge, Face, Loop, Solid};

/// `.T.` and `.F.` reach the parser as enumerations.
fn flag(p: Option<&Parameter>) -> Option<bool> {
    match p?.as_enum()? {
        "T" | "TRUE" => Some(true),
        "F" | "FALSE" => Some(false),
        _ => None,
    }
}

/// The placement's origin and axis, in the file's own units.
fn placement(ex: &Exchange, inst: &Instance) -> ([f64; 3], [f64; 3]) {
    let at = |n: usize| {
        inst.parameters()
            .get(n)
            .and_then(Parameter::as_ref)
            .and_then(|i| ex.get(i))
    };
    let origin = at(1).and_then(point).unwrap_or([0.0; 3]);
    let axis = at(2).and_then(direction).unwrap_or([0.0, 0.0, 1.0]);
    (origin, axis)
}

fn point(inst: &Instance) -> Option<[f64; 3]> {
    let l = inst.parameters().get(1)?.as_list()?;
    Some([
        l.first()?.as_f64()?,
        l.get(1)?.as_f64()?,
        l.get(2).and_then(Parameter::as_f64).unwrap_or(0.0),
    ])
}

fn direction(inst: &Instance) -> Option<[f64; 3]> {
    point(inst)
}

/// The curve an edge really runs along.
///
/// A hole's edges are rarely stated as a bare `CIRCLE`. An exporter
/// records the curve together with its image on each surface, as a
/// `SURFACE_CURVE`, or as the `SEAM_CURVE` where a closed surface joins
/// itself, and the shape is the first of those a wrapper holds. Reading
/// only the outer entity leaves every edge of every bore unknown, and
/// with it every rule that asks what bounds a face.
fn curve_3d<'a>(ex: &'a Exchange, c: &'a Instance, depth: usize) -> Option<&'a Instance> {
    const WRAPPERS: [&str; 4] = [
        "SURFACE_CURVE",
        "SEAM_CURVE",
        "INTERSECTION_CURVE",
        "BOUNDED_SURFACE_CURVE",
    ];
    if depth == 0 || !WRAPPERS.iter().any(|w| c.has_type(w)) {
        return Some(c);
    }
    let inner = c
        .parameters()
        .get(1)
        .and_then(Parameter::as_ref)
        .and_then(|i| ex.get(i))?;
    curve_3d(ex, inner, depth - 1)
}

/// What an `EDGE_CURVE` runs along, and where.
fn edge_of(ex: &Exchange, e: &Instance, scale: Scale) -> Edge {
    let p = e.parameters();
    let curve = p
        .get(3)
        .and_then(Parameter::as_ref)
        .and_then(|i| ex.get(i))
        .and_then(|c| curve_3d(ex, c, 4));
    let kind = match curve {
        Some(c) if c.has_type("CIRCLE") => CurveKind::Circle,
        Some(c) if c.has_type("ELLIPSE") => CurveKind::Ellipse,
        Some(c) if c.has_type("LINE") => CurveKind::Line,
        _ => CurveKind::Other,
    };
    let (centre, axis, radius) = match curve {
        Some(c) if c.has_type("CIRCLE") => {
            let placed = c
                .parameters()
                .get(1)
                .and_then(Parameter::as_ref)
                .and_then(|i| ex.get(i))
                .map(|a| placement(ex, a));
            let r = c.parameters().get(2).and_then(Parameter::as_f64);
            match (placed, r) {
                (Some((o, a)), Some(r)) => (Some(scale.point(o)), Some(a), Some(r * scale.length)),
                _ => (None, None, None),
            }
        }
        _ => (None, None, None),
    };
    let ends = [1usize, 2]
        .into_iter()
        .filter_map(|n| {
            p.get(n)
                .and_then(Parameter::as_ref)
                .and_then(|i| ex.get(i))
                .and_then(|v| vertex_point(ex, v))
        })
        .map(|v| scale.point(v))
        .collect();
    Edge {
        curve: kind,
        centre,
        axis,
        radius,
        ends,
    }
}

/// Follow an `ORIENTED_EDGE` to the `EDGE_CURVE` it uses, which is what
/// two neighbouring faces have in common.
fn edge_curve(oe: &Instance) -> Option<Id> {
    if oe.has_type("ORIENTED_EDGE") {
        return oe.parameters().get(3).and_then(Parameter::as_ref);
    }
    None
}

/// Every solid in the file, as neutral B-reps.
///
/// A shell is taken as a solid whether or not a `MANIFOLD_SOLID_BREP`
/// names it, so that a file which states a shell alone still yields the
/// faces it holds rather than nothing.
pub fn solids(ex: &Exchange, scale: Scale) -> Vec<Solid> {
    solids_with_shells(ex, scale)
        .into_iter()
        .map(|(solid, _)| solid)
        .collect()
}

/// The same, each paired with the shell the file stated it as.
///
/// Recognition does not care which entity a solid came from, but the
/// product reader does: the only way from a body to the part that has it
/// is up from the shell, through the representation that holds it, to
/// the product definition that representation defines (ADR 0014).
pub fn solids_with_shells(ex: &Exchange, scale: Scale) -> Vec<(Solid, Id)> {
    let mut out = Vec::new();
    for shell in ex
        .instances()
        .filter(|i| i.has_type("CLOSED_SHELL") || i.has_type("OPEN_SHELL"))
    {
        let mut face_ids = Vec::new();
        if let Some(p) = shell.parameters().get(1) {
            p.collect_refs(&mut face_ids);
        }
        let mut edges: Vec<Edge> = Vec::new();
        let mut index: BTreeMap<Id, usize> = BTreeMap::new();
        let mut faces: Vec<Face> = Vec::new();

        for f in face_ids.iter().filter_map(|i| ex.get(*i)) {
            if !(f.has_type("ADVANCED_FACE") || f.has_type("FACE_SURFACE")) {
                continue;
            }
            let p = f.parameters();
            let surface = p.get(2).and_then(Parameter::as_ref).and_then(|i| ex.get(i));
            let carrier = surface.map(|s| surface_of(ex, s));
            let kind = carrier
                .as_ref()
                .map(name_of)
                .unwrap_or("unknown")
                .to_owned();
            let mut bounds = Vec::new();
            if let Some(b) = p.get(1) {
                b.collect_refs(&mut bounds);
            }
            let mut loops = Vec::new();
            for b in bounds.iter().filter_map(|i| ex.get(*i)) {
                let Some(el) = b
                    .parameters()
                    .get(1)
                    .and_then(Parameter::as_ref)
                    .and_then(|i| ex.get(i))
                else {
                    continue;
                };
                let mut oriented = Vec::new();
                if let Some(e) = el.parameters().get(1) {
                    e.collect_refs(&mut oriented);
                }
                let mut in_loop = Vec::new();
                for oe in oriented.iter().filter_map(|i| ex.get(*i)) {
                    let Some(id) = edge_curve(oe) else { continue };
                    let at = *index.entry(id).or_insert_with(|| {
                        let e = ex.get(id).map(|e| edge_of(ex, e, scale));
                        edges.push(e.unwrap_or(Edge {
                            curve: CurveKind::Other,
                            centre: None,
                            axis: None,
                            radius: None,
                            ends: Vec::new(),
                        }));
                        edges.len() - 1
                    });
                    in_loop.push(at);
                }
                loops.push(Loop {
                    edges: in_loop,
                    // A file that does not distinguish the two is taken
                    // to mean the outer boundary, as a single loop is.
                    outer: !b.has_type("FACE_BOUND") || b.has_type("FACE_OUTER_BOUND"),
                });
            }
            faces.push(Face {
                surface: match carrier {
                    Some(Surface::Other(_)) | None => None,
                    Some(s) => Some(s.scaled(scale)),
                },
                kind,
                // `same_sense`: whether the face looks the way its
                // surface does. A bore's wall does not.
                same_sense: flag(p.get(3)).unwrap_or(true),
                loops,
            });
        }
        if faces.is_empty() {
            continue;
        }
        let mut solid = Solid {
            name: shell
                .parameters()
                .first()
                .and_then(Parameter::as_str)
                .and_then(|s| (!s.is_empty()).then(|| s.to_owned())),
            faces,
            edges,
            across: BTreeMap::new(),
        };
        solid.link();
        out.push((solid, shell.id));
    }
    out
}

fn name_of(s: &Surface) -> &str {
    match s {
        Surface::Plane { .. } => "plane",
        Surface::Cylinder { .. } => "cylinder",
        Surface::Cone { .. } => "cone",
        Surface::Sphere { .. } => "sphere",
        Surface::Torus { .. } => "torus",
        Surface::Other(name) => name,
    }
}
