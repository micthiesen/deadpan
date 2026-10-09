//! Explicit linked creation and setup retry through production controls.
//! Only the native file pickers are scripted.

use deadpan_store::{AccessMode, ProjectStore};
use egui::{Key, Modifiers};

use super::*;

fn ready(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("The complete Original is ready and displayed", |app| {
        app.workspace.as_ref().is_some_and(|workspace| {
            matches!(
                workspace.single_source,
                Some(SingleSourceState::Ready { .. })
            )
        }) && app.presentation.has_displayed()
            && !app.presentation.loading()
            && !app.service.is_busy()
    })?;
    d.settled()
}

fn ownership(d: &mut Driver<'_>, linked: bool, source: &Path) -> Result<(), String> {
    let workspace = d.app().workspace.as_ref().ok_or("No project")?;
    let store = ProjectStore::open(&workspace.path, AccessMode::ReadOnly)
        .map_err(|error| error.to_string())?;
    let records = store
        .original_records(None, 10)
        .map_err(|error| error.to_string())?;
    let matching = records.iter().find(|record| {
        if linked {
            !record.managed() && record.linked().is_some_and(|link| link.path() == source)
        } else {
            record.managed() && record.linked().is_none()
        }
    });
    d.check(
        "Original ownership matches the chosen action, with a full-source Undo baseline",
        matching.is_some()
            && workspace
                .document
                .duration()
                .map_err(|error| error.to_string())?
                .frames()
                == 120
            && !workspace.can_undo,
        json!({"linked":linked,"frames":120,"undo":false}),
        json!({"records":records,"frames":d.app().sequence_length(),"undo":workspace.can_undo}),
    )
}

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.report.skipped.push("Native file picker results are scripted; media retention, qualification, source baseline, commands and Metal presentation use production code.".into());
    let source = d
        .app()
        .feedback
        .original_fixture
        .clone()
        .ok_or("No Original fixture")?;
    d.harness.set_size(egui::vec2(960.0, 640.0));
    d.step("Paint the minimum start surface", false)?;
    d.capture("Linked Original choice at minimum size")?;
    for needle in [
        "Choose video…",
        "Link video in place…",
        "Choose video keeps",
        "Open project…",
        "Choose cookies file…",
    ] {
        let paint = scenarios::text_paint_visibility(d, needle);
        d.check(
            "Both ownership choices and their explanation fit the minimum window",
            !paint.is_empty() && paint.iter().all(|part| part["fully_visible"] == true),
            json!(needle),
            json!(paint),
        )?;
    }
    d.app_mut().dialogs = Dialogs::scripted(vec![(DialogKind::CreateLinkedProject, None)]);
    d.click("Link video in place…  :new-linked")?;
    d.step("Cancel the linked picker", false)?;
    d.check(
        "Cancelling linked New keeps the start screen and creates no project",
        d.app().workspace.is_none()
            && !d.app().service.is_busy()
            && d.app().dialog_intent.is_none(),
        json!("no project"),
        d.snapshot(),
    )?;

    d.app_mut().dialogs = Dialogs::scripted(vec![(
        DialogKind::CreateLinkedProject,
        Some(source.clone()),
    )]);
    d.command("new-linked")?;
    // Unrelated sound import options cannot change an already chosen intent.
    d.app_mut().linked_import = false;
    ready(d)?;
    ownership(d, true, &source)?;
    d.capture("Linked Original ready")?;
    let linked_project = d.app().workspace.as_ref().unwrap().path.clone();
    let entries = std::fs::read_dir(linked_project.parent().ok_or("No project library")?)
        .map_err(|error| error.to_string())?
        .count();
    d.check(
        "The cancelled picker left no extra package in the private library",
        entries == 1,
        json!(1),
        json!(entries),
    )?;
    let revision = d.revision();
    d.command("original-linked")?;
    d.step("Reject changing an initialized Original", false)?;
    d.check(
        "A linked setup retry cannot replace a ready Original",
        d.revision() == revision
            && !d.app().dialogs.is_open()
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("waiting for its Original")),
        json!(revision),
        d.snapshot(),
    )?;

    d.app_mut().linked_import = true;
    d.app_mut().dialogs =
        Dialogs::scripted(vec![(DialogKind::CreateProject, Some(source.clone()))]);
    d.key_modified(Key::N, Modifiers::COMMAND)?;
    d.wait_for("Ordinary New switches to a fresh project", |app| {
        app.workspace
            .as_ref()
            .is_some_and(|workspace| workspace.path != linked_project)
    })?;
    ready(d)?;
    ownership(d, false, &source)?;

    // Failed qualification retains an explicit incomplete package for retry.
    let bad = source
        .parent()
        .ok_or("Fixture has no parent")?
        .join("../audio-fixtures/pcm-stereo-48000.wav")
        .canonicalize()
        .map_err(|error| error.to_string())?;
    d.app_mut().dialogs = Dialogs::scripted(vec![(DialogKind::CreateLinkedProject, Some(bad))]);
    d.command("new-linked")?;
    d.wait_for("Audio alone leaves an incomplete project", |app| {
        app.import
            .as_ref()
            .is_some_and(|status| status.stage == ImportStage::Failed)
            && app.workspace.as_ref().is_some_and(|workspace| {
                matches!(
                    workspace.single_source,
                    Some(SingleSourceState::AwaitingSource { .. })
                )
            })
    })?;
    let unfinished = d.app().workspace.as_ref().unwrap().path.clone();
    d.capture("Incomplete project teaches the linked retry")?;
    let paint = scenarios::text_paint_visibility(d, "Link Original in place…");
    d.check(
        "An incomplete project teaches the explicit linked retry",
        !paint.is_empty() && paint.iter().all(|part| part["fully_visible"] == true),
        json!(":original-linked"),
        json!(paint),
    )?;
    d.app_mut().dialogs = Dialogs::scripted(vec![(
        DialogKind::InitializeLinkedSource,
        Some(source.clone()),
    )]);
    d.command("original-linked")?;
    ready(d)?;
    ownership(d, true, &source)?;
    d.check(
        "Setup retry completes the same package",
        d.app()
            .workspace
            .as_ref()
            .is_some_and(|workspace| workspace.path == unfinished),
        json!(unfinished),
        d.snapshot(),
    )?;
    d.capture("Linked setup retry completed")
}
