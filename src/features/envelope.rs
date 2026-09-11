//! How big a body is (ADR 0014).
//!
//! The first thing a catalogue asks for and the last thing this project
//! could say. A hole states the diameter it is called by; nothing stated
//! how big the thing holding it was.
//!
//! **What this computes and what it does not.** An axis-aligned box, and
//! the overall size that box implies. Not volume and not surface area:
//! those need the trimmed patch of each face, and [`Solid`] holds the
//! surface a face lies on and the edges around it but not the patch
//! between them. Integrating that needs the geometry kernel ADR 0001
//! declined to depend on, so volume and surface area are read where a
//! file states them and absent where it does not (ADR 0014, amended).
//!
//! **Why a box can still be exact.** A box is decided by extremes, and
//! for the shapes this reads, the extremes are all on something the
//! B-rep does state: a vertex, a full circle, or a sphere. A plate's
//! corners are vertices. A shaft's widest point is the circle capping
//! it. What is left over — an arc bulging past its own endpoints, a face
//! the file gives no closed form for, a torus — can reach beyond
//! anything stated, and there the box is a lower bound and says so.

use crate::fingerprint::Surface;

use super::brep::{CurveKind, Solid};
use super::model::Envelope;

/// The box around `solid`, in the units the solid is already in.
///
/// `None` when the file locates nothing at all, which happens for a
/// tessellation-only export: no box is better than a box around the
/// origin.
pub fn of(solid: &Solid) -> Option<Envelope> {
    let mut box_ = Box3::default();
    // Shapes that can reach past every point the file states, each with
    // the box it certainly stays inside. Settled after the rest, because
    // one that stays inside the box the rest makes cannot have widened
    // it, whatever it does within.
    let mut bulges: Vec<([f64; 3], [f64; 3])> = Vec::new();
    // Something that can reach past the points around it and cannot be
    // bounded at all.
    let mut unbounded = false;

    for edge in &solid.edges {
        for p in &edge.ends {
            box_.add(*p);
        }
        match (edge.curve, edge.centre, edge.axis, edge.radius) {
            (CurveKind::Circle, Some(centre), Some(axis), Some(radius)) => {
                // A circle reaches `radius` from its centre in every
                // direction perpendicular to its axis, which in each
                // coordinate is what is left of the radius after the
                // axis has taken its share.
                let reach = |i: usize| radius * (1.0 - axis[i] * axis[i]).max(0.0).sqrt();
                let circle = (
                    [
                        centre[0] - reach(0),
                        centre[1] - reach(1),
                        centre[2] - reach(2),
                    ],
                    [
                        centre[0] + reach(0),
                        centre[1] + reach(1),
                        centre[2] + reach(2),
                    ],
                );
                if is_whole(edge) {
                    box_.add(circle.0);
                    box_.add(circle.1);
                } else {
                    // An arc bulges past its own ends, and the file does
                    // not say by how much without the parameter range.
                    // It cannot leave the circle it lies on, though, so
                    // that is the bound to settle it against.
                    bulges.push(circle);
                }
            }
            (CurveKind::Ellipse | CurveKind::Other, ..) => unbounded = true,
            _ => {}
        }
    }

    for face in &solid.faces {
        match &face.surface {
            // A sphere's extreme is its pole, which is on no edge.
            Some(Surface::Sphere { origin, radius }) => {
                box_.add([origin[0] - radius, origin[1] - radius, origin[2] - radius]);
                box_.add([origin[0] + radius, origin[1] + radius, origin[2] + radius]);
            }
            // A torus can bulge away from every edge bounding it, but
            // never outside the ring it turns on.
            Some(Surface::Torus {
                origin,
                axis,
                major_radius,
                minor_radius,
            }) => {
                let reach = |i: usize| {
                    let across = (1.0 - axis[i] * axis[i]).max(0.0).sqrt();
                    (major_radius + minor_radius) * across + minor_radius * axis[i].abs()
                };
                bulges.push((
                    [
                        origin[0] - reach(0),
                        origin[1] - reach(1),
                        origin[2] - reach(2),
                    ],
                    [
                        origin[0] + reach(0),
                        origin[1] + reach(1),
                        origin[2] + reach(2),
                    ],
                ));
            }
            // A face the file gives no closed form for can go anywhere.
            None | Some(Surface::Other(_)) => unbounded = true,
            _ => {}
        }
    }

    let (min, max) = box_.finish()?;

    // A bulge that stays inside the box everything else made cannot have
    // made the box wrong. Most arcs are the mouth of a hole or a broken
    // corner, well within the part, so this is what keeps a plate with
    // rounded corners an exact measurement rather than a lower bound.
    let q = crate::fingerprint::identity_quantum();
    let escapes = bulges
        .iter()
        .any(|(lo, hi)| (0..3).any(|i| lo[i] < min[i] - q || hi[i] > max[i] + q));

    let mut size = [max[0] - min[0], max[1] - min[1], max[2] - min[2]];
    size.sort_by(|a, b| b.total_cmp(a));
    Some(Envelope {
        min: rounded(min),
        max: rounded(max),
        size: rounded(size),
        approximate: unbounded || escapes,
    })
}

/// Whether a circular edge closes on itself rather than being an arc of
/// one.
///
/// A file states a full circle either without vertices at all or with
/// its two ends at the same point, which is how a seam is written. An
/// arc has two ends that differ.
fn is_whole(edge: &super::brep::Edge) -> bool {
    match edge.ends.len() {
        0 | 1 => true,
        _ => {
            let (a, b) = (edge.ends[0], edge.ends[1]);
            (0..3).all(|i| (a[i] - b[i]).abs() < crate::fingerprint::identity_quantum())
        }
    }
}

/// Numbers are reported at the identity quantum, as every other measured
/// number in this document is, so that two exports of one design state
/// the same size (ADR 0004).
fn rounded(p: [f64; 3]) -> [f64; 3] {
    let q = crate::fingerprint::identity_quantum();
    [
        crate::identity::num(p[0], q).parse().unwrap_or(p[0]),
        crate::identity::num(p[1], q).parse().unwrap_or(p[1]),
        crate::identity::num(p[2], q).parse().unwrap_or(p[2]),
    ]
}

/// The running extremes, kept per axis.
///
/// Per axis rather than as two points, because a circle constrains the
/// axes separately: it can say how far the shape reaches in x without
/// saying anything about z.
#[derive(Debug, Default)]
struct Box3 {
    axes: [Option<(f64, f64)>; 3],
}

impl Box3 {
    fn add(&mut self, p: [f64; 3]) {
        for (i, v) in p.into_iter().enumerate() {
            self.add_axis(i, v);
        }
    }

    fn add_axis(&mut self, i: usize, v: f64) {
        if !v.is_finite() {
            return;
        }
        self.axes[i] = Some(match self.axes[i] {
            Some((lo, hi)) => (lo.min(v), hi.max(v)),
            None => (v, v),
        });
    }

    /// The box, or nothing when any axis was never constrained.
    fn finish(self) -> Option<([f64; 3], [f64; 3])> {
        let mut min = [0.0; 3];
        let mut max = [0.0; 3];
        for i in 0..3 {
            let (lo, hi) = self.axes[i]?;
            min[i] = lo;
            max[i] = hi;
        }
        Some((min, max))
    }
}
