//! Native All-Default duration authoring through a fixed Edit Retime.

use super::*;
use deadpan_core::{HoldVideo, ProjectFrame};
use std::path::Path;

pub(in crate::preview::harness) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let workspace = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No fixed-Retime duration workspace")?;
    let previous = workspace.path.clone();
    let baseline = workspace.document.clone();
    let original = d.app().source_cursor;
    let picture = workspace
        .plan
        .picture(ProjectFrame(0))
        .map_err(|error| error.to_string())?;
    let (asset, _) = picture
        .picture
        .follow_point()
        .ok_or("No actual Original picture")?;
    let index = workspace
        .sources
        .get(asset)
        .and_then(|source| source.video_index.as_ref())
        .ok_or("No measured Original index")?;
    let measured = picture
        .picture
        .select_source_frame(index)
        .map_err(|error| error.to_string())?;
    let expected_picture = measured.identity;
    let freeze = HoldVideo::Freeze {
        asset: asset.clone(),
        timestamp: deadpan_core::SourceTimestamp {
            ticks: measured.pts,
            time_base: index.time_base(),
        },
    };
    let scratch = tempfile::tempdir().map_err(|error| error.to_string())?;
    let live = scratch.path().join("shared-fixed-retime.deadpan");
    super::shared_hold_duration::fixture::create_fixed_retime(&previous, &live, freeze)?;
    let result = (|| {
        super::shared_hold_duration::open(d, &live, original)?;
        d.check(
            "The shared-duration fixture adds a fixed unclocked Edit Retime below the inspected Repeat",
            d.app().workspace.as_ref().is_some_and(|workspace| {
                workspace.document.nodes().contains_key(
                    &super::shared_hold_duration::fixture::id("fixed-stage").unwrap(),
                ) && workspace.document.duration().is_ok_and(|duration| duration.frames() == 45)
            }),
            json!({"Retime":"fixed Edit stage","duration_frames":45}),
            d.snapshot(),
        )?;
        fixed_controls(d, expected_picture)?;
        recording_and_delayed(d)?;
        reopen_check(d, &live, original)?;
        Ok(())
    })();
    d.app_mut().feedback.hold_project_updates = false;
    let cleanup = (|| {
        d.app().service.submit(ProjectRequest::Close)?;
        d.wait_for("Close the private fixed-Retime writer", |app| {
            app.workspace.is_none() && !app.service.is_busy()
        })?;
        super::shared_hold_duration::open(d, &previous, original)?;
        resize(d, 1280.0, 820.0)?;
        d.check(
            "Fixed-Retime duration replay restores the original project and cursor",
            super::shared_hold_duration::equivalent(document(d)?, &baseline)
                && d.app().source_cursor == original
                && d.app().scoped.is_none(),
            json!({"project":"restored","Original":original,"inspector":"closed"}),
            d.snapshot(),
        )
    })();
    if cleanup.is_err() {
        let _ = scratch.keep();
    }
    match (result, cleanup) {
        (Err(error), Err(cleanup)) => Err(format!("{error}; fixed-Retime cleanup: {cleanup}")),
        (Err(error), _) | (_, Err(error)) => Err(error),
        _ => Ok(()),
    }
}

