//! Fingerprinting JT B-rep faces (ADR 0004, ADR 0010).
//!
//! The vertices a fingerprint spans are not read from the point
//! geometry, which is quantised, but recovered by evaluating each edge's
//! curve over the stretch of it the edge covers. That is the part worth
//! testing: everything downstream is the shared recipe, which has its
//! own tests.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use pmix::jt::{Elements, SegmentKind, file::Jt, stt};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/jt/nist_mtc_assembly.jt")
}

fn tables() -> Vec<stt::Topology> {
    let bytes = std::fs::read(fixture()).unwrap();
    let jt = Jt::parse(&bytes).unwrap();
    let mut out = Vec::new();
    for segment in jt.segments().iter().filter(|s| s.kind == SegmentKind::Stt) {
        let data = jt.segment_data(segment).unwrap();
        for element in Elements::new(&data) {
            if element.object_type == stt::STT_ELEMENT {
                out.push(stt::parse(element.data).unwrap());
            }
        }
    }
    out
}

/// What says the curves are evaluated correctly: several edges meet at
/// each vertex, they are evaluated independently, and they have to agree
/// on where it is.
///
/// This checks the parameterisation, the direction an angle is measured
/// from, and the rule that an edge running against its curve starts at
/// the far end of the domain. Get any of them wrong and the corners of a
/// face land somewhere else.
#[test]
fn every_edge_meeting_at_a_vertex_agrees_on_where_it_is() {
    let mut agreed = 0usize;
    let mut worst = 0.0f64;
    for t in tables() {
        let points = t.geometry.unwrap().points;
        let mut at: Vec<Option<[f64; 3]>> = vec![None; points];
        for edge in &t.edges {
            let (Some(curve), Some(domain)) = (edge.curve, edge.domain) else {
                continue;
            };
            let (from, to) = (curve.at(domain[0]), curve.at(domain[1]));
            let (start, end) = if edge.forward { (from, to) } else { (to, from) };
            for (v, p) in [(edge.start_vertex, start), (edge.end_vertex, end)] {
                let Some(slot) = at.get_mut(v as usize) else {
                    continue;
                };
                match slot {
                    None => *slot = Some(p),
                    Some(q) => {
                        let d =
                            ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2))
                                .sqrt();
                        worst = worst.max(d);
                        agreed += 1;
                    }
                }
            }
        }
    }
    assert!(agreed > 1500, "only {agreed} vertices were reached twice");
    // A micron, on a table stated in metres. The identity quantum is a
    // thousandth of a model unit, so anything at this scale rounds the
    // same way; a wrong rule misses by millimetres, not microns.
    assert!(
        worst < 1e-6,
        "edges disagree about a vertex by {} mm",
        worst * 1000.0
    );
}

/// The vertices a face's fingerprint spans are the ones its own edges
/// reach, and every face in the test file reaches all of them.
#[test]
fn every_face_knows_where_its_corners_are() {
    let tables = tables();
    assert_eq!(tables.len(), 8);
    let (mut faces, mut complete) = (0usize, 0usize);
    for t in &tables {
        for f in &t.faces {
            faces += 1;
            let boundary = t.boundary(f);
            if boundary.complete {
                complete += 1;
                assert!(
                    !boundary.vertices.is_empty(),
                    "a bounded face has corners: {f:?}"
                );
            }
        }
    }
    assert_eq!(faces, 939);
    assert_eq!(complete, faces, "every face's boundary is accounted for");
}

