//! The Smart Topology Table (specification annex H).
//!
//! JT stores its precise geometry as Parasolid XT, a format of its own
//! that this reader does not parse. Alongside it a file may carry a
//! Smart Topology Table, which abstracts the same B-rep as counts,
//! topology, and analytic geometry. That is what `pmix` needs: the
//! surfaces a PMI callout applies to, without a Parasolid reader.
//!
//! The whole table is read: the counts that head it, the chain of
//! compressed vectors that ties bodies down to edges, and the analytic
//! surfaces and curves after it. Walking the chain is what proves the
//! reading, because it lands on exactly the counts the header declared:
//! every loop is reached from one face, every coedge from one loop, and
//! every edge from two coedges.
//!
//! Two sections hold one vector more than the specification's figures
//! show. Both extra vectors turned out to be the kind of geometry the
//! entity lies on, written for every face and every edge rather than
//! only for the ones the table goes on to describe.

use std::fmt;
use std::ops::Range;

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
/// These counts head the table as plain integers, and every section
/// after them holds one value per entity counted here.
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

/// The kind of surface a face lies on.
///
/// This is stated for every face, including the ones whose surface the
/// table does not go on to describe, so a face can be told apart from
/// its neighbours even when its geometry is a spline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceKind {
    Plane,
    Cylinder,
    Cone,
    Sphere,
    Torus,
    /// Anything the table has no closed form for.
    Other(u32),
}

impl SurfaceKind {
    fn from_face(value: u32) -> Self {
        match value {
            0 => Self::Plane,
            1 => Self::Cylinder,
            2 => Self::Cone,
            3 => Self::Sphere,
            4 => Self::Torus,
            other => Self::Other(other),
        }
    }

    /// The name this reader uses for the kind.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Plane => "plane",
            Self::Cylinder => "cylinder",
            Self::Cone => "cone",
            Self::Sphere => "sphere",
            Self::Torus => "torus",
            Self::Other(_) => "other",
        }
    }
}

/// The kind of curve an edge lies on, stated for every edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurveKind {
    Line,
    Circle,
    Ellipse,
    /// Anything the table has no closed form for.
    Other(u32),
}

impl CurveKind {
    fn from_edge(value: u32) -> Self {
        match value {
            0 => Self::Line,
            1 => Self::Circle,
            2 => Self::Ellipse,
            other => Self::Other(other),
        }
    }

    /// The name this reader uses for the kind.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Line => "line",
            Self::Circle => "circle",
            Self::Ellipse => "ellipse",
            Self::Other(_) => "other",
        }
    }
}

/// One face of a part's B-rep.
#[derive(Debug, Clone, PartialEq)]
pub struct Face {
    /// The identifier the producing system persisted for this face.
    ///
    /// A PMI association names a face by its position among the faces of
    /// its body ordered by increasing identifier (specification section
    /// 11.13), which is why this is needed at all.
    pub identifier: u32,
    /// Whether the face normal points into the shell that owns it.
    ///
    /// The specification contradicts itself: its prose sets the flag
    /// when the normal points inward and its table when it points
    /// outward. The prose is taken, because it makes every face of every
    /// solid in the test file point outward, as a solid's faces do.
    pub inward: bool,
    /// Whether the face normal opposes the normal of its surface.
    pub normal_reversed: bool,
    /// The face's loops, as a range over [`Topology::loops`].
    pub loops: Range<usize>,
    /// What the face lies on, named even when it is not described below.
    pub surface_kind: SurfaceKind,
    /// The surface itself, when the table describes it.
    pub surface: Option<Surface>,
    /// The tag the originating system knows this face by.
    ///
    /// This is what a PMI association names a face with, so it is what
    /// ties an annotation to the geometry it applies to.
    pub tag: Option<u32>,
}

