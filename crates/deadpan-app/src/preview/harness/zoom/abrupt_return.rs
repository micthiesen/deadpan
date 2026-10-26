//! Chosen-boundary return from an already moving path through real native keys.
//!
//! Split retains the full framed context below neutral Partition wrappers.
//! Entering the right fragment and resetting its copied owner removes that
//! path without sampling or changing the left fragment's animation.

use super::*;
use deadpan_core::{
    AudioSample, Command, CommandRequest, FollowScaleTransition, FramingClock, FramingCurve,
    NodeId, ProjectDocument, RevisionId, SourceSpan, SourceTimestamp, TargetCorrection, TargetId,
    TargetPoint, TargetSample, TrackState, framing_zoom::ZoomCurve,
};
use deadpan_store::{AccessMode, ProjectStore};

const BEATS: &str = "Current group beat outline pane";
const SEAM: u64 = 80;

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.key(Key::Escape)?;
    d.command("sequence")?;
    d.click(BEATS)?;
    at(d, 0)?;
    let original = document(d)?.clone();
    let owner = d.app().selected_beat.clone().ok_or("No return owner")?;
    d.check(
        "Abrupt-return witness starts with the unsplit 120-picture Original owner",
        d.app().sequence_length() == 120
            && matches!(original.nodes()[&owner].kind, NodeKind::Source { .. })
            && d.app().scoped.is_none()
            && d.app().sequence_scope.groups().is_empty(),
        json!({"frames":120,"owner":"Source","scope":"ordinary"}),
        d.snapshot(),
    )?;
    for follow in [false, true] {
        let setup_count = fixture(d, &owner, follow)?;
        let seeded = document(d)?.clone();
        chosen_boundary(d, &owner, follow)?;
        for _ in 0..setup_count {
            undo(d)?;
        }
        d.check(
            "Undoing the return witness setup restores the complete original document",
            equivalent(document(d)?, &original),
            json!("all authored fields restored"),
            d.snapshot(),
        )?;
        // Keep the seed identity in the report even though both product edits
        // and all typed fixture setup are now undone.
        d.check(
            "The exercised return started from an authored retained-clock path",
            seeded.nodes()[&owner]
                .framing
                .as_ref()
                .is_some_and(|framing| {
                    framing.clock == retained_clock()
                        && matches!(
                            framing.value,
                            FramingValue::Envelope { .. }
                                | FramingValue::Follow {
                                    scale_transition: Some(_),
                                    ..
                                }
                        )
                }),
            json!({"offset":"15/2","duration":180,"animated":true}),
            json!(seeded.nodes()[&owner].framing),
        )?;
    }
    at(d, 0)
}

fn retained_clock() -> FramingClock {
    FramingClock::RetainedOutput {
        offset: ExactRatio::new(15, 2).unwrap(),
        duration: ExactRatio::integer(180),
    }
}

