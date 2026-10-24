//! First-class point targets through production Camera keys and project saves.
//! No detector or tracking seam is needed: a point has no tracking rectangle.

use deadpan_core::{ExactRatio, FramingPose, FramingValue, TargetId, TargetPoint};
use egui::{Key, Modifiers};

use super::*;

fn camera(d: &Driver<'_>) -> Value {
    d.app()
        .camera
        .as_ref()
        .map_or(Value::Null, |camera| camera.harness_state())
}

fn target(d: &Driver<'_>) -> Result<deadpan_core::AttentionTarget, String> {
    d.app()
        .workspace
        .as_ref()
        .and_then(|workspace| {
            workspace
                .document
                .targets()
                .get(&TargetId::new("target-1").ok()?)
        })
        .cloned()
        .ok_or_else(|| "The point target is missing.".into())
}

fn framing(d: &Driver<'_>) -> Option<deadpan_core::Framing> {
    let node = d.app().selected_beat.as_ref()?;
    d.app()
        .workspace
        .as_ref()?
        .document
        .nodes()
        .get(node)?
        .framing
        .clone()
}

fn planned_pose(d: &Driver<'_>, frame: i64) -> Option<FramingPose> {
    let node = d.app().selected_beat.as_ref()?;
    d.app()
        .workspace
        .as_ref()?
        .plan
        .picture(deadpan_core::ProjectFrame(frame))
        .ok()?
        .framing
        .iter()
        .find(|layer| &layer.instance.node == node)?
        .pose
}

