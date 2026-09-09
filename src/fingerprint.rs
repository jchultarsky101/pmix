//! Fingerprints of B-rep geometry, shared by the readers (ADR 0004).
//!
//! A fingerprint is a string built from the numbers that fix a face in
//! model space, rounded to the identity quantum, so the same face in two
//! exports of one design yields the same string. It is what anchors a
//! dimension or a tolerance to the thing it is about.
//!
//! The recipe lives here rather than with either reader because both
//! must produce the same string for the same face, and the only way to
//! be sure of that is for there to be one recipe. What each reader
//! supplies is a [`Surface`] and the vertices bounding the face, however
//! its format states them, in model units.

use crate::identity::{QUANTUM, num, triple};

/// Directions are rounded more coarsely than positions: a unit vector's
/// components are of order one, so the same tolerance would demand far
/// more agreement of a direction than of a coordinate.
const DIRECTION_QUANTUM: f64 = 1e-3;

/// An analytic surface, in model units.
///
/// Only the kinds with a closed form are described. A face on anything
/// else is [`Surface::Other`], named by its kind, which keeps such faces
/// apart from each other by the vertices they span but not by shape.
#[derive(Debug, Clone, PartialEq)]
pub enum Surface {
    Plane {
        origin: [f64; 3],
        axis: [f64; 3],
    },
    Cylinder {
        origin: [f64; 3],
        axis: [f64; 3],
        radius: f64,
    },
    Cone {
        origin: [f64; 3],
        axis: [f64; 3],
        radius: f64,
        /// Half the angle at the apex, in radians.
        semi_angle: f64,
    },
    Sphere {
        origin: [f64; 3],
        radius: f64,
    },
    Torus {
        origin: [f64; 3],
        axis: [f64; 3],
        major_radius: f64,
        minor_radius: f64,
    },
    /// A surface with no closed form here, or one whose kind is known
    /// but which the file did not locate.
    Other(String),
}

/// Surfaces of revolution, whose faces span an extent along an axis.
const REVOLVED: [&str; 3] = ["cylinder", "cone", "torus"];

/// The key for the surface alone, without the part of it a face covers.
pub fn surface_key(s: &Surface, q: f64) -> String {
    let dq = DIRECTION_QUANTUM;
    match s {
        // A plane is fixed by its normal and its distance from the model
        // origin, so two exports that place it on different points of
        // the same plane still agree.
        Surface::Plane { origin, axis } => {
            let n = sign_normalise(normalise(*axis));
            format!("plane/{}/{}", triple(n, dq), num(dot(n, *origin), q))
        }
        Surface::Cylinder {
            origin,
            axis,
            radius,
        } => {
            let a = sign_normalise(normalise(*axis));
            let c = closest_on_axis(*origin, a);
            format!(
                "cylinder/{}/{}/{}",
                triple(a, dq),
                triple(c, q),
                num(*radius, q)
            )
        }
        // A cone's radius is stated at its origin, so the axis keeps its
        // sense: flipping it would describe a different cone.
        Surface::Cone {
            origin,
            axis,
            radius,
            semi_angle,
        } => {
            let a = normalise(*axis);
            let c = closest_on_axis(*origin, a);
            format!(
                "cone/{}/{}/{}/{}",
                triple(a, dq),
                triple(c, q),
                num(*radius, q),
                num(*semi_angle, dq)
            )
        }
        Surface::Sphere { origin, radius } => {
            format!("sphere/{}/{}", triple(*origin, q), num(*radius, q))
        }
        Surface::Torus {
            origin,
            axis,
            major_radius,
            minor_radius,
        } => {
            let a = sign_normalise(normalise(*axis));
            format!(
                "torus/{}/{}/{}/{}",
                triple(*origin, q),
                triple(a, dq),
                num(*major_radius, q),
                num(*minor_radius, q)
            )
        }
        Surface::Other(name) => name.clone(),
    }
}

/// The key for a face: its surface, and how much of that surface it is.
///
/// The span is chosen so that a face split in two by a re-export keeps
/// the same key as the whole face did wherever that is possible: the
/// extent along the axis for a surface of revolution, because a cylinder
/// re-cut at a new seam still runs the same length; nothing at all for a
/// sphere; the box the vertices fill otherwise.
pub fn face_key(s: &Surface, vertices: &[[f64; 3]], q: f64) -> String {
    let key = surface_key(s, q);
    let span = match s {
        Surface::Cylinder { origin, axis, .. }
        | Surface::Cone { origin, axis, .. }
        | Surface::Torus { origin, axis, .. } => axial_extent(*origin, *axis, vertices, q),
        Surface::Sphere { .. } => String::new(),
        // An unlocated surface of revolution has no axis to measure
        // along, so it contributes no span rather than a misleading box.
        Surface::Other(name) if REVOLVED.contains(&name.as_str()) || name == "sphere" => {
            String::new()
        }
        _ => bbox(vertices)
            .map(|(lo, hi)| format!("/{}/{}", triple(lo, q), triple(hi, q)))
            .unwrap_or_default(),
    };
    format!("{key}{span}")
}

