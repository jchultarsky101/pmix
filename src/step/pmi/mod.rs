//! AP242 PMI extraction over the untyped Part 21 graph.
//!
//! Each submodule is a *walker* for one family of entities. Walkers share a
//! `Ctx` that holds the exchange, reverse indexes, the ids of instances
//! already consumed, and the records built so far. After all walkers run,
//! any PMI-family instance that no walker consumed is reported in the
//! document's `unknown` list (ADR 0002: never drop silently).

pub(crate) mod dimensions;
pub(crate) mod features;
pub(crate) mod measures;
pub(crate) mod units;

use std::collections::{HashMap, HashSet};

use crate::model::{
    ContentId, Diagnostic, Dimension, Feature, Layer, PmiDocument, Presentation, SCHEMA_VERSION,
    Semantic, Source, Unknown,
};
use crate::step::p21::{Exchange, Id, Instance, Parameter};

/// Extract everything the walkers understand from `ex`.
pub fn extract(ex: &Exchange, file_name: &str) -> PmiDocument {
    let mut ctx = Ctx::new(ex);
    let units = units::document_units(&mut ctx);
    dimensions::walk(&mut ctx);
    let unknown = ctx.collect_unknown();

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
        dimensions: ctx.dimensions,
        ..Default::default()
    };
    semantic.sort();

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
        semantic,
        presentation: Presentation::default(),
        unknown,
        diagnostics,
    }
}

/// Entity families with a walker. Instances of these types that end up
/// unconsumed indicate a gap in a walker. Leaf values such as
/// `TOLERANCE_VALUE` are not listed: they are only meaningful through their
/// parent, which is what gets reported.
const HANDLED_FAMILY: &[&str] = &[
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
];

/// PMI entity families with no walker yet, each with the reason reported.
const PENDING_FAMILY: &[(&[&str], &str)] = &[
    (
        &[
            "GEOMETRIC_TOLERANCE",
            "TOLERANCE_ZONE",
            "TOLERANCE_ZONE_FORM",
            "PROJECTED_ZONE_DEFINITION",
            "RUNOUT_ZONE_DEFINITION",
            "NON_UNIFORM_ZONE_DEFINITION",
            "GEOMETRIC_TOLERANCE_RELATIONSHIP",
        ],
        "geometric tolerances are not supported yet",
    ),
    (
        &[
            "DATUM",
            "DATUM_FEATURE",
            "DATUM_TARGET",
            "PLACED_DATUM_TARGET_FEATURE",
            "DATUM_SYSTEM",
            "DATUM_REFERENCE_COMPARTMENT",
            "DATUM_REFERENCE_ELEMENT",
            "GENERAL_DATUM_REFERENCE",
            "REFERENCED_MODIFIED_DATUM",
        ],
        "datums are not supported yet",
    ),
];

/// Reason for an unconsumed instance, if it belongs to a PMI family.
fn unknown_reason(inst: &Instance) -> Option<&'static str> {
    if inst.type_names().any(|t| HANDLED_FAMILY.contains(&t)) {
        return Some("recognised as a dimension entity but not consumed by the dimension walker");
    }
    for (keywords, reason) in PENDING_FAMILY {
        if inst.type_names().any(|t| keywords.contains(&t)) {
            return Some(reason);
        }
    }
    // AP242 machining feature definitions (basic_round_hole, counterbore
    // hole, ...) carry hole callouts with their own tolerance values.
    if inst.type_names().any(|t| t.contains("_HOLE")) {
        return Some("hole feature definitions are not supported yet");
    }
    None
}

/// Shared state for the walkers.
pub(crate) struct Ctx<'a> {
    pub ex: &'a Exchange,
    pub ids: ContentId,
    pub consumed: HashSet<Id>,
    pub diagnostics: Vec<Diagnostic>,
    pub unit_names: HashMap<Id, Option<String>>,
    /// Shape aspect id to feature id (`None` when it could not be built).
    pub feature_ids: HashMap<Id, Option<String>>,
    pub features: Vec<Feature>,
    pub dimensions: Vec<Dimension>,
    /// `SHAPE_ASPECT_RELATIONSHIP.relating` to its `related` ids, file order.
    pub composite_members: HashMap<Id, Vec<(Id, Id)>>,
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
            ids: ContentId::new(),
            consumed: HashSet::new(),
            diagnostics: Vec::new(),
            unit_names: HashMap::new(),
            feature_ids: HashMap::new(),
            features: Vec::new(),
            dimensions: Vec::new(),
            composite_members: HashMap::new(),
            geometry_usage: HashMap::new(),
            dimension_reprs: HashMap::new(),
            plus_minus: HashMap::new(),
        };
        ctx.build_indexes();
        ctx
    }

    fn build_indexes(&mut self) {
        let ex = self.ex;
        for inst in ex.of_type("SHAPE_ASPECT_RELATIONSHIP") {
            let p = inst.parameters();
            if let (Some(relating), Some(related)) = (
                p.get(2).and_then(Parameter::as_ref),
                p.get(3).and_then(Parameter::as_ref),
            ) {
                self.composite_members
                    .entry(relating)
                    .or_default()
                    .push((inst.id, related));
            }
        }
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
        for inst in ex.of_type("DIMENSIONAL_CHARACTERISTIC_REPRESENTATION") {
            let p = inst.parameters();
            if let (Some(dim), Some(repr)) = (
                p.first().and_then(Parameter::as_ref),
                p.get(1).and_then(Parameter::as_ref),
            ) {
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
    }

    pub fn consume(&mut self, id: Id) {
        self.consumed.insert(id);
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
            let Some(reason) = unknown_reason(inst) else {
                continue;
            };
            out.push(Unknown {
                layer: Layer::Semantic,
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
