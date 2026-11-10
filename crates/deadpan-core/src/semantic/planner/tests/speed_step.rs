//! `:repeat … speed-step=`: per-play Retime overrides made from a recipe the
//! Repeat records, so later changes extend, change or remove them.

use super::*;
use crate::{ExactRatio, PauseLength, PitchPolicy, SpeedStep};

/// Fresh identities for each chained plan: `prefix` keeps them apart.
fn allocator(
    prefix: String,
) -> impl FnMut(SemanticAllocationRequest) -> Result<SemanticAllocation, EditError> {
    move |request| {
        let leaf = |step: usize| revision(&format!("{prefix}-leaf-{step}"));
        Ok(match request {
            SemanticAllocationRequest::Isolation {
                step_index,
                nodes,
                marks,
            } => SemanticAllocation::Isolation {
                new_revision: leaf(step_index),
                identities: crate::OccurrenceIdentities {
                    nodes: (0..nodes)
                        .map(|n| node(&format!("{prefix}-isolate-{step_index}-{n}")))
                        .collect(),
                    marks: (0..marks)
                        .map(|n| {
                            crate::MarkId::new(format!("{prefix}-isolate-{step_index}-{n}"))
                                .unwrap()
                        })
                        .collect(),
                },
            },
            SemanticAllocationRequest::SetRepeatPlays { step_index } => {
                SemanticAllocation::SetRepeatPlays {
                    new_revision: leaf(step_index),
                }
            }
            SemanticAllocationRequest::ParameterEdit { step_index } => {
                SemanticAllocation::ParameterEdit {
                    new_revision: leaf(step_index),
                }
            }
            SemanticAllocationRequest::SetRepeatGaps {
                step_index,
                branches,
            } => SemanticAllocation::SetRepeatGaps {
                new_revision: leaf(step_index),
                nodes: (0..branches)
                    .map(|n| node(&format!("{prefix}-gap-{step_index}-{n}")))
                    .collect(),
            },
            SemanticAllocationRequest::Repeat {
                step_index,
                required_split_ids,
                needs_group,
            } => SemanticAllocation::Repeat {
                new_revision: leaf(step_index),
                identities: RepeatSelectionIdentities {
                    repeat: node(&format!("{prefix}-repeat-{step_index}")),
                    group: needs_group.then(|| node(&format!("{prefix}-group-{step_index}"))),
                    split: SplitIdentities {
                        nodes: (0..required_split_ids)
                            .map(|n| node(&format!("{prefix}-split-{step_index}-{n}")))
                            .collect(),
                    },
                },
            },
            other => return allocate(other),
        })
    }
}

