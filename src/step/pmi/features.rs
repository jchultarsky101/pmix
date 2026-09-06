//! Features: `shape_aspect` and its subtypes, resolved to B-rep geometry.

use super::{Ctx, source_ref};
use crate::model::{Feature, FeatureKind, GeometryKind, GeometryRef, Meta, SurfaceKind};
use crate::step::p21::{Id, Instance, Parameter};

/// The feature id for shape aspect `sa`, building the record on first use.
/// Returns `None` if the instance is missing or is not a shape aspect.
pub(crate) fn feature_for(ctx: &mut Ctx<'_>, sa: Id) -> Option<String> {
    if let Some(known) = ctx.feature_ids.get(&sa) {
        return known.clone();
    }
    // Guard against cycles in composite membership.
    ctx.feature_ids.insert(sa, None);

    let ex = ctx.ex;
    let Some(inst) = ex.get(sa) else {
        ctx.warn(format!("feature reference #{sa} is undefined"), Some(sa));
        return None;
    };
    // A tolerance may target the product_definition_shape itself: "all
    // over". Model that as a feature of the whole part.
    if inst.has_type("PRODUCT_DEFINITION_SHAPE") {
        let id = ctx.ids.make("feat", &["all_over"]);
        ctx.features.push(Feature {
            meta: Meta {
                id: id.clone(),
                source_refs: vec![source_ref(sa)],
                ..Default::default()
            },
            kind: FeatureKind::AllOver,
            name: None,
            geometry: Vec::new(),
            members: Vec::new(),
            count: None,
        });
        ctx.feature_ids.insert(sa, Some(id.clone()));
        return Some(id);
    }

    // Anything else referenced as a feature is a shape_aspect by schema
    // (AP242 has dozens of subtypes: datums, machining feature occurrences,
    // ...), so no type gate beyond rejecting obvious non-features: the
    // record's kind comes from its geometry.
    let not_a_feature = inst.type_names().any(|t| {
        matches!(
            t,
            "PRODUCT_DEFINITION" | "PRODUCT" | "SHAPE_REPRESENTATION" | "REPRESENTATION"
        )
    }) || (inst
        .type_names()
        .any(|t| t.starts_with("DIMENSIONAL_") || t.starts_with("GEOMETRIC_TOLERANCE"))
        && !inst.has_type("DATUM_FEATURE")
        && !inst.type_names().any(|t| t.ends_with("WITH_DATUM_FEATURE")));
    if not_a_feature {
        ctx.warn(
            format!(
                "#{sa} ({}) is used as a feature but is not a shape aspect",
                inst.type_key()
            ),
            Some(sa),
        );
        return None;
    }

    let name = inst
        .segment("SHAPE_ASPECT")
        .or_else(|| inst.segments.iter().find(|s| s.parameters.len() >= 4))
        .and_then(|s| s.parameters.first())
        .and_then(Parameter::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);

    let mut source_refs = vec![source_ref(sa)];

    // Geometry through geometric_item_specific_usage and
    // item_identified_representation_usage.
    let mut geometry = Vec::new();
    for (usage_id, item_id) in ctx.geometry_usage.get(&sa).cloned().unwrap_or_default() {
        ctx.consume(usage_id);
        source_refs.push(source_ref(usage_id));
        match ex.get(item_id) {
            Some(item) => geometry.push(geometry_ref(ex, item)),
            None => ctx.warn(
                format!("#{usage_id} identifies undefined geometry #{item_id}"),
                Some(usage_id),
            ),
        }
    }

    // Members through shape_aspect_relationship (relating = this aspect).
    let mut members = Vec::new();
    for (rel_id, member_id) in ctx.composite_members.get(&sa).cloned().unwrap_or_default() {
        if let Some(id) = feature_for(ctx, member_id) {
            ctx.consume(rel_id);
            source_refs.push(source_ref(rel_id));
            members.push(id);
        }
    }

    let kind = feature_kind(inst, &geometry, &members);
    if geometry.is_empty() && members.is_empty() {
        ctx.warn(
            format!(
                "feature #{sa} ({}) has no geometry and no members",
                inst.type_key()
            ),
            Some(sa),
        );
    }

    let mut geo_keys: Vec<String> = geometry
        .iter()
        .map(|g| match &g.surface {
            Some(s) => format!("{}/{}", g.kind, s),
            None => g.kind.to_string(),
        })
        .collect();
    geo_keys.sort();
    let mut member_keys = members.clone();
    member_keys.sort();
    let id = ctx.ids.make(
        "feat",
        &[
            kind.as_str(),
            name.as_deref().unwrap_or(""),
            &geo_keys.join(","),
            &member_keys.join(","),
        ],
    );

    ctx.consume(sa);
    ctx.features.push(Feature {
        meta: Meta {
            id: id.clone(),
            source_refs,
            ..Default::default()
        },
        kind,
        name,
        geometry,
        members,
        count: None,
    });
    ctx.feature_ids.insert(sa, Some(id.clone()));
    Some(id)
}

