//! A boundary representation both readers can produce.
//!
//! Recognition is written once, over this, rather than twice over the
//! two readers' own topology. What it needs is small: what each face
//! lies on, which way the face looks, what bounds it, and which faces
//! meet across each edge.
//!
//! The numbers here are already in millimetres and degrees. Each adapter
//! applies its file's declared units on the way in (ADR 0004), so a
//! design exported in inches and in millimetres yields one B-rep.

use std::collections::BTreeMap;

use crate::fingerprint::Surface;

/// The kind of curve an edge lies on, as far as recognition cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CurveKind {
    Line,
    Circle,
    Ellipse,
    /// Anything with no closed form in the file, or a kind not named
    /// here. Never treated as one of the above.
    Other,
}

/// One edge, and where it runs.
#[derive(Debug, Clone, PartialEq)]
pub struct Edge {
    pub curve: CurveKind,
    /// The circle's centre and axis, when the edge is a full circle or
    /// an arc of one. Recognition uses these to tell a hole's ends
    /// apart and to decide that two circles share an axis.
    pub centre: Option<[f64; 3]>,
    pub axis: Option<[f64; 3]>,
    pub radius: Option<f64>,
    /// The points the edge runs between, when the file locates them.
    pub ends: Vec<[f64; 3]>,
}

/// One closed circuit of edges bounding a face.
///
/// Whether a loop is the face's outer boundary or a hole cut in it is
/// what separates the far end of a through hole from the floor of a
/// counterbore: at a through hole's mouth the circle is a hole in the
/// face outside it, and at a counterbore's floor the same-shaped circle
/// is that floor's outer boundary. Both formats state the difference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loop {
    /// The loop's edges, as positions in [`Solid::edges`].
    pub edges: Vec<usize>,
    /// Whether this is the face's outer boundary rather than a hole in
    /// it. A file that does not say is taken to mean the outer one, as
    /// the first loop of a face conventionally is.
    pub outer: bool,
}

/// One face of a solid.
#[derive(Debug, Clone, PartialEq)]
pub struct Face {
    /// What the face lies on. `None` when the file describes the surface
    /// with no closed form, which is a fact recognition must state
    /// rather than skip past.
    pub surface: Option<Surface>,
    /// The name the reader's own model uses for the kind of surface,
    /// even when `surface` is `None`.
    pub kind: String,
    /// Whether the face looks the way its surface does.
    ///
    /// A surface of revolution's own normal points away from its axis,
    /// and a solid's faces point away from its material. So a cylinder
    /// with `same_sense` is a shaft or a boss, and one without it is the
    /// wall of a hole. This one flag is what separates the two, and both
    /// readers state it, by different names.
    pub same_sense: bool,
    /// The face's loops, outer boundary and holes alike.
    pub loops: Vec<Loop>,
}

impl Face {
    /// Every edge bounding the face, whichever loop it is in.
    pub fn edges(&self) -> impl Iterator<Item = usize> + '_ {
        self.loops.iter().flat_map(|l| l.edges.iter().copied())
    }

    /// Whether `edge` bounds this face at all.
    pub fn is_bounded_by(&self, edge: usize) -> bool {
        self.loops.iter().any(|l| l.edges.contains(&edge))
    }

    /// Whether `edge` is part of this face's outer boundary.
    pub fn is_outer(&self, edge: usize) -> bool {
        self.loops
            .iter()
            .any(|l| l.outer && l.edges.contains(&edge))
    }
}

/// A solid: its faces, its edges, and which faces meet across each edge.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Solid {
    /// A name for the body, when the file gives one. Not part of any id.
    pub name: Option<String>,
    pub faces: Vec<Face>,
    pub edges: Vec<Edge>,
    /// Which faces each edge separates, as positions in [`Solid::faces`].
    ///
    /// A well-formed solid has exactly two per edge. Files are not always
    /// well formed, so this says what is there rather than assuming.
    pub across: BTreeMap<usize, Vec<usize>>,
}

