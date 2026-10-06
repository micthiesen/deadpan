//! `SetGag`: an inserted gag's parameters changed after insertion.

use super::*;
use crate::{ExactRatio, GagRecipe, HoldVideo, PauseLength, PauseProvider};

fn plan_gags(
    document: &ProjectDocument,
    context: SemanticContext,
    instructions: Vec<SemanticInstruction>,
) -> Result<SemanticPlan, EditError> {
    plan_semantic_with_speech(
        document,
        &context,
        &program(instructions),
        SemanticRegisterBank {
            entries: &BTreeMap::new(),
            version: 7,
        },
        revision("outer"),
        allocate,
        no_original,
        |_| Err(crate::speech_unavailable()),
        |_, _| {
            Ok(PauseProvider {
                video: HoldVideo::Background,
                picture_context: None,
                audio: crate::HoldAudio::Silence,
            })
        },
    )
}

fn frames(value: u32) -> PauseLength {
    PauseLength::Frames {
        frames: NonZeroU32::new(value).unwrap(),
    }
}

fn long_answer(pause: u32, scale: (i128, i128)) -> GagRecipe {
    GagRecipe::LongAnswer {
        version: crate::GAG_RECIPE_VERSION,
        pause: frames(pause),
        scale: ExactRatio::new(scale.0, scale.1).unwrap(),
    }
}

fn one_more_time(plays: u32, gap: u32, shorten: u32) -> GagRecipe {
    GagRecipe::OneMoreTime {
        version: crate::GAG_RECIPE_VERSION,
        plays: NonZeroU32::new(plays).unwrap(),
        gap: frames(gap),
        shorten: frames(shorten),
        variation: None,
    }
}

fn hold_frames(document: &ProjectDocument, node: &NodeId) -> i64 {
    match &document.nodes()[node].kind {
        NodeKind::Hold { recipe } => recipe.duration.frames(),
        other => panic!("{other:?}"),
    }
}

