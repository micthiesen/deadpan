//! Shared duration intent keeps explicit All plays choices and dormant definitions.
use super::*;
use crate::{preview::copied::Content, project::scoped::Target};
use deadpan_core::{
    HoldRecipe, HoldVideo, PauseLength, RepeatEditBranch, SemanticInstruction as I,
};
use std::{collections::BTreeMap, num::NonZeroU32, path::Path};

pub(super) mod fixture;

const CHANGE: &str = "Change duration…  Enter";

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let workspace = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No shared-duration workspace")?;
    let previous = workspace.path.clone();
    let baseline = workspace.document.clone();
    let original = d.app().source_cursor;
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
    let expected_picture = measured.identity;
    let freeze = HoldVideo::Freeze {
        asset: asset.clone(),
        timestamp: deadpan_core::SourceTimestamp {
            ticks: measured.pts,
            time_base: index.time_base(),
        },
    };
    let scratch = tempfile::tempdir().map_err(|e| e.to_string())?;
    let live = scratch.path().join("shared-live.deadpan");
    let dormant = scratch.path().join("shared-dormant.deadpan");
    fixture::create(&previous, &live, false, freeze.clone())?;
    fixture::create(&previous, &dormant, true, freeze)?;
    let result = (|| {
        open(d, &live, original)?;
        controls(d, expected_picture)?;
        recording(d)?;
        delayed(d)?;
        open(d, &dormant, original)?;
        dormant_case(d, &dormant, original)
    })();
    d.app_mut().feedback.hold_project_updates = false;
    let cleanup = (|| {
        d.app().service.submit(ProjectRequest::Close)?;
        d.wait_for("Close the private shared-duration writer", |app| {
            app.workspace.is_none() && !app.service.is_busy()
        })?;
        open(d, &previous, original)?;
        resize(d, 1280.0, 820.0)?;
        d.check(
            "Shared-duration replay restores the main edit and independent Original cursor",
            equivalent(document(d)?, &baseline)
                && d.app().source_cursor == original
                && d.app().scoped.is_none(),
            json!({"document":"restored","Original":original,"inspector":"closed"}),
            d.snapshot(),
        )
    })();
    if cleanup.is_err() {
        let _ = scratch.keep();
    }
    match (result, cleanup) {
        (Err(error), Err(cleanup)) => Err(format!("{error}; shared-duration cleanup: {cleanup}")),
        (Err(error), _) | (_, Err(error)) => Err(error),
        _ => Ok(()),
    }
}

