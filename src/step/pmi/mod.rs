//! AP242 PMI extraction over the untyped Part 21 graph.
//!
//! Each submodule is a *walker* for one family of entities. Walkers share a
//! `Ctx` that holds the exchange, reverse indexes, the ids of instances
//! already consumed, and the records built so far. After all walkers run,
//! any PMI-family instance that no walker consumed is reported in the
//! document's `unknown` list (ADR 0002: never drop silently).

pub(crate) mod datums;
pub(crate) mod dimensions;
pub(crate) mod features;
pub(crate) mod fingerprint;
pub(crate) mod identity;
pub(crate) mod measures;
pub(crate) mod presentation;
pub(crate) mod properties;
pub(crate) mod tolerances;
pub(crate) mod units;

use std::collections::{HashMap, HashSet};

use crate::ExtractOptions;
use crate::model::{
    Annotation, ContentId, Datum, DatumSystem, Diagnostic, Dimension, Feature, GeometricTolerance,
    Layer, PmiDocument, Presentation, Property, SCHEMA_VERSION, SavedView, Semantic, Source,
    Unknown,
};
use crate::step::p21::{Exchange, Id, Instance, Parameter};

/// Extract everything the walkers understand from `ex`.
pub fn extract(ex: &Exchange, file_name: &str, options: &ExtractOptions) -> PmiDocument {
    // Each walker's time, so that a slow file can be attributed to a
    // phase rather than guessed at. Silent unless debug logging is on.
    macro_rules! phase {
        ($name:literal, $e:expr) => {{
            let started = std::time::Instant::now();
            let value = $e;
            tracing::debug!(
                phase = $name,
                ms = started.elapsed().as_secs_f64() * 1000.0,
                "walked"
            );
            value
        }};
    }
    let mut ctx = phase!("ctx", Ctx::new(ex));
    let units = phase!("units", units::document_units(&mut ctx));
    // Everything keyed on geometry is stated in millimetres and degrees,
    // whatever the file declares, so that one design exported in two
    // unit systems keys the same way.
    ctx.scale = crate::fingerprint::Scale::of(units.length.as_deref(), units.angle.as_deref());
    phase!("datums", datums::walk(&mut ctx));
    phase!("dimensions", dimensions::walk(&mut ctx));
    phase!("tolerances", tolerances::walk(&mut ctx));
    phase!("presentation", presentation::walk(&mut ctx, options));
    phase!("properties", properties::walk(&mut ctx));
    phase!("identity", identity::finalise(&mut ctx));
    let unknown = phase!("unknown", ctx.collect_unknown());

    let mut diagnostics: Vec<Diagnostic> = ex
        .diagnostics
        .iter()
        .map(|d| Diagnostic {
            message: format!("parser: {d}"),
            source_ref: None,
        })
        .collect();
    diagnostics.append(&mut ctx.diagnostics);

    let mut semantic = Semantic {
        features: ctx.features,
        datums: ctx.datums,
        datum_systems: ctx.datum_systems,
        dimensions: ctx.dimensions,
        tolerances: ctx.tolerances,
        ..Default::default()
    };
    semantic.sort();
    let mut properties = ctx.properties;
    properties.sort_by(|a, b| a.id.cmp(&b.id));
    let mut presentation = Presentation {
        annotations: ctx.annotations,
        views: ctx.views,
    };
    presentation.sort();

    let header = &ex.header;
    PmiDocument {
        schema_version: SCHEMA_VERSION,
        source: Source {
            file_name: file_name.to_owned(),
            format: "STEP".to_owned(),
            schema: header.schema_name(),
            writer: header
                .preprocessor_version()
                .filter(|s| !s.trim().is_empty())
                .map(str::to_owned),
            time_stamp: header
                .time_stamp()
                .filter(|s| !s.trim().is_empty())
                .map(str::to_owned),
        },
        units,
        properties,
        semantic,
        presentation,
        unknown,
        diagnostics,
    }
}

