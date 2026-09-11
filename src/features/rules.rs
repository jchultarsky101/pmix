//! Recognising features from local rules (ADR 0011).
//!
//! Every rule here is decided by a surface kind, the curves bounding a
//! face, which faces meet across those curves, and which way a face
//! looks. Nothing needs a global view of the solid, and nothing chooses
//! between two readings of the same material: where a rule cannot settle
//! something, the output says so instead of guessing.

use std::collections::{BTreeMap, BTreeSet};

use crate::fingerprint::{self, Scale, Surface};
use crate::identity::{num, triple};
use crate::model::ContentId;

use super::brep::{CurveKind, Solid};
use super::model::{Body, FaceCounts, Feature, Kind, Shape, UnassignedFace};
use super::surface;

/// How close two numbers must be to count as the same. The identity
/// quantum (ADR 0004): a fixed thousandth of a millimetre, so that what
/// counts as coaxial does not depend on export settings.
fn tolerance() -> f64 {
    fingerprint::identity_quantum()
}

/// A surface a rule might claim, before the rules run.
///
/// It is a *set* of faces, not one: exporters routinely cut a cylinder
/// into halves meeting along two straight edges, and a rule that asks
/// what bounds a bore has to ask it of the whole bore. Every face here
/// lies on one surface, so the group is a face of that surface however
/// the file happened to divide it.
#[derive(Debug, Clone)]
struct Candidate {
    /// The faces the group is made of, sorted.
    faces: Vec<usize>,
    /// The edges bounding the group: those not shared with another face
    /// of the same group, and not a seam.
    boundary: Vec<usize>,
    /// The line the surface turns about, as a canonical key, so that
    /// coaxial surfaces group by string equality rather than by a
    /// pairwise distance test.
    axis_key: String,
    axis: [f64; 3],
    position: [f64; 3],
    /// The surface itself, exactly as the file states it (in
    /// millimetres). Identity is taken from this rather than rebuilt
    /// from the canonical axis and position below: those describe the
    /// *line* the surface turns about, and pairing one of them with a
    /// radius measured somewhere else on that line describes a surface
    /// that is not in the file.
    surface: Surface,
    radius: f64,
    /// Half the apex angle in radians, for a cone.
    semi_angle: Option<f64>,
    /// The radius of the tube, for a torus. That is the radius a blend
    /// is called by; `radius` then holds the circle the tube is swept
    /// along, which is what puts the surface in space.
    minor_radius: Option<f64>,
    /// Whether every edge bounding the group is a circle, seams aside.
    /// A bore is; a blend running along an edge is not.
    circular: bool,
    /// The largest circle bounding the face. A cylinder's circles are
    /// all its own radius; a cone's differ, and the wider one is what a
    /// countersink is called by.
    major_radius: f64,
    /// The stretch of the axis the face occupies.
    extent: [f64; 2],
    /// Whether the face looks away from its axis: a shaft rather than a
    /// bore.
    outward: bool,
}

/// The axis of a surface, in the fixed sign and with the canonical point
/// on it, so that two exports of one design describe one line one way.
fn axis_of(axis: [f64; 3], origin: [f64; 3]) -> ([f64; 3], [f64; 3]) {
    let a = fingerprint::sign_normalise(fingerprint::normalise(axis));
    (a, fingerprint::closest_on_axis(origin, a))
}

/// How far along `axis` a point lies.
fn along(axis: [f64; 3], p: [f64; 3]) -> f64 {
    fingerprint::dot(axis, p)
}

/// The stretch of `axis` covered by the edges in `edges`.
fn extent_of(solid: &Solid, edges: &[usize], axis: [f64; 3]) -> Option<[f64; 2]> {
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    let mut any = false;
    for e in edges.iter().copied() {
        let edge = &solid.edges[e];
        let points: Vec<[f64; 3]> = match edge.centre {
            Some(c) => vec![c],
            None => edge.ends.clone(),
        };
        for p in points {
            let t = along(axis, p);
            lo = lo.min(t);
            hi = hi.max(t);
            any = true;
        }
    }
    any.then_some([lo, hi])
}