/// One trim loop, a closed circuit of coedges bounding a face.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loop {
    /// The loop's coedges, as a range over [`Topology::coedges`].
    pub coedges: Range<usize>,
    /// The loop type (specification table H.7); 2 is an outer boundary
    /// and 3 a hole.
    pub kind: u32,
    /// The vertex a loop of no edges stands at.
    pub vertex: Option<u32>,
}

/// One oriented use of an edge by a loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoEdge {
    /// Which edge, as a position in [`Topology::edges`].
    pub edge: usize,
    /// Whether the coedge runs the same way as the edge.
    pub forward: bool,
}

/// One edge of a part's B-rep.
#[derive(Debug, Clone, PartialEq)]
pub struct Edge {
    /// Whether the edge runs the same way as its curve.
    pub forward: bool,
    /// Whether the edge is shorter than ten microns.
    pub tiny: bool,
    pub start_vertex: u32,
    pub end_vertex: u32,
    /// What the edge lies on, named even when it is not described below.
    pub curve_kind: CurveKind,
    /// The curve itself, when the table describes it.
    pub curve: Option<Curve>,
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

/// An analytic curve an edge lies on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Curve {
    Line {
        point: [f64; 3],
        direction: [f64; 3],
    },
    Circle {
        centre: [f64; 3],
        axis: [f64; 3],
        radius: f64,
    },
    Ellipse {
        centre: [f64; 3],
        axis: [f64; 3],
        major_radius: f64,
        minor_radius: f64,
    },
}

impl Curve {
    /// The name this reader uses for the kind of curve.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Line { .. } => "line",
            Self::Circle { .. } => "circle",
            Self::Ellipse { .. } => "ellipse",
        }
    }

    /// A point the curve passes through, or the centre it turns about.
    pub fn location(&self) -> [f64; 3] {
        match self {
            Self::Line { point, .. } => *point,
            Self::Circle { centre, .. } | Self::Ellipse { centre, .. } => *centre,
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
    /// The loops, pointed into by [`Face::loops`].
    pub loops: Vec<Loop>,
    /// The coedges, pointed into by [`Loop::coedges`].
    pub coedges: Vec<CoEdge>,
    /// The edges, pointed into by [`CoEdge::edge`].
    pub edges: Vec<Edge>,
    /// How many surfaces and curves the geometry after the topology
    /// holds. Read only when the whole topology could be.
    pub geometry: Option<GeometryCounts>,
    /// The checksum the file states over its topology.
    pub hash: Option<u32>,
    /// How many values each compressed vector held, in the order the
    /// table writes them. Reading the whole chain is what shows the
    /// table is understood.
    pub vectors: Vec<usize>,
    /// Why reading stopped, when it stopped early.
    pub stopped: Option<String>,
}

impl Topology {
    /// The face a PMI association names, by the tag it names it with.
    pub fn face_with_tag(&self, tag: u32) -> Option<&Face> {
        self.faces.iter().find(|f| f.tag == Some(tag))
    }
}

/// The compressed vectors of the topology, read but not yet assembled.
struct Chain {
    start_loop: Vec<u32>,
    identifier: Vec<u32>,
    orientation: Vec<i32>,
    normal_reversed: Vec<i32>,
    face_surface_kind: Vec<u32>,
    start_coedge: Vec<u32>,
    loop_kind: Vec<u32>,
    loop_vertex: Vec<i32>,
    coedge_edge: Vec<u32>,
    coedge_sense: Vec<i32>,
    edge_sense: Vec<i32>,
    tiny: Vec<i32>,
    start_vertex: Vec<u32>,
    end_vertex: Vec<u32>,
    edge_curve_kind: Vec<u32>,
}

/// A cursor that also records the length of every vector it reads.
struct Vectors<'a> {
    cursor: Cursor<'a>,
    lengths: Vec<usize>,
}