/// The identity key of a feature: what kind of feature it is, the
/// geometry it is made of, and its own extent.
///
/// `kind` is empty where it says nothing the geometry does not, which is
/// so for a feature that is simply a face or an edge. `keys` must be
/// sorted and deduplicated by the caller, because a feature is the same
/// feature however its geometry happened to be listed.
pub fn feature_key(kind: &str, keys: &[String], span: &str) -> String {
    [kind, &keys.join(";"), span].join("|")
}

/// [`feature_key`] for a feature that is one piece of geometry and
/// nothing else: a face, an edge, a vertex.
pub fn single_feature_key(geometry_key: &str) -> String {
    feature_key("", std::slice::from_ref(&geometry_key.to_owned()), "")
}

/// The key for an edge: the kind of curve it runs along, and its ends.
///
/// The ends are sorted, because an edge is the same edge whichever way
/// round a file states it.
pub fn edge_key(curve: &str, ends: &[[f64; 3]], q: f64) -> String {
    let mut ends: Vec<String> = ends.iter().map(|p| triple(*p, q)).collect();
    ends.sort();
    format!("edge/{curve}/{}", ends.join("/"))
}

/// `[min, max]` of the vertices projected onto the surface's axis.
fn axial_extent(origin: [f64; 3], axis: [f64; 3], vertices: &[[f64; 3]], q: f64) -> String {
    if vertices.is_empty() {
        return String::new();
    }
    let a = sign_normalise(normalise(axis));
    let c = closest_on_axis(origin, a);
    let ts = vertices
        .iter()
        .map(|v| dot([v[0] - c[0], v[1] - c[1], v[2] - c[2]], a));
    let (lo, hi) = ts.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), t| {
        (lo.min(t), hi.max(t))
    });
    format!("/t{}..{}", num(lo, q), num(hi, q))
}

pub fn normalise(v: [f64; 3]) -> [f64; 3] {
    let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if n == 0.0 {
        v
    } else {
        [v[0] / n, v[1] / n, v[2] / n]
    }
}

/// Flip a direction so its first non-negligible component is positive.
///
/// An axis says which line a surface turns about, not which way along
/// it; two exports may state opposite senses of the same line.
pub fn sign_normalise(d: [f64; 3]) -> [f64; 3] {
    for c in d {
        if c.abs() > 1e-9 {
            return if c < 0.0 { [-d[0], -d[1], -d[2]] } else { d };
        }
    }
    d
}

pub fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Point on the axis through `origin` closest to the model origin.
///
/// Two exports may place the same axis at different points along it, so
/// the key names the one point on it that neither export chose.
pub fn closest_on_axis(origin: [f64; 3], axis: [f64; 3]) -> [f64; 3] {
    let t = dot(origin, axis);
    [
        origin[0] - t * axis[0],
        origin[1] - t * axis[1],
        origin[2] - t * axis[2],
    ]
}

