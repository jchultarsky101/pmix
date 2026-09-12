//! Reading a STEP file's product structure (ADR 0014).
//!
//! The PMI side of this reader already climbs part of this chain: a
//! property attached to a product definition is attributed to the part
//! by following shape → definition → formation → product (ADR 0007).
//! That walk is `product_at` below, which the property reader now calls
//! rather than keeping a second copy of.
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
use crate::product::model::{
    Approval, Classification, Diagnostic, Involvement, Part, ProductDocument, Relation,
    SCHEMA_VERSION, Unattached,
};
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

    /// The instances referencing `id` that are one of `family`.
    fn to_any<'a>(
        &'a self,
        ex: &'a Exchange,
        id: Id,
        family: &'a [&'a str],
    ) -> impl Iterator<Item = &'a Instance> {
        self.0
            .get(&id)
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .filter_map(move |i| ex.get(*i))
            .filter(move |i| is_a(i, family))
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

/// What a file may wrap a shell in on the way to a representation.
const SOLID_MODEL: &[&str] = &[
    "MANIFOLD_SOLID_BREP",
    "BREP_WITH_VOIDS",
    "SHELL_BASED_SURFACE_MODEL",
    "FACETED_BREP",
];

/// What holds a solid model and is itself defined by a product.
const REPRESENTATION: &[&str] = &[
    "ADVANCED_BREP_SHAPE_REPRESENTATION",
    "MANIFOLD_SURFACE_SHAPE_REPRESENTATION",
    "FACETED_BREP_SHAPE_REPRESENTATION",
    "SHAPE_REPRESENTATION",
    "REPRESENTATION",
];

/// What ties a representation to the product definition it is the shape
/// of.
const SHAPE_DEFINITION: &[&str] = &[
    "SHAPE_DEFINITION_REPRESENTATION",
    "PROPERTY_DEFINITION_REPRESENTATION",
];

/// A non-empty, trimmed string parameter.
fn text(p: Option<&Parameter>) -> Option<String> {
    let s = p?.as_str()?.trim();
    (!s.is_empty()).then(|| s.to_owned())
}

