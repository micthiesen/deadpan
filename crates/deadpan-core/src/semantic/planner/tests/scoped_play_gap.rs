//! `SetPlayGap` edits the shared default gap after one chosen play: one
//! Compound isolates that gap and applies the pause edit to the copy only.

use super::*;
use crate::{AudioTimingId, GainDb, PauseLength, PlayGapChange, capture_unbound_audio_bindings};

fn tone() -> HoldAudio {
    HoldAudio::Tone {
        frequency_hz: 440,
        level: GainDb::new(-6000).unwrap(),
    }
}

fn toned(frames: i64) -> BeatNode {
    let mut beat = hold(frames);
    let NodeKind::Hold { recipe } = &mut beat.kind else {
        unreachable!()
    };
    recipe.audio = tone();
    beat
}

/// The scoped fixture with a 2-frame default gap in `inner` and a 3-frame gap
/// in `outer`, both sounding, and every physical owner bound to its clock.
fn gapped() -> ProjectDocument {
    let mut document = fixture();
    for (repeat, frames) in [("inner", 2), ("outer", 3)] {
        let NodeKind::Repeat { gap, .. } = &mut document.nodes.get_mut(&node(repeat)).unwrap().kind
        else {
            unreachable!()
        };
        let NodeKind::Hold { recipe } = toned(frames).kind else {
            unreachable!()
        };
        *gap = Some(recipe);
    }
    document.nodes.insert(node("tail"), toned(2));
    document.nodes.insert(node("lead"), toned(2));
    let bindings = capture_unbound_audio_bindings(
        &document,
        AudioTimingId {
            allocation: revision("gap-capture"),
            ordinal: 0,
        },
    )
    .unwrap();
    let mut wire = serde_json::to_value(&document).unwrap();
    wire["audio_bindings"] = serde_json::to_value(bindings).unwrap();
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    document.validate().unwrap();
    document
}

/// This-play authority over the contents of `inner` play `inner` inside
/// `outer` play `outer`, or over the body of `outer` play `outer`.
fn play_context(outer: u32, inner: Option<u32>) -> SemanticContext {
    let mut context = entry();
    let mut within = presentation(outer, inner.unwrap_or(0));
    within.node = node(if inner.is_some() { "inside" } else { "body" });
    if inner.is_none() {
        within.repeats.pop();
    }
    let scoped = context.scoped.as_mut().unwrap();
    scoped.target = ScopedNodeTarget {
        node: within.node.clone(),
        repeats: within
            .repeats
            .iter()
            .map(|step| RepeatEditStep {
                repeat: step.node.clone(),
                branch: RepeatEditBranch::Play {
                    iteration: step.iteration.clone(),
                },
            })
            .collect(),
    };
    scoped.presentation = within;
    context
}

fn plan(
    document: &ProjectDocument,
    context: &SemanticContext,
    change: PlayGapChange,
) -> Result<SemanticPlan, EditError> {
    plan_semantic_with_framing(
        document,
        context,
        &program(vec![SemanticInstruction::SetPlayGap { change }]),
        SemanticRegisterBank {
            entries: &BTreeMap::new(),
            version: 3,
        },
        revision("gap-outer"),
        super::super::allocate,
        no_original,
        |_| Err(speech_unavailable()),
        |_: &ProjectDocument, _| panic!("a play's gap copies its recipe"),
        None,
        state,
    )
}

fn length(frames: u32) -> PlayGapChange {
    PlayGapChange::Length {
        length: PauseLength::Frames {
            frames: NonZeroU32::new(frames).unwrap(),
        },
    }
}

fn leaves(plan: &SemanticPlan, before: &ProjectDocument) -> Vec<Command> {
    let Command::Compound { transaction } = &plan.request.as_ref().unwrap().command else {
        panic!("one Compound")
    };
    let mut staged = before.clone();
    transaction
        .steps()
        .iter()
        .map(|step| {
            let request = step.edit().unwrap().request(&staged);
            staged = crate::apply(&staged, &request)
                .unwrap()
                .forward
                .apply(&staged)
                .unwrap();
            request.command
        })
        .collect()
}

fn check_undo(before: &ProjectDocument, plan: &SemanticPlan) {
    let request = plan.request.as_ref().unwrap();
    let replay = crate::replay_compound::<EditError>(before, request, |_| Ok(())).unwrap();
    assert_eq!(replay.document, plan.document);
    assert_eq!(replay.edit.inverse.apply(&plan.document).unwrap(), *before);
}

