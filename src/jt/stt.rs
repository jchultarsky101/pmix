//! The Smart Topology Table (specification annex H).
//!
//! JT stores its precise geometry as Parasolid XT, a format of its own
//! that this reader does not parse. Alongside it a file may carry a
//! Smart Topology Table, which abstracts the same B-rep as counts,
//! topology, and analytic geometry. That is what `pmix` needs: the
//! surfaces a PMI callout applies to, without a Parasolid reader.
//!
//! This reads the counts that head the table, the faces, and the counts
//! that head the geometry after it. The face section holds five vectors
//! where the specification's figure shows four; three of the five are
//! identified from what they contain, and the other two are left unnamed
//! rather than guessed at. The geometry itself is not read yet.

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

/// An analytic surface a face lies on.
///
/// The table describes only the five kinds that have a closed form. A
/// face on anything else has no entry, which is why the geometry states
/// how many of its surfaces it represents.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Surface {
    Plane {
        location: [f64; 3],
        axis: [f64; 3],
    },
    Cylinder {
        location: [f64; 3],
        axis: [f64; 3],
        radius: f64,
    },
    Cone {
        location: [f64; 3],
        axis: [f64; 3],
        radius: f64,
        /// Half the angle at the apex, in radians.
        semi_angle: f64,
    },
    Sphere {
        location: [f64; 3],
        axis: [f64; 3],
        radius: f64,
    },
    Torus {
        location: [f64; 3],
        axis: [f64; 3],
        major_radius: f64,
        minor_radius: f64,
    },
}

impl Surface {
    /// The name this reader uses for the kind of surface.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Plane { .. } => "plane",
            Self::Cylinder { .. } => "cylinder",
            Self::Cone { .. } => "cone",
            Self::Sphere { .. } => "sphere",
            Self::Torus { .. } => "torus",
        }
    }

    /// Where the surface sits.
    pub fn location(&self) -> [f64; 3] {
        match self {
            Self::Plane { location, .. }
            | Self::Cylinder { location, .. }
            | Self::Cone { location, .. }
            | Self::Sphere { location, .. }
            | Self::Torus { location, .. } => *location,
        }
    }

    /// The direction that orients it.
    pub fn axis(&self) -> [f64; 3] {
        match self {
            Self::Plane { axis, .. }
            | Self::Cylinder { axis, .. }
            | Self::Cone { axis, .. }
            | Self::Sphere { axis, .. }
            | Self::Torus { axis, .. } => *axis,
        }
    }
}

/// How many of each thing the geometry holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GeometryCounts {
    pub surfaces: usize,
    /// Surfaces the table actually describes; the rest are implied.
    pub represented_surfaces: usize,
    pub curves: usize,
    pub represented_curves: usize,
    pub points: usize,
}

