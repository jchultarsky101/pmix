//! Recognising features from geometry (ADR 0011), and the promise that
//! makes the result usable for comparison (ADR 0012).
//!
//! The synthetic plates are one design and three single deliberate
//! changes to it: a wider hole, a moved hole, and the same design stated
//! in inches. Each is here because it is a question people bring to a
//! pair of CAD files, and the point of the document is that answering it
//! needs only one field of it.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use pmix::features::{self, FeatureDocument, Kind};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn read(name: &str) -> FeatureDocument {
    features::read_path(&fixture(name)).expect("the fixture reads")
}

/// The one feature of a one-feature body.
fn only_feature(doc: &FeatureDocument) -> &features::Feature {
    let body = doc.bodies.first().expect("a body");
    assert_eq!(body.features.len(), 1, "{:?}", body.features);
    &body.features[0]
}

#[test]
fn a_plate_with_one_hole_yields_that_hole_and_accounts_for_the_rest() {
    let doc = read("synthetic/plate_one_hole.stp");
    assert_eq!(doc.bodies.len(), 1);
    let body = &doc.bodies[0];
    let hole = only_feature(&doc);
    assert_eq!(hole.kind, Kind::Hole);
    assert_eq!(hole.shape.diameter, Some(8.0));
    assert_eq!(hole.shape.depth, Some(10.0));
    assert_eq!(hole.shape.through, Some(true));
    assert_eq!(hole.shape.position, Some([20.0, 15.0, 0.0]));

    // The six sides of the plate went into nothing, and say so. A
    // recogniser that reported the hole and stayed quiet about them
    // would invite the reader to assume there was nothing else.
    assert_eq!(body.faces.total, 7);
    assert_eq!(body.faces.in_features, 1);
    assert_eq!(body.unassigned.len(), 6);
    assert!(body.unassigned.iter().all(|u| u.surface == "plane"));
}

#[test]
fn the_numbers_are_millimetres_whatever_the_file_declared() {
    let mm = read("synthetic/plate_one_hole.stp");
    let inches = read("synthetic/plate_one_hole_inches.stp");
    assert_eq!(mm.units.declared_length.as_deref(), Some("mm"));
    assert_eq!(inches.units.declared_length.as_deref(), Some("in"));
    assert_eq!(inches.units.length, "mm");

    // One design, so one document: the same ids and the same numbers,
    // not merely the same shape described twice.
    assert_eq!(mm.bodies, inches.bodies);
}

#[test]
fn boring_the_hole_wider_changes_the_diameter_and_nothing_else() {
    let before = read("synthetic/plate_one_hole.stp");
    let after = read("synthetic/plate_hole_larger.stp");
    let (a, b) = (only_feature(&before), only_feature(&after));

    assert_eq!(a.shape.diameter, Some(8.0));
    assert_eq!(b.shape.diameter, Some(10.0));
    // Everything that did not change reads the same, which is what lets
    // a reader of the two documents say the hole was bored rather than
    // moved, replaced, or renumbered.
    assert_eq!(a.shape.position, b.shape.position);
    assert_eq!(a.shape.extent, b.shape.extent);
    assert_eq!(a.shape.depth, b.shape.depth);
    assert_eq!(a.shape.axis, b.shape.axis);
    assert_eq!(a.kind, b.kind);
    // The plate around it is untouched, so its faces keep their ids.
    let faces = |d: &FeatureDocument| -> BTreeSet<String> {
        d.bodies[0]
            .unassigned
            .iter()
            .map(|u| u.id.clone())
            .collect()
    };
    assert_eq!(faces(&before), faces(&after));
}

#[test]
fn moving_the_hole_changes_the_position_and_nothing_else() {
    let before = read("synthetic/plate_one_hole.stp");
    let after = read("synthetic/plate_hole_moved.stp");
    let (a, b) = (only_feature(&before), only_feature(&after));

    assert_eq!(a.shape.position, Some([20.0, 15.0, 0.0]));
    assert_eq!(b.shape.position, Some([25.0, 15.0, 0.0]));
    assert_eq!(a.shape.diameter, b.shape.diameter);
    assert_eq!(a.shape.depth, b.shape.depth);
    assert_eq!(a.shape.extent, b.shape.extent);
    assert_eq!(a.shape.axis, b.shape.axis);
}