impl Vectors<'_> {
    fn signed(&mut self, predictor: Predictor) -> Result<Vec<i32>> {
        let v = self.cursor.packet(predictor)?;
        self.lengths.push(v.len());
        Ok(v)
    }

    fn unsigned(&mut self, predictor: Predictor) -> Result<Vec<u32>> {
        let v = self.cursor.packet_u32(predictor)?;
        self.lengths.push(v.len());
        Ok(v)
    }
}

/// Read the chain of topology vectors in the order annex H writes them.
///
/// The predictor each vector uses is not written in the file; it belongs
/// to the field, so it is named here from the specification's figures.
fn read_chain(v: &mut Vectors<'_>) -> Result<Chain> {
    // Bodies (figure H.5), regions (H.6), and shells (H.7). Nothing
    // above a face is needed yet, but the vectors have to be stepped
    // over to reach the ones that are. Start Face Index has one entry
    // per represented shell rather than per shell, because an inner
    // shell shares its faces with its outer counterpart.
    v.unsigned(Predictor::Lag1)?; // start region index
    v.signed(Predictor::None)?; // body types
    v.unsigned(Predictor::Lag1)?; // start shell index
    v.signed(Predictor::None)?; // solid region flag
    v.unsigned(Predictor::Lag1)?; // start face index
    v.signed(Predictor::None)?; // shell types
    v.signed(Predictor::None)?; // shell signs
    v.unsigned(Predictor::Lag1)?; // shell map

    Ok(Chain {
        // Faces (figure H.8), plus the surface kind the figure omits.
        start_loop: v.unsigned(Predictor::Lag1)?,
        identifier: v.unsigned(Predictor::Lag1)?,
        orientation: v.signed(Predictor::None)?,
        normal_reversed: v.signed(Predictor::None)?,
        face_surface_kind: v.unsigned(Predictor::None)?,
        // Loops (figure H.9).
        start_coedge: v.unsigned(Predictor::Lag1)?,
        loop_kind: v.unsigned(Predictor::None)?,
        loop_vertex: v.signed(Predictor::None)?,
        // CoEdges (figure H.10).
        coedge_edge: v.unsigned(Predictor::None)?,
        coedge_sense: v.signed(Predictor::None)?,
        // Edges (figure H.11), plus the curve kind the figure omits.
        edge_sense: v.signed(Predictor::None)?,
        tiny: v.signed(Predictor::None)?,
        start_vertex: v.unsigned(Predictor::None)?,
        end_vertex: v.unsigned(Predictor::None)?,
        edge_curve_kind: v.unsigned(Predictor::None)?,
    })
}

/// Turn a vector of start indices into the range each owner covers.
///
/// The last owner runs to the end of the section. An owner with nothing
/// of its own gives an empty range rather than a backwards one, which a
/// loop standing at a single vertex does.
fn ranges(starts: &[u32], total: usize) -> Vec<Range<usize>> {
    (0..starts.len())
        .map(|k| {
            let from = (starts[k] as usize).min(total);
            let to = starts
                .get(k + 1)
                .map_or(total, |n| (*n as usize).min(total))
                .max(from);
            from..to
        })
        .collect()
}

