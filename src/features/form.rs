//! What a body is made of, and how it was likely made (ADR 0014).
//!
//! A feature list says what a shape *has*. This says what it *is*, in
//! the two coarsest terms a person reaches for first: the surfaces it is
//! built from, and whether those surfaces add up to something turned on
//! a lathe, milled from one direction, or neither.
//!
//! It matters for sourcing because it is the vocabulary. "A turned steel
//! shaft, 120 long, Ø20" is a thing a catalogue can be searched for; "a
//! body with 47 faces" is not.
//!
//! **Only what a local rule settles.** ADR 0011 set the bar and it holds
//! here: a class is claimed when the surfaces themselves decide it, and
//! left unstated otherwise. In particular this does *not* recognise
//! sheet metal. The test people mean by it — a wall of constant
//! thickness, bent — needs each face matched against an offset of
//! another, which is the same trimmed-surface work that puts volume out
//! of reach. Guessing it from a pair of parallel planes would be wrong
//! on every flat plate.

use std::collections::BTreeMap;

use crate::fingerprint::Surface;

use super::brep::Solid;
use super::model::{ShapeClass, Surfaces};

/// How nearly parallel two directions must be to count as one axis.
const PARALLEL: f64 = 1e-6;

/// What the faces of `solid` lie on.
pub fn surfaces(solid: &Solid) -> Surfaces {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut without_closed_form = 0usize;
    for face in &solid.faces {
        *counts.entry(face.kind.clone()).or_default() += 1;
        if face.surface.is_none() {
            without_closed_form += 1;
        }
    }
    Surfaces {
        counts,
        without_closed_form,
    }
}

/// What kind of shape `solid` is, where a rule settles it.
pub fn class_of(solid: &Solid) -> Option<ShapeClass> {
    if solid.faces.is_empty() {
        return None;
    }
    // A face the file gives no closed form for is a free-form face, and
    // one of them makes the body one: nothing else can be concluded
    // about a shape with a surface this reader cannot describe.
    if solid.faces.iter().any(|f| f.surface.is_none()) {
        return Some(ShapeClass::FreeForm);
    }
    if turned(solid) {
        return Some(ShapeClass::Turned);
    }
    if prismatic(solid) {
        return Some(ShapeClass::Prismatic);
    }
    None
}

/// Whether every surface turns about one line.
///
/// That is what a lathe does: the tool moves along one axis and the work
/// spins about it, so every surface it can cut is a surface of
/// revolution about that axis, and every flat it can face is
/// perpendicular to it.
fn turned(solid: &Solid) -> bool {
    let Some(axis) = common_axis(solid) else {
        return false;
    };
    solid.faces.iter().all(|f| match &f.surface {
        // A face cut on a lathe is square to the axis.
        Some(Surface::Plane { axis: normal, .. }) => parallel(*normal, axis),
        Some(Surface::Cylinder { axis: a, .. })
        | Some(Surface::Cone { axis: a, .. })
        | Some(Surface::Torus { axis: a, .. }) => parallel(*a, axis),
        // A ball end is a surface of revolution about any line through
        // its centre, so it settles nothing and breaks nothing.
        Some(Surface::Sphere { .. }) => true,
        _ => false,
    })
}

/// Whether the body could be cut from one direction: every face either
/// flat, or a wall running the same way as every other wall.
fn prismatic(solid: &Solid) -> bool {
    let Some(axis) = common_axis(solid) else {
        return false;
    };
    // A part with nothing but planes is prismatic in any direction, and
    // there is no common axis to find; that case is handled below.
    solid.faces.iter().all(|f| match &f.surface {
        Some(Surface::Plane { .. }) => true,
        Some(Surface::Cylinder { axis: a, .. })
        | Some(Surface::Cone { axis: a, .. })
        | Some(Surface::Torus { axis: a, .. }) => parallel(*a, axis),
        _ => false,
    }) || all_planes(solid)
}

/// Whether every face is flat, which makes the body prismatic whichever
/// way it was cut.
fn all_planes(solid: &Solid) -> bool {
    solid
        .faces
        .iter()
        .all(|f| matches!(f.surface, Some(Surface::Plane { .. })))
}

/// The one line every surface of revolution in the body turns about, if
/// they all turn about one.
fn common_axis(solid: &Solid) -> Option<[f64; 3]> {
    let mut found: Option<[f64; 3]> = None;
    for face in &solid.faces {
        let axis = match &face.surface {
            Some(Surface::Cylinder { axis, .. })
            | Some(Surface::Cone { axis, .. })
            | Some(Surface::Torus { axis, .. }) => *axis,
            _ => continue,
        };
        match found {
            None => found = Some(axis),
            Some(first) if parallel(axis, first) => {}
            Some(_) => return None,
        }
    }
    // Nothing turns, so every direction is the common one; the planes
    // decide it. Pick the first plane's normal so the caller has a line
    // to test against.
    found.or_else(|| {
        solid.faces.iter().find_map(|f| match &f.surface {
            Some(Surface::Plane { axis, .. }) => Some(*axis),
            _ => None,
        })
    })
}

/// Whether two directions lie along one line, whichever way each points.
fn parallel(a: [f64; 3], b: [f64; 3]) -> bool {
    let cross = [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ];
    cross.iter().all(|v| v.abs() < PARALLEL)
}
