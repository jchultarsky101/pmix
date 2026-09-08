//! Identity keys shared by the readers (ADR 0004).
//!
//! A record's `id` comes from its **identity key**: the attributes that
//! say *which* design element it is, never what it currently says. The
//! recipes live with each reader, because they depend on what a format
//! states, but the machinery that turns a key into an id is the same for
//! both, and so are the recipes for records whose identity is design
//! intent rather than geometry: a datum is its letter and a saved view is
//! its name in either format.

use std::collections::{BTreeMap, HashMap};

use crate::model::content_hash;

/// The rounding applied to coordinates in an identity key, in model
/// units. Deliberately not the file's own uncertainty, which differs
/// between exports of one design.
pub const QUANTUM: f64 = 1e-3;

/// The rounding used where a key can only be approximate, such as an
/// annotation located by where it sits rather than by what it is on.
pub const COARSE_QUANTUM: f64 = QUANTUM * 100.0;

/// Round `v` to a multiple of `q` and print it stably.
///
/// The value is first snapped to a 1e-5 grid: re-exports print the same
/// coordinate with different precision (`-2.0315` against `-2.03149999`),
/// and without the snap such pairs would round to different multiples of
/// `q` whenever the true value sits on a rounding boundary, which
/// engineering values in round fractions of an inch often do.
pub fn num(v: f64, q: f64) -> String {
    let snapped = (v * 1e5).round() / 1e5;
    let r = (snapped / q).round() * q;
    let r = if r == 0.0 { 0.0 } else { r };
    format!("{r:.4}")
}

/// Three coordinates rounded together.
pub fn triple(p: [f64; 3], q: f64) -> String {
    format!("{},{},{}", num(p[0], q), num(p[1], q), num(p[2], q))
}

/// Hash of a record's content, with the fields that are not content
/// removed. Used to order records that share an identity key.
pub fn hash_content<T: serde::Serialize>(rec: &T, skip: &[&str]) -> String {
    let mut v = serde_json::to_value(rec).unwrap_or_default();
    if let Some(obj) = v.as_object_mut() {
        for k in skip {
            obj.remove(*k);
        }
    }
    content_hash([v.to_string()])
}

/// Replace an id that a finalised record has been given a new name for.
pub fn remap(id: &mut String, map: &HashMap<String, String>) {
    if let Some(new) = map.get(id) {
        *id = new.clone();
    }
}

/// [`remap`] over a list, sorted and deduplicated so that the order the
/// walkers happened to produce does not reach the output.
pub fn remap_all(ids: &mut Vec<String>, map: &HashMap<String, String>) {
    for id in ids.iter_mut() {
        remap(id, map);
    }
    ids.sort();
    ids.dedup();
}

/// Turn `(index, identity key, content hash)` triples into `(index, id)`.
///
/// Records that share an identity key are two callouts of the same thing,
/// which a file is allowed to contain. They are ordered by content hash
/// rather than by the order they were read, so that the diff pairs them
/// consistently and adding one does not disturb the others.
pub fn assign(
    batch: Vec<(usize, String, String)>,
    prefix: &str,
    readable: bool,
) -> Vec<(usize, String)> {
    let mut groups: BTreeMap<String, Vec<(String, usize)>> = BTreeMap::new();
    for (i, key, content) in batch {
        groups.entry(key).or_default().push((content, i));
    }
    let mut out = Vec::new();
    for (key, mut members) in groups {
        members.sort();
        let base = if readable && is_readable(&key) {
            format!("{prefix}:{key}")
        } else {
            format!("{prefix}:{}", content_hash([key.as_str()]))
        };
        for (n, (_, i)) in members.into_iter().enumerate() {
            let id = if n == 0 {
                base.clone()
            } else {
                format!("{base}-{}", n + 1)
            };
            tracing::trace!(%id, %key, "identity key");
            out.push((i, id));
        }
    }
    out
}

/// Whether a key is short and plain enough to appear in an id as it is,
/// rather than as a hash of itself.
pub fn is_readable(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 40
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '|' | '_' | '.' | '-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_readable_key_becomes_the_id_itself() {
        let ids = assign(vec![(0, "A".into(), "x".into())], "datum", true);
        assert_eq!(ids[0].1, "datum:A");
        let ids = assign(vec![(0, "A|B|C".into(), "x".into())], "dsys", true);
        assert_eq!(ids[0].1, "dsys:A|B|C");
    }

    #[test]
    fn a_key_that_is_long_or_odd_is_hashed() {
        assert!(!is_readable(&"x".repeat(41)));
        assert!(!is_readable("has a space"));
        assert!(!is_readable(""));
        let ids = assign(vec![(0, "has a space".into(), "x".into())], "view", true);
        assert!(ids[0].1.starts_with("view:") && !ids[0].1.contains(' '));
        // A key is hashed whenever the caller says the prefix is not
        // readable, however plain the key looks.
        let ids = assign(vec![(0, "A".into(), "x".into())], "dim", false);
        assert_ne!(ids[0].1, "dim:A");
    }

    #[test]
    fn records_sharing_a_key_are_ordered_by_content_not_by_position() {
        let batch = vec![
            (0, "same".into(), "zzz".into()),
            (1, "same".into(), "aaa".into()),
            (2, "other".into(), "mmm".into()),
        ];
        let ids: BTreeMap<usize, String> = assign(batch, "tol", false).into_iter().collect();
        // The one with the lower content hash keeps the plain id.
        assert!(!ids[&1].ends_with("-2"));
        assert!(ids[&0].ends_with("-2"));
        assert_eq!(ids[&0].trim_end_matches("-2"), ids[&1]);
        assert_ne!(ids[&2], ids[&1]);
    }

    #[test]
    fn rounding_absorbs_the_precision_an_export_prints_with() {
        assert_eq!(num(-2.0315, QUANTUM), num(-2.03149999, QUANTUM));
        assert_eq!(num(0.0, QUANTUM), num(-0.0, QUANTUM));
        // A difference larger than the quantum still registers.
        assert_ne!(num(1.0, QUANTUM), num(1.002, QUANTUM));
        assert_eq!(triple([0.0; 3], QUANTUM), "0.0000,0.0000,0.0000");
    }

    #[test]
    fn remapping_sorts_and_deduplicates() {
        let map: HashMap<String, String> =
            [("b".to_owned(), "tol:2".to_owned())].into_iter().collect();
        let mut ids = vec!["b".to_owned(), "a".to_owned(), "b".to_owned()];
        remap_all(&mut ids, &map);
        assert_eq!(ids, ["a", "tol:2"]);
    }
}
