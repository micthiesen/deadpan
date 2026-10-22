//! Native selected-play Hold duration through one fixed Edit Retime.
use super::nested_fixed_duration::fixture;
use super::*;
use deadpan_core::{
    HoldAudio, HoldVideo, NodeKind, ProjectFrame, SemanticSelectedHoldDurationContext,
    SourceTimestamp,
};
use egui::Key;
use std::{num::NonZeroU32, path::Path};

#[derive(Clone, Copy, Debug)]
enum Selection {
    InnerDefault,
    InnerPlay,
    OuterDefault,
    OuterPlay,
}
impl Selection {
    fn name(self) -> &'static str {
        match self {
            Self::InnerDefault => "Inner selected under All plays",
            Self::InnerPlay => "Inner selected under This play",
            Self::OuterDefault => "Outer selected with inner All plays",
            Self::OuterPlay => "Outer selected with inner This play",
        }
    }
    fn affects(self, outer: i64, inner: i64) -> bool {
        match self {
            Self::InnerDefault => true,
            Self::InnerPlay => outer == 1,
            Self::OuterDefault => outer < 2,
            Self::OuterPlay => outer < 2 && inner == 1,
        }
    }
    fn fresh_pool(self) -> usize {
        match self {
            Self::InnerDefault => 10,
            Self::InnerPlay => 16,
            Self::OuterDefault => 12,
            Self::OuterPlay => 22,
        }
    }
    fn overrides(self) -> usize {
        match self {
            Self::InnerDefault | Self::OuterDefault => 2,
            Self::InnerPlay => 3,
            Self::OuterPlay => 4,
        }
    }
}

pub(in crate::preview::harness) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let workspace = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No selected fixed-duration workspace")?;
    let previous = workspace.path.clone();
    let baseline = workspace.document.clone();
    let original = d.app().source_cursor;
    let edit_cursor = d.app().sequence_cursor;
    let picture = workspace
        .plan
        .picture(ProjectFrame(0))
        .map_err(|e| e.to_string())?;
    let (asset, _) = picture
        .picture
        .follow_point()
        .ok_or("No actual Original picture")?;
    let index = workspace
        .sources
        .get(asset)
        .and_then(|s| s.video_index.as_ref())
        .ok_or("No measured Original index")?;
    let measured = picture
        .picture
        .select_source_frame(index)
        .map_err(|e| e.to_string())?;
    let freeze = HoldVideo::Freeze {
        asset: asset.clone(),
        timestamp: SourceTimestamp {
            ticks: measured.pts,
            time_base: index.time_base(),
        },
    };
    let scratch = tempfile::tempdir().map_err(|e| e.to_string())?;
    let result = (|| {
        for (ordinal, selection) in [
            Selection::InnerDefault,
            Selection::InnerPlay,
            Selection::OuterDefault,
            Selection::OuterPlay,
        ]
        .into_iter()
        .enumerate()
        {
            let live = scratch
                .path()
                .join(format!("selected-fixed-{ordinal}.deadpan"));
            fixture::create(&previous, &live, freeze.clone())?;
            super::shared_hold_duration::open(d, &live, original)?;
            fixture::assert_fixture(d)?;
            controls(d, selection)?;
            macros_and_delayed(d, selection)?;
            let committed = document(d)?.clone();
            reopen(d, &live, original, selection, &committed)?;
            d.app().service.submit(ProjectRequest::Close)?;
            d.wait_for("Close completed selected fixed-duration project", |app| {
                app.workspace.is_none() && !app.service.is_busy()
            })?;
        }
        Ok(())
    })();
    d.app_mut().feedback.hold_project_updates = false;
    let cleanup = (|| {
        if d.app().workspace.is_some() {
            d.app().service.submit(ProjectRequest::Close)?;
            d.wait_for("Close private selected fixed-duration writer", |app| {
                app.workspace.is_none() && !app.service.is_busy()
            })?;
        }
        super::shared_hold_duration::open(d, &previous, original)?;
        if edit_cursor > 0 {
            let mut keys = edit_cursor
                .to_string()
                .chars()
                .map(|digit| {
                    Key::from_name(&digit.to_string()).ok_or("Invalid Edit cursor digit".to_owned())
                })
                .collect::<Result<Vec<_>, _>>()?;
            keys.push(Key::L);
            d.chord(&keys)?;
        }
        super::shared_hold_duration::settled(d)?;
        resize(d, 1280.0, 820.0)?;
        d.check(
            "Selected fixed-duration replay restores the original project and cursor",
            super::shared_hold_duration::equivalent(document(d)?, &baseline)
                && d.app().source_cursor == original
                && d.app().sequence_cursor == edit_cursor
                && d.app().scoped.is_none(),
            json!({"project":"restored","Original":original,"Edit":edit_cursor}),
            d.snapshot(),
        )
    })();
    if cleanup.is_err() {
        let _ = scratch.keep();
    }
    match (result, cleanup) {
        (Err(error), Err(cleanup)) => Err(format!(
            "{error}; selected fixed-duration cleanup: {cleanup}"
        )),
        (Err(error), _) | (_, Err(error)) => Err(error),
        _ => Ok(()),
    }
}

