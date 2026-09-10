//! Reading a STEP file's product structure (ADR 0014).
//!
//! The PMI side of this reader already climbs part of this chain: a
//! property attached to a product definition is attributed to the part
//! by following shape → definition → formation → product (ADR 0007).
//! That walk is [`product_at`] below, which the property reader now
//! calls rather than keeping a second copy of.
//!
//! What is new here is the rest of the graph. A product definition is a
//! part; a `NEXT_ASSEMBLY_USAGE_OCCURRENCE` says one part is used inside
//! another; and the transformation that says *where* is three references
//! away from the occurrence rather than on it:
//!
//! ```text
//! NEXT_ASSEMBLY_USAGE_OCCURRENCE   the occurrence
//!   ^ definition
//! PRODUCT_DEFINITION_SHAPE         its shape
//!   ^ represented_product_relation
//! CONTEXT_DEPENDENT_SHAPE_REPRESENTATION
//!   | representation_relation
//! (... REPRESENTATION_RELATIONSHIP_WITH_TRANSFORMATION ...)
//!   | transformation_operator
//! ITEM_DEFINED_TRANSFORMATION      where the child sits
//! ```
//!
//! Two of those arrows point *backwards*: the occurrence does not name
//! its shape, the shape names the occurrence. [`Exchange::referrers`]
//! answers that but scans every instance per call, which on an assembly
//! of a few hundred parts is a few hundred scans of a few hundred
//! thousand instances. So this builds one reverse index up front and
//! uses it throughout.

use std::collections::BTreeMap;

use crate::fingerprint::Scale;
use crate::identity::triple;
use crate::model::{ContentId, Direction, Placement};
use crate::product::model::{Diagnostic, Part, ProductDocument, Relation, SCHEMA_VERSION};
use crate::step::p21::{Exchange, Id, Instance, Parameter};

/// The entities that stand for "the whole part": a shape names its
/// definition, a definition names its formation, and a formation names
/// its product.
pub(crate) const PRODUCT_LEVEL: &[&str] = &[
    "PRODUCT_DEFINITION",
    "PRODUCT_DEFINITION_SHAPE",
    "PRODUCT_DEFINITION_FORMATION",
    "PRODUCT",
];

/// The product a product-level entity belongs to.
///
/// Following the one reference that is itself product-level arrives at
/// the product in at most three steps. Shared with the property reader
/// (ADR 0007), which needs the same answer to say which part states a
/// property, so that the two cannot disagree about it.
pub(crate) fn product_at(ex: &Exchange, target: Id) -> Option<&Instance> {
    let mut at = target;
    for _ in 0..PRODUCT_LEVEL.len() {
        let inst = ex.get(at)?;
        if inst.has_type("PRODUCT") {
            return Some(inst);
        }
        at = inst.references().into_iter().find(|r| {
            ex.get(*r)
                .is_some_and(|i| i.type_names().any(|t| PRODUCT_LEVEL.contains(&t)))
        })?;
    }
    None
}

/// Which instances reference each instance, built once.
///
/// Half of this walk runs against the direction of the file's own
/// references, and doing that with a linear scan per lookup is the
/// difference between reading an assembly and appearing to hang.
struct Referrers(BTreeMap<Id, Vec<Id>>);

impl Referrers {
    fn of(ex: &Exchange) -> Self {
        let mut map: BTreeMap<Id, Vec<Id>> = BTreeMap::new();
        for inst in ex.instances() {
            let mut refs = inst.references();
            refs.sort_unstable();
            refs.dedup();
            for r in refs {
                map.entry(r).or_default().push(inst.id);
            }
        }
        Self(map)
    }

    /// The instances referencing `id` that have `keyword` as a type.
    fn to<'a>(
        &'a self,
        ex: &'a Exchange,
        id: Id,
        keyword: &'a str,
    ) -> impl Iterator<Item = &'a Instance> {
        self.0
            .get(&id)
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .filter_map(move |i| ex.get(*i))
            .filter(move |i| i.has_type(keyword))
    }
}

