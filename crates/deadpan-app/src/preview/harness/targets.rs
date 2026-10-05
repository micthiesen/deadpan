//! Attention targets through the production keys, Camera, commands and
//! inspector: keyboard rectangle creation, the numbered picker with saved
//! targets, following a target, Camera on a follow, background tracking and a
//! correction that re-tracks one range.
//!
//! `targets` replaces only the Vision worker with the scripted test seam
//! (`project::targets::Backend::Scripted`): range resolution against the
//! stored shot analysis, the verified Original copy, the tracking policy,
//! compaction and the revision-guarded saves are real.

use std::sync::Arc;
use std::time::Duration;

use deadpan_core::{FramingValue, TargetId};
use egui::{Key, Modifiers};

use super::*;
use crate::project::targets::{Backend, Outcome, Script, ScriptEnding, ScriptQueue};

/// Every start tracks a subject moving right, slowly enough to show progress.
pub(super) fn backend() -> Backend {
    Backend::Scripted(Arc::new(ScriptQueue::new([Script {
        unavailable: None,
        steps: 12,
        step_interval: Duration::from_millis(40),
        ending: ScriptEnding::Moving { step: 0.003 },
    }])))
}

fn camera(d: &Driver<'_>) -> Value {
    d.app()
        .camera
        .as_ref()
        .map_or(Value::Null, |camera| camera.harness_state())
}

fn widget_text(d: &Driver<'_>) -> String {
    d.widgets().to_string()
}

fn target(d: &Driver<'_>, id: &str) -> Option<deadpan_core::AttentionTarget> {
    d.app()
        .workspace
        .as_ref()?
        .document
        .targets()
        .get(&TargetId::new(id).ok()?)
        .cloned()
}

fn job_outcome(d: &Driver<'_>) -> Option<Outcome> {
    d.app().targets.job()?.outcome.clone()
}

