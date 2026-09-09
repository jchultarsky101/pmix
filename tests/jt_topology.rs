//! Reading the smart topology table of a real JT file (ADR 0010).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use pmix::jt::codec::{Cursor, Predictor};
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

#[test]
fn every_part_states_a_coherent_topology() {
    let tables = tables();
    assert_eq!(tables.len(), 8, "one table per part with precise geometry");
    for t in &tables {
        assert_eq!(t.version, 1);
        let c = t.counts;
        assert_eq!(c.bodies, 1);
        assert!(c.faces > 0 && c.edges > 0, "{c:?}");
        // A shell belongs to a region, and a region to a body.
        assert!(c.regions >= c.bodies && c.shells >= c.regions, "{c:?}");
        // Every edge is used by two coedges in a closed solid, which is
        // the strongest arithmetic check the header offers.
        assert_eq!(c.coedges, c.edges * 2, "{c:?}");
        assert!(c.loops >= c.faces, "a face has at least one loop: {c:?}");
    }
    // The parts range from small fasteners to the moulded housings.
    let faces: Vec<usize> = tables.iter().map(|t| t.counts.faces).collect();
    assert!(faces.iter().min().unwrap() < &40);
    assert!(faces.iter().max().unwrap() > &300);
}

#[test]
fn the_compressed_vectors_after_the_header_are_readable() {
    for t in tables() {
        // Whatever else, the table's own vectors decode until one uses a
        // codec this reader does not implement yet.
        assert!(
            t.vectors.len() >= 8,
            "only {} vectors read: {:?}",
            t.vectors.len(),
            t.stopped
        );
        // The chain begins with the body, region, and shell vectors,
        // whose lengths the header predicts.
        assert_eq!(t.vectors[0], t.counts.bodies);
        assert_eq!(t.vectors[1], t.counts.bodies);
        assert_eq!(t.vectors[2], t.counts.regions);
        assert_eq!(t.vectors[3], t.counts.regions);
        assert_eq!(t.vectors[5], t.counts.shells);
        assert_eq!(t.vectors[6], t.counts.shells);
        assert_eq!(t.vectors[7], t.counts.shells);
        // Start Face Index has one entry per represented shell, not per
        // shell: an inner shell shares its faces with its outer one.
        assert!(t.vectors[4] <= t.counts.shells);
    }
}

/// The check that catches a wrong bitlength decode. A face identifier is
/// unique within its body, so a decode that drops or repeats a bit shows
/// up here as a repeated identifier. Getting the field-width constant
/// wrong does exactly that on the one part whose identifiers use the
/// adaptive path.
#[test]
fn every_face_identifier_is_distinct() {
    let tables = tables();
    let with_faces: Vec<_> = tables.iter().filter(|t| !t.faces.is_empty()).collect();
    assert_eq!(with_faces.len(), 8, "every part gives up its faces");
    for t in with_faces {
        assert_eq!(t.faces.len(), t.counts.faces);
        let ids: Vec<u32> = t.faces.iter().map(|f| f.identifier).collect();
        let distinct: BTreeSet<u32> = ids.iter().copied().collect();
        assert_eq!(distinct.len(), ids.len(), "repeated identifier");
        // Identifiers number the faces of the body, but not in the
        // order the table stores them: they are a permutation, which is
        // why a face cannot be found by its identifier alone.
        assert_eq!(
            distinct,
            (0..t.counts.faces as u32).collect::<BTreeSet<u32>>(),
            "identifiers number the faces of the body"
        );
        assert!(
            !ids.windows(2).all(|w| w[1] > w[0]),
            "at least one part stores its faces out of identifier order"
        );
    }
}

/// The check that would catch a wrong predictor. Walking the chain from
/// the faces has to reach every loop once, every coedge once, and every
/// edge twice; a start index that is off by anything at all breaks one
/// of those. This is what the specification's four primer values fix.
#[test]
fn walking_from_the_faces_reaches_the_whole_topology() {
    let tables = tables();
    assert_eq!(tables.len(), 8);
    for t in &tables {
        let c = t.counts;
        assert_eq!(t.loops.len(), c.loops);
        assert_eq!(t.coedges.len(), c.coedges);
        assert_eq!(t.edges.len(), c.edges);
        let mut loops = vec![0u32; c.loops];
        let mut coedges = vec![0u32; c.coedges];
        let mut edges = vec![0u32; c.edges];
        for face in &t.faces {
            for l in face.loops.clone() {
                loops[l] += 1;
                for ce in t.loops[l].coedges.clone() {
                    coedges[ce] += 1;
                    edges[t.coedges[ce].edge] += 1;
                }
            }
        }
        assert!(loops.iter().all(|n| *n == 1), "every loop bounds one face");
        assert!(
            coedges.iter().all(|n| *n == 1),
            "every coedge belongs to one loop"
        );
        assert!(
            edges.iter().all(|n| *n == 2),
            "every edge separates two faces"
        );
        // Every vertex an edge names is one the geometry counts.
        let points = t.geometry.unwrap().points as u32;
        assert!(
            t.edges
                .iter()
                .all(|e| e.start_vertex < points && e.end_vertex < points)
        );
    }
}