fn plan_steps(
    document: &ProjectDocument,
    selected: &str,
    instructions: Vec<SemanticInstruction>,
) -> Result<SemanticPlan, EditError> {
    static PLANS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let prefix = format!(
        "p{}",
        PLANS.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    let context = SemanticContext {
        selected_child: Some(node(selected)),
        ..context("root", 0)
    };
    plan_semantic_with_speech(
        document,
        &context,
        &program(instructions),
        SemanticRegisterBank {
            entries: &BTreeMap::new(),
            version: 7,
        },
        revision(&format!("{prefix}-outer")),
        allocator(prefix.clone()),
        no_original,
        |_| Err(crate::speech_unavailable()),
        |_, _| {
            Ok(crate::PauseProvider {
                video: crate::HoldVideo::Background,
                picture_context: None,
                audio: crate::HoldAudio::Silence,
                cutaways: Vec::new(),
            })
        },
    )
}

/// The plan, after checking its request replays to it and undoes exactly.
fn applied(document: &ProjectDocument, plan: &SemanticPlan) -> ProjectDocument {
    let request = plan.request.as_ref().expect("an edit");
    let edit = crate::apply(document, request).unwrap();
    let mut after = edit.forward.apply(document).unwrap();
    assert_eq!(edit.inverse.apply(&after).unwrap(), *document, "exact Undo");
    after.revision_id = request.new_revision.clone();
    after.validate().unwrap();
    assert_eq!(after, plan.document);
    after
}

fn speed(step: ExactRatio, multiply: bool, preserve_pitch: bool) -> Option<SpeedStep> {
    Some(SpeedStep {
        step,
        multiply,
        preserve_pitch,
    })
}

fn set(plays: Option<u32>, speed: Option<SpeedStep>) -> SemanticInstruction {
    SemanticInstruction::SetRepeat {
        plays: plays.and_then(NonZeroU32::new),
        gaps: None,
        escalation: None,
        speed,
    }
}

/// Each play's `(start, duration, gap_after)` and its override's pitch.
fn plays(document: &ProjectDocument, repeat: &NodeId) -> Vec<(i64, i64, i64, Option<PitchPolicy>)> {
    let NodeKind::Repeat {
        child,
        iterations,
        gap,
        ..
    } = &document.nodes()[repeat].kind
    else {
        panic!("a Repeat")
    };
    let layout = crate::RepeatLayout::compile_with_gap_overrides(
        iterations,
        child,
        document.overrides().get(repeat),
        gap.as_ref().map_or(FrameDuration::ZERO, |gap| gap.duration),
        document.gap_overrides().get(repeat),
        &document.durations().unwrap(),
    )
    .unwrap();
    (0..iterations.len())
        .map(|index| {
            let iteration = iterations.at(index).unwrap();
            let play = layout.play(&iteration).unwrap();
            let pitch = document
                .overrides()
                .get(repeat)
                .and_then(|plays| plays.get(&iteration))
                .map(|root| match &document.nodes()[root].kind {
                    NodeKind::Retime { pitch, .. } => *pitch,
                    _ => panic!("a speed override is a Retime"),
                });
            (
                play.start,
                play.duration.frames(),
                play.gap_after.frames(),
                pitch,
            )
        })
        .collect()
}

fn recipe(document: &ProjectDocument, repeat: &NodeId) -> Option<SpeedStep> {
    match &document.nodes()[repeat].kind {
        NodeKind::Repeat { escalation, .. } => escalation.as_ref().and_then(|e| e.speed),
        _ => panic!("a Repeat"),
    }
}

fn ntsc(frames: i64) -> ProjectDocument {
    let mut document = tree(&["a", "b"], vec![("a", hold(frames)), ("b", hold(4))]);
    document.presentation_basis.frame_rate = crate::FrameRate::new(30000, 1001).unwrap();
    document
}

#[test]
fn each_later_play_runs_at_its_own_exact_speed_with_exact_starts_and_gaps() {
    let document = ntsc(13);
    // :repeat 4 gap=100ms speed-step=25% pitch=tape at 30000/1001 fps.
    let planned = plan_steps(
        &document,
        "a",
        vec![SemanticInstruction::SetRepeat {
            plays: NonZeroU32::new(4),
            gaps: Some(vec![PauseLength::Milliseconds {
                milliseconds: NonZeroU32::new(100).unwrap(),
            }]),
            escalation: None,
            speed: speed(ExactRatio::new(1, 4).unwrap(), false, false),
        }],
    )
    .unwrap();
    let after = applied(&document, &planned);
    let repeat = planned.context.selected_child.clone().unwrap();
    // Speeds 1, 1.25, 1.5 and 1.75: 13/1.25 = 10.4 → 10, 13/1.5 = 8.67 → 9,
    // 13/1.75 = 7.43 → 7, each rounded once. 100 ms is 2.997 → 3 frames.
    let tape = Some(PitchPolicy::FollowSpeed);
    assert_eq!(
        plays(&after, &repeat),
        vec![
            (0, 13, 3, None),
            (16, 10, 3, tape),
            (29, 9, 3, tape),
            (41, 7, 0, tape),
        ]
    );
    assert_eq!(after.duration().unwrap().frames(), 48 + 4);
    assert_eq!(
        recipe(&after, &repeat),
        speed(ExactRatio::new(1, 4).unwrap(), false, false)
    );
    // The recipe is not live: the plan reads only the overrides, so the
    // first play keeps the shared definition.
    assert_eq!(after.overrides()[&repeat].len(), 3);

    // Multiplying by 1.5 per play: 13, 8.67 → 9, 5.78 → 6, 3.85 → 4.
    let multiplied = plan_steps(
        &document,
        "a",
        vec![set(
            Some(4),
            speed(ExactRatio::new(1, 2).unwrap(), true, true),
        )],
    )
    .unwrap();
    let multiplied = applied(&document, &multiplied);
    let repeat_multiplied = multiplied.parent_of(&node("a")).unwrap();
    let kept = Some(PitchPolicy::Preserve);
    assert_eq!(
        plays(&multiplied, &repeat_multiplied),
        vec![
            (0, 13, 0, None),
            (13, 9, 0, kept),
            (22, 6, 0, kept),
            (28, 4, 0, kept)
        ]
    );
}

#[test]
fn the_recipe_changes_extends_and_removes_its_own_plays() {
    let document = ntsc(13);
    let first = plan_steps(
        &document,
        "a",
        vec![set(
            Some(3),
            speed(ExactRatio::new(1, 4).unwrap(), false, false),
        )],
    )
    .unwrap();
    let first = applied(&document, &first);
    let repeat = first.parent_of(&node("a")).unwrap();
    let roots: Vec<NodeId> = first.overrides()[&repeat]
        .iter()
        .map(|(_, root)| root.clone())
        .collect();

    // A new step keeps each play's Retime and its contents; an omitted
    // pitch is given explicitly here, as the app resolves it.
    let half = plan_steps(
        &first,
        repeat.as_str(),
        vec![set(
            None,
            speed(ExactRatio::new(1, 2).unwrap(), false, true),
        )],
    )
    .unwrap();
    let half = applied(&first, &half);
    let preserve = Some(PitchPolicy::Preserve);
    // Speeds 1, 1.5, 2: 8.67 → 9 and 6.5 → 6 (ties to even).
    assert_eq!(
        plays(&half, &repeat),
        vec![(0, 13, 0, None), (13, 9, 0, preserve), (22, 6, 0, preserve)]
    );
    let kept: Vec<NodeId> = half.overrides()[&repeat]
        .iter()
        .map(|(_, root)| root.clone())
        .collect();
    assert_eq!(kept, roots, "the same Retime overrides change in place");

    // A larger count continues the progression onto the new plays, through
    // the plain count setter as well as SetRepeat.
    for instruction in [
        SemanticInstruction::SetRepeatPlays {
            plays: NonZeroU32::new(5).unwrap(),
        },
        set(Some(5), None),
    ] {
        let more = plan_steps(&half, repeat.as_str(), vec![instruction]).unwrap();
        let more = applied(&half, &more);
        // Speeds 2.5 and 3: 5.2 → 5 and 4.33 → 4.
        assert_eq!(
            plays(&more, &repeat)[3..],
            [(28, 5, 0, preserve), (33, 4, 0, preserve)]
        );
    }

    // Repeating the same recipe changes nothing.
    let same = plan_steps(
        &half,
        repeat.as_str(),
        vec![set(
            None,
            speed(ExactRatio::new(1, 2).unwrap(), false, true),
        )],
    )
    .unwrap();
    assert!(
        same.request.is_none(),
        "no revision for an unchanged recipe"
    );

    // A zero step returns every play to normal speed and drops the recipe.
    let removed = plan_steps(
        &half,
        repeat.as_str(),
        vec![set(None, speed(ExactRatio::ZERO, false, true))],
    )
    .unwrap();
    let removed = applied(&half, &removed);
    assert_eq!(
        plays(&removed, &repeat)
            .iter()
            .map(|play| play.1)
            .collect::<Vec<_>>(),
        vec![13, 13, 13]
    );
    assert_eq!(recipe(&removed, &repeat), None);
    // ...and a new recipe can take those plays again.
    let again = plan_steps(
        &removed,
        repeat.as_str(),
        vec![set(None, speed(ExactRatio::integer(1), false, false))],
    )
    .unwrap();
    let again = applied(&removed, &again);
    assert_eq!(
        plays(&again, &repeat)
            .iter()
            .map(|play| play.1)
            .collect::<Vec<_>>(),
        // Speeds 1, 2 and 3: 6.5 → 6 (ties to even) and 4.33 → 4.
        vec![13, 6, 4]
    );
}

#[test]
fn hand_changed_plays_refuse_a_new_recipe_and_are_left_alone_by_a_count() {
    let document = ntsc(12);
    let first = plan_steps(
        &document,
        "a",
        vec![set(
            Some(3),
            speed(ExactRatio::new(1, 2).unwrap(), false, true),
        )],
    )
    .unwrap();
    let first = applied(&document, &first);
    let repeat = first.parent_of(&node("a")).unwrap();
    // Play 2 (speed 1.5, 8 frames) is retimed by hand to 10 frames.
    let NodeKind::Repeat { iterations, .. } = &first.nodes()[&repeat].kind else {
        panic!("a Repeat")
    };
    let second = first.overrides()[&repeat]
        .get(&iterations.at(1).unwrap())
        .unwrap()
        .clone();
    let edit = crate::apply(
        &first,
        &crate::CommandRequest {
            project_id: first.project_id.clone(),
            expected_revision: first.revision_id.clone(),
            new_revision: revision("by-hand"),
            command: Command::SetRetime {
                node: second.clone(),
                duration: FrameDuration::new(10).unwrap(),
                pitch: PitchPolicy::Preserve,
            },
        },
    )
    .unwrap();
    let mut edited = edit.forward.apply(&first).unwrap();
    edited.revision_id = revision("by-hand");
    let error = plan_steps(
        &edited,
        repeat.as_str(),
        vec![set(None, speed(ExactRatio::integer(1), false, true))],
    )
    .unwrap_err();
    assert!(error.to_string().contains("play 2"), "{error}");
    // A larger count continues the recipe on the new play only.
    let more = plan_steps(
        &edited,
        repeat.as_str(),
        vec![SemanticInstruction::SetRepeatPlays {
            plays: NonZeroU32::new(4).unwrap(),
        }],
    )
    .unwrap();
    let more = applied(&edited, &more);
    assert_eq!(
        plays(&more, &repeat)
            .iter()
            .map(|play| play.1)
            .collect::<Vec<_>>(),
        // Play 2 keeps its 10 frames; play 4 runs at 2.5: 4.8 → 5.
        vec![12, 10, 6, 5]
    );
}

#[test]
fn speeds_outside_the_limits_refuse_naming_the_last_play() {
    let document = ntsc(12);
    for (plays, step, multiply) in [
        // 1 - 2·0.5 = 0 on the third play.
        (3, ExactRatio::new(-1, 2).unwrap(), false),
        // 2^7 = 128 times normal on the eighth play.
        (8, ExactRatio::integer(1), true),
        // A zero factor.
        (2, ExactRatio::integer(-1), true),
    ] {
        let error = plan_steps(
            &document,
            "a",
            vec![set(Some(plays), speed(step, multiply, true))],
        )
        .unwrap_err();
        assert!(error.to_string().contains("speed"), "{error}");
    }
    // Growing a recipe's Repeat past the limit refuses too.
    let doubling = plan_steps(
        &document,
        "a",
        vec![set(Some(7), speed(ExactRatio::integer(1), true, true))],
    )
    .unwrap();
    let doubling = applied(&document, &doubling);
    let repeat = doubling.parent_of(&node("a")).unwrap();
    assert!(
        plan_steps(
            &doubling,
            repeat.as_str(),
            vec![SemanticInstruction::SetRepeatPlays {
                plays: NonZeroU32::new(8).unwrap(),
            }],
        )
        .is_err()
    );
    // One play has nothing to speed up, and nothing to remove refuses.
    assert!(
        plan_steps(
            &document,
            "a",
            vec![set(Some(1), speed(ExactRatio::integer(1), false, true))]
        )
        .is_err()
    );
    assert!(
        plan_steps(
            &doubling,
            repeat.as_str(),
            vec![set(None, speed(ExactRatio::ZERO, false, true))]
        )
        .is_ok()
    );
    let plain = plan_steps(&document, "a", vec![set(Some(3), None)]).unwrap();
    let plain = applied(&document, &plain);
    let plain_repeat = plain.parent_of(&node("a")).unwrap();
    assert!(
        plan_steps(
            &plain,
            plain_repeat.as_str(),
            vec![set(None, speed(ExactRatio::ZERO, false, true))]
        )
        .is_err()
    );
}

#[test]
fn explode_keeps_each_play_at_its_speed_without_an_escalation_wrapper() {
    let document = ntsc(13);
    let sped = plan_steps(
        &document,
        "a",
        vec![set(
            Some(3),
            speed(ExactRatio::new(1, 4).unwrap(), false, false),
        )],
    )
    .unwrap();
    let sped = applied(&document, &sped);
    let repeat = sped.parent_of(&node("a")).unwrap();
    let exploded = plan_steps(&sped, repeat.as_str(), vec![SemanticInstruction::Explode]).unwrap();
    let exploded = applied(&sped, &exploded);
    let NodeKind::Sequence { children } = &exploded.nodes()[&repeat].kind else {
        panic!("Explode leaves a Sequence")
    };
    let durations = exploded.durations().unwrap();
    // The plays themselves, each the same Retime it was, with no group.
    assert_eq!(
        children
            .iter()
            .map(|child| durations[child].frames())
            .collect::<Vec<_>>(),
        vec![13, 10, 9]
    );
    for child in &children[1..] {
        assert!(matches!(
            exploded.nodes()[child].kind,
            NodeKind::Retime {
                pitch: PitchPolicy::FollowSpeed,
                ..
            }
        ));
    }
    assert_eq!(exploded.duration().unwrap(), sped.duration().unwrap());
}
