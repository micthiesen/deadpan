//! Native selected-play Hold duration through the scoped inspector.

use super::*;
use deadpan_core::SemanticInstruction as I;

pub(super) fn run(
    d: &mut Driver<'_>,
    live: &Path,
    original: u64,
    expected_picture: SourceFrameId,
) -> Result<(), String> {
    let baseline = document(d)?.clone();
    direct_case(d, &baseline, live, original, expected_picture, false)?;
    direct_case(d, &baseline, live, original, expected_picture, true)?;
    delayed_case(d, &baseline, original)?;
    macro_cases(d, &baseline)?;
    dot_case(d, &baseline)?;
    Ok(())
}

fn delayed_case(
    d: &mut Driver<'_>,
    baseline: &ProjectDocument,
    original: u64,
) -> Result<(), String> {
    select_pair(d, false, fixture::id("selected")?)?;
    let bank = registers(d, None);
    let revision = d.revision();
    d.app_mut().feedback.hold_project_updates = true;
    let result = (|| {
        d.command("hold-duration 3f")?;
        d.wait_for(
            "Selected-play duration is durable before away/back navigation",
            |app| !app.service.is_busy(),
        )?;
        let entry = target(d)?;
        let captured = entry.also.clone();
        d.chord(&[Key::K, Key::J, Key::L])?;
        let current = target(d)?;
        let cursor = d.app().sequence_cursor;
        d.check(
            "Selected-play away/back navigation retains the pair while replacing its navigation token",
            captured.len() == 1
                && current.target == entry.target
                && current.also == captured
                && current.navigation != entry.navigation
                && cursor == 20,
            json!({"selected_targets":2,"navigation":"new generation","Edit":20}),
            d.snapshot(),
        )?;
        d.app_mut().feedback.hold_project_updates = false;
        d.changed(&revision)?;
        settled(d)?;
        let after = document(d)?;
        let selected_three_frame_holds = after
            .nodes()
            .iter()
            .filter(|(id, node)| {
                !baseline.nodes().contains_key(*id)
                    && matches!(&node.kind, NodeKind::Hold { recipe } if recipe.duration.frames() == 3)
            })
            .count();
        d.check(
            "A delayed selected-play receipt cannot reclaim newer navigation or lose its mapped counterpart",
            d.app().scoped.is_none()
                && d.app().sequence_cursor == cursor
                && d.app().source_cursor == original
                && d.app().sequence_length() == 45
                && selected_three_frame_holds == 1
                && recipe(after, &fixture::id("inner-pause")?)?.duration.frames() == 3
                && recipe(after, &fixture::id("selected")?)?.duration.frames() == 2
                && recipe(after, &fixture::id("outer-two-pause")?)?.duration.frames() == 7
                && registers(d, None) == bank,
            json!({"Edit":20,"frames":45,"new_3f_Holds":selected_three_frame_holds,"existing_inner_pause_frames":3,"literal_default_frames":2,"continuation":"revoked","registers":"unchanged"}),
            d.snapshot(),
        )?;
        undo(d, baseline)
    })();
    d.app_mut().feedback.hold_project_updates = false;
    result
}

fn select_pair(d: &mut Driver<'_>, outer_play: bool, expected_node: NodeId) -> Result<(), String> {
    if d.app().scoped.is_some() {
        d.key(Key::Escape)?;
    }
    d.chord(&[Key::G, Key::G, Key::Num1, Key::Num9, Key::L, Key::Enter])?;
    if outer_play {
        d.command("scope play 2")?;
    }
    d.key(Key::Enter)?;
    if !outer_play {
        d.command("scope play 1")?;
    }
    d.chord(&[Key::Enter, Key::J])?;
    d.command("scope plays 1,2")?;
    // Changing the inner Repeat's selected-play set returns the inspector to
    // its body Sequence; enter that body again to select the Hold itself.
    d.chord(&[Key::Enter, Key::J])?;
    settled(d)?;
    scroll_top(d)?;
    let selected = target(d)?;
    if selected.target.node != expected_node
        || selected.also.len() != 1
        || branches(&selected) != [outer_play, true]
    {
        return Err("Selected-play navigation missed the intended shared Hold pair".into());
    }
    Ok(())
}

