//! Old-code regression witness for All-Default Hold duration under
//! a fixed unclocked Edit Retime inside an outer Repeat.
//!
//! Install as preview/scoped/model/tests/shared_fixed_retime_duration.rs and
//! register `mod shared_fixed_retime_duration;` in tests.rs. Uses silent
//! structural Holds only; this proves native duration admission and exact
//! geometry, not media or audio qualification.

use super::*;
use deadpan_core::{
    Anchor, AnchorIndex, AnchorTarget, BoundaryAnchor, ExactRatio, InsertionBias, InstancePath,
    RepeatInstance,
};

fn fixed_repeated_retime_fixture() -> ProjectDocument {
    let mut stage = retime("fixed-stage", "fixed-body", 0, 8, 6);
    if let NodeKind::Retime { pitch, .. } = &mut stage.1.kind {
        *pitch = PitchPolicy::FollowSpeed;
    }
    let document = fixture(
        "outer",
        vec![
            repeat("outer", "fixed-stage", 3, None),
            stage,
            sequence("fixed-body", &["before", "held", "speech"]),
            hold("before", 2),
            hold("held", 2),
            hold("speech", 6),
        ],
        BTreeMap::new(),
        BTreeMap::new(),
    );
    let insert = |document: &ProjectDocument, name: &str, index: usize| {
        edited(
            document,
            Command::Insert {
                parent: document.root().clone(),
                index,
                subtree: Subtree {
                    root: node(name),
                    nodes: [hold(name, 3)].into_iter().collect(),
                    overrides: BTreeMap::new(),
                    gap_overrides: BTreeMap::new(),
                },
            },
            &format!("fixed-repeat-retime-{name}"),
        )
    };
    let document = insert(&document, "lead", 0);
    let document = insert(&document, "tail", 2);
    document.validate().unwrap();
    document
}

fn outer_iteration(document: &ProjectDocument, zero_based: u32) -> deadpan_core::IterationId {
    let NodeKind::Repeat { iterations, .. } = &document.nodes()[&node("outer")].kind else {
        panic!("expected outer Repeat");
    };
    iterations.at(zero_based).expect("fixture Repeat iteration")
}

fn exact_occurrence_boundary(
    document: &ProjectDocument,
    owner: &str,
    iteration: deadpan_core::IterationId,
    position: i64,
) -> ExactRatio {
    AnchorIndex::new(document)
        .unwrap()
        .resolve_target(&AnchorTarget {
            boundary: BoundaryAnchor {
                coordinate: Anchor::Occurrence {
                    instance: InstancePath {
                        node: node(owner),
                        repeats: vec![RepeatInstance {
                            node: node("outer"),
                            iteration,
                        }],
                    },
                    position: ExactRatio::integer(position),
                },
                bias: InsertionBias::Right,
            },
            occurrence: None,
        })
        .unwrap()
        .exact_frame
}

fn selected_second_play(document: ProjectDocument) -> State {
    // Outer is a direct child of the root Sequence. Its child is the fixed
    // Retime; the Retime child is the ordinary Before/Held/Speech Sequence.
    let mut state = state(document, "outer", 10);
    assert!(state.enter_in().unwrap());
    assert!(state.enter_in().unwrap());
    state.select_in(1).unwrap();
    assert_eq!(state.selected.node, node("held"));
    assert_eq!(
        state.selected.repeats,
        vec![branch(&state.index.document, "outer", None)]
    );
    assert_eq!(
        presentation(&state).instance.repeats[0].iteration,
        outer_iteration(&state.index.document, 1)
    );
    state
}

