//! The B-rep faces a JT callout applies to, as feature records.
//!
//! A PMI association names a face by a tag; the topology table says
//! which face that is and what it lies on; and [`crate::fingerprint`]
//! turns that into the key a feature is identified by. The recipe is the
//! STEP reader's, so a feature built here and a feature built from a
//! STEP file describing the same face carry the same id.
//!
//! What cannot be claimed is that this has been seen to happen. No model
//! is published in both formats, so the two readers agree by sharing one
//! recipe rather than by having been checked against a matched pair.

use std::collections::{BTreeMap, HashMap};

use crate::fingerprint::single_feature_key;
use crate::model::{
    Feature, FeatureKind, GeometryKind, GeometryRef, Meta, SurfaceKind, content_hash,
};

use super::pmi::PmiManager;
use super::stt::{self, Topology};

/// The faces the callouts of a file apply to.
#[derive(Debug, Default)]
pub struct Anchors {
    /// One feature per face any callout names, ready for the document.
    pub features: Vec<Feature>,
    /// The features each entity names, by the element it is in and its
    /// position there.
    pub by_entity: BTreeMap<(usize, usize), Vec<String>>,
}

/// The id a face's fingerprint gives it.
///
/// This is the STEP reader's formula, not a parallel one: a feature is
/// its key hashed, and a feature that is one face has no key of its own
/// beyond the face's.
fn feature_id(key: &str) -> String {
    format!("feat:{}", content_hash([single_feature_key(key).as_str()]))
}

/// The name this reader gives the kind of surface, in the shared model's
/// vocabulary, so a JT feature reads like a STEP one.
fn surface_kind(kind: stt::SurfaceKind) -> Option<SurfaceKind> {
    Some(match kind {
        stt::SurfaceKind::Plane => SurfaceKind::Plane,
        stt::SurfaceKind::Cylinder => SurfaceKind::Cylinder,
        stt::SurfaceKind::Cone => SurfaceKind::Cone,
        stt::SurfaceKind::Sphere => SurfaceKind::Sphere,
        stt::SurfaceKind::Torus => SurfaceKind::Torus,
        stt::SurfaceKind::Other(_) => return None,
    })
}

/// A feature that is one piece of a part's B-rep and nothing else.
fn one(
    id: &str,
    kind: FeatureKind,
    geometry: GeometryKind,
    tag: u32,
    surface: Option<SurfaceKind>,
) -> Feature {
    let source_ref = format!("{geometry}#{tag}");
    Feature {
        meta: Meta {
            id: id.to_owned(),
            source_refs: vec![source_ref.clone()],
            ..Default::default()
        },
        kind,
        name: None,
        geometry: vec![GeometryRef {
            kind: geometry,
            surface,
            source_ref,
        }],
        members: Vec::new(),
        count: None,
    }
}

/// Work out which faces and edges each callout applies to.
///
/// `per_metre` converts the topology table's metres into the unit the
/// model declares, because a fingerprint is in model units.
pub fn build(managers: &[PmiManager], topologies: &[Topology], per_metre: f64) -> Anchors {
    // A tag names a face across the whole file, which is what lets an
    // assembly-level callout reach into a part. A tag two parts both
    // claim names neither: resolving it would be a guess.
    let mut faces: HashMap<u32, Option<(usize, usize)>> = HashMap::new();
    let mut edges: HashMap<u32, Option<(usize, usize)>> = HashMap::new();
    for (t, topology) in topologies.iter().enumerate() {
        let note = |map: &mut HashMap<u32, Option<(usize, usize)>>, tag, at| {
            map.entry(tag)
                .and_modify(|found| *found = None)
                .or_insert(Some((t, at)));
        };
        for (f, face) in topology.faces.iter().enumerate() {
            if let Some(tag) = face.tag {
                note(&mut faces, tag, f);
            }
        }
        for (e, edge) in topology.edges.iter().enumerate() {
            if let Some(tag) = edge.tag {
                note(&mut edges, tag, e);
            }
        }
    }

    let mut out = Anchors::default();
    let mut made: BTreeMap<String, Feature> = BTreeMap::new();
    for (m, manager) in managers.iter().enumerate() {
        for e in 0..manager.entities.len() {
            let mut ids = Vec::new();
            for tag in manager.faces_of(e) {
                let Some(Some((t, f))) = faces.get(&tag).copied() else {
                    continue;
                };
                let topology = &topologies[t];
                let face = &topology.faces[f];
                let Some(key) = topology.fingerprint(face, per_metre) else {
                    continue;
                };
                let id = feature_id(&key);
                made.entry(id.clone()).or_insert_with(|| {
                    one(
                        &id,
                        FeatureKind::Face,
                        GeometryKind::Face,
                        tag,
                        surface_kind(face.surface_kind),
                    )
                });
                ids.push(id);
            }
            for tag in manager.edges_of(e) {
                let Some(Some((t, x))) = edges.get(&tag).copied() else {
                    continue;
                };
                let topology = &topologies[t];
                let edge = &topology.edges[x];
                let Some(key) = topology.edge_fingerprint(edge, per_metre) else {
                    continue;
                };
                let id = feature_id(&key);
                made.entry(id.clone())
                    .or_insert_with(|| one(&id, FeatureKind::Edge, GeometryKind::Edge, tag, None));
                ids.push(id);
            }
            ids.sort();
            ids.dedup();
            if !ids.is_empty() {
                out.by_entity.insert((m, e), ids);
            }
        }
    }
    out.features = made.into_values().collect();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_feature_that_is_one_face_is_keyed_by_that_face_alone() {
        // The STEP reader keys a feature as kind, geometry, span; a lone
        // face states nothing for the first or the last.
        assert_eq!(single_feature_key("plane/0,0,1/5"), "|plane/0,0,1/5|");
        let id = feature_id("plane/0,0,1/5");
        assert!(id.starts_with("feat:"), "{id}");
        // The same face gives the same id, a different face a different
        // one; nothing else about the file enters into it.
        assert_eq!(id, feature_id("plane/0,0,1/5"));
        assert_ne!(id, feature_id("plane/0,0,1/6"));
    }

    #[test]
    fn a_surface_with_no_closed_form_names_no_kind() {
        assert_eq!(
            surface_kind(stt::SurfaceKind::Cylinder),
            Some(SurfaceKind::Cylinder)
        );
        assert_eq!(surface_kind(stt::SurfaceKind::Other(8)), None);
    }
}
