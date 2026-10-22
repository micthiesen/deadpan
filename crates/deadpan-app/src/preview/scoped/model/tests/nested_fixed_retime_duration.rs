//! Public isolation and ordinary duration controls for nested fixed windows.
//! This silent structural fixture makes no audible-phase or media claim.

use super::*;
use deadpan_core::{
    Anchor, AnchorIndex, AnchorTarget, BoundaryAnchor, ExactRatio, InsertionBias, InstancePath,
    RepeatInstance,
};

#[path = "mixed_fixed_retime_duration.rs"]
mod mixed_fixed_retime_duration;

fn nested_fixed_retime_fixture() -> ProjectDocument {
    let mut stage = retime("fixed-stage", "fixed-body", 0, 8, 6);
    if let NodeKind::Retime { pitch, .. } = &mut stage.1.kind {
        *pitch = PitchPolicy::FollowSpeed;
    }
    let document = fixture(
        "outer",
        vec![
            repeat("outer", "inner", 3, None),
            repeat("inner", "fixed-stage", 2, None),
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
            &format!("nested-fixed-retime-{name}"),
        )
    };
    let document = insert(&document, "lead", 0);
    let document = insert(&document, "tail", 2);
    document.validate().unwrap();
    document
}

fn iteration(
    document: &ProjectDocument,
    repeat_name: &str,
    zero_based: u32,
) -> deadpan_core::IterationId {
    let NodeKind::Repeat { iterations, .. } = &document.nodes()[&node(repeat_name)].kind else {
        panic!("expected Repeat {repeat_name}");
    };
    iterations.at(zero_based).expect("fixture Repeat iteration")
}

fn nested_instance(
    document: &ProjectDocument,
    leaf: &str,
    outer_play: u32,
    inner_play: u32,
) -> InstancePath {
    InstancePath {
        node: node(leaf),
        repeats: vec![
            RepeatInstance {
                node: node("outer"),
                iteration: iteration(document, "outer", outer_play),
            },
            RepeatInstance {
                node: node("inner"),
                iteration: iteration(document, "inner", inner_play),
            },
        ],
    }
}

fn exact_nested_boundary(
    document: &ProjectDocument,
    instance: &InstancePath,
    position: i64,
) -> ExactRatio {
    AnchorIndex::new(document)
        .unwrap()
        .resolve_target(&AnchorTarget {
            boundary: BoundaryAnchor {
                coordinate: Anchor::Occurrence {
                    instance: instance.clone(),
                    position: ExactRatio::integer(position),
                },
                bias: InsertionBias::Right,
            },
            occurrence: None,
        })
        .unwrap()
        .exact_frame
}

fn select_second_second(document: ProjectDocument) -> State {
    let mut selected = state(document, "outer", 22);
    selected.switch_play_in(2).unwrap();
    assert!(selected.enter_in().unwrap()); // selected Outer Play -> Inner
    selected.switch_play_in(2).unwrap();
    assert!(selected.enter_in().unwrap()); // selected Inner Play -> fixed Stage
    assert!(selected.enter_in().unwrap()); // fixed Stage -> ordinary Body Sequence
    selected.select_in(1).unwrap(); // Held
    selected
}

