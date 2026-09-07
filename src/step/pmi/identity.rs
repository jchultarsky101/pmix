//! Finalisation pass: replace the walkers' temporary ids with identity
//! keys (ADR 0004), resolve collisions, and rewrite cross references.

use std::collections::{BTreeMap, HashMap};

use super::Ctx;
use super::fingerprint::{identity_quantum, num};
use crate::model::{Meta, content_hash};

/// Assign final ids to every record in `ctx`.
pub(crate) fn finalise(ctx: &mut Ctx<'_>) {
    let q = identity_quantum(ctx.ex);
    let coarse = q * 100.0;
    let mut map: HashMap<String, String> = HashMap::new();

    // 1. Features, members first.
    let mut aggregated: HashMap<String, (Vec<String>, Vec<[f64; 3]>)> = HashMap::new();
    let mut pending: Vec<usize> = (0..ctx.features.len()).collect();
    while !pending.is_empty() {
        let mut ready = Vec::new();
        let mut later = Vec::new();
        for i in pending {
            let f = &ctx.features[i];
            if f.members.iter().all(|m| map.contains_key(m)) {
                ready.push(i);
            } else {
                later.push(i);
            }
        }
        if ready.is_empty() {
            // Cycle or dangling member: assign whatever remains as is.
            ready = later;
            later = Vec::new();
        }
        let mut batch = Vec::new();
        let mut pending_aggregates: Vec<(String, Vec<String>, Vec<[f64; 3]>)> = Vec::new();
        for i in ready {
            let f = &mut ctx.features[i];
            for m in &mut f.members {
                if let Some(n) = map.get(m) {
                    *m = n.clone();
                }
            }
            f.members.sort();
            // Own geometry plus everything the members span.
            let mut keys: Vec<String> = Vec::new();
            let mut points: Vec<[f64; 3]> = Vec::new();
            for fp in ctx
                .feature_fingerprints
                .get(&f.meta.id)
                .into_iter()
                .flatten()
            {
                // Items that carry no geometry (a mapped_item, a bare
                // representation) say nothing about where the feature is.
                if fp.key.contains('/') {
                    keys.push(fp.key.clone());
                }
                points.extend(fp.points.iter().copied());
            }
            for m in &f.members {
                if let Some((k, p)) = aggregated.get(m) {
                    keys.extend(k.iter().cloned());
                    points.extend(p.iter().copied());
                }
            }
            keys.sort();
            keys.dedup();
            let span = String::new();
            // The kind matters only when it says more than the geometry
            // does (all around, between, derived, ...).
            let kind = match f.kind.as_str() {
                "face" | "edge" | "vertex" | "composite" | "composite_group" | "mixed" => "",
                other => other,
            };
            let mut parts = vec![kind.to_owned(), keys.join(";"), span];
            if keys.is_empty() && f.members.is_empty() {
                // Nothing to anchor on: the name, or failing that the
                // source entity, which keeps distinct unresolved features
                // distinct even though it is not stable across exports.
                match f.name.as_deref().filter(|n| !n.is_empty()) {
                    Some(n) => parts.push(n.to_owned()),
                    None if f.kind.as_str() == "all_over" => {}
                    None => parts.push(f.meta.source_refs.first().cloned().unwrap_or_default()),
                }
            }
            pending_aggregates.push((f.meta.id.clone(), keys, points));
            batch.push((
                i,
                parts.join("|"),
                hash_content(&f, &["id", "source_refs", "presentation"]),
            ));
        }
        // Shape aspects on the same geometry are the same design feature:
        // merge them into one record instead of suffixing.
        let mut by_key: BTreeMap<String, Vec<(String, usize)>> = BTreeMap::new();
        for (i, key, content) in batch {
            by_key.entry(key).or_default().push((content, i));
        }
        let mut merged_away: Vec<usize> = Vec::new();
        for (key, mut members) in by_key {
            members.sort();
            let id = format!("feat:{}", content_hash([key.as_str()]));
            tracing::trace!(%id, %key, "identity key");
            let keep = members[0].1;
            for (n, (_, i)) in members.iter().enumerate() {
                map.insert(ctx.features[*i].meta.id.clone(), id.clone());
                if n > 0 {
                    let extra = std::mem::take(&mut ctx.features[*i].meta.source_refs);
                    ctx.features[keep].meta.source_refs.extend(extra);
                    if ctx.features[keep].name.is_none() {
                        ctx.features[keep].name = ctx.features[*i].name.take();
                    }
                    merged_away.push(*i);
                }
            }
            ctx.features[keep].meta.id = id;
        }
        for (tmp, keys, points) in pending_aggregates {
            if let Some(final_id) = map.get(&tmp) {
                aggregated.entry(final_id.clone()).or_insert((keys, points));
            }
        }
        // Drop merged records, keeping indexes of `later` valid.
        if !merged_away.is_empty() {
            merged_away.sort_unstable();
            let removed: std::collections::HashSet<usize> = merged_away.iter().copied().collect();
            let mut kept = Vec::with_capacity(ctx.features.len());
            let mut new_index = vec![usize::MAX; ctx.features.len()];
            for (i, f) in std::mem::take(&mut ctx.features).into_iter().enumerate() {
                if !removed.contains(&i) {
                    new_index[i] = kept.len();
                    kept.push(f);
                }
            }
            ctx.features = kept;
            later = later.into_iter().map(|i| new_index[i]).collect();
        }
        pending = later;
    }

    // 2. Datums.
    let mut batch = Vec::new();
    for (i, d) in ctx.datums.iter_mut().enumerate() {
        remap_all(&mut d.features, &map);
        for t in &mut d.targets {
            if let Some(f) = &mut t.feature {
                remap(f, &map);
            }
        }
        batch.push((
            i,
            d.label.clone(),
            hash_content(&d, &["id", "source_refs", "presentation"]),
        ));
    }
    for (i, id) in assign(batch, "datum", true) {
        map.insert(ctx.datums[i].meta.id.clone(), id.clone());
        ctx.datums[i].meta.id = id;
    }
    let datum_labels: HashMap<String, String> = ctx
        .datums
        .iter()
        .map(|d| (d.meta.id.clone(), d.label.clone()))
        .collect();

    // 3. Datum systems.
    let mut batch = Vec::new();
    for (i, s) in ctx.datum_systems.iter_mut().enumerate() {
        for c in &mut s.compartments {
            for r in &mut c.datums {
                remap(&mut r.datum, &map);
            }
        }
        let key = s
            .compartments
            .iter()
            .map(|c| {
                c.datums
                    .iter()
                    .map(|r| {
                        datum_labels
                            .get(&r.datum)
                            .cloned()
                            .unwrap_or_else(|| "?".into())
                    })
                    .collect::<Vec<_>>()
                    .join("-")
            })
            .collect::<Vec<_>>()
            .join("|");
        batch.push((
            i,
            key,
            hash_content(&s, &["id", "source_refs", "presentation"]),
        ));
    }
    for (i, id) in assign(batch, "dsys", true) {
        map.insert(ctx.datum_systems[i].meta.id.clone(), id.clone());
        ctx.datum_systems[i].meta.id = id;
    }

    // 4. Dimensions. The feature list is positional (the two ends of a
    // location), so it is remapped without sorting or deduplication.
    let mut batch = Vec::new();
    for (i, d) in ctx.dimensions.iter_mut().enumerate() {
        for f in d.features.iter_mut() {
            remap(f, &map);
        }
        if let Some(p) = &mut d.path {
            remap(p, &map);
        }
        let key = [
            d.kind.as_str(),
            d.subtype.as_str(),
            &d.features.join(","),
            if d.directed { "directed" } else { "" },
            d.path.as_deref().unwrap_or(""),
        ]
        .join("|");
        batch.push((
            i,
            key,
            hash_content(&d, &["id", "source_refs", "presentation"]),
        ));
    }
    for (i, id) in assign(batch, "dim", false) {
        map.insert(ctx.dimensions[i].meta.id.clone(), id.clone());
        ctx.dimensions[i].meta.id = id;
    }

    // 5. Tolerances: upper segments first so lowers can key on them.
    let mut assigned = std::collections::HashSet::new();
    for pass in 0..2 {
        let mut batch = Vec::new();
        for (i, t) in ctx.tolerances.iter_mut().enumerate() {
            let is_lower = t.composite_of.is_some();
            if (pass == 0) == is_lower || assigned.contains(&i) {
                continue;
            }
            remap_all(&mut t.features, &map);
            if let Some(ds) = &mut t.datum_system {
                remap(ds, &map);
            }
            if let Some(c) = &mut t.composite_of {
                remap(c, &map);
            }
            let key = [
                t.kind.as_str(),
                &t.features.join(","),
                t.composite_of.as_deref().unwrap_or(""),
            ]
            .join("|");
            batch.push((
                i,
                key,
                hash_content(&t, &["id", "source_refs", "presentation"]),
            ));
        }
        for (i, id) in assign(batch, "tol", false) {
            map.insert(ctx.tolerances[i].meta.id.clone(), id.clone());
            ctx.tolerances[i].meta.id = id;
            assigned.insert(i);
        }
    }

    // 6. Annotations.
    let mut batch = Vec::new();
    for (i, a) in ctx.annotations.iter_mut().enumerate() {
        remap_all(&mut a.semantic, &map);
        remap_all(&mut a.features, &map);
        let key = if !a.semantic.is_empty() {
            format!("{}|{}", a.semantic.join(","), a.kind.as_str())
        } else {
            let plane = a
                .plane
                .as_ref()
                .map(|p| {
                    format!(
                        "{},{},{}",
                        num(p.origin[0], coarse),
                        num(p.origin[1], coarse),
                        num(p.origin[2], coarse)
                    )
                })
                .unwrap_or_default();
            let bbox = a
                .geometry
                .bbox
                .as_ref()
                .map(|b| {
                    format!(
                        "{},{},{}/{},{},{}",
                        num(b.min[0], coarse),
                        num(b.min[1], coarse),
                        num(b.min[2], coarse),
                        num(b.max[0], coarse),
                        num(b.max[1], coarse),
                        num(b.max[2], coarse)
                    )
                })
                .unwrap_or_default();
            format!("{}|{plane}|{bbox}", a.kind.as_str())
        };
        batch.push((i, key, hash_content(&a, &["id", "source_refs", "views"])));
    }
    for (i, id) in assign(batch, "ann", false) {
        map.insert(ctx.annotations[i].id.clone(), id.clone());
        ctx.annotations[i].id = id;
    }

    // 7. Views.
    let mut batch = Vec::new();
    for (i, v) in ctx.views.iter_mut().enumerate() {
        remap_all(&mut v.annotations, &map);
        batch.push((i, v.name.clone(), hash_content(&v, &["id", "source_refs"])));
    }
    for (i, id) in assign(batch, "view", true) {
        map.insert(ctx.views[i].id.clone(), id.clone());
        ctx.views[i].id = id;
    }

    // 8. Properties, after everything they can point at.
    let mut product_level = Vec::new();
    let mut attached = Vec::new();
    for (i, prop) in ctx.properties.iter_mut().enumerate() {
        if let Some(a) = &mut prop.applies_to {
            remap(a, &map);
        }
        let key = [
            prop.category.as_deref().unwrap_or(""),
            prop.name.as_str(),
            prop.applies_to.as_deref().unwrap_or(""),
        ]
        .join("|");
        let content = hash_content(&prop, &["id", "source_refs"]);
        if prop.applies_to.is_none() {
            // Readable as `prop:Part_Number` when the name allows it.
            product_level.push((i, prop.name.clone(), content, key));
        } else {
            attached.push((i, key, content));
        }
    }
    // Product-level properties key on all three parts but are named by the
    // property alone, so a name shared by two categories collides and takes
    // the usual content-ordered suffix.
    let readable: Vec<(usize, String, String)> = product_level
        .into_iter()
        .map(|(i, name, content, key)| {
            let base = if is_readable(&name) { name } else { key };
            (i, base, content)
        })
        .collect();
    for (i, id) in assign(readable, "prop", true) {
        map.insert(ctx.properties[i].id.clone(), id.clone());
        ctx.properties[i].id = id;
    }
    for (i, id) in assign(attached, "prop", false) {
        map.insert(ctx.properties[i].id.clone(), id.clone());
        ctx.properties[i].id = id;
    }

    // 9. Remaining cross references: annotation -> views, semantic -> presentation.
    for a in &mut ctx.annotations {
        remap_all(&mut a.views, &map);
    }
    let metas: Vec<&mut Meta> = ctx
        .dimensions
        .iter_mut()
        .map(|d| &mut d.meta)
        .chain(ctx.tolerances.iter_mut().map(|t| &mut t.meta))
        .chain(ctx.datums.iter_mut().map(|d| &mut d.meta))
        .chain(ctx.datum_systems.iter_mut().map(|d| &mut d.meta))
        .chain(ctx.features.iter_mut().map(|f| &mut f.meta))
        .collect();
    for m in metas {
        remap_all(&mut m.presentation, &map);
    }
}

fn remap(id: &mut String, map: &HashMap<String, String>) {
    if let Some(n) = map.get(id.as_str()) {
        *id = n.clone();
    }
}

fn remap_all(ids: &mut Vec<String>, map: &HashMap<String, String>) {
    for id in ids.iter_mut() {
        remap(id, map);
    }
    ids.sort();
    ids.dedup();
}

/// Hash of a record's JSON without the listed keys.
fn hash_content<T: serde::Serialize>(rec: &T, skip: &[&str]) -> String {
    let mut v = serde_json::to_value(rec).unwrap_or_default();
    if let Some(obj) = v.as_object_mut() {
        for k in skip {
            obj.remove(*k);
        }
    }
    content_hash([v.to_string()])
}

/// Turn `(index, identity key, content hash)` triples into `(index, id)`.
/// Colliding keys are ordered by content hash; the first keeps the plain
/// id, the rest get `-2`, `-3`, ...
fn assign(
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

fn is_readable(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 40
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '|' | '_' | '.' | '-'))
}
