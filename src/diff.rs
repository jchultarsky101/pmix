//! Comparing two documents (ADR 0005).
//!
//! Records match by id (ADR 0004); matched records are compared field by
//! field on their JSON form, with the fields that describe the extraction
//! rather than the design excluded.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::{Layer, PmiDocument};

/// Schema version of the diff report.
pub const DIFF_SCHEMA_VERSION: u32 = 1;

/// The result of comparing two documents.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiffReport {
    pub schema_version: u32,
    /// File name of the left (baseline) document.
    pub left: String,
    /// File name of the right document.
    pub right: String,
    pub changes: Vec<Change>,
    pub summary: Summary,
}

/// One record that differs between the documents.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Change {
    pub layer: Layer,
    /// `tolerances`, `dimensions`, `annotations`, ...
    pub collection: String,
    pub id: String,
    pub kind: ChangeKind,
    /// Human label: a tolerance's text, a datum's letter, a view's name.
    pub label: String,
    /// Changed fields; empty for added and removed records.
    pub fields: Vec<FieldChange>,
    /// How the two records were paired. Omitted when they were paired by
    /// id, which is how a record should be paired.
    #[serde(default, skip_serializing_if = "MatchKind::is_id")]
    pub matched: MatchKind,
}

/// What paired two records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchKind {
    /// The two share an identity key (ADR 0004).
    #[default]
    Id,
    /// No id paired them, but they name the same property once the two
    /// formats' ways of stating its group are put in the same shape.
    /// A weaker statement than an id: it says the readers disagree about
    /// how to name this property, not that the design changed.
    Name,
}

impl MatchKind {
    fn is_id(&self) -> bool {
        matches!(self, Self::Id)
    }
}

/// How a record differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Added,
    Removed,
    Changed,
}

/// One field that differs on a matched record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldChange {
    /// Path such as `value.value` or `compartments[1].datums[0].modifiers`.
    pub path: String,
    pub left: Value,
    pub right: Value,
}

/// Counts.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Summary {
    pub unchanged: usize,
    pub changed: usize,
    pub added: usize,
    pub removed: usize,
    pub left_unknown: usize,
    pub right_unknown: usize,
    pub left_diagnostics: usize,
    pub right_diagnostics: usize,
}

impl DiffReport {
    /// `true` when the documents are equivalent under the comparison rules.
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    /// Human-readable rendering.
    pub fn render_text(&self) -> String {
        self.to_string()
    }
}

/// The collections compared: `(section, collection, layer, label keys)`.
/// An empty section means the collection is at the top level.
const COLLECTIONS: &[(&str, &str, Layer, &[&str])] = &[
    ("", "properties", Layer::Properties, &["name"]),
    ("semantic", "features", Layer::Semantic, &["name", "kind"]),
    ("semantic", "datums", Layer::Semantic, &["label"]),
    ("semantic", "datum_systems", Layer::Semantic, &["text"]),
    (
        "semantic",
        "dimensions",
        Layer::Semantic,
        &["text", "subtype"],
    ),
    ("semantic", "tolerances", Layer::Semantic, &["text", "kind"]),
    ("semantic", "notes", Layer::Semantic, &["text"]),
    ("semantic", "other", Layer::Semantic, &["kind"]),
    (
        "presentation",
        "annotations",
        Layer::Presentation,
        &["text", "kind"],
    ),
    ("presentation", "views", Layer::Presentation, &["name"]),
];

/// Fields excluded from comparison everywhere (ADR 0005).
const EXCLUDED_KEYS: &[&str] = &[
    "source_refs",
    "source_ref",
    "hash",
    "bbox",
    "polylines",
    "vertices",
    "triangles",
];

/// Fields excluded only on annotations: the vendor label, and the
/// tessellation summary, which changes with the exporting CAD version.
const EXCLUDED_ANNOTATION_KEYS: &[&str] = &["label", "geometry", "parts"];

/// Fields excluded only on features: the vendor name, and the B-rep
/// decomposition (kind, face list, members), which changes when a
/// re-export splits or merges faces. Identity already anchors the feature.
const EXCLUDED_FEATURE_KEYS: &[&str] = &["name", "kind", "geometry", "members"];