pub(super) fn controls(d: &mut Driver<'_>, expected_picture: SourceFrameId) -> Result<(), String> {
    resize(d, 960.0, 640.0)?;
    choose(d)?;
    let baseline = document(d)?.clone();
    let prior = target(d)?;
    let original = d.app().source_cursor;
    let bank = register_bank(d, None);
    d.check("Nested shared duration captures two explicit All plays choices and the real later representative",
        prior.target.repeats.len() == 2
            && prior.target.repeats.iter().all(|s| s.branch == RepeatEditBranch::Default)
            && prior.also.is_empty() && !prior.has_single_play_authority()
            && prior.presentation.is_some() && d.app().sequence_cursor == 19
            && d.app().sequence_length() == 45
            && d.app().workspace.as_ref().is_some_and(|w| matches!(w.single_source,
                Some(deadpan_store::single_source::SingleSourceState::Ready { .. })))
            && matches!(hold(&baseline, "selected")?.audio, deadpan_core::HoldAudio::RoomTone { .. })
            && matches!(hold(&baseline, "selected")?.video, HoldVideo::Freeze { .. })
            && prior.semantic_framing(d.app().workspace.as_ref().ok_or("No workspace")?).is_err(),
        json!({"scope":["All plays","All plays"],"Edit":19,"output_frames":45,"generic_framing":"refused"}), d.snapshot())?;
    let revision = d.revision();
    d.key(Key::Enter)?;
    d.check(
        "All plays Hold Enter prefills the actual shared two-frame recipe",
        d.app().command_open && d.app().command == "hold-duration 2f" && d.revision() == revision,
        json!({"command":"hold-duration 2f","history":"unchanged"}),
        d.snapshot(),
    )?;
    scenarios::footer_anchored(d, "All plays duration entry keeps the minimum-size footer")?;
    d.capture("Nested All plays Hold duration entry at minimum size")?;
    submit(d, "hold-duration 4f")?;
    d.changed(&revision)?;
    settled(d)?;
    continuation(d, &baseline, &prior, 4, false)?;
    let grown = document(d)?.clone();
    d.check(
        "The resized shared Hold submits its first picture at the continued later representative",
        d.app()
            .presentation
            .displayed_sequence()
            .is_some_and(|(frame, _)| frame == ProjectFrame(21))
            && d.app().presentation.displayed_source_frame() == Some(expected_picture),
        json!({"displayed_Edit":21,"Original_picture":expected_picture}),
        d.snapshot(),
    )?;
    d.chord(&[Key::Num3, Key::L])?;
    settled(d)?;
    d.check(
        "The resized shared Hold displays its actual Original freeze at the last authored sample",
        d.app().sequence_cursor == 24
            && d.app()
                .presentation
                .displayed_sequence()
                .is_some_and(|(frame, _)| frame == ProjectFrame(24))
            && d.app().presentation.displayed_source_frame() == Some(expected_picture),
        json!({"Edit":24,"Original_picture":expected_picture}),
        d.snapshot(),
    )?;
    let cursor = d.app().sequence_cursor;
    let selected = target(d)?;
    let revision = d.revision();
    d.command("hold-duration 4f")?;
    settled(d)?;
    d.check("An unchanged All plays duration preserves interior navigation, target, history and registers",
        document(d)? == &grown && d.revision() == revision && target(d)? == selected
            && d.app().sequence_cursor == cursor && register_bank(d, None) == bank
            && d.app().source_cursor == original,
        json!({"history":"unchanged","Edit":24,"registers":"unchanged","Original":original}), d.snapshot())?;
    scroll_top(d)?;
    d.click(CHANGE)?;
    let revision = d.revision();
    submit(d, "hold-duration 3f")?;
    d.changed(&revision)?;
    settled(d)?;
    continuation(d, &baseline, &prior, 3, false)?;
    let shortened = document(d)?.clone();
    edit(d, "hold-duration 4f")?;
    continuation(d, &baseline, &prior, 4, false)?;
    let regrown = document(d)?.clone();
    d.capture("All plays grows two shared uses while independent inner and outer pauses retain their lengths")?;
    undo(d, &shortened)?;
    let revision = d.revision();
    d.key_modified(Key::R, egui::Modifiers::CTRL)?;
    d.changed(&revision)?;
    settled(d)?;
    d.check(
        "Redo restores the complete shared default edit without extra isolation",
        equivalent(document(d)?, &regrown),
        json!({"shared_frames":4,"output_frames":49,"overrides":"unchanged",
            "pre_undo_audio_bindings":regrown.audio_bindings(),
            "earlier_equal_length_audio_bindings":grown.audio_bindings()}),
        d.snapshot(),
    )?;
    undo(d, &shortened)?;
    undo(d, &grown)?;
    undo(d, &baseline)
}

