//! Commits adopt the core's validated result and extend the history receipt
//! instead of replaying. Over qualified sources, every committed revision must
//! equal what complete validation, full history replay and an independent
//! rebuild from storage produce.
use super::*;
use deadpan_core::{
    AudioEdgePolicy, AudioSample, AudioTimingId, FrameDuration, HoldAudio, HoldRecipe, HoldVideo,
    ProjectFrame, SoundEvent, SoundId, SoundOverflowPolicy, SourceAudio, SourceAudioMapping,
    SourceSpan, SourceTimestamp, SourceTrimCapture, SourceTrimIntent, SourceTrimPolicy,
    SourceTrimResources, SplitIdentities,
};

fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn edit(current: &ProjectDocument, next: &str, command: Command) -> CommandRequest {
    CommandRequest {
        project_id: current.project_id().clone(),
        expected_revision: current.revision_id().clone(),
        new_revision: revision(next),
        command,
    }
}

fn timing(next: &str) -> AudioTimingId {
    AudioTimingId {
        allocation: revision(next),
        ordinal: 0,
    }
}

fn trim(current: &ProjectDocument, next: &str, intent: SourceTrimIntent) -> Option<CommandRequest> {
    let resolution = current
        .source_trim_edit(&node("root"), &node("left"), Some(&node("right")), intent)
        .ok()?;
    Some(edit(
        current,
        next,
        Command::ApplySourceTrim {
            parent: node("root"),
            node: node("left"),
            right: Some(node("right")),
            intent,
            resources: SourceTrimResources {
                target_wrapper: resolution
                    .required_target_wrapper
                    .then(|| node(&format!("{next}-a"))),
                right_wrapper: resolution
                    .required_right_wrapper
                    .then(|| node(&format!("{next}-b"))),
                split: SplitIdentities {
                    nodes: (0..resolution.required_split_nodes)
                        .map(|n| node(&format!("{next}-split-{n}")))
                        .collect(),
                },
                fillers: (0..resolution.required_filler_nodes)
                    .map(|n| node(&format!("{next}-filler-{n}")))
                    .collect(),
                timing: (resolution.capture != SourceTrimCapture::None).then(|| timing(next)),
            },
        },
    ))
}

/// A selected half of `asset`'s measured audio, or a short exact span.
fn audio(document: &ProjectDocument, asset: &str, short: bool) -> Option<SourceAudio> {
    let full = document.assets().get(&id(asset))?.audio?;
    let (start, end) = if short {
        (full.start().ticks + 17, full.start().ticks + 113)
    } else {
        (
            full.start().ticks,
            full.start().ticks + (full.end().ticks - full.start().ticks) / 2,
        )
    };
    Some(SourceAudio {
        asset: id(asset),
        span: SourceSpan::new(
            SourceTimestamp {
                ticks: start,
                time_base: full.start().time_base,
            },
            SourceTimestamp {
                ticks: end,
                time_base: full.start().time_base,
            },
        )
        .ok()?,
    })
}

fn sound(document: &ProjectDocument, gain: i32) -> Option<SoundEvent> {
    let source = audio(document, "right", true)?;
    Some(SoundEvent {
        owner: document.root().clone(),
        label: "Overlay".into(),
        mapping: SourceAudioMapping::natural_rate(
            source.span,
            document.presentation_basis().frame_rate,
        )
        .ok()?,
        source,
        offset: AudioSample(0),
        gain_millidecibels: gain,
        start_edge: AudioEdgePolicy::Automatic,
        end_edge: AudioEdgePolicy::Hard,
        overflow: SoundOverflowPolicy::Reject,
    })
}