fn gap_branch(document: &ProjectDocument, repeat: &NodeId, play: &IterationId) -> NodeId {
    document.gap_overrides()[repeat].get(play).unwrap().clone()
}

#[test]
fn top_level_shared_gap_sound_isolates_without_copying_plays() {
    let before = gapped();
    let plan = plan(
        &before,
        &play_context(1, None),
        PlayGapChange::Audio {
            audio: HoldAudio::Silence,
        },
    )
    .unwrap();
    let leaves = leaves(&plan, &before);
    assert_eq!(leaves.len(), 2);
    let Command::IsolateGap {
        node: repeat,
        iteration,
        id,
        ..
    } = &leaves[0]
    else {
        panic!("a top-level gap isolates directly: {:?}", leaves[0]);
    };
    assert_eq!(repeat, &node("outer"));
    assert_eq!(iteration, &iteration_of("outer", 1));
    assert_eq!(plan.document.duration(), before.duration());
    let NodeKind::Hold { recipe } = &plan.document.nodes()[id].kind else {
        panic!("the play's gap is an ordinary Hold");
    };
    assert_eq!(recipe.audio, HoldAudio::Silence);
    assert_eq!(recipe.duration.frames(), 3);
    let NodeKind::Repeat { gap, .. } = &plan.document.nodes()[&node("outer")].kind else {
        unreachable!()
    };
    assert_eq!(
        gap.as_ref().unwrap().audio,
        tone(),
        "other gaps keep the default"
    );
    assert_eq!(
        gap_branch(&plan.document, &node("outer"), &iteration_of("outer", 1)),
        *id
    );
    assert_eq!(
        plan.document.gap_overrides()[&node("outer")].len(),
        1,
        "only this play's gap is its own"
    );
    // The cursor continues at the gap's first frame.
    let start = crate::AnchorIndex::new(&plan.document)
        .unwrap()
        .resolve_target(&crate::AnchorTarget {
            boundary: BoundaryAnchor {
                coordinate: Anchor::Occurrence {
                    instance: plan.context.scoped.as_ref().unwrap().presentation.clone(),
                    position: ExactRatio::ZERO,
                },
                bias: InsertionBias::Right,
            },
            occurrence: None,
        })
        .unwrap();
    assert_eq!(plan.context.cursor, start.frame);
    check_undo(&before, &plan);
}

#[test]
fn play_gap_refusals_name_the_reason_and_plan_nothing() {
    let before = gapped();
    let refuse = |document: &ProjectDocument, context: &SemanticContext, change, expected: &str| {
        let error = plan(document, context, change).unwrap_err();
        assert!(
            error.to_string().contains(expected),
            "{error} should mention {expected:?}"
        );
    };
    refuse(
        &before,
        &play_context(1, Some(0)),
        length(2),
        "already has that length",
    );
    let mut silent = before.clone();
    let NodeKind::Repeat { gap, .. } = &mut silent.nodes.get_mut(&node("outer")).unwrap().kind
    else {
        unreachable!()
    };
    gap.as_mut().unwrap().audio = HoldAudio::Silence;
    refuse(
        &silent,
        &play_context(1, None),
        PlayGapChange::Audio {
            audio: HoldAudio::Silence,
        },
        "already has that sound",
    );
    refuse(
        &before,
        &play_context(1, Some(1)),
        length(4),
        "last play has no gap",
    );
    let mut shared = play_context(1, Some(0));
    shared.scoped.as_mut().unwrap().target.repeats[0].branch = RepeatEditBranch::Default;
    refuse(&before, &shared, length(4), "explicit Play");
    refuse(
        &fixture(),
        &play_context(0, None),
        length(4),
        "no gap between plays",
    );
    // An owned gap is already selectable and edits as an ordinary pause.
    let request = crate::CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: revision("owned-gap"),
        command: Command::IsolateGap {
            node: node("outer"),
            iteration: iteration_of("outer", 1),
            id: node("owned"),
            timing: AudioTimingId {
                allocation: revision("owned-gap"),
                ordinal: 0,
            },
        },
    };
    let owned = crate::apply(&before, &request)
        .unwrap()
        .forward
        .apply(&before)
        .unwrap();
    refuse(
        &owned,
        &play_context(1, None),
        length(6),
        "already has its own gap",
    );
}

fn iteration_of(repeat: &str, ordinal: u32) -> IterationId {
    iteration(repeat, ordinal)
}