/// The identity of a hole is geometric, so a hole that changed is a
/// different hole and says so by its id. That is what makes an exact
/// match worth trusting, and it is why matching cannot be the whole
/// answer: the two documents above differ in one number, and their ids
/// agree about nothing.
#[test]
fn a_changed_hole_gets_a_different_id() {
    let a = only_feature(&read("synthetic/plate_one_hole.stp"))
        .id
        .clone();
    let wider = only_feature(&read("synthetic/plate_hole_larger.stp"))
        .id
        .clone();
    let moved = only_feature(&read("synthetic/plate_hole_moved.stp"))
        .id
        .clone();
    assert_ne!(a, wider);
    assert_ne!(a, moved);
    assert_ne!(wider, moved);
}

/// Most exporters do not write a bore as one cylindrical face. They cut
/// it into halves meeting along two straight edges. It is the same bore,
/// and a document that called it something else would pair with nothing.
#[test]
fn a_bore_cut_into_halves_is_one_hole_with_one_id() {
    let whole = read("synthetic/plate_one_hole.stp");
    let split = read("synthetic/plate_hole_split.stp");
    let (a, b) = (only_feature(&whole), only_feature(&split));

    // The file states one more face, and the hole is made of two.
    assert_eq!(whole.bodies[0].faces.total, 7);
    assert_eq!(split.bodies[0].faces.total, 8);
    assert_eq!(a.faces.len(), 1);
    assert_eq!(b.faces.len(), 2);

    // None of which the description depends on.
    assert_eq!(a.id, b.id);
    assert_eq!(a.kind, b.kind);
    assert_eq!(a.shape, b.shape);

    // Nor does pairing the bodies they are in, which is the first thing
    // anything comparing two documents has to do.
    assert_eq!(whole.bodies[0].id, split.bodies[0].id);

    // The plate around the bore is untouched either way.
    assert_eq!(whole.bodies[0].unassigned, split.bodies[0].unassigned);
}

/// A blend is decided at the join, not on the face: the surface carries
/// on smoothly through it. Which side the material is on is what makes
/// the same R5 cylinder a round on an outside corner and a fillet on an
/// inside one, and the two look nothing alike on the part.
#[test]
fn a_blend_is_a_round_outside_and_a_fillet_inside() {
    let outside = read("synthetic/plate_corner_round.stp");
    let inside = read("synthetic/plate_inside_fillet.stp");
    let (r, f) = (only_feature(&outside), only_feature(&inside));

    assert_eq!(r.kind, Kind::Round);
    assert_eq!(f.kind, Kind::Fillet);
    // The same blend radius either way, so nothing but the side it is
    // on separates them.
    assert_eq!(r.shape.radius, Some(5.0));
    assert_eq!(f.shape.radius, Some(5.0));
    assert_eq!(r.shape.length, Some(10.0));
    assert_eq!(f.shape.length, Some(10.0));
    // The corner it rounds, and the corner it fills.
    assert_eq!(r.shape.position, Some([5.0, 5.0, 0.0]));
    assert_eq!(f.shape.position, Some([20.0, 20.0, 0.0]));

    // The flat faces around them are not blends and say so.
    assert_eq!(outside.bodies[0].unassigned.len(), 6);
    assert_eq!(inside.bodies[0].unassigned.len(), 8);
}

/// A cone meeting a bore is a countersink; the same cone meeting a shaft
/// is a chamfer. Nothing about the cone itself separates them — only
/// what it opens into does.
#[test]
fn a_cone_beside_a_shaft_is_a_chamfer_not_a_countersink() {
    let doc = read("synthetic/shaft_chamfered.stp");
    let body = &doc.bodies[0];
    let kinds: Vec<Kind> = body.features.iter().map(|f| f.kind).collect();
    assert!(kinds.contains(&Kind::Chamfer), "{kinds:?}");
    assert!(kinds.contains(&Kind::Boss), "{kinds:?}");
    assert!(!kinds.contains(&Kind::Countersink), "{kinds:?}");

    let chamfer = body
        .features
        .iter()
        .find(|f| f.kind == Kind::Chamfer)
        .unwrap();
    // The full angle at the apex, as a countersink states one: a
    // chamfer cut at forty-five degrees a side is ninety included.
    assert_eq!(chamfer.shape.angle, Some(90.0));
    assert_eq!(chamfer.shape.diameter, Some(20.0));
    assert_eq!(chamfer.shape.extent, Some([25.0, 30.0]));
}

