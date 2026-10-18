//! Invocation-relative ranged framing, recording and pending entry ownership.
use super::*;
use crate::preview::copied::Content;
use deadpan_core::{SemanticInstruction as I, SemanticZoomCenter, framing_zoom::ZoomKind};

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(960.0, 640.0));
    let input = d.harness.input_mut();
    input.screen_rect = Some(viewport);
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .ok_or("Missing ranged framing recording viewport")?
        .inner_rect = Some(viewport);
    d.step("Record ranged framing at the minimum workspace size", true)?;
    d.key(Key::Escape)?;
    let revision = d.revision();
    d.command("zoom off")?;
    d.changed(&revision)?;
    idle(d)?;
    let baseline = document(d)?.clone();
    let original = d.app().source_cursor;
    ranged_smash(d, &baseline)?;
    ranged_creep(d, &baseline)?;
    sampled_refusal(d)?;
    dot(d, &baseline)?;
    pending_escape(d, &baseline)?;
    d.check(
        "Ranged framing retains the independent Original clock and complete authored baseline",
        d.app().source_cursor == original && equivalent(document(d)?, &baseline),
        json!({"Original_cursor":original,"baseline":"restored"}),
        d.snapshot(),
    )
}

fn document<'a>(d: &'a Driver<'_>) -> Result<&'a deadpan_core::ProjectDocument, String> {
    d.app()
        .workspace
        .as_ref()
        .map(|workspace| workspace.document.as_ref())
        .ok_or("Missing zoom workspace".into())
}

fn equivalent(
    actual: &deadpan_core::ProjectDocument,
    expected: &deadpan_core::ProjectDocument,
) -> bool {
    let mut actual = serde_json::to_value(actual).unwrap();
    actual["revision_id"] = json!(expected.revision_id());
    actual == serde_json::to_value(expected).unwrap()
}

fn idle(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Semantic framing receipt finishes", |app| {
        !app.service.is_busy() && !app.macros.is_pending() && app.camera_pending.is_none()
    })?;
    d.settled()
}

fn motion(d: &mut Driver<'_>, frames: u64, forward: bool) -> Result<(), String> {
    if frames == 0 {
        return Ok(());
    }
    for digit in frames.to_string().bytes() {
        d.key(match digit {
            b'0' => Key::Num0,
            b'1' => Key::Num1,
            b'2' => Key::Num2,
            b'3' => Key::Num3,
            b'4' => Key::Num4,
            b'5' => Key::Num5,
            b'6' => Key::Num6,
            b'7' => Key::Num7,
            b'8' => Key::Num8,
            b'9' => Key::Num9,
            _ => unreachable!(),
        })?;
    }
    d.key(if forward { Key::L } else { Key::H })
}

fn navigate(d: &mut Driver<'_>, frame: u64) -> Result<(), String> {
    d.key(Key::Escape)?;
    d.chord(&[Key::G, Key::G])?;
    motion(d, frame, true)?;
    d.settled()
}

fn select(d: &mut Driver<'_>, anchor: u64, head: u64) -> Result<(), String> {
    navigate(d, anchor)?;
    d.key(Key::V)?;
    motion(d, anchor.abs_diff(head), head > anchor)?;
    d.key(Key::V)?;
    d.settled()
}

fn undo(d: &mut Driver<'_>, expected: &deadpan_core::ProjectDocument) -> Result<(), String> {
    let revision = d.revision();
    d.key(Key::U)?;
    d.changed(&revision)?;
    idle(d)?;
    d.check(
        "One Undo restores every authored field after semantic ranged framing",
        equivalent(document(d)?, expected),
        json!("full document restored"),
        d.snapshot(),
    )
}

fn program(d: &Driver<'_>, register: char) -> Option<Vec<I>> {
    d.app()
        .copied
        .entries()
        .find_map(|(name, content)| match content {
            Content::Macro(program) if name == register => Some(program.instructions().to_vec()),
            _ => None,
        })
}

fn save(d: &mut Driver<'_>, register: char, expected: Vec<I>) -> Result<(), String> {
    d.key(Key::Q)?;
    d.wait_for("Ranged framing macro is saved", |app| {
        !app.service.is_busy() && !app.macros.is_pending() && !app.macros.recording()
    })?;
    d.settled()?;
    d.check(
        "The ranged macro saves typed intent without recording range fractions or an owner",
        program(d, register) == Some(expected.clone()),
        json!(expected),
        json!(program(d, register)),
    )
}

