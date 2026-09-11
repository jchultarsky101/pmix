//! The logical scene graph's node hierarchy (specification section 5.3).
//!
//! This is JT's answer to what a STEP file says with product definitions
//! and assembly usages: which parts the file holds, and how they are
//! arranged (ADR 0014). Until now only the scene graph's *property
//! table* was read — enough to learn a part's name and which segment
//! belongs to it — and the graph itself was skipped.
//!
//! The shape is small. Every node begins with the same base data, and
//! each kind adds to it:
//!
//! ```text
//! Base node   version, flags, attribute ids
//! Group       base + version + child ids
//! Meta data   group + version
//! Part        meta data + version + an empty field
//! Instance    base + version + one child id
//! Partition   group + flags + file name + bounding box + counts
//! ```
//!
//! **The reading is checked against the element's own length.** This
//! format's figures have been wrong before — six times, each costing
//! byte-level debugging — so a node whose fields do not fit the bytes
//! the file gave it is reported rather than believed. A graph that read
//! half its nodes says so; it does not pass off half an assembly as a
//! whole one.

use std::collections::BTreeMap;

use super::element::Elements;
use super::file::Guid;

/// Object type identifiers, from the specification's section 5.3 and
/// 5.4.7. Stored as the raw little-endian bytes, as every other
/// identifier in this reader is.
pub const PARTITION_NODE: Guid = Guid([
    0x3e, 0x10, 0xdd, 0x10, 0xc8, 0x2a, 0xd1, 0x11, 0x9b, 0x6b, 0x00, 0x80, 0xc7, 0xbb, 0x59, 0x97,
]);
pub const GROUP_NODE: Guid = Guid([
    0x1b, 0x10, 0xdd, 0x10, 0xc8, 0x2a, 0xd1, 0x11, 0x9b, 0x6b, 0x00, 0x80, 0xc7, 0xbb, 0x59, 0x97,
]);
pub const INSTANCE_NODE: Guid = Guid([
    0x2a, 0x10, 0xdd, 0x10, 0xc8, 0x2a, 0xd1, 0x11, 0x9b, 0x6b, 0x00, 0x80, 0xc7, 0xbb, 0x59, 0x97,
]);
pub const PART_NODE: Guid = Guid([
    0x44, 0x72, 0x35, 0xce, 0xfb, 0x38, 0xd1, 0x11, 0xa5, 0x06, 0x00, 0x60, 0x97, 0xbd, 0xc6, 0xe1,
]);
pub const META_DATA_NODE: Guid = Guid([
    0x45, 0x72, 0x35, 0xce, 0xfb, 0x38, 0xd1, 0x11, 0xa5, 0x06, 0x00, 0x60, 0x97, 0xbd, 0xc6, 0xe1,
]);
pub const GEOMETRIC_TRANSFORM: Guid = Guid([
    0x83, 0x10, 0xdd, 0x10, 0xc8, 0x2a, 0xd1, 0x11, 0x9b, 0x6b, 0x00, 0x80, 0xc7, 0xbb, 0x59, 0x97,
]);

/// What a node is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    /// The root of a file's graph.
    Partition,
    /// An ordered list of children, and nothing else.
    Group,
    /// One use of another node: JT's occurrence.
    Instance,
    /// The root of one part.
    Part,
    /// A group carrying late-loaded properties.
    MetaData,
}

impl NodeKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Partition => "partition",
            Self::Group => "group",
            Self::Instance => "instance",
            Self::Part => "part",
            Self::MetaData => "meta data",
        }
    }
}

/// One node of the graph.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub id: i32,
    pub kind: NodeKind,
    /// The nodes beneath this one, in the order the file lists them.
    pub children: Vec<i32>,
    /// The attribute objects attached to this node, by their ids.
    pub attributes: Vec<i32>,
    /// The bounding box a partition node states, in the file's own
    /// units: the low corner and the high corner.
    pub bbox: Option<([f32; 3], [f32; 3])>,
}

/// The graph a scene graph segment describes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Graph {
    pub nodes: BTreeMap<i32, Node>,
    /// The 4×4 transform each geometric transform attribute states, by
    /// the attribute's object id, in row-major order.
    pub transforms: BTreeMap<i32, [f64; 16]>,
    /// What could not be read, said out loud.
    pub diagnostics: Vec<String>,
}