fn fixed_controls(d: &mut Driver<'_>, expected_picture: SourceFrameId) -> Result<(), String> {
    resize(d, 960.0, 640.0)?;
    super::shared_hold_duration::choose(d)?;
    let baseline = document(d)?.clone();
    let prior = super::shared_hold_duration::target(d)?;
    let retime = super::shared_hold_duration::fixture::id("fixed-stage")?;
    let held = super::shared_hold_duration::fixture::id("selected")?;
    d.check(
        "The duration editor captures both literal All-Default Repeat ancestors through the fixed Retime",
        prior.target.repeats.len() == 2
            && prior.target.repeats.iter().all(|step| step.branch == deadpan_core::RepeatEditBranch::Default)
            && prior.also.is_empty()
            && prior.presentation.is_some()
            && matches!(baseline.nodes()[&retime].kind, deadpan_core::NodeKind::Retime {
                purpose: deadpan_core::RetimePurpose::Edit, ..
            })
            && d.app().sequence_length() == 45,
        json!({"scope":"All plays at both Repeat levels","Retime":"fixed Edit","output_frames":45}),
        d.snapshot(),
    )?;
    let fixed = baseline.nodes()[&retime].clone();
    let revision = d.revision();
    d.key(Key::Enter)?;
    d.check(
        "Enter prefills the actual shared Hold duration without a write",
        d.app().command_open && d.app().command == "hold-duration 2f" && d.revision() == revision,
        json!({"prefill":"hold-duration 2f","history":"unchanged"}),
        d.snapshot(),
    )?;
    d.capture("Fixed-Retime All plays duration prefill")?;
    super::shared_hold_duration::submit(d, "hold-duration 4f")?;
    d.changed(&revision)?;
    super::shared_hold_duration::settled(d)?;
    let grown = document(d)?.clone();
    d.check(
        "Growing the shared Hold preserves the fixed-stage clock and All-Default authority",
        grown.duration().is_ok_and(|duration| duration.frames() == 45)
            && grown.nodes()[&retime] == fixed
            && matches!(grown.nodes()[&held].kind, deadpan_core::NodeKind::Hold { ref recipe }
                if recipe.duration.frames() == 4)
            && grown.overrides() == baseline.overrides()
            && super::shared_hold_duration::target(d)?.target == prior.target
            && d.app().presentation.displayed_source_frame() == Some(expected_picture),
        json!({"hold_frames":4,"fixed_output_frames":45,"overrides":"unchanged","picture":expected_picture}),
        d.snapshot(),
    )?;
    let revision = d.revision();
    d.command("hold-duration 4f")?;
    super::shared_hold_duration::settled(d)?;
    d.check(
        "Repeating the same duration is a no-op with no history entry",
        document(d)? == &grown && d.revision() == revision,
        json!({"history":"unchanged","duration":4}),
        d.snapshot(),
    )?;
    d.key(Key::U)?;
    d.changed(&revision)?;
    super::shared_hold_duration::settled(d)?;
    d.check(
        "Undo restores the baseline shared duration without changing the fixed Retime",
        super::shared_hold_duration::equivalent(document(d)?, &baseline)
            && document(d)?.nodes()[&retime] == fixed,
        json!({"hold_frames":2,"output_frames":45}),
        d.snapshot(),
    )?;
    let revision = d.revision();
    d.key_modified(Key::R, egui::Modifiers::CTRL)?;
    d.changed(&revision)?;
    super::shared_hold_duration::settled(d)?;
    d.check(
        "Redo restores the shared duration and retains its fixed output clock",
        super::shared_hold_duration::equivalent(document(d)?, &grown)
            && document(d)?.nodes()[&retime] == fixed,
        json!({"hold_frames":4,"output_frames":45}),
        d.snapshot(),
    )?;
    super::shared_hold_duration::choose(d)?;
    scroll_top(d)?;
    d.click("Change duration…  Enter")?;
    let revision = d.revision();
    super::shared_hold_duration::submit(d, "hold-duration 3f")?;
    d.changed(&revision)?;
    super::shared_hold_duration::settled(d)?;
    let shortened = document(d)?.clone();
    d.check(
        "Shortening the Hold also leaves the fixed output unchanged",
        shortened
            .duration()
            .is_ok_and(|duration| duration.frames() == 45)
            && shortened.nodes()[&retime] == fixed
            && matches!(shortened.nodes()[&held].kind, deadpan_core::NodeKind::Hold { ref recipe }
                if recipe.duration.frames() == 3),
        json!({"hold_frames":3,"output_frames":45}),
        d.snapshot(),
    )?;
    let revision = d.revision();
    d.key(Key::U)?;
    d.changed(&revision)?;
    super::shared_hold_duration::settled(d)?;
    let after_undo = document(d)?.clone();
    super::shared_hold_duration::choose(d)?;
    let revision = d.revision();
    d.key(Key::Period)?;
    d.changed(&revision)?;
    super::shared_hold_duration::settled(d)?;
    let actual = serde_json::to_value(document(d)?).map_err(|error| error.to_string())?;
    let mut expected = serde_json::to_value(&shortened).map_err(|error| error.to_string())?;
    expected["revision_id"] = actual["revision_id"].clone();
    let differences: Vec<_> = actual
        .as_object()
        .unwrap()
        .iter()
        .filter_map(|(key, value)| (expected[key] != *value).then_some(key.clone()))
        .collect();
    d.check(
        "Dot repeats the prior duration edit against the retained shared target",
        differences.iter().all(|field| field == "audio_bindings")
            && same_audio_clock_intent(document(d)?.audio_bindings(), shortened.audio_bindings())
            && document(d)?.nodes()[&retime] == fixed
            && super::shared_hold_duration::target(d)?.target == prior.target,
        json!({"dot":"hold-duration 3f","output_frames":45,
            "different_document_fields":differences,
            "prior_audio_bindings":shortened.audio_bindings(),
            "current_audio_bindings":document(d)?.audio_bindings()}),
        d.snapshot(),
    )?;
    let revision = d.revision();
    d.key(Key::U)?;
    d.changed(&revision)?;
    super::shared_hold_duration::settled(d)?;
    d.check(
        "Undo after dot restores the immediately preceding four-frame state",
        super::shared_hold_duration::equivalent(document(d)?, &after_undo),
        json!({"hold_frames":4,"output_frames":45}),
        d.snapshot(),
    )?;
    d.capture("Shared duration history under fixed Edit Retime")?;
    Ok(())
}