fn ranged_smash(
    d: &mut Driver<'_>,
    baseline: &deadpan_core::ProjectDocument,
) -> Result<(), String> {
    select(d, 10, 20)?;
    d.chord(&[Key::Q, Key::R])?;
    let revision = d.revision();
    d.command("zoom 2 target=center")?;
    d.changed(&revision)?;
    idle(d)?;
    d.check(
        "A recorded smash changes only its exact half-open range",
        scale_of(planned_pose(d, 9)) == 1.0
            && scale_of(planned_pose(d, 10)) == 2.0
            && scale_of(planned_pose(d, 19)) == 2.0
            && scale_of(planned_pose(d, 20)) == 1.0
            && d.app().macros.instruction_count() == 1
            && d.app()
                .selected_edit_range()
                .is_some_and(|r| r.start() == ProjectFrame(10) && r.end() == ProjectFrame(20)),
        json!({"inside":[10,20],"scale":2,"outside":1,"instructions":1}),
        d.snapshot(),
    )?;
    d.capture("Recording a ranged zoom retains its selection and semantic intent")?;
    save(
        d,
        'r',
        vec![I::Zoom {
            kind: ZoomKind::Smash {
                scale: ExactRatio::integer(2),
            },
            center: SemanticZoomCenter::OriginalCenter,
        }],
    )?;
    undo(d, baseline)?;
    select(d, 50, 30)?;
    let revision = d.revision();
    super::super::macros::execute(d, 'r', None)?;
    d.changed(&revision)?;
    idle(d)?;
    d.check(
        "The saved ranged zoom resolves a new backward range and retains its orientation",
        scale_of(planned_pose(d, 29)) == 1.0
            && scale_of(planned_pose(d, 30)) == 2.0
            && scale_of(planned_pose(d, 49)) == 2.0
            && scale_of(planned_pose(d, 50)) == 1.0
            && d.app().capture_visual_selection().ok().flatten()
                == Some(deadpan_core::SemanticVisualSelection::Time {
                    anchor: ProjectFrame(50),
                    head: ProjectFrame(30),
                    extending: false,
                }),
        json!({"range":[30,50],"anchor":50,"head":30,"outside":1}),
        d.snapshot(),
    )?;
    d.capture("Replayed ranged zoom uses the new backward invocation selection")?;
    undo(d, baseline)
}

fn ranged_creep(
    d: &mut Driver<'_>,
    baseline: &deadpan_core::ProjectDocument,
) -> Result<(), String> {
    select(d, 40, 60)?;
    d.chord(&[Key::Q, Key::S])?;
    let revision = d.revision();
    d.command("creep from=1 to=1.4")?;
    d.changed(&revision)?;
    idle(d)?;
    save(
        d,
        's',
        vec![I::Zoom {
            kind: ZoomKind::Creep {
                from: Some(ExactRatio::ONE),
                to: ExactRatio::new(7, 5).unwrap(),
                curve: deadpan_core::framing_zoom::ZoomCurve::Smoothstep,
            },
            center: SemanticZoomCenter::Keep,
        }],
    )?;
    undo(d, baseline)?;
    select(d, 70, 90)?;
    let revision = d.revision();
    super::super::macros::execute(d, 's', None)?;
    d.changed(&revision)?;
    idle(d)?;
    d.check(
        "A ranged creep arrives at the new Out and holds its end pose afterward",
        scale_of(planned_pose(d, 69)) == 1.0
            && scale_of(planned_pose(d, 70)) < 1.01
            && (scale_of(planned_pose(d, 90)) - 1.4).abs() < 1e-6
            && (scale_of(planned_pose(d, 119)) - 1.4).abs() < 1e-6,
        json!({"range":[70,90],"after_scale":1.4}),
        d.snapshot(),
    )?;
    d.capture("A replayed ranged creep holds its new arrival pose")?;
    undo(d, baseline)
}

fn sampled_refusal(d: &mut Driver<'_>) -> Result<(), String> {
    select(d, 5, 15)?;
    d.chord(&[Key::Q, Key::T])?;
    let revision = d.revision();
    for command in ["zoom 2 target=current", "creep to=1.4 target=face:1"] {
        d.command(command)?;
        d.wait_for("Sampled ranged recording reports its restriction", |app| {
            app.camera_pending.is_none()
        })?;
        d.settled()?;
        d.check(
            "A sampled-target range refuses recording without edits or an instruction",
            d.revision() == revision
                && d.app().macros.recording_name() == Some('t')
                && d.app().macros.instruction_count() == 0
                && d.app()
                    .error
                    .as_deref()
                    .is_some_and(|e| e.contains("cannot sample")),
            json!({"command":command,"instructions":0,"saved":"unchanged"}),
            d.snapshot(),
        )?;
    }
    d.capture("Ranged macro recording explains its sampled-target restriction")?;
    d.command("record-cancel")?;
    d.key(Key::Escape)?;
    Ok(())
}

