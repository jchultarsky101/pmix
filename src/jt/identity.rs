//! Finalisation pass: replace the walkers' temporary ids with identity
//! keys (ADR 0004), resolve collisions, and rewrite cross references.
//!
//! The recipes match the STEP reader's wherever the two formats state
//! the same thing. A datum is its letter, a datum reference frame is its
//! compartments, and a saved view is its name in either format, so those
//! records get the same id from a STEP file and a JT file of one design.
//!
//! Dimensions and geometric tolerances cannot yet. STEP anchors them on
//! fingerprints of the B-rep faces they apply to; JT states no features,
//! so the reader anchors them on where the annotation attaches to the
//! part instead (see [`super::semantic::anchor`]). Both survive a
//! re-export, but they are different keys, so a dimension read from STEP
//! and the same dimension read from JT do not share an id.

use std::collections::HashMap;

use crate::identity::{COARSE_QUANTUM, assign, hash_content, remap, remap_all, triple};
use crate::model::{Annotation, PmiDocument};

use super::semantic::Keys;

/// Fields that name a record rather than state its content, and so are
/// left out of the hash that orders records sharing an identity key.
const NOT_CONTENT: &[&str] = &["id", "source_refs", "presentation"];

/// Assign final ids to every record in `doc`.
pub fn finalise(doc: &mut PmiDocument, keys: &Keys) {
    let mut map: HashMap<String, String> = HashMap::new();
    // A record whose key the walker did not record keeps its temporary
    // id as the key, which is a content hash: distinct, but not stable.
    let key_of = |id: &str| keys.get(id).cloned().unwrap_or_else(|| id.to_owned());

    // 1. Datums, which everything else can reference.
    let batch = doc
        .semantic
        .datums
        .iter()
        .enumerate()
        .map(|(i, d)| (i, key_of(&d.meta.id), hash_content(d, NOT_CONTENT)))
        .collect();
    for (i, id) in assign(batch, "datum", true) {
        map.insert(doc.semantic.datums[i].meta.id.clone(), id.clone());
        doc.semantic.datums[i].meta.id = id;
    }
    let labels: HashMap<String, String> = doc
        .semantic
        .datums
        .iter()
        .map(|d| (d.meta.id.clone(), d.label.clone()))
        .collect();
    // JT states a datum reference as the letter itself rather than as a
    // link, so the reference has to be resolved to the datum record it
    // names. A part states its datums once, so the search is confined to
    // the PMI element the reference was read from, and only falls back
    // to the whole file when that finds nothing.
    let mut by_element: HashMap<(String, String), String> = HashMap::new();
    let mut by_label: HashMap<String, String> = HashMap::new();
    for d in &doc.semantic.datums {
        by_element
            .entry((element_of(&d.meta.source_refs), d.label.clone()))
            .or_insert_with(|| d.meta.id.clone());
        by_label.entry(d.label.clone()).or_insert(d.meta.id.clone());
    }

    // 2. Datum reference frames, keyed by the letters in precedence
    // order, exactly as the STEP reader keys them.
    let mut batch = Vec::new();
    for (i, s) in doc.semantic.datum_systems.iter_mut().enumerate() {
        let element = element_of(&s.meta.source_refs);
        for c in &mut s.compartments {
            for r in &mut c.datums {
                remap(&mut r.datum, &map);
                if !labels.contains_key(&r.datum) {
                    if let Some(id) = by_element
                        .get(&(element.clone(), r.datum.clone()))
                        .or_else(|| by_label.get(&r.datum))
                    {
                        r.datum = id.clone();
                    }
                }
            }
        }
        let key = s
            .compartments
            .iter()
            .map(|c| {
                c.datums
                    .iter()
                    .map(|r| labels.get(&r.datum).cloned().unwrap_or_else(|| "?".into()))
                    .collect::<Vec<_>>()
                    .join("-")
            })
            .collect::<Vec<_>>()
            .join("|");
        batch.push((i, key, hash_content(&s, NOT_CONTENT)));
    }
    for (i, id) in assign(batch, "dsys", true) {
        map.insert(doc.semantic.datum_systems[i].meta.id.clone(), id.clone());
        doc.semantic.datum_systems[i].meta.id = id;
    }

    // 3. Dimensions.
    let batch = doc
        .semantic
        .dimensions
        .iter()
        .enumerate()
        .map(|(i, d)| (i, key_of(&d.meta.id), hash_content(d, NOT_CONTENT)))
        .collect();
    for (i, id) in assign(batch, "dim", false) {
        map.insert(doc.semantic.dimensions[i].meta.id.clone(), id.clone());
        doc.semantic.dimensions[i].meta.id = id;
    }

    // 4. Geometric tolerances.
    let mut batch = Vec::new();
    for (i, t) in doc.semantic.tolerances.iter_mut().enumerate() {
        if let Some(ds) = &mut t.datum_system {
            remap(ds, &map);
        }
        batch.push((i, key_of(&t.meta.id), hash_content(&t, NOT_CONTENT)));
    }
    for (i, id) in assign(batch, "tol", false) {
        map.insert(doc.semantic.tolerances[i].meta.id.clone(), id.clone());
        doc.semantic.tolerances[i].meta.id = id;
    }

    // 5. Notes and anything the model has no record for yet.
    let batch = doc
        .semantic
        .notes
        .iter()
        .enumerate()
        .map(|(i, n)| (i, key_of(&n.meta.id), hash_content(n, NOT_CONTENT)))
        .collect();
    for (i, id) in assign(batch, "note", false) {
        map.insert(doc.semantic.notes[i].meta.id.clone(), id.clone());
        doc.semantic.notes[i].meta.id = id;
    }
    let batch = doc
        .semantic
        .other
        .iter()
        .enumerate()
        .map(|(i, o)| (i, key_of(&o.meta.id), hash_content(o, NOT_CONTENT)))
        .collect();
    for (i, id) in assign(batch, "other", false) {
        map.insert(doc.semantic.other[i].meta.id.clone(), id.clone());
        doc.semantic.other[i].meta.id = id;
    }

    // 6. Annotations, which name the semantic records they display.
    let mut batch = Vec::new();
    for (i, a) in doc.presentation.annotations.iter_mut().enumerate() {
        remap_all(&mut a.semantic, &map);
        batch.push((
            i,
            annotation_key(a),
            hash_content(&a, &["id", "source_refs", "views"]),
        ));
    }
    for (i, id) in assign(batch, "ann", false) {
        map.insert(doc.presentation.annotations[i].id.clone(), id.clone());
        doc.presentation.annotations[i].id = id;
    }

    // 7. Saved views, keyed by name as the STEP reader keys them.
    let mut batch = Vec::new();
    for (i, v) in doc.presentation.views.iter_mut().enumerate() {
        remap_all(&mut v.annotations, &map);
        batch.push((i, v.name.clone(), hash_content(&v, &["id", "source_refs"])));
    }
    for (i, id) in assign(batch, "view", true) {
        map.insert(doc.presentation.views[i].id.clone(), id.clone());
        doc.presentation.views[i].id = id;
    }
    // The annotations were named before the views were, so the views
    // they point at are still under their old names.
    for a in &mut doc.presentation.annotations {
        remap_all(&mut a.views, &map);
    }

    // 8. Properties, after everything they could be about.
    let mut batch = Vec::new();
    for (i, p) in doc.properties.iter_mut().enumerate() {
        if let Some(a) = &mut p.applies_to {
            remap(a, &map);
        }
        let key = [
            p.part.as_deref().unwrap_or(""),
            p.name.as_str(),
            p.applies_to.as_deref().unwrap_or(""),
        ]
        .join("|");
        batch.push((i, key, hash_content(&p, &["id", "source_refs"])));
    }
    for (i, id) in assign(batch, "prop", true) {
        doc.properties[i].id = id;
    }

    // Every semantic record names the annotations that display it, which
    // is the other half of the link the annotations already carry.
    let shown_by: HashMap<String, Vec<String>> =
        doc.presentation
            .annotations
            .iter()
            .fold(HashMap::new(), |mut acc, a| {
                for id in &a.semantic {
                    acc.entry(id.clone()).or_default().push(a.id.clone());
                }
                acc
            });
    for meta in metas(doc) {
        if let Some(annotations) = shown_by.get(&meta.id) {
            meta.presentation = annotations.clone();
            meta.presentation.sort();
            meta.presentation.dedup();
        }
    }

    doc.semantic.sort();
    doc.presentation.sort();
    doc.properties.sort_by(|a, b| a.id.cmp(&b.id));
}

