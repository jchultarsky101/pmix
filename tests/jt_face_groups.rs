//! Resolving a PMI callout to the B-rep faces it applies to (ADR 0010).
//!
//! An association names a face by a tag the originating system assigned.
//! The topology table's attribute section carries one tag per face, and
//! the two meet here. The ordering is the part worth testing, because a
//! wrong one resolves to a real face of the same part and so looks
//! entirely plausible.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use pmix::jt::{Elements, SegmentKind, file::Jt, pmi, stt};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/jt/nist_mtc_assembly.jt")
}

fn tables(jt: &Jt<'_>) -> Vec<stt::Topology> {
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

fn managers(jt: &Jt<'_>) -> Vec<pmi::PmiManager> {
    let mut out = Vec::new();
    for segment in jt
        .segments()
        .iter()
        .filter(|s| s.kind == SegmentKind::PmiData)
    {
        let data = jt.segment_data(segment).unwrap();
        for element in Elements::new(&data) {
            if element.object_type == pmi::PMI_MANAGER {
                if let Ok(m) = pmi::parse(element.data) {
                    out.push(m);
                }
            }
        }
    }
    out
}

/// Every face of every part carries a tag, and the tags of a part are
/// distinct, because a tag is what names a face uniquely.
#[test]
fn every_face_carries_a_distinct_tag() {
    let bytes = std::fs::read(fixture()).unwrap();
    let jt = Jt::parse(&bytes).unwrap();
    let tables = tables(&jt);
    assert_eq!(tables.len(), 8);
    for t in &tables {
        let tags: Vec<u32> = t.faces.iter().filter_map(|f| f.tag).collect();
        assert_eq!(tags.len(), t.counts.faces, "a tag for every face");
        let distinct: BTreeSet<u32> = tags.iter().copied().collect();
        assert_eq!(distinct.len(), tags.len(), "tags name faces uniquely");
        // A tag is not a position: it is the identifier the originating
        // system assigned, and it outruns the face count.
        assert!(*distinct.iter().next_back().unwrap() as usize > t.counts.faces);
    }
}

/// Every face a PMI association names is a face some part really has.
///
/// This is what says the tags were read from the right place. Nothing
/// arranges for a number decoded out of the wrong bytes to appear in
/// another segment's tag list.
#[test]
fn every_face_a_callout_names_is_a_face_of_some_part() {
    let bytes = std::fs::read(fixture()).unwrap();
    let jt = Jt::parse(&bytes).unwrap();
    let tables = tables(&jt);
    let known: BTreeSet<u32> = tables
        .iter()
        .flat_map(|t| t.faces.iter().filter_map(|f| f.tag))
        .collect();

    let mut named = BTreeSet::new();
    for m in managers(&jt) {
        for a in &m.associations {
            for end in [a.source, a.destination] {
                if end.kind == pmi::EndPoint::FACE {
                    if let Some(tag) = m.tag_of(end) {
                        named.insert(u32::try_from(tag).unwrap());
                    }
                }
            }
        }
    }
    assert!(named.len() > 100, "only {} faces named", named.len());
    let strays: Vec<&u32> = named.difference(&known).collect();
    assert!(strays.is_empty(), "faces named but not found: {strays:?}");
}

/// The ordering, tested against where the leader arrows point.
///
/// A face group numbers a part's faces, and the table stores its faces
/// in that order, so the nth tag belongs to the nth face. The competing
/// reading orders the faces by the identifier the table gives them,
/// which is a permutation of their positions, so the two disagree about
/// most faces and both resolve to a real face either way.
///
/// What tells them apart is geometry. A callout's leader ends on the
/// thing it points at, in millimetres, so under the right reading it
/// lands on a face the callout names: on its surface, or on the axis of
/// a cylinder, which is where a leader to a hole is drawn to.
#[test]
fn a_callout_resolves_to_the_faces_its_leaders_point_at() {
    let bytes = std::fs::read(fixture()).unwrap();
    let jt = Jt::parse(&bytes).unwrap();
    let tables = tables(&jt);

    // [by position, by identifier] -> (leaders that land, leaders that do not)
    let mut score = [(0usize, 0usize); 2];
    for m in managers(&jt) {
        // The part a segment's callouts are about is the one whose tags
        // cover them. An assembly-level callout naming another part's
        // faces matches nothing and is left alone.
        let named: BTreeSet<u32> = m
            .associations
            .iter()
            .flat_map(|a| [a.source, a.destination])
            .filter(|e| e.kind == pmi::EndPoint::FACE)
            .filter_map(|e| m.tag_of(e))
            .filter_map(|t| u32::try_from(t).ok())
            .collect();
        if named.is_empty() {
            continue;
        }
        let Some(t) = tables.iter().find(|t| {
            let have: BTreeSet<u32> = t.faces.iter().filter_map(|f| f.tag).collect();
            named.is_subset(&have)
        }) else {
            continue;
        };
        // The competing reading: the faces in order of their identifier.
        let mut by_identifier: Vec<usize> = (0..t.faces.len()).collect();
        by_identifier.sort_by_key(|k| t.faces[*k].identifier);

        for (at, entity) in m.entities.iter().enumerate() {
            let faces = m.faces_of(at);
            if faces.is_empty() {
                continue;
            }
            let groups: Vec<usize> = faces
                .iter()
                .filter_map(|tag| t.faces.iter().position(|f| f.tag == Some(*tag)))
                // A face the permutation leaves where it is tells the
                // two readings apart not at all.
                .filter(|g| by_identifier[*g] != *g)
                .collect();
            let leaders: Vec<[f64; 3]> = entity
                .properties
                .iter()
                .filter(|(k, _)| k.ends_with(".terminator"))
                .filter_map(|(_, v)| {
                    let n: Vec<f64> = v
                        .split_whitespace()
                        .filter_map(|x| x.parse().ok())
                        .collect();
                    // The file states these in millimetres; the topology
                    // table states its geometry in metres.
                    (n.len() == 3).then(|| [n[0] / 1000.0, n[1] / 1000.0, n[2] / 1000.0])
                })
                .collect();
            for point in &leaders {
                for (which, order) in [(0, None), (1, Some(&by_identifier))] {
                    let nearest = groups
                        .iter()
                        .map(|g| order.map_or(*g, |o| o[*g]))
                        .filter_map(|k| t.faces.get(k))
                        .filter_map(|f| f.surface)
                        .map(|s| how_far(&s, *point))
                        .fold(f64::INFINITY, f64::min);
                    if nearest.is_infinite() {
                        continue;
                    }
                    if nearest < 1e-6 {
                        score[which].0 += 1;
                    } else {
                        score[which].1 += 1;
                    }
                }
            }
        }
    }

    let (position, identifier) = (score[0], score[1]);
    assert!(
        position.0 + position.1 > 100,
        "too few leaders to conclude anything: {score:?}"
    );
    // Not a close call, and it must not be allowed to become one.
    assert!(
        position.0 > 4 * identifier.0,
        "the faces are stored in face group order: by position {position:?}, \
         by identifier {identifier:?}"
    );
    assert!(
        position.0 * 2 > position.1,
        "most leaders should land on a face their callout names: {position:?}"
    );
}

/// How far a point is from the surface, or from the axis it turns about.
///
/// A leader to a hole is drawn to the hole's axis rather than to its
/// wall, so both count as pointing at the face.
fn how_far(surface: &stt::Surface, p: [f64; 3]) -> f64 {
    let sub = |a: [f64; 3], b: [f64; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let cross = |a: [f64; 3], b: [f64; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let norm = |a: [f64; 3]| dot(a, a).sqrt();
    match surface {
        stt::Surface::Plane { location, axis } => dot(sub(p, *location), *axis).abs(),
        stt::Surface::Cylinder {
            location,
            axis,
            radius,
        } => {
            let out = norm(cross(sub(p, *location), *axis));
            (out - radius).abs().min(out)
        }
        stt::Surface::Sphere {
            location, radius, ..
        } => (norm(sub(p, *location)) - radius).abs(),
        stt::Surface::Cone {
            location,
            axis,
            radius,
            semi_angle,
        } => {
            let along = dot(sub(p, *location), *axis);
            let out = norm(cross(sub(p, *location), *axis));
            (out - (radius + along * semi_angle.tan())).abs().min(out)
        }
        stt::Surface::Torus {
            location,
            axis,
            major_radius,
            minor_radius,
        } => {
            let d = sub(p, *location);
            let along = dot(d, *axis);
            let out = norm(cross(d, *axis)) - major_radius;
            ((out * out + along * along).sqrt() - minor_radius).abs()
        }
    }
}

/// The whole chain, end to end: a callout names faces, and those faces
/// are the ones a machinist would expect a hole callout to be about.
#[test]
fn a_hole_callout_resolves_to_a_cylinder() {
    let bytes = std::fs::read(fixture()).unwrap();
    let jt = Jt::parse(&bytes).unwrap();
    let tables = tables(&jt);
    let mut checked = 0;
    for m in managers(&jt) {
        let named: BTreeSet<u32> = m
            .associations
            .iter()
            .flat_map(|a| [a.source, a.destination])
            .filter(|e| e.kind == pmi::EndPoint::FACE)
            .filter_map(|e| m.tag_of(e))
            .filter_map(|t| u32::try_from(t).ok())
            .collect();
        if named.is_empty() {
            continue;
        }
        let Some(t) = tables.iter().find(|t| {
            let have: BTreeSet<u32> = t.faces.iter().filter_map(|f| f.tag).collect();
            named.is_subset(&have)
        }) else {
            continue;
        };
        for (at, entity) in m.entities.iter().enumerate() {
            // A callout that says THRU is about a hole, so it has to
            // reach a cylinder.
            if !entity
                .properties
                .iter()
                .any(|(k, v)| k.ends_with(".string") && v.trim() == "THRU")
            {
                continue;
            }
            let kinds: BTreeSet<&str> = m
                .faces_of(at)
                .iter()
                .filter_map(|tag| t.face_with_tag(*tag))
                .map(|f| f.surface_kind.name())
                .collect();
            if kinds.is_empty() {
                continue;
            }
            assert!(
                kinds.contains("cylinder"),
                "a through hole callout reached {kinds:?} and no cylinder"
            );
            checked += 1;
        }
    }
    assert!(checked > 0, "no through hole callout found");
}