fn dot(d: &mut Driver<'_>, baseline: &deadpan_core::ProjectDocument) -> Result<(), String> {
    navigate(d, 60)?;
    let revision = d.revision();
    d.key(Key::S)?;
    d.changed(&revision)?;
    idle(d)?;
    let split = document(d)?.clone();
    select(d, 10, 20)?;
    let revision = d.revision();
    d.command("zoom 2 target=center")?;
    d.changed(&revision)?;
    idle(d)?;
    let first = document(d)?.clone();
    select(d, 70, 80)?;
    let revision = d.revision();
    d.key(Key::Period)?;
    d.changed(&revision)?;
    idle(d)?;
    d.check(
        "Dot applies the same ranged zoom intent to another beat at its new local range",
        scale_of(planned_pose(d, 69)) == 1.0
            && scale_of(planned_pose(d, 70)) == 2.0
            && scale_of(planned_pose(d, 79)) == 2.0
            && scale_of(planned_pose(d, 80)) == 1.0,
        json!({"second_beat_range":[70,80],"outside":1,"inside":2}),
        d.snapshot(),
    )?;
    undo(d, &first)?;
    undo(d, &split)?;
    undo(d, baseline)
}

fn pending_escape(
    d: &mut Driver<'_>,
    baseline: &deadpan_core::ProjectDocument,
) -> Result<(), String> {
    for (recording, ranged) in [(false, false), (true, false), (true, true)] {
        navigate(d, 0)?;
        if ranged {
            select(d, 5, 15)?;
        }
        let revision = d.revision();
        let bank = d.app().copied.bank_version();
        if recording {
            d.chord(&[Key::Q, Key::U])?;
        }
        d.app_mut().feedback.hold_preview = true;
        motion(d, 35, true)?;
        d.command("zoom 1.75 target=center")?;
        d.wait_for(
            "Retain the real stopped-frame reply before zoom admission",
            |app| app.feedback.held_reply.is_some(),
        )?;
        d.check(
            "Zoom waits for its exact picture without changing the captured selection",
            d.app().camera_pending.is_some()
                && d.app().selected_edit_range().is_some() == ranged
                && d.revision() == revision,
            json!({"pending":true,"ranged":ranged,"saved":"unchanged"}),
            d.snapshot(),
        )?;
        if recording {
            d.key(Key::Q)?;
            d.check(
                "A recording cannot save before its pending framing instruction finishes",
                d.app().macros.recording_name() == Some('u')
                    && d.app().macros.instruction_count() == 1
                    && !d.app().macros.is_pending()
                    && program(d, 'u').is_none()
                    && d.app().copied.bank_version() == bank
                    && d.app()
                        .error
                        .as_deref()
                        .is_some_and(|e| e.contains("pending framing")),
                json!({"recording":"u","instructions":1,"saved":"unchanged"}),
                d.snapshot(),
            )?;
        }
        d.key(Key::Escape)?;
        let feedback = &mut d.app_mut().feedback;
        feedback.release_reply = feedback.held_reply.take();
        feedback.hold_preview = false;
        idle(d)?;
        d.check(
            "Escape cancels pending Zoom and recording before the delayed picture arrives",
            d.revision() == revision
                && d.app().camera_pending.is_none()
                && !d.app().macros.recording()
                && d.app().copied.bank_version() == bank
                && program(d, 'u').is_none()
                && !d
                    .app()
                    .error
                    .as_deref()
                    .is_some_and(|e| e.contains("pending framing"))
                && !d.app().message.as_deref().is_some_and(|message| {
                    message.contains("Preparing the stopped picture for framing")
                })
                && framing(d).is_none()
                && equivalent(document(d)?, baseline),
            json!({"pending":false,"framing":null,"saved":"unchanged"}),
            d.snapshot(),
        )?;
        d.capture(
            "Cancelled pending zoom leaves the edit unchanged after delayed picture delivery",
        )?;
    }
    Ok(())
}