fn group_children(document: &ProjectDocument, group: &NodeId) -> Vec<NodeId> {
    match &document.nodes()[group].kind {
        NodeKind::Sequence { children } => children.clone(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn setting_a_long_answers_parameters_edits_its_pause_and_creep_and_relabels_it() {
    let document = tree(&["a", "b"], vec![("a", hold(3)), ("b", hold(4))]);
    let inserted = long_answer(6, (27, 20));
    let changed = long_answer(10, (3, 2));
    let planned = plan_gags(
        &document,
        context("root", 3),
        vec![
            SemanticInstruction::Gag { recipe: inserted },
            SemanticInstruction::SetGag {
                recipe: changed,
                parameters: Vec::new(),
            },
        ],
    )
    .unwrap();
    let group = planned.context.selected_child.clone().unwrap();
    assert_eq!(planned.document.nodes()[&group].label, changed.label());
    assert_eq!(
        GagRecipe::from_label(&planned.document.nodes()[&group].label),
        Some(changed)
    );
    let children = group_children(&planned.document, &group);
    assert_eq!(children.len(), 1);
    assert_eq!(hold_frames(&planned.document, &children[0]), 10);
    // The creep now ends at 1.5x, as a fresh insertion would author it.
    let fresh = plan_gags(
        &document,
        context("root", 3),
        vec![SemanticInstruction::Gag { recipe: changed }],
    )
    .unwrap();
    let fresh_group = fresh.context.selected_child.clone().unwrap();
    let fresh_pause = group_children(&fresh.document, &fresh_group)[0].clone();
    assert_eq!(
        planned.document.nodes()[&children[0]].framing,
        fresh.document.nodes()[&fresh_pause].framing
    );
    assert_eq!(planned.document.duration().unwrap().frames(), 17);
    assert_eq!(planned.context.cursor, ProjectFrame(3));
    assert_eq!(
        planned.trace.last().unwrap().resolved_range,
        Some(range(3, 13))
    );
    let replay =
        crate::replay_compound::<EditError>(&document, planned.request.as_ref().unwrap(), |_| {
            Ok(())
        })
        .unwrap();
    assert_eq!(replay.document, planned.document);
    assert_eq!(
        replay.edit.inverse.apply(&replay.document).unwrap(),
        document
    );
    // Only the pause length: the creep (in the pause's own progress) stays.
    let pause_only = plan_gags(
        &document,
        context("root", 3),
        vec![
            SemanticInstruction::Gag { recipe: inserted },
            SemanticInstruction::SetGag {
                recipe: long_answer(8, (27, 20)),
                parameters: Vec::new(),
            },
        ],
    )
    .unwrap();
    let pause = group_children(
        &pause_only.document,
        &pause_only.context.selected_child.unwrap(),
    )[0]
    .clone();
    assert_eq!(hold_frames(&pause_only.document, &pause), 8);
}

#[test]
fn setting_one_more_time_changes_its_plays_and_gap_ladder() {
    let document = tree(&["a", "b"], vec![("a", hold(3)), ("b", hold(4))]);
    let mut start = context("root", 0);
    start.selected_child = Some(node("a"));
    let planned = plan_gags(
        &document,
        start.clone(),
        vec![
            SemanticInstruction::Gag {
                recipe: one_more_time(4, 6, 2),
            },
            SemanticInstruction::SetGag {
                recipe: one_more_time(3, 5, 1),
                parameters: Vec::new(),
            },
        ],
    )
    .unwrap();
    // Three 3-frame plays with 5 and 4 frames between them, then b.
    assert_eq!(planned.document.duration().unwrap().frames(), 9 + 9 + 4);
    let group = planned.context.selected_child.clone().unwrap();
    assert_eq!(
        planned.document.nodes()[&group].label,
        "One More Time · v1 · 3 plays, gap 5f shortening by 1f"
    );
    // The same edit equals inserting the new recipe directly.
    let fresh = plan_gags(
        &document,
        start.clone(),
        vec![SemanticInstruction::Gag {
            recipe: one_more_time(3, 5, 1),
        }],
    )
    .unwrap();
    assert_eq!(
        fresh.document.duration().unwrap(),
        planned.document.duration().unwrap()
    );
    let replay =
        crate::replay_compound::<EditError>(&document, planned.request.as_ref().unwrap(), |_| {
            Ok(())
        })
        .unwrap();
    assert_eq!(
        replay.edit.inverse.apply(&replay.document).unwrap(),
        document
    );
}

#[test]
fn a_gag_set_refuses_other_recipes_unchanged_values_and_reshaped_parts() {
    let document = tree(&["a", "b"], vec![("a", hold(3)), ("b", hold(4))]);
    let inserted = long_answer(6, (27, 20));
    let gag = SemanticInstruction::Gag { recipe: inserted };
    // Unchanged parameters and another recipe refuse.
    for refused in [
        SemanticInstruction::SetGag {
            recipe: inserted,
            parameters: Vec::new(),
        },
        SemanticInstruction::SetGag {
            recipe: one_more_time(3, 5, 1),
            parameters: Vec::new(),
        },
    ] {
        assert!(plan_gags(&document, context("root", 3), vec![gag.clone(), refused]).is_err());
    }
    // A plain beat or a renamed group is not a gag.
    let mut plain = context("root", 0);
    plain.selected_child = Some(node("a"));
    assert!(plan_gags(&document, plain, vec![set(long_answer(8, (27, 20)))]).is_err());
    // A gag that gained a second part has another shape.
    let grouped = tree(
        &["g", "b"],
        vec![
            (
                "g",
                BeatNode::sequence(inserted.label(), vec![node("x"), node("y")]),
            ),
            ("x", hold(3)),
            ("y", hold(2)),
            ("b", hold(4)),
        ],
    );
    let mut selected = context("root", 0);
    selected.selected_child = Some(node("g"));
    let error = plan_gags(
        &grouped,
        selected,
        vec![SemanticInstruction::SetGag {
            recipe: long_answer(8, (27, 20)),
            parameters: Vec::new(),
        }],
    )
    .unwrap_err();
    assert!(
        error.message.contains("changed after it was inserted"),
        "{error:?}"
    );
}

fn set(recipe: GagRecipe) -> SemanticInstruction {
    SemanticInstruction::SetGag {
        recipe,
        parameters: Vec::new(),
    }
}

fn escalator(plays: u32, gain: i32, zoom: (i128, i128)) -> GagRecipe {
    GagRecipe::Escalator {
        version: crate::GAG_RECIPE_VERSION,
        plays: NonZeroU32::new(plays).unwrap(),
        gain_step: crate::GainDb::new(gain).unwrap(),
        zoom_step: crate::quantize_zoom_step(ExactRatio::new(zoom.0, zoom.1).unwrap()).unwrap(),
    }
}

fn steps(planned: &SemanticPlan) -> usize {
    match &planned.request.as_ref().unwrap().command {
        crate::Command::Compound { transaction } => transaction.steps().len(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn setting_an_escalators_plays_and_steps_rewrites_its_repeat_and_zero_zoom_removes_the_zoom() {
    let document = tree(&["a", "b"], vec![("a", hold(3)), ("b", hold(4))]);
    let mut start = context("root", 0);
    start.selected_child = Some(node("a"));
    let inserted = escalator(3, 3_000, (8, 100));
    for changed in [escalator(4, 2_000, (8, 100)), escalator(3, 3_000, (0, 1))] {
        let planned = plan_gags(
            &document,
            start.clone(),
            vec![SemanticInstruction::Gag { recipe: inserted }, set(changed)],
        )
        .unwrap();
        let group = planned.context.selected_child.clone().unwrap();
        assert_eq!(planned.document.nodes()[&group].label, changed.label());
        let repeat = group_children(&planned.document, &group)[0].clone();
        let GagRecipe::Escalator {
            plays,
            gain_step,
            zoom_step,
            ..
        } = changed
        else {
            unreachable!()
        };
        assert!(matches!(
            &planned.document.nodes()[&repeat].kind,
            NodeKind::Repeat { iterations, escalation: Some(escalation), .. }
                if iterations.len() == plays.get()
                    && escalation.gain_step == gain_step
                    && escalation.zoom.map(|zoom| zoom.step) == (zoom_step != ExactRatio::ZERO).then_some(zoom_step)
        ));
        let replay = crate::replay_compound::<EditError>(
            &document,
            planned.request.as_ref().unwrap(),
            |_| Ok(()),
        )
        .unwrap();
        assert_eq!(
            replay.edit.inverse.apply(&replay.document).unwrap(),
            document
        );
    }
}

#[test]
fn a_hand_changed_part_refuses_instead_of_being_rebuilt() {
    let document = tree(&["a", "b"], vec![("a", hold(3)), ("b", hold(4))]);
    let mut start = context("root", 0);
    start.selected_child = Some(node("a"));
    let inserted = plan_gags(
        &document,
        start.clone(),
        vec![SemanticInstruction::Gag {
            recipe: one_more_time(4, 6, 2),
        }],
    )
    .unwrap();
    let group = inserted.context.selected_child.clone().unwrap();
    let repeat = group_children(&inserted.document, &group)[0].clone();
    // `:repeat 5` on the part, by hand.
    let edit = |document: &ProjectDocument, command: crate::Command| {
        let request = crate::CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: revision("hand"),
            command,
        };
        crate::apply(document, &request)
            .unwrap()
            .forward
            .apply(document)
            .unwrap()
    };
    let changed = edit(
        &inserted.document,
        crate::Command::SetRepeatPlays {
            node: repeat.clone(),
            plays: 5,
            timing: crate::AudioTimingId {
                allocation: revision("hand"),
                ordinal: 0,
            },
        },
    );
    let mut selected = context("root", 0);
    selected.selected_child = Some(group.clone());
    let error = plan_gags(
        &changed,
        selected.clone(),
        vec![set(one_more_time(3, 6, 2))],
    )
    .unwrap_err();
    assert!(
        error.message.contains("changed after it was inserted"),
        "{error:?}"
    );
    // Framing added by hand to a Long Answer's pause refuses too.
    let long = plan_gags(
        &document,
        context("root", 3),
        vec![SemanticInstruction::Gag {
            recipe: long_answer(6, (27, 20)),
        }],
    )
    .unwrap();
    let group = long.context.selected_child.clone().unwrap();
    let pause = group_children(&long.document, &group)[0].clone();
    let reframed = edit(
        &long.document,
        crate::Command::SetFraming {
            node: pause,
            framing: None,
        },
    );
    let mut selected = context("root", 3);
    selected.selected_child = Some(group);
    let error = plan_gags(&reframed, selected, vec![set(long_answer(8, (27, 20)))]).unwrap_err();
    assert!(
        error.message.contains("changed after it was inserted"),
        "{error:?}"
    );
}

#[test]
fn named_parameters_change_only_themselves_so_dot_keeps_the_other_gags_values() {
    let document = tree(&["a", "b"], vec![("a", hold(3)), ("b", hold(4))]);
    let mut start = context("root", 0);
    start.selected_child = Some(node("a"));
    // `:gag-set plays=2` as recorded on another gag (gap 6f, shorten 2f):
    // only Plays is taken, so this gag keeps gap 8f shortening by 3f.
    let planned = plan_gags(
        &document,
        start,
        vec![
            SemanticInstruction::Gag {
                recipe: one_more_time(3, 8, 3),
            },
            SemanticInstruction::SetGag {
                recipe: one_more_time(2, 6, 2),
                parameters: vec![crate::GagParameter::Plays],
            },
        ],
    )
    .unwrap();
    let group = planned.context.selected_child.clone().unwrap();
    assert_eq!(
        planned.document.nodes()[&group].label,
        "One More Time · v1 · 2 plays, gap 8f shortening by 3f"
    );
    // A parameter the recipe lacks refuses.
    assert!(
        one_more_time(2, 6, 2)
            .with_parameters(&one_more_time(3, 6, 2), &[crate::GagParameter::Creep])
            .is_err()
    );
}

#[test]
fn every_gag_set_stays_within_its_resolved_step_estimate() {
    let document = tree(&["a", "b"], vec![("a", hold(3)), ("b", hold(4))]);
    let mut start = context("root", 0);
    start.selected_child = Some(node("a"));
    // The Gag stages two leaves and a group; SetGag at most five more.
    for (inserted, changed) in [
        (one_more_time(4, 6, 2), one_more_time(3, 5, 1)),
        (escalator(3, 3_000, (8, 100)), escalator(5, 1_000, (0, 1))),
    ] {
        let alone = plan_gags(
            &document,
            start.clone(),
            vec![SemanticInstruction::Gag { recipe: inserted }],
        )
        .unwrap();
        let planned = plan_gags(
            &document,
            start.clone(),
            vec![SemanticInstruction::Gag { recipe: inserted }, set(changed)],
        )
        .unwrap();
        assert!(
            steps(&planned) - steps(&alone) <= 5,
            "{} steps",
            steps(&planned) - steps(&alone)
        );
    }
    let alone = plan_gags(
        &document,
        context("root", 3),
        vec![SemanticInstruction::Gag {
            recipe: long_answer(6, (27, 20)),
        }],
    )
    .unwrap();
    let planned = plan_gags(
        &document,
        context("root", 3),
        vec![
            SemanticInstruction::Gag {
                recipe: long_answer(6, (27, 20)),
            },
            set(long_answer(9, (3, 2))),
        ],
    )
    .unwrap();
    assert!(steps(&planned) - steps(&alone) <= 5);
}
