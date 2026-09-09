//! Building the neutral B-rep from JT's smart topology table.
//!
//! The table already holds everything recognition needs, including which
//! faces meet across an edge and whether a loop bounds a face or cuts a
//! hole in it. What this does is restate it in the neutral form and put
//! the numbers into millimetres, which JT's are not (ADR 0010).

use crate::fingerprint::Scale;
use crate::jt::stt::{self, Topology};

use super::brep::{CurveKind, Edge, Face, Loop, Solid};

/// JT states lengths in metres, and a feature document in millimetres.
const SCALE: Scale = Scale {
    length: 1000.0,
    angle: 1.0,
};

fn curve_kind(k: stt::CurveKind) -> CurveKind {
    match k {
        stt::CurveKind::Line => CurveKind::Line,
        stt::CurveKind::Circle => CurveKind::Circle,
        stt::CurveKind::Ellipse => CurveKind::Ellipse,
        stt::CurveKind::Other(_) => CurveKind::Other,
    }
}

/// The solid a topology table describes.
pub fn solid(t: &Topology) -> Solid {
    let edges: Vec<Edge> = t
        .edges
        .iter()
        .map(|e| {
            let (centre, axis, radius) = match e.curve {
                Some(stt::Curve::Circle {
                    centre,
                    axis,
                    radius,
                    ..
                }) => (
                    Some(SCALE.point(centre)),
                    Some(axis),
                    Some(radius * SCALE.length),
                ),
                _ => (None, None, None),
            };
            let at = |v: u32| t.vertices.get(v as usize).copied().flatten();
            let ends = [at(e.start_vertex), at(e.end_vertex)]
                .into_iter()
                .flatten()
                .map(|p| SCALE.point(p))
                .collect();
            Edge {
                curve: curve_kind(e.curve_kind),
                centre,
                axis,
                radius,
                ends,
            }
        })
        .collect();

    let faces: Vec<Face> = t
        .faces
        .iter()
        .map(|f| {
            let loops = f
                .loops
                .clone()
                .filter_map(|l| t.loops.get(l))
                .map(|lp| Loop {
                    edges: lp
                        .coedges
                        .clone()
                        .filter_map(|c| t.coedges.get(c))
                        .map(|c| c.edge)
                        .collect(),
                    // Loop type 2 is an outer boundary and 3 a hole
                    // (specification table H.7).
                    outer: lp.kind != 3,
                })
                .collect();
            Face {
                // A face the table gives no closed form for is named by
                // its kind and located nowhere, which the neutral form
                // says with `None` rather than with a placeholder.
                surface: match t.surface_of(f) {
                    Some(crate::fingerprint::Surface::Other(_)) | None => None,
                    Some(s) => Some(s.scaled(SCALE)),
                },
                kind: f.surface_kind.name().to_owned(),
                // The table says whether a face's normal opposes its
                // surface's and whether it points into the shell. A face
                // looks the way its surface does when the two agree.
                same_sense: f.normal_reversed == f.inward,
                loops,
            }
        })
        .collect();

    let mut solid = Solid {
        name: None,
        faces,
        edges,
        across: Default::default(),
    };
    solid.link();
    solid
}
