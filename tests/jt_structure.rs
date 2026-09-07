//! Reading the structure of a JT file (ADR 0009), against the NIST fixture.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use pmix::jt::{ByteOrder, Elements, Guid, Jt, SegmentKind};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/jt/nist_mtc_assembly.jt")
}

fn bytes() -> Vec<u8> {
    std::fs::read(fixture()).expect("the JT fixture is present")
}

/// The PMI Manager meta data element, specification section 8.3.
const PMI_MANAGER: &str = "ce357249-38fb-11d1-a506-006097bdc6e1";
/// The property proxy meta data element.
const PROPERTY_PROXY: &str = "ce357247-38fb-11d1-a506-006097bdc6e1";

#[test]
fn reads_the_header() {
    let b = bytes();
    let jt = Jt::parse(&b).expect("parses");
    assert_eq!(jt.header.major, 10);
    assert_eq!(jt.header.minor, 5);
    assert_eq!(jt.header.byte_order, ByteOrder::Little);
    assert!(
        jt.header.version.starts_with("Version 10.5 JT"),
        "{}",
        jt.header.version
    );
    // The table of contents follows a 109-byte header.
    assert_eq!(jt.header.toc_offset, 109);
    assert_eq!(jt.header.length, 109);
}

#[test]
fn enumerates_every_segment() {
    let b = bytes();
    let jt = Jt::parse(&b).expect("parses");
    assert_eq!(jt.segments().len(), 107);

    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for s in jt.segments() {
        *counts.entry(s.kind.as_str()).or_default() += 1;
    }
    assert_eq!(counts.get("meta data"), Some(&30));
    assert_eq!(counts.get("PMI data"), Some(&14));
    assert_eq!(counts.get("XT B-Rep"), Some(&8));
    assert_eq!(counts.get("logical scene graph"), Some(&1));

    // Every segment lies inside the file and the scene graph segment the
    // header names is one of them.
    for s in jt.segments() {
        assert!(
            s.offset as usize + s.length as usize <= b.len(),
            "segment {} runs past the end of the file",
            s.id
        );
    }
    assert!(
        jt.segments().iter().any(|s| s.id == jt.header.lsg_segment),
        "the header's scene graph segment is in the table of contents"
    );
}

#[test]
fn decompresses_pmi_segments_and_walks_their_elements() {
    let b = bytes();
    let jt = Jt::parse(&b).expect("parses");
    let mut pmi_managers = 0;
    let mut property_proxies = 0;
    let mut decoded = 0;
    let mut expanded = 0;
    let mut elements = 0;

    for segment in jt.segments().iter().filter(|s| s.kind.carries_pmi()) {
        let data = jt
            .segment_data(segment)
            .unwrap_or_else(|e| panic!("segment {} at {}: {e}", segment.id, segment.offset));
        decoded += 1;
        assert!(
            !data.is_empty(),
            "segment {} decoded to nothing",
            segment.id
        );
        if data.len() > segment.length as usize {
            expanded += 1;
        }
        for element in Elements::new(&data) {
            elements += 1;
            let ty = element.object_type.to_string();
            if ty == PMI_MANAGER {
                pmi_managers += 1;
            } else if ty == PROPERTY_PROXY {
                property_proxies += 1;
            }
        }
    }

    assert_eq!(decoded, 44, "30 metadata plus 14 PMI segments");
    assert!(
        elements >= decoded,
        "every segment yielded at least one element"
    );
    // These segments are XZ compressed, so most decode to more bytes than
    // they occupy on disk; the smallest carry less content than their
    // headers, so the count is a floor rather than all of them.
    assert!(
        expanded > decoded / 2,
        "only {expanded} of {decoded} segments grew when decompressed"
    );
    assert!(pmi_managers > 0, "the fixture carries PMI manager elements");
    assert!(property_proxies > 0, "and property proxy elements");
}

#[test]
fn geometry_segments_are_listed_but_not_decoded() {
    let b = bytes();
    let jt = Jt::parse(&b).expect("parses");
    let geometry: Vec<_> = jt
        .segments()
        .iter()
        .filter(|s| matches!(s.kind, SegmentKind::ShapeLod(_) | SegmentKind::XtBRep))
        .collect();
    assert!(!geometry.is_empty());
    assert!(
        geometry.iter().all(|s| !s.kind.carries_pmi()),
        "geometry is never decoded for PMI (ADR 0009)"
    );
}

#[test]
fn rejects_input_that_is_not_jt() {
    assert!(Jt::parse(b"ISO-10303-21;").is_err());
    assert!(Jt::parse(&[]).is_err());
    // A valid header followed by nothing is truncated, not a panic.
    let mut truncated = b"Version 10.5 JT".to_vec();
    truncated.resize(80, b' ');
    assert!(Jt::parse(&truncated).is_err());
}

#[test]
fn the_end_of_elements_marker_is_recognised() {
    let b = bytes();
    let jt = Jt::parse(&b).expect("parses");
    let segment = jt
        .segments()
        .iter()
        .find(|s| s.kind == SegmentKind::PmiData)
        .expect("a PMI segment");
    let data = jt.segment_data(segment).expect("decodes");
    let mut walker = Elements::new(&data);
    let count = walker.by_ref().count();
    assert!(count >= 1);
    // Iteration stopped before the end of the payload, at the marker.
    assert!(
        walker.position() < data.len(),
        "the walker consumed the whole payload instead of stopping at the marker"
    );
    assert_ne!(Guid::END_OF_ELEMENTS, Guid([0; 16]));
}