/// Entity families with a walker. Instances of these types that end up
/// unconsumed indicate a gap in a walker. Leaf values such as
/// `TOLERANCE_VALUE` are not listed: they are only meaningful through their
/// parent, which is what gets reported.
const HANDLED_FAMILY: &[(&[&str], &str, Layer)] = &[
    (
        &[
            "DIMENSIONAL_SIZE",
            "DIMENSIONAL_SIZE_WITH_PATH",
            "DIMENSIONAL_SIZE_WITH_DATUM_FEATURE",
            "DIMENSIONAL_LOCATION",
            "DIMENSIONAL_LOCATION_WITH_PATH",
            "DIMENSIONAL_LOCATION_WITH_DATUM_FEATURE",
            "DIRECTED_DIMENSIONAL_LOCATION",
            "ANGULAR_SIZE",
            "ANGULAR_LOCATION",
            "DIMENSIONAL_CHARACTERISTIC_REPRESENTATION",
            "SHAPE_DIMENSION_REPRESENTATION",
            "PLUS_MINUS_TOLERANCE",
        ],
        "recognised as a dimension entity but not consumed by the dimension walker",
        Layer::Semantic,
    ),
    (
        &[
            "GEOMETRIC_TOLERANCE",
            "ANGULARITY_TOLERANCE",
            "CIRCULAR_RUNOUT_TOLERANCE",
            "COAXIALITY_TOLERANCE",
            "CONCENTRICITY_TOLERANCE",
            "CYLINDRICITY_TOLERANCE",
            "FLATNESS_TOLERANCE",
            "LINE_PROFILE_TOLERANCE",
            "PARALLELISM_TOLERANCE",
            "PERPENDICULARITY_TOLERANCE",
            "POSITION_TOLERANCE",
            "ROUNDNESS_TOLERANCE",
            "STRAIGHTNESS_TOLERANCE",
            "SURFACE_PROFILE_TOLERANCE",
            "SYMMETRY_TOLERANCE",
            "TOTAL_RUNOUT_TOLERANCE",
            "TOLERANCE_ZONE",
            "TOLERANCE_ZONE_FORM",
            "PROJECTED_ZONE_DEFINITION",
            "RUNOUT_ZONE_DEFINITION",
            "NON_UNIFORM_ZONE_DEFINITION",
            "GEOMETRIC_TOLERANCE_RELATIONSHIP",
        ],
        "recognised as a geometric tolerance entity but not consumed by the tolerance walker",
        Layer::Semantic,
    ),
    (
        &[
            "DATUM",
            "DATUM_FEATURE",
            "DATUM_TARGET",
            "PLACED_DATUM_TARGET_FEATURE",
            "FEATURE_FOR_DATUM_TARGET_RELATIONSHIP",
            "DATUM_SYSTEM",
            "DATUM_REFERENCE_COMPARTMENT",
            "DATUM_REFERENCE_ELEMENT",
            "GENERAL_DATUM_REFERENCE",
            "REFERENCED_MODIFIED_DATUM",
            "DATUM_REFERENCE_MODIFIER_WITH_VALUE",
        ],
        "recognised as a datum entity but not consumed by the datum walker",
        Layer::Semantic,
    ),
    (
        &[
            "DRAUGHTING_CALLOUT",
            "DRAUGHTING_CALLOUT_RELATIONSHIP",
            "ANNOTATION_OCCURRENCE",
            "TESSELLATED_ANNOTATION_OCCURRENCE",
            "ANNOTATION_CURVE_OCCURRENCE",
            "ANNOTATION_FILL_AREA_OCCURRENCE",
            "ANNOTATION_PLACEHOLDER_OCCURRENCE",
            "ANNOTATION_PLACEHOLDER_OCCURRENCE_WITH_LEADER_LINE",
            "ANNOTATION_TEXT_OCCURRENCE",
            "ANNOTATION_PLANE",
            "DRAUGHTING_MODEL_ITEM_ASSOCIATION",
            "DRAUGHTING_MODEL_ITEM_ASSOCIATION_WITH_PLACEHOLDER",
            "CAMERA_MODEL_D3",
            "CAMERA_MODEL_D3_MULTI_CLIPPING",
            "MODEL_GEOMETRIC_VIEW",
            "DEFAULT_MODEL_GEOMETRIC_VIEW",
        ],
        "recognised as a presentation entity but not consumed by the presentation walker",
        Layer::Presentation,
    ),
];

