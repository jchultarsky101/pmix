//! JT file structure: header, table of contents, and segments.

use std::fmt;

/// A JT globally unique identifier.
///
/// Stored as the raw 16 bytes. The first three fields are little-endian,
/// which is how the specification writes them and how [`fmt::Display`]
/// renders them.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Guid(pub [u8; 16]);

impl Guid {
    /// The end-of-elements marker: every byte `0xFF`.
    pub const END_OF_ELEMENTS: Guid = Guid([0xFF; 16]);

    fn read(bytes: &[u8]) -> Option<Self> {
        Some(Self(bytes.get(..16)?.try_into().ok()?))
    }
}

impl fmt::Display for Guid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let b = &self.0;
        let a = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        let c = u16::from_le_bytes([b[4], b[5]]);
        let d = u16::from_le_bytes([b[6], b[7]]);
        write!(f, "{a:08x}-{c:04x}-{d:04x}-")?;
        for x in &b[8..10] {
            write!(f, "{x:02x}")?;
        }
        f.write_str("-")?;
        for x in &b[10..16] {
            write!(f, "{x:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for Guid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Guid({self})")
    }
}

impl serde::Serialize for Guid {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

/// Byte order declared by the file header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ByteOrder {
    Little,
    Big,
}

/// A failure that makes the file unreadable.
#[derive(Debug, Clone, PartialEq)]
pub enum ParseError {
    /// The file does not begin with a JT version string.
    NotJt,
    /// The file is truncated at the given offset.
    Truncated { offset: usize, needed: usize },
    /// Big-endian files are not supported (ADR 0009).
    BigEndian,
    /// A segment's payload could not be decompressed.
    Decompression { segment: Guid, reason: String },
    /// A segment declared a compression algorithm the reader does not know.
    UnknownCompression { segment: Guid, algorithm: u8 },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotJt => f.write_str("not a JT file: no `Version` header"),
            Self::Truncated { offset, needed } => {
                write!(
                    f,
                    "file truncated at offset {offset}, needed {needed} bytes"
                )
            }
            Self::BigEndian => f.write_str("big-endian JT files are not supported yet"),
            Self::Decompression { segment, reason } => {
                write!(f, "segment {segment} could not be decompressed: {reason}")
            }
            Self::UnknownCompression { segment, algorithm } => {
                write!(
                    f,
                    "segment {segment} uses unknown compression algorithm {algorithm}"
                )
            }
        }
    }
}

impl std::error::Error for ParseError {}

type Result<T> = std::result::Result<T, ParseError>;

/// The file header.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Header {
    /// The full 80-character version string, trimmed.
    pub version: String,
    /// Major version, e.g. 10.
    pub major: u32,
    /// Minor version, e.g. 5.
    pub minor: u32,
    pub byte_order: ByteOrder,
    /// Byte offset of the table of contents.
    pub toc_offset: u64,
    /// Identifier of the logical scene graph segment.
    pub lsg_segment: Guid,
    /// Length of the header in bytes.
    pub length: usize,
}

/// What a segment contains (specification table 6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SegmentKind {
    LogicalSceneGraph,
    JtBRep,
    PmiData,
    MetaData,
    Shape,
    /// Shape level of detail 0 through 9.
    ShapeLod(u8),
    XtBRep,
    Wireframe,
    Ulp,
    Stt,
    Lwpa,
    MultiXtBRep,
    InfoSegment,
    StepBRep,
    /// A type the specification does not list.
    Other(u8),
}

impl SegmentKind {
    fn from_code(code: u8) -> Self {
        match code {
            1 => Self::LogicalSceneGraph,
            2 => Self::JtBRep,
            3 => Self::PmiData,
            4 => Self::MetaData,
            6 => Self::Shape,
            7..=16 => Self::ShapeLod(code - 7),
            17 => Self::XtBRep,
            18 => Self::Wireframe,
            20 => Self::Ulp,
            23 => Self::Stt,
            24 => Self::Lwpa,
            30 => Self::MultiXtBRep,
            31 => Self::InfoSegment,
            33 => Self::StepBRep,
            other => Self::Other(other),
        }
    }

    /// Whether the type may have compression applied to all its data.
    pub fn compressible(&self) -> bool {
        !matches!(self, Self::Shape | Self::ShapeLod(_) | Self::Other(_))
    }

    /// Whether this segment can carry PMI or metadata, and so is worth
    /// decoding (ADR 0009).
    pub fn carries_pmi(&self) -> bool {
        matches!(self, Self::PmiData | Self::MetaData)
    }

    /// Name used in output.
    pub fn as_str(&self) -> String {
        match self {
            Self::LogicalSceneGraph => "logical scene graph".into(),
            Self::JtBRep => "JT B-Rep".into(),
            Self::PmiData => "PMI data".into(),
            Self::MetaData => "meta data".into(),
            Self::Shape => "shape".into(),
            Self::ShapeLod(n) => format!("shape LOD{n}"),
            Self::XtBRep => "XT B-Rep".into(),
            Self::Wireframe => "wireframe".into(),
            Self::Ulp => "ULP".into(),
            Self::Stt => "STT".into(),
            Self::Lwpa => "LWPA".into(),
            Self::MultiXtBRep => "multi XT B-Rep".into(),
            Self::InfoSegment => "info segment".into(),
            Self::StepBRep => "STEP B-Rep".into(),
            Self::Other(c) => format!("type {c}"),
        }
    }
}