fn direct_case(
    d: &mut Driver<'_>,
    baseline: &ProjectDocument,
    live: &Path,
    original: u64,
    expected_picture: SourceFrameId,
    outer_play: bool,
) -> Result<(), String> {
    select_pair(d, outer_play, fixture::id("selected")?)?;
    let before = document(d)?.clone();
    let prior = target(d)?;
    let expected_scope = if outer_play {
        [true, true]
    } else {
        [false, true]
    };
    d.check(
        "Native multi-play Hold capture preserves its exact inner selection and primary picture",
        branches(&prior) == expected_scope
            && prior.also.len() == 1
            && prior.target.node == fixture::id("selected")?
            && prior.also[0].node == fixture::id("inner-pause")?
            && prior.presentation.is_some()
            && prior.cursor == ProjectFrame(19)
            && d.app().sequence_length() == 45,
        json!({"outer_play":outer_play,"inner_plays":2,"Edit":19,"frames":45}),
        d.snapshot(),
    )?;
    resize(d, 960.0, 640.0)?;
    let revision = d.revision();
    d.key(Key::Enter)?;
    d.check(
        "Multi-play Hold Enter prefills without publishing a partial edit",
        d.app().command_open
            && d.app().command == "hold-duration 2f"
            && d.revision() == revision
            && document(d)? == &before,
        json!({"prefill":"hold-duration 2f","history":"unchanged"}),
        d.snapshot(),
    )?;
    scenarios::footer_anchored(
        d,
        "Selected-play duration entry keeps the compact footer anchored",
    )?;
    d.capture(if outer_play {
        "Selected-play Hold duration under outer Play"
    } else {
        "Selected-play Hold duration under outer Default"
    })?;
    submit(d, "hold-duration 3f")?;
    d.changed(&revision)?;
    settled(d)?;
    let after = document(d)?.clone();
    let mapped = target(d)?;
    let copies = if outer_play { 13 } else { 4 };
    let other = mapped
        .also
        .first()
        .ok_or("Selected duration lost its mapped counterpart")?;
    let selected = [&mapped.target, other];
    let mapped_holds_are_three = selected.iter().all(|target| {
        matches!(after.nodes().get(&target.node).map(|node| &node.kind), Some(NodeKind::Hold { recipe }) if recipe.duration.frames() == 3)
    });
    let cloned_inner_default_is_two = if outer_play {
        mapped
            .target
            .repeats
            .get(1)
            .and_then(|step| after.nodes().get(&step.repeat))
            .and_then(|node| match &node.kind {
                NodeKind::Repeat { child, .. } => Some(child),
                _ => None,
            })
            .and_then(|child| after.nodes().get(child))
            .and_then(|node| match &node.kind {
                NodeKind::Sequence { children } if children.len() == 3 => Some(children),
                _ => None,
            })
            .is_some_and(|children| {
                children.iter().all(|child| {
                    recipe(&after, child).is_ok_and(|recipe| recipe.duration.frames() == 2)
                })
            })
    } else {
        true
    };
    let exact_primary_picture = mapped.presentation.as_ref().is_some_and(|instance| {
        [19, 21].into_iter().all(|frame| {
            d.app().workspace.as_ref().is_some_and(|workspace| {
                workspace
                    .plan
                    .picture(ProjectFrame(frame))
                    .is_ok_and(|picture| {
                        picture
                            .framing
                            .iter()
                            .any(|layer| &layer.instance == instance)
                    })
            })
        })
    });
    d.check(
        "One native duration edit updates both mapped Holds and preserves their surrounding owners",
        mapped.also.len() == 1
            && mapped.target.node != prior.target.node
            && branches(&mapped) == expected_scope
            && mapped_holds_are_three
            && cloned_inner_default_is_two
            && recipe(&after, &fixture::id("selected")?)?.duration.frames() == 2
            && recipe(&after, &fixture::id("outer-two-pause")?)?.duration.frames() == 7
            && after.nodes().len() == before.nodes().len() + copies
            && d.app().sequence_length() == 45
            && d.app().sequence_cursor == 19
            && exact_primary_picture
            && d.app().presentation.displayed_source_frame() == Some(expected_picture),
        json!({"outer_play":outer_play,"mapped_targets":2,"new_nodes":copies,"durations":[3,3],"cloned_inner_literal_default":cloned_inner_default_is_two,"unselected_outer_owned":7,"Edit":19,"frames":45,"Freeze_picture":expected_picture}),
        d.snapshot(),
    )?;
    if !outer_play {
        d.capture("Selected-play Hold duration updates both mapped Holds in one edit")?;
    }
    d.chord(&[Key::Num2, Key::L])?;
    settled(d)?;
    d.check(
        "The last sample of the selected Hold remains the same real Freeze picture",
        d.app().sequence_cursor == 21
            && d.app()
                .presentation
                .displayed_sequence()
                .is_some_and(|(frame, _)| frame == ProjectFrame(21))
            && d.app().presentation.displayed_source_frame() == Some(expected_picture),
        json!({"Edit":21,"Freeze_picture":expected_picture}),
        d.snapshot(),
    )?;

    undo(d, baseline)?;
    let revision = d.revision();
    d.key_modified(Key::R, egui::Modifiers::CTRL)?;
    d.changed(&revision)?;
    settled(d)?;
    d.check(
        "One Redo restores the exact selected-play Compound result",
        equivalent(document(d)?, &after)
            && recipe(document(d)?, &fixture::id("selected")?)?
                .duration
                .frames()
                == 2,
        json!({"history":"one exact redo","selected_targets":2}),
        d.snapshot(),
    )?;
    open(d, live, original)?;
    select_pair(d, outer_play, mapped.target.node.clone())?;
    let reopened = target(d)?;
    d.check(
        "Reopening retains both selected Hold durations, stable mappings and exact clocks",
        equivalent(document(d)?, &after)
            && reopened.also.len() == 1
            && reopened.target.node == mapped.target.node
            && reopened.also[0].node == mapped.also[0].node
            && d.app().sequence_length() == 45,
        json!({"reopened":"exact saved document","selected_targets":2,"frames":45}),
        d.snapshot(),
    )?;
    undo(d, baseline)
}

