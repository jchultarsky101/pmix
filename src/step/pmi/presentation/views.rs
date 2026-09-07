//! Saved views: cameras and the annotations they show.

use std::collections::{BTreeSet, HashSet};

use super::super::Ctx;
use super::super::datums::placement;
use super::Index;
use super::geometry::{placement_key, rounding_quantum};
use crate::model::{Camera, Placement, Projection, SavedView, Unmapped};
use crate::step::p21::{Exchange, Id, Parameter};

const CAMERA_TYPES: &[&str] = &["CAMERA_MODEL_D3", "CAMERA_MODEL_D3_MULTI_CLIPPING"];

pub(crate) fn walk(ctx: &mut Ctx<'_>, ix: &Index) {
    let ex = ctx.ex;
    let quantum = rounding_quantum(ex);

    // Camera to the representations (draughting models) that list it, and
    // to the model_geometric_view that names it.
    let cameras: Vec<Id> = ex
        .instances()
        .filter(|i| CAMERA_TYPES.iter().any(|t| i.has_type(t)))
        .map(|i| i.id)
        .collect();
    tracing::debug!(count = cameras.len(), "cameras");
    let mut reps_with_camera: std::collections::HashMap<Id, Vec<Id>> = Default::default();
    for r in ex
        .instances()
        .filter(|i| i.has_type("REPRESENTATION") || i.has_type("DRAUGHTING_MODEL"))
    {
        let Some(seg) = r.segment("REPRESENTATION").or_else(|| r.segments.first()) else {
            continue;
        };
        let mut items = Vec::new();
        if let Some(p) = seg.parameters.get(1) {
            p.collect_refs(&mut items);
        }
        for c in &cameras {
            if items.contains(c) {
                reps_with_camera.entry(*c).or_default().push(r.id);
            }
        }
    }
    let mut mgv_of: std::collections::HashMap<Id, Vec<(Id, bool, String)>> = Default::default();
    for keyword in ["MODEL_GEOMETRIC_VIEW", "DEFAULT_MODEL_GEOMETRIC_VIEW"] {
        for v in ex.of_type(keyword) {
            let mut refs = Vec::new();
            for p in v.parameters() {
                p.collect_refs(&mut refs);
            }
            let cam = refs.iter().copied().find(|r| cameras.contains(r));
            let Some(cam) = cam else { continue };
            for r in &refs {
                if ex
                    .get(*r)
                    .is_some_and(|i| i.has_type("DRAUGHTING_MODEL") || i.has_type("REPRESENTATION"))
                {
                    reps_with_camera.entry(cam).or_default().push(*r);
                }
            }
            let name = v
                .parameters()
                .first()
                .and_then(Parameter::as_str)
                .unwrap_or_default()
                .trim()
                .to_owned();
            mgv_of.entry(cam).or_default().push((
                v.id,
                keyword == "DEFAULT_MODEL_GEOMETRIC_VIEW",
                name,
            ));
        }
    }

    for cam_id in cameras {
        let Some(cam) = ex.get(cam_id) else { continue };
        let p = cam.parameters();
        let mut unmapped = Vec::new();
        let mut source_refs = vec![format!("#{cam_id}")];
        let camera_name = p
            .first()
            .and_then(Parameter::as_str)
            .unwrap_or_default()
            .trim()
            .to_owned();
        let Some(placement) = p
            .get(1)
            .and_then(Parameter::as_ref)
            .and_then(|i| ex.get(i))
            .and_then(|i| placement(ex, i))
        else {
            ctx.warn(format!("camera #{cam_id} has no placement"), Some(cam_id));
            continue;
        };
        let mut camera = Camera {
            placement,
            projection: Projection::Other(String::new()),
            view_plane_distance: None,
            view_window: None,
            front_clip: None,
            back_clip: None,
        };
        if let Some(vv) = p.get(2).and_then(Parameter::as_ref).and_then(|i| ex.get(i)) {
            // (projection_type, projection_point, view_plane_distance,
            //  front_plane_distance, front_plane_clipping, back_plane_distance,
            //  back_plane_clipping, view_volume_sides_clipping, view_window)
            let q = vv.parameters();
            camera.projection = q
                .first()
                .and_then(Parameter::as_enum)
                .map(|e| Projection::parse(&e.to_ascii_lowercase()))
                .unwrap_or(Projection::Other(String::new()));
            camera.view_plane_distance = q.get(2).and_then(Parameter::as_f64);
            if q.get(4).and_then(Parameter::as_enum) == Some("T") {
                camera.front_clip = q.get(3).and_then(Parameter::as_f64);
            }
            if q.get(6).and_then(Parameter::as_enum) == Some("T") {
                camera.back_clip = q.get(5).and_then(Parameter::as_f64);
            }
            if let Some(w) = q.get(8).and_then(Parameter::as_ref).and_then(|i| ex.get(i)) {
                let wp = w.parameters();
                if let (Some(x), Some(y)) = (
                    wp.get(1).and_then(Parameter::as_f64),
                    wp.get(2).and_then(Parameter::as_f64),
                ) {
                    camera.view_window = Some([x, y]);
                }
            }
            ctx.consume(vv.id);
            source_refs.push(format!("#{}", vv.id));
        }

        // Clipping planes.
        let mut clipping_planes = Vec::new();
        if cam.has_type("CAMERA_MODEL_D3_MULTI_CLIPPING") {
            if let Some(c) = p.get(3) {
                collect_planes(ex, c, &mut clipping_planes, &mut unmapped, 0);
            }
        }

        // Membership.
        let mut ann: BTreeSet<String> = BTreeSet::new();
        let mut seen = HashSet::new();
        for rep in reps_with_camera.get(&cam_id).cloned().unwrap_or_default() {
            collect_annotations(ex, ix, rep, &mut ann, &mut seen);
            ctx.consume(rep);
        }
        let mut default = false;
        let mut name = String::new();
        for (mgv, is_default, n) in mgv_of.get(&cam_id).cloned().unwrap_or_default() {
            ctx.consume(mgv);
            source_refs.push(format!("#{mgv}"));
            default |= is_default;
            if name.is_empty() {
                name = n;
            }
        }
        if name.is_empty() {
            name = camera_name;
        }
        let annotations: Vec<String> = ann.into_iter().collect();
        let id = ctx.ids.make(
            "view",
            &[
                &name,
                &placement_key(&camera.placement, quantum),
                &annotations.join(","),
            ],
        );
        for a in &annotations {
            if let Some(rec) = ctx.annotations.iter_mut().find(|r| &r.id == a) {
                rec.views.push(id.clone());
            }
        }
        ctx.consume(cam_id);
        ctx.views.push(SavedView {
            id,
            name,
            camera,
            clipping_planes,
            annotations,
            default,
            unmapped,
            source_refs,
        });
    }
}

