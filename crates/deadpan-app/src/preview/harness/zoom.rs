//! Specification §7.5 `,z`/`,c`, §6.4 `:zoom`/`:creep` and §8.2 smash zoom,
//! slow creep, abrupt return and black-frame punctuation, through the
//! production keys, command line, project service and shared picture plan.

use deadpan_core::{ExactRatio, Framing, FramingPose, FramingValue, NodeKind, ProjectFrame};
use egui::Key;

use super::*;

fn framing(d: &Driver<'_>) -> Option<Framing> {
    let node = d.app().selected_beat.clone()?;
    d.app().workspace.as_ref()?.document.nodes()[&node]
        .framing
        .clone()
}

/// The selected beat's pose on the displayed picture.
fn shown_pose(d: &Driver<'_>) -> Option<FramingPose> {
    let selected = d.app().selected_beat.clone()?;
    d.app()
        .presentation
        .picture()?
        .framing
        .iter()
        .find(|layer| !layer.escalation && layer.instance.node == selected)?
        .pose
}

/// The selected beat's evaluated pose at `frame` in the committed plan.
fn planned_pose(d: &Driver<'_>, frame: i64) -> Option<FramingPose> {
    let selected = d.app().selected_beat.clone()?;
    d.app()
        .workspace
        .as_ref()?
        .plan
        .picture(ProjectFrame(frame))
        .ok()?
        .framing
        .iter()
        .find(|layer| layer.instance.node == selected)?
        .pose
}

fn approx(value: ExactRatio) -> f64 {
    value.numerator() as f64 / value.denominator() as f64
}