/// Each feature states the measurement it is actually called by, and
/// leaves the others out rather than filling them in with something a
/// reader would then compare against the wrong thing.
#[test]
fn a_blend_states_a_radius_and_a_bore_a_diameter() {
    let hole = only_feature(&read("synthetic/plate_one_hole.stp"))
        .shape
        .clone();
    assert!(hole.diameter.is_some() && hole.depth.is_some() && hole.through.is_some());
    assert!(hole.radius.is_none() && hole.length.is_none());

    let round = only_feature(&read("synthetic/plate_corner_round.stp"))
        .shape
        .clone();
    assert!(round.radius.is_some() && round.length.is_some());
    // A blend runs along an edge rather than into the material, so it
    // has neither a depth nor an answer to whether it goes through.
    assert!(round.diameter.is_none() && round.depth.is_none());
    assert!(round.through.is_none());
}

#[test]
fn reading_the_same_file_twice_gives_the_same_document() {
    assert_eq!(
        read("synthetic/plate_one_hole.stp"),
        read("synthetic/plate_one_hole.stp")
    );
}

#[test]
fn every_face_is_either_in_a_feature_or_listed_as_in_none() {
    for name in ["synthetic/plate_one_hole.stp", "jt/nist_mtc_assembly.jt"] {
        let doc = read(name);
        for body in &doc.bodies {
            let claimed: BTreeSet<&String> =
                body.features.iter().flat_map(|f| f.faces.iter()).collect();
            let listed: BTreeSet<&String> = body.unassigned.iter().map(|u| &u.id).collect();
            assert!(
                claimed.is_disjoint(&listed),
                "{name}: a face is both in a feature and reported as in none"
            );
            assert_eq!(
                claimed.len() + listed.len(),
                body.faces.total,
                "{name}: {} faces, {} in features, {} unassigned",
                body.faces.total,
                claimed.len(),
                listed.len()
            );
        }
    }
}

/// The JT fixture states its own PMI, and the callouts resolved from it
/// name a through hole and a counterbore. Recognising the same sizes
/// from the geometry alone is two independent readings of one file
/// agreeing, which is the closest thing to ground truth available here.
#[test]
fn the_jt_fixture_yields_the_holes_its_own_callouts_state() {
    let doc = read("jt/nist_mtc_assembly.jt");
    let all: Vec<&features::Feature> = doc.bodies.iter().flat_map(|b| &b.features).collect();
    let with = |kind: Kind, d: f64| -> Vec<&&features::Feature> {
        all.iter()
            .filter(|f| f.kind == kind && f.shape.diameter == Some(d))
            .collect()
    };

    // The through-hole callout: Ø4.50.
    let thru = with(Kind::Hole, 4.5);
    assert!(!thru.is_empty(), "no Ø4.5 hole was recognised");
    assert!(thru.iter().all(|f| f.shape.through == Some(true)));

    // The counterbore callout: Ø11.00 opening into a Ø6.60 bore. The
    // wider stage is closed by its own floor, so it is not through, and
    // it names the hole it opens into.
    let bore = with(Kind::Counterbore, 11.0);
    assert!(!bore.is_empty(), "no Ø11 counterbore was recognised");
    let pilots: BTreeSet<&String> = with(Kind::Hole, 6.6).iter().map(|f| &f.id).collect();
    for cb in &bore {
        assert_eq!(cb.shape.through, Some(false));
        assert!(
            cb.coaxial_with.iter().any(|id| pilots.contains(id)),
            "a counterbore names no bore it opens into: {:?}",
            cb.coaxial_with
        );
    }
}