/// A fingerprint has to tell one face from another, or a callout on one
/// would anchor on the other.
#[test]
fn a_fingerprint_tells_the_faces_of_a_part_apart() {
    use std::collections::BTreeMap;
    let (mut faces, mut distinct) = (0usize, 0usize);
    for t in tables() {
        let mut by_key: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (i, f) in t.faces.iter().enumerate() {
            let Some(key) = t.fingerprint(f) else {
                panic!("a face of a fully read part has no fingerprint")
            };
            by_key.entry(key).or_default().push(i);
        }
        faces += t.faces.len();
        distinct += by_key.len();
        // Where two faces do share a key, they share a surface of
        // revolution and differ only in the sector of it they cover:
        // six facets of a hex socket on one cone. The recipe measures
        // such a face along the axis rather than by the box its corners
        // fill, so that a face re-cut at a new seam keeps its key, and
        // that is deliberate (ADR 0004). What must not happen is two
        // unrelated faces colliding.
        for (key, faces) in by_key.iter().filter(|(_, v)| v.len() > 1) {
            let kinds: BTreeSet<&str> = faces
                .iter()
                .map(|i| t.faces[*i].surface_kind.name())
                .collect();
            assert_eq!(kinds.len(), 1, "unrelated faces share {key}");
            let kind = *kinds.iter().next().unwrap();
            assert!(
                matches!(kind, "cylinder" | "cone" | "torus"),
                "{kind} faces share {key}, and only a surface of \
                 revolution is measured in a way that allows it"
            );
        }
    }
    assert_eq!(faces, 939);
    assert!(
        distinct * 100 >= faces * 98,
        "only {distinct} of {faces} faces are told apart"
    );
}

/// Every edge is fingerprinted, and an edge is the same edge whichever
/// way round the file states it.
#[test]
fn every_edge_is_fingerprinted_by_its_curve_and_its_ends() {
    let (mut edges, mut distinct) = (0usize, 0usize);
    for t in tables() {
        let keys: Vec<String> = t
            .edges
            .iter()
            .filter_map(|e| t.edge_fingerprint(e))
            .collect();
        assert_eq!(keys.len(), t.counts.edges, "every edge is fingerprinted");
        assert!(keys.iter().all(|k| k.starts_with("edge/")), "{keys:?}");
        edges += keys.len();
        distinct += keys.iter().collect::<BTreeSet<&String>>().len();
    }
    assert!(edges > 2000, "only {edges} edges checked");
    // Nearly all are told apart. Two arcs between the same pair of
    // points share a key, which this recipe cannot separate and the
    // STEP reader cannot either, so a handful is expected.
    assert!(
        distinct * 100 >= edges * 99,
        "only {distinct} of {edges} edges are told apart"
    );
}

/// A fingerprint is stated in millimetres, whatever unit the file used.
/// The topology table is always in metres, so the reader converts.
#[test]
fn a_fingerprint_is_in_millimetres() {
    let t = tables().into_iter().next().unwrap();
    let face = t
        .faces
        .iter()
        .find(|f| matches!(f.surface, Some(stt::Surface::Cylinder { .. })))
        .expect("a cylindrical face");
    let Some(stt::Surface::Cylinder { radius, .. }) = face.surface else {
        unreachable!()
    };
    let key = t.fingerprint(face).unwrap();
    // The table states the radius in metres; the key states it in
    // millimetres, so it is a thousand times larger.
    let stated = key.split("/t").next().unwrap().rsplit('/').next().unwrap();
    assert_eq!(
        stated,
        pmix::identity::num(radius * 1000.0, pmix::identity::QUANTUM)
    );
    // And reading it twice says the same thing.
    assert_eq!(key, t.fingerprint(face).unwrap());
}

/// A cylindrical face's fingerprint states the radius the file states,
/// which is the plainest check that the numbers reach the key intact.
#[test]
fn a_cylinder_fingerprints_with_the_radius_it_has() {
    let mut checked = 0;
    for t in tables() {
        for face in &t.faces {
            let Some(stt::Surface::Cylinder { radius, .. }) = face.surface else {
                continue;
            };
            let key = t.fingerprint(face).unwrap();
            assert!(key.starts_with("cylinder/"), "{key}");
            // The radius is the last field before the axial span, in
            // millimetres where the table states metres.
            let stated = key.split("/t").next().unwrap().rsplit('/').next().unwrap();
            let want = pmix::identity::num(radius * 1000.0, pmix::identity::QUANTUM);
            assert_eq!(stated, want, "in {key}");
            checked += 1;
        }
    }
    assert!(checked > 100, "only {checked} cylinders checked");
}