#[test]
fn native_all_default_fixed_retime_hold_duration_offers_entry_after_public_control() {
    let before = fixed_repeated_retime_fixture();
    before.validate().unwrap();
    assert_eq!(before.duration().unwrap().frames(), 24);
    let NodeKind::Retime {
        duration,
        mapping,
        pitch,
        purpose,
        ..
    } = &before.nodes()[&node("fixed-stage")].kind
    else {
        panic!("expected fixed Edit Retime");
    };
    assert_eq!(*duration, frames(6));
    assert_eq!(
        *mapping,
        FrameRange::new(ProjectFrame(0), ProjectFrame(8)).unwrap()
    );
    assert_eq!(*pitch, PitchPolicy::FollowSpeed);
    assert_eq!(*purpose, RetimePurpose::Edit);
    assert!(before.overrides().is_empty());
    assert!(before.gap_overrides().is_empty());

    // Confirm the second stable Repeat play is the representative and the
    // selected Hold's sampled envelope begins at output frame 10.
    let before_state = selected_second_play(before.clone());
    assert_eq!(before_state.cursor, ProjectFrame(10));
    assert_eq!(presentation(&before_state).frames, 10..12);

    let request = CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: revision("fixed-retime-held-growth-control"),
        command: Command::SetHoldDuration {
            node: node("held"),
            duration: frames(4),
        },
    };
    let transaction = apply(&before, &request).unwrap();
    assert_eq!(transaction.duration_delta, 0);
    let after = transaction.forward.apply(&before).unwrap();

    // The raw public reducer accepts 2->4 and changes only the Hold duration.
    // Retime output/mapping, Repeat, exterior beats, identities and branch
    // tables stay fixed, so no time escapes the fixed stage or Repeat.
    assert_eq!(after.duration().unwrap().frames(), 24);
    assert_eq!(after.nodes().len(), before.nodes().len());
    assert_eq!(
        after.nodes()[&node("fixed-stage")],
        before.nodes()[&node("fixed-stage")]
    );
    assert_eq!(
        after.nodes()[&node("outer")],
        before.nodes()[&node("outer")]
    );
    assert_eq!(after.nodes()[&node("lead")], before.nodes()[&node("lead")]);
    assert_eq!(after.nodes()[&node("tail")], before.nodes()[&node("tail")]);
    assert_eq!(after.overrides(), before.overrides());
    assert_eq!(after.gap_overrides(), before.gap_overrides());
    assert!(matches!(
        &after.nodes()[&node("held")].kind,
        NodeKind::Hold { recipe } if recipe.duration == frames(4)
    ));

    let second = outer_iteration(&before, 1);
    assert_eq!(
        exact_occurrence_boundary(&before, "fixed-stage", second.clone(), 0),
        ExactRatio::integer(9)
    );
    assert_eq!(
        exact_occurrence_boundary(&after, "fixed-stage", second.clone(), 0),
        ExactRatio::integer(9)
    );
    assert_eq!(
        exact_occurrence_boundary(&before, "held", second.clone(), 0),
        ExactRatio::new(21, 2).unwrap()
    );
    assert_eq!(
        exact_occurrence_boundary(&before, "held", second.clone(), 2),
        ExactRatio::integer(12)
    );
    assert_eq!(
        exact_occurrence_boundary(&after, "held", second.clone(), 4),
        ExactRatio::new(27, 2).unwrap()
    );
    assert_eq!(
        exact_occurrence_boundary(&before, "speech", second.clone(), 0),
        ExactRatio::integer(12)
    );
    assert_eq!(
        exact_occurrence_boundary(&after, "speech", second, 0),
        ExactRatio::new(27, 2).unwrap()
    );

    // The projected sampled envelope grows from [10,12) to [10,13), while the
    // retained cursor stays at 10. Exact start/end are checked above.
    let after_state = selected_second_play(after.clone());
    assert_eq!(after_state.cursor, ProjectFrame(10));
    assert_eq!(presentation(&after_state).frames, 10..13);

    // Check exact Undo before the sole public availability expectation. This
    // isolates the old scoped guard: duration mutation is valid, but the
    // All-Default scoped inspector still refuses Change duration.
    assert_eq!(transaction.inverse.apply(&after).unwrap(), before);
    assert_eq!(
        ProjectDocument::from_json(&after.to_json().unwrap()).unwrap(),
        after
    );
    assert_eq!(*before_state.index.document, before);
    assert_eq!(before_state.selected.node, node("held"));

    // Sole intended OLD failure. The immutable original capture remains
    // All plays with the second stable play as a separate presentation.
    assert!(
        before_state.hold_duration_available(),
        "A visible shared Hold inside this fixed Edit window must offer Change duration"
    );
    assert_eq!(
        before_state.hold_parameter_in().as_deref(),
        Some("hold-duration 2f")
    );
}
