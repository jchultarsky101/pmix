//! Comparing two feature documents (ADR 0012).
//!
//! What this does and does not do is the whole point of it. It pairs what
//! is provably the same, says what is left over on each side, and states
//! a displacement once where one displacement explains everything. It
//! does **not** decide that a hole moved rather than being removed with
//! another added somewhere else: one hole moved and one deleted with
//! another added are the same geometry, and nothing here can tell them
//! apart. Where two leftovers look related it says how they differ and
//! leaves the reading to whoever is reading.
//!
//! So the output is evidence, ordered by how much weight it carries:
//!
//! 1. **Matched.** Same id, so provably the same feature. Certain.
//! 2. **A placement.** Every leftover is the same shape as a counterpart
//!    and every one of them sits the same distance away, so one number
//!    explains the lot. Measured, not guessed.
//! 3. **Candidates.** Two leftovers of one kind agreeing on either size
//!    or position. An observation with its distance attached, offered so
//!    that a reader does not have to scan two lists by eye.
//! 4. **Leftovers.** Stated in full, because they are the part no rule
//!    here can account for.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::fingerprint;
use crate::identity::{num, triple};

/// A number as a reader wants it: rounded to the quantum ids are built
/// at, with no trailing zeros. `num` pads to four decimals because it
/// builds keys, and a key is not meant to be read.
fn text(v: f64) -> String {
    let q = tolerance();
    let r: f64 = num(v, q).parse().unwrap_or(v);
    format!("{r}")
}

/// Three of those.
fn point_text(p: [f64; 3]) -> String {
    format!("{},{},{}", text(p[0]), text(p[1]), text(p[2]))
}
use crate::model::Source;

use super::model::{Body, Feature, FeatureDocument};

/// Version of the comparison document, independent of the feature
/// document's.
pub const SCHEMA_VERSION: u32 = 1;

/// How close two numbers must be to count as the same, which is the
/// quantum ids are built at (ADR 0004).
fn tolerance() -> f64 {
    fingerprint::identity_quantum()
}

/// What comparing two feature documents found.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Comparison {
    pub schema_version: u32,
    /// Where the two came from. Excluded from comparison of comparisons.
    pub baseline: Source,
    pub compared: Source,
    pub bodies: Vec<BodyPair>,
    pub summary: Summary,
}

/// The counts, so a reader knows how much of the answer is certain
/// before reading any of it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Summary {
    pub matched: usize,
    pub only_baseline: usize,
    pub only_compared: usize,
    pub bodies_paired: usize,
    pub bodies_only_baseline: usize,
    pub bodies_only_compared: usize,
}

impl Summary {
    /// Whether the two documents describe the same shapes throughout.
    pub fn identical(&self) -> bool {
        self.only_baseline == 0
            && self.only_compared == 0
            && self.bodies_only_baseline == 0
            && self.bodies_only_compared == 0
    }
}

/// What paired two bodies, strongest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Paired {
    /// The same id: the two are made of the same surfaces throughout, so
    /// nothing about the body changed.
    Id,
    /// Faces in common. A body whose hole was bored gets a new id, and
    /// shares no feature with itself either, but every face the edit did
    /// not touch is still the same face. That is what pairs anything
    /// that changed, and the count says how much it rests on.
    Faces,
    /// The same features, none of them in the same place. A body that
    /// moved shares no id with itself, so nothing above this can pair
    /// it; what is left is that both are made of the same shapes.
    Shapes,
    /// Only the name the file gave it. The weakest of the three: a name
    /// is a label a user can change without touching the shape.
    Name,
}

/// Two bodies set against each other, or one with no counterpart.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BodyPair {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compared: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// What paired them; absent when there was nothing to pair with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paired: Option<Paired>,
    /// How many faces the pairing rested on, for [`Paired::Faces`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paired_on: Option<usize>,
    /// Features present in both, by id. Provably the same features.
    pub matched: Vec<String>,
    /// Features in the baseline with no counterpart, stated in full.
    pub only_baseline: Vec<Feature>,
    /// Features in the compared document with no counterpart.
    pub only_compared: Vec<Feature>,
    /// Leftovers that look related, with how they differ. Advisory: a
    /// candidate is an observation, never a conclusion.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<Candidate>,
    /// One displacement carrying every leftover onto its counterpart,
    /// where one does. Stated once here rather than repeated on every
    /// feature in the body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<[f64; 3]>,
    /// Set when every leftover is the same shape as a counterpart but no
    /// single displacement explains where they sit. Something moved, and
    /// this refuses to say what: a rotation, a different origin, and
    /// several independent edits all look like this.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub same_shapes_moved: bool,
}

