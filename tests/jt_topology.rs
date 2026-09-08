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
    }
}

/// The check that catches a wrong bitlength decode. A face identifier is
/// unique within its body, so a decode that drops or repeats a bit shows
/// up here as a repeated identifier. Getting the field-width constant
/// wrong does exactly that on the one part whose identifiers use the
/// adaptive path.
#[test]
fn every_face_identifier_is_distinct_and_ascending() {
    let tables = tables();
    let with_faces: Vec<_> = tables.iter().filter(|t| !t.faces.is_empty()).collect();
    assert_eq!(with_faces.len(), 8, "every part gives up its faces");
    for t in with_faces {
        assert_eq!(t.faces.len(), t.counts.faces);
        let ids: Vec<u32> = t.faces.iter().map(|f| f.identifier).collect();
        let distinct: BTreeSet<u32> = ids.iter().copied().collect();
        assert_eq!(distinct.len(), ids.len(), "repeated identifier in {ids:?}");
        assert!(
            ids.windows(2).all(|w| w[1] > w[0]),
            "identifiers are stored ascending: {ids:?}"
        );
        assert_eq!(ids[0], 0, "the first identifier is zero");
        // An identifier is not a position, so it outruns the face count.
        assert!(
            *ids.last().unwrap() as usize >= t.counts.faces,
            "identifiers leave gaps: {ids:?}"
        );
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
fn the_codec_refuses_a_packet_it_cannot_read() {
    // A packet claiming more values than the bytes could hold must fail
    // rather than allocate.
    let mut bytes = i32::MAX.to_le_bytes().to_vec();
    bytes.push(1);
    let mut c = Cursor::new(&bytes);
    assert!(c.packet(Predictor::None).is_err());
}