pub(super) fn recording(d: &mut Driver<'_>) -> Result<(), String> {
    choose(d)?;
    let baseline = document(d)?.clone();
    let prior = target(d)?;
    let bank = register_bank(d, Some('w'));
    let revision = d.revision();
    d.chord(&[Key::Q, Key::W])?;
    d.check(
        "All plays recording teaches only the available duration instructions",
        d.app().message.as_ref().is_some_and(|message| {
            message.contains("pause lengths and named duration calls only")
                && !message.contains("in this play")
        }),
        json!({"recording":"w","instructions":"pause lengths and named duration calls only"}),
        d.snapshot(),
    )?;
    let captured = target(d)?;
    let all_registers = register_bank(d, None);
    for command in ["gain -6", "room-tone", "hold-silence"] {
        d.command(command)?;
        settled(d)?;
        d.check("All plays duration-only recording rejects another action before draft or recording mutation",
            document(d)? == &baseline && d.revision() == revision && target(d)? == captured
                && d.app().macros.recording_name() == Some('w') && d.app().macros.instruction_count() == 0
                && register_bank(d, None) == all_registers && d.app().gain.is_none()
                && d.app().room_tone.is_none()
                && d.app().error.as_ref().is_some_and(|e| e.contains("duration-only")),
            json!({"rejected":command,"authored":"unchanged","instructions":0,"drafts":"closed"}), d.snapshot())?;
    }
    d.chord(&[Key::Comma, Key::F])?;
    d.check(
        "All plays duration-only recording rejects Camera without acquiring framing authority",
        d.app().camera.is_none()
            && d.app().camera_pending.is_none()
            && d.app().macros.instruction_count() == 0
            && document(d)? == &baseline,
        json!({"Camera":"closed","instructions":0,"authored":"unchanged"}),
        d.snapshot(),
    )?;
    d.key(Key::Enter)?;
    submit(d, "hold-duration 4f")?;
    d.changed(&revision)?;
    settled(d)?;
    continuation(d, &baseline, &prior, 4, false)?;
    d.key(Key::Q)?;
    settled(d)?;
    let intent = intent(4);
    d.check(
        "Saved All plays duration records length intent without target or presentation identities",
        saved(d, 'w') == Some(vec![intent.clone()]) && register_bank(d, Some('w')) == bank,
        json!([intent]),
        d.snapshot(),
    )?;
    undo(d, &baseline)?;
    choose(d)?;
    d.chord(&[Key::Q, Key::X])?;
    let revision = d.revision();
    super::super::macros::execute(d, 'w', None)?;
    d.changed(&revision)?;
    settled(d)?;
    continuation(d, &baseline, &prior, 4, false)?;
    d.key(Key::Q)?;
    settled(d)?;
    d.check(
        "Recording a named duration call saves the call instead of concrete All plays identities",
        saved(d, 'x')
            == Some(vec![I::Call {
                register: deadpan_core::RegisterName::new('w').map_err(|e| e.to_string())?,
                count: NonZeroU32::new(1).unwrap(),
            }]),
        json!({"saved_x":"Call w"}),
        d.snapshot(),
    )?;
    undo(d, &baseline)?;
    choose(d)?;
    let revision = d.revision();
    super::super::macros::execute(d, 'x', None)?;
    d.changed(&revision)?;
    settled(d)?;
    continuation(d, &baseline, &prior, 4, false)?;
    undo(d, &baseline)?;
    choose(d)?;
    // A named Run and Undo retain their existing dot policies. Seed dot with
    // an actual parameter commit, then resolve another shared Hold at invocation.
    edit(d, "hold-duration 4f")?;
    let direct = document(d)?.clone();
    d.key(Key::K)?;
    let repeatable = d
        .app()
        .semantic
        .snapshot()
        .and_then(|s| s.edit_for(d.app().workspace.as_ref()?).ok())
        .is_some_and(|edit| {
            edit.operation == crate::project::semantic::RepeatableEdit::Parameter(intent.clone())
        });
    d.check(
        "Direct All plays parameter commit establishes the checked duration dot intent",
        repeatable,
        json!({"operation":"SetHoldDuration","frames":4}),
        d.snapshot(),
    )?;
    let revision = d.revision();
    d.key(Key::Period)?;
    d.changed(&revision)?;
    settled(d)?;
    let mut expected = direct.nodes().clone();
    let NodeKind::Hold { recipe } = &mut expected
        .get_mut(&fixture::id("before")?)
        .ok_or("No dot Hold")?
        .kind
    else {
        return Err("Dot target is not a Hold".into());
    };
    recipe.duration = deadpan_core::FrameDuration::new(4).map_err(|e| e.to_string())?;
    d.check("Dot resolves the newly selected shared Hold and preserves the separately resized pause and owned branches",
        document(d)?.nodes() == &expected && document(d)?.overrides() == direct.overrides()
            && target(d)?.target.node == fixture::id("before")?
            && d.app().sequence_length() == 53 && d.app().sequence_cursor == 21,
        json!({"dot":"Before shared pause","before_frames":4,"selected_frames":4,"output_frames":53,"Edit":21}), d.snapshot())?;
    d.capture(
        "All plays recording, named Call and direct duration dot preserve independent owned pauses",
    )?;
    undo(d, &direct)?;
    undo(d, &baseline)?;
    wrong_type(d)
}