fn authority(d: &Driver<'_>) -> Result<SemanticSelectedHoldDurationContext, String> {
    let target = super::shared_hold_duration::target(d)?;
    Ok(SemanticSelectedHoldDurationContext {
        root: fixture::id("outer")?,
        target: target.target,
        presentation: target.presentation,
        also: target.also,
    })
}

fn choose(d: &mut Driver<'_>, selection: Selection) -> Result<(), String> {
    if d.app().scoped.is_some() {
        d.key(Key::Escape)?;
    }
    d.command("sequence")?;
    super::super::transcript::focus_your_edit(d)?;
    d.chord(&[Key::G, Key::G, Key::Num2, Key::Num2, Key::L, Key::Enter])?;
    d.command(match selection {
        Selection::InnerDefault => "scope all",
        Selection::InnerPlay => "scope play 2",
        Selection::OuterDefault | Selection::OuterPlay => "scope plays 1,2",
    })?;
    d.key(Key::Enter)?;
    d.command(match selection {
        Selection::InnerDefault | Selection::InnerPlay => "scope plays 1,2",
        Selection::OuterDefault => "scope all",
        Selection::OuterPlay => "scope play 2",
    })?;
    d.chord(&[Key::Enter, Key::Enter, Key::J])?;
    super::shared_hold_duration::settled(d)?;
    scroll_top(d)?;
    let target = super::shared_hold_duration::target(d)?;
    let visible = document(d)?
        .selected_hold_duration_visibility(document(d)?.root(), &authority(d)?)
        .map_err(|e| e.to_string())?
        .ok_or("No primary selected picture")?;
    if target.also.len() != 1
        || target.presentation.is_none()
        || target.cursor != visible.frames.start()
        || !matches!(&document(d)?.nodes()[&target.target.node].kind,
            NodeKind::Hold { recipe } if matches!(recipe.video, HoldVideo::Freeze { .. }))
    {
        return Err(format!(
            "{} did not select both exact Hold counterparts",
            selection.name()
        ));
    }
    Ok(())
}

