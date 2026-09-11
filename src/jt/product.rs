//! A JT file's product structure (ADR 0014).
//!
//! The same document the STEP reader produces, built from the scene
//! graph's node hierarchy instead of from product definitions and
//! assembly usages. The two formats say the same thing in different
//! words:
//!
//! | STEP | JT |
//! | --- | --- |
//! | `PRODUCT_DEFINITION` | a part node |
//! | `NEXT_ASSEMBLY_USAGE_OCCURRENCE` | an instance node |
//! | `ITEM_DEFINED_TRANSFORMATION` | a geometric transform attribute |
//!
//! An instance node references the node it is an occurrence of, and sits
//! somewhere beneath the part that contains it. So a relation is found
//! by looking *down* from the instance for the part it uses and *up* for
//! the part that uses it, through however many groups the writer put in
//! between — JT has no rule that a part's children are its direct
//! children, and real files nest them several deep.

use std::collections::BTreeMap;

use crate::features::Units;
use crate::model::{ContentId, Placement, Source};
use crate::product::model::{Diagnostic, Part, ProductDocument, Relation, SCHEMA_VERSION};

use super::file::{Jt, SegmentKind};
use super::lsg::{Graph, NodeKind};
use super::property::{self, Properties};

/// How deep to look through groups for the part on either side of an
/// instance. Real files nest a few levels; a limit keeps a graph with a
/// cycle in it from being a hang rather than a diagnostic.
const REACH: usize = 16;

/// What the scene graph's own numbers are in.
///
/// Not metres. The topology table states geometry in metres, which is
/// JT's base unit and what the feature reader converts from (ADR 0010),
/// but the scene graph's transforms and bounding boxes are in the unit
/// the file declares — millimetres in every file seen so far. Reading
/// them as metres puts a 148mm assembly 22 metres from the origin.
fn scale_of(declared: Option<&str>) -> f64 {
    crate::fingerprint::Scale::of(declared, None).length
}

/// Build the product document for a parsed JT file.
pub fn document(jt: &Jt<'_>, source: Source) -> ProductDocument {
    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    let mut graph = Graph::default();
    let mut scene = Properties::default();

    for segment in jt
        .segments()
        .iter()
        .filter(|s| s.kind == SegmentKind::LogicalSceneGraph)
    {
        match jt.segment_data(segment) {
            Ok(data) => {
                let read = super::lsg::read(&data, jt.header.major);
                graph.nodes.extend(read.nodes);
                graph.transforms.extend(read.transforms);
                diagnostics.extend(
                    read.diagnostics
                        .into_iter()
                        .map(|m| Diagnostic { message: m }),
                );
                let props = property::read(&data, jt.header.major);
                scene.by_element.extend(props.by_element);
                scene.segments.extend(props.segments);
            }
            Err(e) => diagnostics.push(Diagnostic {
                message: format!("scene graph segment could not be read: {e}"),
            }),
        }
    }

    let declared = scene.find("JT_PROP_MEASUREMENT_UNITS");
    let declared_length = declared.and_then(property::unit_name).map(str::to_owned);
    let scale = scale_of(declared_length.as_deref());

    let (mut parts, relations) = build(&graph, &scene, scale, &mut diagnostics);

    // Each body under the part that holds it. A part node points at its
    // own topology segment through a late-loaded property, so the
    // mapping is the one the file already states — no walk needed, which
    // is the one thing easier here than in STEP (ADR 0014).
    let mut unattached = Vec::new();
    for (owner, body) in bodies(jt, &scene, &mut diagnostics) {
        let under = owner
            .and_then(|node| part_id_of(&graph, &scene, node, &parts))
            .and_then(|id| parts.iter_mut().find(|p| p.id == id));
        match under {
            Some(part) => part.bodies.push(body),
            None => unattached.push(crate::product::model::Unattached {
                body,
                reason: "no part node points at the topology segment this shape was read from"
                    .into(),
            }),
        }
    }

    // A partition node states the box around everything beneath it.
    // Reading it beats computing one, and it is there even in a file
    // exported without precise geometry, where there is no body to
    // measure (ADR 0014).
    let envelope = graph.nodes.values().find_map(|n| n.bbox).map(|(lo, hi)| {
        let at = |v: [f32; 3]| {
            [
                v[0] as f64 * scale,
                v[1] as f64 * scale,
                v[2] as f64 * scale,
            ]
        };
        let (min, max) = (at(lo), at(hi));
        let mut size = [max[0] - min[0], max[1] - min[1], max[2] - min[2]];
        size.sort_by(|a, b| b.total_cmp(a));
        crate::features::Envelope {
            min,
            max,
            size,
            // Stated by the file rather than derived from what it
            // locates, so there is nothing left unbounded.
            approximate: false,
        }
    });

    if parts.is_empty() {
        diagnostics.push(Diagnostic {
            message: "the scene graph states no part node, so this file names no part".into(),
        });
    }

    let mut doc = ProductDocument {
        schema_version: SCHEMA_VERSION,
        source,
        envelope,
        units: Units {
            length: "mm".into(),
            angle: "deg".into(),
            declared_length,
            declared_angle: None,
        },
        parts,
        relations,
        unattached,
        roots: Vec::new(),
        diagnostics,
    };
    doc.settle();
    doc
}