/// What the reader can make of one part's topology table.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Topology {
    pub version: u8,
    pub counts: Counts,
    /// The faces, in the order the table stores them.
    pub faces: Vec<Face>,
    /// How many surfaces and curves the geometry after the topology
    /// holds. Read only when the whole topology could be.
    pub geometry: Option<GeometryCounts>,
    /// The checksum the file states over its topology.
    pub hash: Option<u32>,
    /// The surfaces the geometry describes, each with the index of the
    /// surface it stands for. Surfaces run parallel to faces, so that
    /// index is also the face's position in [`Topology::faces`].
    pub surfaces: Vec<(usize, Surface)>,
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
        geometry: None,
        hash: None,
        surfaces: Vec::new(),
        vectors: Vec::new(),
        stopped: None,
    };

    // The topology is a fixed chain of compressed vectors: two for the
    // bodies, two for the regions, four for the shells, five for the
    // faces, three for the loops, two for the coedges, and five for the
    // edges. Every table in every file seen so far agrees.
    const BEFORE_FACES: usize = 2 + 2 + 4;
    const FACE_VECTORS: usize = 5;
    const TOPOLOGY_VECTORS: usize = BEFORE_FACES + FACE_VECTORS + 3 + 2 + 5;

    let mut cursor = Cursor::new(&data[29..]);
    let mut faces: Vec<Vec<i32>> = Vec::new();
    while out.vectors.len() < TOPOLOGY_VECTORS {
        let index = out.vectors.len();
        let in_faces = (BEFORE_FACES..BEFORE_FACES + FACE_VECTORS).contains(&index);
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

    // A checksum over the topology, then the counts that head the
    // geometry. Both are plain integers rather than packets.
    if out.vectors.len() == TOPOLOGY_VECTORS {
        let after = 29 + cursor.at;
        let word = |k: usize| {
            data.get(after + k * 4..after + k * 4 + 4)
                .and_then(|b| b.try_into().ok())
                .map(u32::from_le_bytes)
        };
        out.hash = word(0);
        if let (Some(surfaces), Some(rs), Some(curves), Some(rc), Some(points)) =
            (word(1), word(2), word(3), word(4), word(5))
        {
            let counts = GeometryCounts {
                surfaces: surfaces as usize,
                represented_surfaces: rs as usize,
                curves: curves as usize,
                represented_curves: rc as usize,
                points: points as usize,
            };
            out.geometry = Some(counts);
            if counts.represented_surfaces > 0 {
                // The counts are six words past the last vector.
                cursor.at += 6 * 4;
                out.surfaces = read_surfaces(&mut cursor).unwrap_or_default();
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

/// Read the surfaces the geometry describes.
///
/// Each surface takes what it needs from four arrays laid end to end,
/// in the order the surfaces appear: every kind takes a location and two
/// directions, and the kinds with curvature take radii and angles as
/// well (specification figure H.15).
fn read_surfaces(cursor: &mut Cursor<'_>) -> Result<Vec<(usize, Surface)>> {
    let index = cursor.packet_u32(Predictor::Lag1)?;
    // The type values run one above the enumeration the specification
    // lists, so a plane is written as 1 rather than 0. The counts prove
    // it: with the documented values the radius and angle arrays are far
    // too short for the surfaces they would have to describe, and with
    // these they match exactly.
    let kinds: Vec<u32> = cursor
        .packet_u32(Predictor::None)?
        .into_iter()
        .map(|k| k.saturating_sub(1))
        .collect();
    let coordinates = cursor.floats()?;
    let axes = cursor.floats()?;
    let radii = cursor.floats()?;
    let radians = cursor.floats()?;

    let triple = |v: &[f64], at: usize| -> Option<[f64; 3]> {
        Some([*v.get(at)?, *v.get(at + 1)?, *v.get(at + 2)?])
    };
    let (mut coordinate, mut axis, mut radius, mut radian) = (0, 0, 0, 0);
    let mut out = Vec::with_capacity(kinds.len().min(1 << 16));
    for (position, kind) in kinds.iter().enumerate() {
        let (Some(location), Some(direction)) =
            (triple(&coordinates, coordinate), triple(&axes, axis))
        else {
            break;
        };
        // Every kind takes a location and a pair of directions; the
        // second direction fixes the rotation about the first and says
        // nothing a fingerprint needs, so it is stepped over.
        coordinate += 3;
        axis += 6;
        let mut take_radius = || {
            let v = radii.get(radius).copied();
            radius += 1;
            v
        };
        let surface = match kind {
            0 => Surface::Plane {
                location,
                axis: direction,
            },
            1 | 3 => {
                let Some(r) = take_radius() else { break };
                if *kind == 1 {
                    Surface::Cylinder {
                        location,
                        axis: direction,
                        radius: r,
                    }
                } else {
                    Surface::Sphere {
                        location,
                        axis: direction,
                        radius: r,
                    }
                }
            }
            2 => {
                let Some(r) = take_radius() else { break };
                let Some(angle) = radians.get(radian).copied() else {
                    break;
                };
                radian += 1;
                Surface::Cone {
                    location,
                    axis: direction,
                    radius: r,
                    semi_angle: angle,
                }
            }
            4 => {
                let (Some(major), Some(minor)) = (take_radius(), take_radius()) else {
                    break;
                };
                Surface::Torus {
                    location,
                    axis: direction,
                    major_radius: major,
                    minor_radius: minor,
                }
            }
            // A kind the table does not define leaves the arrays at an
            // unknown offset, so nothing after it can be trusted.
            _ => break,
        };
        out.push((
            index.get(position).copied().unwrap_or(position as u32) as usize,
            surface,
        ));
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