impl Graph {
    /// The nodes nothing references: the graph's roots.
    pub fn roots(&self) -> Vec<i32> {
        let mut used: Vec<i32> = self
            .nodes
            .values()
            .flat_map(|n| n.children.iter().copied())
            .collect();
        used.sort_unstable();
        used.dedup();
        self.nodes
            .keys()
            .copied()
            .filter(|id| used.binary_search(id).is_err())
            .collect()
    }

    /// The transform attached to a node, if it has exactly one.
    ///
    /// A node carrying two transforms is a node this cannot place, and
    /// saying so beats multiplying them in an order the file does not
    /// state.
    pub fn transform_of(&self, node: i32) -> Option<&[f64; 16]> {
        let node = self.nodes.get(&node)?;
        let mut found = node
            .attributes
            .iter()
            .filter_map(|a| self.transforms.get(a));
        let first = found.next()?;
        found.next().is_none().then_some(first)
    }
}

/// A cursor that refuses to read past the bytes it was given.
struct Cursor<'a> {
    data: &'a [u8],
    at: usize,
    /// How wide a local version number is: two bytes before JT 10, one
    /// from JT 10 on. The same difference the property atoms have
    /// (ADR 0009).
    version: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8], major: u32) -> Self {
        Self {
            data,
            at: 0,
            version: if major >= 10 { 1 } else { 2 },
        }
    }

    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let out = self.data.get(self.at..self.at + n)?;
        self.at += n;
        Some(out)
    }

    fn version(&mut self) -> Option<()> {
        self.take(self.version).map(|_| ())
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn i32(&mut self) -> Option<i32> {
        Some(i32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn f32(&mut self) -> Option<f32> {
        Some(f32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn f64(&mut self) -> Option<f64> {
        Some(f64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }

    /// A count, refused when it could not fit in the bytes that remain.
    ///
    /// A misread offset turns the next four bytes into a count of
    /// millions, and allocating for it is how a reader stops being a
    /// reader. The remaining length is the bound the file itself gives.
    fn count(&mut self, each: usize) -> Option<usize> {
        let n = self.i32()?;
        let n = usize::try_from(n).ok()?;
        (n.checked_mul(each)? <= self.data.len() - self.at).then_some(n)
    }

    /// Base node data: version, flags, and the attributes attached.
    fn base(&mut self) -> Option<Vec<i32>> {
        self.version()?;
        let _flags = self.u32()?;
        let count = self.count(4)?;
        (0..count).map(|_| self.i32()).collect()
    }

    /// Group node data: base node data, then the children.
    fn group(&mut self) -> Option<(Vec<i32>, Vec<i32>)> {
        let attributes = self.base()?;
        self.version()?;
        let count = self.count(4)?;
        let children: Vec<i32> = (0..count).map(|_| self.i32()).collect::<Option<_>>()?;
        Some((attributes, children))
    }
}

/// Read the node hierarchy of a decoded scene graph payload.
pub fn read(payload: &[u8], major: u32) -> Graph {
    let mut graph = Graph::default();
    let mut unreadable: BTreeMap<String, usize> = BTreeMap::new();

    for element in Elements::new(payload) {
        let kind = match element.object_type {
            PARTITION_NODE => NodeKind::Partition,
            GROUP_NODE => NodeKind::Group,
            INSTANCE_NODE => NodeKind::Instance,
            PART_NODE => NodeKind::Part,
            META_DATA_NODE => NodeKind::MetaData,
            GEOMETRIC_TRANSFORM => {
                if let Some(matrix) = transform(element.data, major) {
                    graph.transforms.insert(element.object_id, matrix);
                } else {
                    *unreadable
                        .entry("geometric transform".to_owned())
                        .or_default() += 1;
                }
                continue;
            }
            // Attributes and shape nodes this does not need. They are
            // not a failure: the graph is the hierarchy, not everything
            // hanging off it.
            _ => continue,
        };

        match node(kind, element.data, major, element.object_id) {
            Some(n) => {
                graph.nodes.insert(n.id, n);
            }
            None => *unreadable.entry(kind.name().to_owned()).or_default() += 1,
        }
    }

    for (what, count) in unreadable {
        graph.diagnostics.push(format!(
            "{count} {what} element(s) did not fit the layout this reader expects, so that part \
             of the scene graph is not stated"
        ));
    }
    graph
}

/// One node, or nothing when its fields do not fit its bytes.
fn node(kind: NodeKind, data: &[u8], major: u32, id: i32) -> Option<Node> {
    let mut c = Cursor::new(data, major);
    let (attributes, children, bbox) = match kind {
        NodeKind::Instance => {
            let attributes = c.base()?;
            c.version()?;
            let child = c.i32()?;
            (attributes, vec![child], None)
        }
        NodeKind::Group => {
            let (attributes, children) = c.group()?;
            (attributes, children, None)
        }
        NodeKind::MetaData => {
            let (attributes, children) = c.group()?;
            c.version()?;
            (attributes, children, None)
        }
        NodeKind::Part => {
            let (attributes, children) = c.group()?;
            // Meta data node data, then the part's own version and the
            // empty field the specification reserves.
            c.version()?;
            c.version()?;
            let _empty = c.i32()?;
            (attributes, children, None)
        }
        NodeKind::Partition => {
            let (attributes, children) = c.group()?;
            // A version number the specification's figure 23 does not
            // show, between the children and the partition flags. It is
            // there in the bytes: without it the file name's character
            // count reads as 6656 instead of 26, and the flags read as
            // 0x101 instead of the 1 that says a bounding box follows.
            // The seventh place this format disagrees with its own
            // figures (ADR 0009).
            c.version()?;
            let flags = c.i32()?;
            // The file name is an MbString: a character count, then that
            // many UTF-16 code units.
            let chars = c.count(2)?;
            c.take(chars * 2)?;
            // The low bit of the flags says whether the box was written.
            let bbox = (flags & 1 != 0)
                .then(|| {
                    let mut v = [0f32; 6];
                    for slot in v.iter_mut() {
                        *slot = c.f32()?;
                    }
                    Some(([v[0], v[1], v[2]], [v[3], v[4], v[5]]))
                })
                .flatten();
            (attributes, children, bbox)
        }
    };
    Some(Node {
        id,
        kind,
        children,
        attributes,
        bbox,
    })
}

/// The 4×4 matrix a geometric transform attribute states.
///
/// The attribute begins with base attribute data, whose length the
/// specification gives as a version byte and three fields — and which
/// changed at JT 10.6, in a format whose figures have been wrong before.
/// So rather than trusting one offset, this tries the plausible ones and
/// takes the reading whose stored-values mask accounts for exactly the
/// bytes the element holds. A wrong offset does not produce a matrix
/// that fits; it produces one that does not.
fn transform(data: &[u8], major: u32) -> Option<[f64; 16]> {
    // Base attribute data: a version number, state flags, and either one
    // or two flag words depending on the version of the format.
    let version = if major >= 10 { 1 } else { 2 };
    for base in [version + 1 + 4 + 4, version + 1 + 4] {
        let start = base + version;
        let Some(mask_bytes) = data.get(start..start + 2) else {
            continue;
        };
        let mask = u16::from_le_bytes(mask_bytes.try_into().ok()?);
        let stored = mask.count_ones() as usize;
        if stored > 16 {
            continue;
        }
        // A mask storing nothing is the identity written the short way,
        // which is a transform and not a failure to read one.
        if stored == 0 {
            return Some(IDENTITY);
        }
        // The values follow the mask, and nothing of this element comes
        // after them but the optional V2 fields.
        if data.len() < start + 2 + stored * 8 {
            continue;
        }
        let mut c = Cursor::new(&data[start + 2..], major);
        // The mask is read from its top bit down, one bit per element of
        // the matrix in row-major order; a bit that is clear means the
        // identity's value for that element.
        let mut matrix = IDENTITY;
        let mut bits = mask;
        for slot in matrix.iter_mut() {
            if bits & 0x8000 != 0 {
                *slot = c.f64()?;
            }
            bits <<= 1;
        }
        return Some(matrix);
    }
    None
}

const IDENTITY: [f64; 16] = [
    1.0, 0.0, 0.0, 0.0, //
    0.0, 1.0, 0.0, 0.0, //
    0.0, 0.0, 1.0, 0.0, //
    0.0, 0.0, 0.0, 1.0,
];