impl fmt::Display for SegmentKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.as_str())
    }
}

impl serde::Serialize for SegmentKind {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

/// One entry of the table of contents.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Segment {
    pub id: Guid,
    pub kind: SegmentKind,
    pub offset: u64,
    pub length: u32,
    /// The raw attributes word; the high byte is the type code.
    pub attributes: u32,
}

/// A parsed JT file. Segment payloads are decoded on demand.
#[derive(Debug)]
pub struct Jt<'a> {
    pub header: Header,
    pub segments: Vec<Segment>,
    bytes: &'a [u8],
}

fn u32_at(b: &[u8], at: usize) -> Result<u32> {
    let s = b.get(at..at + 4).ok_or(ParseError::Truncated {
        offset: at,
        needed: 4,
    })?;
    Ok(u32::from_le_bytes(s.try_into().unwrap()))
}

fn i32_at(b: &[u8], at: usize) -> Result<i32> {
    u32_at(b, at).map(|v| v as i32)
}

fn u64_at(b: &[u8], at: usize) -> Result<u64> {
    let s = b.get(at..at + 8).ok_or(ParseError::Truncated {
        offset: at,
        needed: 8,
    })?;
    Ok(u64::from_le_bytes(s.try_into().unwrap()))
}

fn guid_at(b: &[u8], at: usize) -> Result<Guid> {
    Guid::read(b.get(at..).unwrap_or(&[])).ok_or(ParseError::Truncated {
        offset: at,
        needed: 16,
    })
}

/// Length of the segment header: identifier, type, and length.
const SEGMENT_HEADER: usize = 16 + 4 + 4;

impl<'a> Jt<'a> {
    /// Parse the header and table of contents. Segment payloads are not
    /// touched.
    pub fn parse(bytes: &'a [u8]) -> Result<Self> {
        let header = Self::parse_header(bytes)?;
        if header.byte_order == ByteOrder::Big {
            return Err(ParseError::BigEndian);
        }
        let toc = header.toc_offset as usize;
        let count = i32_at(bytes, toc)?.max(0) as usize;
        // An entry states its segment's offset in the same width the
        // header states the table's own, so version 10 entries are four
        // bytes longer than the ones before them.
        let wide = header.major >= 10;
        let entry = if wide { 32 } else { 28 };
        let mut segments = Vec::with_capacity(count.min(4096));
        for i in 0..count {
            let at = toc + 4 + i * entry;
            let offset = if wide {
                u64_at(bytes, at + 16)?
            } else {
                u32_at(bytes, at + 16)? as u64
            };
            let after = at + 16 + if wide { 8 } else { 4 };
            let attributes = u32_at(bytes, after + 4)?;
            segments.push(Segment {
                id: guid_at(bytes, at)?,
                kind: SegmentKind::from_code((attributes >> 24) as u8),
                offset,
                length: u32_at(bytes, after)?,
                attributes,
            });
        }
        Ok(Self {
            header,
            segments,
            bytes,
        })
    }

    fn parse_header(bytes: &[u8]) -> Result<Header> {
        let raw = bytes.get(..80).ok_or(ParseError::Truncated {
            offset: 0,
            needed: 80,
        })?;
        let version = String::from_utf8_lossy(raw)
            .trim_end_matches(['\0', ' ', '\n', '\r'])
            .trim()
            .to_owned();
        if !version.starts_with("Version") {
            return Err(ParseError::NotJt);
        }
        // "Version M.n Comment"
        let (mut major, mut minor) = (0, 0);
        if let Some(number) = version.split_whitespace().nth(1) {
            let (m, n) = number.split_once('.').unwrap_or((number, "0"));
            major = m.parse().unwrap_or(0);
            minor = n.parse().unwrap_or(0);
        }
        let byte_order = match bytes.get(80) {
            Some(0) => ByteOrder::Little,
            Some(_) => ByteOrder::Big,
            None => {
                return Err(ParseError::Truncated {
                    offset: 80,
                    needed: 1,
                });
            }
        };
        let reserved = i32_at(bytes, 81)?;
        // Version 10 widened the offset of the table of contents to 64
        // bits, which moved everything after it. Older files state it in
        // 32, so their header is four bytes shorter.
        let (toc_offset, lsg_segment, length) = if major >= 10 {
            let toc = u64_at(bytes, 85)?;
            // A non-zero reserved field is followed by one more identifier.
            let length = if reserved != 0 { 109 + 16 } else { 109 };
            (toc, guid_at(bytes, 93)?, length)
        } else {
            (u32_at(bytes, 85)? as u64, guid_at(bytes, 89)?, 105)
        };
        Ok(Header {
            version,
            major,
            minor,
            byte_order,
            toc_offset,
            lsg_segment,
            length,
        })
    }

