//! Invocation authority comes from the play chosen now, never the saved body.
use super::*;
use crate::preview::copied::Content;
use deadpan_core::{
    ExactRatio, SemanticInstruction as I, SemanticZoomCenter,
    framing_zoom::{ZoomCurve, ZoomKind},
};

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let baseline = document(d)?.clone();
    let original = d.app().source_cursor;
    resize(d, 960.0, 640.0)?;
    choose(d, 3)?;
    d.chord(&[Key::Q, Key::J])?;
    let revision = d.revision();
    let camera = format!("Camera…  {}", d.app().editor_key(EditorKey::Camera));
    d.click(&camera)?;
    d.check(
        "The Camera button obeys the same one-play recording gate as its key",
        d.app().camera.is_none()
            && d.app().camera_pending.is_none()
            && d.app().macros.recording_name() == Some('j')
            && d.app().macros.instruction_count() == 0
            && d.revision() == revision
            && d.app().error.as_deref() == Some("A one-play macro records freeze and black pauses, pause lengths, zooms, creeps and named calls. Save or cancel recording before another action."),
        json!({"camera":"closed","instructions":0,"authored":"unchanged"}),
        d.snapshot(),
    )?;
    d.command("zoom 2 target=center")?;
    d.changed(&revision)?;
    settled(d)?;
    let target = d.app().scoped_target()?.ok_or("Zoom lost its play")?;
    d.check(
        "A recorded punch-in isolates only the chosen play and keeps its root clock",
        d.app().macros.recording_name() == Some('j')
            && d.app().macros.instruction_count() == 1
            && d.app().sequence_cursor == 240
            && scale(d, 0)? == ExactRatio::ONE
            && scale(d, 120)? == ExactRatio::ONE
            && scale(d, 240)? == ExactRatio::integer(2)
            && target
                .presentation
                .as_ref()
                .is_some_and(|p| p.node == target.target.node),
        json!({"recording":"j","instructions":1,"Edit":240,"scales":[1,1,2]}),
        d.snapshot(),
    )?;
    painted(d, "This play 3/3")?;
    painted(d, "punch in / creep")?;
    scenarios::footer_anchored(d, "One-play recording controls fit the minimum workspace")?;
    d.capture("One-play framing records intent while retaining the selected play")?;
    save(
        d,
        'j',
        I::Zoom {
            kind: ZoomKind::Smash {
                scale: ExactRatio::integer(2),
            },
            center: SemanticZoomCenter::OriginalCenter,
        },
    )?;
    undo(d, &baseline)?;

    choose(d, 1)?;
    let bank = d.app().copied.bank_version();
    let revision = d.revision();
    super::super::macros::execute(d, 'j', None)?;
    d.changed(&revision)?;
    settled(d)?;
    d.check("The saved punch-in resolves the invocation's first play instead of its recorded third play",
        d.app().scoped.is_some() && d.app().sequence_cursor == 0
            && scale(d, 0)? == ExactRatio::integer(2) && scale(d, 120)? == ExactRatio::ONE
            && scale(d, 240)? == ExactRatio::ONE && d.app().copied.bank_version() == bank,
        json!({"Edit":0,"scales":[2,1,1],"bank":"unchanged"}),d.snapshot())?;
    d.capture("A saved framing macro applies to the newly selected first play")?;
    undo(d, &baseline)?;
    choose(d, 1)?;
    let revision = d.revision();
    d.command("zoom 2 target=center")?;
    d.changed(&revision)?;
    settled(d)?;
    let first = document(d)?.clone();
    choose(d, 3)?;
    let revision = d.revision();
    d.key(Key::Period)?;
    d.changed(&revision)?;
    settled(d)?;
    d.check("Dot captures the newly chosen third play and preserves the first play's independent framing",
        d.app().scoped.is_some() && d.app().sequence_cursor == 240
            && scale(d, 0)? == ExactRatio::integer(2) && scale(d, 120)? == ExactRatio::ONE
            && scale(d, 240)? == ExactRatio::integer(2) && d.app().copied.bank_version() == bank,
        json!({"Edit":240,"scales":[2,1,2],"bank":"unchanged"}),d.snapshot())?;
    d.capture("Dot repeats framing only on the play selected now")?;
    undo(d, &first)?;
    undo(d, &baseline)?;

    choose(d, 3)?;
    d.chord(&[Key::Q, Key::K])?;
    let revision = d.revision();
    d.command("creep from=1 to=1.4 target=center")?;
    d.changed(&revision)?;
    settled(d)?;
    save(
        d,
        'k',
        I::Zoom {
            kind: ZoomKind::Creep {
                from: Some(ExactRatio::ONE),
                to: ExactRatio::new(7, 5).unwrap(),
                curve: ZoomCurve::Smoothstep,
            },
            center: SemanticZoomCenter::OriginalCenter,
        },
    )?;
    undo(d, &baseline)?;
    choose(d, 1)?;
    let revision = d.revision();
    super::super::macros::execute(d, 'k', None)?;
    d.changed(&revision)?;
    settled(d)?;
    let target = d.app().scoped_target()?.ok_or("Creep lost its play")?;
    let framing = document(d)?.nodes()[&target.target.node]
        .framing
        .as_ref()
        .ok_or("Creep lost its envelope")?;
    let endpoint = framing
        .evaluate_exact(ExactRatio::integer(120), ExactRatio::integer(120))
        .map_err(|error| error.to_string())?;
    let last_picture = scale(d, 119)?;
    d.check("A saved creep uses the new play's intrinsic owner clock and leaves the other plays unchanged",
        d.app().scoped.is_some() && last_picture.compare_integer(1).is_gt()
            && last_picture.checked_sub(ExactRatio::new(7,5).unwrap()).map_err(|e|e.to_string())?.compare_integer(0).is_lt()
            && endpoint.scale == ExactRatio::new(7,5).unwrap()
            && scale(d,120)? == ExactRatio::ONE && scale(d,240)? == ExactRatio::ONE,
        json!({"last_picture":"strictly between 1 and 7/5","endpoint":120,"endpoint_scale":"7/5","other_plays":1}),d.snapshot())?;
    d.capture("A replayed one-play creep retains its own owner clock")?;
    undo(d, &baseline)?;

    refuse(d, &baseline)?;
    late(d, &baseline)?;
    late_round_trip(d, &baseline)?;
    d.check("One-play framing verification restores every authored field and the independent Original cursor",
        equivalent(document(d)?, &baseline) && d.app().source_cursor == original,
        json!({"authored_baseline":"restored","Original":original}),d.snapshot())?;
    resize(d, 1280.0, 820.0)
}