fn fixture(d: &mut Driver<'_>, owner: &NodeId, follow: bool) -> Result<usize, String> {
    let workspace = d.app().workspace.clone().ok_or("No return workspace")?;
    let path = workspace.path.clone();
    let label = if follow { "follow" } else { "envelope" };
    let target_id = TargetId::new("abrupt-return-target").map_err(string)?;
    let mut target = workspace
        .document
        .targets()
        .values()
        .next()
        .cloned()
        .ok_or("The zoom scenario has no saved target template")?;
    let index = workspace
        .sources
        .get(&target.asset)
        .and_then(|source| source.video_index.as_ref())
        .ok_or("No return Original index")?;
    let point = |x| {
        TargetPoint {
            center: [x, 500_000],
        }
        .into()
    };
    target.label = "Abrupt return moving point".into();
    target.region = point(400_000);
    target.span = SourceSpan::new(
        target.span.start(),
        SourceTimestamp {
            ticks: index.frames()[110].pts,
            time_base: target.span.start().time_base,
        },
    )
    .map_err(string)?;
    target.samples = [
        (10, 400_000, TrackState::Tracked),
        (30, 650_000, TrackState::Tracked),
        (40, 650_000, TrackState::Lost),
        (60, 300_000, TrackState::Tracked),
    ]
    .into_iter()
    .map(|(ordinal, x, state)| TargetSample {
        at: index.frames()[ordinal].pts,
        region: point(x),
        confidence: 900,
        state,
    })
    .collect();
    target.corrections = vec![TargetCorrection {
        at: index.frames()[50].pts,
        region: point(550_000),
    }];
    target.provenance = None;
    let mut framing = if follow {
        Framing {
            clock: retained_clock(),
            value: FramingValue::Follow {
                target: target_id.clone(),
                scale: ExactRatio::new(3, 2).unwrap(),
                fallback: FramingPose::new(
                    ExactRatio::new(2, 5).unwrap(),
                    ExactRatio::new(1, 2).unwrap(),
                    ExactRatio::new(5, 4).unwrap(),
                )
                .map_err(string)?,
                scale_transition: Some(FollowScaleTransition {
                    factor: ExactRatio::integer(2),
                    curve: ZoomCurve::Smoothstep,
                    range: None,
                }),
            },
        }
    } else {
        Framing::creep(
            FramingPose::new(
                ExactRatio::new(2, 5).unwrap(),
                ExactRatio::new(1, 2).unwrap(),
                ExactRatio::new(5, 4).unwrap(),
            )
            .map_err(string)?,
            FramingPose::new(
                ExactRatio::new(3, 5).unwrap(),
                ExactRatio::new(3, 5).unwrap(),
                ExactRatio::integer(2),
            )
            .map_err(string)?,
            FramingCurve::Smoothstep,
        )
        .map_err(string)?
    };
    framing.clock = retained_clock();
    let ancestor = Framing::static_pose(FramingPose {
        scale: ExactRatio::new(11, 10).unwrap(),
        ..FramingPose::identity()
    })
    .map_err(string)?;
    close(d)?;
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite).map_err(string)?;
    if follow {
        apply(
            &mut store,
            &format!("abrupt-{label}-target"),
            Command::SetTarget {
                id: target_id,
                target,
            },
        )?;
    }
    apply(
        &mut store,
        &format!("abrupt-{label}-path"),
        Command::SetFraming {
            node: owner.clone(),
            framing: Some(framing),
        },
    )?;
    apply(
        &mut store,
        &format!("abrupt-{label}-ancestor"),
        Command::SetFraming {
            node: workspace.document.root().clone(),
            framing: Some(ancestor),
        },
    )?;
    drop(store);
    d.app().service.submit(ProjectRequest::Open(path.clone()))?;
    d.wait_for(
        "Reopen the retained return fixture through the project service",
        |app| {
            !app.service.is_busy()
                && app
                    .workspace
                    .as_ref()
                    .is_some_and(|workspace| workspace.path == path)
        },
    )?;
    d.command("sequence")?;
    d.click(BEATS)?;
    at(d, SEAM)?;
    Ok(if follow { 3 } else { 2 })
}

