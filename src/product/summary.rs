//! What a part is, gathered from all three documents (ADR 0014).
//!
//! The product document says which parts a file holds, the features
//! document says how big each body is, and the PMI document carries what
//! the file states about them as named properties. A sourcing question —
//! *what is this part, and what else would do?* — needs all three, and
//! needs them in one place rather than as three documents to join.
//!
//! **Promotion, and why it is careful.** A file states a material under
//! whatever key its author chose. Promoting those keys into named fields
//! is what turns a heap of properties into something searchable, and it
//! is also where a reader is most tempted to guess. Two rules keep it
//! honest:
//!
//! - **Nothing is lost.** A promoted attribute names the key it came
//!   from, and a key that matched nothing stays exactly where it was in
//!   the PMI document. Promotion is a view, never an edit.
//! - **Ambiguity is stated, not resolved.** Where two keys both look
//!   like the same field — a solid volume and a bounding-box volume both
//!   read as "volume" — neither is promoted and both are named. Picking
//!   one would be a guess, and a guess about a number is worse than no
//!   number.

use serde::{Deserialize, Serialize};

use crate::features::{Envelope, FeatureDocument};
use crate::model::{PmiDocument, Property, PropertyValue};

use super::model::ProductDocument;

/// The fields worth lifting out of a file's own properties, and the
/// names a writer may have used for each.
///
/// These are ordinary English words for the thing, not keys copied from
/// any file: matching is by what a name *says*, so that a table tuned to
/// one exporter does not quietly become the only one that works.
const FIELDS: &[(&str, &[&str])] = &[
    (
        "material",
        &["material", "matl", "werkstoff", "material name"],
    ),
    ("mass", &["mass", "weight"]),
    ("volume", &["volume"]),
    ("surface_area", &["surface area", "area"]),
    ("density", &["density"]),
    (
        "part_number",
        &["part number", "part no", "partnumber", "drawing number"],
    ),
    ("revision", &["revision", "rev", "version"]),
    (
        "finish",
        &[
            "finish",
            "surface finish",
            "coating",
            "plating",
            "treatment",
        ],
    ),
    ("supplier", &["supplier", "vendor", "manufacturer"]),
    ("project", &["project", "program", "programme"]),
];

/// Everything known about one part.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PartSummary {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    /// How many times the file uses this part.
    pub occurrences: usize,
    /// The shapes it is made of.
    pub bodies: Vec<BodySummary>,
    /// Values lifted out of the file's own properties.
    pub attributes: Vec<Attribute>,
    /// Fields a key looked like but that more than one key claimed, so
    /// none was promoted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ambiguous: Vec<Ambiguity>,
    /// How many of this part's properties were not promoted. They are
    /// all still in the PMI document; this says how much is there.
    pub other_properties: usize,
}

/// One shape the part is made of.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BodySummary {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub envelope: Option<Envelope>,
    /// What kind of shape it is, where a rule settles it. This is the
    /// vocabulary a search needs: "a turned steel shaft" is something a
    /// catalogue holds, "a body with 47 faces" is not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape_class: Option<crate::features::ShapeClass>,
    /// What its faces lie on.
    pub surfaces: crate::features::Surfaces,
    /// How many features of each kind, by the kind's name.
    pub features: std::collections::BTreeMap<String, usize>,
    /// Faces no rule claimed. Stated so that *not recognised* is not
    /// read as *not there*.
    pub unassigned_faces: usize,
    /// Arrangements of those features: the bolt circles, rows and grids
    /// a substitute part would have to match.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub patterns: Vec<crate::features::Pattern>,
}

/// A value lifted out of a property, with where it came from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Attribute {
    /// The name this reader gives the field: `material`, `mass`, and so
    /// on.
    pub field: String,
    pub value: PropertyValue,
    /// The key the file used. Kept so that the promotion can be checked
    /// rather than trusted.
    pub from: String,
    /// The unit, where the key named one — `Mass (g)` states grams.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}

/// A field more than one key claimed, with values that disagree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ambiguity {
    pub field: String,
    /// Every key that looked like this field, and what each said. None
    /// was promoted; deciding between them is not this reader's to make.
    pub candidates: Vec<Candidate>,
}

/// One key's claim on a field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Candidate {
    pub key: String,
    pub value: PropertyValue,
}