/// Every body the file holds, with the node that points at it.
///
/// Read the same way [`crate::features::from_jt`] reads them, and in the
/// same order, so that a body has one id whichever document names it.
fn bodies(
    jt: &Jt<'_>,
    scene: &Properties,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<(Option<i32>, String)> {
    use super::stt;
    use crate::features::brep::Solid;

    let mut solids: Vec<Solid> = Vec::new();
    let mut owners: Vec<Option<i32>> = Vec::new();
    for segment in jt.segments().iter().filter(|s| s.kind == SegmentKind::Stt) {
        let Ok(data) = jt.segment_data(segment) else {
            diagnostics.push(Diagnostic {
                message: format!("topology segment {} could not be read", segment.id),
            });
            continue;
        };
        for element in crate::jt::Elements::new(&data) {
            if element.object_type != stt::STT_ELEMENT {
                continue;
            }
            if let Ok(t) = stt::parse(element.data) {
                solids.push(crate::features::jt::solid(&t));
                owners.push(scene.owner_of(segment.id));
            }
        }
    }

    crate::features::recognise_in_order(&solids)
        .into_iter()
        .zip(owners)
        .map(|(body, owner)| (owner, body.id))
        .collect()
}

/// The part a node belongs to: itself when it is a part node, or the
/// part above it.
fn part_id_of(graph: &Graph, scene: &Properties, node: i32, parts: &[Part]) -> Option<String> {
    let n = graph.nodes.get(&node)?;
    let at = if n.kind == NodeKind::Part || n.kind == NodeKind::Partition {
        node
    } else {
        let mut parent: BTreeMap<i32, i32> = BTreeMap::new();
        for node in graph.nodes.values() {
            for child in &node.children {
                parent.entry(*child).or_insert(node.id);
            }
        }
        part_above(graph, &parent, node, REACH)?
    };
    let name = part_name(graph, scene, at)?;
    parts
        .iter()
        .find(|p| p.number.as_deref() == Some(name.as_str()))
        .map(|p| p.id.clone())
}

/// The parts and the occurrences the graph describes.
fn build(
    graph: &Graph,
    scene: &Properties,
    scale: f64,
    diagnostics: &mut Vec<Diagnostic>,
) -> (Vec<Part>, Vec<Relation>) {
    let mut ids = ContentId::new();

    // Which node holds each node, so an instance can be traced back to
    // the part that contains it.
    let mut parent: BTreeMap<i32, i32> = BTreeMap::new();
    for node in graph.nodes.values() {
        for child in &node.children {
            parent.entry(*child).or_insert(node.id);
        }
    }

    // One part per part node.
    let mut parts: Vec<Part> = Vec::new();
    let mut part_of: BTreeMap<i32, String> = BTreeMap::new();
    for node in graph
        .nodes
        .values()
        .filter(|n| n.kind == NodeKind::Part || n.kind == NodeKind::Partition)
    {
        let name = part_name(graph, scene, node.id);
        // A part keys on what a person would call it, as it does in a
        // STEP file (ADR 0004). JT states one name, not a number and a
        // revision, so that name is the whole key.
        let key = name.clone().unwrap_or_default();
        let id = ids.make("part", &[&key]);
        part_of.insert(node.id, id.clone());
        parts.push(Part {
            id,
            number: name.clone(),
            name: None,
            description: None,
            revision: node_property(scene, node.id, "JT_PROP_REVISION"),
            occurrences: 0,
            bodies: Vec::new(),
            source_refs: vec![format!("node {}", node.id)],
        });
    }

    let mut anonymous = 0usize;
    for part in &parts {
        if part.number.is_none() {
            anonymous += 1;
        }
    }
    if anonymous > 1 {
        diagnostics.push(Diagnostic {
            message: format!(
                "{anonymous} part nodes state no name, so they are told apart only by the order \
                 the file lists them in and their ids will not match another export of the same \
                 design"
            ),
        });
    }

    // One relation per instance node: what it uses, and who uses it.
    let mut relations = Vec::new();
    for node in graph
        .nodes
        .values()
        .filter(|n| n.kind == NodeKind::Instance)
    {
        let Some(child) = node
            .children
            .first()
            .and_then(|c| part_below(graph, *c, REACH))
        else {
            continue;
        };
        let Some(owner) = part_above(graph, &parent, node.id, REACH) else {
            continue;
        };
        let (Some(parent_id), Some(child_id)) = (part_of.get(&owner), part_of.get(&child)) else {
            continue;
        };

        let placement = placement_of(graph, node.id, scale);
        let key = format!(
            "{parent_id};{child_id};{}",
            placement
                .as_ref()
                .map(|p| crate::identity::triple(p.origin, crate::fingerprint::identity_quantum()))
                .unwrap_or_default()
        );
        relations.push(Relation {
            id: ids.make("use", &[&key]),
            parent: parent_id.clone(),
            child: child_id.clone(),
            name: node_name(scene, node.id),
            placement,
            source_refs: vec![format!("node {}", node.id)],
        });
    }

    (parts, relations)
}

/// The part node at or beneath `node`, looking through groups.
fn part_below(graph: &Graph, node: i32, reach: usize) -> Option<i32> {
    if reach == 0 {
        return None;
    }
    let n = graph.nodes.get(&node)?;
    if n.kind == NodeKind::Part {
        return Some(node);
    }
    n.children
        .iter()
        .find_map(|c| part_below(graph, *c, reach - 1))
}

/// The part node this one sits beneath.
fn part_above(graph: &Graph, parent: &BTreeMap<i32, i32>, node: i32, reach: usize) -> Option<i32> {
    let mut at = node;
    for _ in 0..reach {
        at = *parent.get(&at)?;
        let n = graph.nodes.get(&at)?;
        if n.kind == NodeKind::Part || n.kind == NodeKind::Partition {
            return Some(at);
        }
    }
    None
}

/// Where an instance puts what it references, in millimetres.
///
/// JT states a full 4×4 matrix. Only its translation is stated here:
/// a placement in this document is an origin and two directions, and
/// turning a general matrix into those is a decomposition this reader
/// does not do — so it states what it can read plainly and leaves the
/// rest rather than inventing an orientation.
fn placement_of(graph: &Graph, node: i32, scale: f64) -> Option<Placement> {
    let m = graph.transform_of(node)?;
    // Row-major, so the translation is the last row's first three.
    Some(Placement {
        origin: [m[12] * scale, m[13] * scale, m[14] * scale],
        axis: None,
        ref_direction: None,
    })
}

/// What a part is called.
///
/// A JT part node carries a great deal — its material, its surface area,
/// its centre of gravity — and not its name. The name is on the instance
/// node directly above it, the one that wraps the part into the graph,
/// together with the occurrence suffix a writer appends. The suffix is
/// dropped, because it changes between exports and a part's identity
/// must not (ADR 0004).
fn part_name(graph: &Graph, scene: &Properties, node: i32) -> Option<String> {
    let name = node_name(scene, node).or_else(|| {
        let wrapper = graph
            .nodes
            .values()
            .find(|n| n.kind == NodeKind::Instance && n.children.contains(&node))?;
        node_name(scene, wrapper.id)
    })?;
    Some(without_occurrence(&name)).filter(|n| !n.is_empty())
}

/// A name with the occurrence suffix a writer appends taken off.
///
/// NX writes `90591A141 HEX NUT.asm;23;1790:`, where everything from the
/// first semicolon is that use of the part rather than the part.
fn without_occurrence(name: &str) -> String {
    name.split(';').next().unwrap_or(name).trim().to_owned()
}

/// The name a node gives itself, if it gives one.
fn node_name(scene: &Properties, node: i32) -> Option<String> {
    ["CAD_PARTNAME::", "JT_PROP_NAME", "Name::"]
        .iter()
        .find_map(|key| node_property(scene, node, key))
}

/// One property of one node.
fn node_property(scene: &Properties, node: i32, key: &str) -> Option<String> {
    scene
        .by_element
        .get(&node)?
        .iter()
        .find(|(k, v)| k == key && !v.is_empty())
        .map(|(_, v)| v.clone())
}
