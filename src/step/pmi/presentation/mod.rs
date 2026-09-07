//! Presentation layer (ADR 0003): annotations and saved views.

mod annotations;
mod geometry;
mod views;

pub(crate) use geometry::sample_curve;

use std::collections::HashMap;

use super::Ctx;
use crate::ExtractOptions;
use crate::step::p21::{Id, Parameter};

/// Reverse indexes specific to the presentation walkers.
#[derive(Default)]
pub(crate) struct Index {
    /// Occurrence id to the callout that contains it.
    pub callout_of: HashMap<Id, Id>,
    /// Callout id to the related callouts merged into it, with the
    /// relationship ids.
    pub merged: HashMap<Id, Vec<(Id, Id)>>,
    /// Callout ids that are merged into another and must not stand alone.
    pub merged_into: HashMap<Id, Id>,
    /// Element (callout or occurrence) to every `(annotation_plane id,
    /// placement item id)` listing it.
    pub plane_of: HashMap<Id, Vec<(Id, Id)>>,
    /// Identified item (callout or occurrence) to `(dmia id, definition id)`.
    pub links: HashMap<Id, Vec<(Id, Id)>>,
    /// Item to layer name.
    pub layer_of: HashMap<Id, String>,
    /// Callout or standalone occurrence id to annotation record id.
    pub annotation_of: HashMap<Id, String>,
}

/// Walk callouts, occurrences, planes, links, and cameras.
pub(crate) fn walk(ctx: &mut Ctx<'_>, options: &ExtractOptions) {
    let mut index = build_index(ctx);
    annotations::walk(ctx, &mut index, options);
    views::walk(ctx, &index);
}

fn build_index(ctx: &mut Ctx<'_>) -> Index {
    let ex = ctx.ex;
    let mut ix = Index::default();
    for c in ex.of_type("DRAUGHTING_CALLOUT") {
        let mut items = Vec::new();
        if let Some(p) = c.parameters().get(1) {
            p.collect_refs(&mut items);
        }
        for o in items {
            ix.callout_of.insert(o, c.id);
        }
    }
    for r in ex.of_type("DRAUGHTING_CALLOUT_RELATIONSHIP") {
        let p = r.parameters();
        if let (Some(relating), Some(related)) = (
            p.get(2).and_then(Parameter::as_ref),
            p.get(3).and_then(Parameter::as_ref),
        ) {
            ix.merged.entry(relating).or_default().push((r.id, related));
            ix.merged_into.insert(related, relating);
        }
    }
    for pl in ex.of_type("ANNOTATION_PLANE") {
        let p = pl.parameters();
        let Some(item) = p.get(2).and_then(Parameter::as_ref) else {
            continue;
        };
        let mut elems = Vec::new();
        if let Some(e) = p.get(3) {
            e.collect_refs(&mut elems);
        }
        for e in elems {
            ix.plane_of.entry(e).or_default().push((pl.id, item));
        }
    }
    for keyword in [
        "DRAUGHTING_MODEL_ITEM_ASSOCIATION",
        "DRAUGHTING_MODEL_ITEM_ASSOCIATION_WITH_PLACEHOLDER",
    ] {
        for d in ex.of_type(keyword) {
            let p = d.parameters();
            let Some(def) = p.get(2).and_then(Parameter::as_ref) else {
                continue;
            };
            let mut items = Vec::new();
            if let Some(i) = p.get(4) {
                i.collect_refs(&mut items);
            }
            if let Some(i) = p.get(5) {
                i.collect_refs(&mut items);
            }
            for i in items {
                ix.links.entry(i).or_default().push((d.id, def));
            }
        }
    }
    for l in ex.of_type("PRESENTATION_LAYER_ASSIGNMENT") {
        let p = l.parameters();
        let name = p
            .first()
            .and_then(Parameter::as_str)
            .unwrap_or_default()
            .trim();
        let desc = p
            .get(1)
            .and_then(Parameter::as_str)
            .unwrap_or_default()
            .trim();
        let label = if desc.is_empty() { name } else { desc };
        if label.is_empty() {
            continue;
        }
        let mut items = Vec::new();
        if let Some(i) = p.get(2) {
            i.collect_refs(&mut items);
        }
        for i in items {
            ix.layer_of.entry(i).or_insert_with(|| label.to_owned());
        }
    }
    ix
}