/// The type names a file may use for one thing.
///
/// A file states a formation as `PRODUCT_DEFINITION_FORMATION` or as its
/// subtype `..._WITH_SPECIFIED_SOURCE`, and the NIST corpus uses both,
/// sometimes within one file. Part 21 writes a subtype's own attributes
/// after those it inherits, so the inherited ones keep their positions
/// and [`family_attr`] can read them under whichever name is there.
///
/// The families are listed rather than matched by prefix, because a
/// prefix would be wrong: `PRODUCT_DEFINITION_SHAPE` and
/// `PRODUCT_DEFINITION_CONTEXT` begin with `PRODUCT_DEFINITION` and are
/// not subtypes of it.
const DEFINITION: &[&str] = &[
    "PRODUCT_DEFINITION",
    "PRODUCT_DEFINITION_WITH_ASSOCIATED_DOCUMENTS",
];
const FORMATION: &[&str] = &[
    "PRODUCT_DEFINITION_FORMATION",
    "PRODUCT_DEFINITION_FORMATION_WITH_SPECIFIED_SOURCE",
];
const USAGE: &[&str] = &["NEXT_ASSEMBLY_USAGE_OCCURRENCE"];

/// Whether `inst` is one of `family`.
fn is_a(inst: &Instance, family: &[&str]) -> bool {
    family.iter().any(|k| inst.has_type(k))
}

/// The `n`th attribute of `inst`, read under whichever name of `family`
/// the file used.
fn family_attr<'a>(inst: &'a Instance, family: &[&str], n: usize) -> Option<&'a Parameter> {
    family.iter().find_map(|k| inst.attr(k, n))
}

/// A non-empty, trimmed string parameter.
fn text(p: Option<&Parameter>) -> Option<String> {
    let s = p?.as_str()?.trim();
    (!s.is_empty()).then(|| s.to_owned())
}

/// Read the product structure of `ex`.
pub fn read(ex: &Exchange, scale: Scale) -> (Vec<Part>, Vec<Relation>, Vec<Diagnostic>) {
    let referrers = Referrers::of(ex);
    let mut ids = ContentId::new();
    let mut diagnostics = Vec::new();

    // Every product definition is a part. Keyed by the instance so that
    // an occurrence naming a definition can find the part it made.
    let mut parts: Vec<Part> = Vec::new();
    let mut part_of: BTreeMap<Id, String> = BTreeMap::new();
    for pd in ex.instances().filter(|i| is_a(i, DEFINITION)) {
        let part = part_from(ex, pd, &mut ids);
        part_of.insert(pd.id, part.id.clone());
        parts.push(part);
    }
    tracing::debug!(count = parts.len(), "product definitions");

    // A part with nothing to be called by has nothing to key on, so its
    // id falls back to an ordinal in file order — which is the one thing
    // an id must not rest on (ADR 0004). Files like this exist: one NIST
    // model states four products with an empty number, name and
    // revision apiece. Nothing can be done about it here, so it is said
    // rather than left for a reader to discover by diffing two exports.
    let anonymous = parts
        .iter()
        .filter(|p| p.number.is_none() && p.name.is_none() && p.revision.is_none())
        .count();
    if anonymous > 1 {
        diagnostics.push(Diagnostic {
            message: format!(
                "{anonymous} parts state no number, name or revision, so they are told apart \
                 only by the order the file lists them in and their ids will not match another \
                 export of the same design"
            ),
        });
    }

    let mut relations = Vec::new();
    for nauo in ex.instances().filter(|i| is_a(i, USAGE)) {
        let end = |n: usize| {
            family_attr(nauo, USAGE, n)
                .and_then(Parameter::as_ref)
                .and_then(|id| part_of.get(&id))
                .cloned()
        };
        let (Some(parent), Some(child)) = (end(3), end(4)) else {
            diagnostics.push(Diagnostic {
                message: format!(
                    "assembly usage #{} names a product definition the file does not state, \
                     so that occurrence is not counted",
                    nauo.id
                ),
            });
            continue;
        };
        // The reference designator names the occurrence where it is
        // used; the name and the id are the writer's own labels for the
        // usage. The first that says anything is the best available.
        let name = text(family_attr(nauo, USAGE, 5))
            .or_else(|| text(family_attr(nauo, USAGE, 1)))
            .or_else(|| text(family_attr(nauo, USAGE, 0)));
        let (placement, mut refs) = placement_of(ex, &referrers, nauo.id, scale, &mut diagnostics);
        refs.push(format!("#{}", nauo.id));
        refs.sort();
        let key = [
            parent.as_str(),
            child.as_str(),
            name.as_deref().unwrap_or_default(),
            &placement_key(placement.as_ref()),
        ]
        .join(";");
        relations.push(Relation {
            id: ids.make("use", &[&key]),
            parent,
            child,
            name,
            placement,
            source_refs: refs,
        });
    }
    tracing::debug!(count = relations.len(), "assembly usages");

    (parts, relations, diagnostics)
}

