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

#[inline]
fn fnv_step(h: u64, b: u8) -> u64 {
    (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
}

#[cfg(test)]
mod tests {
    use super::ContentId;

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