fn collect_planes(
    ex: &Exchange,
    p: &Parameter,
    out: &mut Vec<Placement>,
    unmapped: &mut Vec<Unmapped>,
    depth: usize,
) {
    if depth > 8 {
        return;
    }
    let mut ids = Vec::new();
    p.collect_refs(&mut ids);
    for id in ids {
        let Some(i) = ex.get(id) else { continue };
        if i.has_type("PLANE") {
            if let Some(pl) = i
                .parameters()
                .get(1)
                .and_then(Parameter::as_ref)
                .and_then(|x| ex.get(x))
                .and_then(|x| placement(ex, x))
            {
                out.push(pl);
            }
        } else if i.has_type("CAMERA_MODEL_D3_MULTI_CLIPPING_UNION")
            || i.has_type("CAMERA_MODEL_D3_MULTI_CLIPPING_INTERSECTION")
        {
            unmapped.push(Unmapped {
                attribute: "clipping_boolean".into(),
                raw: i.type_key().to_ascii_lowercase(),
            });
            if let Some(q) = i.parameters().get(1) {
                collect_planes(ex, q, out, unmapped, depth + 1);
            }
        }
    }
}

/// Annotations reachable from a representation's items.
fn collect_annotations(
    ex: &Exchange,
    ix: &Index,
    rep_id: Id,
    out: &mut BTreeSet<String>,
    seen: &mut HashSet<Id>,
) {
    if !seen.insert(rep_id) {
        return;
    }
    let Some(rep) = ex.get(rep_id) else { return };
    let Some(seg) = rep
        .segment("REPRESENTATION")
        .or_else(|| rep.segments.first())
    else {
        return;
    };
    let mut items = Vec::new();
    if let Some(p) = seg.parameters.get(1) {
        p.collect_refs(&mut items);
    }
    for item in items {
        visit_item(ex, ix, item, out, seen);
    }
}

fn visit_item(
    ex: &Exchange,
    ix: &Index,
    item: Id,
    out: &mut BTreeSet<String>,
    seen: &mut HashSet<Id>,
) {
    if let Some(a) = ix.annotation_of.get(&item) {
        out.insert(a.clone());
        return;
    }
    let Some(i) = ex.get(item) else { return };
    if i.has_type("ANNOTATION_PLANE") {
        let mut elems = Vec::new();
        if let Some(p) = i.parameters().get(3) {
            p.collect_refs(&mut elems);
        }
        for e in elems {
            if let Some(a) = ix.annotation_of.get(&e) {
                out.insert(a.clone());
            }
        }
    } else if i.has_type("MAPPED_ITEM") {
        if let Some(map) = i
            .parameters()
            .get(1)
            .and_then(Parameter::as_ref)
            .and_then(|m| ex.get(m))
        {
            if let Some(rep) = map.parameters().get(1).and_then(Parameter::as_ref) {
                collect_annotations(ex, ix, rep, out, seen);
            }
        }
    } else if i.has_type("DRAUGHTING_CALLOUT") {
        // A callout merged into another: resolve through the merge.
        if let Some(main) = ix.merged_into.get(&item) {
            if let Some(a) = ix.annotation_of.get(main) {
                out.insert(a.clone());
            }
        }
    }
}
