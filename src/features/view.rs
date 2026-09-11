//! Narrower views of the documents, for a reader with a budget.
//!
//! A feature document describes every body and every face; a comparison
//! lists every pair. That is right for a file and wrong for a context
//! window, where a question about one hole should not cost a hundred
//! faces nobody asked about (ADR 0013). These views answer narrower
//! questions. They live here rather than in the server so that they are
//! tested, and so that the server stays what it is meant to be: a
//! transport.
//!
//! A view never silently changes what a number means. Counts on a body
//! describe the body as it was read, whatever was left out of the list
//! beneath them, and each view carries the filter that produced it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::compare::{BodyPair, Comparison};
use super::model::{Body, Feature, FeatureDocument, Kind, Units};
use crate::model::Source;

/// What a body holds, without listing it.
///
/// The first call a reader with a budget should make: which bodies are
/// there, how big each is, and what kinds of feature it has, so that the
/// next question can be a narrow one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Overview {
    pub source: Source,
    pub units: Units,
    pub bodies: Vec<BodyOverview>,
}

/// One body, counted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BodyOverview {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub faces: super::model::FaceCounts,
    /// How many features of each kind, by the kind's name.
    pub features: BTreeMap<String, usize>,
}

/// The overview of a document.
pub fn overview(doc: &FeatureDocument) -> Overview {
    Overview {
        source: doc.source.clone(),
        units: doc.units.clone(),
        bodies: doc
            .bodies
            .iter()
            .map(|b| {
                let mut features = BTreeMap::new();
                for f in &b.features {
                    *features.entry(f.kind.name().to_owned()).or_insert(0) += 1;
                }
                BodyOverview {
                    id: b.id.clone(),
                    name: b.name.clone(),
                    faces: b.faces,
                    features,
                }
            })
            .collect(),
    }
}

/// What to keep of a feature document.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Filter {
    /// Only this body, by id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// Only features of this kind. When set, the unassigned faces are
    /// left out too: a reader asking for holes did not ask for planes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<Kind>,
}

impl Filter {
    fn is_empty(&self) -> bool {
        self.body.is_none() && self.kind.is_none()
    }
}

/// A feature document with a filter applied, carrying the filter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Narrowed {
    /// What was asked for, so that a reader knows what is not here.
    pub filter: Filter,
    pub document: FeatureDocument,
}

fn keep(f: &Feature, kind: Option<Kind>) -> bool {
    kind.is_none_or(|k| f.kind == k)
}

/// The document with only what `filter` asks for.
///
/// Face counts on each body are left as they were read: they say what
/// the body is, not what this view lists. A body that `filter` names
/// but the document does not have yields an empty body list rather than
/// an error, because the id may simply belong to the other document of a
/// pair.
pub fn narrow(doc: &FeatureDocument, filter: Filter) -> Narrowed {
    if filter.is_empty() {
        return Narrowed {
            filter,
            document: doc.clone(),
        };
    }
    let bodies: Vec<Body> = doc
        .bodies
        .iter()
        .filter(|b| filter.body.as_deref().is_none_or(|id| b.id == id))
        .map(|b| Body {
            id: b.id.clone(),
            name: b.name.clone(),
            faces: b.faces,
            envelope: b.envelope.clone(),
            surfaces: b.surfaces.clone(),
            shape_class: b.shape_class,
            // Patterns are about the features as a whole, so a view
            // narrowed to one kind would misrepresent them.
            patterns: if filter.kind.is_some() {
                Vec::new()
            } else {
                b.patterns.clone()
            },
            features: b
                .features
                .iter()
                .filter(|f| keep(f, filter.kind))
                .cloned()
                .collect(),
            unassigned: if filter.kind.is_some() {
                Vec::new()
            } else {
                b.unassigned.clone()
            },
        })
        .collect();
    Narrowed {
        filter,
        document: FeatureDocument {
            schema_version: doc.schema_version,
            source: doc.source.clone(),
            units: doc.units.clone(),
            bodies,
            diagnostics: doc.diagnostics.clone(),
        },
    }
}

/// What to keep of a comparison.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CompareFilter {
    /// Only pairs involving this body id, on either side.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// Only leftovers and pairings of this kind. `matched` lists ids
    /// alone and is left as it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<Kind>,
    /// Leave out body pairs in which nothing differs. The summary still
    /// counts them.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub only_changed: bool,
}

impl CompareFilter {
    fn is_empty(&self) -> bool {
        self.body.is_none() && self.kind.is_none() && !self.only_changed
    }
}

/// A comparison with a filter applied, carrying the filter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NarrowedComparison {
    pub filter: CompareFilter,
    pub comparison: Comparison,
}

fn unchanged(p: &BodyPair) -> bool {
    p.baseline.is_some()
        && p.compared.is_some()
        && p.only_baseline.is_empty()
        && p.only_compared.is_empty()
}