    /// Segments in table-of-contents order.
    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }

    /// The decoded payload of a segment: the element stream, decompressed
    /// when the segment says it is compressed.
    pub fn segment_data(&self, segment: &Segment) -> Result<Vec<u8>> {
        let start = segment.offset as usize;
        let end = start
            .checked_add(segment.length as usize)
            .unwrap_or(self.bytes.len())
            .min(self.bytes.len());
        let body = start + SEGMENT_HEADER;
        if body > end {
            return Err(ParseError::Truncated {
                offset: start,
                needed: SEGMENT_HEADER,
            });
        }
        if !segment.kind.compressible() {
            return Ok(self.bytes[body..end].to_vec());
        }
        // The first element of a compressible segment is prefixed with the
        // compression flag, the compressed length, and the algorithm.
        let flag = u32_at(self.bytes, body)?;
        let compressed_len = i32_at(self.bytes, body + 4)?.max(0) as usize;
        let algorithm = *self.bytes.get(body + 8).ok_or(ParseError::Truncated {
            offset: body + 8,
            needed: 1,
        })?;
        // The flag and the algorithm say the same thing: version 10
        // writes 3 for XZ, and version 9 and older write 2 for ZLIB. The
        // algorithm byte counts toward the compressed length.
        let uncompressed = || Ok(self.bytes[body + 9..end].to_vec());
        let known = matches!(algorithm, 2 | 3);
        if algorithm == 1 || !matches!(flag, 2 | 3) {
            return uncompressed();
        }
        if !known {
            // The flag says the data is compressed by something this
            // reader has no decoder for. Handing back the bytes as they
            // stand would parse as nonsense, so say so instead.
            return Err(ParseError::UnknownCompression {
                segment: segment.id,
                algorithm,
            });
        }
        // The compressed length is what says how far the payload runs,
        // and it is the field to trust: this file's segments state a
        // length one byte short of what they occupy, so bounding the
        // payload by that instead would cut the last byte off the
        // stream. The buffer is still the hard limit.
        let from = body + 9;
        let to = (body + 8 + compressed_len).min(self.bytes.len());
        let payload = self.bytes.get(from..to).ok_or(ParseError::Truncated {
            offset: from,
            needed: compressed_len,
        })?;
        let fail = |reason: String| ParseError::Decompression {
            segment: segment.id,
            reason,
        };
        match algorithm {
            2 => miniz_oxide::inflate::decompress_to_vec_zlib(payload)
                .map_err(|e| fail(e.to_string())),
            _ => {
                let mut out = Vec::new();
                lzma_rs::xz_decompress(&mut std::io::Cursor::new(payload), &mut out)
                    .map_err(|e| fail(e.to_string()))?;
                Ok(out)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guid_renders_in_the_specification_order() {
        // 0xce357249, 0x38fb, 0x11d1, 0xa5, 0x6, 0x0, 0x60, 0x97, 0xbd, 0xc6, 0xe1
        let g = Guid([
            0x49, 0x72, 0x35, 0xce, 0xfb, 0x38, 0xd1, 0x11, 0xa5, 0x06, 0x00, 0x60, 0x97, 0xbd,
            0xc6, 0xe1,
        ]);
        assert_eq!(g.to_string(), "ce357249-38fb-11d1-a506-006097bdc6e1");
    }

    #[test]
    fn segment_kinds_follow_the_specification_table() {
        assert_eq!(SegmentKind::from_code(1), SegmentKind::LogicalSceneGraph);
        assert_eq!(SegmentKind::from_code(3), SegmentKind::PmiData);
        assert_eq!(SegmentKind::from_code(7), SegmentKind::ShapeLod(0));
        assert_eq!(SegmentKind::from_code(16), SegmentKind::ShapeLod(9));
        assert_eq!(SegmentKind::from_code(17), SegmentKind::XtBRep);
        assert_eq!(SegmentKind::from_code(99), SegmentKind::Other(99));
        assert!(SegmentKind::PmiData.compressible());
        assert!(!SegmentKind::ShapeLod(0).compressible());
        assert!(SegmentKind::MetaData.carries_pmi());
        assert!(!SegmentKind::XtBRep.carries_pmi());
    }

    #[test]
    fn rejects_input_that_is_not_jt() {
        // Long enough to hold a header, but not a JT one.
        let mut wrong = b"ISO-10303-21;".to_vec();
        wrong.resize(200, b' ');
        assert_eq!(Jt::parse(&wrong).unwrap_err(), ParseError::NotJt);

        // Too short to hold a version string at all.
        assert!(matches!(
            Jt::parse(b"short").unwrap_err(),
            ParseError::Truncated { .. }
        ));

        // A JT header whose table of contents lies past the end of the file.
        let mut truncated = b"Version 10.5 JT".to_vec();
        truncated.resize(109, b' ');
        truncated[80] = 0;
        assert!(matches!(
            Jt::parse(&truncated).unwrap_err(),
            ParseError::Truncated { .. }
        ));
    }
}