/// Read the product structure of `ex`.
///
/// `bodies` pairs each shell the file states with the id the features
/// document gives the body read from it, so that each body can be put
/// under the part whose shape holds it.
pub fn read(
    ex: &Exchange,
    scale: Scale,
    bodies: &[(Id, String)],
) -> (Vec<Part>, Vec<Relation>, Vec<Unattached>, Vec<Diagnostic>) {
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

    identity(ex, &referrers, &mut parts, &part_of);

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

    // Each body goes under the part whose shape representation holds the
    // shell it was read from. A body that reaches no product definition
    // is listed rather than dropped: a file can state geometry it never
    // defines a product for, and silence would make that look like a
    // part with no shape.
    let mut unattached = Vec::new();
    let by_part: BTreeMap<Id, Vec<String>> = {
        let mut map: BTreeMap<Id, Vec<String>> = BTreeMap::new();
        for (shell, body) in bodies {
            match definition_holding(ex, &referrers, *shell) {
                Some(pd) => map.entry(pd).or_default().push(body.clone()),
                None => unattached.push(Unattached {
                    body: body.clone(),
                    reason: format!(
                        "shell #{shell} reaches no product definition, so the file states this \
                         shape without saying which part it is"
                    ),
                }),
            }
        }
        map
    };
    for (pd, body_ids) in by_part {
        let Some(part_id) = part_of.get(&pd) else {
            continue;
        };
        if let Some(part) = parts.iter_mut().find(|p| &p.id == part_id) {
            part.bodies.extend(body_ids);
        }
    }
    tracing::debug!(count = unattached.len(), "bodies under no part");

    (parts, relations, unattached, diagnostics)
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
        people: Vec::new(),
        approvals: Vec::new(),
        classification: None,
        categories: Vec::new(),
        bodies: Vec::new(),
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
    for shape in referrers.to_any(ex, nauo, &["PRODUCT_DEFINITION_SHAPE"]) {
        for cdsr in referrers.to_any(ex, shape.id, &["CONTEXT_DEPENDENT_SHAPE_REPRESENTATION"]) {
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

/// The management data a file records against its products: who owns
/// and made them, what was approved and when, how they are classified,
/// and what category they fall in.
///
/// AP203 states these with `CC_DESIGN_*` assignments and AP242 with
/// `APPLIED_*_ASSIGNMENT`; the shape is the same — the thing assigned,
/// sometimes a role, and the items it is assigned to — so one reader
/// takes both. An item is whatever the writer chose to hang the
/// assignment on: the product, its formation, or its definition. All of
/// them are resolved to the product, and the record goes on every part
/// of that product.
///
/// A classification whose every field is blank is still a classification
/// — real files write exactly that — and it is kept so that *stated but
/// empty* is distinguishable from *not stated* (ADR 0014).
fn identity(
    ex: &Exchange,
    referrers: &Referrers,
    parts: &mut [Part],
    part_of: &BTreeMap<Id, String>,
) {
    // Product id -> the part ids that realise it, so an assignment on a
    // product reaches every definition of it.
    let mut parts_of_product: BTreeMap<Id, Vec<String>> = BTreeMap::new();
    for (pd, part_id) in part_of {
        if let Some(product) = product_of_any(ex, *pd) {
            parts_of_product
                .entry(product)
                .or_default()
                .push(part_id.clone());
        }
    }
    // An assignment's item is not always a product. The officer who
    // classified a part is assigned to the *classification*, and an
    // approver may be assigned to the *approval*; both are about the
    // part those records sit on. So the records are mapped to their
    // parts first, and an item that is one of them resolves through it.
    let mut parts_of_record: BTreeMap<Id, Vec<String>> = BTreeMap::new();
    for inst in ex.instances() {
        let (record, items) = if is_a(inst, CLASSIFICATION_ASSIGNMENT) {
            (
                family_attr(inst, CLASSIFICATION_ASSIGNMENT, 0),
                family_attr(inst, CLASSIFICATION_ASSIGNMENT, 1),
            )
        } else if is_a(inst, APPROVAL_ASSIGNMENT) {
            (
                family_attr(inst, APPROVAL_ASSIGNMENT, 0),
                family_attr(inst, APPROVAL_ASSIGNMENT, 1),
            )
        } else {
            continue;
        };
        let (Some(record), Some(items)) = (record.and_then(Parameter::as_ref), items) else {
            continue;
        };
        let mut ids = Vec::new();
        items.collect_refs(&mut ids);
        for item in ids {
            if let Some(ps) = product_of_any(ex, item).and_then(|p| parts_of_product.get(&p)) {
                parts_of_record
                    .entry(record)
                    .or_default()
                    .extend(ps.iter().cloned());
            }
        }
    }
    let targets = |items: &Parameter| -> Vec<String> {
        let mut ids = Vec::new();
        items.collect_refs(&mut ids);
        let mut out = Vec::new();
        for item in ids {
            if let Some(ps) = product_of_any(ex, item).and_then(|p| parts_of_product.get(&p)) {
                out.extend(ps.iter().cloned());
            } else if let Some(ps) = parts_of_record.get(&item) {
                out.extend(ps.iter().cloned());
            }
        }
        out
    };
    let mut apply = |part_ids: Vec<String>, f: &mut dyn FnMut(&mut Part)| {
        for part in parts.iter_mut() {
            if part_ids.contains(&part.id) {
                f(part);
            }
        }
    };

    for inst in ex.instances() {
        if is_a(inst, PERSON_ASSIGNMENT) {
            let Some(who) = family_attr(inst, PERSON_ASSIGNMENT, 0)
                .and_then(Parameter::as_ref)
                .and_then(|id| {
                    involvement(
                        ex,
                        id,
                        role_name(ex, family_attr(inst, PERSON_ASSIGNMENT, 1)),
                    )
                })
            else {
                continue;
            };
            let Some(items) = family_attr(inst, PERSON_ASSIGNMENT, 2) else {
                continue;
            };
            apply(targets(items), &mut |p| p.people.push(who.clone()));
        } else if is_a(inst, APPROVAL_ASSIGNMENT) {
            let Some(approval) = family_attr(inst, APPROVAL_ASSIGNMENT, 0)
                .and_then(Parameter::as_ref)
                .and_then(|id| approval(ex, referrers, id))
            else {
                continue;
            };
            let Some(items) = family_attr(inst, APPROVAL_ASSIGNMENT, 1) else {
                continue;
            };
            apply(targets(items), &mut |p| p.approvals.push(approval.clone()));
        } else if is_a(inst, CLASSIFICATION_ASSIGNMENT) {
            let Some(class) = family_attr(inst, CLASSIFICATION_ASSIGNMENT, 0)
                .and_then(Parameter::as_ref)
                .and_then(|id| ex.get(id))
                .filter(|c| c.has_type("SECURITY_CLASSIFICATION"))
            else {
                continue;
            };
            let level = class
                .attr("SECURITY_CLASSIFICATION", 2)
                .and_then(Parameter::as_ref)
                .and_then(|id| ex.get(id))
                .and_then(|l| text(l.attr("SECURITY_CLASSIFICATION_LEVEL", 0)));
            let stated = Classification {
                level,
                name: text(class.attr("SECURITY_CLASSIFICATION", 0)),
                purpose: text(class.attr("SECURITY_CLASSIFICATION", 1)),
            };
            let Some(items) = family_attr(inst, CLASSIFICATION_ASSIGNMENT, 1) else {
                continue;
            };
            // A part classified twice keeps the first; two markings on
            // one part is a file problem this cannot settle.
            apply(targets(items), &mut |p| {
                p.classification.get_or_insert_with(|| stated.clone());
            });
        } else if inst.has_type("PRODUCT_RELATED_PRODUCT_CATEGORY") {
            let Some(name) = text(inst.attr("PRODUCT_RELATED_PRODUCT_CATEGORY", 0)) else {
                continue;
            };
            let Some(items) = inst.attr("PRODUCT_RELATED_PRODUCT_CATEGORY", 2) else {
                continue;
            };
            apply(targets(items), &mut |p| p.categories.push(name.clone()));
        }
    }
}

/// The product a product-level entity belongs to, reading the
/// formation under whichever name the file used.
///
/// Not [`product_at`]: that walk knows the plain
/// `PRODUCT_DEFINITION_FORMATION` only, and the property reader shares
/// it, so teaching it the subtype would move every property id in a
/// file that uses one. This walk is private to the identity reader and
/// reads the families, as the part reader does — and it has to, because
/// the D2MI models state their formation as
/// `..._WITH_SPECIFIED_SOURCE`, and under the exact-name walk every
/// assignment in them reached no part at all.
fn product_of_any(ex: &Exchange, target: Id) -> Option<Id> {
    let mut at = target;
    for _ in 0..4 {
        let inst = ex.get(at)?;
        if inst.has_type("PRODUCT") {
            return Some(at);
        }
        at = if is_a(inst, DEFINITION) {
            family_attr(inst, DEFINITION, 2)?.as_ref()?
        } else if is_a(inst, FORMATION) {
            family_attr(inst, FORMATION, 2)?.as_ref()?
        } else if inst.has_type("PRODUCT_DEFINITION_SHAPE") {
            inst.attr("PRODUCT_DEFINITION_SHAPE", 2)?.as_ref()?
        } else {
            return None;
        };
    }
    None
}

/// The two spellings of each assignment: AP203's and AP242's.
const PERSON_ASSIGNMENT: &[&str] = &[
    "APPLIED_PERSON_AND_ORGANIZATION_ASSIGNMENT",
    "CC_DESIGN_PERSON_AND_ORGANIZATION_ASSIGNMENT",
];
const APPROVAL_ASSIGNMENT: &[&str] = &["APPLIED_APPROVAL_ASSIGNMENT", "CC_DESIGN_APPROVAL"];
const CLASSIFICATION_ASSIGNMENT: &[&str] = &[
    "APPLIED_SECURITY_CLASSIFICATION_ASSIGNMENT",
    "CC_DESIGN_SECURITY_CLASSIFICATION",
];

/// The name a role entity carries.
fn role_name(ex: &Exchange, role: Option<&Parameter>) -> String {
    role.and_then(Parameter::as_ref)
        .and_then(|id| ex.get(id))
        .and_then(|r| {
            text(r.attr("PERSON_AND_ORGANIZATION_ROLE", 0))
                .or_else(|| text(r.attr("APPROVAL_ROLE", 0)))
        })
        .unwrap_or_else(|| "unspecified".into())
}

/// A person-and-organisation, named.
fn involvement(ex: &Exchange, id: Id, role: String) -> Option<Involvement> {
    let pao = ex.get(id)?;
    if !pao.has_type("PERSON_AND_ORGANIZATION") {
        return None;
    }
    let person = pao
        .attr("PERSON_AND_ORGANIZATION", 0)
        .and_then(Parameter::as_ref)
        .and_then(|id| ex.get(id))
        .and_then(|p| {
            // `Last, First`, or whichever of the two the file gives.
            let last = text(p.attr("PERSON", 1));
            let first = text(p.attr("PERSON", 2));
            match (last, first) {
                (Some(l), Some(f)) => Some(format!("{l}, {f}")),
                (Some(l), None) => Some(l),
                (None, Some(f)) => Some(f),
                (None, None) => None,
            }
        });
    let organisation = pao
        .attr("PERSON_AND_ORGANIZATION", 1)
        .and_then(Parameter::as_ref)
        .and_then(|id| ex.get(id))
        .and_then(|o| text(o.attr("ORGANIZATION", 1)).or_else(|| text(o.attr("ORGANIZATION", 0))));
    Some(Involvement {
        role,
        organisation,
        person,
    })
}

/// An approval: its status and level, when it was given, and by whom.
///
/// The date and the approver both point *at* the approval rather than
/// being pointed at by it, which is what the reverse index is for.
fn approval(ex: &Exchange, referrers: &Referrers, id: Id) -> Option<Approval> {
    let a = ex.get(id)?;
    if !a.has_type("APPROVAL") {
        return None;
    }
    let status = a
        .attr("APPROVAL", 0)
        .and_then(Parameter::as_ref)
        .and_then(|id| ex.get(id))
        .and_then(|s| text(s.attr("APPROVAL_STATUS", 0)))
        .unwrap_or_else(|| "unspecified".into());
    let level = text(a.attr("APPROVAL", 1));
    let date = referrers
        .to_any(ex, id, &["APPROVAL_DATE_TIME"])
        .find_map(|adt| {
            adt.attr("APPROVAL_DATE_TIME", 0)
                .and_then(Parameter::as_ref)
                .and_then(|id| calendar_date(ex, id))
        });
    let mut by: Vec<Involvement> = referrers
        .to_any(ex, id, &["APPROVAL_PERSON_ORGANIZATION"])
        .filter_map(|apo| {
            let role = role_name(ex, apo.attr("APPROVAL_PERSON_ORGANIZATION", 2));
            apo.attr("APPROVAL_PERSON_ORGANIZATION", 0)
                .and_then(Parameter::as_ref)
                .and_then(|id| involvement(ex, id, role))
        })
        .collect();
    by.sort();
    Some(Approval {
        status,
        level,
        date,
        by,
    })
}

/// A date as `YYYY-MM-DD`, from a `DATE_AND_TIME` or a `CALENDAR_DATE`.
///
/// `CALENDAR_DATE` states year, **day**, month — in that order. It is
/// easy to read the second field as the month, and a file dated the
/// 17th of July then reads as the 7th of the seventeenth month.
fn calendar_date(ex: &Exchange, id: Id) -> Option<String> {
    let inst = ex.get(id)?;
    let date = if inst.has_type("DATE_AND_TIME") {
        inst.attr("DATE_AND_TIME", 0)
            .and_then(Parameter::as_ref)
            .and_then(|id| ex.get(id))?
    } else {
        inst
    };
    if !date.has_type("CALENDAR_DATE") {
        return None;
    }
    let n = |i: usize| date.attr("CALENDAR_DATE", i).and_then(Parameter::as_i64);
    let (year, day, month) = (n(0)?, n(1)?, n(2)?);
    // A writer with no date to give fills the fields with zeros. That is
    // a blank, and printing it as the first of January in year nought
    // would present a placeholder as a fact.
    if year <= 0 || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(format!("{year:04}-{month:02}-{day:02}"))
}

/// The product definition whose shape holds `shell`, if the file says.
///
/// Up rather than down: a shell does not name the solid that holds it,
/// the solid does not name the representation, and the representation
/// does not name the product definition it is the shape of. Every arrow
/// in this walk runs backwards, which is what the reverse index is for.
///
/// A file may also state the geometry in one representation and define
/// the product against another, tying the two with a shape
/// representation relationship. One hop across that is followed, which
/// covers what real writers do without turning the walk into a search.
fn definition_holding(ex: &Exchange, referrers: &Referrers, shell: Id) -> Option<Id> {
    // The representations that hold this shell, directly or through the
    // solid model that wraps it.
    let mut reps: Vec<Id> = Vec::new();
    for solid in referrers.to_any(ex, shell, SOLID_MODEL) {
        reps.extend(referrers.to_any(ex, solid.id, REPRESENTATION).map(|r| r.id));
    }
    reps.extend(referrers.to_any(ex, shell, REPRESENTATION).map(|r| r.id));
    reps.sort_unstable();
    reps.dedup();

    for hop in 0..2 {
        for rep in &reps {
            if let Some(pd) = definition_of(ex, referrers, *rep) {
                return Some(pd);
            }
        }
        if hop == 0 {
            // Follow shape representation relationships to whatever else
            // stands for the same shape, then try again.
            let mut next = Vec::new();
            for rep in &reps {
                for rel in referrers.to_any(ex, *rep, &["SHAPE_REPRESENTATION_RELATIONSHIP"]) {
                    for other in rel.references() {
                        if other != *rep && ex.get(other).is_some_and(|i| is_a(i, REPRESENTATION)) {
                            next.push(other);
                        }
                    }
                }
            }
            next.sort_unstable();
            next.dedup();
            if next.is_empty() {
                return None;
            }
            reps = next;
        }
    }
    None
}

/// The product definition a representation is the shape of.
fn definition_of(ex: &Exchange, referrers: &Referrers, rep: Id) -> Option<Id> {
    for sdr in referrers.to_any(ex, rep, SHAPE_DEFINITION) {
        let Some(shape) = family_attr(sdr, SHAPE_DEFINITION, 0)
            .and_then(Parameter::as_ref)
            .and_then(|id| ex.get(id))
        else {
            continue;
        };
        // A product definition shape stands for a part; the same entity
        // pointed at an assembly usage stands for one occurrence of one,
        // and a body belongs to the part rather than to the occurrence.
        let Some(defined) = shape
            .attr("PRODUCT_DEFINITION_SHAPE", 2)
            .and_then(Parameter::as_ref)
        else {
            continue;
        };
        if ex.get(defined).is_some_and(|i| is_a(i, DEFINITION)) {
            return Some(defined);
        }
    }
    None
}

/// The box around the whole assembly.
///
/// Each body's box is stated in its part's own coordinates, so every
/// occurrence of that part puts a copy of it somewhere else. Only the
/// translation is applied: a placement also carries directions, and
/// turning a rotated box into an axis-aligned one needs its corners
/// rather than its extremes. Where an occurrence *is* rotated the box is
/// marked approximate rather than silently wrong.
fn assembled(
    parts: &[Part],
    relations: &[Relation],
    by_body: &std::collections::BTreeMap<&str, &crate::features::Envelope>,
) -> Option<crate::features::Envelope> {
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    let mut approximate = false;
    let mut any = false;

    for part in parts {
        // Where this part is used. A part nothing places is taken to sit
        // at the origin, which is what a file holding one part means.
        let places: Vec<[f64; 3]> = {
            let used: Vec<[f64; 3]> = relations
                .iter()
                .filter(|r| r.child == part.id)
                .map(|r| r.placement.as_ref().map(|p| p.origin).unwrap_or([0.0; 3]))
                .collect();
            if used.is_empty() {
                vec![[0.0; 3]]
            } else {
                used
            }
        };
        if relations
            .iter()
            .filter(|r| r.child == part.id)
            .any(|r| r.placement.as_ref().is_some_and(is_turned))
        {
            approximate = true;
        }
        for body in &part.bodies {
            let Some(box_) = by_body.get(body.as_str()) else {
                continue;
            };
            approximate |= box_.approximate;
            for at in &places {
                any = true;
                for i in 0..3 {
                    min[i] = min[i].min(box_.min[i] + at[i]);
                    max[i] = max[i].max(box_.max[i] + at[i]);
                }
            }
        }
    }

    if !any {
        return None;
    }
    let mut size = [max[0] - min[0], max[1] - min[1], max[2] - min[2]];
    size.sort_by(|a, b| b.total_cmp(a));
    Some(crate::features::Envelope {
        min,
        max,
        size,
        approximate,
    })
}

/// Whether a placement turns what it places, rather than only moving it.
fn is_turned(p: &Placement) -> bool {
    let same = |d: Option<&Direction>, to: [f64; 3]| match d {
        None => true,
        Some(d) => {
            (d.x - to[0]).abs() < 1e-9 && (d.y - to[1]).abs() < 1e-9 && (d.z - to[2]).abs() < 1e-9
        }
    };
    !(same(p.axis.as_ref(), [0.0, 0.0, 1.0]) && same(p.ref_direction.as_ref(), [1.0, 0.0, 0.0]))
}

/// Build the whole document for a parsed exchange.
///
/// The bodies are recognised here rather than taken from a features
/// document a caller might pass in, so that the two cannot be of
/// different files, and through the same entry point the features
/// document uses, so that a body has one id whichever document names it.
pub fn document(ex: &Exchange, source: crate::model::Source) -> ProductDocument {
    let declared = super::pmi::units_of(ex);
    let scale = Scale::of(declared.length.as_deref(), declared.angle.as_deref());

    let shelled = crate::features::step::solids_with_shells(ex, scale);
    let solids: Vec<_> = shelled.iter().map(|(s, _)| s.clone()).collect();
    let shapes = crate::features::recognise_in_order(&solids);
    let bodies: Vec<(Id, String)> = shapes
        .iter()
        .zip(shelled.iter())
        .map(|(body, (_, shell))| (*shell, body.id.clone()))
        .collect();

    let (parts, relations, unattached, mut diagnostics) = read(ex, scale, &bodies);

    // The assembly's own size: each body's box, put where its
    // occurrences put it. A body states its shape in its own
    // coordinates, and only the placements say where those coordinates
    // sit (ADR 0014).
    let by_body: std::collections::BTreeMap<&str, &crate::features::Envelope> = shapes
        .iter()
        .filter_map(|b| b.envelope.as_ref().map(|e| (b.id.as_str(), e)))
        .collect();
    let envelope = assembled(&parts, &relations, &by_body);
    if parts.is_empty() {
        diagnostics.push(Diagnostic {
            message: "the file states no product definition, so it names no part".into(),
        });
    }
    let mut doc = ProductDocument {
        schema_version: SCHEMA_VERSION,
        source,
        envelope,
        units: crate::features::Units {
            length: "mm".into(),
            angle: "deg".into(),
            declared_length: declared.length,
            declared_angle: declared.angle,
        },
        parts,
        relations,
        unattached,
        roots: Vec::new(),
        diagnostics,
    };
    doc.settle();
    doc
}