/// The comparison with only what `filter` asks for.
///
/// The summary is the whole comparison's, whatever this view lists, so
/// that "3 matched, 1 only in baseline" stays true after a body pair
/// with nothing in it has been dropped. The notes stay too: a caution
/// about a field is not less needed because the field was filtered.
pub fn narrow_comparison(c: &Comparison, filter: CompareFilter) -> NarrowedComparison {
    if filter.is_empty() {
        return NarrowedComparison {
            filter,
            comparison: c.clone(),
        };
    }
    let bodies: Vec<BodyPair> = c
        .bodies
        .iter()
        .filter(|p| !(filter.only_changed && unchanged(p)))
        .filter(|p| {
            filter.body.as_deref().is_none_or(|id| {
                p.baseline.as_deref() == Some(id) || p.compared.as_deref() == Some(id)
            })
        })
        .map(|p| {
            let only_baseline: Vec<Feature> = p
                .only_baseline
                .iter()
                .filter(|f| keep(f, filter.kind))
                .cloned()
                .collect();
            let only_compared: Vec<Feature> = p
                .only_compared
                .iter()
                .filter(|f| keep(f, filter.kind))
                .cloned()
                .collect();
            let kept: std::collections::BTreeSet<&str> =
                only_baseline.iter().map(|f| f.id.as_str()).collect();
            BodyPair {
                possible_pairings: p
                    .possible_pairings
                    .iter()
                    .filter(|x| filter.kind.is_none() || kept.contains(x.baseline.as_str()))
                    .cloned()
                    .collect(),
                only_baseline,
                only_compared,
                ..p.clone()
            }
        })
        .collect();
    NarrowedComparison {
        filter,
        comparison: Comparison {
            bodies,
            ..c.clone()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::model::{FaceCounts, Shape};

    fn feature(id: &str, kind: Kind) -> Feature {
        Feature {
            id: id.into(),
            kind,
            faces: vec![format!("face:{id}")],
            shape: Shape {
                diameter: None,
                radius: None,
                depth: None,
                length: None,
                through: None,
                axis: None,
                position: None,
                extent: None,
                angle: None,
            },
            overlaps: Vec::new(),
            coaxial_with: Vec::new(),
        }
    }

    fn doc() -> FeatureDocument {
        FeatureDocument {
            schema_version: 1,
            source: Source {
                file_name: "x.stp".into(),
                format: "STEP".into(),
                schema: None,
                writer: None,
                time_stamp: None,
            },
            units: Units {
                length: "mm".into(),
                angle: "deg".into(),
                declared_length: None,
                declared_angle: None,
            },
            bodies: vec![
                Body {
                    id: "body:a".into(),
                    name: None,
                    envelope: None,
                    surfaces: Default::default(),
                    shape_class: None,
                    patterns: Vec::new(),
                    faces: FaceCounts {
                        total: 9,
                        in_features: 3,
                        unassigned: 6,
                    },
                    features: vec![
                        feature("h1", Kind::Hole),
                        feature("h2", Kind::Hole),
                        feature("r1", Kind::Round),
                    ],
                    unassigned: (0..6)
                        .map(|i| super::super::model::UnassignedFace {
                            id: format!("face:p{i}"),
                            surface: "plane".into(),
                        })
                        .collect(),
                },
                Body {
                    id: "body:b".into(),
                    name: Some("pin".into()),
                    envelope: None,
                    surfaces: Default::default(),
                    shape_class: None,
                    patterns: Vec::new(),
                    faces: FaceCounts {
                        total: 3,
                        in_features: 1,
                        unassigned: 2,
                    },
                    features: vec![feature("s1", Kind::Boss)],
                    unassigned: Vec::new(),
                },
            ],
            diagnostics: Vec::new(),
        }
    }

    #[test]
    fn an_overview_counts_without_listing() {
        let o = overview(&doc());
        assert_eq!(o.bodies.len(), 2);
        assert_eq!(o.bodies[0].features["hole"], 2);
        assert_eq!(o.bodies[0].features["round"], 1);
        assert_eq!(o.bodies[0].faces.unassigned, 6);
        assert_eq!(o.bodies[1].name.as_deref(), Some("pin"));
    }

    #[test]
    fn narrowing_to_a_kind_keeps_the_counts_honest() {
        let n = narrow(
            &doc(),
            Filter {
                body: Some("body:a".into()),
                kind: Some(Kind::Hole),
            },
        );
        assert_eq!(n.document.bodies.len(), 1);
        let b = &n.document.bodies[0];
        assert_eq!(b.features.len(), 2);
        assert!(b.features.iter().all(|f| f.kind == Kind::Hole));
        // Nobody asked for the planes.
        assert!(b.unassigned.is_empty());
        // But the body is still nine faces with six unassigned; the view
        // did not shrink the part.
        assert_eq!(b.faces.total, 9);
        assert_eq!(b.faces.unassigned, 6);
        assert_eq!(n.filter.kind, Some(Kind::Hole));
    }

    #[test]
    fn an_empty_filter_is_the_whole_document() {
        let d = doc();
        assert_eq!(narrow(&d, Filter::default()).document, d);
    }

    #[test]
    fn a_body_the_document_does_not_have_yields_nothing_not_an_error() {
        let n = narrow(
            &doc(),
            Filter {
                body: Some("body:zzz".into()),
                kind: None,
            },
        );
        assert!(n.document.bodies.is_empty());
    }
}
