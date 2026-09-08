//! The Smart Topology Table (specification annex H).
//!
//! JT stores its precise geometry as Parasolid XT, a format of its own
//! that this reader does not parse. Alongside it a file may carry a
//! Smart Topology Table, which abstracts the same B-rep as counts,
//! topology, and analytic geometry. That is what `pmix` needs: the
//! surfaces a PMI callout applies to, without a Parasolid reader.
//!
//! This reads the counts that head the table, the faces, and walks the
//! rest of the vectors to show the chain is understood. The face section
//! holds five vectors where the specification's figure shows four; three
//! of the five are identified from what they contain, and the other two
//! are left unnamed rather than guessed at.

use std::fmt;

use super::codec::{CodecError, Cursor, Predictor};
use super::file::Guid;

/// Object type identifier of the JT STT Element.
pub const STT_ELEMENT: Guid = Guid([
    0x89, 0x6f, 0x7e, 0xca, 0xc8, 0x97, 0xf0, 0x47, 0x9f, 0xca, 0x16, 0x99, 0x0c, 0xfb, 0xe2, 0x17,
]);

/// A table that could not be read.
#[derive(Debug, Clone, PartialEq)]
pub enum SttError {
    /// The element ended before the table did.
    Truncated,
    /// A compressed packet inside the table was malformed.
    Packet(CodecError),
    /// The table declares more of something than the element could hold.
    Implausible(String),
}

impl fmt::Display for SttError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => f.write_str("the topology table is truncated"),
            Self::Packet(e) => write!(f, "{e}"),
            Self::Implausible(what) => write!(f, "the topology table declares {what}"),
        }
    }
}

impl std::error::Error for SttError {}

impl From<CodecError> for SttError {
    fn from(e: CodecError) -> Self {
        Self::Packet(e)
    }
}

type Result<T> = std::result::Result<T, SttError>;

/// How many of each thing a part's B-rep contains.
///
/// These counts head the table as plain integers, so they are certain in
/// a way the compressed vectors after them are not yet: the per-face
/// fields are read but not yet named, because this file's element writes
/// five vectors per face where the specification's figure shows four.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counts {
    pub bodies: usize,
    pub regions: usize,
    pub shells: usize,
    pub faces: usize,
    pub loops: usize,
    pub coedges: usize,
    pub edges: usize,
}

/// One face of a part's B-rep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Face {
    /// The identifier the producing system persisted for this face.
    ///
    /// A PMI association names a face by its position among the faces of
    /// its body ordered by increasing identifier, which is why this is
    /// needed at all (specification section 11.13). It is not a position
    /// itself: the values are distinct and ascending but leave gaps, so
    /// the largest exceeds the number of faces.
    pub identifier: u32,
    /// Whether the face normal points into the shell that owns it.
    ///
    /// The specification writes two per-face flags and this reader
    /// cannot tell which of its two flag vectors is which by position,
    /// because the section has an extra vector the figure does not show.
    /// The reading is taken from content: this one is clear on every
    /// face of every part in the test file, which is what an outward
    /// facing solid gives, while [`Face::normal_reversed`] varies.
    pub inward: bool,
    /// Whether the face normal opposes the normal of its surface.
    pub normal_reversed: bool,
}

/// What the reader can make of one part's topology table.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Topology {
    pub version: u8,
    pub counts: Counts,
    /// The faces, in the order the table stores them.
    pub faces: Vec<Face>,
    /// How many compressed vectors were read before one could not be,
    /// and the number of values each held. Reading the whole chain is
    /// what shows the table is understood; the meaning of each vector is
    /// the next thing to establish.
    pub vectors: Vec<usize>,
    /// Why reading stopped, when it stopped early.
    pub stopped: Option<String>,
}