#[test]
fn native_this_play_nested_fixed_retime_hold_duration_offers_entry_after_public_control() {
    let before = nested_fixed_retime_fixture();
    before.validate().unwrap();
    assert_eq!(before.duration().unwrap().frames(), 42);
    assert!(before.overrides().is_empty());
    assert!(before.gap_overrides().is_empty());
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

    // Native navigation selects Outer second, then Inner second, then Held.
    // Its source Play identities remain distinct from the entry occurrence.
    let before_state = select_second_second(before.clone());
    let target = before_state.selected.clone();
    let held = presentation(&before_state).instance.clone();
    assert_eq!(target.node, node("held"));
    assert_eq!(target.repeats.len(), 2);
    assert_eq!(target.repeats[0], branch(&before, "outer", Some(1)));
    assert_eq!(target.repeats[1], branch(&before, "inner", Some(1)));
    assert_eq!(held.repeats[0].iteration, iteration(&before, "outer", 1));
    assert_eq!(held.repeats[1].iteration, iteration(&before, "inner", 1));
    assert_eq!(before_state.cursor, ProjectFrame(22));
    assert_eq!(presentation(&before_state).frames, 22..24);

    let original_stage_start = ExactRatio::integer(21);
    let old_start = exact_nested_boundary(&before, &held, 0);
    let old_end = exact_nested_boundary(&before, &held, 2);
    assert_eq!(old_start, ExactRatio::new(45, 2).unwrap());
    assert_eq!(old_end, ExactRatio::integer(24));
    assert_eq!(
        exact_nested_boundary(&before, &nested_instance(&before, "fixed-stage", 1, 1), 0),
        original_stage_start
    );

    // The public Rename is only a real isolation control. This ordinary raw
    // SetHoldDuration on the mapped Hold must pass on old code.
    let rename = ScopedNodeEdit::Rename {
        label: "Nested This play isolation control".into(),
    };
    let needs = before.scoped_edit_requirements(&target, &rename).unwrap();
    assert!(!needs.unchanged);
    assert_eq!((needs.nodes, needs.marks), (11, 0));
    let isolation_request = CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: revision("nested-this-play-isolation-control"),
        command: Command::EditScoped {
            target: target.clone(),
            edit: rename,
            identities: OccurrenceIdentities {
                nodes: (0..needs.nodes)
                    .map(|index| node(&format!("nested-this-play-control-{index}")))
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
    assert_eq!(prepared.isolation().steps().len(), 2);

    // Every queried address is obtained from the actual proof mapping.
    let mapped_held = prepared.map_instance(&before, &held).unwrap();
    assert_eq!(mapped_held.node, prepared.target.node);
    assert_ne!(mapped_held.node, held.node);
    assert_eq!(mapped_held.repeats.len(), 2);
    assert_eq!(mapped_held.repeats[0].node, node("outer"));
    assert_ne!(mapped_held.repeats[1].node, node("inner"));
    assert_eq!(mapped_held.repeats[0].iteration, held.repeats[0].iteration);
    assert_eq!(mapped_held.repeats[1].iteration, held.repeats[1].iteration);
    let map_instance = |leaf: &str, outer_play: u32, inner_play: u32| {
        let original = nested_instance(&before, leaf, outer_play, inner_play);
        prepared.map_instance(&before, &original).unwrap()
    };
    let mapped_speech = map_instance("speech", 1, 1);
    let mapped_stage_instance = map_instance("fixed-stage", 1, 1);
    let map_target = |leaf: &str| {
        let mut other = target.clone();
        other.node = node(leaf);
        prepared.map_target(&before, &other).unwrap().node
    };
    let mapped_stage = map_target("fixed-stage");
    let mapped_body = map_target("fixed-body");
    let isolated = &prepared.document;
    assert_eq!(isolated.nodes().len(), before.nodes().len() + 11);
    assert_eq!(isolated.duration().unwrap().frames(), 42);
    assert_eq!(prepared.target.repeats.len(), 2);
    assert_eq!(
        isolated.overrides()[&node("outer")].get(&iteration(&before, "outer", 1)),
        Some(&mapped_held.repeats[1].node)
    );
    assert_eq!(
        isolated.overrides()[&mapped_held.repeats[1].node].get(&iteration(&before, "inner", 1)),
        Some(&mapped_stage)
    );
    assert_eq!(
        exact_nested_boundary(isolated, &mapped_stage_instance, 0),
        original_stage_start
    );
    let isolated_state = select_second_second(isolated.clone());
    assert_eq!(isolated_state.selected, prepared.target);
    assert_eq!(presentation(&isolated_state).instance, mapped_held);
    assert_eq!(presentation(&isolated_state).frames, 22..24);

    let resize_request = CommandRequest {
        project_id: isolated.project_id().clone(),
        expected_revision: isolated.revision_id().clone(),
        new_revision: revision("nested-this-play-duration-control"),
        command: Command::SetHoldDuration {
            node: mapped_held.node.clone(),
            duration: frames(4),
        },
    };
    let resized = apply(isolated, &resize_request).unwrap();
    assert_eq!(resized.duration_delta, 0);
    let after = resized.forward.apply(isolated).unwrap();
    assert_eq!(after.duration().unwrap().frames(), 42);
    assert_eq!(after.nodes().len(), isolated.nodes().len());
    assert_eq!(after.overrides(), isolated.overrides());
    assert_eq!(after.gap_overrides(), isolated.gap_overrides());
    assert!(matches!(
        &after.nodes()[&mapped_held.node].kind,
        NodeKind::Hold { recipe } if recipe.duration == frames(4)
    ));

    // Every original definition stays intact. The mapped fixed Stage retains
    // its exact six-frame output and [0,8) source window.
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
        exact_nested_boundary(&after, &mapped_stage_instance, 0),
        original_stage_start
    );
    assert_eq!(exact_nested_boundary(&before, &held, 0), old_start);
    assert_eq!(exact_nested_boundary(&before, &held, 2), old_end);
    assert_eq!(
        exact_nested_boundary(&after, &mapped_held, 0),
        ExactRatio::new(45, 2).unwrap()
    );
    assert_eq!(
        exact_nested_boundary(&after, &mapped_held, 4),
        ExactRatio::new(51, 2).unwrap()
    );
    assert_eq!(
        exact_nested_boundary(&before, &nested_instance(&before, "speech", 1, 1), 0),
        ExactRatio::integer(24)
    );
    assert_eq!(
        exact_nested_boundary(&after, &mapped_speech, 0),
        ExactRatio::new(51, 2).unwrap()
    );

    // All five other stable outer/inner combinations retain their original
    // Hold duration and exact start/end despite the chosen Play's growth.
    for outer_play in 0..3 {
        for inner_play in 0..2 {
            if (outer_play, inner_play) == (1, 1) {
                continue;
            }
            let original = nested_instance(&before, "held", outer_play, inner_play);
            let mapped = prepared.map_instance(&before, &original).unwrap();
            assert!(matches!(
                &after.nodes()[&mapped.node].kind,
                NodeKind::Hold { recipe } if recipe.duration == frames(2)
            ));
            assert_eq!(
                exact_nested_boundary(&after, &mapped, 0),
                exact_nested_boundary(&before, &original, 0)
            );
            assert_eq!(
                exact_nested_boundary(&after, &mapped, 2),
                exact_nested_boundary(&before, &original, 2)
            );
        }
    }
    let after_state = select_second_second(after.clone());
    assert_eq!(after_state.selected, prepared.target);
    assert_eq!(presentation(&after_state).instance, mapped_held);
    assert_eq!(presentation(&after_state).frames, 22..25);
    assert_eq!(after_state.cursor, ProjectFrame(22));

    // Structural inverse and JSON round trips precede the intended failure.
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

    // Sole intended old-code failure. Native availability must derive from
    // the retained visible second/second occurrence and fixed scoped Retime.
    assert!(
        before_state.hold_duration_available(),
        "A visible This play Hold inside this fixed Edit window must offer Change duration"
    );
}