/// Read a part's topology table.
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
    let counts = Counts {
        bodies: raw[0] as usize,
        regions: raw[1] as usize,
        shells: raw[2] as usize,
        faces: raw[3] as usize,
        loops: raw[4] as usize,
        coedges: raw[5] as usize,
        edges: raw[6] as usize,
    };
    let mut out = Topology {
        version: head[0],
        counts,
        ..Topology::default()
    };

    let mut vectors = Vectors {
        cursor: Cursor::new(&data[29..]),
        lengths: Vec::new(),
    };
    let chain = match read_chain(&mut vectors) {
        Ok(chain) => chain,
        Err(e) => {
            out.stopped = Some(e.to_string());
            out.vectors = vectors.lengths;
            return Ok(out);
        }
    };

    // A checksum over the topology, then the counts that head the
    // geometry. Both are plain integers rather than packets.
    let after = 29 + vectors.cursor.at;
    let word = |k: usize| {
        data.get(after + k * 4..after + k * 4 + 4)
            .and_then(|b| b.try_into().ok())
            .map(u32::from_le_bytes)
    };
    out.hash = word(0);
    let mut surfaces = Vec::new();
    let mut curves = Vec::new();
    let mut face_tags: Vec<u32> = Vec::new();
    if let (Some(s), Some(rs), Some(c), Some(rc), Some(points)) =
        (word(1), word(2), word(3), word(4), word(5))
    {
        let geometry = GeometryCounts {
            surfaces: s as usize,
            represented_surfaces: rs as usize,
            curves: c as usize,
            represented_curves: rc as usize,
            points: points as usize,
        };
        out.geometry = Some(geometry);
        // The counts are six words past the last vector.
        vectors.cursor.at += 6 * 4;
        if geometry.represented_surfaces > 0 {
            surfaces = read_surfaces(&mut vectors.cursor).unwrap_or_default();
        }
        if geometry.represented_curves > 0 {
            curves = read_curves(&mut vectors.cursor).unwrap_or_default();
        }
        // The attributes come after the geometry, and the face tags in
        // them are what a PMI callout names, so the points have to be
        // stepped over to reach them.
        if skip_points(&mut vectors.cursor).is_ok() {
            face_tags = read_face_tags(&mut vectors.cursor).unwrap_or_default();
        }
    }

    let loop_ranges = ranges(&chain.start_coedge, counts.coedges);
    out.loops = (0..counts.loops.min(chain.start_coedge.len()))
        .map(|k| Loop {
            coedges: loop_ranges[k].clone(),
            kind: chain.loop_kind.get(k).copied().unwrap_or_default(),
            vertex: match chain.loop_vertex.get(k) {
                Some(v) if *v >= 0 => Some(*v as u32),
                _ => None,
            },
        })
        .collect();
    out.coedges = (0..counts.coedges.min(chain.coedge_edge.len()))
        .map(|k| CoEdge {
            edge: chain.coedge_edge[k] as usize,
            forward: chain.coedge_sense.get(k).copied().unwrap_or_default() != 0,
        })
        .collect();
    out.edges = (0..counts.edges.min(chain.start_vertex.len()))
        .map(|k| Edge {
            forward: chain.edge_sense.get(k).copied().unwrap_or_default() != 0,
            tiny: chain.tiny.get(k).copied().unwrap_or_default() != 0,
            start_vertex: chain.start_vertex[k],
            end_vertex: chain.end_vertex.get(k).copied().unwrap_or_default(),
            curve_kind: CurveKind::from_edge(
                chain.edge_curve_kind.get(k).copied().unwrap_or_default(),
            ),
            curve: None,
        })
        .collect();

    let face_ranges = ranges(&chain.start_loop, counts.loops);
    out.faces = (0..counts.faces.min(chain.identifier.len()))
        .map(|k| Face {
            identifier: chain.identifier[k],
            inward: chain.orientation.get(k).copied().unwrap_or_default() != 0,
            normal_reversed: chain.normal_reversed.get(k).copied().unwrap_or_default() != 0,
            loops: face_ranges[k].clone(),
            surface_kind: SurfaceKind::from_face(
                chain.face_surface_kind.get(k).copied().unwrap_or_default(),
            ),
            surface: None,
            // The attribute section writes one tag per face group, and
            // the table stores its faces in that same order, so the two
            // line up position for position.
            tag: face_tags.get(k).copied(),
        })
        .collect();

    // The index a described surface carries is the position of the face
    // it belongs to, and likewise a curve's is the position of its edge.
    // Not a guess: with this reading every analytic curve bounding an
    // analytic face lies on that face's surface, on every part of the
    // test file, and neither of the other two readings comes close.
    for (at, surface) in surfaces {
        if let Some(face) = out.faces.get_mut(at) {
            face.surface = Some(surface);
        }
    }
    for (at, curve) in curves {
        if let Some(edge) = out.edges.get_mut(at) {
            edge.curve = Some(curve);
        }
    }

    out.vectors = vectors.lengths;
    Ok(out)
}