fn controls(d: &mut Driver<'_>, selection: Selection) -> Result<(), String> {
    resize(d, 960.0, 640.0)?;
    choose(d, selection)?;
    let baseline = document(d)?.clone();
    let prior = super::shared_hold_duration::target(d)?;
    let original = d.app().source_cursor;
    let displayed = d.app().presentation.displayed_source_frame();
    let fixed = baseline.nodes()[&fixture::id("stage")?].clone();
    let cursor = prior.cursor;
    let hint = scenarios::text_paint_visibility(d, "Changes apply to the selected plays.");
    d.check(
        "Selected fixed-duration capture retains both literal targets and paints its exact scope",
        prior.also.len() == 1
            && prior.presentation.is_some()
            && prior.target.node == prior.also[0].node
            && !hint.is_empty()
            && hint.iter().all(|text| text["fully_visible"] == true)
            && d.app().sequence_length() == 42,
        json!({"selection":selection.name(),"targets":2,"Edit":cursor.0,"output_frames":42}),
        json!({"snapshot":d.snapshot(),"paint":hint}),
    )?;
    let revision = d.revision();
    d.key(Key::Enter)?;
    d.check(
        "Enter prefills the selected two-frame Hold without a partial batch write",
        d.app().command_open
            && d.app().command == "hold-duration 2f"
            && d.revision() == revision
            && document(d)? == &baseline,
        json!({"prefill":"hold-duration 2f","history":"unchanged"}),
        d.snapshot(),
    )?;
    super::shared_hold_duration::submit(d, "hold-duration 4f")?;
    d.changed(&revision)?;
    super::shared_hold_duration::settled(d)?;
    let grown = document(d)?.clone();
    let mapped = super::shared_hold_duration::target(d)?;
    let proof = grown.nodes().len() == baseline.nodes().len() + selection.fresh_pool()
        && baseline.nodes().iter().all(|(id, node)| grown.nodes().get(id) == Some(node))
        && override_count(&grown) == selection.overrides()
        && mapped.also.len() == 1
        && [&mapped.target, &mapped.also[0]].into_iter().all(|target|
            matches!(&grown.nodes()[&target.node].kind, NodeKind::Hold { recipe }
                if recipe.duration.frames() == 4 && matches!(recipe.audio, HoldAudio::RoomTone { .. })))
        && mapped.target.node != mapped.also[0].node
        && mapped.target.repeats.iter().map(|s| &s.branch).collect::<Vec<_>>()
            == prior.target.repeats.iter().map(|s| &s.branch).collect::<Vec<_>>()
        && d.app().sequence_cursor == u64::try_from(cursor.0).map_err(|e| e.to_string())?
        && d.app().source_cursor == original
        && d.app().presentation.displayed_source_frame() == displayed
        && grown.nodes()[&fixture::id("stage")?] == fixed;
    d.check("One selected duration commit owns exactly the requested families and retains its counterpart",
        proof, json!({"selection":selection.name(),"new_nodes":selection.fresh_pool(),"overrides":selection.overrides(),"Hold_frames":4,"output_frames":42}),
        d.snapshot())?;
    verify_uses(d, selection, 4)?;
    let revision = d.revision();
    d.command("hold-duration 4f")?;
    super::shared_hold_duration::settled(d)?;
    d.check(
        "A selected fixed-duration no-op preserves the complete mapped pair and revision",
        document(d)? == &grown
            && d.revision() == revision
            && authority(d)?.target == mapped.target
            && authority(d)?.also == mapped.also
            && d.app().error.is_none()
            && d.app().message.as_deref()
                == Some("The selected pauses already have that duration. No edit was made."),
        json!({"history":"unchanged","targets":2,"notice":"The selected pauses already have that duration. No edit was made."}),
        d.snapshot(),
    )?;
    undo(d)?;
    d.check(
        "Undo removes the complete selected batch and both allocated branches",
        super::shared_hold_duration::equivalent(document(d)?, &baseline)
            && d.app().scoped.is_none(),
        json!({"overrides":0,"Hold_frames":2}),
        d.snapshot(),
    )?;
    let revision = d.revision();
    d.key_modified(Key::R, egui::Modifiers::CTRL)?;
    d.changed(&revision)?;
    super::shared_hold_duration::settled(d)?;
    d.check(
        "Redo restores the complete selected batch and fixed Retime",
        super::shared_hold_duration::equivalent(document(d)?, &grown),
        json!({"overrides":selection.overrides(),"Hold_frames":4}),
        d.snapshot(),
    )?;
    choose(d, selection)?;
    edit(d, "hold-duration 3f")?;
    let shrunk = document(d)?.clone();
    let selected = authority(d)?;
    d.check(
        "Shrinking selected owned Holds allocates no further nodes and retains all counterparts",
        shrunk.nodes().len() == grown.nodes().len()
            && override_count(&shrunk) == selection.overrides()
            && selected.also.len() == 1
            && shrunk.nodes()[&fixture::id("stage")?] == fixed,
        json!({"Hold_frames":3,"new_nodes":0,"targets":2}),
        d.snapshot(),
    )?;
    verify_uses(d, selection, 3)?;
    scenarios::footer_anchored(
        d,
        "Selected fixed-duration history keeps compact footer and keys visible",
    )?;
    d.capture(selection.name())?;
    undo(d)?;
    choose(d, selection)?;
    let before_dot = document(d)?.clone();
    let revision = d.revision();
    d.key(Key::Period)?;
    d.changed(&revision)?;
    super::shared_hold_duration::settled(d)?;
    d.check(
        "Dot resolves the duration-only intent after explicitly reselecting the complete pair",
        same_content(document(d)?, &shrunk)? && authority(d)?.also.len() == 1,
        json!({"dot":"SetHoldDuration 3f","targets":2}),
        d.snapshot(),
    )?;
    undo(d)?;
    d.check(
        "Undo after dot restores the immediate four-frame state",
        super::shared_hold_duration::equivalent(document(d)?, &before_dot),
        json!({"Hold_frames":4}),
        d.snapshot(),
    )
}