fn chosen_boundary(d: &mut Driver<'_>, owner: &NodeId, follow: bool) -> Result<(), String> {
    let label = if follow {
        "live animated Follow"
    } else {
        "retained Smoothstep path"
    };
    let baseline = d
        .app()
        .workspace
        .clone()
        .ok_or("No seeded return workspace")?;
    let before = views(&baseline.plan)?;
    if follow {
        let poses = [15, 30, 45, 55, 80, 115]
            .into_iter()
            .map(|frame| {
                baseline
                    .plan
                    .picture(ProjectFrame(frame))
                    .map_err(string)?
                    .framing
                    .iter()
                    .find(|layer| layer.instance.node == *owner)
                    .and_then(|layer| layer.pose)
                    .ok_or("The moving return owner has no posed layer".to_owned())
            })
            .collect::<Result<Vec<_>, String>>()?;
        d.check(
            "The return fixture actually changes its live target center, holds loss, applies correction and reaches fallback",
            poses[0].center_x != poses[1].center_x
                && poses[1].center_x == poses[2].center_x
                && poses[2].center_x != poses[3].center_x
                && poses[4].center_x != ExactRatio::new(2, 5).unwrap()
                && poses[5].center_x == ExactRatio::new(2, 5).unwrap(),
            json!({"moving":true,"Lost":"held","correction":"changed","live_at_seam":true,"fallback_x":"2/5"}),
            json!(poses),
        )?;
    }
    let source_cursor = d.app().source_cursor;
    let revision = d.revision();
    d.key(Key::S)?;
    d.changed(&revision)?;
    idle(d)?;
    let right = d
        .app()
        .selected_beat
        .clone()
        .ok_or("Split did not select its right fragment")?;
    let copied = match &document(d)?.nodes()[&right].kind {
        NodeKind::Retime {
            child,
            purpose: deadpan_core::RetimePurpose::Partition,
            ..
        } => child.clone(),
        _ => return Err("Split return fragment is not a Partition".into()),
    };
    d.check(
        &format!(
            "Splitting {label} preserves all 120 posed pictures and the copied full-context recipe"
        ),
        views(&d.app().workspace.as_ref().unwrap().plan)? == before
            && document(d)?.nodes()[&copied].framing == baseline.document.nodes()[owner].framing
            && document(d)?.nodes()[&right].framing.is_none()
            && d.app().sequence_length() == 120,
        json!({"pictures":120,"copied_recipe":"exact","wrapper":"unframed","frames":120}),
        d.snapshot(),
    )?;
    let split = d.app().workspace.clone().ok_or("No split workspace")?;
    let audio_before = audio(&split.plan)?;
    d.key(Key::Enter)?;
    idle(d)?;
    d.check(
        "Enter reaches the right fragment's copied framing owner without writing",
        d.app()
            .scoped
            .as_ref()
            .is_some_and(|state| state.selected_target().node == copied)
            && d.app().sequence_cursor == SEAM
            && document(d)? == split.document.as_ref(),
        json!({"copied_owner":copied,"Edit_frame":SEAM,"write":false}),
        d.snapshot(),
    )?;
    camera(d)?;
    d.key(Key::R)?;
    d.settled()?;
    let draft = d
        .app()
        .camera
        .as_ref()
        .ok_or("Return Camera closed")?
        .harness_state();
    d.check(
        &format!(
            "Reset previews the abrupt return from {label} without changing the committed path"
        ),
        draft["framing"].is_null() && document(d)? == split.document.as_ref(),
        json!({"draft_framing":null,"committed":"unchanged"}),
        draft,
    )?;
    d.capture(&format!(
        "Abrupt return from {label}: right-fragment Camera reset draft"
    ))?;
    d.key(Key::Escape)?;
    idle(d)?;
    d.check(
        "Escape cancels the chosen-boundary return and restores every authored field",
        document(d)? == split.document.as_ref() && views(&split.plan)? == before,
        json!("complete split document retained"),
        d.snapshot(),
    )?;
    camera(d)?;
    d.chord(&[Key::R, Key::Enter])?;
    d.changed(split.document.revision_id().as_str())?;
    idle(d)?;
    let committed = d
        .app()
        .workspace
        .clone()
        .ok_or("No saved return workspace")?;
    let mut expected = serde_json::to_value(split.document.as_ref()).map_err(string)?;
    expected["nodes"][copied.as_str()]
        .as_object_mut()
        .ok_or("The copied return owner is not a serialized node")?
        .remove("framing");
    let mut actual = serde_json::to_value(committed.document.as_ref()).map_err(string)?;
    actual["revision_id"] = json!(split.document.revision_id());
    let after = views(&committed.plan)?;
    let root = split.document.root();
    let seam = usize::try_from(SEAM).map_err(string)?;
    let root_once = (0..120).all(|frame| {
        committed
            .plan
            .picture(ProjectFrame(frame))
            .is_ok_and(|sample| {
                sample
                    .framing
                    .iter()
                    .filter(|layer| layer.instance.node == *root)
                    .count()
                    == 1
            })
    });
    d.check(
        &format!("One Camera save removes only the copied {label}, preserves every pre-seam picture and keeps ancestor framing once"),
        actual == expected
            && after[..seam] == before[..seam]
            && after[seam..].iter().all(|view| view["poses"].as_array().is_some_and(|poses| poses.len() == 1))
            && after.iter().zip(&before).all(|(after, before)| after["picture"] == before["picture"] && after["context"] == before["context"] && after["captions"] == before["captions"])
            && root_once && audio(&committed.plan)? == audio_before
            && d.app().sequence_length() == 120 && d.app().source_cursor == source_cursor,
        json!({"only_changed":"right copied owner framing","preseam_pictures":SEAM,"ancestor_layers":1,"audio_plan":"unchanged","frames":120}),
        json!({"document_exact":actual==expected,"prefix_exact":after[..seam]==before[..seam],"root_once":root_once,"audio_exact":audio(&committed.plan)?==audio_before,"frames":d.app().sequence_length(),"state":d.snapshot()}),
    )?;
    d.capture(&format!(
        "Saved abrupt return from {label} retains the chosen split boundary"
    ))?;
    d.key(Key::Backspace)?;
    idle(d)?;
    d.check(
        "Backspace returns to the right fragment after the Camera save",
        d.app().scoped.is_none() && d.app().selected_beat.as_ref() == Some(&right),
        json!({"selected_fragment":right,"nested":false}),
        d.snapshot(),
    )?;
    undo(d)?;
    d.check(
        "One Undo restores the complete moving framing after the chosen-boundary return",
        equivalent(document(d)?, split.document.as_ref())
            && views(&d.app().workspace.as_ref().unwrap().plan)? == before,
        json!("complete split path restored"),
        d.snapshot(),
    )?;
    let revision = d.revision();
    d.key_modified(Key::R, egui::Modifiers::CTRL)?;
    d.changed(&revision)?;
    idle(d)?;
    d.check(
        "Redo restores only the saved right-fragment framing removal",
        equivalent(document(d)?, committed.document.as_ref())
            && views(&d.app().workspace.as_ref().unwrap().plan)? == after,
        json!("exact return restored"),
        d.snapshot(),
    )?;
    undo(d)?;
    undo(d)?;
    d.check(
        "Undoing the reset and Split restores the complete original moving owner and its clocks",
        equivalent(document(d)?, baseline.document.as_ref())
            && views(&d.app().workspace.as_ref().unwrap().plan)? == before,
        json!("unsplit animated owner restored"),
        d.snapshot(),
    )
}

