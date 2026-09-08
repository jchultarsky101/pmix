//! Reading the smart topology table of a real JT file (ADR 0010).

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

#[test]
fn a_table_says_why_it_stopped_rather_than_guessing_on() {
    // Every table stops somewhere, for one of two honest reasons: it
    // reaches a codec the reader does not implement, or it reaches the
    // end of the vectors and the bytes after them are not a packet.
    // Neither produces numbers the reader cannot stand behind.
    let tables = tables();
    assert!(tables.iter().all(|t| t.stopped.is_some()));
    let unimplemented = tables
        .iter()
        .filter(|t| {
            t.stopped
                .as_deref()
                .is_some_and(|w| w.contains("not implemented"))
        })
        .count();
    assert!(
        unimplemented >= 3,
        "most parts reach a codec not written yet, not {unimplemented}"
    );
    // Whatever the reason, it names the byte it stopped at, so the edge
    // of what the reader understands is always locatable.
    for t in &tables {
        let why = t.stopped.as_deref().unwrap();
        assert!(why.starts_with("compressed packet at byte"), "{why}");
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