fn camera_layer_pose(d: &Driver<'_>) -> Option<deadpan_core::FramingPose> {
    let selected = d.app().selected_beat.clone()?;
    d.app()
        .presentation
        .picture()?
        .framing
        .iter()
        .find(|layer| !layer.escalation && layer.instance.node == selected)?
        .pose
}

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.report.skipped.push(
        "The Vision worker is the scripted test seam; real tracking is covered by native/deadpan-track tests and the CLI (docs/TRACKING.md).".into(),
    );
    d.wait_for("Shot analysis stored for the Original", |app| {
        app.workspace
            .as_ref()
            .is_some_and(|workspace| workspace.shot_analysis.is_some())
    })?;
    super::transcript::focus_your_edit(d)?;
    d.chord(&[Key::G, Key::G, Key::Num1, Key::Num0, Key::L])?;
    d.settled()?;
    let start = d.revision();
    let widgets = widget_text(d);
    d.check(
        "The inspector lists targets and teaches how to draw one",
        widgets.contains("TARGETS") && widgets.contains("press n to draw one"),
        json!({"section":"TARGETS","hint":"In Camera (,f), press n to draw one on the picture."}),
        json!({"has_section":widgets.contains("TARGETS")}),
    )?;

    // Keyboard rectangle creation inside Camera.
    d.chord(&[Key::Comma, Key::F])?;
    d.wait_for("Camera opens", |app| app.camera.is_some())?;
    d.key(Key::N)?;
    d.step("Rectangle opened", true)?;
    let opened = camera(d);
    d.check(
        "n opens a centered rectangle on the Center field",
        opened["phase"] == "Region"
            && opened["field"] == "Center"
            && opened["region"]["center"] == json!([500000, 500000]),
        json!({"phase":"Region","field":"Center","center":[500000,500000]}),
        opened.clone(),
    )?;
    d.chord(&[Key::Num3, Key::L])?;
    d.key(Key::Tab)?;
    d.key_modified(Key::K, Modifiers::SHIFT)?;
    d.key(Key::Tab)?;
    d.chord(&[Key::Num2, Key::ArrowDown])?;
    d.step(
        "Rectangle adjusted with Tab, counts, h/j/k/l and arrows",
        true,
    )?;
    let adjusted = camera(d);
    let size = &opened["region"]["size"];
    let expected = json!({
        "center": [530000, 500000],
        "size": [size[0].as_u64().unwrap_or(0) + 50_000, size[1].as_u64().unwrap_or(0).saturating_sub(20_000).max(10_000)],
        "field": "Height",
    });
    d.check(
        "Tab cycles center, width and height; counts and Shift scale the 1% steps",
        adjusted["region"]["center"] == expected["center"]
            && adjusted["region"]["size"] == expected["size"]
            && adjusted["field"] == "Height"
            && d.revision() == start,
        expected,
        adjusted.clone(),
    )?;
    let widgets = widget_text(d);
    d.check(
        "The footer and inspector teach the rectangle keys",
        widgets.contains("center / width / height")
            && widgets.contains("NEW TARGET")
            && widgets.contains("Save target"),
        json!([
            "Tab center / width / height",
            "Enter save",
            "NEW TARGET",
            "Save target  Enter"
        ]),
        json!({"widgets":widgets.contains("NEW TARGET")}),
    )?;
    d.capture("Keyboard target rectangle")?;
    d.key(Key::Enter)?;
    d.wait_for("Target saved and Camera continued on its revision", |app| {
        app.camera.as_ref().is_some_and(|camera| {
            let state = camera.harness_state();
            state["saving"] == false && state["rebasing"] == false && state["phase"] == "Adjust"
        }) && app
            .workspace
            .as_ref()
            .is_some_and(|workspace| !workspace.document.targets().is_empty())
    })?;
    d.settled()?;
    let saved = target(d, "target-1").ok_or("target-1 was not saved")?;
    let state = camera(d);
    d.check(
        "Enter saves one target edit; Camera stays open with it selected",
        d.revision() != start
            && state["revision"] == d.revision()
            && state["selected"] == "target-1"
            && saved.label == "Target 1"
            && saved.samples.is_empty()
            && saved.region.center == [530_000, 500_000],
        json!({"camera_revision":"current","selected":"target-1","label":"Target 1","samples":0}),
        json!({"camera":state,"label":saved.label,"samples":saved.samples.len(),"region":format!("{:?}", saved.region)}),
    )?;
    d.capture("Saved target selected in Camera")?;
    let saved_revision = d.revision();

    // The numbered picker lists the saved target first.
    d.key(Key::F)?;
    d.step("Target picker", true)?;
    let picker = camera(d);
    d.check(
        "The picker numbers saved targets before the center and corners",
        picker["picker"][0]["label"] == "Target 1"
            && picker["picker"][0]["number"] == 1
            && picker["picker"][1]["label"] == "Center"
            && picker["picker"][5]["number"] == 6,
        json!([{"number":1,"label":"Target 1"},{"number":2,"label":"Center"}]),
        picker["picker"].clone(),
    )?;
    let painted = scenarios::text_paint_visibility(d, "1 Target 1 · drawn");
    d.check(
        "The saved rectangle is drawn with its number, label and state",
        !painted.is_empty(),
        json!("1 Target 1 · drawn"),
        json!(painted),
    )?;
    d.capture("Picker with a saved target")?;
    d.key(Key::Num1)?;
    d.key(Key::T)?;
    d.chord(&[Key::Num2, Key::Plus])?;
    d.settled()?;
    let following = camera(d);
    let preview = camera_layer_pose(d);
    d.check(
        "t follows the chosen target and + scales the follow without an edit",
        following["follow"] == "target-1"
            && following["phase"] == "Adjust"
            && d.revision() == saved_revision,
        json!({"follow":"target-1","revision":saved_revision}),
        json!({"camera":following,"revision":d.revision()}),
    )?;
    d.key(Key::H)?;
    d.step("Nudge refused while following", false)?;
    d.check(
        "A center nudge while following is refused with a reason",
        d.app()
            .error
            .as_deref()
            .is_some_and(|error| error.contains("supplies the center")),
        json!("The followed target supplies the center…"),
        json!(d.app().error),
    )?;
    d.capture("Following Target 1 at 1.10x")?;
    d.key(Key::Enter)?;
    d.changed(&saved_revision)?;
    let node = d.app().selected_beat.clone().ok_or("No selected beat")?;
    let framing = d.app().workspace.as_ref().unwrap().document.nodes()[&node]
        .framing
        .clone();
    let committed = camera_layer_pose(d);
    d.check(
        "Enter saves a Follow; the committed picture matches the Camera preview exactly",
        matches!(&framing, Some(deadpan_core::Framing { value: FramingValue::Follow { target, .. }, .. }) if target.as_str() == "target-1")
            && d.app().camera.is_none()
            && preview.is_some()
            && committed == preview,
        json!({"framing":"Follow target-1","preview":format!("{preview:?}")}),
        json!({"framing":format!("{framing:?}"),"committed":format!("{committed:?}")}),
    )?;
    let widgets = widget_text(d);
    d.check(
        "The inspector names the followed target by label",
        widgets.contains("Follows") && widgets.contains("Target 1 · 1.10×"),
        json!({"label":"Follows","value":"Target 1 · 1.10×"}),
        json!({"has_value":widgets.contains("Target 1 · 1.10×"),"has_id":widgets.contains("target-1 · 1.10×")}),
    )?;
    d.capture("Inspector names the followed target")?;
    let followed_revision = d.revision();

    // Camera on a follow: the scale changes, the target keeps the center.
    d.chord(&[Key::Comma, Key::F])?;
    d.wait_for("Camera opens on the follow", |app| app.camera.is_some())?;
    let entry = camera_layer_pose(d);
    d.key(Key::Plus)?;
    d.settled()?;
    let scaled = camera_layer_pose(d);
    d.check(
        "Camera on a follow previews the target-centered pose at the new scale",
        camera(d)["follow"] == "target-1"
            && entry.zip(scaled).is_some_and(|(entry, scaled)| {
                entry.center_x == scaled.center_x
                    && entry.center_y == scaled.center_y
                    && scaled.scale.compare(entry.scale).is_gt()
            }),
        json!({"center":"unchanged","scale":"larger"}),
        json!({"entry":format!("{entry:?}"),"scaled":format!("{scaled:?}")}),
    )?;
    d.key(Key::Escape)?;
    d.settled()?;
    d.check(
        "Escape restores the entry follow without an edit",
        d.app().camera.is_none()
            && camera_layer_pose(d) == entry
            && d.revision() == followed_revision,
        json!(format!("{entry:?}")),
        json!(format!("{:?}", camera_layer_pose(d))),
    )?;

    // Background tracking from Camera; Camera continues on the saved result.
    d.chord(&[Key::Comma, Key::F])?;
    d.wait_for("Camera opens again", |app| app.camera.is_some())?;
    d.key_modified(Key::T, Modifiers::SHIFT)?;
    d.wait_for("Tracking progress reported", |app| {
        app.targets.running().is_some_and(|job| {
            matches!(job.phase, crate::project::targets::Phase::Tracking(percent) if percent > 0)
        })
    })?;
    d.step("Tracking in the background", false)?;
    let widgets = widget_text(d);
    d.check(
        "Tracking runs in the background with progress and an explicit cancel",
        widgets.contains("Tracking Target 1") && widgets.contains("Cancel tracking") && d.app().camera.is_some(),
        json!(["Tracking Target 1 · Tracking N% · m:ss","Cancel tracking  :track-cancel"]),
        json!({"tracking":widgets.contains("Tracking Target 1"),"cancel":widgets.contains("Cancel tracking")}),
    )?;
    d.capture("Tracking in the background")?;
    d.wait_for("Tracking saved and Camera continued", |app| {
        app.targets.job().is_some_and(|job| !job.running())
            && app.camera.as_ref().is_some_and(|camera| {
                let state = camera.harness_state();
                state["rebasing"] == false
                    && app.workspace.as_ref().is_some_and(|workspace| {
                        state["revision"] == workspace.document.revision_id().as_str()
                    })
            })
    })?;
    d.settled()?;
    let tracked = target(d, "target-1").ok_or("target-1 disappeared")?;
    let outcome = job_outcome(d);
    d.check(
        "The tracked path is saved expecting the entry head and Camera keeps its follow",
        matches!(outcome, Some(Outcome::Saved { .. }))
            && !tracked.samples.is_empty()
            && tracked.provenance.as_ref().is_some_and(|provenance| provenance.engine == crate::project::targets::SCRIPTED_ENGINE)
            && d.revision() != followed_revision
            && camera(d)["follow"] == "target-1",
        json!({"outcome":"Saved","samples":">0","follow":"target-1"}),
        json!({"outcome":format!("{outcome:?}"),"samples":tracked.samples.len(),"camera":camera(d)}),
    )?;
    // The selection picture keeps its drawn rectangle; tracked pictures follow.
    let painted = scenarios::text_paint_visibility(d, "Target 1 · drawn · following");
    d.check(
        "The overlay shows the drawn seed at the selection picture",
        !painted.is_empty(),
        json!("Target 1 · drawn · following"),
        json!(painted),
    )?;
    d.capture("Tracked target followed in Camera")?;
    let tracked_revision = d.revision();

    // A correction at a later picture re-tracks only from that picture.
    d.key(Key::Escape)?;
    d.settled()?;
    d.chord(&[Key::Num5, Key::L])?;
    d.settled()?;
    d.chord(&[Key::Comma, Key::F])?;
    d.wait_for("Camera opens at the later picture", |app| {
        app.camera.is_some()
    })?;
    d.settled()?;
    let painted = scenarios::text_paint_visibility(d, "Target 1 · tracked · following");
    d.check(
        "Five pictures later the overlay shows the tracked position and state",
        !painted.is_empty() && camera(d)["follow"] == "target-1",
        json!("Target 1 · tracked · following"),
        json!({"painted":painted,"camera":camera(d)}),
    )?;
    d.capture("Tracked position five pictures later")?;
    d.key(Key::C)?;
    d.step("Correction rectangle", true)?;
    d.check(
        "c opens the target's rectangle at this picture for correction",
        camera(d)["phase"] == "Region",
        json!("Region"),
        camera(d),
    )?;
    d.chord(&[Key::Num4, Key::H])?;
    d.capture("Correcting the target at this picture")?;
    d.key(Key::Enter)?;
    d.wait_for("Correction re-tracked and saved", |app| {
        app.targets
            .job()
            .is_some_and(|job| job.correction && !job.running())
            && app.workspace.as_ref().is_some_and(|workspace| {
                workspace.document.revision_id().as_str() != tracked_revision
            })
            && app
                .camera
                .as_ref()
                .is_some_and(|camera| camera.harness_state()["rebasing"] == false)
    })?;
    d.settled()?;
    let corrected = target(d, "target-1").ok_or("target-1 disappeared")?;
    d.check(
        "The correction is stored and only its range is re-tracked",
        corrected.corrections.len() == 1
            && corrected.span == tracked.span
            && matches!(job_outcome(d), Some(Outcome::Saved { .. })),
        json!({"corrections":1,"span":"unchanged"}),
        json!({"corrections":corrected.corrections.len(),"outcome":format!("{:?}", job_outcome(d))}),
    )?;
    d.capture("Corrected and re-tracked")?;
    d.key(Key::Escape)?;
    d.settled()?;

    // Commands.
    d.command("track target-1")?;
    d.step(":track on a tracked target", false)?;
    d.check(
        ":track refuses to overwrite a tracked target and points to corrections",
        d.app()
            .error
            .as_deref()
            .or(d.app().project_error.as_deref())
            .is_some_and(|error| error.contains("already tracked")),
        json!("Target 1 is already tracked…"),
        json!({"error":d.app().error,"project":d.app().project_error}),
    )?;
    d.command("track-cancel")?;
    d.step(":track-cancel without a job", false)?;
    d.check(
        ":track-cancel without a running job says so",
        d.app().error.as_deref() == Some("No target is tracking."),
        json!("No target is tracking."),
        json!(d.app().error),
    )?;
    let before = d.revision();
    d.command("undo")?;
    d.changed(&before)?;
    d.check(
        "Undo restores the target before the correction",
        target(d, "target-1").is_some_and(|target| target.corrections.is_empty()),
        json!({"corrections":0}),
        json!(target(d, "target-1").map(|target| target.corrections.len())),
    )?;
    d.report.skipped.push(
        "Face/person proposals are not implemented; the ,z target punch-in and :zoom target= are covered by the zoom scenario."
            .into(),
    );
    d.capture("Targets after undo")
}
