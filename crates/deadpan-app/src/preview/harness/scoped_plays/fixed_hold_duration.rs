//! Ordinary fixed-window duration intent through real native controls.
use super::*;
use crate::{preview::copied::Content, project::scoped::Target};
use deadpan_core::{ExactRatio, HoldRecipe, SemanticInstruction as I};
use std::{collections::BTreeMap, num::NonZeroU32, path::Path};

mod fixture;

const CHANGE: &str = "Change duration…  Enter";

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let previous = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No fixed-duration workspace")?
        .path
        .clone();
    let original = d.app().source_cursor;
    let before = document(d)?.clone();
    let workspace = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No qualified freeze workspace")?;
    let picture = workspace
        .plan
        .picture(ProjectFrame(0))
        .map_err(|e| e.to_string())?;
    let (asset, _) = picture
        .picture
        .follow_point()
        .ok_or("The actual baseline picture has no Original source")?;
    let index = workspace
        .sources
        .get(asset)
        .and_then(|source| source.video_index.as_ref())
        .ok_or("The actual Original picture has no qualified index")?;
    let measured = picture
        .picture
        .select_source_frame(index)
        .map_err(|e| e.to_string())?;
    let expected_picture = measured.identity;
    let freeze = deadpan_core::HoldVideo::Freeze {
        asset: asset.clone(),
        timestamp: deadpan_core::SourceTimestamp {
            ticks: measured.pts,
            time_base: index.time_base(),
        },
    };
    let scratch = tempfile::tempdir().map_err(|e| e.to_string())?;
    let ordinary = scratch.path().join("ordinary-fixed-duration.deadpan");
    let cropped = scratch.path().join("cropped-fixed-duration.deadpan");
    fixture::create(&previous, &ordinary, false, freeze.clone())?;
    fixture::create(&previous, &cropped, true, freeze)?;
    let result = (|| {
        open(d, &ordinary, original)?;
        controls(d, expected_picture)?;
        recording(d)?;
        delayed(d, false)?;
        delayed(d, true)?;
        open(d, &cropped, original)?;
        fractional(d)
    })();
    d.app_mut().feedback.hold_project_updates = false;
    let cleanup = (|| {
        d.app().service.submit(ProjectRequest::Close)?;
        d.wait_for("Close the private fixed-duration writer", |app| {
            app.workspace.is_none() && !app.service.is_busy()
        })?;
        open(d, &previous, original)?;
        resize(d, 1280.0, 820.0)?;
        d.check(
            "Fixed-window replay restores the main authored edit and independent Original cursor",
            equivalent(document(d)?, &before)
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
        (Err(error), Err(cleanup)) => Err(format!("{error}; fixed-duration cleanup: {cleanup}")),
        (Err(error), _) | (_, Err(error)) => Err(error),
        _ => Ok(()),
    }
}

fn open(d: &mut Driver<'_>, path: &Path, original: u64) -> Result<(), String> {
    let path = path.canonicalize().map_err(|e| e.to_string())?;
    d.app().service.submit(ProjectRequest::Open(path.clone()))?;
    d.wait_for("Open the qualified fixed-window project", |app| {
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
    settled(d)?;
    scroll_top(d, "Selected beat inspector pane")
}

fn choose(d: &mut Driver<'_>) -> Result<(), String> {
    if d.app().scoped.is_some() {
        d.key(Key::Escape)?;
    }
    d.chord(&[Key::G, Key::G, Key::Enter, Key::Enter, Key::J])?;
    settled(d)?;
    let selected = target(d)?;
    scroll_top(d, "Nested beat inspector pane")?;
    d.check(
        "A visible ordinary Hold has fixed Retime authority without Repeat choices",
        selected.target.node == fixture::id("selected")?
            && selected.target.repeats.is_empty()
            && selected.also.is_empty()
            && !selected.has_single_play_authority()
            && document(d)?
                .nodes()
                .values()
                .all(|node| !matches!(node.kind, NodeKind::Repeat { .. })),
        json!({"Hold":"selected","Repeat_ancestry":0,"several_plays":0}),
        d.snapshot(),
    )
}

fn controls(d: &mut Driver<'_>, expected_picture: SourceFrameId) -> Result<(), String> {
    resize(d, 960.0, 640.0)?;
    choose(d)?;
    painted(d, "Changes apply to this beat inside the Retime.")?;
    let baseline = document(d)?.clone();
    let original = d.app().source_cursor;
    let bank = register_bank(d, None);
    d.chord(&[Key::Num3, Key::L])?;
    let revision = d.revision();
    d.key(Key::Enter)?;
    d.check(
        "Ordinary fixed-window Hold Enter prefills its actual eight-frame duration",
        d.app().command_open
            && d.app().command == "hold-duration 8f"
            && d.revision() == revision
            && d.app().sequence_cursor == 5,
        json!({"command":"hold-duration 8f","Edit":5,"history":"unchanged"}),
        d.snapshot(),
    )?;
    scenarios::footer_anchored(
        d,
        "Fixed-window duration parameter keeps the minimum-size footer",
    )?;
    d.capture("Ordinary fixed Retime Hold duration parameter at minimum size")?;
    submit(d, "hold-duration 10f")?;
    d.changed(&revision)?;
    settled(d)?;
    continuation(d, &baseline, 10, 2, 20)?;
    let lengthened = document(d)?.clone();
    d.click(CHANGE)?;
    let revision = d.revision();
    submit(d, "hold-duration 9f")?;
    d.changed(&revision)?;
    settled(d)?;
    continuation(d, &baseline, 9, 2, 20)?;
    let shortened = document(d)?.clone();
    edit(d, "hold-duration 10f")?;
    continuation(d, &baseline, 10, 2, 20)?;
    let direct = document(d)?.clone();
    d.key(Key::L)?;
    let cursor = d.app().sequence_cursor;
    let captured = target(d)?;
    let revision = d.revision();
    d.command("hold-duration 10f")?;
    settled(d)?;
    d.check("An unchanged ordinary fixed duration preserves history, interior cursor and captured target",
        d.revision() == revision && target(d)? == captured && d.app().sequence_cursor == cursor
            && register_bank(d, None) == bank && d.app().source_cursor == original,
        json!({"history":"unchanged","Edit":cursor,"registers":"unchanged","Original":original}), d.snapshot())?;
    painted(d, CHANGE)?;
    d.capture("Ordinary Retime inspector retains the resized Hold and independent output sound")?;
    undo(d, &shortened)?;
    let revision = d.revision();
    d.key_modified(Key::R, egui::Modifiers::CTRL)?;
    d.changed(&revision)?;
    settled(d)?;
    d.check(
        "Redo restores the complete fixed-output duration edit and preserved sound clock",
        equivalent(document(d)?, &direct),
        json!("same complete authored state"),
        d.snapshot(),
    )?;
    // Return through the three actual successful setters to the fixture baseline.
    undo(d, &shortened)?;
    undo(d, &lengthened)?;
    undo(d, &baseline)?;
    choose(d)?;
    edit(d, "zoom 2 target=center")?;
    d.check(
        "Ordinary nonrecording Zoom retains its direct framing path beside duration-only macro authority",
        document(d)?.nodes()[&fixture::id("selected")?].framing.is_some()
            && recipe(document(d)?)? == recipe(&baseline)?
            && document(d)?.nodes()[&fixture::id("rate")?] == baseline.nodes()[&fixture::id("rate")?]
            && document(d)?.beat_sounds() == baseline.beat_sounds()
            && d.app().sequence_length() == 20 && d.app().source_cursor == original
            && d.app().presentation.displayed_source_frame() == Some(expected_picture),
        json!({"Zoom":"authored","Hold_frames":8,"fixed_output_frames":20,"output_sound":"unchanged"}),
        d.snapshot(),
    )?;
    d.capture("Ordinary fixed-window Original freeze keeps direct nonrecording Zoom")?;
    undo(d, &baseline)
}

fn recording(d: &mut Driver<'_>) -> Result<(), String> {
    choose(d)?;
    let baseline = document(d)?.clone();
    let original = d.app().source_cursor;
    let bank = register_bank(d, Some('w'));
    d.chord(&[Key::Q, Key::W])?;
    let captured = target(d)?;
    let revision = d.revision();
    let all_registers = register_bank(d, None);
    d.command("gain -6")?;
    settled(d)?;
    d.check(
        "Ordinary duration-only recording refuses Gain before authored state or recording changes",
        document(d)? == &baseline && d.revision() == revision && target(d)? == captured
            && register_bank(d, None) == all_registers
            && d.app().macros.recording_name() == Some('w')
            && d.app().macros.instruction_count() == 0 && d.app().gain.is_none()
            && d.app().error.as_deref() == Some("This duration-only macro records pause lengths and named duration calls. Save or cancel recording before another action."),
        json!({"document":"unchanged","registers":"unchanged","recording":"w","instructions":0,"gain_draft":false,"error":"duration-only macro"}),
        d.snapshot(),
    )?;
    d.key(Key::Enter)?;
    d.check(
        "Enter opens the offered duration parameter while ordinary duration-only recording remains active",
        d.app().command_open && d.app().command == "hold-duration 8f"
            && d.app().macros.recording_name() == Some('w')
            && d.app().macros.instruction_count() == 0 && document(d)? == &baseline,
        json!({"command":"hold-duration 8f","recording":"w","instructions":0,"history":"unchanged"}),
        d.snapshot(),
    )?;
    scenarios::footer_anchored(d, "Recording duration Enter keeps the minimum-size footer")?;
    d.capture("Duration-only recording accepts the offered Enter parameter at minimum size")?;
    submit(d, "hold-duration 10f")?;
    d.changed(&revision)?;
    settled(d)?;
    continuation(d, &baseline, 10, 2, 20)?;
    let intent = I::SetHoldDuration {
        length: deadpan_core::PauseLength::Frames {
            frames: NonZeroU32::new(10).unwrap(),
        },
    };
    d.check(
        "Recording acknowledges one successful ordinary fixed-window duration intent",
        d.app().macros.recording_name() == Some('w') && d.app().macros.instruction_count() == 1,
        json!({"recording":"w","instructions":1}),
        d.snapshot(),
    )?;
    d.key(Key::Q)?;
    settled(d)?;
    d.check(
        "Saved fixed-window duration contains length intent without target identities",
        saved(d, 'w') == Some(vec![intent.clone()]) && register_bank(d, Some('w')) == bank,
        json!([intent]),
        d.snapshot(),
    )?;
    d.capture("Ordinary fixed-window duration recording saves one target-free instruction")?;
    undo(d, &baseline)?;
    choose(d)?;
    let revision = d.revision();
    super::super::macros::execute(d, 'w', None)?;
    d.changed(&revision)?;
    settled(d)?;
    continuation(d, &baseline, 10, 2, 20)?;
    undo(d, &baseline)?;
    choose(d)?;
    // Named Run keeps its existing dot policy. A direct parameter commit
    // establishes the duration intent to repeat on another eligible Hold.
    edit(d, "hold-duration 10f")?;
    continuation(d, &baseline, 10, 2, 20)?;
    let direct = document(d)?.clone();
    d.key(Key::K)?;
    let lead = fixture::id("lead")?;
    let repeatable = d
        .app()
        .semantic
        .snapshot()
        .and_then(|snapshot| snapshot.edit_for(d.app().workspace.as_ref()?).ok())
        .is_some_and(|edit| {
            edit.operation == crate::project::semantic::RepeatableEdit::Parameter(intent.clone())
        });
    d.check("Direct duration intent remains the checked repeatable parameter before invocation-relative dot",
        repeatable, json!({"operation":"SetHoldDuration","frames":10}), d.snapshot())?;
    let mut expected_lead = direct.nodes()[&lead].clone();
    let NodeKind::Hold {
        recipe: lead_recipe,
    } = &mut expected_lead.kind
    else {
        return Err("The dot target must be the ordinary Lead Hold".into());
    };
    lead_recipe.duration = deadpan_core::FrameDuration::new(10).map_err(|e| e.to_string())?;
    let revision = d.revision();
    d.key(Key::Period)?;
    d.changed(&revision)?;
    settled(d)?;
    d.check(
        "Invocation-relative dot sets the newly selected Lead Hold and preserves the called Hold",
        target(d)?.target.node == lead && d.app().sequence_cursor == 0
            && document(d)?.nodes()[&lead] == expected_lead
            && document(d)?.nodes()[&fixture::id("selected")?] == direct.nodes()[&fixture::id("selected")?]
            && document(d)?.nodes()[&fixture::id("rate")?] == direct.nodes()[&fixture::id("rate")?]
            && document(d)?.beat_sounds() == direct.beat_sounds()
            && document(d)?.audio_bindings().sound_clocks() == direct.audio_bindings().sound_clocks()
            && d.app().sequence_length() == 20,
        json!({"dot_Hold":"Lead","Lead_frames":10,"called_Hold_frames":10,"Edit":0,"fixed_output_frames":20,"output_sound":"unchanged"}),
        d.snapshot(),
    )?;
    d.check("Macro call and dot resolve the currently selected ordinary Hold without changing other registers",
        saved(d, 'w') == Some(vec![intent]) && register_bank(d, Some('w')) == bank
            && d.app().source_cursor == original,
        json!({"Hold_frames":10,"registers_except_w":"unchanged"}), d.snapshot())?;
    undo(d, &direct)?;
    undo(d, &baseline)
}

fn fractional(d: &mut Driver<'_>) -> Result<(), String> {
    choose(d)?;
    resize(d, 960.0, 640.0)?;
    let baseline = document(d)?.clone();
    let visible = d
        .app()
        .scoped
        .as_ref()
        .ok_or("No cropped inspector")?
        .visible_range(d.app().workspace.as_ref().ok_or("No workspace")?)?
        .ok_or("No visible cropped range")?;
    d.check(
        "Cropped Hold visibility clips intrinsic minus-two-thirds start to exact visible zero",
        visible == (ExactRatio::integer(0)..ExactRatio::new(14, 3).map_err(|e| e.to_string())?),
        json!({"intrinsic_start":"-2/3","visible":["0","14/3"]}),
        d.snapshot(),
    )?;
    d.chord(&[Key::Num2, Key::L])?;
    let revision = d.revision();
    d.key(Key::Enter)?;
    submit(d, "hold-duration 10f")?;
    d.changed(&revision)?;
    settled(d)?;
    continuation(d, &baseline, 10, 0, 10)?;
    d.capture("Cropped fractional Hold duration continues at its visible start")?;
    let lengthened = document(d)?.clone();
    let bank = register_bank(d, None);
    let cursor = d.app().sequence_cursor;
    let selected = target(d)?;
    let revision = d.revision();
    d.command("hold-duration 5f")?;
    settled(d)?;
    d.check("A shrink below the fixed input crop refuses atomically without duration continuation",
        d.revision() == revision && document(d)? == &lengthened && target(d)? == selected
            && d.app().sequence_cursor == cursor && register_bank(d, None) == bank
            && d.app().error.as_ref().is_some_and(|error| !error.is_empty()),
        json!({"proposed_body_frames":17,"required_input_end":18,"document":"unchanged","registers":"unchanged","error":"explicit"}), d.snapshot())?;
    d.capture("Invalid fixed-window Hold shrink preserves the complete authored edit")?;
    undo(d, &baseline)
}

fn delayed(d: &mut Driver<'_>, record: bool) -> Result<(), String> {
    choose(d)?;
    let baseline = document(d)?.clone();
    let original = d.app().source_cursor;
    let bank = register_bank(d, None);
    let previous_macro = saved(d, 'v');
    if record {
        d.chord(&[Key::Q, Key::V])?;
    }
    let revision = d.revision();
    d.app_mut().feedback.hold_project_updates = true;
    let result = (|| {
        d.command("hold-duration 9f")?;
        d.wait_for(
            "Fixed-window duration is durable before away/back navigation",
            |app| !app.service.is_busy(),
        )?;
        let entry = target(d)?;
        if record {
            // Pending recording intentionally blocks keyboard navigation.
            // Actual row selection abandons recording before returning to the
            // same Hold, so its old receipt still cannot reclaim authority.
            d.click("1. Lead · Hold")?;
            d.click("2. Resize this pause · Hold")?;
        } else {
            d.key(Key::K)?;
            d.key(Key::J)?;
        }
        d.key(Key::L)?;
        let newer = target(d)?;
        let cursor = d.app().sequence_cursor;
        d.check(
            "Away/back navigation returns to the same Hold with a distinct continuation generation",
            entry.target == newer.target && entry.navigation != newer.navigation && cursor == 3,
            json!({"same_Hold":true,"navigation":"new generation","Edit":3}),
            d.snapshot(),
        )?;
        d.app_mut().feedback.hold_project_updates = false;
        d.changed(&revision)?;
        settled(d)?;
        d.check("A delayed ordinary duration stays saved but cannot reclaim newer away/back navigation or recording",
            recipe(document(d)?)?.duration.frames() == 9 && d.app().sequence_cursor == cursor
                && d.app().source_cursor == original && !d.app().macros.recording()
                && saved(d, 'v') == previous_macro && register_bank(d, None) == bank
                && d.app().scoped.is_none(),
            json!({"saved_Hold_frames":9,"Edit":cursor,"Original":original,"recording":false,"prior_macro":"preserved","continuation":"revoked"}), d.snapshot())?;
        d.capture(if record {
            "Delayed fixed-window recorded duration preserves away/back navigation"
        } else {
            "Delayed fixed-window duration cannot restore an older inspector cursor"
        })?;
        undo(d, &baseline)
    })();
    d.app_mut().feedback.hold_project_updates = false;
    result
}

fn continuation(
    d: &mut Driver<'_>,
    before: &ProjectDocument,
    length: i64,
    cursor: u64,
    output: u64,
) -> Result<(), String> {
    let selected = target(d)?;
    let after = document(d)?;
    let shown = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No fixed-window workspace")?
        .plan
        .picture(ProjectFrame(
            i64::try_from(cursor).map_err(|e| e.to_string())?,
        ))
        .map_err(|e| e.to_string())?;
    let instance = shown
        .framing
        .iter()
        .find(|layer| layer.instance.node == selected.target.node)
        .map(|layer| &layer.instance)
        .ok_or("Continued frame does not display the selected Hold")?;
    let mut expected = recipe(before)?.clone();
    expected.duration = deadpan_core::FrameDuration::new(length).map_err(|e| e.to_string())?;
    d.check("Duration continuation preserves fixed output, map, provider, exact Hold and independent output sound",
        selected.target.node == fixture::id("selected")? && selected.target.repeats.is_empty()
            && selected.presentation.as_ref() == Some(instance) && d.app().sequence_cursor == cursor
            && recipe(after)? == &expected && d.app().sequence_length() == output
            && after.nodes()[&fixture::id("rate")?] == before.nodes()[&fixture::id("rate")?]
            && after.beat_sounds() == before.beat_sounds()
            && after.assets() == before.assets()
            && after.audio_bindings().sound_clocks() == before.audio_bindings().sound_clocks(),
        json!({"Hold_frames":length,"Edit":cursor,"output_frames":output,"Retime":"unchanged","output_sound":"unchanged"}), d.snapshot())
}
fn recipe(document: &ProjectDocument) -> Result<&HoldRecipe, String> {
    match &document.nodes()[&fixture::id("selected")?].kind {
        NodeKind::Hold { recipe } => Ok(recipe),
        _ => Err("Expected selected fixed-window Hold".into()),
    }
}
fn target(d: &Driver<'_>) -> Result<Target, String> {
    d.app()
        .scoped_target()?
        .ok_or("No ordinary fixed-window Hold target".into())
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
fn equivalent(actual: &ProjectDocument, expected: &ProjectDocument) -> bool {
    let mut value = serde_json::to_value(actual).unwrap();
    value["revision_id"] = json!(expected.revision_id());
    value == serde_json::to_value(expected).unwrap()
}
fn settled(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Fixed-window duration receipt is admitted", |app| {
        !app.service.is_busy() && !app.macros.is_pending()
    })?;
    d.settled()
}
fn scroll_top(d: &mut Driver<'_>, heading: &str) -> Result<(), String> {
    let point = d.rect(heading)?.center() + egui::vec2(0.0, 120.0);
    d.events(
        "Restore the fixed-window inspector controls with the wheel",
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
        d.step("Fixed-window inspector scroll settles", false)?;
    }
    Ok(())
}
fn edit(d: &mut Driver<'_>, command: &str) -> Result<(), String> {
    let revision = d.revision();
    d.command(command)?;
    d.changed(&revision)?;
    settled(d)
}
fn submit(d: &mut Driver<'_>, text: &str) -> Result<(), String> {
    d.key_modified(Key::A, egui::Modifiers::COMMAND)?;
    d.events(
        "Replace the fixed-window Hold-duration parameter",
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
        "One Undo restores the complete prior fixed-window duration state",
        equivalent(document(d)?, expected) && d.app().scoped.is_none(),
        json!("complete prior content restored"),
        d.snapshot(),
    )
}
