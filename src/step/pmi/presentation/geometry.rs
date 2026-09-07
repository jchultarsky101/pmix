//! Graphical geometry: tessellation, curves, placements, summaries.

use crate::model::{BBox, GeometrySummary, Placement, content_hash};
use crate::step::p21::{Exchange, Instance, Parameter};

use super::super::Ctx;
use super::super::datums::placement;

/// Collected geometry of one occurrence.
#[derive(Debug, Default, Clone)]
pub(crate) struct Geometry {
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
fn round(v: f64, quantum: f64) -> String {
    let r = (v / quantum).round() * quantum;
    // Avoid "-0".
    let r = if r == 0.0 { 0.0 } else { r };
    format!("{r:.6}")
}

/// The file's length uncertainty, used as the rounding quantum for
/// geometry hashes; 1e-6 when absent.
pub(crate) fn rounding_quantum(ex: &Exchange) -> f64 {
    ex.of_type("UNCERTAINTY_MEASURE_WITH_UNIT")
        .filter_map(|u| u.parameters().first().and_then(Parameter::as_f64))
        .find(|v| *v > 0.0)
        .unwrap_or(1e-6)
}

/// Rigid transform from a placement (origin, z axis, x reference).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Frame {
    origin: [f64; 3],
    x: [f64; 3],
    y: [f64; 3],
    z: [f64; 3],
}

impl Frame {
    pub fn from_placement(p: &Placement) -> Option<Self> {
        let z = p.axis.as_ref().map(|d| normalise([d.x, d.y, d.z]))?;
        let xr = p
            .ref_direction
            .as_ref()
            .map(|d| [d.x, d.y, d.z])
            .unwrap_or(if z[0].abs() < 0.9 {
                [1.0, 0.0, 0.0]
            } else {
                [0.0, 1.0, 0.0]
            });
        // Gram-Schmidt x against z.
        let dot = xr[0] * z[0] + xr[1] * z[1] + xr[2] * z[2];
        let x = normalise([xr[0] - dot * z[0], xr[1] - dot * z[1], xr[2] - dot * z[2]]);
        let y = [
            z[1] * x[2] - z[2] * x[1],
            z[2] * x[0] - z[0] * x[2],
            z[0] * x[1] - z[1] * x[0],
        ];
        Some(Self {
            origin: p.origin,
            x,
            y,
            z,
        })
    }

    pub fn apply(&self, p: [f64; 3]) -> [f64; 3] {
        [
            self.origin[0] + p[0] * self.x[0] + p[1] * self.y[0] + p[2] * self.z[0],
            self.origin[1] + p[0] * self.x[1] + p[1] * self.y[1] + p[2] * self.z[1],
            self.origin[2] + p[0] * self.x[2] + p[1] * self.y[2] + p[2] * self.z[2],
        ]
    }
}

fn normalise(v: [f64; 3]) -> [f64; 3] {
    let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if n == 0.0 {
        v
    } else {
        [v[0] / n, v[1] / n, v[2] / n]
    }
}