/// Step over the point geometry, which is quantised rather than exact
/// and which nothing needs yet (specification figure H.18).
///
/// A point is written explicitly only when it cannot be recovered from
/// the curves that meet there, so the flags say how many follow.
fn skip_points(cursor: &mut Cursor<'_>) -> Result<()> {
    let explicit = cursor.packet(Predictor::None)?;
    // The figure branches only around the quantiser, but the
    // coordinates go with it: a part with nothing written explicitly
    // has neither, and its precision follows the flags directly.
    if explicit.iter().any(|f| *f != 0) {
        cursor.skip(4 + 4 + 1)?; // the quantiser: a range and a width
        cursor.packet(Predictor::None)?; // the coordinates
    }
    cursor.skip(4)?; // the precision they were quantised to
    cursor.skip(4)?; // the hash over the whole geometry
    Ok::<(), SttError>(())
}

/// A vector written as a plain count and that many fixed-size values.
fn skip_plain(cursor: &mut Cursor<'_>, each: usize) -> Result<()> {
    let n = cursor.count(each)?;
    cursor.skip(n * each).map_err(Into::into)
}

/// `n` `MbString`s, each a count of UTF-16 units then the units. The
/// count belongs to the collection around them rather than to the
/// strings, so it is passed in.
fn skip_strings(cursor: &mut Cursor<'_>, n: usize) -> Result<()> {
    for _ in 0..n {
        skip_plain(cursor, 2)?;
    }
    Ok(())
}

/// Read the tag the originating system gave each face.
///
/// The attribute section carries the B-rep attributes the Parasolid data
/// held (specification figure H.20). The face tags are the first thing
/// in it that `pmix` needs, and the body attributes before them have to
/// be stepped over to get there. Every count here is either zero or the
/// number of entities, which is what makes a wrong offset show up
/// immediately rather than as plausible numbers.
fn read_face_tags(cursor: &mut Cursor<'_>) -> Result<Vec<u32>> {
    // Body attributes, which have to be stepped over to reach the faces.
    // Two of these fields are not in the specification's figure and are
    // not named here either; they are stepped over because the file
    // writes them, and the landing point is checked below.
    if cursor.word()? > 0 {
        cursor.packet(Predictor::None)?; // the identifier of each body
    }
    // Two checksum blocks, where the figure shows one. Faces have two as
    // well, an exact one and a relaxed one, so this is most likely the
    // same pair.
    for _ in 0..2 {
        let n = cursor.word()? as usize;
        if n > 0 {
            cursor.skip(n * 16)?; // the checksums
            cursor.packet(Predictor::None)?; // whether each is valid
        }
    }
    cursor.word()?; // unnamed
    cursor.packet(Predictor::Lag1)?; // where each body's monikers start
    let n = cursor.word()? as usize;
    if n > 0 {
        cursor.skip(n * 4)?; // the identifier of each moniker's GUID
        cursor.skip(n * 16)?; // the GUIDs
        skip_strings(cursor, n)?; // the application each came from
    }
    let n = cursor.word()? as usize;
    if n > 0 {
        cursor.skip(n * 8)?; // a version number per body
        skip_strings(cursor, n)?;
    }
    cursor.packet(Predictor::None)?; // unnamed

    // Face attributes. The identifiers are ordered as face groups are,
    // so the nth is the tag of the face in face group n.
    let n = cursor.word()? as usize;
    if n == 0 {
        return Ok(Vec::new());
    }
    cursor.packet_u32(Predictor::None).map_err(Into::into)
}

/// Take three values from an array laid end to end.
fn triple(v: &[f64], at: usize) -> Option<[f64; 3]> {
    Some([*v.get(at)?, *v.get(at + 1)?, *v.get(at + 2)?])
}

