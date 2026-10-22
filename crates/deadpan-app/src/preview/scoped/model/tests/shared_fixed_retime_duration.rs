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

#[test]
fn native_this_play_fixed_retime_hold_duration_offers_entry_after_public_isolation_control() {
    let before = fixed_repeated_retime_fixture();
    let second = outer_iteration(&before, 1);
    let select_second = |document| {
        let mut selected = state(document, "outer", 10);
        selected.switch_play_in(2).unwrap();
        assert!(selected.enter_in().unwrap());
        assert!(selected.enter_in().unwrap());
        selected.select_in(1).unwrap();
        selected
    };
    let before_state = select_second(before.clone());
    let target = before_state.selected.clone();
    let held = presentation(&before_state).instance.clone();
    assert_eq!(target.node, node("held"));
    assert_eq!(target.repeats, vec![branch(&before, "outer", Some(1))]);
    assert_eq!(held.repeats[0].iteration, second);
    assert_eq!(before_state.cursor, ProjectFrame(10));
    assert_eq!(presentation(&before_state).frames, 10..12);
    assert_eq!(before.duration().unwrap().frames(), 24);
    assert!(before.overrides().is_empty());
    assert!(before.gap_overrides().is_empty());

    // A real public value edit proves that this exact Play can own a copy of
    // the Stage and its complete body. Rename is only the isolation control;
    // the requested duration action must not need a separate authored Rename.
    let rename = ScopedNodeEdit::Rename {
        label: "This play isolation control".into(),
    };
    let needs = before.scoped_edit_requirements(&target, &rename).unwrap();
    assert!(!needs.unchanged);
    assert_eq!((needs.nodes, needs.marks), (5, 0));
    let isolation_request = CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: revision("this-play-fixed-retime-isolation-control"),
        command: Command::EditScoped {
            target: target.clone(),
            edit: rename,
            identities: OccurrenceIdentities {
                nodes: (0..needs.nodes)
                    .map(|index| node(&format!("this-play-fixed-retime-control-{index}")))
                    .collect(),
                marks: Vec::new(),
            },
        },
    };
    let prepared = prepare_scoped_edit(&before, &isolation_request).unwrap();
    assert_eq!(
        prepared.transaction,
        apply(&before, &isolation_request).unwrap()
    );
    assert_eq!(prepared.transaction.duration_delta, 0);
    assert_eq!(prepared.isolation().steps().len(), 1);
    let mapped_held = prepared.map_instance(&before, &held).unwrap();
    assert_eq!(mapped_held.node, prepared.target.node);
    assert_ne!(mapped_held.node, held.node);
    assert_eq!(mapped_held.repeats, held.repeats);
    let mut speech = held.clone();
    speech.node = node("speech");
    let mapped_speech = prepared.map_instance(&before, &speech).unwrap();
    let map_node = |name: &str| {
        let mut other = target.clone();
        other.node = node(name);
        prepared.map_target(&before, &other).unwrap().node
    };
    let mapped_stage = map_node("fixed-stage");
    let mapped_body = map_node("fixed-body");
    let isolated = &prepared.document;
    assert_eq!(isolated.nodes().len(), before.nodes().len() + 5);
    assert_eq!(isolated.duration().unwrap().frames(), 24);
    let entries = &isolated.overrides()[&node("outer")];
    assert_eq!(entries.len(), 1);
    assert_eq!(entries.get(&second), Some(&mapped_stage));
    let isolated_state = select_second(isolated.clone());
    assert_eq!(isolated_state.selected, prepared.target);
    assert_eq!(presentation(&isolated_state).instance, mapped_held);
    assert_eq!(presentation(&isolated_state).frames, 10..12);

    let resize_request = CommandRequest {
        project_id: isolated.project_id().clone(),
        expected_revision: isolated.revision_id().clone(),
        new_revision: revision("this-play-fixed-retime-duration-control"),
        command: Command::SetHoldDuration {
            node: mapped_held.node.clone(),
            duration: frames(4),
        },
    };
    let resized = apply(isolated, &resize_request).unwrap();
    assert_eq!(resized.duration_delta, 0);
    let after = resized.forward.apply(isolated).unwrap();
    assert_eq!(after.duration().unwrap().frames(), 24);
    assert_eq!(after.nodes().len(), isolated.nodes().len());
    assert_eq!(after.overrides(), isolated.overrides());
    assert_eq!(after.gap_overrides(), before.gap_overrides());
    assert!(matches!(
        &after.nodes()[&mapped_held.node].kind,
        NodeKind::Hold { recipe } if recipe.duration == frames(4)
    ));
    // Every original definition remains intact. Only the chosen Play uses the
    // cloned Hold; its sibling Plays continue using the two-frame default.
    for (id, beat) in before.nodes() {
        assert_eq!(&after.nodes()[id], beat);
    }
    let mut expected_stage = before.nodes()[&node("fixed-stage")].clone();
    let NodeKind::Retime { child, .. } = &mut expected_stage.kind else {
        panic!("expected fixed Edit Retime");
    };
    *child = mapped_body;
    assert_eq!(after.nodes()[&mapped_stage], expected_stage);
    assert_eq!(
        after.nodes()[&mapped_stage],
        isolated.nodes()[&mapped_stage]
    );
    assert_eq!(
        after.nodes()[&mapped_speech.node],
        before.nodes()[&node("speech")]
    );
    for (document, owner, position, expected) in [
        (&before, &held.node, 0, ExactRatio::new(21, 2).unwrap()),
        (&before, &held.node, 2, ExactRatio::integer(12)),
        (
            &after,
            &mapped_held.node,
            0,
            ExactRatio::new(21, 2).unwrap(),
        ),
        (
            &after,
            &mapped_held.node,
            4,
            ExactRatio::new(27, 2).unwrap(),
        ),
        (&before, &speech.node, 0, ExactRatio::integer(12)),
        (
            &after,
            &mapped_speech.node,
            0,
            ExactRatio::new(27, 2).unwrap(),
        ),
        (&after, &mapped_stage, 0, ExactRatio::integer(9)),
    ] {
        assert_eq!(
            exact_occurrence_boundary(document, owner.as_str(), second.clone(), position),
            expected
        );
    }
    let after_state = select_second(after.clone());
    assert_eq!(after_state.selected, prepared.target);
    assert_eq!(presentation(&after_state).instance, mapped_held);
    assert_eq!(presentation(&after_state).frames, 10..13);
    assert_eq!(after_state.cursor, ProjectFrame(10));

    // These are silent structural recipes. Fresh public captures verify their
    // exact placement lattices, not retention of an audible pre-edit phase.
    // Silence intentionally acquires no new retained clock during the resize.
    let lattice = |document: &ProjectDocument, instance: &InstancePath| {
        deadpan_core::capture_unbound_audio_bindings(
            document,
            deadpan_core::AudioTimingId {
                allocation: revision("this-play-fixed-retime-lattice-control"),
                ordinal: 0,
            },
        )
        .unwrap()
        .resolve(&instance.node, instance, deadpan_core::MAX_DOCUMENT_NODES)
        .unwrap()
        .lattice
    };
    let old_speech = lattice(&before, &speech);
    let isolated_speech = lattice(isolated, &mapped_speech);
    let new_speech = lattice(&after, &mapped_speech);
    assert_eq!(old_speech.origin, ExactRatio::integer(12));
    assert_eq!(isolated_speech.origin, old_speech.origin);
    assert_eq!(new_speech.origin, ExactRatio::new(27, 2).unwrap());
    for placement in [&old_speech, &isolated_speech, &new_speech] {
        assert_eq!(
            placement.frames_per_local_frame,
            ExactRatio::new(3, 4).unwrap()
        );
    }
    for ordinal in [0, 2] {
        let mut untouched = speech.clone();
        untouched.repeats[0].iteration = outer_iteration(&before, ordinal);
        let old = lattice(&before, &untouched);
        let current = lattice(&after, &untouched);
        assert_eq!(current.origin, old.origin);
        assert_eq!(current.frames_per_local_frame, old.frames_per_local_frame);
        assert_eq!(current.grid_origin, old.grid_origin);
        assert_eq!(current.frames_per_sample, old.frames_per_sample);
        assert_eq!(current.local_support, old.local_support);
        assert_eq!(
            exact_occurrence_boundary(&after, "held", untouched.repeats[0].iteration.clone(), 2),
            ExactRatio::integer(6 + 6 * i64::from(ordinal))
        );
    }
    assert_eq!(after.audio_bindings(), isolated.audio_bindings());
    assert_eq!(resized.inverse.apply(&after).unwrap(), *isolated);
    assert_eq!(
        prepared.transaction.inverse.apply(isolated).unwrap(),
        before
    );
    for document in [&before, isolated, &after] {
        assert_eq!(
            ProjectDocument::from_json(&document.to_json().unwrap()).unwrap(),
            *document
        );
    }
    assert_eq!(*before_state.index.document, before);
    assert_eq!(before_state.selected, target);
    assert_eq!(presentation(&before_state).instance, held);

    // Sole intended old-code failure, after both public controls and inverses.
    assert!(
        before_state.hold_duration_available(),
        "A visible This play Hold inside this fixed Edit window must offer Change duration"
    );
    assert!(after_state.hold_duration_available());
    assert_eq!(
        after_state.hold_parameter_in().as_deref(),
        Some("hold-duration 4f")
    );
}