/// A drill tip is the bottom of the hole it left, not a countersink at
/// the far end from any countersink. It belongs to the hole's faces and
/// is not a feature of its own.
#[test]
fn a_blind_hole_owns_the_tip_that_drilled_it() {
    let doc = read("jt/nist_mtc_assembly.jt");
    let blind: Vec<&features::Feature> = doc
        .bodies
        .iter()
        .flat_map(|b| &b.features)
        .filter(|f| f.kind == Kind::Hole && f.shape.through == Some(false))
        .collect();
    assert!(!blind.is_empty(), "the fixture has blind holes");
    assert!(
        blind.iter().any(|f| f.faces.len() > 1),
        "no blind hole claimed the face closing it"
    );
    // A countersink of no depth would be a tip counted twice.
    for f in doc.bodies.iter().flat_map(|b| &b.features) {
        if f.kind == Kind::Countersink {
            assert!(
                f.shape.depth.unwrap_or_default() > 0.0,
                "a countersink of no depth is a drill tip: {f:?}"
            );
        }
    }
}

#[test]
fn a_file_with_no_geometry_says_so_rather_than_reporting_nothing() {
    let doc = read("synthetic/dimension_basics.stp");
    assert!(doc.bodies.is_empty());
    assert!(
        doc.diagnostics.iter().any(|d| d.message.contains("shell")),
        "{:?}",
        doc.diagnostics
    );
}

// Comparing two feature documents (ADR 0012). The plates exist for this:
// each is the baseline with one deliberate change, so each comparison
// has one right answer and it is known in advance.

mod comparing {
    use super::read;
    use pmix::features::compare::{self, Comparison, Paired, PairedOn};

    fn against(a: &str, b: &str) -> Comparison {
        compare::compare(&read(a), &read(b))
    }

    /// The first of the two questions people bring to a pair of files.
    /// The hole keeps its place and its id changes, so nothing matches;
    /// what makes the answer readable is that the one field which
    /// differs is named.
    #[test]
    fn boring_a_hole_wider_reads_as_one_changed_diameter() {
        let c = against(
            "synthetic/plate_one_hole.stp",
            "synthetic/plate_hole_larger.stp",
        );
        assert_eq!(c.bodies.len(), 1);
        let b = &c.bodies[0];

        // The plate around the bore is untouched, and those six faces are
        // what pairs the bodies at all: no id and no feature survives.
        assert_eq!(b.paired, Some(Paired::Faces));
        assert_eq!(b.paired_on, Some(6));
        assert!(b.matched.is_empty());
        assert_eq!(b.only_baseline.len(), 1);
        assert_eq!(b.only_compared.len(), 1);

        assert_eq!(b.possible_pairings.len(), 1);
        let cand = &b.possible_pairings[0];
        assert_eq!(cand.distance, 0.0, "the hole did not move");
        assert_eq!(cand.paired_on, PairedOn::Place);
        let fields: Vec<&str> = cand.differs.iter().map(|d| d.field.as_str()).collect();
        assert_eq!(fields, ["diameter"], "{:?}", cand.differs);
        assert_eq!(
            (cand.differs[0].from.as_str(), cand.differs[0].to.as_str()),
            ("8", "10")
        );
        // A single changed hole is not a body that moved.
        assert_eq!(b.placement, None);
    }

    /// The second question. The hole keeps its size, so only where it
    /// sits differs, and the distance is stated.
    #[test]
    fn moving_a_hole_reads_as_one_changed_position() {
        let c = against(
            "synthetic/plate_one_hole.stp",
            "synthetic/plate_hole_moved.stp",
        );
        let b = &c.bodies[0];
        assert_eq!(b.possible_pairings.len(), 1);
        let cand = &b.possible_pairings[0];
        assert_eq!(cand.distance, 5.0);
        assert_eq!(cand.paired_on, PairedOn::Size);
        let fields: Vec<&str> = cand.differs.iter().map(|d| d.field.as_str()).collect();
        assert_eq!(fields, ["position"], "{:?}", cand.differs);
        assert_eq!(cand.differs[0].from, "20,15,0");
        assert_eq!(cand.differs[0].to, "25,15,0");
    }

    /// A body exported from a different origin has no feature in the same
    /// place, so nothing matches and every feature differs. Reporting
    /// four differences there would be reporting one fact four times.
    #[test]
    fn a_body_that_moved_is_one_displacement_and_not_four_differences() {
        let c = against(
            "synthetic/plate_four_holes.stp",
            "synthetic/plate_four_holes_shifted.stp",
        );
        let b = &c.bodies[0];
        // Nothing survived, so the bodies pair on being made of the same
        // shapes — the only thing a displacement leaves alone.
        assert_eq!(b.paired, Some(Paired::Shapes));
        assert!(b.matched.is_empty());
        assert_eq!(b.only_baseline.len(), 4);
        assert_eq!(b.placement, Some([5.0, 0.0, 0.0]));
        assert!(!b.same_shapes_moved);
    }