/// Read the surfaces the geometry describes, each with the position of
/// the face it belongs to.
///
/// Each surface takes what it needs from four arrays laid end to end,
/// in the order the surfaces appear: every kind takes a location and two
/// directions, and the kinds with curvature take radii and angles as
/// well (specification figure H.14).
fn read_surfaces(cursor: &mut Cursor<'_>) -> Result<Vec<(usize, Surface)>> {
    let index = cursor.packet_u32(Predictor::Lag1)?;
    // The type values run one above the enumeration the specification
    // lists, so a plane is written as 1 rather than 0. The counts prove
    // it: with the documented values the radius and angle arrays are far
    // too short for the surfaces they would have to describe, and with
    // these they match exactly. The per-face kinds settle it too, since
    // those use the documented values for the very same faces.
    let kinds = cursor.packet_u32(Predictor::None)?;
    let coordinates = cursor.floats()?;
    let axes = cursor.floats()?;
    let radii = cursor.floats()?;
    let radians = cursor.floats()?;

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
            1 => Surface::Plane {
                location,
                axis: direction,
            },
            2 | 4 => {
                let Some(r) = take_radius() else { break };
                if *kind == 2 {
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
            3 => {
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
            5 => {
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

/// Read the curves the geometry describes, each with the position of the
/// edge it belongs to (specification figure H.16).
///
/// A line takes one direction where a circle and an ellipse take two, so
/// the axis array is not a fixed stride per curve.
fn read_curves(cursor: &mut Cursor<'_>) -> Result<Vec<(usize, Curve)>> {
    let index = cursor.packet_u32(Predictor::Lag1)?;
    // As with the surfaces the written values are not the documented
    // ones, and here they are not even a constant apart: a line is 1, a
    // circle 2, and an ellipse 4. The arrays say so, since only this
    // reading consumes the axis and radius arrays exactly, and the
    // per-edge kinds agree face for face.
    let kinds = cursor.packet_u32(Predictor::None)?;
    let coordinates = cursor.floats()?;
    let axes = cursor.floats()?;
    let radii = cursor.floats()?;
    // The parametric domain bounds the curve rather than shaping it.
    let _domain = cursor.floats()?;

    let (mut coordinate, mut axis, mut radius) = (0, 0, 0);
    let mut out = Vec::with_capacity(kinds.len().min(1 << 16));
    for (position, kind) in kinds.iter().enumerate() {
        let (Some(location), Some(direction)) =
            (triple(&coordinates, coordinate), triple(&axes, axis))
        else {
            break;
        };
        coordinate += 3;
        let mut take_radius = || {
            let v = radii.get(radius).copied();
            radius += 1;
            v
        };
        let curve = match kind {
            1 => {
                axis += 3;
                Curve::Line {
                    point: location,
                    direction,
                }
            }
            2 => {
                axis += 6;
                let Some(r) = take_radius() else { break };
                Curve::Circle {
                    centre: location,
                    axis: direction,
                    radius: r,
                }
            }
            4 => {
                axis += 6;
                let (Some(major), Some(minor)) = (take_radius(), take_radius()) else {
                    break;
                };
                Curve::Ellipse {
                    centre: location,
                    axis: direction,
                    major_radius: major,
                    minor_radius: minor,
                }
            }
            _ => break,
        };
        out.push((
            index.get(position).copied().unwrap_or(position as u32) as usize,
            curve,
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
        assert!(t.loops.is_empty());
        assert!(t.edges.is_empty());
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

    #[test]
    fn a_start_index_becomes_the_range_each_owner_covers() {
        // Three owners over seven members, the middle one owning none.
        assert_eq!(ranges(&[0, 3, 3], 7), [0..3, 3..3, 3..7]);
        // A start index past the end cannot make a range that would be
        // sliced with.
        assert_eq!(ranges(&[0, 9], 4), [0..4, 4..4]);
        assert_eq!(ranges(&[], 4), Vec::<Range<usize>>::new());
    }
}