fn macros_and_delayed(d: &mut Driver<'_>, selection: Selection) -> Result<(), String> {
    use deadpan_core::{PauseLength, SemanticInstruction};
    choose(d, selection)?;
    let baseline = document(d)?.clone();
    let revision = d.revision();
    d.chord(&[Key::Q, Key::W])?;
    d.command("hold-duration 3f")?;
    d.changed(&revision)?;
    super::shared_hold_duration::settled(d)?;
    d.key(Key::Q)?;
    super::shared_hold_duration::settled(d)?;
    let saved = d
        .app()
        .copied
        .entries()
        .find_map(|(name, value)| match value {
            crate::preview::copied::Content::Macro(program) if name == 'w' => {
                Some(program.instructions().to_vec())
            }
            _ => None,
        });
    let intent = SemanticInstruction::SetHoldDuration {
        length: PauseLength::Frames {
            frames: NonZeroU32::new(3).unwrap(),
        },
    };
    let recorded = document(d)?.clone();
    d.check(
        "A selected fixed-duration recording stores only semantic length intent",
        saved == Some(vec![intent]) && authority(d)?.also.len() == 1,
        json!({"saved":"SetHoldDuration 3f","captured_ids":false}),
        d.snapshot(),
    )?;
    undo(d)?;
    choose(d, selection)?;
    let revision = d.revision();
    super::super::macros::execute(d, 'w', None)?;
    d.changed(&revision)?;
    super::shared_hold_duration::settled(d)?;
    d.check(
        "Run resolves both current selected fixed-duration targets",
        same_content(document(d)?, &recorded)? && authority(d)?.also.len() == 1,
        json!({"run":"w","targets":2}),
        d.snapshot(),
    )?;
    undo(d)?;
    choose(d, selection)?;
    let prior = super::shared_hold_duration::target(d)?;
    d.app_mut().feedback.hold_project_updates = true;
    let pending = (|| {
        let revision = d.revision();
        d.command("hold-duration 3f")?;
        d.wait_for(
            "Selected fixed-duration batch is durable before navigation",
            |app| !app.service.is_busy(),
        )?;
        d.chord(&[Key::K, Key::J, Key::L])?;
        let current = super::shared_hold_duration::target(d)?;
        let cursor = d.app().sequence_cursor;
        d.check(
            "Newer navigation retains the selected pair with a new navigation token",
            current.target == prior.target
                && current.also == prior.also
                && current.navigation != prior.navigation,
            json!({"targets":2,"navigation":"new generation"}),
            d.snapshot(),
        )?;
        d.app_mut().feedback.hold_project_updates = false;
        d.changed(&revision)?;
        super::shared_hold_duration::settled(d)?;
        d.check(
            "A delayed receipt persists both Holds without reclaiming newer navigation",
            d.app().scoped.is_none()
                && d.app().sequence_cursor == cursor
                && d.app().sequence_length() == 42
                && same_content(document(d)?, &recorded)?
                && override_count(document(d)?) == override_count(&baseline),
            json!({"continuation":"revoked","Edit":cursor,"output_frames":42}),
            d.snapshot(),
        )
    })();
    d.app_mut().feedback.hold_project_updates = false;
    pending
}