/// Whether `face` closes off a bore rather than continuing it.
///
/// A plane bounded by the circle closes it: that is the bottom of a
/// blind hole and the floor of a counterbore. The plate face a through
/// hole opens onto is also a plane, but there the circle is a hole cut
/// in it rather than its boundary, which is the difference both formats
/// state. A cone closes it only when it converges to a point, as a drill
/// tip does; a cone with a circle at each end is a chamfer or a
/// countersink, and the bore continues past it.
fn closes(solid: &Solid, face: usize, edge: usize) -> bool {
    let f = &solid.faces[face];
    if !f.is_outer(edge) {
        return false;
    }
    match &f.surface {
        Some(Surface::Plane { .. }) => true,
        Some(Surface::Cone { .. }) => {
            f.edges()
                .filter(|e| solid.edges[*e].curve == CurveKind::Circle)
                .count()
                == 1
        }
        _ => false,
    }
}

/// The face closing the end of `c` at `t` along its axis, if any.
fn cap_at(solid: &Solid, c: &Candidate, t: f64) -> Option<usize> {
    let tol = tolerance();
    for e in c.boundary.iter().copied() {
        let edge = &solid.edges[e];
        if edge.curve != CurveKind::Circle {
            continue;
        }
        let Some(centre) = edge.centre else { continue };
        if (along(c.axis, centre) - t).abs() > tol {
            continue;
        }
        for other in solid.across.get(&e).into_iter().flatten() {
            if !c.faces.contains(other) && closes(solid, *other, e) {
                return Some(*other);
            }
        }
    }
    None
}

/// How a face is grouped with the others lying on the same surface.
///
/// The identity recipe's own key for that surface (ADR 0004), because
/// that is exactly the question: do these two faces lie on one surface?
/// Building a key by hand here got it wrong for cones, whose radius is
/// the radius *where the file placed them* — six patches of one cone,
/// placed six different ways, compared as six surfaces.
fn surface_group(surface: &Surface) -> String {
    fingerprint::surface_key(surface, Scale::NONE)
}