/// Reason and layer for an unconsumed instance, if it belongs to a PMI family.
fn unknown_reason(inst: &Instance) -> Option<(&'static str, Layer)> {
    for (keywords, reason, layer) in HANDLED_FAMILY {
        if inst.type_names().any(|t| keywords.contains(&t)) {
            return Some((reason, *layer));
        }
    }
    // AP242 machining feature definitions (basic_round_hole, counterbore
    // hole, ...) carry hole callouts with their own tolerance values.
    if inst.type_names().any(|t| t.contains("_HOLE")) {
        return Some((
            "hole feature definitions are not supported yet",
            Layer::Semantic,
        ));
    }
    None
}

/// A property definition and the representation carrying its values:
/// `(property_definition, definition_representation, representation)`.
pub(crate) type PropertyRepr = (Id, Id, Id);

/// Shared state for the walkers.
pub(crate) struct Ctx<'a> {
    pub ex: &'a Exchange,
    /// What the file's numbers have to be multiplied by to reach the
    /// units a fingerprint is stated in.
    pub scale: crate::fingerprint::Scale,
    pub ids: ContentId,
    pub consumed: HashSet<Id>,
    pub diagnostics: Vec<Diagnostic>,
    pub unit_names: HashMap<Id, Option<String>>,
    /// Shape aspect id to feature id (`None` when it could not be built).
    pub feature_ids: HashMap<Id, Option<String>>,
    pub features: Vec<Feature>,
    pub dimensions: Vec<Dimension>,
    pub datums: Vec<Datum>,
    pub datum_systems: Vec<DatumSystem>,
    pub tolerances: Vec<GeometricTolerance>,
    pub annotations: Vec<Annotation>,
    pub views: Vec<SavedView>,
    pub properties: Vec<Property>,
    /// Dimension instance id to record id.
    pub dimension_ids: HashMap<Id, String>,
    /// Feature (temporary) record id to the fingerprints of its geometry.
    pub feature_fingerprints: HashMap<String, Vec<fingerprint::Fingerprint>>,
    /// Feature (temporary) record id to what anchors it when the file
    /// gives it no geometry: the datum it establishes or is a target of.
    pub feature_anchors: HashMap<String, String>,
    /// Datum instance id to record id.
    pub datum_ids: HashMap<Id, Option<String>>,
    /// Datum record id to its label, for rendering.
    pub datum_labels: HashMap<String, String>,
    /// Datum system instance id to record id.
    pub datum_system_ids: HashMap<Id, Option<String>>,
    /// Tolerance instance id to record id.
    pub tolerance_ids: HashMap<Id, Option<String>>,
    /// `SHAPE_ASPECT_RELATIONSHIP.relating` to its `related` ids, file order.
    pub composite_members: HashMap<Id, Vec<(Id, Id)>>,
    /// Datum id to `(relationship id, relating shape aspect)` for every
    /// `SHAPE_ASPECT_RELATIONSHIP` whose `related` is that datum.
    pub datum_links: HashMap<Id, Vec<(Id, Id)>>,
    /// Datum target id to `(relationship id, feature)` from
    /// `FEATURE_FOR_DATUM_TARGET_RELATIONSHIP`.
    pub target_features: HashMap<Id, Vec<(Id, Id)>>,
    /// Definition id to the property representations describing it.
    pub property_reprs: HashMap<Id, Vec<PropertyRepr>>,
    /// Tolerance id to the `TOLERANCE_ZONE`s defining it.
    pub zones: HashMap<Id, Vec<Id>>,
    /// Zone id to its `*_ZONE_DEFINITION`s.
    pub zone_definitions: HashMap<Id, Vec<Id>>,
    /// `(relationship id, relating, related, name)` of every
    /// `GEOMETRIC_TOLERANCE_RELATIONSHIP`.
    pub tolerance_relationships: Vec<(Id, Id, Id, String)>,
    /// Shape aspect id to the geometry items it identifies, with the id of
    /// the usage instance that links them.
    pub geometry_usage: HashMap<Id, Vec<(Id, Id)>>,
    /// Dimension id to `(DIMENSIONAL_CHARACTERISTIC_REPRESENTATION id,
    /// SHAPE_DIMENSION_REPRESENTATION id)` pairs.
    pub dimension_reprs: HashMap<Id, Vec<(Id, Id)>>,
    /// Dimension id to `PLUS_MINUS_TOLERANCE` ids.
    pub plus_minus: HashMap<Id, Vec<Id>>,
}

