//! Annotations: draughting callouts and their occurrences.

use std::collections::BTreeSet;

use super::super::Ctx;
use super::super::datums::placement;
use super::super::features::feature_for;
use super::Index;
use super::geometry::{self, Geometry, placement_key, plane_placement, rounding_quantum};
use crate::ExtractOptions;
use crate::model::{
    Annotation, AnnotationKind, Leader, Measure, Part, PartForm, Placeholder, Style, TextOrigin,
    Unmapped,
};
use crate::step::p21::{Id, Instance, Parameter};

const OCCURRENCE_TYPES: &[&str] = &[
    "TESSELLATED_ANNOTATION_OCCURRENCE",
    "ANNOTATION_CURVE_OCCURRENCE",
    "ANNOTATION_FILL_AREA_OCCURRENCE",
    "ANNOTATION_PLACEHOLDER_OCCURRENCE",
    "ANNOTATION_PLACEHOLDER_OCCURRENCE_WITH_LEADER_LINE",
    "ANNOTATION_TEXT_OCCURRENCE",
    "ANNOTATION_OCCURRENCE",
];

pub(crate) fn is_occurrence(inst: &Instance) -> bool {
    OCCURRENCE_TYPES.iter().any(|t| inst.has_type(t))
}

pub(crate) fn walk(ctx: &mut Ctx<'_>, ix: &mut Index, options: &ExtractOptions) {
    let ex = ctx.ex;
    let quantum = rounding_quantum(ex);
    let deg = ctx_angle_is_degrees(ctx);

    // Callouts that are not merged into another.
    let callouts: Vec<Id> = ex
        .of_type("DRAUGHTING_CALLOUT")
        .map(|c| c.id)
        .filter(|id| !ix.merged_into.contains_key(id))
        .collect();
    tracing::debug!(count = callouts.len(), "draughting callouts");
    for id in callouts {
        // Gather related callouts transitively (relationships can chain).
        let mut members = vec![id];
        let mut rel_ids = Vec::new();
        let mut queue = vec![id];
        while let Some(c) = queue.pop() {
            for (rel, related) in ix.merged.get(&c).cloned().unwrap_or_default() {
                if !members.contains(&related) {
                    members.push(related);
                    rel_ids.push(rel);
                    queue.push(related);
                }
            }
        }
        let mut occurrences = Vec::new();
        for m in &members {
            if let Some(c) = ex.get(*m) {
                let mut items = Vec::new();
                if let Some(p) = c.parameters().get(1) {
                    p.collect_refs(&mut items);
                }
                occurrences.extend(items);
            }
        }
        let label = ex
            .get(id)
            .and_then(|c| c.parameters().first())
            .and_then(Parameter::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        let mut source_refs: Vec<String> = members.iter().map(|m| format!("#{m}")).collect();
        source_refs.extend(rel_ids.iter().map(|r| format!("#{r}")));
        for m in &members {
            ctx.consume(*m);
        }
        for r in rel_ids {
            ctx.consume(r);
        }
        build(
            ctx,
            ix,
            options,
            quantum,
            deg,
            &members,
            &occurrences,
            label,
            source_refs,
        );
    }

    // Planes with no elements carry no PMI; consume them so they are not
    // reported as unmapped.
    let empty_planes: Vec<Id> = ex
        .of_type("ANNOTATION_PLANE")
        .filter(|pl| {
            pl.parameters()
                .get(3)
                .and_then(Parameter::as_list)
                .is_none_or(|l| l.is_empty())
        })
        .map(|pl| pl.id)
        .collect();
    for id in empty_planes {
        ctx.consume(id);
    }

    // Occurrences outside any callout.
    let standalone: Vec<Id> = ex
        .instances()
        .filter(|i| is_occurrence(i) && !ix.callout_of.contains_key(&i.id))
        .map(|i| i.id)
        .collect();
    tracing::debug!(
        count = standalone.len(),
        "standalone annotation occurrences"
    );
    for id in standalone {
        let label = ex
            .get(id)
            .and_then(|o| o.parameters().first())
            .and_then(Parameter::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        build(
            ctx,
            ix,
            options,
            quantum,
            deg,
            &[id],
            &[id],
            label,
            vec![format!("#{id}")],
        );
    }
}

fn ctx_angle_is_degrees(ctx: &mut Ctx<'_>) -> bool {
    super::super::units::document_units(ctx)
        .angle
        .as_deref()
        .is_some_and(|u| u == "deg")
}

#[allow(clippy::too_many_arguments)]
fn build(
    ctx: &mut Ctx<'_>,
    ix: &mut Index,
    options: &ExtractOptions,
    quantum: f64,
    deg: bool,
    elements: &[Id],
    occurrences: &[Id],
    label: Option<String>,
    mut source_refs: Vec<String>,
) {
    let ex = ctx.ex;
    let mut unmapped = Vec::new();
    let mut parts = Vec::new();
    let mut placeholder = None;
    let mut leaders = Vec::new();
    let mut style: Option<Style> = None;
    let mut explicit_text: Option<String> = None;
    let mut all = Geometry::default();

    for oid in occurrences {
        let Some(o) = ex.get(*oid) else {
            ctx.warn(format!("callout lists undefined occurrence #{oid}"), None);
            continue;
        };
        if !is_occurrence(o) {
            unmapped.push(Unmapped {
                attribute: "callout_content".into(),
                raw: o.to_string(),
            });
            continue;
        }
        ctx.consume(*oid);
        source_refs.push(format!("#{oid}"));
        let p = o.parameters();
        let item = p
            .get(2)
            .and_then(Parameter::as_ref)
            .and_then(|id| ex.get(id));
        if style.is_none() {
            style = style_of(ctx, ix, o);
        }
        let mut geo = Geometry::default();
        let mut kind = AnnotationKind::Other(String::new());
        let form;
        if o.has_type("TESSELLATED_ANNOTATION_OCCURRENCE") {
            form = PartForm::Tessellated;
            if let Some(set) = item {
                ctx.consume(set.id);
                kind = kind_of_set(set);
                geo = geometry::tessellated(ctx, set);
            }
        } else if o.has_type("ANNOTATION_PLACEHOLDER_OCCURRENCE")
            || o.has_type("ANNOTATION_PLACEHOLDER_OCCURRENCE_WITH_LEADER_LINE")
        {
            form = PartForm::Placeholder;
            kind = AnnotationKind::Placeholder;
            let mut ph = Placeholder {
                placement: None,
                box_size: None,
                role: p
                    .get(3)
                    .and_then(Parameter::as_enum)
                    .map(|r| r.to_ascii_lowercase()),
                text_height: p
                    .get(4)
                    .and_then(Parameter::as_f64)
                    .map(|h| Measure::new(h, ctx_length_unit(ctx))),
            };
            if let Some(set) = item {
                ctx.consume(set.id);
                let mut ids = Vec::new();
                if let Some(e) = set.parameters().get(1) {
                    e.collect_refs(&mut ids);
                }
                for id in ids {
                    let Some(e) = ex.get(id) else { continue };
                    if e.has_type("AXIS2_PLACEMENT_3D") {
                        ph.placement = placement(ex, e);
                    } else if e.has_type("PLANAR_BOX") {
                        let bp = e.parameters();
                        if let (Some(w), Some(h)) = (
                            bp.get(1).and_then(Parameter::as_f64),
                            bp.get(2).and_then(Parameter::as_f64),
                        ) {
                            ph.box_size = Some([w, h]);
                        }
                        if ph.placement.is_none() {
                            ph.placement = bp
                                .get(3)
                                .and_then(Parameter::as_ref)
                                .and_then(|i| ex.get(i))
                                .and_then(|i| placement(ex, i));
                        }
                    }
                }
            }
            let mut leader_ids = Vec::new();
            if let Some(l) = p.get(5) {
                l.collect_refs(&mut leader_ids);
            }
            for lid in leader_ids {
                let Some(l) = ex.get(lid) else { continue };
                if let Some(leader) = leader(ex, l) {
                    ctx.consume(lid);
                    leaders.push(leader);
                }
            }
            if placeholder.is_none() {
                placeholder = Some(ph);
            } else {
                unmapped.push(Unmapped {
                    attribute: "extra_placeholder".into(),
                    raw: o.to_string(),
                });
            }
        } else if o.has_type("ANNOTATION_TEXT_OCCURRENCE") {
            form = PartForm::Text;
            kind = AnnotationKind::Note;
            if let Some(t) = item {
                ctx.consume(t.id);
                if let Some(s) = text_literal(ctx, t) {
                    explicit_text = Some(s);
                }
            }
        } else if o.has_type("ANNOTATION_FILL_AREA_OCCURRENCE") {
            form = PartForm::FillArea;
            if let Some(set) = item {
                ctx.consume(set.id);
                kind = kind_of_set(set);
                geo = geometry::curve_set(ctx, set, deg);
            }
        } else {
            form = PartForm::Polyline;
            if let Some(set) = item {
                ctx.consume(set.id);
                kind = kind_of_set(set);
                geo = if set.has_type("TESSELLATED_GEOMETRIC_SET")
                    || set.has_type("TESSELLATED_ITEM")
                {
                    geometry::tessellated(ctx, set)
                } else {
                    geometry::curve_set(ctx, set, deg)
                };
            }
        }
        let summary = geo.summary(quantum);
        all.extend(&geo);
        parts.push(Part {
            form,
            kind,
            geometry: summary,
            polylines: (options.presentation_geometry && !geo.polylines.is_empty())
                .then(|| geo.polylines.clone()),
            vertices: (options.presentation_geometry && !geo.vertices.is_empty())
                .then(|| geo.vertices.clone()),
            triangles: (options.presentation_geometry && !geo.triangles.is_empty())
                .then(|| geo.triangles.clone()),
            source_ref: format!("#{oid}"),
        });
    }

    // Plane: the first one found is the annotation's; others are consumed
    // and noted when they differ.
    let mut plane: Option<crate::model::Placement> = None;
    let mut seen_planes = BTreeSet::new();
    for e in elements.iter().chain(occurrences.iter()) {
        for (pl_id, item_id) in ix.plane_of.get(e).cloned().unwrap_or_default() {
            if !seen_planes.insert(pl_id) {
                continue;
            }
            let candidate = ex.get(item_id).and_then(|item| plane_placement(ex, item));
            match (&plane, candidate) {
                (None, c) => plane = c,
                (Some(p), Some(c)) if placement_key(p, quantum) != placement_key(&c, quantum) => {
                    unmapped.push(Unmapped {
                        attribute: "extra_plane".into(),
                        raw: format!("#{pl_id}"),
                    });
                }
                _ => {}
            }
            ctx.consume(pl_id);
            source_refs.push(format!("#{pl_id}"));
        }
    }

    // Links to semantic records and features.
    let mut semantic: BTreeSet<String> = BTreeSet::new();
    let mut features: BTreeSet<String> = BTreeSet::new();
    let mut definitions: Vec<Id> = Vec::new();
    for e in elements.iter().chain(occurrences.iter()) {
        for (dmia, def) in ix.links.get(e).cloned().unwrap_or_default() {
            ctx.consume(dmia);
            source_refs.push(format!("#{dmia}"));
            definitions.push(def);
        }
    }
    for def in &definitions {
        if let Some(Some(id)) = ctx.tolerance_ids.get(def) {
            semantic.insert(id.clone());
        } else if let Some(id) = ctx.dimension_ids.get(def) {
            semantic.insert(id.clone());
        } else if let Some(Some(id)) = ctx.datum_ids.get(def) {
            semantic.insert(id.clone());
        } else if let Some(Some(id)) = ctx.datum_system_ids.get(def) {
            semantic.insert(id.clone());
        } else {
            match ex.get(*def) {
                Some(d) if d.has_type("PROPERTY_DEFINITION") => {}
                Some(d) if d.has_type("DATUM_FEATURE") => {
                    // The datum feature's datum is the semantic record.
                    let mut linked = false;
                    for (datum_id, rid) in ctx.datum_ids.clone() {
                        if let Some(rid) = rid {
                            if ctx
                                .datum_links
                                .get(&datum_id)
                                .is_some_and(|l| l.iter().any(|(_, r)| *r == *def))
                            {
                                semantic.insert(rid);
                                linked = true;
                            }
                        }
                    }
                    if !linked {
                        if let Some(f) = feature_for(ctx, *def) {
                            features.insert(f);
                        }
                    }
                }
                Some(_) => {
                    if let Some(f) = feature_for(ctx, *def) {
                        features.insert(f);
                    }
                }
                None => ctx.warn(format!("association references undefined #{def}"), None),
            }
        }
    }

    // Kind: from the parts, else from the linked semantic record.
    let mut kind = parts
        .iter()
        .map(|p| p.kind.clone())
        .find(|k| !matches!(k, AnnotationKind::Placeholder | AnnotationKind::Other(_)))
        .or_else(|| {
            parts
                .iter()
                .map(|p| p.kind.clone())
                .find(|k| matches!(k, AnnotationKind::Other(s) if !s.is_empty()))
        });
    if kind.is_none() {
        kind = semantic.iter().find_map(|sid| semantic_kind(ctx, sid));
    }
    let kind = kind.unwrap_or_else(|| {
        if placeholder.is_some() {
            AnnotationKind::Placeholder
        } else {
            AnnotationKind::Other(String::new())
        }
    });

    // Text.
    let mut text = explicit_text.take();
    let mut text_origin = text.as_ref().map(|_| TextOrigin::Explicit);
    if text.is_none() {
        for def in &definitions {
            if let Some(t) = validation_text(ctx, *def) {
                text = Some(t);
                text_origin = Some(TextOrigin::Explicit);
                break;
            }
        }
    }
    if text.is_none() && semantic.len() == 1 {
        let sid = semantic.iter().next().unwrap().clone();
        if let Some(t) = semantic_text(ctx, &sid) {
            text = Some(t);
            text_origin = Some(TextOrigin::Semantic);
        }
    }

    let geometry_summary = all.summary(quantum);
    let mut seen_refs = BTreeSet::new();
    source_refs.retain(|r| seen_refs.insert(r.clone()));
    let semantic: Vec<String> = semantic.into_iter().collect();
    let features: Vec<String> = features.into_iter().collect();
    let id = ctx.ids.make(
        "ann",
        &[
            kind.as_str(),
            text.as_deref().unwrap_or(""),
            &plane
                .as_ref()
                .map(|p| placement_key(p, quantum))
                .unwrap_or_default(),
            &semantic.join(","),
            &features.join(","),
            &geometry_summary.hash,
        ],
    );
    for e in elements {
        ix.annotation_of.insert(*e, id.clone());
    }
    for o in occurrences {
        ix.annotation_of.insert(*o, id.clone());
    }
    for sid in &semantic {
        ctx.link_presentation(sid, &id);
    }
    ctx.annotations.push(Annotation {
        id,
        kind,
        label,
        text,
        text_origin,
        plane,
        placeholder,
        leaders,
        geometry: geometry_summary,
        parts,
        style,
        semantic,
        features,
        views: Vec::new(),
        attributes: Default::default(),
        unmapped,
        source_refs,
    });
}

fn ctx_length_unit(ctx: &mut Ctx<'_>) -> String {
    super::super::units::document_units(ctx)
        .length
        .unwrap_or_default()
}

/// The presented PMI type from a set's name. In the complex instance
/// form the name lives on the `REPRESENTATION_ITEM` segment.
fn kind_of_set(set: &Instance) -> AnnotationKind {
    let name = set
        .attr("REPRESENTATION_ITEM", 0)
        .or_else(|| {
            set.segments
                .iter()
                .flat_map(|seg| seg.parameters.iter())
                .find(|p| p.as_str().is_some())
        })
        .and_then(Parameter::as_str)
        .unwrap_or_default();
    AnnotationKind::from_presented_name(name)
}

fn leader(ex: &crate::step::p21::Exchange, l: &Instance) -> Option<Leader> {
    if !l.has_type("ANNOTATION_TO_MODEL_LEADER_LINE") && !l.has_type("AUXILIARY_LEADER_LINE") {
        return None;
    }
    let mut ids = Vec::new();
    l.parameters().get(1)?.collect_refs(&mut ids);
    let mut points = Vec::new();
    let mut terminator = None;
    for id in ids {
        let Some(pt) = ex.get(id) else { continue };
        let p = pt.parameters();
        if let Some(c) = p.get(1).and_then(Parameter::as_list) {
            if let (Some(x), Some(y)) = (
                c.first().and_then(Parameter::as_f64),
                c.get(1).and_then(Parameter::as_f64),
            ) {
                points.push([x, y, c.get(2).and_then(Parameter::as_f64).unwrap_or(0.0)]);
            }
        }
        if let Some(sym) = p
            .get(2)
            .and_then(Parameter::as_enum)
            .filter(|s| *s != "NONE")
        {
            terminator = Some(sym.to_ascii_lowercase());
        }
    }
    Some(Leader {
        points,
        terminator,
        auxiliary: l.has_type("AUXILIARY_LEADER_LINE"),
    })
}

fn text_literal(ctx: &mut Ctx<'_>, t: &Instance) -> Option<String> {
    if t.has_type("TEXT_LITERAL") {
        return t
            .parameters()
            .get(1)
            .and_then(Parameter::as_str)
            .map(str::to_owned);
    }
    if t.has_type("COMPOSITE_TEXT") {
        let ex = ctx.ex;
        let mut ids = Vec::new();
        t.parameters().get(1)?.collect_refs(&mut ids);
        let parts: Vec<String> = ids
            .iter()
            .filter_map(|id| ex.get(*id))
            .filter_map(|c| text_literal(ctx, c))
            .collect();
        return (!parts.is_empty()).then(|| parts.join("\n"));
    }
    None
}

/// Style from the occurrence's `styles` and its layer.
fn style_of(ctx: &mut Ctx<'_>, ix: &Index, o: &Instance) -> Option<Style> {
    let ex = ctx.ex;
    let mut style = Style::default();
    let mut ids = Vec::new();
    if let Some(s) = o.parameters().get(1) {
        s.collect_refs(&mut ids);
    }
    for psa in ids.iter().filter_map(|id| ex.get(*id)) {
        let mut styles = Vec::new();
        if let Some(s) = psa.parameters().first() {
            s.collect_refs(&mut styles);
        }
        for st in styles.iter().filter_map(|id| ex.get(*id)) {
            if st.has_type("CURVE_STYLE") {
                let p = st.parameters();
                if style.line_font.is_none() {
                    style.line_font = p
                        .get(1)
                        .and_then(Parameter::as_ref)
                        .and_then(|id| ex.get(id))
                        .and_then(|f| f.parameters().first())
                        .and_then(Parameter::as_str)
                        .map(str::to_owned);
                }
                if style.line_width.is_none() {
                    style.line_width = p
                        .get(2)
                        .and_then(Parameter::as_f64)
                        .map(|w| Measure::new(w, ctx_length_unit(ctx)));
                }
                if style.colour.is_none() {
                    style.colour = p
                        .get(3)
                        .and_then(Parameter::as_ref)
                        .and_then(|id| ex.get(id))
                        .and_then(colour);
                }
            } else if st.has_type("FILL_AREA_STYLE") {
                let mut fs = Vec::new();
                if let Some(s) = st.parameters().get(1) {
                    s.collect_refs(&mut fs);
                }
                for f in fs.iter().filter_map(|id| ex.get(*id)) {
                    if style.colour.is_none() {
                        style.colour = f
                            .parameters()
                            .get(1)
                            .and_then(Parameter::as_ref)
                            .and_then(|id| ex.get(id))
                            .and_then(colour);
                    }
                }
            }
        }
    }
    style.layer = ix
        .layer_of
        .get(&o.id)
        .or_else(|| ix.callout_of.get(&o.id).and_then(|c| ix.layer_of.get(c)))
        .cloned();
    (style != Style::default()).then_some(style)
}

fn colour(c: &Instance) -> Option<String> {
    if c.has_type("COLOUR_RGB") {
        let p = c.parameters();
        let ch = |i: usize| {
            p.get(i)
                .and_then(Parameter::as_f64)
                .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
        };
        return Some(format!("#{:02x}{:02x}{:02x}", ch(1)?, ch(2)?, ch(3)?));
    }
    if c.has_type("DRAUGHTING_PRE_DEFINED_COLOUR") || c.has_type("PRE_DEFINED_COLOUR") {
        return c
            .parameters()
            .first()
            .and_then(Parameter::as_str)
            .map(str::to_owned);
    }
    None
}

/// Explicit text from the `pmi validation property` of a semantic entity
/// (`equivalent unicode string`).
fn validation_text(ctx: &mut Ctx<'_>, def: Id) -> Option<String> {
    let ex = ctx.ex;
    for (pd_id, pdr_id, rep_id) in ctx.property_reprs.get(&def).cloned().unwrap_or_default() {
        let Some(pd) = ex.get(pd_id) else { continue };
        let is_pmi = pd
            .parameters()
            .first()
            .and_then(Parameter::as_str)
            .is_some_and(|n| n.eq_ignore_ascii_case("pmi validation property"));
        if !is_pmi {
            continue;
        }
        let Some(rep) = ex.get(rep_id) else { continue };
        let mut items = Vec::new();
        if let Some(i) = rep.parameters().get(1) {
            i.collect_refs(&mut items);
        }
        for it in items.iter().filter_map(|id| ex.get(*id)) {
            if it.has_type("DESCRIPTIVE_REPRESENTATION_ITEM") {
                let p = it.parameters();
                if p.first()
                    .and_then(Parameter::as_str)
                    .is_some_and(|n| n.eq_ignore_ascii_case("equivalent unicode string"))
                {
                    if let Some(raw) = p.get(1).and_then(Parameter::as_str) {
                        ctx.consume(pd_id);
                        ctx.consume(pdr_id);
                        return Some(decode_pmi_string(raw));
                    }
                }
            }
        }
    }
    None
}

/// Decode the CAx-IF PMI Unicode String markup: `\w` separates
/// compartments, `\n` breaks lines, `\x` opens a nested frame. The
/// leading type tag (DIM, FCF, DTM, ...) is dropped: the annotation kind
/// already carries it.
fn decode_pmi_string(raw: &str) -> String {
    let text = raw
        .replace("\\\\w", " | ")
        .replace("\\\\n", "\n")
        .replace("\\\\x", " ")
        .replace("\\w", " | ")
        .replace("\\n", "\n")
        .replace("\\x", " ");
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let tags = [
        "DIM | ", "FCF | ", "DTM | ", "DTT | ", "NOTE | ", "TXT | ", "SFT | ",
    ];
    for t in tags {
        if let Some(rest) = text.strip_prefix(t) {
            return rest.to_owned();
        }
    }
    text
}

fn semantic_kind(ctx: &Ctx<'_>, sid: &str) -> Option<AnnotationKind> {
    if let Some(t) = ctx.tolerances.iter().find(|t| t.meta.id == sid) {
        return Some(AnnotationKind::parse(t.kind.as_str()));
    }
    if let Some(d) = ctx.dimensions.iter().find(|d| d.meta.id == sid) {
        use crate::model::DimensionSubtype::*;
        return Some(match d.subtype {
            Diameter | SphericalDiameter => AnnotationKind::DiameterDimension,
            Radius | SphericalRadius => AnnotationKind::RadialDimension,
            Angle => AnnotationKind::AngularDimension,
            CurveLength | CurvedDistance => AnnotationKind::CurvedDimension,
            _ => AnnotationKind::LinearDimension,
        });
    }
    if ctx.datums.iter().any(|d| d.meta.id == sid) {
        return Some(AnnotationKind::Datum);
    }
    None
}

fn semantic_text(ctx: &Ctx<'_>, sid: &str) -> Option<String> {
    if let Some(t) = ctx.tolerances.iter().find(|t| t.meta.id == sid) {
        return t.text.clone();
    }
    if let Some(d) = ctx.dimensions.iter().find(|d| d.meta.id == sid) {
        return d.text.clone();
    }
    if let Some(d) = ctx.datums.iter().find(|d| d.meta.id == sid) {
        return Some(d.label.clone());
    }
    if let Some(s) = ctx.datum_systems.iter().find(|s| s.meta.id == sid) {
        return Some(s.text.clone());
    }
    None
}
