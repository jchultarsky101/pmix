//! Graphical geometry shared by the readers.
//!
//! Both formats summarise what an annotation draws the same way, so a
//! summary means the same thing whichever file it came from.

use std::fmt::Write;

use crate::model::{BBox, ContentHasher, GeometrySummary};

/// Collected geometry of one occurrence.
#[derive(Debug, Default, Clone)]
pub struct Geometry {
    pub polylines: Vec<Vec<[f64; 3]>>,
    pub vertices: Vec<[f64; 3]>,
    pub triangles: Vec<[u32; 3]>,
}

impl Geometry {
    /// Summary with coordinates rounded to `quantum`.
    pub fn summary(&self, quantum: f64) -> GeometrySummary {
        let mut acc = Accumulator::new(quantum);
        for pl in &self.polylines {
            acc.push("pl");
            for p in pl {
                acc.visit(p);
            }
        }
        if !self.vertices.is_empty() {
            acc.push("v");
            for p in &self.vertices {
                acc.visit(p);
            }
            acc.push("t");
            for t in &self.triangles {
                acc.scratch.clear();
                let _ = write!(acc.scratch, "{},{},{}", t[0], t[1], t[2]);
                acc.hasher.push(&acc.scratch);
            }
        }
        GeometrySummary {
            polylines: self.polylines.len(),
            triangles: self.triangles.len(),
            points: acc.points,
            bbox: (acc.points > 0).then_some(BBox {
                min: acc.min,
                max: acc.max,
            }),
            hash: acc.hasher.finish(),
        }
    }

    /// Merge another occurrence's geometry into this one.
    pub fn extend(&mut self, other: &Geometry) {
        let base = self.vertices.len() as u32;
        self.polylines.extend(other.polylines.iter().cloned());
        self.vertices.extend(other.vertices.iter().copied());
        self.triangles.extend(
            other
                .triangles
                .iter()
                .map(|t| [t[0] + base, t[1] + base, t[2] + base]),
        );
    }
}

/// Walks a geometry once, tracking its extent and hashing its rounded
/// coordinates as it goes rather than collecting them.
struct Accumulator {
    quantum: f64,
    points: usize,
    min: [f64; 3],
    max: [f64; 3],
    hasher: ContentHasher,
    /// Reused for each number written, so summarising allocates nothing
    /// per coordinate.
    scratch: String,
}

impl Accumulator {
    fn new(quantum: f64) -> Self {
        Self {
            quantum,
            points: 0,
            min: [f64::INFINITY; 3],
            max: [f64::NEG_INFINITY; 3],
            hasher: ContentHasher::new(),
            scratch: String::with_capacity(64),
        }
    }

    fn push(&mut self, part: &str) {
        self.hasher.push(part);
    }

    fn visit(&mut self, p: &[f64; 3]) {
        self.points += 1;
        for (i, v) in p.iter().enumerate() {
            self.min[i] = self.min[i].min(*v);
            self.max[i] = self.max[i].max(*v);
        }
        let q = self.quantum;
        self.scratch.clear();
        for (i, v) in p.iter().enumerate() {
            if i > 0 {
                self.scratch.push(',');
            }
            write_round(&mut self.scratch, *v, q);
        }
        self.hasher.push(&self.scratch);
    }
}

/// Round to a multiple of `quantum`, printed with enough digits.
pub fn round(v: f64, quantum: f64) -> String {
    let mut out = String::new();
    write_round(&mut out, v, quantum);
    out
}

/// [`round`] written into an existing buffer, so a caller rounding many
/// numbers does not allocate for each one.
///
/// Summarising a tessellated annotation formats several numbers per
/// coordinate and does it for hundreds of thousands of coordinates, which
/// made `{:.6}` the reader's single largest cost. A value already snapped
/// to a multiple of `quantum` needs only fixed-point arithmetic, so the
/// general formatter is kept for the values that fall outside the range
/// where that is exact.
pub fn write_round(out: &mut String, v: f64, quantum: f64) {
    let r = (v / quantum).round() * quantum;
    // Avoid "-0".
    let r = if r == 0.0 { 0.0 } else { r };
    if write_fixed6(out, r) {
        return;
    }
    let _ = write!(out, "{r:.6}");
}

/// Numbers this large lose the exactness the fixed-point path depends on,
/// because scaling by a million overruns what a double counts in ones.
const FIXED6_LIMIT: f64 = 9_007_199_254.0 / 1e3;

/// Write `r` with six decimal places without the general float formatter.
/// Returns `false` when `r` is outside the range this is exact for, and
/// writes nothing in that case.
fn write_fixed6(out: &mut String, r: f64) -> bool {
    // Anything that is not a number, and anything too large to scale
    // exactly, goes to the general formatter.
    if r.is_nan() || r.abs() >= FIXED6_LIMIT {
        return false;
    }
    let scaled = (r * 1e6).round();
    let mut n = scaled as i64;
    if n < 0 {
        out.push('-');
        n = -n;
    }
    let whole = n / 1_000_000;
    let frac = (n % 1_000_000) as u32;
    let mut buf = itoa(whole as u64);
    out.push_str(&buf);
    out.push('.');
    buf = itoa(frac as u64);
    for _ in buf.len()..6 {
        out.push('0');
    }
    out.push_str(&buf);
    true
}