    /// One hole of the four moved. Three holes keep their ids, and that
    /// is proof the body itself did not move, so this must not be read as
    /// a displacement however neatly one number fits the fourth.
    #[test]
    fn one_hole_moving_is_not_the_body_moving() {
        let c = against(
            "synthetic/plate_four_holes.stp",
            "synthetic/plate_four_holes_one_moved.stp",
        );
        let b = &c.bodies[0];
        assert_eq!(b.matched.len(), 3);
        assert_eq!(b.only_baseline.len(), 1);
        assert_eq!(b.placement, None, "three holes stayed put");
        assert!(!b.same_shapes_moved);
        assert_eq!(b.possible_pairings.len(), 1);
        assert_eq!(b.possible_pairings[0].distance, 5.0);
    }

    /// The same design in two units is the same design, and a comparison
    /// of it has to come out empty or the unit normalisation is worth
    /// nothing.
    #[test]
    fn one_design_in_two_units_compares_as_unchanged() {
        let c = against(
            "synthetic/plate_one_hole.stp",
            "synthetic/plate_one_hole_inches.stp",
        );
        assert!(c.summary.identical(), "{:?}", c.summary);
        assert_eq!(c.bodies[0].paired, Some(Paired::Id));
        assert_eq!(c.summary.matched, 1);
    }

    /// A bore written as one face and the same bore written as two is the
    /// same bore, so it pairs exactly rather than showing up as a change.
    #[test]
    fn a_split_bore_compares_as_unchanged() {
        let c = against(
            "synthetic/plate_one_hole.stp",
            "synthetic/plate_hole_split.stp",
        );
        assert!(c.summary.identical(), "{:?}", c.summary);
    }

    /// Nothing pairs two unrelated parts, and the comparison says so
    /// rather than reaching for a candidate to fill the silence.
    #[test]
    fn unrelated_parts_pair_with_nothing() {
        let c = against(
            "synthetic/plate_one_hole.stp",
            "synthetic/shaft_chamfered.stp",
        );
        assert_eq!(c.summary.bodies_paired, 0);
        assert_eq!(c.summary.bodies_only_baseline, 1);
        assert_eq!(c.summary.bodies_only_compared, 1);
        assert!(c.bodies.iter().all(|b| b.possible_pairings.is_empty()));
        assert!(!c.summary.identical());
    }

    /// The one thing this document states that `pmix` does not stand
    /// behind must say so *in the data*. A consumer rendering only the
    /// fields it recognises — a language model reading the JSON, say —
    /// would never see a caution printed beside the output (ADR 0013).
    #[test]
    fn a_possible_pairing_carries_its_own_caution() {
        let c = against(
            "synthetic/plate_one_hole.stp",
            "synthetic/plate_hole_larger.stp",
        );
        assert!(!c.bodies[0].possible_pairings.is_empty());

        // Stated at the document level, naming the field it is about.
        let note = c
            .notes
            .iter()
            .find(|n| n.field == "possible_pairings")
            .expect("the caution travels with the document");
        assert!(note.note.contains("not a conclusion"), "{}", note.note);

        // And carried by each pairing, so that dropping the note does not
        // turn an observation into a finding: what it rests on is part of
        // the record.
        let p = &c.bodies[0].possible_pairings[0];
        assert_eq!(p.paired_on, PairedOn::Place);

        // A document with nothing inferred in it says nothing.
        let same = against(
            "synthetic/plate_one_hole.stp",
            "synthetic/plate_one_hole.stp",
        );
        assert!(same.notes.is_empty(), "{:?}", same.notes);
    }