fn recording_and_delayed(d: &mut Driver<'_>) -> Result<(), String> {
    use crate::preview::copied::Content;
    use deadpan_core::{PauseLength, SemanticInstruction};
    use std::num::NonZeroU32;

    super::shared_hold_duration::choose(d)?;
    let baseline = document(d)?.clone();
    let prior = super::shared_hold_duration::target(d)?;
    let held = super::shared_hold_duration::fixture::id("selected")?;
    let retime = super::shared_hold_duration::fixture::id("fixed-stage")?;
    let revision = d.revision();
    d.chord(&[Key::Q, Key::W])?;
    d.command("hold-duration 3f")?;
    d.changed(&revision)?;
    super::shared_hold_duration::settled(d)?;
    d.key(Key::Q)?;
    super::shared_hold_duration::settled(d)?;
    let intent = SemanticInstruction::SetHoldDuration {
        length: PauseLength::Frames {
            frames: NonZeroU32::new(3).unwrap(),
        },
    };
    let saved = d
        .app()
        .copied
        .entries()
        .find_map(|(name, value)| match value {
            Content::Macro(program) if name == 'w' => Some(program.instructions().to_vec()),
            _ => None,
        });
    d.check(
        "Recording beneath a fixed Retime saves duration intent without captured identities",
        saved == Some(vec![intent.clone()])
            && d.app().sequence_length() == 45
            && document(d)?.nodes()[&retime] == baseline.nodes()[&retime]
            && super::shared_hold_duration::target(d)?.target == prior.target,
        json!({"saved_w":[intent],"output_frames":45}),
        d.snapshot(),
    )?;
    let revision = d.revision();
    d.key(Key::U)?;
    d.changed(&revision)?;
    super::shared_hold_duration::settled(d)?;
    super::shared_hold_duration::choose(d)?;
    let revision = d.revision();
    super::super::macros::execute(d, 'w', None)?;
    d.changed(&revision)?;
    super::shared_hold_duration::settled(d)?;
    d.check(
        "A named duration Run resolves the current fixed-window All plays target",
        d.app().sequence_length() == 45
            && document(d)?.nodes()[&retime] == baseline.nodes()[&retime]
            && matches!(&document(d)?.nodes()[&held].kind, NodeKind::Hold { recipe }
                if recipe.duration.frames() == 3)
            && super::shared_hold_duration::target(d)?.target == prior.target,
        json!({"run":"w","hold_frames":3,"output_frames":45}),
        d.snapshot(),
    )?;
    let revision = d.revision();
    d.key(Key::U)?;
    d.changed(&revision)?;
    super::shared_hold_duration::settled(d)?;
    d.check(
        "One Undo restores the complete pre-Run fixed-window document",
        super::shared_hold_duration::equivalent(document(d)?, &baseline),
        json!({"hold_frames":4,"output_frames":45}),
        d.snapshot(),
    )?;

    super::shared_hold_duration::choose(d)?;
    let revision = d.revision();
    let original = d.app().source_cursor;
    d.app_mut().feedback.hold_project_updates = true;
    let result = (|| {
        d.command("hold-duration 3f")?;
        d.wait_for(
            "Fixed-window duration is durable before newer navigation",
            |app| !app.service.is_busy(),
        )?;
        let captured = super::shared_hold_duration::target(d)?;
        d.chord(&[Key::K, Key::J, Key::L])?;
        let current = super::shared_hold_duration::target(d)?;
        let cursor = d.app().sequence_cursor;
        d.check(
            "Away/back navigation revokes the earlier fixed-window continuation",
            captured.target == current.target && captured.navigation != current.navigation,
            json!({"same_Hold":true,"navigation":"new generation"}),
            d.snapshot(),
        )?;
        d.app_mut().feedback.hold_project_updates = false;
        d.changed(&revision)?;
        super::shared_hold_duration::settled(d)?;
        d.check(
            "A delayed fixed-window duration stays saved without reclaiming newer navigation",
            d.app().sequence_length() == 45
                && d.app().sequence_cursor == cursor
                && d.app().source_cursor == original
                && d.app().scoped.is_none()
                && document(d)?.nodes()[&retime] == baseline.nodes()[&retime]
                && matches!(&document(d)?.nodes()[&held].kind, NodeKind::Hold { recipe }
                    if recipe.duration.frames() == 3),
            json!({"hold_frames":3,"output_frames":45,"Edit":cursor,"continuation":"revoked"}),
            d.snapshot(),
        )?;
        let revision = d.revision();
        d.key(Key::U)?;
        d.changed(&revision)?;
        super::shared_hold_duration::settled(d)?;
        d.check(
            "Undo after delayed delivery restores the fixed-window document",
            super::shared_hold_duration::equivalent(document(d)?, &baseline),
            json!({"hold_frames":4,"output_frames":45}),
            d.snapshot(),
        )
    })();
    d.app_mut().feedback.hold_project_updates = false;
    result
}