/// Id lists, compared as sets.
const ID_LIST_KEYS: &[&str] = &[
    "features",
    "semantic",
    "presentation",
    "views",
    "annotations",
    "members",
];

/// Compare `left` (the baseline) with `right`.
pub fn diff(left: &PmiDocument, right: &PmiDocument) -> DiffReport {
    let l = serde_json::to_value(left).unwrap_or_default();
    let r = serde_json::to_value(right).unwrap_or_default();
    let mut changes = Vec::new();
    let mut summary = Summary {
        left_unknown: left.unknown.len(),
        right_unknown: right.unknown.len(),
        left_diagnostics: left.diagnostics.len(),
        right_diagnostics: right.diagnostics.len(),
        ..Default::default()
    };

    // Units, as a single pseudo record.
    let mut fields = Vec::new();
    diff_value("", &l["units"], &r["units"], "units", &mut fields);
    if !fields.is_empty() {
        summary.changed += 1;
        changes.push(Change {
            layer: Layer::Semantic,
            collection: "units".into(),
            id: "units".into(),
            kind: ChangeKind::Changed,
            label: "units".into(),
            fields,
            matched: MatchKind::Id,
        });
    } else {
        summary.unchanged += 1;
    }

    for (section, collection, layer, label_keys) in COLLECTIONS {
        let pick = |doc: &Value| -> Value {
            let v = if section.is_empty() {
                &doc[collection]
            } else {
                &doc[section][collection]
            };
            // Validation properties are derived from the PMI they describe,
            // so comparing them would restate every PMI change (ADR 0007).
            if *collection == "properties" {
                let kept: Vec<Value> = v
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter(|p| p.get("kind").and_then(Value::as_str) != Some("validation"))
                            .cloned()
                            .collect()
                    })
                    .unwrap_or_default();
                Value::Array(kept)
            } else {
                v.clone()
            }
        };
        let (lv, rv) = (pick(&l), pick(&r));
        let lm = by_id(&lv);
        let rm = by_id(&rv);
        // Where no id pairs two properties, they may still be the same
        // property named differently by the two readers.
        let mut handled: BTreeSet<String> = BTreeSet::new();
        if *collection == "properties" {
            for (left_id, right_id) in pair_by_name(&lm, &rm) {
                let (Some(a), Some(b)) = (lm.get(&left_id), rm.get(&right_id)) else {
                    continue;
                };
                handled.insert(left_id.clone());
                handled.insert(right_id);
                let mut fields = Vec::new();
                diff_value("", a, b, collection, &mut fields);
                if fields.is_empty() {
                    summary.unchanged += 1;
                } else {
                    summary.changed += 1;
                }
                // Reported even when nothing else differs, because the
                // two readers disagreed about the name and that is worth
                // seeing.
                changes.push(Change {
                    layer: *layer,
                    collection: (*collection).into(),
                    id: left_id,
                    kind: ChangeKind::Changed,
                    label: label_of(a, label_keys),
                    fields,
                    matched: MatchKind::Name,
                });
            }
        }
        let ids: BTreeSet<&String> = lm.keys().chain(rm.keys()).collect();
        for id in ids {
            if handled.contains(id) {
                continue;
            }
            match (lm.get(id), rm.get(id)) {
                (Some(a), None) => {
                    summary.removed += 1;
                    changes.push(Change {
                        layer: *layer,
                        collection: (*collection).into(),
                        id: id.clone(),
                        kind: ChangeKind::Removed,
                        label: label_of(a, label_keys),
                        fields: Vec::new(),
                        matched: MatchKind::Id,
                    });
                }
                (None, Some(b)) => {
                    summary.added += 1;
                    changes.push(Change {
                        layer: *layer,
                        collection: (*collection).into(),
                        id: id.clone(),
                        kind: ChangeKind::Added,
                        label: label_of(b, label_keys),
                        fields: Vec::new(),
                        matched: MatchKind::Id,
                    });
                }
                (Some(a), Some(b)) => {
                    let mut fields = Vec::new();
                    diff_value("", a, b, collection, &mut fields);
                    if fields.is_empty() {
                        summary.unchanged += 1;
                    } else {
                        summary.changed += 1;
                        changes.push(Change {
                            layer: *layer,
                            collection: (*collection).into(),
                            id: id.clone(),
                            kind: ChangeKind::Changed,
                            label: label_of(a, label_keys),
                            fields,
                            matched: MatchKind::Id,
                        });
                    }
                }
                (None, None) => {}
            }
        }
    }

    DiffReport {
        schema_version: DIFF_SCHEMA_VERSION,
        left: left.source.file_name.clone(),
        right: right.source.file_name.clone(),
        changes,
        summary,
    }
}