/// Decimal digits of `v`, without allocating on the heap.
fn itoa(mut v: u64) -> arrayvec::Digits {
    let mut d = arrayvec::Digits::new();
    if v == 0 {
        d.push(b'0');
        return d;
    }
    let mut tmp = [0u8; 20];
    let mut n = 0;
    while v > 0 {
        tmp[n] = b'0' + (v % 10) as u8;
        v /= 10;
        n += 1;
    }
    for i in (0..n).rev() {
        d.push(tmp[i]);
    }
    d
}

/// A tiny fixed-capacity string, so formatting a number touches no
/// allocator at all.
mod arrayvec {
    #[derive(Debug)]
    pub struct Digits {
        buf: [u8; 24],
        len: usize,
    }

    impl Digits {
        pub fn new() -> Self {
            Self {
                buf: [0; 24],
                len: 0,
            }
        }

        pub fn push(&mut self, b: u8) {
            if self.len < self.buf.len() {
                self.buf[self.len] = b;
                self.len += 1;
            }
        }

        pub fn len(&self) -> usize {
            self.len
        }
    }

    impl std::ops::Deref for Digits {
        type Target = str;

        fn deref(&self) -> &str {
            // Only ASCII digits are ever pushed.
            std::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Geometry {
        Geometry {
            polylines: vec![vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]]],
            ..Default::default()
        }
    }

    /// The quanta the readers actually round with.
    const QUANTA: [f64; 4] = [1e-6, 1e-3, 1e-1, 0.005];

    fn reference(v: f64, q: f64) -> String {
        let r = (v / q).round() * q;
        let r = if r == 0.0 { 0.0 } else { r };
        format!("{r:.6}")
    }

    #[test]
    fn the_fast_formatter_agrees_with_the_general_one() {
        // A cheap deterministic generator, so the check is reproducible.
        let mut state = 0x2545_f491_4f6c_dd1d_u64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let mut checked = 0;
        for _ in 0..200_000 {
            let bits = next();
            // Values spread over the magnitudes a model uses, both signs.
            let mag = 10f64.powi((bits % 13) as i32 - 6);
            let v = ((bits >> 8) as f64 / u64::MAX as f64)
                * mag
                * if bits & 1 == 0 { 1.0 } else { -1.0 };
            for q in QUANTA {
                let mut fast = String::new();
                write_round(&mut fast, v, q);
                assert_eq!(fast, reference(v, q), "v={v:?} q={q:?}");
                checked += 1;
            }
        }
        assert!(checked > 500_000);
    }

    #[test]
    fn the_fast_formatter_agrees_on_the_awkward_values() {
        let awkward = [
            0.0,
            -0.0,
            1.0,
            -1.0,
            0.5,
            -0.5,
            0.0000005,
            -0.0000005,
            0.9999995,
            123.456789,
            -2.03149999,
            -2.0315,
            1e-9,
            -1e-9,
            9_007_199.254,
            -9_007_199.254,
            // Beyond the fixed-point range, so the general formatter runs.
            1e12,
            -1e12,
            1e300,
            f64::MAX,
            f64::MIN,
        ];
        for v in awkward {
            for q in QUANTA {
                let mut fast = String::new();
                write_round(&mut fast, v, q);
                assert_eq!(fast, reference(v, q), "v={v:?} q={q:?}");
            }
        }
        // Values that are not numbers fall through to the general path
        // rather than producing digits.
        for v in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut fast = String::new();
            write_round(&mut fast, v, 1e-6);
            assert_eq!(fast, reference(v, 1e-6), "v={v:?}");
        }
    }

    #[test]
    fn a_summary_counts_and_bounds_what_it_draws() {
        let s = square().summary(1e-6);
        assert_eq!((s.polylines, s.points, s.triangles), (1, 3, 0));
        let b = s.bbox.unwrap();
        assert_eq!((b.min, b.max), ([0.0; 3], [1.0, 1.0, 0.0]));
        assert!(!s.hash.is_empty());
    }

    #[test]
    fn the_hash_follows_the_coordinates() {
        let a = square().summary(1e-6);
        let mut moved = square();
        moved.polylines[0][2][1] = 2.0;
        assert_ne!(a.hash, moved.summary(1e-6).hash);
        // Two runs over the same geometry agree.
        assert_eq!(a.hash, square().summary(1e-6).hash);
        // A difference finer than the quantum does not register.
        let mut nudged = square();
        nudged.polylines[0][2][1] += 1e-9;
        assert_eq!(a.hash, nudged.summary(1e-6).hash);
    }

    #[test]
    fn merging_renumbers_the_triangles_it_takes_on() {
        let mut a = Geometry {
            vertices: vec![[0.0; 3], [1.0; 3], [2.0; 3]],
            triangles: vec![[0, 1, 2]],
            ..Default::default()
        };
        let b = a.clone();
        a.extend(&b);
        assert_eq!(a.triangles, [[0, 1, 2], [3, 4, 5]]);
        assert_eq!(a.vertices.len(), 6);
    }
}