/// The part one product definition describes.
fn part_from(ex: &Exchange, pd: &Instance, ids: &mut ContentId) -> Part {
    let mut source_refs = vec![format!("#{}", pd.id)];

    let formation = family_attr(pd, DEFINITION, 2)
        .and_then(Parameter::as_ref)
        .and_then(|id| ex.get(id));
    let revision = formation.and_then(|f| {
        source_refs.push(format!("#{}", f.id));
        text(family_attr(f, FORMATION, 0))
    });
    let product = formation
        .and_then(|f| family_attr(f, FORMATION, 2))
        .and_then(Parameter::as_ref)
        .and_then(|id| ex.get(id));

    let (number, name, description) = match product {
        Some(p) => {
            source_refs.push(format!("#{}", p.id));
            (
                text(p.attr("PRODUCT", 0)),
                text(p.attr("PRODUCT", 1)),
                text(p.attr("PRODUCT", 2)),
            )
        }
        None => (None, None, None),
    };

    // A part is what a person would call it: number, name, revision.
    // Not the geometry — a subassembly has none — and not the entity
    // numbering, which changes on every export (ADR 0004).
    let key = [
        number.as_deref().unwrap_or_default(),
        name.as_deref().unwrap_or_default(),
        revision.as_deref().unwrap_or_default(),
    ]
    .join(";");

    // A writer that puts the same string in both says it once here.
    let name = name.filter(|n| Some(n) != number.as_ref());

    Part {
        id: ids.make("part", &[&key]),
        number,
        name,
        description,
        revision,
        occurrences: 0,
        source_refs,
    }
}

/// Where the occurrence `nauo` puts its child, and the entities that say
/// so.
///
/// The transformation states two placements: the frame the parent is
/// measured in and the frame the child is placed at. Writers set the
/// first to the identity and put the occurrence in the second, and this
/// reads the second. Where the first is *not* the identity the reading
/// would be wrong, so that is said rather than passed over.
fn placement_of(
    ex: &Exchange,
    referrers: &Referrers,
    nauo: Id,
    scale: Scale,
    diagnostics: &mut Vec<Diagnostic>,
) -> (Option<Placement>, Vec<String>) {
    let mut refs = Vec::new();
    for shape in referrers.to(ex, nauo, "PRODUCT_DEFINITION_SHAPE") {
        for cdsr in referrers.to(ex, shape.id, "CONTEXT_DEPENDENT_SHAPE_REPRESENTATION") {
            let Some(rel) = cdsr
                .attr("CONTEXT_DEPENDENT_SHAPE_REPRESENTATION", 0)
                .and_then(Parameter::as_ref)
                .and_then(|id| ex.get(id))
            else {
                continue;
            };
            let Some(transform) = rel
                .attr("REPRESENTATION_RELATIONSHIP_WITH_TRANSFORMATION", 0)
                .and_then(Parameter::as_ref)
                .and_then(|id| ex.get(id))
            else {
                continue;
            };
            let at = |n: usize| {
                transform
                    .attr("ITEM_DEFINED_TRANSFORMATION", n)
                    .and_then(Parameter::as_ref)
                    .and_then(|id| ex.get(id))
                    .map(|a| axis_placement(ex, a, scale))
            };
            let (parent_frame, child_frame) = (at(2), at(3));
            let Some(child_frame) = child_frame else {
                continue;
            };
            if parent_frame.as_ref().is_some_and(|p| !is_identity(p)) {
                diagnostics.push(Diagnostic {
                    message: format!(
                        "assembly usage #{nauo} is measured from a frame that is not the model \
                         origin, so its placement is stated relative to that frame rather than \
                         to the parent"
                    ),
                });
            }
            refs.push(format!("#{}", cdsr.id));
            refs.push(format!("#{}", transform.id));
            return (Some(child_frame), refs);
        }
    }
    (None, refs)
}