fn macro_cases(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    select_pair(d, false, fixture::id("selected")?)?;
    d.chord(&[Key::Q, Key::E])?;
    edit(d, "hold-duration 3f")?;
    d.key(Key::Q)?;
    settled(d)?;
    d.check(
        "A selected-play macro stores only its duration intent, never captured node IDs",
        saved(d, 'e') == Some(vec![intent(3)]) && {
            let capture = target(d)?;
            capture.also.len() == 1 && capture.target.node != fixture::id("selected")?
        },
        json!({"register":"e","instruction":"SetHoldDuration 3f","captured_ids":false}),
        d.snapshot(),
    )?;
    undo(d, baseline)?;

    select_pair(d, false, fixture::id("selected")?)?;
    let revision = d.revision();
    super::super::super::macros::execute(d, 'e', None)?;
    d.changed(&revision)?;
    settled(d)?;
    check_selected_result(
        d,
        45,
        "A saved duration-only macro resolves the current selected plays",
    )?;
    undo(d, baseline)?;

    select_pair(d, false, fixture::id("selected")?)?;
    d.chord(&[Key::Q, Key::F])?;
    let revision = d.revision();
    super::super::super::macros::execute(d, 'e', None)?;
    d.changed(&revision)?;
    settled(d)?;
    d.key(Key::Q)?;
    settled(d)?;
    d.check(
        "A caller of the selected-duration macro stores a name, not either Play address",
        saved(d, 'f')
            == Some(vec![I::Call {
                register: RegisterName::new('e').map_err(|error| error.to_string())?,
                count: NonZeroU32::new(1).unwrap(),
            }]),
        json!({"register":"f","instruction":"Call e once","captured_ids":false}),
        d.snapshot(),
    )?;
    undo(d, baseline)?;

    select_pair(d, false, fixture::id("selected")?)?;
    let revision = d.revision();
    super::super::super::macros::execute(d, 'f', None)?;
    d.changed(&revision)?;
    settled(d)?;
    check_selected_result(d, 45, "A named Call resolves both current selected Plays")?;
    undo(d, baseline)
}

