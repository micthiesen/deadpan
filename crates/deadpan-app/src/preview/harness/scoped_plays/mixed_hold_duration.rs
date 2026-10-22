//! Mixed duration authoring isolates only explicit Play branches.
use super::*;
use crate::{preview::copied::Content, project::scoped::Target};
use deadpan_core::{
    HoldRecipe, HoldVideo, PauseLength, RegisterName, RepeatEditBranch, SemanticInstruction as I,
};
use std::{collections::BTreeMap, num::NonZeroU32, path::Path};

// Reuse the verified qualified Original/RoomTone fixture without a second seed.
use super::shared_hold_duration::fixture;

const CHANGE: &str = "Change duration…  Enter";

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let workspace = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No mixed-duration workspace")?;
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
    let live = scratch.path().join("mixed-live.deadpan");
    let dormant = scratch.path().join("mixed-dormant.deadpan");
    fixture::create(&previous, &live, false, freeze.clone())?;
    fixture::create(&previous, &dormant, true, freeze)?;
    let result = (|| {
        open(d, &live, original)?;
        controls(d, &live, original, true, expected_picture)?;
        controls(d, &live, original, false, expected_picture)?;
        recording(d)?;
        delayed(d)?;
        open(d, &dormant, original)?;
        dormant_case(d, &dormant, original)
    })();
    d.app_mut().feedback.hold_project_updates = false;
    let cleanup = (|| {
        d.app().service.submit(ProjectRequest::Close)?;
        d.wait_for("Close the private mixed-duration writer", |app| {
            app.workspace.is_none() && !app.service.is_busy()
        })?;
        open(d, &previous, original)?;
        resize(d, 1280.0, 820.0)?;
        d.check(
            "Mixed-duration replay restores the main edit and independent Original clock",
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
        (Err(error), Err(cleanup)) => Err(format!("{error}; mixed-duration cleanup: {cleanup}")),
        (Err(error), _) | (_, Err(error)) => Err(error),
        _ => Ok(()),
    }
}

fn controls(
    d: &mut Driver<'_>,
    path: &Path,
    original: u64,
    outer_play: bool,
    expected_picture: SourceFrameId,
) -> Result<(), String> {
    resize(d, 960.0, 640.0)?;
    choose(d, outer_play)?;
    let baseline = document(d)?.clone();
    let prior = target(d)?;
    let bank = registers(d, None);
    let revision = d.revision();
    d.check("A native mixed Hold retains its explicit Default/Play authoring address and real representative",
        branches(&prior) == [outer_play, !outer_play] && prior.also.is_empty()
            && prior.presentation.is_some() && prior.target.node == fixture::id("selected")?
            && d.app().sequence_cursor == 19 && d.app().sequence_length() == 45
            && prior.semantic_framing(d.app().workspace.as_ref().ok_or("No workspace")?).is_err()
            && matches!(recipe(&baseline, &prior.target.node)?.video, HoldVideo::Freeze { .. })
            && matches!(recipe(&baseline, &prior.target.node)?.audio, deadpan_core::HoldAudio::RoomTone { .. }),
        json!({"outer_play":outer_play,"scope":scope(outer_play),"Edit":19,"output_frames":45,"framing":"refused"}), d.snapshot())?;
    d.key(Key::Enter)?;
    d.check(
        "Mixed Hold Enter prefills the current two-frame recipe without isolation or history",
        d.app().command_open
            && d.app().command == "hold-duration 2f"
            && d.revision() == revision
            && document(d)? == &baseline,
        json!({"command":"hold-duration 2f","nodes":"unchanged","history":"unchanged"}),
        d.snapshot(),
    )?;
    scenarios::footer_anchored(d, "Mixed Hold duration entry keeps the minimum-size footer")?;
    d.capture(if outer_play {
        "This play then All plays duration entry at minimum size"
    } else {
        "All plays then This play duration entry at minimum size"
    })?;
    submit(d, "hold-duration 3f")?;
    d.changed(&revision)?;
    settled(d)?;
    continuation(d, &baseline, &prior, outer_play, 3, false)?;
    let first = document(d)?.clone();
    let mapped = target(d)?;
    let start = if outer_play { 19 } else { 20 };
    d.check(
        "Mixed duration submits the first picture of the actual mapped Freeze occurrence",
        d.app()
            .presentation
            .displayed_sequence()
            .is_some_and(|(frame, _)| frame == ProjectFrame(start))
            && d.app().presentation.displayed_source_frame() == Some(expected_picture),
        json!({"displayed_Edit":start,"Original_picture":expected_picture}),
        d.snapshot(),
    )?;
    d.chord(&[Key::Num2, Key::L])?;
    settled(d)?;
    d.check(
        "Mixed duration submits the last authored Freeze sample before the surviving suffix",
        d.app().sequence_cursor == u64::try_from(start + 2).map_err(|e| e.to_string())?
            && d.app()
                .presentation
                .displayed_sequence()
                .is_some_and(|(frame, _)| frame == ProjectFrame(start + 2))
            && d.app().presentation.displayed_source_frame() == Some(expected_picture),
        json!({"displayed_Edit":start+2,"Original_picture":expected_picture}),
        d.snapshot(),
    )?;
    let cursor = d.app().sequence_cursor;
    let selected = target(d)?;
    let revision = d.revision();
    d.command("hold-duration 3f")?;
    settled(d)?;
    d.check("A mixed duration no-op preserves exact history, interior cursor, mapped target and registers",
        document(d)? == &first && d.revision() == revision && target(d)? == selected
            && d.app().sequence_cursor == cursor && d.app().source_cursor == original
            && registers(d, None) == bank,
        json!({"history":"unchanged","Edit":cursor,"target":"unchanged","registers":"unchanged"}), d.snapshot())?;
    scroll_top(d)?;
    d.click(CHANGE)?;
    let revision = d.revision();
    submit(d, "hold-duration 4f")?;
    d.changed(&revision)?;
    settled(d)?;
    continuation(d, &baseline, &prior, outer_play, 4, false)?;
    let longer = document(d)?.clone();
    d.check(
        "Revisiting an already isolated mixed Hold consumes no further node identities",
        target(d)?.target == mapped.target && document(d)?.nodes().len() == first.nodes().len(),
        json!({"new_nodes":0,"mapped_target":"same"}),
        d.snapshot(),
    )?;
    edit(d, "hold-duration 3f")?;
    continuation(d, &baseline, &prior, outer_play, 3, false)?;
    let latest = document(d)?.clone();
    d.capture(if outer_play {
        "Mixed outer This play isolates one body and preserves inner All plays"
    } else {
        "Mixed inner This play remains shared through outer All plays"
    })?;
    undo(d, &longer)?;
    let revision = d.revision();
    d.key_modified(Key::R, egui::Modifiers::CTRL)?;
    d.changed(&revision)?;
    settled(d)?;
    d.check(
        "Mixed duration Redo restores the exact latest pre-Undo document including retained clocks",
        equivalent(document(d)?, &latest),
        json!({"history":"exact pre-Undo state","scope":scope(outer_play)}),
        d.snapshot(),
    )?;
    open(d, path, original)?;
    d.check(
        "Reopening preserves the mapped mixed edit, sparse overrides and exact retained clocks",
        equivalent(document(d)?, &latest),
        json!({"reopened":"exact saved state","scope":scope(outer_play)}),
        d.snapshot(),
    )?;
    undo(d, &longer)?;
    undo(d, &first)?;
    undo(d, &baseline)
}

fn recording(d: &mut Driver<'_>) -> Result<(), String> {
    choose(d, true)?;
    let baseline = document(d)?.clone();
    let prior = target(d)?;
    let bank = registers(d, Some('e'));
    let revision = d.revision();
    d.chord(&[Key::Q, Key::E])?;
    let captured = target(d)?;
    let all_registers = registers(d, None);
    for command in ["gain -6", "room-tone", "hold-silence"] {
        d.command(command)?;
        settled(d)?;
        d.check("Mixed duration-only recording refuses another operation before draft, identity or instruction mutation",
            document(d)? == &baseline && d.revision() == revision && target(d)? == captured
                && registers(d, None) == all_registers && d.app().macros.recording_name() == Some('e')
                && d.app().macros.instruction_count() == 0 && d.app().gain.is_none() && d.app().room_tone.is_none()
                && d.app().error.as_ref().is_some_and(|error| error.contains("duration-only")),
            json!({"rejected":command,"document":"unchanged","instructions":0,"drafts":"closed"}), d.snapshot())?;
    }
    d.chord(&[Key::Comma, Key::F])?;
    d.check(
        "Mixed recording cannot acquire Camera or generic framing authority",
        d.app().camera.is_none()
            && d.app().camera_pending.is_none()
            && document(d)? == &baseline
            && d.app().macros.recording_name() == Some('e')
            && d.app().macros.instruction_count() == 0,
        json!({"Camera":"closed","instructions":0,"document":"unchanged"}),
        d.snapshot(),
    )?;
    edit(d, "hold-duration 3f")?;
    continuation(d, &baseline, &prior, true, 3, false)?;
    d.key(Key::Q)?;
    settled(d)?;
    d.check(
        "A saved mixed duration contains only a length and preserves the other registers",
        saved(d, 'e') == Some(vec![intent(3)]) && registers(d, Some('e')) == bank,
        json!({"saved_e":"SetHoldDuration 3f","targets":"absent"}),
        d.snapshot(),
    )?;
    undo(d, &baseline)?;
    choose(d, false)?;
    let opposite = target(d)?;
    let revision = d.revision();
    super::super::macros::execute(d, 'e', None)?;
    d.changed(&revision)?;
    settled(d)?;
    continuation(d, &baseline, &opposite, false, 3, false)?;
    d.check(
        "A mixed duration macro resolves the opposite current authoring address",
        branches(&target(d)?) == [false, true] && d.app().sequence_length() == 47,
        json!({"recorded_scope":scope(true),"invoked_scope":scope(false),"output_frames":47}),
        d.snapshot(),
    )?;
    undo(d, &baseline)?;
    choose(d, false)?;
    d.chord(&[Key::Q, Key::F])?;
    let revision = d.revision();
    super::super::macros::execute(d, 'e', None)?;
    d.changed(&revision)?;
    settled(d)?;
    d.key(Key::Q)?;
    settled(d)?;
    d.check(
        "A recorded mixed duration caller saves a named Call without mapped node IDs",
        saved(d, 'f')
            == Some(vec![I::Call {
                register: RegisterName::new('e').map_err(|e| e.to_string())?,
                count: NonZeroU32::new(1).unwrap(),
            }]),
        json!({"saved_f":"Call e once"}),
        d.snapshot(),
    )?;
    undo(d, &baseline)?;
    choose(d, false)?;
    let opposite = target(d)?;
    let revision = d.revision();
    super::super::macros::execute(d, 'f', None)?;
    d.changed(&revision)?;
    settled(d)?;
    continuation(d, &baseline, &opposite, false, 3, false)?;
    undo(d, &baseline)?;
    choose(d, false)?;
    edit(d, "hold-duration 3f")?;
    let direct = document(d)?.clone();
    d.key(Key::K)?;
    let next = target(d)?;
    let repeatable = d
        .app()
        .semantic
        .snapshot()
        .and_then(|snapshot| snapshot.edit_for(d.app().workspace.as_ref()?).ok())
        .is_some_and(|edit| {
            edit.operation == crate::project::semantic::RepeatableEdit::Parameter(intent(3))
        });
    d.check(
        "A direct mixed parameter commit seeds the checked invocation-relative duration dot",
        repeatable && branches(&next) == [false, true],
        json!({"operation":"SetHoldDuration","frames":3,"scope":scope(false)}),
        d.snapshot(),
    )?;
    let revision = d.revision();
    d.key(Key::Period)?;
    d.changed(&revision)?;
    settled(d)?;
    let mut expected = direct.nodes().clone();
    let NodeKind::Hold { recipe } = &mut expected
        .get_mut(&next.target.node)
        .ok_or("No mixed dot Hold")?
        .kind
    else {
        return Err("Mixed dot target is not a Hold".into());
    };
    recipe.duration = deadpan_core::FrameDuration::new(3).map_err(|e| e.to_string())?;
    d.check("Mixed duration dot resolves another Hold in the current owned branch without another isolation",
        document(d)?.nodes() == &expected && document(d)?.overrides() == direct.overrides()
            && target(d)?.target == next.target && d.app().sequence_length() == 49
            && d.app().sequence_cursor == 19,
        json!({"dot":"Before shared pause","before_frames":3,"selected_frames":3,"output_frames":49,"Edit":19,"new_nodes":0}), d.snapshot())?;
    scenarios::footer_anchored(d, "Mixed recorded duration and dot keep the compact footer")?;
    d.capture(
        "Mixed duration recording Call and direct dot retain the current Default Play choices",
    )?;
    undo(d, &direct)?;
    undo(d, &baseline)
}

fn delayed(d: &mut Driver<'_>) -> Result<(), String> {
    choose(d, true)?;
    let baseline = document(d)?.clone();
    let original = d.app().source_cursor;
    let bank = registers(d, None);
    let revision = d.revision();
    d.app_mut().feedback.hold_project_updates = true;
    let result = (|| {
        d.command("hold-duration 3f")?;
        d.wait_for(
            "Mixed isolation and duration are durable before away/back navigation",
            |app| !app.service.is_busy(),
        )?;
        let entry = target(d)?;
        d.chord(&[Key::K, Key::J, Key::L])?;
        let current = target(d)?;
        let cursor = d.app().sequence_cursor;
        d.check("Mixed away/back navigation revokes the old continuation while retaining the original selected address",
            current.target == entry.target && current.navigation != entry.navigation && cursor == 20,
            json!({"same_target":true,"navigation":"new generation","Edit":20}), d.snapshot())?;
        d.app_mut().feedback.hold_project_updates = false;
        d.changed(&revision)?;
        settled(d)?;
        let changed = document(d)?;
        let new_holds = changed.nodes().iter().filter(|(id, node)| !baseline.nodes().contains_key(*id)
            && matches!(&node.kind, NodeKind::Hold { recipe } if recipe.duration.frames() == 3)).count();
        d.check("A durable mixed edit cannot reclaim newer navigation after its target was isolated",
            d.app().scoped.is_none() && d.app().sequence_cursor == cursor && d.app().source_cursor == original
                && d.app().sequence_length() == 46 && new_holds == 1
                && registers(d, None) == bank,
            json!({"output_frames":46,"new_3f_Holds":1,"Edit":20,"continuation":"revoked","registers":"unchanged"}), d.snapshot())?;
        undo(d, &baseline)
    })();
    d.app_mut().feedback.hold_project_updates = false;
    result
}

fn dormant_case(d: &mut Driver<'_>, path: &Path, original: u64) -> Result<(), String> {
    choose(d, false)?;
    let baseline = document(d)?.clone();
    let prior = target(d)?;
    let cursor = d.app().sequence_cursor;
    let picture = d.app().presentation.displayed_source_frame();
    let bank = registers(d, None);
    d.check("A dormant outer Default and inner Play retain mixed authority without an invented presentation",
        branches(&prior) == [false, true] && prior.presentation.is_none() && d.app().sequence_length() == 39,
        json!({"scope":scope(false),"presentation":null,"output_frames":39}), d.snapshot())?;
    painted(
        d,
        "All plays override this default; it has no active picture.",
    )?;
    let revision = d.revision();
    d.key(Key::Enter)?;
    d.check(
        "Dormant mixed Enter offers the authored duration before any isolation",
        d.app().command_open && d.app().command == "hold-duration 2f" && document(d)? == &baseline,
        json!({"command":"hold-duration 2f","history":"unchanged"}),
        d.snapshot(),
    )?;
    submit(d, "hold-duration 3f")?;
    d.changed(&revision)?;
    settled(d)?;
    continuation(d, &baseline, &prior, false, 3, true)?;
    let changed = document(d)?.clone();
    d.check("Dormant mixed isolation changes only authoring while preserving both clocks picture and registers",
        d.app().sequence_cursor == cursor && d.app().source_cursor == original
            && d.app().presentation.displayed_source_frame() == picture && registers(d, None) == bank,
        json!({"Edit":cursor,"Original":original,"picture":"retained","registers":"unchanged"}), d.snapshot())?;
    scenarios::footer_anchored(
        d,
        "Dormant mixed Hold keeps its visible duration and compact footer",
    )?;
    d.capture(
        "Dormant mixed All plays This play duration isolates authoring without moving output",
    )?;
    undo(d, &baseline)?;
    let revision = d.revision();
    d.key_modified(Key::R, egui::Modifiers::CTRL)?;
    d.changed(&revision)?;
    settled(d)?;
    d.check(
        "Dormant mixed Redo restores its exact mapped definition and unchanged output",
        equivalent(document(d)?, &changed) && d.app().sequence_length() == 39,
        json!({"history":"exact pre-Undo state","output_frames":39}),
        d.snapshot(),
    )?;
    open(d, path, original)?;
    d.check(
        "Reopening preserves the dormant mixed isolation and original shared recipe",
        equivalent(document(d)?, &changed)
            && recipe(document(d)?, &fixture::id("selected")?)?
                .duration
                .frames()
                == 2,
        json!({"reopened":"exact saved state","literal_default_frames":2}),
        d.snapshot(),
    )
}

fn continuation(
    d: &mut Driver<'_>,
    before: &ProjectDocument,
    prior: &Target,
    outer_play: bool,
    length: i64,
    dormant: bool,
) -> Result<(), String> {
    let after = document(d)?;
    let selected = target(d)?;
    let copies = if outer_play { 9 } else { 4 };
    let uses = if outer_play { 1 } else { 2 };
    let output = if dormant {
        39
    } else {
        45 + uses * (length - 2)
    };
    let start = if dormant || outer_play {
        19
    } else {
        17 + length
    };
    let old_nodes_unchanged = before
        .nodes()
        .iter()
        .all(|(id, node)| after.nodes().get(id) == Some(node));
    let mut expected_recipe = recipe(before, &prior.target.node)?.clone();
    expected_recipe.duration =
        deadpan_core::FrameDuration::new(length).map_err(|e| e.to_string())?;
    let plan = &d.app().workspace.as_ref().ok_or("No mixed plan")?.plan;
    let (presentation_matches, sampled_uses) = if let Some(presentation) = &selected.presentation {
        let matches = plan
            .picture(ProjectFrame(start))
            .map_err(|e| e.to_string())?
            .framing
            .iter()
            .any(|layer| &layer.instance == presentation);
        let mut frames = 0;
        for frame in 0..plan.duration().frames() {
            frames += i64::from(
                plan.picture(ProjectFrame(frame))
                    .map_err(|e| e.to_string())?
                    .framing
                    .iter()
                    .any(|layer| layer.instance.node == selected.target.node),
            );
        }
        (matches, frames)
    } else {
        (dormant, 0)
    };
    let unchanged_output = if dormant {
        let old_plan = deadpan_plan::RenderPlan::compile(before).map_err(|e| e.to_string())?;
        let mut same = true;
        for frame in 0..39 {
            let mut old = old_plan
                .picture(ProjectFrame(frame))
                .map_err(|e| e.to_string())?;
            old.revision_id = after.revision_id().clone();
            same &= old
                == plan
                    .picture(ProjectFrame(frame))
                    .map_err(|e| e.to_string())?;
        }
        same
    } else {
        true
    };
    let stable_play = prior
        .target
        .repeats
        .iter()
        .zip(&selected.target.repeats)
        .all(|(old, new)| old.branch == new.branch);
    d.check("Mixed duration follows actual isolation maps preserves literal and independent owners and affects only intended live uses",
        branches(&selected) == [outer_play, !outer_play] && stable_play
            && selected.root == prior.root && selected.target.node != prior.target.node
            && selected.also.is_empty() && selected.presentation.is_none() == dormant
            && after.nodes().len() == before.nodes().len() + copies && old_nodes_unchanged
            && recipe(after, &selected.target.node)? == &expected_recipe
            && recipe(after, &fixture::id("inner-pause")?)?.duration.frames() == 4
            && recipe(after, &fixture::id("outer-two-pause")?)?.duration.frames() == 7
            && d.app().sequence_length() == u64::try_from(output).map_err(|e| e.to_string())?
            && d.app().sequence_cursor == u64::try_from(start).map_err(|e| e.to_string())?
            && sampled_uses == if dormant { 0 } else { uses * length }
            && presentation_matches && unchanged_output && after.assets() == before.assets(),
        json!({"scope":scope(outer_play),"new_nodes":copies,"Hold_frames":length,"output_frames":output,"Edit":start,
            "live_uses":if dormant {0}else{uses},"owned_inner_frames":4,"owned_outer_frames":7,"dormant":dormant}), d.snapshot())
}

fn choose(d: &mut Driver<'_>, outer_play: bool) -> Result<(), String> {
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
    settled(d)?;
    scroll_top(d)?;
    if target(d)?.target.node != fixture::id("selected")? {
        return Err("Mixed navigation missed the literal shared Hold".into());
    }
    Ok(())
}
fn open(d: &mut Driver<'_>, path: &Path, original: u64) -> Result<(), String> {
    let path = path.canonicalize().map_err(|e| e.to_string())?;
    d.app().service.submit(ProjectRequest::Open(path.clone()))?;
    d.wait_for("Open the qualified mixed-duration project", |app| {
        app.workspace.as_ref().is_some_and(|w| w.path == path) && !app.service.is_busy()
    })?;
    d.command("source")?;
    d.chord(&[Key::G, Key::G])?;
    if original > 0 {
        let mut keys = original
            .to_string()
            .chars()
            .map(|digit| {
                Key::from_name(&digit.to_string()).ok_or("Invalid Original digit".to_owned())
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
        "Restore mixed-duration inspector controls",
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
        d.step("Mixed-duration scroll settles", false)?;
    }
    Ok(())
}
fn branches(target: &Target) -> Vec<bool> {
    target
        .target
        .repeats
        .iter()
        .map(|step| matches!(step.branch, RepeatEditBranch::Play { .. }))
        .collect()
}
fn scope(outer_play: bool) -> [&'static str; 2] {
    if outer_play {
        ["This play 2", "All plays"]
    } else {
        ["All plays", "This play 1"]
    }
}
fn recipe<'a>(document: &'a ProjectDocument, node: &NodeId) -> Result<&'a HoldRecipe, String> {
    match &document.nodes()[node].kind {
        NodeKind::Hold { recipe } => Ok(recipe),
        _ => Err("Expected mixed Hold".into()),
    }
}
fn intent(frames: u32) -> I {
    I::SetHoldDuration {
        length: PauseLength::Frames {
            frames: NonZeroU32::new(frames).unwrap(),
        },
    }
}
fn target(d: &Driver<'_>) -> Result<Target, String> {
    d.app().scoped_target()?.ok_or("No mixed target".into())
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
fn registers(d: &Driver<'_>, except: Option<char>) -> BTreeMap<char, String> {
    d.app()
        .copied
        .entries()
        .filter(|(name, _)| Some(*name) != except)
        .map(|(name, value)| (name, format!("{value:?}")))
        .collect()
}
fn equivalent(actual: &ProjectDocument, expected: &ProjectDocument) -> bool {
    let mut value = serde_json::to_value(actual).unwrap();
    value["revision_id"] = json!(expected.revision_id());
    value == serde_json::to_value(expected).unwrap()
}
fn settled(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Mixed-duration receipt is admitted", |app| {
        !app.service.is_busy() && !app.macros.is_pending()
    })?;
    d.settled()
}
fn edit(d: &mut Driver<'_>, command: &str) -> Result<(), String> {
    let revision = d.revision();
    d.command(command)?;
    d.changed(&revision)?;
    settled(d)
}
fn submit(d: &mut Driver<'_>, command: &str) -> Result<(), String> {
    d.key_modified(Key::A, egui::Modifiers::COMMAND)?;
    d.events(
        "Replace the mixed Hold duration parameter",
        vec![
            egui::Event::Text(command.into()),
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
        "One Undo restores the exact preceding mixed-duration state",
        equivalent(document(d)?, expected) && d.app().scoped.is_none(),
        json!("restored"),
        d.snapshot(),
    )
}