fn reopen(
    d: &mut Driver<'_>,
    path: &Path,
    original: u64,
    selection: Selection,
    expected: &ProjectDocument,
) -> Result<(), String> {
    d.app().service.submit(ProjectRequest::Close)?;
    d.wait_for(
        "Close selected fixed-duration project before reopening",
        |app| app.workspace.is_none() && !app.service.is_busy(),
    )?;
    super::shared_hold_duration::open(d, path, original)?;
    choose(d, selection)?;
    d.check("Reopening retains both selected fixed-duration bodies and fixed output",
        document(d)? == expected && authority(d)?.also.len() == 1
            && override_count(document(d)?) == selection.overrides() && d.app().sequence_length() == 42,
        json!({"selection":selection.name(),"targets":2,"overrides":selection.overrides(),"output_frames":42}), d.snapshot())?;
    verify_uses(d, selection, 3)
}

/// Input [2,2+length) sampled at exact centers by six output frames over [0,8).
fn verify_uses(d: &mut Driver<'_>, selection: Selection, length: i64) -> Result<(), String> {
    let selected = authority(d)?;
    let nodes = [&selected.target.node, &selected.also[0].node];
    let workspace = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No selected fixed-duration workspace")?;
    let mut observed = Vec::new();
    for frame in 0..42_i64 {
        if workspace
            .plan
            .picture(ProjectFrame(frame))
            .map_err(|e| e.to_string())?
            .framing
            .iter()
            .any(|layer| nodes.contains(&&layer.instance.node))
        {
            observed.push(frame);
        }
    }
    let mut expected = Vec::new();
    for outer in 0..3 {
        for inner in 0..2 {
            if selection.affects(outer, inner) {
                for local in 0..6 {
                    let center = (2 * local + 1) * 2;
                    if center >= 6 && center < 3 * (2 + length) {
                        expected.push(3 + outer * 12 + inner * 6 + local);
                    }
                }
            }
        }
    }
    d.check(
        "Both selected Hold identities occupy precisely the authored affected families",
        observed == expected,
        json!({"selection":selection.name(),"sampled_frames":expected}),
        json!({"sampled_frames":observed}),
    )
}

fn override_count(document: &ProjectDocument) -> usize {
    document
        .overrides()
        .values()
        .map(|entries| entries.len())
        .sum()
}
fn same_content(actual: &ProjectDocument, expected: &ProjectDocument) -> Result<bool, String> {
    let mut a = serde_json::to_value(actual).map_err(|e| e.to_string())?;
    let mut b = serde_json::to_value(expected).map_err(|e| e.to_string())?;
    a["revision_id"] = b["revision_id"].clone();
    if !super::fixed_retime_duration::same_audio_clock_intent(
        actual.audio_bindings(),
        expected.audio_bindings(),
    ) {
        return Ok(false);
    }
    b["audio_bindings"] = a["audio_bindings"].clone();
    Ok(a == b)
}
fn undo(d: &mut Driver<'_>) -> Result<(), String> {
    let revision = d.revision();
    d.key(Key::U)?;
    d.changed(&revision)?;
    super::shared_hold_duration::settled(d)
}
fn edit(d: &mut Driver<'_>, text: &str) -> Result<(), String> {
    let revision = d.revision();
    d.command(text)?;
    d.changed(&revision)?;
    super::shared_hold_duration::settled(d)
}
fn scroll_top(d: &mut Driver<'_>) -> Result<(), String> {
    let point = d.rect("Nested beat inspector pane")?.center() + egui::vec2(0.0, 120.0);
    d.events(
        "Restore selected fixed-duration controls",
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
        d.step("Selected fixed-duration inspector scroll settles", false)?;
    }
    Ok(())
}