fn feature_kind(inst: &Instance, geometry: &[GeometryRef], members: &[String]) -> FeatureKind {
    let by_type = [
        ("COMPOSITE_GROUP_SHAPE_ASPECT", FeatureKind::CompositeGroup),
        ("COMPOSITE_SHAPE_ASPECT", FeatureKind::Composite),
        ("ALL_AROUND_SHAPE_ASPECT", FeatureKind::AllAround),
        ("BETWEEN_SHAPE_ASPECT", FeatureKind::Between),
        ("DERIVED_SHAPE_ASPECT", FeatureKind::Derived),
        ("CENTRE_OF_SYMMETRY", FeatureKind::Center),
        ("APEX", FeatureKind::Apex),
        ("TANGENT", FeatureKind::Tangent),
        ("PARALLEL_OFFSET", FeatureKind::ParallelOffset),
        ("GEOMETRIC_ALIGNMENT", FeatureKind::GeometricAlignment),
        ("PERPENDICULAR_TO", FeatureKind::PerpendicularTo),
    ];
    for (t, kind) in by_type {
        if inst.has_type(t) {
            return kind;
        }
    }
    if geometry.is_empty() {
        return if members.is_empty() {
            FeatureKind::Other("unresolved".into())
        } else {
            FeatureKind::Composite
        };
    }
    let first = &geometry[0].kind;
    if geometry.iter().all(|g| &g.kind == first) {
        match first {
            GeometryKind::Face | GeometryKind::Surface => FeatureKind::Face,
            GeometryKind::Edge | GeometryKind::Curve => FeatureKind::Edge,
            GeometryKind::Vertex | GeometryKind::Point => FeatureKind::Vertex,
            GeometryKind::Other(o) => FeatureKind::Other(o.clone()),
        }
    } else {
        FeatureKind::Mixed
    }
}

fn geometry_ref(ex: &crate::step::p21::Exchange, item: &Instance) -> GeometryRef {
    let source_ref = source_ref(item.id);
    let face = ["ADVANCED_FACE", "FACE_SURFACE", "FACE"];
    let edge = ["EDGE_CURVE", "ORIENTED_EDGE", "EDGE"];
    let vertex = ["VERTEX_POINT", "VERTEX"];
    let curves = [
        "LINE",
        "CIRCLE",
        "ELLIPSE",
        "TRIMMED_CURVE",
        "POLYLINE",
        "COMPOSITE_CURVE",
        "B_SPLINE_CURVE",
        "B_SPLINE_CURVE_WITH_KNOTS",
    ];
    if face.iter().any(|t| item.has_type(t)) {
        let surface = item
            .parameters()
            .get(2)
            .and_then(Parameter::as_ref)
            .and_then(|id| ex.get(id))
            .map(surface_kind);
        return GeometryRef {
            kind: GeometryKind::Face,
            surface,
            source_ref,
        };
    }
    if edge.iter().any(|t| item.has_type(t)) {
        return GeometryRef {
            kind: GeometryKind::Edge,
            surface: None,
            source_ref,
        };
    }
    if vertex.iter().any(|t| item.has_type(t)) {
        return GeometryRef {
            kind: GeometryKind::Vertex,
            surface: None,
            source_ref,
        };
    }
    if item.has_type("CARTESIAN_POINT") {
        return GeometryRef {
            kind: GeometryKind::Point,
            surface: None,
            source_ref,
        };
    }
    if curves.iter().any(|t| item.has_type(t)) {
        return GeometryRef {
            kind: GeometryKind::Curve,
            surface: None,
            source_ref,
        };
    }
    if let Some(s) = known_surface(item) {
        return GeometryRef {
            kind: GeometryKind::Surface,
            surface: Some(s),
            source_ref,
        };
    }
    GeometryRef {
        kind: GeometryKind::Other(item.type_key().to_ascii_lowercase()),
        surface: None,
        source_ref,
    }
}

fn known_surface(inst: &Instance) -> Option<SurfaceKind> {
    let table = [
        ("PLANE", SurfaceKind::Plane),
        ("CYLINDRICAL_SURFACE", SurfaceKind::Cylinder),
        ("CONICAL_SURFACE", SurfaceKind::Cone),
        ("SPHERICAL_SURFACE", SurfaceKind::Sphere),
        ("TOROIDAL_SURFACE", SurfaceKind::Torus),
        ("DEGENERATE_TOROIDAL_SURFACE", SurfaceKind::Torus),
        ("SURFACE_OF_REVOLUTION", SurfaceKind::Revolution),
        ("SURFACE_OF_LINEAR_EXTRUSION", SurfaceKind::Extrusion),
        ("OFFSET_SURFACE", SurfaceKind::Offset),
    ];
    for (t, k) in table {
        if inst.has_type(t) {
            return Some(k);
        }
    }
    if inst.type_names().any(|t| t.starts_with("B_SPLINE_SURFACE")) {
        return Some(SurfaceKind::BSpline);
    }
    None
}

fn surface_kind(inst: &Instance) -> SurfaceKind {
    known_surface(inst).unwrap_or_else(|| SurfaceKind::Other(inst.type_key().to_ascii_lowercase()))
}