#[test]
fn qualified_commits_equal_complete_validation_replay_and_storage() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = super::roll::ready(scratch.path(), 10..13)?;
    let mut state = 0x9e37_79b9_u64;
    let mut draw = |bound: u64| {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (state >> 33) % bound
    };
    let mut committed = Vec::new();
    for step in 0..36 {
        let current = store.snapshot()?;
        let next = format!("eq-{step}");
        let signed = |value: u64| value as i64 - 1;
        // Every kind runs first while both Sources are direct children; a
        // trim may then wrap them in crops, after which some draws refuse.
        let kind = match step {
            0 => 0,
            1 => 3,
            2 => 2,
            3 | 4 => 4,
            5 => 1,
            _ => draw(8),
        };
        let request = match kind {
            0 => Some(edit(
                &current,
                &next,
                Command::RollSources {
                    parent: node("root"),
                    left: node("left"),
                    right: node("right"),
                    delta_frames: if draw(2) == 0 { -1 } else { 1 },
                    left_wrapper: None,
                    right_wrapper: (draw(2) == 0).then(|| node(&format!("{next}-wrap"))),
                    timing: timing(&next),
                },
            )),
            1 => trim(
                &current,
                &next,
                SourceTrimIntent {
                    in_frames: signed(draw(3)),
                    out_frames: signed(draw(3)),
                    slip_frames: signed(draw(3)),
                    roll_frames: signed(draw(3)),
                    policy: if draw(2) == 0 {
                        SourceTrimPolicy::Ripple
                    } else {
                        SourceTrimPolicy::Overwrite
                    },
                },
            ),
            2 => Some(edit(
                &current,
                &next,
                Command::SlipSource {
                    parent: node("root"),
                    node: node(if draw(2) == 0 { "left" } else { "right" }),
                    delta_frames: if draw(2) == 0 { -1 } else { 1 },
                },
            )),
            3 => sound(&current, -(draw(6000) as i32)).map(|event| {
                edit(
                    &current,
                    &next,
                    Command::SetSound {
                        id: SoundId::new("overlay").unwrap(),
                        event,
                    },
                )
            }),
            4 if !current.nodes().contains_key(&node("hold")) => Some(edit(
                &current,
                &next,
                Command::InsertTime {
                    at: ProjectFrame(0),
                    hold: HoldRecipe {
                        duration: FrameDuration::new(3)?,
                        video: HoldVideo::Background,
                        audio: HoldAudio::Silence,
                        picture_context: None,
                    },
                    id: node("hold"),
                    identities: SplitIdentities::default(),
                    timing: timing(&next),
                },
            )),
            4 => audio(&current, "right", true).map(|source| {
                edit(
                    &current,
                    &next,
                    Command::SetHoldAudio {
                        node: node("hold"),
                        audio: if draw(2) == 0 {
                            HoldAudio::RoomTone { source }
                        } else {
                            HoldAudio::Silence
                        },
                    },
                )
            }),
            5 | 6 => {
                let outcome = if draw(2) == 0 {
                    store.undo(current.revision_id(), revision(&next))
                } else {
                    store.redo(current.revision_id(), revision(&next))
                };
                if outcome.is_ok() {
                    committed.push("navigation");
                } else {
                    assert_eq!(store.snapshot()?, current);
                    continue;
                }
                None
            }
            _ => Some(edit(
                &current,
                &next,
                Command::Rename {
                    node: node("right"),
                    label: format!("Right {step}"),
                },
            )),
        };
        if let Some(request) = request {
            match store.commit(&request) {
                Ok(outcome) => {
                    committed.push(match request.command {
                        Command::RollSources { .. } => "roll",
                        Command::ApplySourceTrim { .. } => "trim",
                        Command::SlipSource { .. } => "slip",
                        Command::SetSound { .. } => "sound",
                        Command::InsertTime { .. } => "pause",
                        Command::SetHoldAudio { .. } => "hold audio",
                        _ => "other",
                    });
                    assert_eq!(
                        outcome.edit.forward.apply(&current)?,
                        store.snapshot()?,
                        "{request:?}"
                    );
                }
                Err(_) => {
                    // A refused draw changes nothing.
                    assert_eq!(store.snapshot()?, current);
                    continue;
                }
            }
        } else if store.snapshot()? == current {
            continue;
        }
        let head = store.snapshot()?;
        head.validate()?;
        store.validate_full()?;
        let reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
        assert_eq!(reader.open_validation().replayed, 0);
        assert_eq!(reader.snapshot()?, head);
    }
    for kind in [
        "roll",
        "trim",
        "slip",
        "sound",
        "pause",
        "hold audio",
        "navigation",
    ] {
        assert!(
            committed.contains(&kind),
            "no committed {kind}: {committed:?}"
        );
    }
    Ok(())
}