impl Solid {
    /// Fill [`Solid::across`] from the faces' edges. Adapters build the
    /// faces and edges and call this once.
    pub fn link(&mut self) {
        self.across.clear();
        for (i, f) in self.faces.iter().enumerate() {
            for e in f.edges() {
                let at = self.across.entry(e).or_default();
                if !at.contains(&i) {
                    at.push(i);
                }
            }
        }
    }

    /// The faces meeting `face` across its edges, each once, in order.
    pub fn neighbours(&self, face: usize) -> Vec<usize> {
        let mut out = Vec::new();
        for e in self.faces[face].edges() {
            for other in self.across.get(&e).into_iter().flatten() {
                if *other != face && !out.contains(other) {
                    out.push(*other);
                }
            }
        }
        out
    }

    /// Whether an edge is where `face` closes on itself.
    ///
    /// A full cylinder is a rectangle rolled up, and the join is an edge
    /// the face has on both sides, so it appears twice among the face's
    /// own loop edges. Exporters differ on whether they write one: JT's
    /// topology table has none and STEP files generally do. It bounds
    /// nothing, and a rule that asks what shape bounds a face has to
    /// pass over it or no bore in any STEP file will ever match.
    pub fn is_seam(&self, face: usize, edge: usize) -> bool {
        self.faces[face].edges().filter(|e| *e == edge).count() > 1
    }

    /// Whether every edge bounding `face`, seams aside, is a circle.
    pub fn bounded_by_circles(&self, face: usize) -> bool {
        let mut any = false;
        for e in self.faces[face].edges() {
            if self.is_seam(face, e) {
                continue;
            }
            any = true;
            if self.edges[e].curve != CurveKind::Circle {
                return false;
            }
        }
        any
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edge(curve: CurveKind) -> Edge {
        Edge {
            curve,
            centre: None,
            axis: None,
            radius: None,
            ends: Vec::new(),
        }
    }

    fn face(edges: Vec<usize>) -> Face {
        Face {
            surface: None,
            kind: "plane".into(),
            same_sense: true,
            loops: vec![Loop { edges, outer: true }],
        }
    }

    #[test]
    fn an_edge_links_the_faces_that_share_it() {
        let mut s = Solid {
            faces: vec![face(vec![0, 1]), face(vec![1, 2])],
            edges: vec![
                edge(CurveKind::Line),
                edge(CurveKind::Circle),
                edge(CurveKind::Line),
            ],
            ..Solid::default()
        };
        s.link();
        assert_eq!(s.across[&1], vec![0, 1]);
        assert_eq!(s.across[&0], vec![0]);
        assert_eq!(s.neighbours(0), vec![1]);
        assert_eq!(s.neighbours(1), vec![0]);
    }

    #[test]
    fn a_face_is_bounded_by_circles_only_when_every_edge_is_one() {
        let mut s = Solid {
            faces: vec![face(vec![1]), face(vec![0, 1]), face(vec![])],
            edges: vec![edge(CurveKind::Line), edge(CurveKind::Circle)],
            ..Solid::default()
        };
        s.link();
        assert!(s.bounded_by_circles(0));
        assert!(!s.bounded_by_circles(1));
        // A face with no edges is not "bounded only by circles"; saying
        // so would make every unbounded surface a hole candidate.
        assert!(!s.bounded_by_circles(2));
    }

    /// A cylinder as a STEP file states one: two circles and the seam
    /// line where it closes on itself, which the face has on both sides.
    #[test]
    fn the_seam_of_a_closed_face_does_not_count_against_it() {
        let mut s = Solid {
            faces: vec![face(vec![0, 2, 1, 2])],
            edges: vec![
                edge(CurveKind::Circle),
                edge(CurveKind::Circle),
                edge(CurveKind::Line),
            ],
            ..Solid::default()
        };
        s.link();
        assert!(s.is_seam(0, 2));
        assert!(!s.is_seam(0, 0));
        assert!(s.bounded_by_circles(0));
    }
}