/// Decode a `coordinates_list` into points.
fn coordinates(inst: &Instance) -> Vec<[f64; 3]> {
    inst.parameters()
        .get(2)
        .and_then(Parameter::as_list)
        .map(|l| {
            l.iter()
                .filter_map(|p| {
                    let t = p.as_list()?;
                    Some([
                        t.first()?.as_f64()?,
                        t.get(1)?.as_f64()?,
                        t.get(2)?.as_f64()?,
                    ])
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A list of lists of 1-based indices (line strips, triangle strips, fans).
fn index_lists(p: Option<&Parameter>) -> Vec<Vec<u32>> {
    p.and_then(Parameter::as_list)
        .map(|l| {
            l.iter()
                .filter_map(|s| {
                    let items = s.as_list()?;
                    Some(
                        items
                            .iter()
                            .filter_map(|i| i.as_i64().map(|v| v.max(1) as u32 - 1))
                            .collect(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Geometry of a `tessellated_geometric_set` (possibly repositioned) or of
/// a single tessellated item.
pub(crate) fn tessellated(ctx: &mut Ctx<'_>, set: &Instance) -> Geometry {
    let ex = ctx.ex;
    let mut geo = Geometry::default();
    let frame = set
        .attr("REPOSITIONED_TESSELLATED_ITEM", 0)
        .and_then(Parameter::as_ref)
        .and_then(|id| ex.get(id))
        .and_then(|p| placement(ex, p))
        .and_then(|p| Frame::from_placement(&p));
    let children: Vec<_> = match set.segment("TESSELLATED_GEOMETRIC_SET") {
        Some(seg) => {
            // Simple form: (name, children); complex form: (children).
            let mut v = Vec::new();
            if let Some(list) = seg.parameters.iter().find(|q| q.as_list().is_some()) {
                list.collect_refs(&mut v);
            }
            v.into_iter().filter_map(|id| ex.get(id)).collect()
        }
        None => vec![set],
    };
    for child in children {
        ctx.consume(child.id);
        let p = child.parameters();
        let coords_inst = p
            .iter()
            .filter_map(Parameter::as_ref)
            .filter_map(|id| ex.get(id))
            .find(|i| i.has_type("COORDINATES_LIST"));
        let Some(coords_inst) = coords_inst else {
            ctx.warn(
                format!("tessellated item #{} has no coordinates list", child.id),
                Some(child.id),
            );
            continue;
        };
        ctx.consume(coords_inst.id);
        let mut coords = coordinates(coords_inst);
        if let Some(f) = frame {
            coords = coords.into_iter().map(|c| f.apply(c)).collect();
        }
        if child.has_type("TESSELLATED_CURVE_SET") {
            // (name, coordinates, line_strips)
            for strip in index_lists(p.get(2)) {
                let pts: Vec<[f64; 3]> = strip
                    .iter()
                    .filter_map(|i| coords.get(*i as usize).copied())
                    .collect();
                if pts.len() >= 2 {
                    geo.polylines.push(pts);
                }
            }
        } else if child.has_type("COMPLEX_TRIANGULATED_SURFACE_SET")
            || child.has_type("TRIANGULATED_SURFACE_SET")
            || child.has_type("TRIANGULATED_FACE")
            || child.has_type("COMPLEX_TRIANGULATED_FACE")
        {
            let base = geo.vertices.len() as u32;
            geo.vertices.extend(coords.iter().copied());
            // Strips and fans are the last two list-of-list parameters.
            let lists: Vec<&Parameter> = p
                .iter()
                .filter(|q| {
                    q.as_list()
                        .is_some_and(|l| l.iter().all(|e| e.as_list().is_some()))
                })
                .collect();
            let n = lists.len();
            if n >= 1 {
                let (strips, fans) = if n >= 2 {
                    (
                        index_lists(Some(lists[n - 2])),
                        index_lists(Some(lists[n - 1])),
                    )
                } else {
                    (index_lists(Some(lists[0])), Vec::new())
                };
                for s in strips {
                    for w in s.windows(3) {
                        geo.triangles.push([base + w[0], base + w[1], base + w[2]]);
                    }
                }
                for f in fans {
                    if let Some((&c, rest)) = f.split_first() {
                        for w in rest.windows(2) {
                            geo.triangles.push([base + c, base + w[0], base + w[1]]);
                        }
                    }
                }
            }
        } else if child.has_type("TESSELLATED_POINT_SET") || child.has_type("TESSELLATED_WIRE") {
            geo.polylines.push(coords);
        } else {
            ctx.warn(
                format!(
                    "unrecognised tessellated item #{} ({})",
                    child.id,
                    child.type_key()
                ),
                Some(child.id),
            );
        }
    }
    geo
}

/// Geometry of a `geometric_curve_set` / `geometric_set`: polylines kept,
/// circles and trimmed curves sampled.
pub(crate) fn curve_set(ctx: &mut Ctx<'_>, set: &Instance, angle_is_degrees: bool) -> Geometry {
    let ex = ctx.ex;
    let mut geo = Geometry::default();
    let mut elems = Vec::new();
    if let Some(p) = set.parameters().get(1) {
        p.collect_refs(&mut elems);
    }
    for id in elems {
        let Some(e) = ex.get(id) else { continue };
        if let Some(pl) = curve(ctx, e, angle_is_degrees) {
            geo.polylines.push(pl);
            ctx.consume(id);
        } else if e.has_type("CARTESIAN_POINT") {
            if let Some(pt) = point(e) {
                geo.polylines.push(vec![pt]);
            }
        }
    }
    geo
}

fn point(inst: &Instance) -> Option<[f64; 3]> {
    let l = inst.parameters().get(1)?.as_list()?;
    Some([
        l.first()?.as_f64()?,
        l.get(1)?.as_f64()?,
        l.get(2).and_then(Parameter::as_f64).unwrap_or(0.0),
    ])
}

const CIRCLE_SEGMENTS: usize = 32;

/// Sample a curve as a polyline.
fn curve(ctx: &mut Ctx<'_>, e: &Instance, deg: bool) -> Option<Vec<[f64; 3]>> {
    let ex = ctx.ex;
    if e.has_type("POLYLINE") {
        let mut ids = Vec::new();
        e.parameters().get(1)?.collect_refs(&mut ids);
        let pts: Vec<_> = ids
            .iter()
            .filter_map(|id| ex.get(*id))
            .filter_map(point)
            .collect();
        return (pts.len() >= 2).then_some(pts);
    }
    if e.has_type("CIRCLE") {
        let (frame, r) = circle_frame(ex, e)?;
        return Some(sample_arc(
            &frame,
            r,
            0.0,
            std::f64::consts::TAU,
            CIRCLE_SEGMENTS,
        ));
    }
    if e.has_type("TRIMMED_CURVE") {
        // (name, basis_curve, trim_1, trim_2, sense_agreement, master_representation)
        let p = e.parameters();
        let basis = ex.get(p.get(1)?.as_ref()?)?;
        let trim = |i: usize| -> Option<(Option<f64>, Option<[f64; 3]>)> {
            let l = p.get(i)?.as_list()?;
            let mut param = None;
            let mut pt = None;
            for t in l {
                match t {
                    Parameter::Typed { keyword, value } if keyword == "PARAMETER_VALUE" => {
                        param = value.as_f64();
                    }
                    Parameter::Reference(id) => pt = ex.get(*id).and_then(point),
                    _ => {}
                }
            }
            Some((param, pt))
        };
        let (t1, t2) = (trim(2)?, trim(3)?);
        if basis.has_type("CIRCLE") {
            let (frame, r) = circle_frame(ex, basis)?;
            let to_rad = |a: f64| if deg { a.to_radians() } else { a };
            let mut a1 =
                t1.0.map(to_rad)
                    .or_else(|| t1.1.map(|p| angle_of(&frame, p)))?;
            let mut a2 =
                t2.0.map(to_rad)
                    .or_else(|| t2.1.map(|p| angle_of(&frame, p)))?;
            let sense = p.get(4).and_then(Parameter::as_enum) != Some("F");
            if !sense {
                std::mem::swap(&mut a1, &mut a2);
            }
            if a2 <= a1 {
                a2 += std::f64::consts::TAU;
            }
            let n = ((a2 - a1) / std::f64::consts::TAU * CIRCLE_SEGMENTS as f64)
                .ceil()
                .max(1.0) as usize;
            return Some(sample_arc(&frame, r, a1, a2, n));
        }
        if basis.has_type("LINE") {
            // LINE(name, pnt, dir: VECTOR(name, orientation, magnitude))
            let bp = basis.parameters();
            let p0 = ex.get(bp.get(1)?.as_ref()?).and_then(point)?;
            let v = ex.get(bp.get(2)?.as_ref()?)?;
            let vp = v.parameters();
            let d = ex.get(vp.get(1)?.as_ref()?)?;
            let dl = d.parameters().get(1)?.as_list()?;
            let mag = vp.get(2).and_then(Parameter::as_f64).unwrap_or(1.0);
            let dir = [
                dl.first()?.as_f64()? * mag,
                dl.get(1)?.as_f64()? * mag,
                dl.get(2).and_then(Parameter::as_f64).unwrap_or(0.0) * mag,
            ];
            let at = |t: f64| [p0[0] + t * dir[0], p0[1] + t * dir[1], p0[2] + t * dir[2]];
            let a = t1.1.or_else(|| t1.0.map(at))?;
            let b = t2.1.or_else(|| t2.0.map(at))?;
            return Some(vec![a, b]);
        }
        ctx.warn(
            format!(
                "trimmed curve #{} on unsupported basis {}",
                e.id,
                basis.type_key()
            ),
            Some(e.id),
        );
        return None;
    }
    if e.has_type("COMPOSITE_CURVE") {
        // (name, segments: COMPOSITE_CURVE_SEGMENT(transition, same_sense, parent_curve), self_intersect)
        let mut ids = Vec::new();
        e.parameters().get(1)?.collect_refs(&mut ids);
        let mut out = Vec::new();
        for sid in ids {
            let Some(seg) = ex.get(sid) else { continue };
            let Some(parent) = seg
                .parameters()
                .get(2)
                .and_then(Parameter::as_ref)
                .and_then(|i| ex.get(i))
            else {
                continue;
            };
            if let Some(pl) = curve(ctx, parent, deg) {
                out.extend(pl);
            }
        }
        return (out.len() >= 2).then_some(out);
    }
    if e.has_type("B_SPLINE_CURVE_WITH_KNOTS")
        || e.type_names().any(|t| t.starts_with("B_SPLINE_CURVE"))
    {
        // Control polygon as an approximation; recorded as such.
        let seg = e
            .segments
            .iter()
            .find(|s| s.keyword == "B_SPLINE_CURVE")
            .or_else(|| e.segments.first())?;
        let mut ids = Vec::new();
        seg.parameters.get(2)?.collect_refs(&mut ids);
        let pts: Vec<_> = ids
            .iter()
            .filter_map(|id| ex.get(*id))
            .filter_map(point)
            .collect();
        return (pts.len() >= 2).then_some(pts);
    }
    None
}

fn circle_frame(ex: &Exchange, c: &Instance) -> Option<(Frame, f64)> {
    let p = c.parameters();
    let pl = ex.get(p.get(1)?.as_ref()?)?;
    let placement = placement(ex, pl)?;
    let frame = Frame::from_placement(&placement)?;
    Some((frame, p.get(2)?.as_f64()?))
}

fn sample_arc(frame: &Frame, r: f64, a1: f64, a2: f64, n: usize) -> Vec<[f64; 3]> {
    (0..=n)
        .map(|i| {
            let a = a1 + (a2 - a1) * i as f64 / n as f64;
            frame.apply([r * a.cos(), r * a.sin(), 0.0])
        })
        .collect()
}

fn angle_of(frame: &Frame, p: [f64; 3]) -> f64 {
    let d = [
        p[0] - frame.origin[0],
        p[1] - frame.origin[1],
        p[2] - frame.origin[2],
    ];
    let x = d[0] * frame.x[0] + d[1] * frame.x[1] + d[2] * frame.x[2];
    let y = d[0] * frame.y[0] + d[1] * frame.y[1] + d[2] * frame.y[2];
    y.atan2(x)
}

/// Placement of an `annotation_plane` item: a `plane` or a placement.
pub(crate) fn plane_placement(ex: &Exchange, item: &Instance) -> Option<Placement> {
    if item.has_type("AXIS2_PLACEMENT_3D") {
        return placement(ex, item);
    }
    if item.has_type("PLANE") {
        let p = ex.get(item.parameters().get(1)?.as_ref()?)?;
        return placement(ex, p);
    }
    None
}

/// Round a placement for identity hashing.
pub(crate) fn placement_key(p: &Placement, quantum: f64) -> String {
    let mut s = format!(
        "{},{},{}",
        round(p.origin[0], quantum),
        round(p.origin[1], quantum),
        round(p.origin[2], quantum)
    );
    if let Some(a) = &p.axis {
        s.push_str(&format!(";{:.4},{:.4},{:.4}", a.x, a.y, a.z));
    }
    s
}