fn continued(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for(
        "Point saved and Camera continued on the saved revision",
        |app| {
            app.camera.as_ref().is_some_and(|camera| {
                let state = camera.harness_state();
                state["saving"] == false
                    && state["rebasing"] == false
                    && state["phase"] == "Adjust"
                    && app.workspace.as_ref().is_some_and(|workspace| {
                        state["revision"] == workspace.document.revision_id().as_str()
                    })
            })
        },
    )?;
    d.settled()
}

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Shot analysis stored before drawing the point", |app| {
        app.workspace
            .as_ref()
            .is_some_and(|workspace| workspace.shot_analysis.is_some())
    })?;
    super::transcript::focus_your_edit(d)?;
    d.chord(&[Key::G, Key::G, Key::Num1, Key::Num0, Key::L])?;
    d.settled()?;
    let frames = d.app().sequence_length();
    let start = d.revision();
    d.chord(&[Key::Comma, Key::F])?;
    d.wait_for("Camera opens for a new point", |app| app.camera.is_some())?;
    d.key(Key::N)?;
    d.key(Key::N)?;
    d.step("Point geometry selected", true)?;
    let opened = camera(d);
    d.check(
        "n switches the new target from rectangle to a center-only point",
        opened["phase"] == "Region"
            && opened["region"]["point"] == true
            && opened["region"]["size"].is_null()
            && opened["region"]["kind_locked"] == false
            && opened["field"] == "Center"
            && d.revision() == start,
        json!({"point":true,"size":null,"field":"Center","saved":false}),
        opened,
    )?;
    d.chord(&[Key::Num3, Key::L, Key::Num2, Key::K])?;
    d.key(Key::Tab)?;
    d.key_modified(Key::Tab, Modifiers::SHIFT)?;
    d.step("Point center adjusted", true)?;
    let adjusted = camera(d);
    let widgets = d.widgets().to_string();
    d.check(
        "Counts move the point in source percent and Tab has no size field",
        adjusted["region"]["center"] == json!([530000, 480000])
            && adjusted["field"] == "Center"
            && adjusted["region"]["size"].is_null()
            && widgets.contains("point")
            && widgets.contains("Use rectangle"),
        json!({"center":[530000,480000],"size":null,"field":"Center"}),
        adjusted,
    )?;
    d.capture("New point target with center controls")?;
    d.key(Key::Enter)?;
    continued(d)?;
    let saved = target(d)?;
    let saved_revision = d.revision();
    let saved_bytes = serde_json::to_value(&saved).map_err(|error| error.to_string())?;
    let indexed_pts = d.app().workspace.as_ref().and_then(|workspace| {
        Some(
            workspace
                .sources
                .get(&saved.asset)?
                .video_index
                .as_ref()?
                .frames()
                .get(10)?
                .pts,
        )
    });
    d.check(
        "Enter commits only a center-only point and continues Camera on that head",
        saved.region
            == TargetPoint {
                center: [530000, 480000],
            }
            .into()
            && saved.samples.is_empty()
            && saved.corrections.is_empty()
            && framing(d).is_none()
            && saved_revision != start
            && saved_bytes["region"] == json!({"center":[530000,480000]})
            && indexed_pts == Some(saved.span.start().ticks)
            && saved.display_aspect == Some(ExactRatio::new(16, 9).unwrap())
            && d.app().sequence_length() == frames,
        json!({"region":{"center":[530000,480000]},"framing":null,"frames":frames}),
        json!({"target":saved_bytes,"framing":framing(d),"frames":d.app().sequence_length()}),
    )?;

    d.key_modified(Key::T, Modifiers::SHIFT)?;
    d.wait_for("Point tracking refusal arrives", |app| {
        app.error
            .as_deref()
            .or(app.project_error.as_deref())
            .is_some_and(|error| error.contains("no tracking rectangle"))
    })?;
    d.check(
        "Tracking a point refuses before launching a job or writing history",
        d.app().targets.job().is_none()
            && d.revision() == saved_revision
            && target(d)? == saved
            && camera(d)["selected"] == "target-1",
        json!({"job":null,"revision":saved_revision,"point":"unchanged"}),
        json!({"job":format!("{:?}",d.app().targets.job()),"revision":d.revision(),"camera":camera(d)}),
    )?;

    // Follow immediately after saving, without first centering via the picker.
    d.key(Key::T)?;
    d.settled()?;
    let expected = FramingPose::new(
        ExactRatio::new(53, 100).unwrap(),
        ExactRatio::new(48, 100).unwrap(),
        ExactRatio::ONE,
    )
    .map_err(|error| error.to_string())?
    .quantized()
    .map_err(|error| error.to_string())?;
    let live: FramingPose = serde_json::from_value(camera(d)["draft_pose"].clone())
        .map_err(|error| error.to_string())?;
    d.check(
        "t immediately previews the saved point center without a framing commit",
        camera(d)["follow"] == "target-1"
            && live == expected
            && framing(d).is_none()
            && d.revision() == saved_revision,
        json!({"pose":expected,"framing":null}),
        json!({"camera":camera(d),"framing":framing(d)}),
    )?;
    d.capture("Following the point before the separate framing commit")?;
    d.key(Key::Enter)?;
    d.changed(&saved_revision)?;
    d.settled()?;
    let followed = framing(d);
    d.check(
        "The separate Camera commit follows the point and preserves the neutral fallback",
        matches!(&followed, Some(deadpan_core::Framing { value: FramingValue::Follow { target, fallback, .. }, .. })
            if target.as_str() == "target-1" && fallback.center_x == ExactRatio::new(1,2).unwrap()
                && fallback.center_y == ExactRatio::new(1,2).unwrap())
            && d.app().camera.is_none(),
        json!({"target":"target-1","fallback":[0.5,0.5]}),
        json!(followed),
    )?;
    let before = d.revision();
    d.command("undo")?;
    d.changed(&before)?;
    d.check(
        "Undo removes framing while retaining the separately saved point",
        framing(d).is_none() && target(d)? == saved,
        json!({"framing":null,"point":"retained"}),
        json!(framing(d)),
    )?;
    let before = d.revision();
    d.command("redo")?;
    d.changed(&before)?;
    d.check(
        "Redo restores the exact point Follow",
        framing(d) == followed,
        json!(followed),
        json!(framing(d)),
    )?;

    let before = d.revision();
    d.command("creep to=1.35")?;
    d.changed(&before)?;
    d.settled()?;
    let creep = framing(d);
    d.check("Creep keeps the point live while adding the scale transition",
        matches!(&creep, Some(deadpan_core::Framing { value: FramingValue::Follow { target, scale_transition: Some(_), .. }, .. }) if target.as_str() == "target-1")
            && d.app().sequence_length() == frames,
        json!({"target":"target-1","transition":true,"frames":frames}), json!(creep))?;

    d.chord(&[Key::G, Key::G, Key::Num3, Key::Num0, Key::L])?;
    d.settled()?;
    d.chord(&[Key::Comma, Key::F])?;
    d.wait_for("Camera opens at the point correction picture", |app| {
        app.camera.is_some()
    })?;
    let before = d.revision();
    d.key(Key::C)?;
    d.key(Key::N)?;
    d.step("Point correction keeps geometry kind", true)?;
    let locked = camera(d);
    d.check(
        "Corrections keep the point kind and explain the rejected switch",
        locked["region"]["point"] == true
            && locked["region"]["kind_locked"] == true
            && locked["region"]["size"].is_null()
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("keeps the target's geometry kind"))
            && d.revision() == before,
        json!({"point":true,"kind_locked":true,"saved":false}),
        locked,
    )?;
    d.key(Key::Tab)?;
    d.chord(&[Key::Num8, Key::L, Key::Num7, Key::J])?;
    let widgets = d.widgets().to_string();
    d.check(
        "A valid point move clears the earlier refusal and the help names a target",
        d.app().error.is_none()
            && widgets.contains("Finish the target")
            && !widgets.contains("Finish the rectangle"),
        json!({"error":null,"help":"Finish the target"}),
        json!({"error":d.app().error,"help":widgets.contains("Finish the target")}),
    )?;
    d.capture("Correcting the point without inventing a tracking box")?;
    d.key(Key::Enter)?;
    continued(d)?;
    let corrected = target(d)?;
    d.check(
        "A manual point correction saves once without tracking or changing framing",
        corrected.region == saved.region
            && corrected.span == saved.span
            && corrected.corrections.len() == 1
            && corrected.corrections[0].region
                == TargetPoint {
                    center: [610000, 550000],
                }
                .into()
            && corrected.samples.is_empty()
            && d.app().targets.job().is_none()
            && framing(d) == creep
            && d.app().sequence_length() == frames,
        json!({"corrections":1,"center":[610000,550000],"framing":"unchanged","frames":frames}),
        json!({"target":corrected,"framing":framing(d),"frames":d.app().sequence_length()}),
    )?;
    d.key(Key::Escape)?;
    d.settled()?;
    let correction_pts = d.app().workspace.as_ref().and_then(|workspace| {
        Some(
            workspace
                .sources
                .get(&saved.asset)?
                .video_index
                .as_ref()?
                .frames()
                .get(30)?
                .pts,
        )
    });
    let first_pose = planned_pose(d, 0);
    let early_pose = planned_pose(d, 20);
    let late_pose = planned_pose(d, 40);
    let corrected_center = FramingPose::new(
        ExactRatio::new(61, 100).unwrap(),
        ExactRatio::new(55, 100).unwrap(),
        ExactRatio::ONE,
    )
    .map_err(|error| error.to_string())?
    .quantized()
    .map_err(|error| error.to_string())?;
    d.check("The point correction uses indexed Original PTS and changes only later source time",
        correction_pts == Some(corrected.corrections[0].at)
            && first_pose.is_some_and(|pose| pose.center_x == ExactRatio::new(1,2).unwrap()
                && pose.center_y == ExactRatio::new(1,2).unwrap())
            && early_pose.is_some_and(|pose| pose.center_x == expected.center_x && pose.center_y == expected.center_y)
            && late_pose.is_some_and(|pose| pose.center_x == corrected_center.center_x && pose.center_y == corrected_center.center_y),
        json!({"correction_pts":correction_pts,"before_span":"fallback","early":expected,"later":corrected_center}),
        json!({"correction_pts":corrected.corrections[0].at,"before_span":first_pose,"early":early_pose,"later":late_pose}))?;
    let before = d.revision();
    d.command("undo")?;
    d.changed(&before)?;
    d.check(
        "Undo removes only the manual correction, keeping the point and creep",
        target(d)? == saved && framing(d) == creep,
        json!({"point":"original","framing":"unchanged"}),
        json!(target(d)?),
    )?;
    let before = d.revision();
    d.command("redo")?;
    d.changed(&before)?;
    d.check(
        "Redo restores the exact manual point correction",
        target(d)? == corrected && framing(d) == creep,
        json!(corrected),
        json!(target(d)?),
    )?;
    d.capture("Point target correction and live creep committed")
}