/// The surfaces a rule could claim, each as the whole set of faces the
/// file cut it into.
fn candidates(solid: &Solid) -> Vec<Candidate> {
    // Every cylindrical or conical face, with what identifies the
    // surface it lies on.
    struct Part {
        face: usize,
        axis_key: String,
        axis: [f64; 3],
        position: [f64; 3],
        surface: Surface,
        radius: f64,
        semi_angle: Option<f64>,
        minor_radius: Option<f64>,
    }
    let mut parts: Vec<Part> = Vec::new();
    for (i, f) in solid.faces.iter().enumerate() {
        let Some(surface) = f.surface.clone() else {
            continue;
        };
        let (axis, origin, radius, semi_angle, minor_radius) = match &f.surface {
            Some(Surface::Cylinder {
                origin,
                axis,
                radius,
            }) => (*axis, *origin, *radius, None, None),
            Some(Surface::Cone {
                origin,
                axis,
                radius,
                semi_angle,
            }) => (*axis, *origin, *radius, Some(*semi_angle), None),
            Some(Surface::Torus {
                origin,
                axis,
                major_radius,
                minor_radius,
            }) => (*axis, *origin, *major_radius, None, Some(*minor_radius)),
            _ => continue,
        };
        let (axis, position) = axis_of(axis, origin);
        let q = tolerance();
        parts.push(Part {
            face: i,
            axis_key: format!("{}|{}", triple(axis, q), triple(position, q)),
            axis,
            position,
            surface,
            radius,
            semi_angle,
            minor_radius,
        });
    }

    // Faces on one surface that touch each other are one face of it.
    // Two separate bores of the same size on one axis stay separate,
    // because nothing joins them.
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut by_surface: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (n, p) in parts.iter().enumerate() {
        by_surface
            .entry(surface_group(&p.surface))
            .or_default()
            .push(n);
    }
    for members in by_surface.values() {
        let faces: Vec<usize> = members.iter().map(|n| parts[*n].face).collect();
        // How far along the axis each face reaches, taken before anything
        // is merged, so that faces lying over one another can be told
        // from faces following one behind the other.
        let axis = parts[members[0]].axis;
        let reach: BTreeMap<usize, [f64; 2]> = faces
            .iter()
            .filter_map(|f| {
                let edges: Vec<usize> = solid.faces[*f].edges().collect();
                extent_of(solid, &edges, axis).map(|e| (*f, e))
            })
            .collect();
        let tol = tolerance();
        let over = |x: usize, y: usize| match (reach.get(&x), reach.get(&y)) {
            (Some(p), Some(q)) => p[0] < q[1] - tol && q[0] < p[1] - tol,
            _ => false,
        };
        // Union, repeated until nothing more joins.
        //
        // Touching is not the only way to be one face of a surface: the
        // chamfer round a hexagonal head is six patches of one cone,
        // parted by the six flats, and no two of them touch. What tells
        // those from two bores of one size drilled one behind the other
        // is that the bores follow each other along the axis where these
        // lie over each other.
        let mut merged: Vec<Vec<usize>> = faces.iter().map(|f| vec![*f]).collect();
        let mut changed = true;
        while changed {
            changed = false;
            'outer: for a in 0..merged.len() {
                for b in (a + 1)..merged.len() {
                    let touching = merged[a].iter().any(|x| {
                        merged[b].iter().any(|y| {
                            solid.faces[*x]
                                .edges()
                                .any(|e| solid.faces[*y].is_bounded_by(e))
                                || over(*x, *y)
                        })
                    });
                    if touching {
                        let moved = merged.remove(b);
                        merged[a].extend(moved);
                        changed = true;
                        break 'outer;
                    }
                }
            }
        }
        for mut g in merged {
            g.sort_unstable();
            groups.push(g);
        }
    }

    let mut out = Vec::new();
    for group in groups {
        let first = group[0];
        let Some(p) = parts.iter().find(|p| p.face == first) else {
            continue;
        };
        // What bounds the group: everything not shared with another face
        // of it, and not the seam of a face that closes on itself.
        let mut boundary: Vec<usize> = Vec::new();
        for f in &group {
            for e in solid.faces[*f].edges() {
                if solid.is_seam(*f, e) || boundary.contains(&e) {
                    continue;
                }
                let inside = solid
                    .across
                    .get(&e)
                    .map(|at| at.iter().all(|x| group.contains(x)))
                    .unwrap_or(false);
                if !inside {
                    boundary.push(e);
                }
            }
        }
        boundary.sort_unstable();
        if boundary.is_empty() {
            continue;
        }
        // A bore is bounded by circles and nothing else. A blend running
        // along an edge is not, and used to be dropped here; it is kept
        // now, and the rules that follow sort out which is which.
        let circular = boundary
            .iter()
            .all(|e| solid.edges[*e].curve == CurveKind::Circle);
        let Some(extent) = extent_of(solid, &boundary, p.axis) else {
            continue;
        };
        let major = boundary
            .iter()
            .filter_map(|e| solid.edges[*e].radius)
            .fold(p.radius, f64::max);
        out.push(Candidate {
            faces: group.clone(),
            boundary,
            axis_key: p.axis_key.clone(),
            axis: p.axis,
            position: p.position,
            surface: p.surface.clone(),
            radius: p.radius,
            semi_angle: p.semi_angle,
            minor_radius: p.minor_radius,
            circular,
            major_radius: major,
            extent,
            // A surface of revolution's own normal points away from its
            // axis, and a solid's faces point away from its material.
            outward: solid.faces[first].same_sense,
        });
    }
    out
}