/// Which face a surface belongs to, and which edge a curve belongs to.
///
/// The index a described surface carries is the position of its face.
/// The proof is geometric rather than structural: a curve bounding a
/// face lies on that face's surface, so a wrong mapping puts points off
/// the surface by millimetres. Nothing here may be relaxed to a
/// tolerance that would let a wrong mapping through.
#[test]
fn every_curve_lies_on_the_surface_of_the_face_it_bounds() {
    use pmix::jt::stt::{Curve, Surface};
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let sub = |a: [f64; 3], b: [f64; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let cross = |a: [f64; 3], b: [f64; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let mut checked = 0usize;
    for t in tables() {
        for face in &t.faces {
            let Some(surface) = face.surface else {
                continue;
            };
            for l in face.loops.clone() {
                for ce in t.loops[l].coedges.clone() {
                    let Some(curve) = t.edges[t.coedges[ce].edge].curve else {
                        continue;
                    };
                    let at = curve.location();
                    // Only points the surface has to contain: a circle
                    // on a cylinder is centred on the axis, not on the
                    // surface, so it is measured against the axis.
                    let off = match (surface, curve) {
                        // Every bounding curve of a plane lies in it.
                        (Surface::Plane { location, axis }, _) => dot(sub(at, location), axis),
                        (
                            Surface::Cylinder {
                                location,
                                axis,
                                radius,
                            },
                            Curve::Line { .. },
                        ) => {
                            let d = cross(sub(at, location), axis);
                            dot(d, d).sqrt() - radius
                        }
                        (
                            Surface::Cylinder { location, axis, .. },
                            Curve::Circle { .. } | Curve::Ellipse { .. },
                        ) => {
                            let d = cross(sub(at, location), axis);
                            dot(d, d).sqrt()
                        }
                        _ => continue,
                    };
                    checked += 1;
                    assert!(
                        off.abs() < 1e-9,
                        "a {} bounding a {} face misses it by {off} metres",
                        curve.kind(),
                        surface.kind()
                    );
                }
            }
        }
    }
    assert!(checked > 2000, "only {checked} curves checked");
}

/// The two vectors the specification's figures do not show. Both name
/// the kind of geometry the entity lies on, for every entity rather
/// than only for the ones described afterwards, which is why they
/// account for exactly the entities left undescribed.
#[test]
fn every_face_and_edge_names_what_it_lies_on() {
    use pmix::jt::stt::{CurveKind, SurfaceKind};
    for t in tables() {
        let g = t.geometry.unwrap();
        let analytic = t
            .faces
            .iter()
            .filter(|f| !matches!(f.surface_kind, SurfaceKind::Other(_)))
            .count();
        assert_eq!(
            analytic, g.represented_surfaces,
            "the faces on an analytic surface are exactly the ones described"
        );
        let analytic = t
            .edges
            .iter()
            .filter(|e| !matches!(e.curve_kind, CurveKind::Other(_)))
            .count();
        assert_eq!(analytic, g.represented_curves);
        // The kind a face states and the kind its surface turns out to
        // be are two different fields, written to two different
        // enumerations, and they have to agree.
        for face in &t.faces {
            if let Some(surface) = face.surface {
                assert_eq!(face.surface_kind.name(), surface.kind());
            } else {
                assert!(matches!(face.surface_kind, SurfaceKind::Other(_)));
            }
        }
        for edge in &t.edges {
            if let Some(curve) = edge.curve {
                assert_eq!(edge.curve_kind.name(), curve.kind());
            } else {
                assert!(matches!(edge.curve_kind, CurveKind::Other(_)));
            }
        }
    }
}

#[test]
fn faces_point_out_of_their_shell_and_some_reverse_their_surface() {
    for t in tables().iter().filter(|t| !t.faces.is_empty()) {
        assert!(
            t.faces.iter().all(|f| !f.inward),
            "every part is a solid whose faces point outward"
        );
    }
    // The other flag is what distinguishes the two, and it varies.
    let reversed: usize = tables()
        .iter()
        .map(|t| t.faces.iter().filter(|f| f.normal_reversed).count())
        .sum();
    assert!(reversed > 0);
}

/// The whole topology now decodes, and the counts that follow it prove
/// the chain ended where it should: a B-rep has one surface per face and
/// one curve per edge, so reading the wrong number of vectors would put
/// arbitrary bytes here instead.
#[test]
fn the_geometry_after_the_topology_matches_it() {
    let tables = tables();
    assert_eq!(tables.len(), 8);
    for t in &tables {
        assert!(t.stopped.is_none(), "{:?}", t.stopped);
        assert_eq!(t.vectors.len(), 23, "the topology is a fixed chain");
        let g = t.geometry.expect("the geometry counts follow the topology");
        assert_eq!(g.surfaces, t.counts.faces, "one surface per face");
        assert_eq!(g.curves, t.counts.edges, "one curve per edge");
        // Not every surface is written out; the rest are implied.
        assert!(g.represented_surfaces <= g.surfaces);
        assert!(g.represented_curves <= g.curves);
        assert!(g.points > 0);
        assert!(t.hash.is_some());
    }
}

#[test]
fn every_described_surface_is_recovered() {
    use pmix::jt::stt::Surface;
    let mut kinds: BTreeSet<&str> = BTreeSet::new();
    for t in tables() {
        let g = t.geometry.unwrap();
        // The four arrays a surface draws from are laid end to end, so
        // reading one surface wrongly would derail every one after it.
        // Ending on exactly the count the geometry declares is what says
        // the reading is right.
        let surfaces: Vec<Surface> = t.faces.iter().filter_map(|f| f.surface).collect();
        assert_eq!(
            surfaces.len(),
            g.represented_surfaces,
            "every described surface recovered"
        );
        for surface in &surfaces {
            kinds.insert(surface.kind());
            // An axis is a direction, so it has unit length.
            let axis = surface.axis();
            let length = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
            assert!(
                (length - 1.0).abs() < 1e-6,
                "{surface:?} has an axis of length {length}"
            );
            // A radius is a size, so it is positive and not absurd. The
            // file states lengths in metres.
            match surface {
                Surface::Cylinder { radius, .. }
                | Surface::Sphere { radius, .. }
                | Surface::Cone { radius, .. } => {
                    assert!(*radius > 0.0 && *radius < 10.0, "{surface:?}")
                }
                Surface::Torus {
                    major_radius,
                    minor_radius,
                    ..
                } => {
                    assert!(*major_radius > 0.0 && *minor_radius > 0.0, "{surface:?}");
                }
                Surface::Plane { .. } => {}
            }
            if let Surface::Cone { semi_angle, .. } = surface {
                // Half the angle at the apex, so it is a quarter turn at
                // most.
                assert!(
                    *semi_angle > 0.0 && *semi_angle < std::f64::consts::FRAC_PI_2,
                    "{surface:?}"
                );
            }
        }
    }
    // The assembly uses every kind the table can describe.
    assert_eq!(
        kinds,
        BTreeSet::from(["cone", "cylinder", "plane", "sphere", "torus"])
    );
}

#[test]
fn a_chamfer_is_recovered_as_a_cone_at_forty_five_degrees() {
    use pmix::jt::stt::Surface;
    // A machined chamfer is cut at 45 degrees, so its cone's half angle
    // is an eighth of a turn. Finding that exactly is a strong sign the
    // angles are read from the right place.
    let eighth = std::f64::consts::FRAC_PI_4;
    let found = tables().iter().any(|t| {
        t.faces.iter().any(|f| {
            matches!(f.surface, Some(Surface::Cone { semi_angle, .. }) if (semi_angle - eighth).abs() < 1e-9)
        })
    });
    assert!(found, "no chamfer cut at 45 degrees");
}

#[test]
fn the_codec_refuses_a_packet_it_cannot_read() {
    // A packet claiming more values than the bytes could hold must fail
    // rather than allocate.
    let mut bytes = i32::MAX.to_le_bytes().to_vec();
    bytes.push(1);
    let mut c = Cursor::new(&bytes);
    assert!(c.packet(Predictor::None).is_err());
}