/// An `AXIS2_PLACEMENT_3D` in millimetres.
fn axis_placement(ex: &Exchange, inst: &Instance, scale: Scale) -> Placement {
    let at = |n: usize| {
        inst.parameters()
            .get(n)
            .and_then(Parameter::as_ref)
            .and_then(|id| ex.get(id))
            .and_then(coords)
    };
    Placement {
        origin: at(1).map(|p| scale.point(p)).unwrap_or([0.0; 3]),
        axis: at(2).map(direction),
        ref_direction: at(3).map(direction),
    }
}

/// The three coordinates of a `CARTESIAN_POINT` or `DIRECTION`.
fn coords(inst: &Instance) -> Option<[f64; 3]> {
    let l = inst.parameters().get(1)?.as_list()?;
    Some([
        l.first()?.as_f64()?,
        l.get(1)?.as_f64()?,
        l.get(2).and_then(Parameter::as_f64).unwrap_or(0.0),
    ])
}

fn direction(v: [f64; 3]) -> Direction {
    Direction {
        x: v[0],
        y: v[1],
        z: v[2],
    }
}

/// Whether a placement leaves the child where it was: at the origin,
/// looking along z, with x to the right — or with either direction left
/// unstated, which means the same thing.
fn is_identity(p: &Placement) -> bool {
    let same = |d: Option<&Direction>, to: [f64; 3]| match d {
        None => true,
        Some(d) => {
            (d.x - to[0]).abs() < 1e-9 && (d.y - to[1]).abs() < 1e-9 && (d.z - to[2]).abs() < 1e-9
        }
    };
    p.origin.iter().all(|v| v.abs() < 1e-9)
        && same(p.axis.as_ref(), [0.0, 0.0, 1.0])
        && same(p.ref_direction.as_ref(), [1.0, 0.0, 0.0])
}

/// A placement as one string, for the relation's id.
///
/// Two occurrences of one part under one parent differ only by where
/// they sit, so the id has to carry the position or the second would
/// collide with the first and be told apart only by an ordinal.
/// Coordinates are rounded to the identity quantum, so that two exports
/// of one assembly key an occurrence the same way (ADR 0004).
fn placement_key(p: Option<&Placement>) -> String {
    let Some(p) = p else {
        return String::new();
    };
    let q = crate::fingerprint::identity_quantum();
    let axis = |d: Option<&Direction>| match d {
        Some(d) => triple([d.x, d.y, d.z], q),
        None => String::new(),
    };
    format!(
        "{}|{}|{}",
        triple(p.origin, q),
        axis(p.axis.as_ref()),
        axis(p.ref_direction.as_ref())
    )
}

/// Build the whole document for a parsed exchange.
pub fn document(ex: &Exchange, source: crate::model::Source) -> ProductDocument {
    let declared = super::pmi::units_of(ex);
    let scale = Scale::of(declared.length.as_deref(), declared.angle.as_deref());
    let (parts, relations, mut diagnostics) = read(ex, scale);
    if parts.is_empty() {
        diagnostics.push(Diagnostic {
            message: "the file states no product definition, so it names no part".into(),
        });
    }
    let mut doc = ProductDocument {
        schema_version: SCHEMA_VERSION,
        source,
        units: crate::features::Units {
            length: "mm".into(),
            angle: "deg".into(),
            declared_length: declared.length,
            declared_angle: declared.angle,
        },
        parts,
        relations,
        roots: Vec::new(),
        diagnostics,
    };
    doc.settle();
    doc
}