/// Whether the faces meeting across `edge` join smoothly there.
///
/// Smoothly means the two look the same way at the points they share,
/// so the surface has no crease along the edge. Sampling the edge's own
/// ends is enough: both surfaces are analytic, so if they agree there
/// they agree along it.
fn tangent_across(solid: &Solid, a: usize, b: usize, edge: usize) -> bool {
    let (fa, fb) = (&solid.faces[a], &solid.faces[b]);
    let (Some(sa), Some(sb)) = (&fa.surface, &fb.surface) else {
        return false;
    };
    let points = &solid.edges[edge].ends;
    if points.is_empty() {
        return false;
    }
    points.iter().all(|p| {
        match (
            surface::face_normal(sa, fa.same_sense, *p),
            surface::face_normal(sb, fb.same_sense, *p),
        ) {
            (Some(na), Some(nb)) => surface::parallel(na, nb),
            _ => false,
        }
    })
}

/// The faces `c` meets, and which of those it joins smoothly.
///
/// A blend has more neighbours than the two it runs between: its ends
/// stop against whatever is there. So what identifies it is how many
/// neighbours it is *tangent* to, not how many it has.
fn joins(solid: &Solid, c: &Candidate) -> (BTreeSet<usize>, BTreeSet<usize>) {
    let (mut neighbours, mut tangent) = (BTreeSet::new(), BTreeSet::new());
    for e in c.boundary.iter().copied() {
        for other in solid.across.get(&e).into_iter().flatten() {
            if c.faces.contains(other) {
                continue;
            }
            neighbours.insert(*other);
            if c.faces
                .iter()
                .any(|f| solid.faces[*f].is_bounded_by(e) && tangent_across(solid, *f, *other, e))
            {
                tangent.insert(*other);
            }
        }
    }
    (neighbours, tangent)
}

/// Whether two stretches of one axis meet end to end.
fn adjacent(a: [f64; 2], b: [f64; 2]) -> bool {
    let tol = tolerance();
    (a[1] - b[0]).abs() <= tol || (b[1] - a[0]).abs() <= tol
}