/// Which PMI element a record was read from, taken from the reference
/// the reader wrote, so that records of one part can be matched to each
/// other rather than to the same letter on another part.
fn element_of(source_refs: &[String]) -> String {
    source_refs
        .first()
        .and_then(|r| r.split('#').next())
        .unwrap_or_default()
        .to_owned()
}

/// How an annotation is identified: by what it displays when it displays
/// something, and by where it sits when it does not. This is the STEP
/// reader's recipe, so an annotation linked to records that both formats
/// name the same way is itself named the same way.
fn annotation_key(a: &Annotation) -> String {
    if !a.semantic.is_empty() {
        return format!("{}|{}", a.semantic.join(","), a.kind.as_str());
    }
    let plane = a
        .plane
        .as_ref()
        .map(|p| triple(p.origin, COARSE_QUANTUM))
        .unwrap_or_default();
    let bbox = a
        .geometry
        .bbox
        .as_ref()
        .map(|b| {
            format!(
                "{}/{}",
                triple(b.min, COARSE_QUANTUM),
                triple(b.max, COARSE_QUANTUM)
            )
        })
        .unwrap_or_default();
    format!("{}|{plane}|{bbox}", a.kind.as_str())
}

/// Every record's shared fields, so cross references can be rewritten in
/// one place.
fn metas(doc: &mut PmiDocument) -> impl Iterator<Item = &mut crate::model::Meta> {
    let s = &mut doc.semantic;
    s.dimensions
        .iter_mut()
        .map(|d| &mut d.meta)
        .chain(s.tolerances.iter_mut().map(|t| &mut t.meta))
        .chain(s.datums.iter_mut().map(|d| &mut d.meta))
        .chain(s.datum_systems.iter_mut().map(|d| &mut d.meta))
        .chain(s.notes.iter_mut().map(|n| &mut n.meta))
        .chain(s.other.iter_mut().map(|o| &mut o.meta))
        .chain(s.features.iter_mut().map(|f| &mut f.meta))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jt::pmi::{Entity, EntityKind, PmiManager};
    use crate::model::PmiDocument;

    fn entity(kind: EntityKind, props: &[(&str, &str)]) -> Entity {
        Entity {
            kind,
            user_label: 1,
            texts: Vec::new(),
            polylines: Vec::new(),
            text_polylines: Vec::new(),
            properties: props
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
            type_name: None,
            valid: true,
        }
    }

    /// Run the whole reader pipeline over one PMI element.
    fn extract(entities: Vec<Entity>) -> PmiDocument {
        let manager = PmiManager {
            cad_tags: (0..entities.len() as i32).collect(),
            entities,
            ..Default::default()
        };
        let built = super::super::semantic::build(&[manager], Some("mm"), &mut Vec::new());
        let mut doc = PmiDocument {
            schema_version: crate::model::SCHEMA_VERSION,
            source: crate::model::Source {
                file_name: "test.jt".into(),
                format: "JT".into(),
                schema: None,
                writer: None,
                time_stamp: None,
            },
            units: crate::model::Units {
                length: Some("mm".into()),
                angle: Some("deg".into()),
            },
            properties: Vec::new(),
            semantic: built.semantic,
            presentation: Default::default(),
            unknown: Vec::new(),
            diagnostics: Vec::new(),
        };
        finalise(&mut doc, &built.keys);
        doc
    }

    fn dimension(value: &str, at: &str) -> Entity {
        entity(
            EntityKind::Dimension,
            &[
                ("type", "1"),
                ("value", value),
                ("Leader[0].terminator", at),
                ("Description", "Linear Dimension (1)"),
            ],
        )
    }

    #[test]
    fn a_dimension_keeps_its_id_when_its_value_changes() {
        let before = extract(vec![dimension("12.5", "10 0 0")]);
        let after = extract(vec![dimension("19.75", "10 0 0")]);
        assert_eq!(
            before.semantic.dimensions[0].meta.id, after.semantic.dimensions[0].meta.id,
            "the id must name the callout, not what it currently says"
        );
        // And the values really did differ, so the test is not vacuous.
        assert_ne!(
            before.semantic.dimensions[0].value,
            after.semantic.dimensions[0].value
        );
    }

    #[test]
    fn a_dimension_keeps_its_id_when_its_tolerance_or_text_changes() {
        let mut edited = dimension("12.5", "10 0 0");
        edited.properties.push(("upperDelta".into(), "0.2".into()));
        edited.properties.push(("lowerDelta".into(), "-0.2".into()));
        edited.properties.retain(|(k, _)| k != "Description");
        edited
            .properties
            .push(("Description".into(), "Linear Dimension (94)".into()));
        let plain = extract(vec![dimension("12.5", "10 0 0")]);
        let toleranced = extract(vec![edited]);
        assert!(toleranced.semantic.dimensions[0].tolerance.is_some());
        assert_eq!(
            plain.semantic.dimensions[0].meta.id,
            toleranced.semantic.dimensions[0].meta.id
        );
    }

    #[test]
    fn a_dimension_measuring_somewhere_else_is_a_different_dimension() {
        let here = extract(vec![dimension("12.5", "10 0 0")]);
        let there = extract(vec![dimension("12.5", "90 0 0")]);
        assert_ne!(
            here.semantic.dimensions[0].meta.id,
            there.semantic.dimensions[0].meta.id
        );
    }

    #[test]
    fn a_tolerance_keeps_its_id_when_its_magnitude_changes() {
        let frame = |value: &str| {
            entity(
                EntityKind::FeatureControlFrame,
                &[
                    ("characteristic", "3"),
                    ("ToleranceCompartment[0].value", value),
                    ("Leader[0].terminator", "4 5 6"),
                ],
            )
        };
        let loose = extract(vec![frame("0.5")]);
        let tight = extract(vec![frame("0.1")]);
        assert_eq!(
            loose.semantic.tolerances[0].meta.id,
            tight.semantic.tolerances[0].meta.id
        );
        // Changing the characteristic replaces the callout, so the id
        // changes with it.
        let other = extract(vec![entity(
            EntityKind::FeatureControlFrame,
            &[
                ("characteristic", "11"),
                ("ToleranceCompartment[0].value", "0.5"),
                ("Leader[0].terminator", "4 5 6"),
            ],
        )]);
        assert_ne!(
            loose.semantic.tolerances[0].meta.id,
            other.semantic.tolerances[0].meta.id
        );
    }

    #[test]
    fn a_datum_is_named_for_its_letter() {
        let doc = extract(vec![entity(
            EntityKind::DatumFeatureSymbol,
            &[
                ("label", "B"),
                ("Description", "Datum Feature Symbol B (7)"),
            ],
        )]);
        assert_eq!(doc.semantic.datums[0].meta.id, "datum:B");
    }

    #[test]
    fn a_frame_names_the_datums_it_references_in_precedence_order() {
        let doc = extract(vec![
            entity(EntityKind::DatumFeatureSymbol, &[("label", "A")]),
            entity(EntityKind::DatumFeatureSymbol, &[("label", "B")]),
            entity(
                EntityKind::FeatureControlFrame,
                &[
                    ("characteristic", "3"),
                    ("ToleranceCompartment[0].value", "0.5"),
                    (
                        "ToleranceCompartment[0].PrimaryDatum.Reference[0].label",
                        "A",
                    ),
                    (
                        "ToleranceCompartment[0].SecondaryDatum.Reference[0].label",
                        "B",
                    ),
                ],
            ),
        ]);
        assert_eq!(doc.semantic.datum_systems[0].meta.id, "dsys:A|B");
        // The frame points at the system, and the system at the datums.
        assert_eq!(
            doc.semantic.tolerances[0].datum_system.as_deref(),
            Some("dsys:A|B")
        );
        let referenced: Vec<&str> = doc.semantic.datum_systems[0]
            .compartments
            .iter()
            .flat_map(|c| c.datums.iter().map(|d| d.datum.as_str()))
            .collect();
        assert_eq!(referenced, ["datum:A", "datum:B"]);
    }

    #[test]
    fn two_identical_callouts_are_told_apart_but_stay_put() {
        let doc = extract(vec![
            dimension("12.5", "10 0 0"),
            dimension("12.5", "10 0 0"),
        ]);
        let ids: Vec<&str> = doc
            .semantic
            .dimensions
            .iter()
            .map(|d| d.meta.id.as_str())
            .collect();
        assert_eq!(ids.len(), 2);
        assert_ne!(ids[0], ids[1]);
        assert!(ids[1].ends_with("-2"), "{ids:?}");
    }
}
