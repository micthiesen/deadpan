//! Both literal mixed choices retain one fixed window through public controls.
//! Silent structural evidence only; this does not qualify audible phase.

use super::*;

fn select(document: ProjectDocument, outer_play: Option<u32>, inner_play: Option<u32>) -> State {
    let mut selected = state(document, "outer", 22);
    if let Some(play) = outer_play {
        selected.switch_play_in(play + 1).unwrap();
    }
    assert!(selected.enter_in().unwrap());
    if let Some(play) = inner_play {
        selected.switch_play_in(play + 1).unwrap();
    }
    assert!(selected.enter_in().unwrap()); // fixed Stage
    assert!(selected.enter_in().unwrap()); // Body Sequence
    selected.select_in(1).unwrap(); // Held
    selected
}

fn sampled_identity(
    plan: &deadpan_plan::RenderPlan,
    instance: &InstancePath,
    start: ExactRatio,
    end: ExactRatio,
) {
    let half = ExactRatio::new(1, 2).unwrap();
    let first = start.checked_sub(half).unwrap().ceil().unwrap();
    let terminal = end.checked_sub(half).unwrap().ceil().unwrap();
    assert!(first < terminal);
    for frame in [first, terminal.checked_sub(1).unwrap()] {
        assert!(
            plan.picture(ProjectFrame(i64::try_from(frame).unwrap()))
                .unwrap()
                .framing
                .iter()
                .any(|layer| &layer.instance == instance),
            "Hold identity at frame {frame}"
        );
    }
}