/// Gather what is known about every part.
///
/// `pmi` and `shapes` may be `None`, in which case the summary states
/// what the product document alone knows. That is not a degraded mode:
/// a JT exported without precise geometry has no bodies to report, and
/// saying so beats refusing to answer.
pub fn summarise(
    product: &ProductDocument,
    pmi: Option<&PmiDocument>,
    shapes: Option<&FeatureDocument>,
) -> Vec<PartSummary> {
    product
        .parts
        .iter()
        .map(|part| {
            let bodies = part
                .bodies
                .iter()
                .filter_map(|id| {
                    let body = shapes?.bodies.iter().find(|b| &b.id == id)?;
                    let mut features = std::collections::BTreeMap::new();
                    for f in &body.features {
                        *features.entry(f.kind.name().to_owned()).or_insert(0) += 1;
                    }
                    Some(BodySummary {
                        id: body.id.clone(),
                        name: body.name.clone(),
                        envelope: body.envelope.clone(),
                        shape_class: body.shape_class,
                        surfaces: body.surfaces.clone(),
                        features,
                        unassigned_faces: body.faces.unassigned,
                        patterns: body.patterns.clone(),
                    })
                })
                .collect();

            let mine: Vec<&Property> = match pmi {
                Some(doc) => doc
                    .properties
                    .iter()
                    .filter(|p| belongs_to(p, part, product))
                    .collect(),
                None => Vec::new(),
            };
            let (attributes, ambiguous, promoted) = promote(&mine);

            PartSummary {
                id: part.id.clone(),
                number: part.number.clone(),
                name: part.name.clone(),
                description: part.description.clone(),
                revision: part.revision.clone(),
                occurrences: part.occurrences,
                bodies,
                attributes,
                ambiguous,
                other_properties: mine.len() - promoted,
            }
        })
        .collect()
}

/// Whether a property is about this part.
///
/// A property names the part that states it by the name the PMI reader
/// resolved (ADR 0007): a product's name, or the name of the occurrence
/// it was attached to. Both are matched, and in a file holding one part
/// everything product-level belongs to it.
fn belongs_to(property: &Property, part: &super::model::Part, product: &ProductDocument) -> bool {
    // Properties about a record of the model are about that record, not
    // about the part.
    if property.applies_to.is_some() {
        return false;
    }
    let Some(named) = property.part.as_deref() else {
        // A file holding one part need not say which part states a
        // property, and the PMI reader leaves it out when it cannot tell
        // them apart. One part means no ambiguity to resolve.
        return product.parts.len() == 1;
    };
    if part.name.as_deref() == Some(named) || part.number.as_deref() == Some(named) {
        return true;
    }
    // Or the name of an occurrence of this part, which is what the PMI
    // reader uses to keep two uses of one product apart.
    product
        .relations
        .iter()
        .any(|r| r.child == part.id && r.name.as_deref() == Some(named))
}

/// Lift what can be lifted, and say what could not be settled.
///
/// Returns the promoted attributes, the fields more than one key
/// claimed, and how many properties went into either.
fn promote(properties: &[&Property]) -> (Vec<Attribute>, Vec<Ambiguity>, usize) {
    let mut attributes = Vec::new();
    let mut ambiguous = Vec::new();
    let mut used = 0usize;

    for (field, names) in FIELDS {
        // An exact name beats one merely containing it: a writer that
        // says `Material` means the material, whatever else on the part
        // happens to have the word in its key.
        let (exact, loose): (Vec<&&Property>, Vec<&&Property>) = properties
            .iter()
            .filter(|p| names.iter().any(|n| mentions(&p.name, n)))
            .partition(|p| names.iter().any(|n| is_exactly(&p.name, n)));

        let candidates = if exact.is_empty() { &loose } else { &exact };
        if candidates.is_empty() {
            continue;
        }
        used += candidates.len();

        // Several keys claiming a field is only a problem when they
        // disagree. An assembly states the same material on every
        // occurrence of a part, and reporting that as a conflict would
        // bury the real ones.
        let mut values: Vec<String> = candidates.iter().map(|p| p.value.canonical()).collect();
        values.sort();
        values.dedup();

        if values.len() == 1 {
            let p = candidates[0];
            attributes.push(Attribute {
                field: (*field).to_owned(),
                value: p.value.clone(),
                from: p.name.clone(),
                unit: unit_in(&p.name),
            });
        } else {
            // They do disagree. Picking one would be a guess, and a
            // guess about a material or a mass is worse than none, so
            // all of them are named and none is promoted.
            let mut all: Vec<Candidate> = candidates
                .iter()
                .map(|p| Candidate {
                    key: p.name.clone(),
                    value: p.value.clone(),
                })
                .collect();
            all.sort_by(|a, b| {
                (a.key.as_str(), a.value.canonical()).cmp(&(b.key.as_str(), b.value.canonical()))
            });
            all.dedup();
            ambiguous.push(Ambiguity {
                field: (*field).to_owned(),
                candidates: all,
            });
        }
    }

    attributes.sort_by(|a, b| a.field.cmp(&b.field));
    ambiguous.sort_by(|a, b| a.field.cmp(&b.field));
    (attributes, ambiguous, used)
}

/// A key with its case, punctuation and trailing unit taken off, so that
/// `Part_Number`, `part number` and `Part Number (PN)` compare as one.
fn normalise(key: &str) -> String {
    let body = match key.split_once('(') {
        Some((before, _)) => before,
        None => key,
    };
    let mut out = String::with_capacity(body.len());
    let mut space = false;
    for c in body.chars() {
        if c.is_alphanumeric() {
            space = false;
            out.extend(c.to_lowercase());
        } else if !out.is_empty() && !space {
            space = true;
            out.push(' ');
        }
    }
    out.trim_end().to_owned()
}

