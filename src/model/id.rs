//! Content-derived identifiers.
//!
//! Interim scheme per ADR 0002: a 64-bit FNV-1a hash of a canonical string
//! built from the record's content, prefixed with the record kind. Stable
//! for identical content; collisions within a document get an ordinal
//! suffix. The identity ADR will replace the recipe, not the shape.

use std::collections::HashMap;

/// Builds ids and resolves collisions within one document.
#[derive(Debug, Default)]
pub struct ContentId {
    seen: HashMap<String, usize>,
}

impl ContentId {
    pub fn new() -> Self {
        Self::default()
    }

    /// Hash `parts` under `prefix` and return a unique id such as
    /// `dim:3f9a1c0b7d2e6a48` (or `...-2` on collision).
    pub fn make(&mut self, prefix: &str, parts: &[&str]) -> String {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for (i, p) in parts.iter().enumerate() {
            if i > 0 {
                h = fnv_step(h, 0x1f);
            }
            for b in p.bytes() {
                h = fnv_step(h, b);
            }
        }
        let base = format!("{prefix}:{h:016x}");
        let n = self.seen.entry(base.clone()).or_insert(0);
        *n += 1;
        if *n == 1 { base } else { format!("{base}-{n}") }
    }
}

/// Plain FNV-1a hash of `parts` as 16 hex digits, without collision
/// handling. Used for geometry hashes.
pub fn content_hash(parts: impl IntoIterator<Item = impl AsRef<str>>) -> String {
    let mut h = ContentHasher::new();
    for p in parts {
        h.push(p.as_ref());
    }
    h.finish()
}

/// [`content_hash`] built up one part at a time.
///
/// Summarising a tessellated annotation hashes every coordinate it draws.
/// Collecting those into a vector of strings first costs one allocation
/// per number and dominated the reader's time; this hashes them as they
/// are produced. The digest is identical either way.
#[derive(Debug, Clone)]
pub struct ContentHasher {
    hash: u64,
    started: bool,
}

impl Default for ContentHasher {
    fn default() -> Self {
        Self::new()
    }
}

impl ContentHasher {
    pub fn new() -> Self {
        Self {
            hash: 0xcbf2_9ce4_8422_2325,
            started: false,
        }
    }

    /// Add one part, separated from the last as [`content_hash`] separates
    /// its arguments.
    pub fn push(&mut self, part: &str) {
        if self.started {
            self.hash = fnv_step(self.hash, 0x1f);
        }
        self.started = true;
        for b in part.bytes() {
            self.hash = fnv_step(self.hash, b);
        }
    }

    pub fn finish(&self) -> String {
        format!("{:016x}", self.hash)
    }
}

#[inline]
fn fnv_step(h: u64, b: u8) -> u64 {
    (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
}

#[cfg(test)]
mod tests {
    use super::{ContentHasher, ContentId, content_hash};

    #[test]
    fn hashing_in_parts_matches_hashing_all_at_once() {
        let parts = ["pl", "1.0000,2.0000,3.0000", "", "t"];
        let mut h = ContentHasher::new();
        for p in parts {
            h.push(p);
        }
        assert_eq!(h.finish(), content_hash(parts));
        // The separator matters, so a different split is a different hash.
        assert_ne!(content_hash(["ab", "c"]), content_hash(["a", "bc"]));
        let none: [&str; 0] = [];
        assert_eq!(ContentHasher::new().finish(), content_hash(none));
    }

    #[test]
    fn same_content_same_id_and_collisions_get_suffixes() {
        let mut a = ContentId::new();
        let mut b = ContentId::new();
        let x = a.make("dim", &["size", "diameter", "35"]);
        let y = b.make("dim", &["size", "diameter", "35"]);
        assert_eq!(x, y);
        assert!(x.starts_with("dim:") && x.len() == 4 + 16, "{x}");
        let z = a.make("dim", &["size", "diameter", "35"]);
        assert_eq!(z, format!("{x}-2"));
        assert_ne!(a.make("dim", &["size", "diameter", "36"]), x);
        // Part boundaries matter.
        let mut c = ContentId::new();
        assert_ne!(c.make("k", &["ab", "c"]), c.make("k", &["a", "bc"]));
    }
}