fn scale_of(pose: Option<FramingPose>) -> f64 {
    pose.map_or(1.0, |pose| approx(pose.scale))
}

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Shot analysis stored for the Original", |app| {
        app.workspace
            .as_ref()
            .is_some_and(|workspace| workspace.shot_analysis.is_some())
    })?;
    super::transcript::focus_your_edit(d)?;
    d.chord(&[Key::G, Key::G, Key::Num1, Key::Num0, Key::L])?;
    d.settled()?;
    let duration = d.app().sequence_length();

    // ,z without a saved target punches in at the current center and says so.
    let start = d.revision();
    d.chord(&[Key::Comma, Key::Z])?;
    d.changed(&start)?;
    d.settled()?;
    let punched = framing(d);
    d.check(
        ",z without a target punches in to 1.35x at the current center and explains why",
        matches!(&punched, Some(Framing { value: FramingValue::Static { pose }, .. })
            if pose.scale == ExactRatio::new(27, 20).unwrap() && pose.center_x == ExactRatio::new(1, 2).unwrap())
            && d.app().message.as_deref().is_some_and(|message| {
                message.starts_with("Framing saved: 1.350× static")
                    && message.contains("No saved target covers this picture")
            }),
        json!({"framing":"Static 1.35 at center","message":"Framing saved: 1.350× static. No saved target covers this picture…"}),
        json!({"framing":format!("{punched:?}"),"message":d.app().message}),
    )?;
    d.capture("Punch-in at the center without a target")?;
    let punched_revision = d.revision();
    d.key(Key::U)?;
    d.changed(&punched_revision)?;
    d.settled()?;

    // Draw a target right of center in Camera.
    let unframed = d.revision();
    d.chord(&[Key::Comma, Key::F])?;
    d.wait_for("Camera opens", |app| app.camera.is_some())?;
    d.key(Key::N)?;
    d.chord(&[Key::Num8, Key::L])?;
    d.key(Key::Enter)?;
    d.wait_for("Target saved", |app| {
        app.workspace
            .as_ref()
            .is_some_and(|workspace| !workspace.document.targets().is_empty())
            && app.camera.as_ref().is_some_and(|camera| {
                let state = camera.harness_state();
                state["saving"] == false && state["rebasing"] == false
            })
    })?;
    d.key(Key::Escape)?;
    d.settled()?;
    let with_target = d.revision();
    d.check(
        "Camera saves one target and Escape leaves the beat unframed",
        with_target != unframed && framing(d).is_none() && d.app().camera.is_none(),
        json!({"framing":null,"camera":false}),
        json!({"framing":format!("{:?}", framing(d)),"camera":d.app().camera.is_some()}),
    )?;

    // ,z on the selected target: a step to 1.35x that follows it.
    d.chord(&[Key::Comma, Key::Z])?;
    d.changed(&with_target)?;
    d.settled()?;
    let followed = framing(d);
    let shown = shown_pose(d);
    d.check(
        ",z punches in to 1.35x following the only saved target in the picture",
        matches!(&followed, Some(Framing { value: FramingValue::Follow { target, scale, .. }, .. })
            if target.as_str() == "target-1" && *scale == ExactRatio::new(27, 20).unwrap())
            && shown.is_some_and(|pose| {
                // Resolved follows are quantized to the framing grid.
                (approx(pose.center_x) - 0.58).abs() < 0.01 && (approx(pose.scale) - 1.35).abs() < 1e-6
            })
            && d.app().message.as_deref() == Some("Framing saved: 1.350× following Target 1")
            && d.app().sequence_length() == duration,
        json!({"framing":"Follow target-1 at 1.35","center_x":0.58,"message":"Framing saved: 1.350× following Target 1","frames":duration}),
        json!({"framing":format!("{followed:?}"),"shown":format!("{shown:?}"),"message":d.app().message,"frames":d.app().sequence_length()}),
    )?;
    let widgets = d.widgets().to_string();
    d.check(
        "The inspector names the followed target and offers framing presets",
        widgets.contains("Target 1 · 1.35×") && widgets.contains("Framing presets"),
        json!(["Target 1 · 1.35×", "Framing presets"]),
        json!({"followed":widgets.contains("Target 1 · 1.35×"),"presets":widgets.contains("Framing presets")}),
    )?;
    d.capture("Punch-in follows Target 1")?;

    // :zoom … target=center curve=step: a smash zoom centered on the Original.
    let revision = d.revision();
    d.command("zoom 2 target=center curve=step")?;
    d.changed(&revision)?;
    d.settled()?;
    let smash = framing(d);
    d.check(
        ":zoom 2 target=center replaces the follow with a centered 2x step",
        matches!(&smash, Some(Framing { value: FramingValue::Static { pose }, .. })
            if pose.scale == ExactRatio::integer(2) && pose.center_x == ExactRatio::new(1, 2).unwrap()),
        json!("Static 2x at (0.5, 0.5)"),
        json!(format!("{smash:?}")),
    )?;

    // :creep toward the current target over the whole beat.
    let revision = d.revision();
    d.command("creep from=1 to=1.4 target=current")?;
    d.changed(&revision)?;
    d.settled()?;
    let creep = framing(d);
    let (first, last) = (planned_pose(d, 0), planned_pose(d, duration as i64 - 1));
    d.check(
        ":creep eases from 1x to 1.4x toward the target's position at this picture",
        matches!(&creep, Some(Framing { value: FramingValue::Envelope { envelope }, .. })
            if envelope.initial.scale == ExactRatio::ONE
                && envelope.segments.last().is_some_and(|segment| {
                    segment.pose.scale == ExactRatio::new(7, 5).unwrap()
                        && (approx(segment.pose.center_x) - 0.58).abs() < 0.01
                }))
            && scale_of(first) < 1.01
            && scale_of(last) > 1.39
            && d.app().sequence_length() == duration,
        json!({"initial":1.0,"final":1.4,"final_center_x":0.58,"frames":duration}),
        json!({"framing":format!("{creep:?}"),"first":format!("{first:?}"),"last":format!("{last:?}"),"frames":d.app().sequence_length()}),
    )?;
    d.capture("Creep toward Target 1")?;
    let revision = d.revision();
    d.chord(&[Key::Comma, Key::C])?;
    d.settled()?;
    d.check(
        ",c without a named target refuses to replace an existing camera path",
        d.revision() == revision
            && framing(d) == creep
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("camera path")),
        json!("This beat has a camera path, and this would replace it…"),
        json!({"revision_unchanged":d.revision() == revision,"error":d.app().error}),
    )?;

    // A range cannot flatten the creep; :zoom off is an abrupt return.
    d.key(Key::V)?;
    d.chord(&[Key::Num5, Key::L])?;
    d.settled()?;
    let range = d.app().selected_edit_range().ok_or("No Edit range")?;
    let revision = d.revision();
    d.chord(&[Key::Comma, Key::Z])?;
    d.settled()?;
    d.check(
        "A ranged punch-in refuses to flatten an existing camera path",
        d.revision() == revision
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("camera path")),
        json!("This beat has a camera path or follow…"),
        json!({"revision_unchanged":d.revision() == revision,"error":d.app().error}),
    )?;
    d.key(Key::Escape)?;
    d.settled()?;
    d.command("zoom off")?;
    d.changed(&revision)?;
    d.settled()?;
    d.check(
        ":zoom off returns the whole beat to the full picture",
        framing(d).is_none()
            && d.app().message.as_deref() == Some("Framing saved: full picture, no framing"),
        json!({"framing":null,"message":"Framing saved: full picture, no framing"}),
        json!({"framing":format!("{:?}", framing(d)),"message":d.app().message}),
    )?;

    // A Visual range inside the beat: ,z punches in only there.
    d.key(Key::V)?;
    d.chord(&[Key::Num5, Key::L])?;
    d.settled()?;
    let range = d.app().selected_edit_range().unwrap_or(range);
    let (start, end) = (range.start().0, range.end().0);
    let revision = d.revision();
    d.chord(&[Key::Comma, Key::Z])?;
    d.changed(&revision)?;
    d.settled()?;
    let poses: Vec<f64> = [start - 1, start, end - 1, end]
        .into_iter()
        .map(|frame| scale_of(planned_pose(d, frame)))
        .collect();
    d.check(
        "With an Edit range, ,z steps to 1.35x on the target inside it and returns after",
        matches!(framing(d), Some(Framing { value: FramingValue::Envelope { .. }, .. }))
            && (poses[0] - 1.0).abs() < 1e-9
            && (poses[1] - 1.35).abs() < 1e-6
            && (poses[2] - 1.35).abs() < 1e-6
            && (poses[3] - 1.0).abs() < 1e-9
            && d.app().message.as_deref().is_some_and(|message| {
                message.contains(&format!("Centered on Target 1 where it is at frame {start}; this framing does not follow it."))
            }),
        json!({"range":[start, end],"scales":[1.0, 1.35, 1.35, 1.0],"message":format!("… Centered on Target 1 where it is at frame {start}; this framing does not follow it.")}),
        json!({"range":[start, end],"scales":poses,"framing":format!("{:?}", framing(d)),"error":d.app().error,"message":d.app().message}),
    )?;
    d.capture("Ranged smash zoom inside the beat")?;
    d.key(Key::Escape)?;
    d.settled()?;

    // Black-frame punctuation.
    let revision = d.revision();
    let length = d.app().sequence_length();
    let cursor = d.app().sequence_cursor as i64;
    d.command("hold 6f video=black")?;
    d.changed(&revision)?;
    d.settled()?;
    let black = d.app().workspace.as_ref().and_then(|workspace| {
        let node = d.app().selected_beat.as_ref()?;
        match &workspace.document.nodes()[node].kind {
            NodeKind::Hold { recipe } => Some(recipe.clone()),
            _ => None,
        }
    });
    let picture = d
        .app()
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.plan.picture(ProjectFrame(cursor)).ok())
        .map(|sample| sample.picture);
    d.check(
        ":hold 6f video=black inserts six silent black frames at the cursor",
        black.as_ref().is_some_and(|recipe| {
            recipe.video == deadpan_core::HoldVideo::Background
                && recipe.audio == deadpan_core::HoldAudio::Silence
                && recipe.duration.frames() == 6
        }) && matches!(picture, Some(deadpan_plan::Picture::Background))
            && d.app().sequence_length() == length + 6,
        json!({"video":"Background","audio":"Silence","frames":length + 6}),
        json!({"recipe":format!("{black:?}"),"picture":format!("{picture:?}"),"frames":d.app().sequence_length()}),
    )?;
    d.capture("Black-frame punctuation")?;
    let inserted = d.revision();
    d.key(Key::U)?;
    d.changed(&inserted)?;
    d.settled()?;
    d.check(
        "One Undo removes the black pause",
        d.app().sequence_length() == length,
        json!(length),
        json!(d.app().sequence_length()),
    )?;

    // :zoom records as framing in a macro and replays after Undo.
    d.chord(&[Key::Q, Key::B])?;
    let before = d.revision();
    d.command("zoom 1.5 target=center")?;
    d.changed(&before)?;
    d.settled()?;
    d.key(Key::Q)?;
    d.wait_for("Macro b saved", |app| {
        !app.service.is_busy() && !app.macros.is_pending() && !app.macros.recording()
    })?;
    let recorded = d.revision();
    d.key(Key::U)?;
    d.changed(&recorded)?;
    d.settled()?;
    let undone = framing(d);
    let before = d.revision();
    let shifted = |key, pressed| egui::Event::Key {
        key,
        physical_key: Some(key),
        pressed,
        repeat: false,
        modifiers: egui::Modifiers::SHIFT,
    };
    d.events(
        "Run macro b with native @ text",
        vec![
            shifted(Key::Num2, true),
            egui::Event::Text("@".into()),
            shifted(Key::Num2, false),
        ],
    )?;
    d.key(Key::B)?;
    d.changed(&before)?;
    d.settled()?;
    let replayed = framing(d);
    d.check(
        "A recorded :zoom replays as the same framing",
        // Undo restores the ranged punch-in recorded over.
        matches!(&undone, Some(Framing { value: FramingValue::Envelope { .. }, .. }))
            && matches!(&replayed, Some(Framing { value: FramingValue::Static { pose }, .. })
                if pose.scale == ExactRatio::new(3, 2).unwrap()),
        json!({"after_undo":"the ranged punch-in","after_replay":"Static 1.5"}),
        json!({"after_undo":format!("{undone:?}"),"after_replay":format!("{replayed:?}"),"error":d.app().error}),
    )?;

    // With two saved targets in the picture, ,z asks which one.
    d.chord(&[Key::Comma, Key::F])?;
    d.wait_for("Camera opens", |app| app.camera.is_some())?;
    d.key(Key::N)?;
    d.chord(&[Key::Num8, Key::H])?;
    d.key(Key::Enter)?;
    d.wait_for("Second target saved", |app| {
        app.workspace
            .as_ref()
            .is_some_and(|workspace| workspace.document.targets().len() == 2)
            && app.camera.as_ref().is_some_and(|camera| {
                let state = camera.harness_state();
                state["saving"] == false && state["rebasing"] == false
            })
    })?;
    d.key(Key::Escape)?;
    d.settled()?;
    let revision = d.revision();
    d.chord(&[Key::Comma, Key::Z])?;
    d.settled()?;
    d.check(
        ",z with two saved targets in the picture names both and makes no edit",
        d.revision() == revision
            && d.app().error.as_deref().is_some_and(|error| {
                error.contains("Several targets cover this picture (Target 1, Target 2)")
            }),
        json!("Several targets cover this picture (Target 1, Target 2). Name one…"),
        json!({"revision_unchanged":d.revision() == revision,"error":d.app().error}),
    )?;
    d.capture("Two targets: ,z asks which one")?;
    d.command("zoom 1.35 target=\"Target 2\"")?;
    d.changed(&revision)?;
    d.settled()?;
    let named = framing(d);
    d.check(
        "A quoted label picks the second target, which the beat then follows",
        matches!(&named, Some(Framing { value: FramingValue::Follow { target, .. }, .. })
            if target.as_str() == "target-2"),
        json!("Follow target-2"),
        json!(format!("{named:?}")),
    )?;
    Ok(())
}