fn dot_case(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    select_pair(d, false, fixture::id("selected")?)?;
    edit(d, "hold-duration 3f")?;
    let first = document(d)?.clone();
    let semantic_snapshot = d.app().semantic.snapshot();
    let semantic_edit = semantic_snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.edit_for(d.app().workspace.as_ref()?).ok());
    let seeded = semantic_edit.is_some_and(|edit| {
        edit.operation == crate::project::semantic::RepeatableEdit::Parameter(intent(3))
    });
    d.check(
        "A selected-duration commit records the exact duration-only repeat intent",
        seeded,
        json!({
            "expected":"SetHoldDuration 3f",
            "snapshot_version":semantic_snapshot.as_ref().map(|snapshot| snapshot.version),
            "snapshot_head":semantic_snapshot.as_ref().and_then(|snapshot| snapshot.head.as_ref()),
            "saved_edit":semantic_edit.map(|edit| format!("{:?}", edit.operation)),
        }),
        d.snapshot(),
    )?;
    d.key(Key::K)?;
    let next = target(d)?;
    let before_node = next.target.node.clone();
    d.check(
        "Navigation revokes the old selected-play counterpart mapping",
        next.also.is_empty()
            && !baseline.nodes().contains_key(&before_node)
            && recipe(document(d)?, &before_node)?.duration.frames() == 2
            && branches(&next) == [false, true]
            && next.cursor == ProjectFrame(17),
        json!({"cursor":17,"target":"before Hold","old_counterparts":"revoked"}),
        d.snapshot(),
    )?;
    d.command("scope plays 1,2")?;
    d.key(Key::Enter)?;
    settled(d)?;
    let before_pair = target(d)?;
    d.check(
        "The current before-Hold pair is explicitly selected before dot resolves again",
        before_pair.target == next.target
            && before_pair.also.len() == 1
            && before_pair.also[0].node == fixture::id("inner-before")?
            && branches(&before_pair) == [false, true]
            && recipe(document(d)?, &before_node)?.duration.frames() == 2
            && recipe(document(d)?, &fixture::id("inner-before")?)?
                .duration
                .frames()
                == 2,
        json!({"target":"before Hold","counterpart":"owned inner-before Hold","durations":[2,2]}),
        d.snapshot(),
    )?;
    let revision = d.revision();
    d.key(Key::Period)?;
    d.changed(&revision)?;
    settled(d)?;
    let selected = target(d)?;
    let selected_after = document(d)?;
    let Some(other) = selected.also.first() else {
        return Err("Selected-play dot lost its mapped counterpart".into());
    };
    let dot_holds_are_three = [&selected.target, other].iter().all(|target| {
        matches!(selected_after.nodes().get(&target.node).map(|node| &node.kind), Some(NodeKind::Hold { recipe }) if recipe.duration.frames() == 3)
    });
    d.check(
        "Dot resolves the recorded duration intent against the explicitly selected new pair",
        selected.target == before_pair.target
            && selected.also.len() == 1
            && selected.also[0].node == fixture::id("inner-before")?
            && dot_holds_are_three,
        json!({"dot":"SetHoldDuration 3f","new_targets":["before","inner-before"]}),
        d.snapshot(),
    )?;
    undo(d, &first)?;
    undo(d, baseline)
}

fn check_selected_result(d: &mut Driver<'_>, frames: u64, description: &str) -> Result<(), String> {
    let selected = target(d)?;
    let after = document(d)?;
    let other = selected
        .also
        .first()
        .ok_or("Selected-play macro lost its mapped counterpart")?;
    let both_three = [&selected.target, other].iter().all(|target| {
        matches!(after.nodes().get(&target.node).map(|node| &node.kind), Some(NodeKind::Hold { recipe }) if recipe.duration.frames() == 3)
    });
    d.check(
        description,
        d.app().sequence_length() == frames
            && selected.also.len() == 1
            && both_three
            && recipe(after, &fixture::id("selected")?)?.duration.frames() == 2
            && recipe(after, &fixture::id("outer-two-pause")?)?
                .duration
                .frames()
                == 7,
        json!({"selected_targets":2,"duration":3,"frames":frames,"unselected_owners":"preserved"}),
        d.snapshot(),
    )
}