fn check_direction(
    before: &ProjectDocument,
    outer_play: Option<u32>,
    inner_play: Option<u32>,
    expected_nodes: usize,
    revision_suffix: &str,
) -> bool {
    let before_state = select(before.clone(), outer_play, inner_play);
    let target = before_state.selected.clone();
    let held = presentation(&before_state).instance.clone();
    assert_eq!(before_state.cursor, ProjectFrame(22));
    assert_eq!(presentation(&before_state).frames, 22..24);
    assert_eq!(target.node, node("held"));
    assert_eq!(
        target.repeats,
        vec![
            branch(before, "outer", outer_play),
            branch(before, "inner", inner_play),
        ]
    );
    // Default retains the occurrence selected by the cursor at 22; both
    // authored choices therefore resolve to the actual second occurrence.
    assert_eq!(held.repeats[0].iteration, iteration(before, "outer", 1));
    assert_eq!(held.repeats[1].iteration, iteration(before, "inner", 1));

    let rename = ScopedNodeEdit::Rename {
        label: format!("Mixed {revision_suffix} isolation control"),
    };
    let needs = before.scoped_edit_requirements(&target, &rename).unwrap();
    assert!(!needs.unchanged);
    assert_eq!((needs.nodes, needs.marks), (expected_nodes, 0));
    let request = CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: revision(&format!("mixed-fixed-{revision_suffix}-isolation")),
        command: Command::EditScoped {
            target: target.clone(),
            edit: rename,
            identities: OccurrenceIdentities {
                nodes: (0..needs.nodes)
                    .map(|i| node(&format!("mixed-{revision_suffix}-{i}")))
                    .collect(),
                marks: Vec::new(),
            },
        },
    };
    let prepared = prepare_scoped_edit(before, &request).unwrap();
    assert_eq!(prepared.transaction, apply(before, &request).unwrap());
    assert_eq!(prepared.transaction.duration_delta, 0);
    assert_eq!(prepared.isolation().steps().len(), 1);
    let mapped_held = prepared.map_instance(before, &held).unwrap();
    let map_instance = |leaf: &str| {
        let mut source = held.clone();
        source.node = node(leaf);
        prepared.map_instance(before, &source).unwrap()
    };
    let mapped_speech = map_instance("speech");
    let map_target = |leaf: &str| {
        let mut source = target.clone();
        source.node = node(leaf);
        prepared.map_target(before, &source).unwrap().node
    };
    let mapped_stage = map_target("fixed-stage");
    let mapped_body = map_target("fixed-body");
    let isolated = &prepared.document;
    assert_eq!(
        isolated.nodes().len(),
        before.nodes().len() + expected_nodes
    );
    assert_eq!(isolated.duration().unwrap().frames(), 42);

    let resize = CommandRequest {
        project_id: isolated.project_id().clone(),
        expected_revision: isolated.revision_id().clone(),
        new_revision: revision(&format!("mixed-fixed-{revision_suffix}-resize")),
        command: Command::SetHoldDuration {
            node: mapped_held.node.clone(),
            duration: frames(4),
        },
    };
    let transaction = apply(isolated, &resize).unwrap();
    assert_eq!(transaction.duration_delta, 0);
    let after = transaction.forward.apply(isolated).unwrap();
    assert_eq!(after.duration().unwrap().frames(), 42);
    assert_eq!(after.overrides(), isolated.overrides());
    assert_eq!(after.gap_overrides(), isolated.gap_overrides());
    assert!(
        matches!(&after.nodes()[&mapped_held.node].kind, NodeKind::Hold { recipe } if recipe.duration == frames(4))
    );
    let NodeKind::Retime {
        duration,
        mapping,
        pitch,
        purpose,
        ..
    } = &after.nodes()[&mapped_stage].kind
    else {
        panic!("expected mapped fixed Edit Retime");
    };
    assert_eq!(*duration, frames(6));
    assert_eq!(
        *mapping,
        FrameRange::new(ProjectFrame(0), ProjectFrame(8)).unwrap()
    );
    assert_eq!(*pitch, PitchPolicy::FollowSpeed);
    assert_eq!(*purpose, RetimePurpose::Edit);

    let resized_state = select(after.clone(), outer_play, inner_play);
    assert_eq!(resized_state.selected, prepared.target);
    assert_eq!(presentation(&resized_state).instance, mapped_held);
    assert_eq!(resized_state.cursor, ProjectFrame(22));
    assert_eq!(presentation(&resized_state).frames, 22..25);
    assert_eq!(
        exact_nested_boundary(&after, &map_instance("fixed-stage"), 0),
        ExactRatio::integer(21)
    );
    assert_eq!(
        exact_nested_boundary(&after, &mapped_held, 0),
        ExactRatio::new(45, 2).unwrap()
    );
    assert_eq!(
        exact_nested_boundary(&after, &mapped_held, 4),
        ExactRatio::new(51, 2).unwrap()
    );
    assert_eq!(
        exact_nested_boundary(&after, &mapped_speech, 0),
        ExactRatio::new(51, 2).unwrap()
    );

    // Enumerate all six authored Repeat combinations by mapping actual
    // proof inputs. Unselected occurrences retain exact two-frame Holds.
    let before_plan = deadpan_plan::RenderPlan::compile(before).unwrap();
    let after_plan = deadpan_plan::RenderPlan::compile(&after).unwrap();
    let mut changed_uses = 0;
    for outer in 0..3 {
        for inner in 0..2 {
            let chosen = match (outer_play, inner_play) {
                (None, Some(play)) => inner == play,
                (Some(play), None) => outer == play,
                _ => unreachable!("the witness covers only mixed directions"),
            };
            let original = nested_instance(before, "held", outer, inner);
            let mapped = prepared.map_instance(before, &original).unwrap();
            sampled_identity(
                &before_plan,
                &original,
                exact_nested_boundary(before, &original, 0),
                exact_nested_boundary(before, &original, 2),
            );
            sampled_identity(
                &after_plan,
                &mapped,
                exact_nested_boundary(&after, &mapped, 0),
                exact_nested_boundary(&after, &mapped, if chosen { 4 } else { 2 }),
            );
            if chosen {
                changed_uses += 1;
                assert_eq!(mapped.node, mapped_held.node);
                assert!(
                    matches!(&after.nodes()[&mapped.node].kind, NodeKind::Hold { recipe } if recipe.duration == frames(4))
                );
                let old_start = exact_nested_boundary(before, &original, 0);
                assert_eq!(exact_nested_boundary(&after, &mapped, 0), old_start);
                assert_eq!(
                    exact_nested_boundary(&after, &mapped, 4),
                    old_start.checked_add(ExactRatio::integer(3)).unwrap(),
                );
            } else {
                assert!(
                    matches!(&after.nodes()[&mapped.node].kind, NodeKind::Hold { recipe } if recipe.duration == frames(2))
                );
                assert_eq!(
                    exact_nested_boundary(&after, &mapped, 0),
                    exact_nested_boundary(before, &original, 0)
                );
                assert_eq!(
                    exact_nested_boundary(&after, &mapped, 2),
                    exact_nested_boundary(before, &original, 2)
                );
            }
        }
    }
    assert_eq!(changed_uses, if outer_play.is_none() { 3 } else { 2 });
    // The selected subtree is the only changed authored definition; its
    // mapped Stage still has the original fixed six-frame output.
    for (id, beat) in before.nodes() {
        assert_eq!(&after.nodes()[id], beat);
    }
    let mut expected_stage = before.nodes()[&node("fixed-stage")].clone();
    let NodeKind::Retime { child, .. } = &mut expected_stage.kind else {
        panic!("expected fixed Stage")
    };
    *child = mapped_body;
    assert_eq!(after.nodes()[&mapped_stage], expected_stage);
    assert_eq!(transaction.inverse.apply(&after).unwrap(), *isolated);
    assert_eq!(
        prepared.transaction.inverse.apply(isolated).unwrap(),
        *before
    );
    for document in [before, isolated, &after] {
        assert_eq!(
            ProjectDocument::from_json(&document.to_json().unwrap()).unwrap(),
            *document
        );
    }
    assert_eq!(*before_state.index.document, *before);
    assert_eq!(before_state.selected, target);
    assert_eq!(presentation(&before_state).instance, held);

    // Deliberately only observe availability here. The caller retains both
    // booleans and performs one last positive assertion after both control
    // paths have completed.
    before_state.hold_duration_available()
}

#[test]
fn native_mixed_fixed_retime_directions_retain_duration_entry_after_controls() {
    let before = nested_fixed_retime_fixture();
    before.validate().unwrap();
    assert_eq!(before.duration().unwrap().frames(), 42);
    assert!(before.overrides().is_empty());
    assert!(before.gap_overrides().is_empty());
    let first = check_direction(&before, None, Some(1), 5, "outer-default-inner-play-second");
    let second = check_direction(&before, Some(1), None, 6, "outer-play-second-inner-default");
    assert!(
        first && second,
        "Both mixed native choices (Outer Default/Inner second Play and Outer second Play/Inner Default) must offer Change duration inside the fixed Edit Retime"
    );
}