fn views(plan: &deadpan_plan::RenderPlan) -> Result<Vec<Value>, String> {
    (0..plan.duration().frames()).map(|frame| {
        let picture = plan.picture(ProjectFrame(frame)).map_err(string)?;
        Ok(json!({"picture":picture.picture,"context":picture.picture_context.as_deref(),
            "poses":picture.framing.iter().filter_map(|layer| layer.pose.map(|pose| json!({"pose":pose,"local":layer.local_position,"duration":layer.effective_duration,"escalation":layer.escalation}))).collect::<Vec<_>>(),
            "captions":picture.captions}))
    }).collect()
}

fn audio(plan: &deadpan_plan::RenderPlan) -> Result<Value, String> {
    let end = plan.audio_duration().map_err(string)?;
    Ok(json!(
        plan.audio(AudioSample(0)..end, Default::default())
            .map_err(string)?
            .spans
    ))
}

fn document<'a>(d: &'a Driver<'_>) -> Result<&'a ProjectDocument, String> {
    d.app()
        .workspace
        .as_ref()
        .map(|workspace| workspace.document.as_ref())
        .ok_or("No return document".into())
}

fn equivalent(actual: &ProjectDocument, expected: &ProjectDocument) -> bool {
    let mut actual = serde_json::to_value(actual).unwrap();
    actual["revision_id"] = json!(expected.revision_id());
    actual == json!(expected)
}

fn camera(d: &mut Driver<'_>) -> Result<(), String> {
    d.chord(&[Key::Comma, Key::F])?;
    d.wait_for("Camera opens on the copied right-fragment owner", |app| {
        app.camera.is_some()
    })?;
    d.settled()
}

fn at(d: &mut Driver<'_>, frame: u64) -> Result<(), String> {
    d.chord(&[Key::G, Key::G])?;
    if frame > 0 {
        for digit in frame.to_string().chars() {
            d.key(Key::from_name(&digit.to_string()).ok_or("Invalid return frame digit")?)?;
        }
        d.key(Key::L)?;
    }
    d.settled()
}

fn idle(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Abrupt return receipt finishes", |app| {
        !app.service.is_busy() && !app.macros.is_pending() && app.camera_pending.is_none()
    })?;
    d.settled()
}

fn undo(d: &mut Driver<'_>) -> Result<(), String> {
    let revision = d.revision();
    d.key(Key::U)?;
    d.changed(&revision)?;
    idle(d)
}

fn close(d: &mut Driver<'_>) -> Result<(), String> {
    d.app().service.submit(ProjectRequest::Close)?;
    d.wait_for(
        "Close the native writer before typed return fixture setup",
        |app| app.workspace.is_none() && !app.service.is_busy(),
    )
}

fn apply(store: &mut ProjectStore, revision: &str, command: Command) -> Result<(), String> {
    let document = store.snapshot().map_err(string)?;
    store
        .commit(&CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: RevisionId::new(revision).map_err(string)?,
            command,
        })
        .map_err(string)?;
    Ok(())
}

fn string(error: impl std::fmt::Display) -> String {
    error.to_string()
}