impl<'a> Ctx<'a> {
    fn new(ex: &'a Exchange) -> Self {
        let mut ctx = Self {
            ex,
            scale: crate::fingerprint::Scale::NONE,
            ids: ContentId::new(),
            consumed: HashSet::new(),
            diagnostics: Vec::new(),
            unit_names: HashMap::new(),
            feature_ids: HashMap::new(),
            features: Vec::new(),
            dimensions: Vec::new(),
            datums: Vec::new(),
            datum_systems: Vec::new(),
            tolerances: Vec::new(),
            annotations: Vec::new(),
            views: Vec::new(),
            properties: Vec::new(),
            dimension_ids: HashMap::new(),
            feature_fingerprints: HashMap::new(),
            feature_anchors: HashMap::new(),
            datum_ids: HashMap::new(),
            datum_labels: HashMap::new(),
            datum_system_ids: HashMap::new(),
            tolerance_ids: HashMap::new(),
            composite_members: HashMap::new(),
            datum_links: HashMap::new(),
            target_features: HashMap::new(),
            property_reprs: HashMap::new(),
            zones: HashMap::new(),
            zone_definitions: HashMap::new(),
            tolerance_relationships: Vec::new(),
            geometry_usage: HashMap::new(),
            dimension_reprs: HashMap::new(),
            plus_minus: HashMap::new(),
        };
        ctx.build_indexes();
        ctx
    }

    fn build_indexes(&mut self) {
        let ex = self.ex;
        let refs2 = |inst: &Instance, a: usize, b: usize| -> Option<(Id, Id)> {
            let p = inst.parameters();
            Some((
                p.get(a).and_then(Parameter::as_ref)?,
                p.get(b).and_then(Parameter::as_ref)?,
            ))
        };

        // shape_aspect_relationship: datum links (related is a datum) vs.
        // composite membership (everything else).
        for inst in ex.of_type("SHAPE_ASPECT_RELATIONSHIP") {
            let Some((relating, related)) = refs2(inst, 2, 3) else {
                continue;
            };
            if ex.get(related).is_some_and(|r| r.has_type("DATUM")) {
                self.datum_links
                    .entry(related)
                    .or_default()
                    .push((inst.id, relating));
            } else {
                self.composite_members
                    .entry(relating)
                    .or_default()
                    .push((inst.id, related));
            }
        }
        for inst in ex.of_type("FEATURE_FOR_DATUM_TARGET_RELATIONSHIP") {
            if let Some((feature, target)) = refs2(inst, 2, 3) {
                self.target_features
                    .entry(target)
                    .or_default()
                    .push((inst.id, feature));
            }
        }

        // Geometry identification.
        for keyword in [
            "GEOMETRIC_ITEM_SPECIFIC_USAGE",
            "ITEM_IDENTIFIED_REPRESENTATION_USAGE",
        ] {
            for inst in ex.of_type(keyword) {
                let p = inst.parameters();
                let Some(definition) = p.get(2).and_then(Parameter::as_ref) else {
                    continue;
                };
                let mut items = Vec::new();
                if let Some(identified) = p.get(4) {
                    identified.collect_refs(&mut items);
                }
                let entry = self.geometry_usage.entry(definition).or_default();
                entry.extend(items.into_iter().map(|item| (inst.id, item)));
            }
        }

        // Property definitions on shape aspects (datum target parameters).
        for keyword in [
            "SHAPE_DEFINITION_REPRESENTATION",
            "PROPERTY_DEFINITION_REPRESENTATION",
        ] {
            for inst in ex.of_type(keyword) {
                let Some((pd_id, rep_id)) = refs2(inst, 0, 1) else {
                    continue;
                };
                let Some(pd) = ex.get(pd_id) else { continue };
                if !pd.has_type("PROPERTY_DEFINITION") {
                    continue;
                }
                if let Some(def) = pd.parameters().get(2).and_then(Parameter::as_ref) {
                    self.property_reprs
                        .entry(def)
                        .or_default()
                        .push((pd_id, inst.id, rep_id));
                }
            }
        }

        // Dimensions.
        for inst in ex.of_type("DIMENSIONAL_CHARACTERISTIC_REPRESENTATION") {
            if let Some((dim, repr)) = refs2(inst, 0, 1) {
                self.dimension_reprs
                    .entry(dim)
                    .or_default()
                    .push((inst.id, repr));
            }
        }
        for inst in ex.of_type("PLUS_MINUS_TOLERANCE") {
            if let Some(dim) = inst.parameters().get(1).and_then(Parameter::as_ref) {
                self.plus_minus.entry(dim).or_default().push(inst.id);
            }
        }

        // Tolerance zones and composites.
        for inst in ex.of_type("TOLERANCE_ZONE") {
            let mut tols = Vec::new();
            if let Some(p) = inst.parameters().get(4) {
                p.collect_refs(&mut tols);
            }
            for t in tols {
                self.zones.entry(t).or_default().push(inst.id);
            }
        }
        for keyword in [
            "PROJECTED_ZONE_DEFINITION",
            "RUNOUT_ZONE_DEFINITION",
            "NON_UNIFORM_ZONE_DEFINITION",
        ] {
            for inst in ex.of_type(keyword) {
                if let Some(z) = inst.parameters().first().and_then(Parameter::as_ref) {
                    self.zone_definitions.entry(z).or_default().push(inst.id);
                }
            }
        }
        for inst in ex.of_type("GEOMETRIC_TOLERANCE_RELATIONSHIP") {
            if let Some((relating, related)) = refs2(inst, 2, 3) {
                let name = inst
                    .parameters()
                    .first()
                    .and_then(Parameter::as_str)
                    .unwrap_or_default()
                    .to_owned();
                self.tolerance_relationships
                    .push((inst.id, relating, related, name));
            }
        }
    }