fn wrong_type(d: &mut Driver<'_>) -> Result<(), String> {
    if d.app().scoped.is_some() {
        d.key(Key::Escape)?;
    }
    d.chord(&[Key::G, Key::G, Key::J, Key::Enter])?;
    settled(d)?;
    let before = document(d)?.clone();
    let selected = target(d)?;
    let bank = register_bank(d, None);
    let cursor = d.app().sequence_cursor;
    let revision = d.revision();
    super::super::macros::execute(d, 'w', None)?;
    settled(d)?;
    d.check("A saved duration refuses a newly selected Repeat instead of reusing its recorded Hold",
        matches!(before.nodes()[&selected.target.node].kind, NodeKind::Repeat { .. })
            && document(d)? == &before && d.revision() == revision && target(d)? == selected
            && d.app().sequence_cursor == cursor && register_bank(d, None) == bank
            && d.app().error.as_ref().is_some_and(|e| !e.is_empty()),
        json!({"target":"inner Repeat","document":"unchanged","registers":"unchanged","error":"explicit"}), d.snapshot())?;
    Ok(())
}

pub(super) fn delayed(d: &mut Driver<'_>) -> Result<(), String> {
    choose(d)?;
    let baseline = document(d)?.clone();
    let bank = register_bank(d, None);
    let original = d.app().source_cursor;
    let revision = d.revision();
    d.app_mut().feedback.hold_project_updates = true;
    let result = (|| {
        d.command("hold-duration 4f")?;
        d.wait_for(
            "Shared duration is durable before away/back navigation",
            |app| !app.service.is_busy(),
        )?;
        let entry = target(d)?;
        d.chord(&[Key::K, Key::J, Key::L])?;
        let current = target(d)?;
        let cursor = d.app().sequence_cursor;
        d.check("All plays away/back navigation creates a new continuation generation for the same authored Hold",
            entry.target == current.target && entry.navigation != current.navigation && cursor == 20,
            json!({"same_Hold":true,"generation":"changed","Edit":20}), d.snapshot())?;
        d.app_mut().feedback.hold_project_updates = false;
        d.changed(&revision)?;
        settled(d)?;
        d.check(
            "A delayed shared duration remains saved without reclaiming newer navigation",
            hold(document(d)?, "selected")?.duration.frames() == 4
                && d.app().sequence_length() == 49
                && d.app().sequence_cursor == cursor
                && d.app().source_cursor == original
                && d.app().scoped.is_none()
                && register_bank(d, None) == bank,
            json!({"saved_shared_frames":4,"output_frames":49,"Edit":20,"continuation":"revoked"}),
            d.snapshot(),
        )?;
        undo(d, &baseline)
    })();
    d.app_mut().feedback.hold_project_updates = false;
    result
}

fn dormant_case(d: &mut Driver<'_>, path: &Path, original: u64) -> Result<(), String> {
    choose(d)?;
    let baseline = document(d)?.clone();
    let prior = target(d)?;
    let cursor = d.app().sequence_cursor;
    let picture = d.app().presentation.displayed_source_frame();
    let bank = register_bank(d, None);
    d.check("A fully overridden outer Repeat exposes its real dormant shared Hold without inventing a play",
        prior.presentation.is_none() && prior.target.repeats.len() == 2
            && prior.target.repeats.iter().all(|s| s.branch == RepeatEditBranch::Default)
            && d.app().sequence_length() == 39,
        json!({"presentation":null,"output_frames":39,"scope":["All plays","All plays"]}), d.snapshot())?;
    painted(
        d,
        "All plays override this default; it has no active picture.",
    )?;
    let revision = d.revision();
    d.key(Key::Enter)?;
    d.check(
        "A dormant shared Hold still offers its actual authored duration through Enter",
        d.app().command_open && d.app().command == "hold-duration 2f",
        json!({"command":"hold-duration 2f","presentation":null}),
        d.snapshot(),
    )?;
    submit(d, "hold-duration 4f")?;
    d.changed(&revision)?;
    settled(d)?;
    continuation(d, &baseline, &prior, 4, true)?;
    let changed = document(d)?.clone();
    d.check(
        "Dormant duration preserves both clocks, retained picture and every register",
        d.app().sequence_cursor == cursor
            && d.app().source_cursor == original
            && d.app().presentation.displayed_source_frame() == picture
            && register_bank(d, None) == bank,
        json!({"Edit":cursor,"Original":original,"picture":"retained","registers":"unchanged"}),
        d.snapshot(),
    )?;
    d.capture("Dormant All plays Hold duration changes the definition while retaining both clocks and picture")?;
    undo(d, &baseline)?;
    let revision = d.revision();
    d.key_modified(Key::R, egui::Modifiers::CTRL)?;
    d.changed(&revision)?;
    settled(d)?;
    d.check(
        "Dormant duration Redo restores only the authored definition",
        equivalent(document(d)?, &changed) && d.app().sequence_length() == 39,
        json!({"shared_frames":4,"output_frames":39}),
        d.snapshot(),
    )?;
    open(d, path, original)?;
    d.check("Reopening retains the dormant recipe and independent owned plays",
        equivalent(document(d)?, &changed) && d.app().sequence_length() == 39,
        json!({"reopened_shared_frames":4,"owned_inner_frames":4,"owned_outer_frames":7,"output_frames":39}), d.snapshot())?;
    choose(d)?;
    d.key(Key::Enter)?;
    d.check(
        "Reopened dormant Enter uses the saved duration without a fabricated presentation",
        d.app().command == "hold-duration 4f" && target(d)?.presentation.is_none(),
        json!({"command":"hold-duration 4f","presentation":null}),
        d.snapshot(),
    )?;
    d.key(Key::Escape)
}