/// Everything recognised in one solid, and everything not.
pub fn recognise(solid: &Solid, ids: &mut ContentId) -> Body {
    let face_ids = face_ids(solid, ids);
    let cands = candidates(solid);

    let mut by_axis: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (n, c) in cands.iter().enumerate() {
        by_axis.entry(&c.axis_key).or_default().push(n);
    }

    // A cylindrical bore or shaft is settled by which way it looks. A
    // cone is not a feature on its own: it is a countersink only where
    // it opens into a bore, which is decided next.
    let mut kinds: BTreeMap<usize, Kind> = BTreeMap::new();
    for (n, c) in cands.iter().enumerate() {
        if c.circular && c.semi_angle.is_none() && c.minor_radius.is_none() {
            kinds.insert(n, if c.outward { Kind::Boss } else { Kind::Hole });
        }
    }

    // A bore wider than the one it meets end to end is that one's
    // counterbore; a cone meeting a bore is its countersink.
    let mut coaxial: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (n, c) in cands.iter().enumerate() {
        if c.outward || !c.circular || c.minor_radius.is_some() {
            continue;
        }
        for other in by_axis.get(c.axis_key.as_str()).into_iter().flatten() {
            let o = &cands[*other];
            if *other == n
                || o.outward
                || !o.circular
                || o.semi_angle.is_some()
                || o.minor_radius.is_some()
                || !adjacent(c.extent, o.extent)
            {
                continue;
            }
            coaxial.entry(n).or_default().push(*other);
            if c.semi_angle.is_some() {
                kinds.insert(n, Kind::Countersink);
            } else if c.radius > o.radius + tolerance() {
                kinds.insert(n, Kind::Counterbore);
            }
        }
    }

    // Faces closing the end of a bore belong to it, so that they are not
    // also reported as unclaimed.
    let mut caps: BTreeMap<usize, usize> = BTreeMap::new();
    for (n, c) in cands.iter().enumerate() {
        if c.outward || !kinds.contains_key(&n) {
            continue;
        }
        for t in [c.extent[0], c.extent[1]] {
            if let Some(cap) = cap_at(solid, c, t) {
                caps.entry(cap).or_insert(n);
            }
        }
    }

    // A cone that closes a bore is the tip the drill left, so it is part
    // of that hole rather than a feature beside it. Without this it
    // would be reported twice: once as the hole's bottom and once as a
    // countersink of no depth, at the far end from any countersink.
    let capping: Vec<usize> = kinds
        .keys()
        .copied()
        .filter(|n| cands[*n].faces.iter().any(|f| caps.contains_key(f)))
        .collect();
    for n in capping {
        kinds.remove(&n);
    }

    // What no bore rule claimed may still be a blend or a chamfer. Both
    // are decided at the join rather than on the face: a blend leaves
    // the surface smooth where two faces would meet at an edge, and a
    // chamfer puts a different edge there instead.
    let taken: BTreeSet<usize> = kinds
        .keys()
        .flat_map(|n| cands[*n].faces.iter().copied())
        .chain(caps.keys().copied())
        .collect();
    for (n, c) in cands.iter().enumerate() {
        if kinds.contains_key(&n) || c.faces.iter().any(|f| taken.contains(f)) {
            continue;
        }
        let (neighbours, tangent) = joins(solid, c);
        if tangent.len() >= 2 {
            // Tangent to both the faces it runs between, so the surface
            // carries on smoothly through it. Which side the material
            // is on is what separates the two words for that.
            kinds.insert(n, if c.outward { Kind::Round } else { Kind::Fillet });
        } else if c.semi_angle.is_some() && neighbours.len() >= 2 {
            kinds.insert(n, Kind::Chamfer);
        }
    }

    // Once each, in a fixed order: a feature is asked for its id again
    // whenever another names it as coaxial, and an id that counted its
    // own collisions would answer differently each time.
    let feature_ids: Vec<String> = cands
        .iter()
        .enumerate()
        .map(|(n, c)| feature_id(solid, c, kinds.get(&n).copied(), ids))
        .collect();
    let id_of = |n: usize| feature_ids[n].clone();

    let mut features: Vec<Feature> = Vec::new();
    for (n, c) in cands.iter().enumerate() {
        let Some(kind) = kinds.get(&n).copied() else {
            continue;
        };
        let blend = matches!(kind, Kind::Fillet | Kind::Round);
        let mut faces: Vec<String> = c.faces.iter().map(|f| face_ids[*f].clone()).collect();
        for (cap, owner) in &caps {
            if *owner == n {
                faces.push(face_ids[*cap].clone());
            }
        }
        faces.sort();
        faces.dedup();
        let lo = cap_at(solid, c, c.extent[0]).is_some();
        let hi = cap_at(solid, c, c.extent[1]).is_some();
        let mut with: Vec<String> = coaxial
            .get(&n)
            .into_iter()
            .flatten()
            .filter(|o| kinds.contains_key(o))
            .map(|o| id_of(*o))
            .collect();
        with.sort();
        with.dedup();
        features.push(Feature {
            id: id_of(n),
            kind,
            faces,
            shape: Shape {
                // A blend is called by its radius and a bore by its
                // diameter, so each states the one it is named by rather
                // than both and a note about which to read.
                diameter: (!blend).then(|| round(c.major_radius * 2.0)),
                radius: blend.then(|| round(c.minor_radius.unwrap_or(c.radius))),
                depth: (!blend).then(|| round(c.extent[1] - c.extent[0])),
                length: blend.then(|| round(c.extent[1] - c.extent[0])),
                // Only a bore is open or closed at its ends. A shaft
                // has no inside, a cone at the mouth of a hole is open
                // at both ends by construction, and a blend runs along
                // an edge rather than into anything, so none of them is
                // asked and none gets a misleading answer.
                through: matches!(kind, Kind::Hole | Kind::Counterbore).then_some(!lo && !hi),
                axis: Some(rounded(c.axis)),
                position: Some(rounded(c.position)),
                extent: Some([round(c.extent[0]), round(c.extent[1])]),
                // Already degrees: the neutral B-rep states them, and the
                // angle a cone is called by is the whole one at its apex.
                angle: c.semi_angle.map(|a| round(a * 2.0)),
            },
            overlaps: Vec::new(),
            coaxial_with: with,
        });
    }
    features.sort_by(|a, b| a.id.cmp(&b.id));

    let claimed: std::collections::BTreeSet<&String> =
        features.iter().flat_map(|f| f.faces.iter()).collect();
    let mut unassigned: Vec<UnassignedFace> = solid
        .faces
        .iter()
        .enumerate()
        .filter(|(i, _)| !claimed.contains(&face_ids[*i]))
        .map(|(i, f)| UnassignedFace {
            id: face_ids[i].clone(),
            surface: f.kind.clone(),
        })
        .collect();
    unassigned.sort_by(|a, b| a.id.cmp(&b.id));

    Body {
        id: body_id(&face_ids, ids),
        name: solid.name.clone(),
        faces: FaceCounts {
            total: solid.faces.len(),
            in_features: claimed.len(),
            unassigned: unassigned.len(),
        },
        envelope: super::envelope::of(solid),
        surfaces: super::form::surfaces(solid),
        shape_class: super::form::class_of(solid),
        patterns: super::patterns::recognise(&features, ids),
        features,
        unassigned,
    }
}