fn choose(d: &mut Driver<'_>, play: u32) -> Result<(), String> {
    if d.app().scoped.is_none() {
        d.key(Key::Enter)?;
    }
    d.command(&format!("scope play {play}"))?;
    settled(d)
}

fn settled(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("One-play semantic receipt is admitted", |app| {
        !app.service.is_busy() && !app.macros.is_pending() && app.camera_pending.is_none()
    })?;
    d.settled()
}

fn equivalent(actual: &ProjectDocument, expected: &ProjectDocument) -> bool {
    let mut actual = serde_json::to_value(actual).unwrap();
    actual["revision_id"] = json!(expected.revision_id());
    actual == serde_json::to_value(expected).unwrap()
}

fn undo(d: &mut Driver<'_>, expected: &ProjectDocument) -> Result<(), String> {
    let revision = d.revision();
    d.key(Key::U)?;
    d.changed(&revision)?;
    settled(d)?;
    d.check(
        "One Undo restores the complete document and removes only its framing isolation",
        equivalent(document(d)?, expected) && d.app().scoped.is_none(),
        json!({"document":"restored","inspector":"closed"}),
        d.snapshot(),
    )
}

fn scale(d: &Driver<'_>, frame: i64) -> Result<ExactRatio, String> {
    let workspace = d
        .app()
        .workspace
        .as_ref()
        .ok_or("Missing scoped workspace")?;
    let picture = workspace
        .plan
        .picture(ProjectFrame(frame))
        .map_err(|e| e.to_string())?;
    let layer = picture
        .framing
        .iter()
        .find(|layer| {
            !layer.escalation
                && matches!(
                    workspace.document.nodes()[&layer.instance.node].kind,
                    NodeKind::Source { .. }
                )
        })
        .ok_or("No scoped Source layer")?;
    Ok(layer.pose.unwrap_or_default().scale)
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

fn save(d: &mut Driver<'_>, name: char, instruction: I) -> Result<(), String> {
    d.key(Key::Q)?;
    d.wait_for(
        "Save the framing macro without concrete play authority",
        |app| !app.service.is_busy() && !app.macros.is_pending() && !app.macros.recording(),
    )?;
    d.settled()?;
    d.check(
        "The saved one-play program contains typed framing intent without owner or play IDs",
        saved(d, name) == Some(vec![instruction.clone()]),
        json!([instruction]),
        d.snapshot(),
    )
}

fn refuse(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    choose(d, 3)?;
    d.chord(&[Key::Q, Key::L])?;
    let target = d.app().scoped_target()?;
    let bank = d.app().copied.bank_version();
    let revision = d.revision();
    d.command("zoom 2 target=current")?;
    settled(d)?;
    d.check(
        "Sampled-target framing refuses without changing a one-play recording or its authority",
        d.revision() == revision
            && d.app().macros.recording_name() == Some('l')
            && d.app().macros.instruction_count() == 0
            && d.app().scoped_target()? == target
            && d.app().copied.bank_version() == bank
            && d.app()
                .error
                .as_deref()
                .is_some_and(|e| e.contains("cannot sample")),
        json!({"instructions":0,"play":"unchanged","saved":"unchanged"}),
        d.snapshot(),
    )?;
    d.capture("One-play recording explains its sampled-target restriction")?;
    d.key(Key::J)?;
    d.check(
        "Unsupported child navigation cannot become hidden one-play macro authority",
        d.app().scoped_target()? == target
            && d.app().macros.recording_name() == Some('l')
            && d.app().macros.instruction_count() == 0,
        json!({"target":"unchanged","instructions":0}),
        d.snapshot(),
    )?;
    d.key(Key::Escape)?;
    settled(d)?;
    d.key(Key::Enter)?;
    d.command("scope all")?;
    settled(d)?;
    d.chord(&[Key::Q, Key::M])?;
    d.check(
        "All plays cannot supply a single-play macro's missing authoring authority",
        !d.app().macros.recording()
            && equivalent(document(d)?, baseline)
            && d.app().copied.bank_version() == bank
            && d.app()
                .error
                .as_deref()
                .is_some_and(|e| e.contains("This play at every Repeat")),
        json!({"recording":false,"saved":"unchanged"}),
        d.snapshot(),
    )?;
    d.capture("Framing macros require explicit This play authority")?;
    d.key(Key::Escape)?;
    Ok(())
}

fn late(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    choose(d, 3)?;
    let bank = d.app().copied.bank_version();
    d.chord(&[Key::Q, Key::N])?;
    d.app_mut().feedback.hold_project_updates = true;
    d.command("zoom 1.75 target=center")?;
    d.wait_for(
        "The scoped macro edit commits while its continuation is withheld",
        |app| !app.service.is_busy(),
    )?;
    let previous = format!("Previous  {}", d.app().editor_key(EditorKey::PlayPrevious));
    d.click(&previous)?;
    let cursor = d.app().sequence_cursor;
    d.app_mut().feedback.hold_project_updates = false;
    settled(d)?;
    d.check("A delayed framing receipt cannot reclaim a newly selected play or append into an abandoned recording",
        cursor==120 && d.app().sequence_cursor==cursor && d.app().scoped.is_none()
            && !d.app().macros.recording() && saved(d,'n').is_none()
            && d.app().copied.bank_version()==bank && scale(d,240)?==ExactRatio::new(7,4).unwrap(),
        json!({"Edit":120,"old_play_zoom":"7/4","recording":false,"saved_macro":"unchanged"}),d.snapshot())?;
    d.capture("Delayed one-play framing preserves the newer navigation")?;
    undo(d, baseline)
}

fn late_round_trip(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    choose(d, 3)?;
    let target = d.app().scoped_target()?;
    let bank = d.app().copied.bank_version();
    d.app_mut().feedback.hold_project_updates = true;
    super::super::macros::execute(d, 'j', None)?;
    d.wait_for(
        "A non-recording framing macro commits before its receipt arrives",
        |app| !app.service.is_busy(),
    )?;
    let events = [Key::OpenBracket, Key::R, Key::CloseBracket, Key::R]
        .into_iter()
        .flat_map(|key| {
            [
                super::super::key_event(key, egui::Modifiers::NONE, true),
                super::super::key_event(key, egui::Modifiers::NONE, false),
            ]
        })
        .collect();
    d.events(
        "Visit another play and return in one native event batch",
        events,
    )?;
    let returned = d.app().scoped_target()?;
    let mut same_address = target.clone();
    if let (Some(expected), Some(actual)) = (&mut same_address, &returned) {
        expected.navigation = actual.navigation;
    }
    d.check(
        "Same-batch play navigation returns to the old address before the receipt arrives",
        returned == same_address && returned != target && d.app().sequence_cursor == 240,
        json!({"Edit":240,"target":"same address"}),
        d.snapshot(),
    )?;
    d.app_mut().feedback.hold_project_updates = false;
    settled(d)?;
    d.check(
        "Returning to a play cannot restore a queued macro's revoked continuation",
        d.app().scoped.is_none()
            && d.app().sequence_cursor == 240
            && d.app().copied.bank_version() == bank
            && scale(d, 240)? == ExactRatio::integer(2),
        json!({"Edit":240,"inspector":"closed","committed_zoom":2,"bank":"unchanged"}),
        d.snapshot(),
    )?;
    d.capture("Returning to the same play cannot revive delayed framing authority")?;
    undo(d, baseline)
}