    /// The whole point of naming it: serialised, it cannot be read as a
    /// list of findings.
    #[test]
    fn the_serialised_document_does_not_call_them_findings() {
        let c = against(
            "synthetic/plate_one_hole.stp",
            "synthetic/plate_hole_moved.stp",
        );
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains("possible_pairings"), "{json}");
        assert!(!json.contains("\"candidates\""), "{json}");
        assert!(json.contains("not a conclusion"), "{json}");
        assert_eq!(c.schema_version, compare::SCHEMA_VERSION);
    }

    /// Comparing a document with itself is the control: if this is not
    /// empty, nothing else the comparison says can be trusted.
    #[test]
    fn a_document_compared_with_itself_is_unchanged() {
        for name in [
            "synthetic/plate_four_holes.stp",
            "synthetic/shaft_chamfered.stp",
            "jt/nist_mtc_assembly.jt",
            "nist/nist_ctc_05_asme1_ap242-e1.stp",
        ] {
            let c = against(name, name);
            assert!(c.summary.identical(), "{name}: {:?}", c.summary);
            assert_eq!(c.summary.only_baseline, 0, "{name}");
            assert!(
                c.bodies.iter().all(|b| b.paired == Some(Paired::Id)),
                "{name}: a body did not pair with itself by id"
            );
        }
    }
}

// --- how big a body is (ADR 0014) ---

/// The plate is 40 by 30 by 10 and the document says so exactly. A box
/// is decided by extremes, and a plate's extremes are its corners.
#[test]
fn a_plate_states_its_size_exactly() {
    let doc = read("synthetic/plate_one_hole.stp");
    let envelope = doc.bodies[0].envelope.as_ref().expect("an envelope");
    assert_eq!(envelope.size, [40.0, 30.0, 10.0]);
    assert_eq!(envelope.min, [0.0, 0.0, 0.0]);
    assert_eq!(envelope.max, [40.0, 30.0, 10.0]);
    assert!(!envelope.approximate, "every face of a plate is analytic");
}

/// Largest first, so that the three numbers do not depend on how the
/// part happened to be oriented when it was exported.
#[test]
fn the_size_is_stated_largest_first() {
    for name in [
        "synthetic/plate_one_hole.stp",
        "synthetic/shaft_chamfered.stp",
        "synthetic/plate_four_holes.stp",
    ] {
        let doc = read(name);
        for body in &doc.bodies {
            let Some(e) = &body.envelope else { continue };
            assert!(
                e.size[0] >= e.size[1] && e.size[1] >= e.size[2],
                "{name}: {:?}",
                e.size
            );
        }
    }
}

/// A shaft's widest point is on no vertex: it is the circle capping the
/// cylinder. A reader that only looked at vertices would report a
/// diameter of zero here.
#[test]
fn a_round_body_is_measured_by_its_circles() {
    let doc = read("synthetic/shaft_chamfered.stp");
    let envelope = doc.bodies[0].envelope.as_ref().expect("an envelope");
    assert_eq!(envelope.size[1], 20.0, "the shaft is 20 across");
    assert_eq!(envelope.size[2], 20.0);
    assert!(!envelope.approximate);
}

/// An arc bulges past its own endpoints, but never outside the circle it
/// lies on. A rounded corner well inside the part therefore leaves the
/// measurement exact rather than turning it into a lower bound.
#[test]
fn an_arc_inside_the_box_does_not_make_it_approximate() {
    let doc = read("synthetic/plate_corner_round.stp");
    let envelope = doc.bodies[0].envelope.as_ref().expect("an envelope");
    assert_eq!(envelope.size, [40.0, 30.0, 10.0]);
    assert!(!envelope.approximate);
}

/// Where the file states a face with no closed form, the box is the
/// smallest the body can be rather than the size it is, and the document
/// says which of those it is giving.
#[test]
fn a_body_the_reader_cannot_bound_says_so() {
    let doc = read("nist/nist_ftc_06_asme1_ap242-e2.stp");
    let envelope = doc.bodies[0].envelope.as_ref().expect("an envelope");
    assert!(
        envelope.approximate,
        "this part has faces with no closed form"
    );
    // Still useful: a part that is roughly a foot across says so.
    assert!(envelope.size[0] > 300.0 && envelope.size[0] < 310.0);
}

/// One design in two units is one document (ADR 0004), and the envelope
/// must not be the field that breaks that.
#[test]
fn the_same_design_in_inches_states_the_same_size() {
    let mm = read("synthetic/plate_one_hole.stp");
    let inches = read("synthetic/plate_one_hole_inches.stp");
    assert_eq!(
        mm.bodies[0].envelope, inches.bodies[0].envelope,
        "millimetres and inches must give one envelope"
    );
}
