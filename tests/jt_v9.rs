//! Reading JT 9 files, which differ from JT 10 in four places (ADR 0009).
//!
//! The file this was worked out against is a customer model and is not
//! committed, so the fixture here is built in the test: a smallest
//! possible JT 9.5 file with invented properties. It exercises every one
//! of the four differences, and a JT 10 file built the same way is
//! rejected by the same code paths, which is what says the version is
//! doing the choosing rather than luck.

use pmix::jt::file::{Jt, SegmentKind};
use pmix::jt::property;

/// The identifier of the one segment the fixture has.
const LSG_GUID: [u8; 16] = [
    0x01, 0x00, 0x00, 0x00, 0x00, 0x70, 0x6d, 0x69, 0x78, 0x74, 0x65, 0x73, 0x74, 0x00, 0x00, 0x00,
];

/// An element of a segment: length, type, base type, object id, data.
fn element(object_type: [u8; 16], id: i32, data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend(((21 + data.len()) as i32).to_le_bytes());
    out.extend(object_type);
    out.push(0);
    out.extend(id.to_le_bytes());
    out.extend(data);
    out
}

/// The end-of-elements marker that closes a stream.
fn marker() -> Vec<u8> {
    let mut out = 16i32.to_le_bytes().to_vec();
    out.extend([0xFF; 16]);
    out
}

/// A string property atom as JT 9 writes one: the versions are two bytes
/// each where JT 10 writes one, and the string counts its terminator.
fn atom_v9(id: i32, value: &str) -> Vec<u8> {
    let string_property_atom = [
        0x6e, 0x10, 0xdd, 0x10, 0xc8, 0x2a, 0xd1, 0x11, 0x9b, 0x6b, 0x00, 0x80, 0xc7, 0xbb, 0x59,
        0x97,
    ];
    let mut data = Vec::new();
    data.extend(1i16.to_le_bytes()); // base property atom version
    data.extend(0x4000_0000u32.to_le_bytes()); // state flags: visible
    data.extend(1i16.to_le_bytes()); // string property atom version
    let units: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
    data.extend((units.len() as i32).to_le_bytes());
    for u in units {
        data.extend(u.to_le_bytes());
    }
    element(string_property_atom, id, &data)
}

/// A whole JT 9.5 file holding one scene graph segment with `pairs` of
/// properties on one node.
fn jt9(pairs: &[(&str, &str)]) -> Vec<u8> {
    // The scene graph payload: an empty node stream, then the property
    // atoms, then the table that says which node holds which.
    let mut payload = marker();
    let mut id = 1;
    let mut table_entries = Vec::new();
    for (key, value) in pairs {
        payload.extend(atom_v9(id, key));
        payload.extend(atom_v9(id + 1, value));
        table_entries.push((id, id + 1));
        id += 2;
    }
    payload.extend(marker());
    payload.extend(1i16.to_le_bytes()); // property table version
    payload.extend(1i32.to_le_bytes()); // one node has properties
    payload.extend(42i32.to_le_bytes()); // the node
    for (key, value) in table_entries {
        payload.extend(key.to_le_bytes());
        payload.extend(value.to_le_bytes());
    }
    payload.extend(0i32.to_le_bytes()); // no more pairs for this node

    let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&payload, 6);

    // The segment: its header, then the compression fields, then ZLIB.
    let mut segment = LSG_GUID.to_vec();
    segment.extend(1i32.to_le_bytes()); // scene graph
    let body_len = 4 + 4 + 1 + compressed.len();
    segment.extend(((24 + body_len) as i32).to_le_bytes());
    segment.extend(2i32.to_le_bytes()); // compression is on
    segment.extend(((compressed.len() + 1) as i32).to_le_bytes());
    segment.push(2); // ZLIB
    segment.extend(&compressed);

    // The header: 105 bytes, with the table of contents right after it.
    let mut out = Vec::new();
    let mut version = b"Version 9.5 JT  pmix synthetic fixture".to_vec();
    version.resize(76, b' ');
    version.extend(b"\n\r\n ");
    assert_eq!(version.len(), 80);
    out.extend(version);
    out.push(0); // little endian
    out.extend(0i32.to_le_bytes()); // reserved
    out.extend(105u32.to_le_bytes()); // where the table of contents is
    out.extend(LSG_GUID);
    assert_eq!(out.len(), 105);

    // One entry, whose segment follows the table.
    out.extend(1i32.to_le_bytes());
    let offset = 105 + 4 + 28;
    out.extend(LSG_GUID);
    out.extend((offset as u32).to_le_bytes());
    out.extend((segment.len() as i32).to_le_bytes());
    out.extend((1u32 << 24).to_le_bytes()); // type 1 in the high byte
    assert_eq!(out.len(), offset);
    out.extend(segment);
    out
}

