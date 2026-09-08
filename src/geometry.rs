//! Graphical geometry shared by the readers.
//!
//! Both formats summarise what an annotation draws the same way, so a
//! summary means the same thing whichever file it came from.

use crate::model::{BBox, GeometrySummary, content_hash};

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
            acc.parts.push("pl".into());
            for p in pl {
                acc.visit(p);
            }
        }
        if !self.vertices.is_empty() {
            acc.parts.push("v".into());
            for p in &self.vertices {
                acc.visit(p);
            }
            acc.parts.push("t".into());
            for t in &self.triangles {
                acc.parts.push(format!("{},{},{}", t[0], t[1], t[2]));
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
            hash: content_hash(acc.parts),
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

struct Accumulator {
    quantum: f64,
    points: usize,
    min: [f64; 3],
    max: [f64; 3],
    parts: Vec<String>,
}

impl Accumulator {
    fn new(quantum: f64) -> Self {
        Self {
            quantum,
            points: 0,
            min: [f64::INFINITY; 3],
            max: [f64::NEG_INFINITY; 3],
            parts: Vec::new(),
        }
    }

    fn visit(&mut self, p: &[f64; 3]) {
        self.points += 1;
        for (i, v) in p.iter().enumerate() {
            self.min[i] = self.min[i].min(*v);
            self.max[i] = self.max[i].max(*v);
        }
        self.parts.push(format!(
            "{},{},{}",
            round(p[0], self.quantum),
            round(p[1], self.quantum),
            round(p[2], self.quantum)
        ));
    }
}

/// Round to a multiple of `quantum`, printed with enough digits.
pub fn round(v: f64, quantum: f64) -> String {
    let r = (v / quantum).round() * quantum;
    // Avoid "-0".
    let r = if r == 0.0 { 0.0 } else { r };
    format!("{r:.6}")
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