/// Two leftovers of one kind that agree on size or on position.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Candidate {
    pub baseline: String,
    pub compared: String,
    /// How far apart they sit, in millimetres.
    pub distance: f64,
    /// The fields that differ, named, so the difference can be read
    /// without setting the two features side by side.
    pub differs: Vec<Change>,
}

/// One field of a feature, before and after.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Change {
    pub field: String,
    pub from: String,
    pub to: String,
}

/// Everything about a feature that a displacement does not change.
///
/// Its kind, its size, and which way it points. Two features with the
/// same signature are the same shape put somewhere else, which is what
/// makes a displacement detectable at all: if the whole model moved, no
/// id matches, and the signature is the only thing left that does.
fn signature(f: &Feature) -> String {
    let q = tolerance();
    let n = |v: Option<f64>| v.map(|x| num(x, q)).unwrap_or_default();
    format!(
        "{}|{}|{}|{}|{}|{}|{:?}|{}",
        f.kind.name(),
        n(f.shape.diameter),
        n(f.shape.radius),
        n(f.shape.depth),
        n(f.shape.length),
        n(f.shape.angle),
        f.shape.through,
        f.shape.axis.map(|a| triple(a, q)).unwrap_or_default()
    )
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

fn delta(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [b[0] - a[0], b[1] - a[1], b[2] - a[2]]
}

/// Compare two documents, the first being the baseline.
pub fn compare(baseline: &FeatureDocument, compared: &FeatureDocument) -> Comparison {
    let pairs = pair_bodies(&baseline.bodies, &compared.bodies);
    let mut bodies: Vec<BodyPair> = pairs
        .iter()
        .map(|(a, b, how)| match (a, b) {
            (Some(a), Some(b)) => compare_bodies(&baseline.bodies[*a], &compared.bodies[*b], *how),
            (Some(a), None) => lone(&baseline.bodies[*a], true),
            (None, Some(b)) => lone(&compared.bodies[*b], false),
            (None, None) => unreachable!("a pair with neither side"),
        })
        .collect();
    // Paired bodies first, then what only the baseline has, then what
    // only the other does: a reader wants the comparison before the
    // leftovers.
    bodies.sort_by_key(|x| {
        let rank = match (x.baseline.is_some(), x.compared.is_some()) {
            (true, true) => 0,
            (true, false) => 1,
            _ => 2,
        };
        (rank, x.baseline.clone(), x.compared.clone())
    });

    let mut summary = Summary::default();
    for b in &bodies {
        summary.matched += b.matched.len();
        summary.only_baseline += b.only_baseline.len();
        summary.only_compared += b.only_compared.len();
        match (b.baseline.is_some(), b.compared.is_some()) {
            (true, true) => summary.bodies_paired += 1,
            (true, false) => summary.bodies_only_baseline += 1,
            (false, true) => summary.bodies_only_compared += 1,
            (false, false) => {}
        }
    }

    Comparison {
        schema_version: SCHEMA_VERSION,
        baseline: baseline.source.clone(),
        compared: compared.source.clone(),
        bodies,
        summary,
    }
}

/// Which body answers which, as indices into the two lists.
///
/// Identical bodies pair by id for nothing. What is left pairs on the
/// features the two have in common, most first, because a body whose
/// hole was bored is a different body by id and has to pair on something
/// weaker. Names come last and only where neither of those worked.
type Pairing = (Option<usize>, Option<usize>, Option<(Paired, usize)>);

fn pair_bodies(a: &[Body], b: &[Body]) -> Vec<Pairing> {
    let mut out = Vec::new();
    let mut used_a: BTreeSet<usize> = BTreeSet::new();
    let mut used_b: BTreeSet<usize> = BTreeSet::new();

    let by_id: BTreeMap<&str, usize> = b
        .iter()
        .enumerate()
        .map(|(i, x)| (x.id.as_str(), i))
        .collect();
    for (i, x) in a.iter().enumerate() {
        if let Some(j) = by_id.get(x.id.as_str()) {
            if used_b.insert(*j) {
                used_a.insert(i);
                out.push((Some(i), Some(*j), Some((Paired::Id, 0))));
            }
        }
    }

    // Then by shared faces, best overlap first. Faces rather than
    // features, because an edit to one feature leaves every face it did
    // not touch exactly as it was, and those are the evidence.
    fn ids(x: &Body) -> BTreeSet<&str> {
        x.features
            .iter()
            .flat_map(|f| f.faces.iter().map(String::as_str))
            .chain(x.unassigned.iter().map(|u| u.id.as_str()))
            .collect()
    }
    let mut scored: Vec<(usize, usize, usize)> = Vec::new();
    for (i, x) in a.iter().enumerate() {
        if used_a.contains(&i) {
            continue;
        }
        let xs = ids(x);
        for (j, y) in b.iter().enumerate() {
            if used_b.contains(&j) {
                continue;
            }
            let shared = xs.intersection(&ids(y)).count();
            if shared > 0 {
                scored.push((shared, i, j));
            }
        }
    }
    scored.sort_by(|p, q| q.0.cmp(&p.0).then(p.1.cmp(&q.1)).then(p.2.cmp(&q.2)));
    for (shared, i, j) in scored {
        if used_a.contains(&i) || used_b.contains(&j) {
            continue;
        }
        used_a.insert(i);
        used_b.insert(j);
        out.push((Some(i), Some(j), Some((Paired::Faces, shared))));
    }

    // Then by being made of the same shapes, which is what is left when
    // a body moved and so shares no id with itself.
    let shapes = |x: &Body| -> BTreeMap<String, usize> {
        let mut m = BTreeMap::new();
        for f in &x.features {
            *m.entry(signature(f)).or_insert(0) += 1;
        }
        m
    };
    for (i, x) in a.iter().enumerate() {
        if used_a.contains(&i) || x.features.is_empty() {
            continue;
        }
        let xs = shapes(x);
        if let Some((j, _)) = b
            .iter()
            .enumerate()
            .find(|(j, y)| !used_b.contains(j) && shapes(y) == xs)
        {
            used_a.insert(i);
            used_b.insert(j);
            out.push((Some(i), Some(j), Some((Paired::Shapes, 0))));
        }
    }

    // Then by name, for bodies sharing no face and no shape.
    for (i, x) in a.iter().enumerate() {
        if used_a.contains(&i) {
            continue;
        }
        let Some(name) = x.name.as_deref() else {
            continue;
        };
        if let Some((j, _)) = b
            .iter()
            .enumerate()
            .find(|(j, y)| !used_b.contains(j) && y.name.as_deref() == Some(name))
        {
            used_a.insert(i);
            used_b.insert(j);
            out.push((Some(i), Some(j), Some((Paired::Name, 0))));
        }
    }

    for i in 0..a.len() {
        if !used_a.contains(&i) {
            out.push((Some(i), None, None));
        }
    }
    for j in 0..b.len() {
        if !used_b.contains(&j) {
            out.push((None, Some(j), None));
        }
    }
    out
}

/// A body with nothing to compare against.
fn lone(body: &Body, is_baseline: bool) -> BodyPair {
    let (baseline, compared) = if is_baseline {
        (Some(body.id.clone()), None)
    } else {
        (None, Some(body.id.clone()))
    };
    let features = body.features.clone();
    BodyPair {
        baseline,
        compared,
        name: body.name.clone(),
        paired: None,
        paired_on: None,
        matched: Vec::new(),
        only_baseline: if is_baseline {
            features.clone()
        } else {
            Vec::new()
        },
        only_compared: if is_baseline { Vec::new() } else { features },
        candidates: Vec::new(),
        placement: None,
        same_shapes_moved: false,
    }
}

fn compare_bodies(a: &Body, b: &Body, how: Option<(Paired, usize)>) -> BodyPair {
    let ids_a: BTreeSet<&str> = a.features.iter().map(|f| f.id.as_str()).collect();
    let ids_b: BTreeSet<&str> = b.features.iter().map(|f| f.id.as_str()).collect();
    let mut matched: Vec<String> = ids_a
        .intersection(&ids_b)
        .map(|s| (*s).to_owned())
        .collect();
    matched.sort();

    let only_a: Vec<Feature> = a
        .features
        .iter()
        .filter(|f| !ids_b.contains(f.id.as_str()))
        .cloned()
        .collect();
    let only_b: Vec<Feature> = b
        .features
        .iter()
        .filter(|f| !ids_a.contains(f.id.as_str()))
        .cloned()
        .collect();

    let (placement, same_shapes_moved) = displacement(&only_a, &only_b, matched.len());
    let (paired, on) = how.unwrap_or((Paired::Name, 0));
    BodyPair {
        baseline: Some(a.id.clone()),
        compared: Some(b.id.clone()),
        name: a.name.clone().or_else(|| b.name.clone()),
        paired: Some(paired),
        paired_on: (paired == Paired::Faces).then_some(on),
        matched,
        candidates: candidates(&only_a, &only_b),
        only_baseline: only_a,
        only_compared: only_b,
        placement,
        same_shapes_moved,
    }
}

/// One displacement carrying every leftover onto a counterpart of the
/// same shape, where one does.
///
/// Needed because a whole model moved has no matching ids at all: every
/// feature's position changed, so every id changed with it. Pairing on
/// what a displacement cannot alter — kind, size, direction — is the only
/// thing left to measure a displacement from. The second return says the
/// shapes all have counterparts but no one displacement puts them there,
/// which is what a rotation and a handful of separate edits both look
/// like, and which is therefore not guessed at.
fn displacement(a: &[Feature], b: &[Feature], matched: usize) -> (Option<[f64; 3]>, bool) {
    // A body that moved takes all its features with it, so anything that
    // kept its id is proof the body stayed where it was and something
    // inside it changed instead. And one feature agreeing with one other
    // is not evidence of anything: two leftovers are the least that can
    // distinguish a body having moved from a feature having moved.
    if matched > 0 || a.len() < 2 || a.len() != b.len() {
        return (None, false);
    }
    let group = |fs: &[Feature]| -> BTreeMap<String, usize> {
        let mut m = BTreeMap::new();
        for f in fs {
            *m.entry(signature(f)).or_insert(0) += 1;
        }
        m
    };
    if group(a) != group(b) {
        return (None, false);
    }

    // Every displacement that carries some leftover onto a counterpart of
    // the same shape, and how many times each one does.
    let q = tolerance();
    let mut votes: BTreeMap<String, ([f64; 3], usize)> = BTreeMap::new();
    for x in a {
        let (Some(px), Some(sx)) = (x.shape.position, Some(signature(x))) else {
            continue;
        };
        for y in b {
            if signature(y) != sx {
                continue;
            }
            let Some(py) = y.shape.position else { continue };
            let d = delta(px, py);
            votes.entry(triple(d, q)).or_insert((d, 0)).1 += 1;
        }
    }
    // One displacement explains the lot only if it accounts for every
    // leftover on both sides exactly once.
    let best = votes
        .values()
        .filter(|(_, n)| *n == a.len())
        .map(|(d, _)| *d);
    match best.min_by(|p, r| {
        distance([0.0; 3], *p)
            .partial_cmp(&distance([0.0; 3], *r))
            .unwrap_or(std::cmp::Ordering::Equal)
    }) {
        Some(d) if distance([0.0; 3], d) > q => (Some(d), false),
        _ => (None, true),
    }
}

/// Leftovers that look related, paired off greedily.
///
/// A candidate has to agree on its size or on its position, so that
/// "the same hole, bored wider" and "the same hole, moved" are offered
/// and two unrelated holes are not. That is a deliberately narrow rule:
/// a candidate is meant to save a reader from scanning two lists, not to
/// stand in for their judgement.
fn candidates(a: &[Feature], b: &[Feature]) -> Vec<Candidate> {
    let q = tolerance();
    let same = |x: Option<f64>, y: Option<f64>| match (x, y) {
        (Some(x), Some(y)) => (x - y).abs() <= q,
        (None, None) => true,
        _ => false,
    };
    let mut scored: Vec<(f64, usize, usize)> = Vec::new();
    for (i, x) in a.iter().enumerate() {
        for (j, y) in b.iter().enumerate() {
            if x.kind != y.kind {
                continue;
            }
            let size =
                same(x.shape.diameter, y.shape.diameter) && same(x.shape.radius, y.shape.radius);
            let here = match (x.shape.position, y.shape.position) {
                (Some(p), Some(r)) => distance(p, r) <= q,
                _ => false,
            };
            if !size && !here {
                continue;
            }
            let d = match (x.shape.position, y.shape.position) {
                (Some(p), Some(r)) => distance(p, r),
                _ => 0.0,
            };
            scored.push((d, i, j));
        }
    }
    scored.sort_by(|p, r| {
        p.0.partial_cmp(&r.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(p.1.cmp(&r.1))
            .then(p.2.cmp(&r.2))
    });
    let (mut used_a, mut used_b) = (BTreeSet::new(), BTreeSet::new());
    let mut out = Vec::new();
    for (d, i, j) in scored {
        if !used_a.insert(i) {
            continue;
        }
        if !used_b.insert(j) {
            used_a.remove(&i);
            continue;
        }
        out.push(Candidate {
            baseline: a[i].id.clone(),
            compared: b[j].id.clone(),
            distance: num(d, q).parse().unwrap_or(d),
            differs: changes(&a[i], &b[j]),
        });
    }
    out.sort_by(|p, r| p.baseline.cmp(&r.baseline));
    out
}

/// Which fields of two features differ, named.
fn changes(a: &Feature, b: &Feature) -> Vec<Change> {
    let q = tolerance();
    let mut out = Vec::new();
    let mut number = |field: &str, x: Option<f64>, y: Option<f64>| {
        let differs = match (x, y) {
            (Some(x), Some(y)) => (x - y).abs() > q,
            (None, None) => false,
            _ => true,
        };
        if differs {
            out.push(Change {
                field: field.to_owned(),
                from: x.map(text).unwrap_or_else(|| "-".into()),
                to: y.map(text).unwrap_or_else(|| "-".into()),
            });
        }
    };
    number("diameter", a.shape.diameter, b.shape.diameter);
    number("radius", a.shape.radius, b.shape.radius);
    number("depth", a.shape.depth, b.shape.depth);
    number("length", a.shape.length, b.shape.length);
    number("angle", a.shape.angle, b.shape.angle);
    if a.shape.through != b.shape.through {
        let say = |v: Option<bool>| match v {
            Some(true) => "through".to_owned(),
            Some(false) => "blind".to_owned(),
            None => "-".to_owned(),
        };
        out.push(Change {
            field: "through".to_owned(),
            from: say(a.shape.through),
            to: say(b.shape.through),
        });
    }
    // The stretch of the axis a feature occupies is part of its
    // identity, so two features can agree on every other number and
    // still be different features. Leaving it out made such a pair look
    // like a difference with nothing different about it.
    let span = |e: Option<[f64; 2]>| e.map(|v| format!("{}..{}", text(v[0]), text(v[1])));
    let extent_differs = match (a.shape.extent, b.shape.extent) {
        (Some(x), Some(y)) => (x[0] - y[0]).abs() > q || (x[1] - y[1]).abs() > q,
        (None, None) => false,
        _ => true,
    };
    if extent_differs {
        out.push(Change {
            field: "extent".to_owned(),
            from: span(a.shape.extent).unwrap_or_else(|| "-".into()),
            to: span(b.shape.extent).unwrap_or_else(|| "-".into()),
        });
    }
    for (field, x, y) in [
        ("position", a.shape.position, b.shape.position),
        ("axis", a.shape.axis, b.shape.axis),
    ] {
        let differs = match (x, y) {
            (Some(x), Some(y)) => distance(x, y) > q,
            (None, None) => false,
            _ => true,
        };
        if differs {
            out.push(Change {
                field: field.to_owned(),
                from: x.map(point_text).unwrap_or_else(|| "-".into()),
                to: y.map(point_text).unwrap_or_else(|| "-".into()),
            });
        }
    }
    out
}