/// Whether a key is exactly this name.
fn is_exactly(key: &str, name: &str) -> bool {
    normalise(key) == name
}

/// Whether a key says this name somewhere in it, as whole words.
fn mentions(key: &str, name: &str) -> bool {
    let key = normalise(key);
    if key == name {
        return true;
    }
    let words: Vec<&str> = key.split(' ').collect();
    let wanted: Vec<&str> = name.split(' ').collect();
    words.windows(wanted.len().max(1)).any(|w| w == wanted)
}

/// The unit a key names in brackets, if it names one.
fn unit_in(key: &str) -> Option<String> {
    let (_, rest) = key.split_once('(')?;
    let inside = rest.split_once(')')?.0.trim();
    (!inside.is_empty() && inside.len() <= 12).then(|| inside.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn property(name: &str, value: &str) -> Property {
        Property {
            id: format!("prop:{name}"),
            name: name.to_owned(),
            category: None,
            kind: crate::model::PropertyKind::User,
            value: PropertyValue::from_text(value),
            part: None,
            applies_to: None,
            unmapped: Vec::new(),
            source_refs: Vec::new(),
        }
    }

    #[test]
    fn a_key_is_matched_however_it_is_punctuated() {
        assert!(is_exactly("Part_Number", "part number"));
        assert!(is_exactly("  part   number  ", "part number"));
        assert!(is_exactly("Mass (g)", "mass"));
        assert!(!is_exactly("Bounding Box Volume", "volume"));
        assert!(mentions("Total Solid Volume (mm3)", "volume"));
        assert!(!mentions("Volumetric Ratio", "volume"));
    }

    #[test]
    fn a_key_naming_its_unit_keeps_it() {
        assert_eq!(unit_in("Mass (g)").as_deref(), Some("g"));
        assert_eq!(unit_in("Volume (mm3)").as_deref(), Some("mm3"));
        assert_eq!(unit_in("Material"), None);
    }

    #[test]
    fn one_key_for_a_field_is_promoted() {
        let props = [property("Material", "6061-T6")];
        let refs: Vec<&Property> = props.iter().collect();
        let (attributes, ambiguous, used) = promote(&refs);
        assert_eq!(used, 1);
        assert!(ambiguous.is_empty());
        assert_eq!(attributes[0].field, "material");
        assert_eq!(attributes[0].from, "Material");
    }

    #[test]
    fn an_exact_key_beats_one_that_merely_mentions_the_field() {
        let props = [
            property("Bounding Box Volume", "1200"),
            property("Volume", "840"),
        ];
        let refs: Vec<&Property> = props.iter().collect();
        let (attributes, ambiguous, _) = promote(&refs);
        assert!(ambiguous.is_empty(), "{ambiguous:?}");
        assert_eq!(attributes[0].from, "Volume");
    }

    /// Two keys, neither exact, both looking like the same field. A
    /// guess about a number is worse than no number.
    #[test]
    fn two_loose_keys_promote_nothing_and_say_so() {
        let props = [
            property("Bounding Box Volume", "1200"),
            property("Total Solid Volume", "840"),
        ];
        let refs: Vec<&Property> = props.iter().collect();
        let (attributes, ambiguous, used) = promote(&refs);
        assert!(attributes.is_empty());
        assert_eq!(used, 2);
        assert_eq!(ambiguous[0].field, "volume");
        assert_eq!(ambiguous[0].candidates.len(), 2);
    }

    /// Two occurrences of one part state the same material. That is
    /// repetition, not disagreement, and the field promotes.
    #[test]
    fn keys_that_agree_are_not_a_conflict() {
        let props = [
            property("Material", "6061-T6"),
            property("Material", "6061-T6"),
        ];
        let refs: Vec<&Property> = props.iter().collect();
        let (attributes, ambiguous, _) = promote(&refs);
        assert!(ambiguous.is_empty(), "{ambiguous:?}");
        assert_eq!(attributes.len(), 1);
        assert_eq!(attributes[0].field, "material");
    }

    /// Two occurrences that state different materials are a real
    /// conflict, and both are shown rather than one being chosen.
    #[test]
    fn keys_that_disagree_promote_nothing_and_show_both() {
        let props = [
            property("Material", "6061-T6"),
            property("Material", "SYN-STEEL-4"),
        ];
        let refs: Vec<&Property> = props.iter().collect();
        let (attributes, ambiguous, _) = promote(&refs);
        assert!(attributes.is_empty());
        assert_eq!(ambiguous[0].candidates.len(), 2);
    }

    #[test]
    fn a_key_nothing_claims_is_left_alone() {
        let props = [property("Sprocket Alignment Code", "ZX-9")];
        let refs: Vec<&Property> = props.iter().collect();
        let (attributes, ambiguous, used) = promote(&refs);
        assert!(attributes.is_empty());
        assert!(ambiguous.is_empty());
        assert_eq!(used, 0, "an unmatched key is not consumed");
    }
}