pub fn bbox(pts: &[[f64; 3]]) -> Option<([f64; 3], [f64; 3])> {
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

/// The quantum a fingerprint rounds with, in model units.
pub fn identity_quantum() -> f64 {
    QUANTUM
}

#[cfg(test)]
mod tests {
    use super::*;

    const Q: f64 = QUANTUM;

    #[test]
    fn a_plane_is_its_normal_and_its_distance() {
        // The same plane described from two points on it, with opposite
        // senses of its normal, is one key.
        let a = Surface::Plane {
            origin: [0.0, 0.0, 5.0],
            axis: [0.0, 0.0, 1.0],
        };
        let b = Surface::Plane {
            origin: [100.0, -40.0, 5.0],
            axis: [0.0, 0.0, -1.0],
        };
        assert_eq!(surface_key(&a, Q), surface_key(&b, Q));
        // A different plane is a different key.
        let c = Surface::Plane {
            origin: [0.0, 0.0, 6.0],
            axis: [0.0, 0.0, 1.0],
        };
        assert_ne!(surface_key(&a, Q), surface_key(&c, Q));
    }

    #[test]
    fn a_cylinder_is_its_line_and_its_radius() {
        // Two placements along one axis, stated in opposite senses.
        let a = Surface::Cylinder {
            origin: [3.0, 4.0, 0.0],
            axis: [0.0, 0.0, 1.0],
            radius: 2.5,
        };
        let b = Surface::Cylinder {
            origin: [3.0, 4.0, 90.0],
            axis: [0.0, 0.0, -1.0],
            radius: 2.5,
        };
        assert_eq!(surface_key(&a, Q), surface_key(&b, Q));
        let wider = Surface::Cylinder {
            origin: [3.0, 4.0, 0.0],
            axis: [0.0, 0.0, 1.0],
            radius: 3.0,
        };
        assert_ne!(surface_key(&a, Q), surface_key(&wider, Q));
    }

    #[test]
    fn a_face_on_a_cylinder_spans_its_length_not_its_box() {
        let cylinder = Surface::Cylinder {
            origin: [0.0, 0.0, 0.0],
            axis: [0.0, 0.0, 1.0],
            radius: 5.0,
        };
        // The same tube, cut at two different seams: the vertices differ
        // but the extent along the axis does not.
        let one = [[5.0, 0.0, 0.0], [5.0, 0.0, 10.0], [-5.0, 0.0, 10.0]];
        let other = [[0.0, 5.0, 0.0], [0.0, -5.0, 10.0], [0.0, 5.0, 10.0]];
        assert_eq!(face_key(&cylinder, &one, Q), face_key(&cylinder, &other, Q));
        // A shorter tube on the same cylinder is a different face.
        let short = [[5.0, 0.0, 0.0], [5.0, 0.0, 4.0]];
        assert_ne!(face_key(&cylinder, &one, Q), face_key(&cylinder, &short, Q));
    }

    #[test]
    fn a_face_on_a_plane_spans_the_box_its_vertices_fill() {
        let plane = Surface::Plane {
            origin: [0.0, 0.0, 0.0],
            axis: [0.0, 0.0, 1.0],
        };
        let square = [
            [0.0; 3],
            [10.0, 0.0, 0.0],
            [10.0, 4.0, 0.0],
            [0.0, 4.0, 0.0],
        ];
        let key = face_key(&plane, &square, Q);
        assert!(key.starts_with("plane/"), "{key}");
        assert!(key.contains("10.0000,4.0000"), "{key}");
        // Two different patches of one plane are two faces.
        let smaller = [[0.0; 3], [10.0, 0.0, 0.0], [10.0, 3.0, 0.0]];
        assert_ne!(key, face_key(&plane, &smaller, Q));
    }

    #[test]
    fn a_sphere_has_no_span_because_it_has_no_axis_to_span() {
        let sphere = Surface::Sphere {
            origin: [1.0, 2.0, 3.0],
            radius: 4.0,
        };
        let a = [[5.0, 2.0, 3.0]];
        let b = [[1.0, 2.0, 7.0], [-3.0, 2.0, 3.0]];
        assert_eq!(face_key(&sphere, &a, Q), face_key(&sphere, &b, Q));
    }

    #[test]
    fn an_unlocated_surface_of_revolution_claims_no_span() {
        // Nothing locates it, so the vertices say where the face is but
        // not which part of the surface it covers.
        let unknown = Surface::Other("cylinder".into());
        let a = [[0.0; 3], [1.0, 0.0, 0.0]];
        assert_eq!(face_key(&unknown, &a, Q), "cylinder");
        // A surface with no closed form is kept apart by its vertices.
        let spline = Surface::Other("b_spline_surface".into());
        assert_ne!(face_key(&spline, &a, Q), face_key(&spline, &[], Q));
    }

    #[test]
    fn an_edge_is_its_curve_and_its_ends_whichever_way_round() {
        let a = [0.0, 0.0, 0.0];
        let b = [10.0, 0.0, 0.0];
        assert_eq!(edge_key("line", &[a, b], Q), edge_key("line", &[b, a], Q));
        // A different curve between the same points is a different edge.
        assert_ne!(edge_key("line", &[a, b], Q), edge_key("circle", &[a, b], Q));
        // As is the same curve between different points.
        assert_ne!(edge_key("line", &[a, b], Q), edge_key("line", &[a, a], Q));
        assert!(edge_key("line", &[a, b], Q).starts_with("edge/line/"));
    }

    #[test]
    fn a_direction_of_no_length_is_left_alone() {
        assert_eq!(normalise([0.0; 3]), [0.0; 3]);
        assert_eq!(sign_normalise([0.0; 3]), [0.0; 3]);
        // A negligible first component does not decide the sign.
        assert_eq!(sign_normalise([0.0, -1.0, 0.0]), [0.0, 1.0, 0.0]);
    }

    #[test]
    fn the_point_named_on_an_axis_is_the_one_neither_export_chose() {
        let axis = [0.0, 0.0, 1.0];
        let a = closest_on_axis([3.0, 4.0, 17.0], axis);
        let b = closest_on_axis([3.0, 4.0, -900.0], axis);
        assert_eq!(a, b);
        assert_eq!(a, [3.0, 4.0, 0.0]);
    }
}