#[test]
fn a_jt_9_header_states_its_table_of_contents_in_32_bits() {
    let bytes = jt9(&[("PART_NUMBER", "SYN-004-REV-A")]);
    let jt = Jt::parse(&bytes).unwrap();
    assert_eq!((jt.header.major, jt.header.minor), (9, 5));
    // Four bytes shorter than a version 10 header, because the offset it
    // ends with is half the width.
    assert_eq!(jt.header.length, 105);
    assert_eq!(jt.header.toc_offset, 105);
    assert_eq!(jt.segments().len(), 1);
    assert_eq!(jt.segments()[0].kind, SegmentKind::LogicalSceneGraph);
}

#[test]
fn a_jt_9_segment_is_compressed_with_zlib() {
    let bytes = jt9(&[("PART_NUMBER", "SYN-004-REV-A")]);
    let jt = Jt::parse(&bytes).unwrap();
    let data = jt.segment_data(&jt.segments()[0]).unwrap();
    // Decompressed, or the marker that closes the first stream would not
    // be the first thing in it.
    assert_eq!(&data[..4], &16i32.to_le_bytes());
    assert!(data.len() > 100);
}

#[test]
fn a_jt_9_property_atom_states_its_versions_in_two_bytes_each() {
    let bytes = jt9(&[
        ("PART_NUMBER", "SYN-004-REV-A"),
        ("MATERIAL", "SYN-ALLOY-17"),
    ]);
    let jt = Jt::parse(&bytes).unwrap();
    let data = jt.segment_data(&jt.segments()[0]).unwrap();
    let read = property::read(&data, jt.header.major);
    let pairs = read.by_element.get(&42).expect("the node has properties");
    assert_eq!(
        pairs.as_slice(),
        [
            ("PART_NUMBER".to_owned(), "SYN-004-REV-A".to_owned()),
            ("MATERIAL".to_owned(), "SYN-ALLOY-17".to_owned()),
        ]
    );
    // The terminator the count includes is not part of the value, or a
    // key read here would not match the same key read from a JT 10 file.
    assert_eq!(read.find("PART_NUMBER"), Some("SYN-004-REV-A"));

    // Read as though it were version 10, the versions are a byte each,
    // so the string is looked for two bytes early and nothing is found.
    assert!(property::read(&data, 10).by_element.is_empty());
}

/// A key ending in a double colon is marked visible to a viewer; the
/// marker is a display hint, not part of the name (specification
/// 11.9.1.2). Keeping it would make one property two records across two
/// files, and would put a JT decoration in a name a STEP file also
/// states.
#[test]
fn the_visible_marker_is_not_part_of_a_property_name() {
    use pmix::{ExtractOptions, Reader};
    let bytes = jt9(&[("PART_NUMBER::", "SYN-004-REV-A"), ("SUBNODE", "1")]);
    let doc = pmix::jt::JtReader
        .read(&bytes, "synthetic.jt", &ExtractOptions::default())
        .unwrap();
    let names: Vec<&str> = doc.properties.iter().map(|p| p.name.as_str()).collect();
    assert!(names.contains(&"PART_NUMBER"), "{names:?}");
    assert!(
        !names.iter().any(|n| n.ends_with("::")),
        "the marker reached a name: {names:?}"
    );
    // Marked or not, it is the same property, so it is the same record.
    let plain = jt9(&[("PART_NUMBER", "SYN-004-REV-A"), ("SUBNODE", "1")]);
    let other = pmix::jt::JtReader
        .read(&plain, "synthetic.jt", &ExtractOptions::default())
        .unwrap();
    let id = |d: &pmix::PmiDocument| {
        d.properties
            .iter()
            .find(|p| p.name == "PART_NUMBER")
            .map(|p| p.id.clone())
    };
    assert_eq!(id(&doc), id(&other));
}

#[test]
fn a_whole_jt_9_file_extracts_its_properties() {
    use pmix::{ExtractOptions, Reader};
    let bytes = jt9(&[("PART_NUMBER", "SYN-004-REV-A")]);
    let doc = pmix::jt::JtReader
        .read(&bytes, "synthetic.jt", &ExtractOptions::default())
        .unwrap();
    assert_eq!(doc.source.format, "JT");
    assert!(doc.source.schema.as_deref().unwrap().contains("9.5"));
    let names: Vec<&str> = doc.properties.iter().map(|p| p.name.as_str()).collect();
    assert!(names.contains(&"PART_NUMBER"), "{names:?}");
}