/// Ids for every face, by the shared recipe (ADR 0004).
fn face_ids(solid: &Solid, ids: &mut ContentId) -> Vec<String> {
    solid
        .faces
        .iter()
        .map(|f| {
            let mut verts: Vec<[f64; 3]> = Vec::new();
            for e in f.edges() {
                verts.extend(solid.edges[e].ends.iter().copied());
                if solid.edges[e].ends.is_empty() {
                    verts.extend(solid.edges[e].centre);
                }
            }
            let surface = f
                .surface
                .clone()
                .unwrap_or_else(|| Surface::Other(f.kind.clone()));
            let key = fingerprint::face_key(&surface, &verts, Scale::NONE);
            ids.make("face", &[&key])
        })
        .collect()
}

/// The id of a feature: what kind it is, and the surface it is the wall
/// of, taken as one whether or not the file cut that surface up.
///
/// Deliberately not built from the ids of its faces. A bore written as
/// one cylindrical face and the same bore written as two halves are the
/// same bore, and so are a JT file's and a STEP file's, so the key is
/// the geometry the shared recipe states for it (ADR 0004): the line it
/// turns about, its size, and the stretch of that line it occupies. The
/// faces capping it are left out, so that a blind hole which gains a
/// chamfer is still the same hole.
fn feature_id(solid: &Solid, c: &Candidate, kind: Option<Kind>, ids: &mut ContentId) -> String {
    let points: Vec<[f64; 3]> = c
        .boundary
        .iter()
        .flat_map(|e| {
            let edge = &solid.edges[*e];
            edge.centre.into_iter().chain(edge.ends.iter().copied())
        })
        .collect();
    let geometry = fingerprint::face_key(&c.surface, &points, Scale::NONE);
    let key = fingerprint::feature_key(
        kind.map(Kind::name).unwrap_or_default(),
        std::slice::from_ref(&geometry),
        "",
    );
    ids.make("feat", &[&key])
}

/// The id of a body: the distinct surfaces it is made of.
///
/// Distinct, because a cylinder an exporter cut into halves is still one
/// surface and both halves key the same way, differing only in the
/// ordinal a collision gets. Counting them would make the same design
/// two bodies to anything comparing the two documents, and pairing them
/// is the first thing such a reader has to do.
fn body_id(face_ids: &[String], ids: &mut ContentId) -> String {
    let mut distinct: Vec<&str> = face_ids
        .iter()
        .map(|id| id.split_once('-').map_or(id.as_str(), |(base, _)| base))
        .collect();
    distinct.sort_unstable();
    distinct.dedup();
    let key = distinct.join(";");
    ids.make("body", &[&key])
}

/// Numbers are reported at the identity quantum, so that a value read
/// from two exports of one design reads the same in both.
fn round(v: f64) -> f64 {
    let q = tolerance();
    num(v, q).parse().unwrap_or(v)
}

fn rounded(p: [f64; 3]) -> [f64; 3] {
    [round(p[0]), round(p[1]), round(p[2])]
}