/// Read a part's topology table.
///
/// The counts are read from the header and the faces from the section
/// that follows the bodies, regions, and shells. The rest of the vectors
/// are walked without being interpreted.
pub fn parse(data: &[u8]) -> Result<Topology> {
    let head = data.get(..29).ok_or(SttError::Truncated)?;
    let count = |k: usize| u32::from_le_bytes(head[1 + k * 4..5 + k * 4].try_into().unwrap());
    let raw: Vec<u32> = (0..7).map(count).collect();
    // Even at one bit each, a table cannot describe more entities than
    // its bytes could hold. This catches a wild count without assuming
    // anything about how densely the vectors encode.
    let room = data.len().saturating_mul(8);
    if let Some(bad) = raw.iter().find(|c| **c as usize > room) {
        return Err(SttError::Implausible(format!(
            "{bad} entities in {} bytes",
            data.len()
        )));
    }
    let mut out = Topology {
        version: head[0],
        counts: Counts {
            bodies: raw[0] as usize,
            regions: raw[1] as usize,
            shells: raw[2] as usize,
            faces: raw[3] as usize,
            loops: raw[4] as usize,
            coedges: raw[5] as usize,
            edges: raw[6] as usize,
        },
        faces: Vec::new(),
        vectors: Vec::new(),
        stopped: None,
    };

    let mut cursor = Cursor::new(&data[29..]);
    // Two vectors for the bodies, two for the regions, and four for the
    // shells stand between the header and the faces.
    const BEFORE_FACES: usize = 8;
    let mut faces: Vec<Vec<i32>> = Vec::new();
    loop {
        if cursor.at >= data.len() - 29 {
            break;
        }
        let index = out.vectors.len();
        let in_faces = (BEFORE_FACES..BEFORE_FACES + 5).contains(&index);
        // Read every vector as written. Which of them a predictor applies
        // to depends on what they turn out to be, so it is undone below.
        match cursor.packet(Predictor::None) {
            Ok(values) => {
                out.vectors.push(values.len());
                if in_faces {
                    faces.push(values);
                }
            }
            Err(e) => {
                out.stopped = Some(e.to_string());
                break;
            }
        }
    }

    if faces.len() == 5 && faces.iter().all(|v| v.len() == out.counts.faces) {
        // The first vector is written as differences, so each identifier
        // is the running total.
        let mut identifier = faces[0].clone();
        for i in 1..identifier.len() {
            identifier[i] = identifier[i].wrapping_add(identifier[i - 1]);
        }
        // The flags are the vectors written as they stand whose values
        // are only ever set or clear; the other two are not identified.
        let flags: Vec<&Vec<i32>> = faces[1..]
            .iter()
            .filter(|v| v.iter().all(|x| (0..=1).contains(x)))
            .collect();
        out.faces = (0..out.counts.faces)
            .map(|i| Face {
                identifier: identifier[i] as u32,
                inward: flags.first().is_some_and(|v| v[i] != 0),
                normal_reversed: flags.get(1).is_some_and(|v| v[i] != 0),
            })
            .collect();
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_truncated_table_is_reported() {
        assert_eq!(parse(&[]), Err(SttError::Truncated));
        assert_eq!(parse(&[1; 28]), Err(SttError::Truncated));
    }

    #[test]
    fn an_implausible_count_is_refused_before_allocating() {
        let mut data = vec![1u8];
        for _ in 0..7 {
            data.extend(u32::MAX.to_le_bytes());
        }
        let err = parse(&data).unwrap_err();
        assert!(matches!(err, SttError::Implausible(_)), "{err}");
    }

    #[test]
    fn faces_are_left_empty_when_the_vectors_cannot_be_read() {
        let mut data = vec![1u8];
        for c in [1u32, 1, 1, 4, 4, 8, 4] {
            data.extend(c.to_le_bytes());
        }
        let t = parse(&data).unwrap();
        assert!(t.faces.is_empty());
        assert!(t.stopped.is_none() || t.faces.is_empty());
    }

    #[test]
    fn the_counts_are_read_from_the_header() {
        let mut data = vec![1u8];
        for c in [1u32, 2, 2, 32, 40, 122, 61] {
            data.extend(c.to_le_bytes());
        }
        let t = parse(&data).unwrap();
        assert_eq!(t.version, 1);
        assert_eq!(t.counts.faces, 32);
        assert_eq!(t.counts.edges, 61);
        // Nothing follows the header, so no vector was read.
        assert!(t.vectors.is_empty());
    }
}