#[test]
fn native_this_play_fixed_retime_duration_hint_uses_retained_scope_and_clock_guards() {
    let before = fixed_repeated_retime_fixture();
    let mut selected = state(before.clone(), "outer", 10);
    selected.switch_play_in(2).unwrap();
    selected.enter_in().unwrap();
    selected.enter_in().unwrap();
    selected.select_in(1).unwrap();
    assert!(selected.body_fixed_retime_path_supported());
    assert!(selected.hold_duration_available());

    // Cached clock ownership and actual visibility each revoke the hint.
    Arc::make_mut(&mut selected.index)
        .clocked_retimes
        .insert(node("fixed-stage"));
    assert!(!selected.hold_duration_available());
    Arc::make_mut(&mut selected.index).clocked_retimes.clear();
    let visible = selected.projection.presentation.take();
    assert!(!selected.hold_duration_available());
    selected.projection.presentation = visible;
    assert!(selected.hold_duration_available());

    // A further selected play goes through the batch policy, retaining its
    // Retime refusal rather than inheriting this single-play exception.
    selected
        .levels
        .last_mut()
        .unwrap()
        .also
        .push(outer_iteration(&before, 0));
    assert!(!selected.hold_duration_available());
    selected.levels.last_mut().unwrap().also.clear();
    assert!(selected.hold_duration_available());
    selected.root = node("fixed-stage");
    assert!(!selected.body_fixed_retime_path_supported());
    assert!(!selected.hold_duration_available());
}