/// One way of shaping a record into a key that a weaker pairing uses.
type Shape<'a> = Box<dyn Fn(&Value) -> Option<String> + 'a>;

/// Pair properties that no id pairs but that name the same thing.
///
/// The two readers state a property's group differently: a STEP file
/// keeps it in `category`, and a JT file folds it into the name, so
/// `Assembly Metadata` + `Analysis Software` and
/// `Assembly Metadata/Analysis Software` are one property with two ids.
/// Folding the category into the name puts both in the same shape; where
/// even that does not meet, the name after the last separator does.
///
/// A pairing is made only where the weaker key names exactly one record
/// on each side, so this never chooses between candidates. It never
/// displaces a match by id either, because only records that no id
/// paired are offered to it.
fn pair_by_name(
    lm: &BTreeMap<String, &Value>,
    rm: &BTreeMap<String, &Value>,
) -> Vec<(String, String)> {
    let text = |rec: &Value, key: &str| -> String {
        rec.get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    // A separator no property name can contain, so the part and the
    // name cannot run together into a false pairing.
    const SEP: char = '\u{1}';
    let folded = |rec: &Value, scoped: bool| -> Option<String> {
        let name = rec.get("name").and_then(Value::as_str)?;
        let category = text(rec, "category");
        let mut key = if scoped {
            text(rec, "part")
        } else {
            String::new()
        };
        key.push(SEP);
        if !category.is_empty() {
            key.push_str(&category);
            key.push('/');
        }
        key.push_str(name);
        Some(key)
    };
    let tail = |rec: &Value, scoped: bool| -> Option<String> {
        let full = folded(rec, scoped)?;
        let (scope, name) = full.split_once(SEP)?;
        let mut key = scope.to_owned();
        key.push(SEP);
        key.push_str(name.rsplit('/').next().unwrap_or(name));
        Some(key)
    };
    // Weakest last, and the part only while it helps. Two formats name a
    // part differently as well as a property, so requiring the part to
    // agree would pair nothing between them; the rule that a key must
    // name exactly one record on each side is what keeps that safe.
    let shapes: [Shape<'_>; 3] = [
        Box::new(move |rec| folded(rec, true)),
        Box::new(move |rec| folded(rec, false)),
        Box::new(move |rec| tail(rec, false)),
    ];

    let mut left: BTreeSet<&String> = lm.keys().filter(|k| !rm.contains_key(*k)).collect();
    let mut right: BTreeSet<&String> = rm.keys().filter(|k| !lm.contains_key(*k)).collect();
    let mut out = Vec::new();
    for shape in &shapes {
        // A key that names more than one record on either side settles
        // nothing, so it is dropped rather than guessed between.
        let index = |ids: &BTreeSet<&String>, m: &BTreeMap<String, &Value>| {
            let mut seen: BTreeMap<String, Option<String>> = BTreeMap::new();
            for id in ids {
                let Some(key) = m.get(*id).and_then(|rec| shape(rec)) else {
                    continue;
                };
                seen.entry(key)
                    .and_modify(|slot| *slot = None)
                    .or_insert_with(|| Some((*id).clone()));
            }
            seen
        };
        let (li, ri) = (index(&left, lm), index(&right, rm));
        for (key, a) in li {
            let (Some(a), Some(Some(b))) = (a, ri.get(&key)) else {
                continue;
            };
            left.remove(&a);
            right.remove(b);
            out.push((a, b.clone()));
        }
    }
    out.sort();
    out
}

fn by_id(v: &Value) -> BTreeMap<String, &Value> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|rec| {
                    rec.get("id")
                        .and_then(Value::as_str)
                        .map(|id| (id.to_owned(), rec))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn label_of(rec: &Value, keys: &[&str]) -> String {
    keys.iter()
        .filter_map(|k| rec.get(*k).and_then(Value::as_str))
        .find(|s| !s.is_empty())
        .map(str::to_owned)
        .unwrap_or_default()
}

fn excluded(key: &str, collection: &str) -> bool {
    EXCLUDED_KEYS.contains(&key)
        || (collection == "annotations" && EXCLUDED_ANNOTATION_KEYS.contains(&key))
        || (collection == "features" && EXCLUDED_FEATURE_KEYS.contains(&key))
}

fn join(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_owned()
    } else {
        format!("{path}.{key}")
    }
}

/// Recursive comparison. `collection` selects collection-specific
/// exclusions; `path` is the field path so far.
fn diff_value(path: &str, l: &Value, r: &Value, collection: &str, out: &mut Vec<FieldChange>) {
    match (l, r) {
        (Value::Object(lo), Value::Object(ro)) => {
            let keys: BTreeSet<&String> = lo.keys().chain(ro.keys()).collect();
            for key in keys {
                if key == "id" && path.is_empty() {
                    continue;
                }
                if excluded(key, collection) {
                    continue;
                }
                let p = join(path, key);
                match (lo.get(key), ro.get(key)) {
                    (Some(a), Some(b)) => {
                        if ID_LIST_KEYS.contains(&key.as_str()) && a.is_array() && b.is_array() {
                            let sa: BTreeSet<String> = a
                                .as_array()
                                .unwrap()
                                .iter()
                                .map(ToString::to_string)
                                .collect();
                            let sb: BTreeSet<String> = b
                                .as_array()
                                .unwrap()
                                .iter()
                                .map(ToString::to_string)
                                .collect();
                            if sa != sb {
                                out.push(FieldChange {
                                    path: p,
                                    left: a.clone(),
                                    right: b.clone(),
                                });
                            }
                        } else {
                            diff_value(&p, a, b, collection, out);
                        }
                    }
                    (Some(a), None) => out.push(FieldChange {
                        path: p,
                        left: a.clone(),
                        right: Value::Null,
                    }),
                    (None, Some(b)) => out.push(FieldChange {
                        path: p,
                        left: Value::Null,
                        right: b.clone(),
                    }),
                    (None, None) => {}
                }
            }
        }
        (Value::Array(la), Value::Array(ra)) => {
            if la.len() != ra.len() {
                out.push(FieldChange {
                    path: path.to_owned(),
                    left: l.clone(),
                    right: r.clone(),
                });
                return;
            }
            for (i, (a, b)) in la.iter().zip(ra.iter()).enumerate() {
                diff_value(&format!("{path}[{i}]"), a, b, collection, out);
            }
        }
        _ => {
            if !values_equal(l, r) {
                out.push(FieldChange {
                    path: path.to_owned(),
                    left: l.clone(),
                    right: r.clone(),
                });
            }
        }
    }
}

/// Numbers are equal within 1e-6 relative (1e-6 absolute near zero):
/// re-exports print the same value with different precision.
fn values_equal(l: &Value, r: &Value) -> bool {
    match (l.as_f64(), r.as_f64()) {
        (Some(a), Some(b)) => (a - b).abs() <= 1e-6 * a.abs().max(b.abs()).max(1.0),
        _ => l == r,
    }
}

fn short(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => "∅".into(),
        other => other.to_string(),
    }
}