    pub fn consume(&mut self, id: Id) {
        self.consumed.insert(id);
    }

    /// Record that annotation `aid` displays semantic record `sid`.
    pub fn link_presentation(&mut self, sid: &str, aid: &str) {
        let meta = self
            .dimensions
            .iter_mut()
            .map(|d| &mut d.meta)
            .chain(self.tolerances.iter_mut().map(|t| &mut t.meta))
            .chain(self.datums.iter_mut().map(|d| &mut d.meta))
            .chain(self.datum_systems.iter_mut().map(|d| &mut d.meta))
            .find(|m| m.id == sid);
        if let Some(m) = meta {
            if !m.presentation.iter().any(|a| a == aid) {
                m.presentation.push(aid.to_owned());
            }
        }
    }

    pub fn warn(&mut self, message: impl Into<String>, source: Option<Id>) {
        let message = message.into();
        tracing::debug!(source = ?source, "{message}");
        self.diagnostics.push(Diagnostic {
            message,
            source_ref: source.map(source_ref),
        });
    }

    /// Follow a reference parameter, warning when it dangles.
    pub fn deref(&mut self, p: Option<&Parameter>, what: &str, from: Id) -> Option<&'a Instance> {
        let id = p.and_then(Parameter::as_ref)?;
        let inst = self.ex.get(id);
        if inst.is_none() {
            self.warn(
                format!("{what} of #{from} references undefined instance #{id}"),
                Some(from),
            );
        }
        inst
    }

    fn collect_unknown(&self) -> Vec<Unknown> {
        let mut out = Vec::new();
        for inst in self.ex.instances() {
            if self.consumed.contains(&inst.id) {
                continue;
            }
            let Some((reason, layer)) = unknown_reason(inst) else {
                continue;
            };
            out.push(Unknown {
                layer,
                kind: inst.type_key(),
                reason: reason.to_owned(),
                source_ref: source_ref(inst.id),
                raw: inst.to_string(),
            });
        }
        out
    }
}

/// `#123` form of an instance id, used in `source_refs`.
pub(crate) fn source_ref(id: Id) -> String {
    format!("#{id}")
}
