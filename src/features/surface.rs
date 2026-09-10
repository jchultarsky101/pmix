//! Evaluating a surface where two faces meet.
//!
//! Telling a fillet from a chamfer is a question about the join, not
//! about either face: a blend leaves the surface smooth, and a chamfer
//! puts a corner in it. Answering it needs the normal of each face at a
//! point they share, which is the one piece of geometry recognition
//! needs that identity never did.

use crate::fingerprint::{self, Surface};

/// How nearly parallel two normals must be to count as one.
///
/// About one and a half degrees. Tangency in an exported file is exact
/// to far better than that, and the loosest thing this must still tell
/// apart is a chamfer, whose smallest useful angle is several degrees.
/// So the threshold sits in a wide gap rather than on a boundary, and
/// nothing turns on its precise value.
const COS_TANGENT: f64 = 0.999;

/// The outward normal of `surface` at `point`, or `None` where the
/// surface has no normal there or none this can evaluate.
///
/// "Outward" is the surface's own sense: away from the axis of a
/// cylinder, away from the centre of a sphere, away from the tube of a
/// torus. Which way the *face* looks is that turned round when the face
/// does not follow its surface, which is what [`face_normal`] does.
pub fn normal_at(surface: &Surface, point: [f64; 3]) -> Option<[f64; 3]> {
    let unit = |v: [f64; 3]| {
        let n = fingerprint::normalise(v);
        (n != [0.0, 0.0, 0.0]).then_some(n)
    };
    let sub = |a: [f64; 3], b: [f64; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    // The part of `p - origin` at right angles to the axis.
    let radial = |origin: [f64; 3], axis: [f64; 3]| {
        let a = fingerprint::normalise(axis);
        let d = sub(point, origin);
        let along = fingerprint::dot(d, a);
        unit([
            d[0] - along * a[0],
            d[1] - along * a[1],
            d[2] - along * a[2],
        ])
    };
    match surface {
        Surface::Plane { axis, .. } => unit(*axis),
        Surface::Cylinder { origin, axis, .. } => radial(*origin, *axis),
        Surface::Cone {
            origin,
            axis,
            semi_angle,
            ..
        } => {
            // A cone's normal leans out of the radial direction by the
            // half angle at its apex, towards the narrow end. The angle
            // is in degrees, as everything in the neutral B-rep is.
            let r = radial(*origin, *axis)?;
            let a = fingerprint::normalise(*axis);
            let (c, s) = (semi_angle.to_radians().cos(), semi_angle.to_radians().sin());
            unit([
                c * r[0] - s * a[0],
                c * r[1] - s * a[1],
                c * r[2] - s * a[2],
            ])
        }
        Surface::Sphere { origin, .. } => unit(sub(point, *origin)),
        Surface::Torus {
            origin,
            axis,
            major_radius,
            ..
        } => {
            // The normal points away from the circle the tube is swept
            // along, so it is measured from the nearest point of that
            // circle rather than from the centre.
            let r = radial(*origin, *axis)?;
            let centre = [
                origin[0] + major_radius * r[0],
                origin[1] + major_radius * r[1],
                origin[2] + major_radius * r[2],
            ];
            unit(sub(point, centre))
        }
        Surface::Other(_) => None,
    }
}

/// The normal of a face at `point`: its surface's, turned round when the
/// face does not follow it.
pub fn face_normal(surface: &Surface, same_sense: bool, point: [f64; 3]) -> Option<[f64; 3]> {
    let n = normal_at(surface, point)?;
    Some(if same_sense { n } else { [-n[0], -n[1], -n[2]] })
}

/// Whether two normals are the same direction.
pub fn parallel(a: [f64; 3], b: [f64; 3]) -> bool {
    fingerprint::dot(a, b) >= COS_TANGENT
}

#[cfg(test)]
mod tests {
    use super::*;

    const Z: [f64; 3] = [0.0, 0.0, 1.0];
    const O: [f64; 3] = [0.0, 0.0, 0.0];

    #[test]
    fn a_cylinder_looks_away_from_its_axis() {
        let s = Surface::Cylinder {
            origin: O,
            axis: Z,
            radius: 2.0,
        };
        assert_eq!(normal_at(&s, [2.0, 0.0, 5.0]), Some([1.0, 0.0, 0.0]));
        // A bore's wall does not follow its surface, so it looks inward.
        assert_eq!(
            face_normal(&s, false, [2.0, 0.0, 5.0]),
            Some([-1.0, 0.0, 0.0])
        );
        // On the axis there is no direction to give.
        assert_eq!(normal_at(&s, [0.0, 0.0, 5.0]), None);
    }

    #[test]
    fn a_torus_looks_away_from_the_circle_it_is_swept_along() {
        let s = Surface::Torus {
            origin: O,
            axis: Z,
            major_radius: 10.0,
            minor_radius: 2.0,
        };
        // Directly outboard of the tube.
        assert_eq!(normal_at(&s, [12.0, 0.0, 0.0]), Some([1.0, 0.0, 0.0]));
        // Directly above it.
        assert_eq!(normal_at(&s, [10.0, 0.0, 2.0]), Some(Z));
    }

    /// A block filling the quarter space `x < 0, z < 0`, with the edge
    /// along Y at the origin rounded off at radius 2. The round's
    /// surface is a cylinder on the axis `(-2, y, -2)`, and where it
    /// meets the top face at `(-2, y, 0)` both look the same way up.
    /// That agreement is what makes the join a blend rather than a
    /// corner, and it is the whole of the test.
    #[test]
    fn a_blend_leaves_the_surface_smooth_and_a_chamfer_does_not() {
        let top = Surface::Plane { origin: O, axis: Z };
        let round = Surface::Cylinder {
            origin: [-2.0, 0.0, -2.0],
            axis: [0.0, 1.0, 0.0],
            radius: 2.0,
        };
        let up = face_normal(&top, true, [-5.0, 0.0, 0.0]).unwrap();
        assert_eq!(up, Z);
        let blend = face_normal(&round, true, [-2.0, 0.0, 0.0]).unwrap();
        assert!(parallel(up, blend), "{up:?} {blend:?}");

        // Chamfer the same corner at forty-five degrees instead and the
        // two no longer agree, however close the point of contact.
        let chamfer = Surface::Cone {
            origin: O,
            axis: Z,
            radius: 2.0,
            semi_angle: 45.0,
        };
        let cut = face_normal(&chamfer, true, [2.0, 0.0, 0.0]).unwrap();
        assert!(!parallel(up, cut), "{up:?} {cut:?}");
    }
}