fn continuation(
    d: &mut Driver<'_>,
    before: &ProjectDocument,
    prior: &Target,
    length: i64,
    dormant: bool,
) -> Result<(), String> {
    let after = document(d)?;
    let selected = target(d)?;
    let mut nodes = before.nodes().clone();
    let NodeKind::Hold { recipe } = &mut nodes
        .get_mut(&fixture::id("selected")?)
        .ok_or("No shared Hold")?
        .kind
    else {
        return Err("Shared target is not a Hold".into());
    };
    recipe.duration = deadpan_core::FrameDuration::new(length).map_err(|e| e.to_string())?;
    let output = if dormant { 39 } else { 41 + 2 * length };
    let cursor = if dormant { 19 } else { 17 + length };
    let picture_matches = if let Some(presentation) = &selected.presentation {
        d.app()
            .workspace
            .as_ref()
            .ok_or("No continued workspace")?
            .plan
            .picture(ProjectFrame(cursor))
            .map_err(|e| e.to_string())?
            .framing
            .iter()
            .any(|layer| &layer.instance == presentation)
    } else {
        dormant
    };
    let dormant_plan_unchanged = if dormant {
        let old_plan = deadpan_plan::RenderPlan::compile(before).map_err(|e| e.to_string())?;
        let current = &d
            .app()
            .workspace
            .as_ref()
            .ok_or("No dormant workspace")?
            .plan;
        let mut unchanged = true;
        for frame in 0..39 {
            let mut old = old_plan
                .picture(ProjectFrame(frame))
                .map_err(|e| e.to_string())?;
            old.revision_id = after.revision_id().clone();
            unchanged &= old
                == current
                    .picture(ProjectFrame(frame))
                    .map_err(|e| e.to_string())?;
        }
        unchanged
    } else {
        true
    };
    d.check("Shared duration changes exactly the literal Hold and retains explicit authoring and presentation authority",
        after.nodes() == &nodes && after.overrides() == before.overrides()
            && after.gap_overrides() == before.gap_overrides()
            && after.assets() == before.assets() && after.sounds() == before.sounds()
            && after.beat_sounds() == before.beat_sounds()
            && selected.target == prior.target && selected.root == prior.root
            && selected.presentation == prior.presentation && selected.also.is_empty()
            && d.app().sequence_length() == u64::try_from(output).map_err(|e| e.to_string())?
            && d.app().sequence_cursor == u64::try_from(cursor).map_err(|e| e.to_string())?
            && picture_matches && dormant_plan_unchanged,
        json!({"shared_frames":length,"output_frames":output,"Edit":cursor,"owned_inner_frames":4,"owned_outer_frames":7,"isolation":"none","dormant":dormant}), d.snapshot())
}