fn reopen_check(d: &mut Driver<'_>, path: &Path, original: u64) -> Result<(), String> {
    d.app().service.submit(ProjectRequest::Close)?;
    d.wait_for("Close the fixed-Retime writer before reopening", |app| {
        app.workspace.is_none() && !app.service.is_busy()
    })?;
    super::shared_hold_duration::open(d, path, original)?;
    resize(d, 960.0, 640.0)?;
    super::shared_hold_duration::choose(d)?;
    let target = super::shared_hold_duration::target(d)?;
    let held = super::shared_hold_duration::fixture::id("selected")?;
    d.check(
        "Reopening preserves the committed shared duration and captured All-Default path",
        d.app().sequence_length() == 45
            && target.target.repeats.len() == 2
            && target.target.repeats.iter().all(|step| step.branch == deadpan_core::RepeatEditBranch::Default)
            && matches!(document(d)?.nodes()[&held].kind, deadpan_core::NodeKind::Hold { ref recipe }
                if recipe.duration.frames() == 4),
        json!({"hold_frames":4,"output_frames":45,"reopened":true}),
        d.snapshot(),
    )
}

fn scroll_top(d: &mut Driver<'_>) -> Result<(), String> {
    let point = d.rect("Nested beat inspector pane")?.center() + egui::vec2(0.0, 120.0);
    d.events(
        "Restore fixed-Retime inspector controls",
        vec![
            egui::Event::PointerMoved(point),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, 10_000.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    )?;
    for _ in 0..8 {
        d.step("Fixed-Retime inspector scroll settles", false)?;
    }
    Ok(())
}

/// Parameter invocations allocate fresh timing IDs. Compare every frozen
/// layout and placement exactly after a bijection of those IDs alone.
fn same_audio_clock_intent(
    actual: &deadpan_core::AudioBindingState,
    expected: &deadpan_core::AudioBindingState,
) -> bool {
    use std::collections::{BTreeMap, BTreeSet};
    if actual.timings().len() != expected.timings().len() {
        return false;
    }
    let mut mapping = BTreeMap::new();
    let mut used = BTreeSet::new();
    for (id, layout) in actual.timings() {
        let candidate = expected
            .timings()
            .get_key_value(id)
            .filter(|(key, value)| !used.contains(*key) && *value == layout)
            .or_else(|| {
                expected
                    .timings()
                    .iter()
                    .find(|(key, value)| !used.contains(*key) && *value == layout)
            });
        let Some((candidate, _)) = candidate else {
            return false;
        };
        mapping.insert(id.clone(), candidate.clone());
        used.insert(candidate.clone());
    }
    let remap = |bindings: &BTreeMap<_, deadpan_core::OwnedAudioBinding>| {
        let mut result = bindings.clone();
        for binding in result.values_mut() {
            let placements = std::iter::once(&mut binding.lattice)
                .chain(binding.resume.iter_mut().flat_map(|resume| {
                    resume
                        .phase
                        .terms
                        .iter_mut()
                        .map(|term| &mut term.placement)
                }))
                .chain(binding.reanchors.iter_mut().map(|step| &mut step.placement));
            for placement in placements {
                placement.reference.timing = mapping.get(&placement.reference.timing)?.clone();
            }
        }
        Some(result)
    };
    remap(actual.bindings()).as_ref() == Some(expected.bindings())
        && remap(actual.gap_bindings()).as_ref() == Some(expected.gap_bindings())
        && actual.sound_clocks() == expected.sound_clocks()
}