impl fmt::Display for DiffReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "left:  {}", self.left)?;
        writeln!(f, "right: {}", self.right)?;
        for layer in [Layer::Properties, Layer::Semantic, Layer::Presentation] {
            let changes: Vec<&Change> = self.changes.iter().filter(|c| c.layer == layer).collect();
            if changes.is_empty() {
                continue;
            }
            writeln!(f)?;
            writeln!(
                f,
                "{}",
                match layer {
                    Layer::Properties => "properties",
                    Layer::Semantic => "semantic",
                    Layer::Presentation => "presentation",
                }
            )?;
            for c in changes {
                let mark = match c.kind {
                    ChangeKind::Added => '+',
                    ChangeKind::Removed => '-',
                    ChangeKind::Changed => '~',
                };
                let how = if c.matched.is_id() {
                    ""
                } else {
                    "  (matched by name)"
                };
                if c.label.is_empty() {
                    writeln!(f, "  {mark} {}{how}", c.id)?;
                } else {
                    writeln!(f, "  {mark} {}  {}{how}", c.id, c.label)?;
                }
                for fc in &c.fields {
                    writeln!(
                        f,
                        "      {}: {} → {}",
                        fc.path,
                        short(&fc.left),
                        short(&fc.right)
                    )?;
                }
            }
        }
        let s = &self.summary;
        writeln!(f)?;
        writeln!(
            f,
            "summary: {} unchanged, {} changed, {} removed, {} added",
            s.unchanged, s.changed, s.removed, s.added
        )?;
        if s.left_unknown != s.right_unknown || s.left_diagnostics != s.right_diagnostics {
            writeln!(
                f,
                "note: unknown entries {} → {}, diagnostics {} → {} (not compared)",
                s.left_unknown, s.right_unknown, s.left_diagnostics, s.right_diagnostics
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_property_named_differently_by_the_two_readers_is_one_property() {
        // A STEP file keeps the group in `category`; a JT file folds it
        // into the name. Same property, two ids.
        let step = serde_json::json!([
            {"id":"prop:a","name":"Analysis Software","category":"Assembly Metadata",
             "part":"widget","value":{"type":"text","value":"CadQuery"}}
        ]);
        let jt = serde_json::json!([
            {"id":"prop:b","name":"Assembly Metadata/Analysis Software",
             "part":"widget.asm;0;0:","value":{"type":"text","value":"CadQuery"}}
        ]);
        let paired = pair_by_name(&by_id(&step), &by_id(&jt));
        assert_eq!(paired, [("prop:a".to_owned(), "prop:b".to_owned())]);
    }

    #[test]
    fn a_name_that_two_records_share_pairs_nothing() {
        // Two components state the same property; nothing says which of
        // them the one on the other side is, so neither is paired.
        let left = serde_json::json!([
            {"id":"prop:a","name":"Mass","part":"one","value":1},
            {"id":"prop:b","name":"Mass","part":"two","value":2}
        ]);
        let right = serde_json::json!([{"id":"prop:c","name":"Mass","part":"other","value":1}]);
        assert!(pair_by_name(&by_id(&left), &by_id(&right)).is_empty());
    }

    #[test]
    fn a_record_that_an_id_already_pairs_is_left_alone() {
        let both = serde_json::json!([{"id":"prop:a","name":"Mass","part":"one","value":1}]);
        assert!(pair_by_name(&by_id(&both), &by_id(&both)).is_empty());
    }

    #[test]
    fn a_weaker_pairing_says_so_in_the_report() {
        let change = Change {
            layer: Layer::Properties,
            collection: "properties".into(),
            id: "prop:a".into(),
            kind: ChangeKind::Changed,
            label: "Mass".into(),
            fields: Vec::new(),
            matched: MatchKind::Name,
        };
        let report = DiffReport {
            schema_version: DIFF_SCHEMA_VERSION,
            left: "a".into(),
            right: "b".into(),
            changes: vec![change],
            summary: Summary::default(),
        };
        assert!(report.render_text().contains("matched by name"));
        // A pairing by id says nothing, so existing reports read as before.
        let json = serde_json::to_string(&report.changes[0]).unwrap();
        assert!(json.contains("\"matched\":\"name\""), "{json}");
        let plain = Change {
            matched: MatchKind::Id,
            ..report.changes[0].clone()
        };
        assert!(!serde_json::to_string(&plain).unwrap().contains("matched"));
    }

    #[test]
    fn nested_paths_and_id_sets() {
        let mut out = Vec::new();
        let a = serde_json::json!({"id":"x","value":{"value":0.1,"unit":"mm"},"features":["b","a"],"source_refs":["#1"],"modifiers":["m"]});
        let b = serde_json::json!({"id":"x","value":{"value":0.2,"unit":"mm"},"features":["a","b"],"source_refs":["#9"],"modifiers":["m","n"]});
        diff_value("", &a, &b, "tolerances", &mut out);
        let paths: Vec<&str> = out.iter().map(|c| c.path.as_str()).collect();
        assert_eq!(paths, ["modifiers", "value.value"]);
    }

    #[test]
    fn floats_compare_with_tolerance() {
        assert!(values_equal(
            &serde_json::json!(0.1),
            &serde_json::json!(0.1 + 1e-15)
        ));
        assert!(!values_equal(
            &serde_json::json!(0.1),
            &serde_json::json!(0.2)
        ));
    }
}