pub(super) fn choose(d: &mut Driver<'_>) -> Result<(), String> {
    if d.app().scoped.is_some() {
        d.key(Key::Escape)?;
    }
    let mut keys = vec![
        Key::G,
        Key::G,
        Key::Num1,
        Key::Num9,
        Key::L,
        Key::Enter,
        Key::Enter,
        Key::Enter,
    ];
    if document(d)?
        .nodes()
        .contains_key(&fixture::id("fixed-stage")?)
    {
        keys.push(Key::Enter);
    }
    keys.push(Key::J);
    d.chord(&keys)?;
    settled(d)?;
    scroll_top(d)?;
    if target(d)?.target.node != fixture::id("selected")? {
        return Err("Native All plays navigation did not select Shared pause".into());
    }
    Ok(())
}
pub(super) fn open(d: &mut Driver<'_>, path: &Path, original: u64) -> Result<(), String> {
    let path = path.canonicalize().map_err(|e| e.to_string())?;
    d.app().service.submit(ProjectRequest::Open(path.clone()))?;
    d.wait_for("Open the qualified shared-duration project", |app| {
        app.workspace.as_ref().is_some_and(|w| w.path == path) && !app.service.is_busy()
    })?;
    d.command("source")?;
    d.chord(&[Key::G, Key::G])?;
    if original > 0 {
        let mut keys = original
            .to_string()
            .chars()
            .map(|digit| {
                Key::from_name(&digit.to_string()).ok_or("Invalid Original cursor digit".to_owned())
            })
            .collect::<Result<Vec<_>, _>>()?;
        keys.push(Key::L);
        d.chord(&keys)?;
    }
    d.command("sequence")?;
    super::super::transcript::focus_your_edit(d)?;
    d.chord(&[Key::G, Key::G])?;
    settled(d)
}
fn scroll_top(d: &mut Driver<'_>) -> Result<(), String> {
    let point = d.rect("Nested beat inspector pane")?.center() + egui::vec2(0.0, 120.0);
    d.events(
        "Restore shared-duration inspector controls",
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
        d.step("Shared-duration inspector scroll settles", false)?;
    }
    Ok(())
}
fn intent(frames: u32) -> I {
    I::SetHoldDuration {
        length: PauseLength::Frames {
            frames: NonZeroU32::new(frames).unwrap(),
        },
    }
}
pub(super) fn target(d: &Driver<'_>) -> Result<Target, String> {
    d.app()
        .scoped_target()?
        .ok_or("No shared Hold target".into())
}
fn hold<'a>(document: &'a ProjectDocument, name: &str) -> Result<&'a HoldRecipe, String> {
    match &document.nodes()[&fixture::id(name)?].kind {
        NodeKind::Hold { recipe } => Ok(recipe),
        _ => Err("Expected shared-duration Hold".into()),
    }
}
fn register_bank(d: &Driver<'_>, except: Option<char>) -> BTreeMap<char, String> {
    d.app()
        .copied
        .entries()
        .filter(|(name, _)| Some(*name) != except)
        .map(|(name, value)| (name, format!("{value:?}")))
        .collect()
}
fn saved(d: &Driver<'_>, name: char) -> Option<Vec<I>> {
    d.app()
        .copied
        .entries()
        .find_map(|(register, value)| match value {
            Content::Macro(program) if register == name => Some(program.instructions().to_vec()),
            _ => None,
        })
}
pub(super) fn equivalent(actual: &ProjectDocument, expected: &ProjectDocument) -> bool {
    let mut value = serde_json::to_value(actual).unwrap();
    value["revision_id"] = json!(expected.revision_id());
    value == serde_json::to_value(expected).unwrap()
}
pub(super) fn settled(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Shared-duration receipt is admitted", |app| {
        !app.service.is_busy() && !app.macros.is_pending()
    })?;
    d.settled()
}
fn edit(d: &mut Driver<'_>, text: &str) -> Result<(), String> {
    let revision = d.revision();
    d.command(text)?;
    d.changed(&revision)?;
    settled(d)
}
pub(super) fn submit(d: &mut Driver<'_>, text: &str) -> Result<(), String> {
    d.key_modified(Key::A, egui::Modifiers::COMMAND)?;
    d.events(
        "Replace the All plays Hold-duration parameter",
        vec![
            egui::Event::Text(text.into()),
            super::super::key_event(Key::Enter, egui::Modifiers::NONE, true),
            super::super::key_event(Key::Enter, egui::Modifiers::NONE, false),
        ],
    )
}
fn undo(d: &mut Driver<'_>, expected: &ProjectDocument) -> Result<(), String> {
    let revision = d.revision();
    d.key(Key::U)?;
    d.changed(&revision)?;
    settled(d)?;
    d.check(
        "One Undo restores the complete prior shared-duration document",
        equivalent(document(d)?, expected) && d.app().scoped.is_none(),
        json!("restored"),
        d.snapshot(),
    )
}
