//! Recovery UI through production input: reopening after an unclean exit,
//! interrupted renders, a missing Original and its relink, and the persistent
//! "Not saved" alert. Only the OS picker results, the crash evidence a killed
//! writer leaves on disk, and one refused commit are scripted; the store's
//! process-kill and disk-image tests qualify the real failures themselves.

use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_core::{FrameRange, ProjectFrame};
use deadpan_jobs::render::{
    RenderAutomaticAlgorithm, RenderAutomaticPolicy, RenderAutomaticSelection, RenderIntent,
    RenderPolicy, document_sha256,
};
use deadpan_jobs::{AttemptId, CancellationToken, RequestId};
use deadpan_store::render_jobs::BeginRenderAttempt;
use deadpan_store::{AccessMode, ProjectStore};
use egui::Key;

use super::*;
use crate::recovery::LaunchJournal;

fn focused(d: &Driver<'_>) -> Option<String> {
    d.harness.root().children_recursive().find_map(|node| {
        let access = node.accesskit_node();
        access.is_focused().then(|| access.label()).flatten()
    })
}

fn saved_edit(d: &mut Driver<'_>) -> Result<String, String> {
    super::transcript::focus_your_edit(d)?;
    d.chord(&[Key::G, Key::G, Key::Num1, Key::Num0, Key::L])?;
    let before = d.revision();
    d.command("split")?;
    d.changed(&before)?;
    Ok(d.revision())
}

fn close_project(d: &mut Driver<'_>) -> Result<std::path::PathBuf, String> {
    let path = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No project to close")?
        .path
        .clone();
    d.app_mut().submit(ProjectRequest::Close);
    d.wait_for("Project closed", |app| {
        app.workspace.is_none() && !app.service.is_busy()
    })?;
    Ok(path)
}

/// What an abandoned render leaves in the database: an active attempt.
fn abandoned_render(path: &std::path::Path) -> Result<(), String> {
    let error = |error: &dyn std::fmt::Display| error.to_string();
    let mut store = ProjectStore::open(path, AccessMode::ReadWrite).map_err(|e| error(&e))?;
    let document = store.snapshot().map_err(|e| error(&e))?;
    let deadline = Instant::now() + Duration::from_secs(30);
    let duration = document.duration().map_err(|e| error(&e))?.frames();
    let intent = store
        .create_render_job(
            RenderIntent {
                schema_version: 2,
                job_id: RequestId::new("replay-interrupted").map_err(|e| error(&e))?,
                project_id: document.project_id().clone(),
                revision_id: document.revision_id().clone(),
                document_sha256: document_sha256(&document, &AtomicBool::new(false), deadline)
                    .map_err(|e| error(&e))?,
                range: FrameRange::new(ProjectFrame(0), ProjectFrame(duration))
                    .map_err(|e| error(&e))?,
                policy: RenderPolicy::Automatic(RenderAutomaticPolicy {
                    schema_version: 1,
                    selection: RenderAutomaticSelection::Automatic,
                    algorithm: RenderAutomaticAlgorithm::AutomaticSdrV1,
                }),
            },
            &AtomicBool::new(false),
            deadline,
        )
        .map_err(|e| error(&e))?;
    store
        .begin_render_attempt(BeginRenderAttempt {
            job_id: intent.job_id,
            attempt_id: AttemptId::new("replay-attempt").map_err(|e| error(&e))?,
            cancellation_token: CancellationToken::new("replay-cancel").map_err(|e| error(&e))?,
            checkpoint_attempt_id: None,
        })
        .map_err(|e| error(&e))?;
    Ok(())
}

/// Reopen after a crash, see what was recovered, and act on it by keyboard.
pub(super) fn crash(d: &mut Driver<'_>) -> Result<(), String> {
    let saved = saved_edit(d)?;
    let path = close_project(d)?;
    abandoned_render(&path)?;
    // A killed writer leaves its session marker, and the killed app's journal
    // still says the project was open.
    std::fs::write(
        path.join(".writer.session"),
        "deadpan writer pid=4242 opened_unix=1",
    )
    .map_err(|e| e.to_string())?;
    let journal_path = path
        .parent()
        .ok_or("Replay project has no parent")?
        .join("replay-launch-session.json");
    LaunchJournal::for_exited_process(journal_path.clone())?.record_open(&path)?;
    let journal = LaunchJournal::at(journal_path);
    d.app_mut().use_launch_journal(journal.clone(), true);
    d.step("Launch after an unclean exit", true)?;
    d.step("Launch offer focused", true)?;
    let name = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let reopen = format!("Reopen {name}  Enter");
    d.check(
        "The launch offer names the project and focuses Reopen for the keyboard",
        d.rect(&reopen).is_ok() && focused(d).as_deref() == Some(reopen.as_str()),
        json!({"focused":reopen}),
        json!({"focused":focused(d)}),
    )?;
    d.capture("Launch offer after an unclean exit")?;
    d.key(Key::Enter)?;
    d.wait_for("Project reopened with its recovery report", |app| {
        app.workspace.is_some() && app.recovery.showing_report() && !app.service.is_busy()
    })?;
    let report = d
        .app()
        .recovery
        .report()
        .cloned()
        .ok_or("Recovery report missing")?;
    d.check(
        "Reopening keeps the last saved revision and reports the crash and the interrupted render",
        d.revision() == saved
            && report.recovery.unclean_previous_writer.is_some()
            && report.recovery.interrupted_render_count == 1,
        json!({"revision":saved,"unclean":true,"interrupted_renders":1}),
        json!({"revision":d.revision(),"unclean":report.recovery.unclean_previous_writer.is_some(),"interrupted_renders":report.recovery.interrupted_render_count}),
    )?;
    d.step("Recovery report focus", true)?;
    d.check(
        "The report focuses its first action, Open Renders",
        focused(d).as_deref() == Some("Open Renders  :renders"),
        json!("Open Renders  :renders"),
        json!(focused(d)),
    )?;
    d.capture("Project recovered report")?;
    d.key(Key::Enter)?;
    d.wait_for("Renders opened on the interrupted job", |app| {
        app.render.history.ready_for_check()
    })?;
    d.capture("Interrupted render's attempts in Renders")?;
    d.check(
        "Renders opens on the interrupted job's attempts with its recovery actions",
        d.rect("Render this saved edit again…").is_ok(),
        json!("Render this saved edit again…"),
        d.widgets(),
    )?;
    d.key(Key::Escape)?;
    d.settled()?;
    d.command("recovery")?;
    d.check(
        ":recovery shows the report again",
        d.app().recovery.showing_report(),
        json!(true),
        json!(d.app().recovery.showing_report()),
    )?;
    d.key(Key::Escape)?;
    d.step("Report dismissed", true)?;
    d.check(
        "Escape answers the report and the journal records the open project",
        !d.app().recovery.showing_report()
            && journal.open_projects() == vec![path.clone()],
        json!({"showing":false,"journal_open":true}),
        json!({"showing":d.app().recovery.showing_report(),"journal":format!("{:?}", journal.open_projects())}),
    )?;
    d.settled()
}

/// Open with the Original's managed copy gone, refuse a different file and
/// restore the identical one, all from the keyboard.
pub(super) fn relink(d: &mut Driver<'_>) -> Result<(), String> {
    let original = d
        .app()
        .feedback
        .original_fixture
        .clone()
        .ok_or("Replay fixture path missing")?;
    let workspace = d.app().workspace.clone().ok_or("No project")?;
    let source = workspace
        .sources
        .values()
        .next()
        .cloned()
        .ok_or("No Original")?;
    let revision = d.revision();
    let path = close_project(d)?;
    let object = path.join("Media/Originals").join(format!(
        "blake3-{}",
        source.original.object().content().digest()
    ));
    std::fs::remove_file(&object).map_err(|e| e.to_string())?;
    let wrong = path
        .parent()
        .ok_or("Replay project has no parent")?
        .join("not-the-original.mp4");
    std::fs::write(&wrong, b"a different file with the same purpose").map_err(|e| e.to_string())?;
    d.app_mut().dialogs = Dialogs::scripted(vec![
        (DialogKind::RelinkOriginal, Some(wrong)),
        (DialogKind::RelinkOriginal, Some(original)),
    ]);
    d.app_mut().submit(ProjectRequest::Open(path.clone()));
    d.wait_for("Project opened without its Original", |app| {
        app.workspace.is_some() && app.recovery.showing_report() && !app.service.is_busy()
    })?;
    d.step("Missing Original report focus", true)?;
    d.check(
        "The project opens degraded, names the missing Original and focuses Locate",
        d.revision() == revision
            && focused(d).as_deref() == Some("Locate Original…  :relink")
            && d.app()
                .recovery
                .report()
                .is_some_and(|report| report.missing().count() == 1),
        json!({"revision":revision,"focused":"Locate Original…  :relink","missing":1}),
        json!({"revision":d.revision(),"focused":focused(d)}),
    )?;
    d.capture("Original missing report")?;
    d.check(
        "The notice names the missing Original and :relink instead of a storage error",
        d.app()
            .missing_original_notice()
            .is_some_and(|notice| notice.contains(":relink")),
        json!("The Original's file is missing… :relink"),
        json!({"notice":d.app().missing_original_notice(),"picture":d.app().presentation.error()}),
    )?;
    d.key(Key::Enter)?;
    d.wait_for("Different file refused", |app| {
        app.project_error
            .as_deref()
            .is_some_and(|error| error.contains("not this project's Original"))
    })?;
    d.check(
        "A different file is refused without publishing bytes",
        !object.exists(),
        json!({"object_exists":false}),
        json!({"object_exists":object.exists(),"error":d.app().project_error}),
    )?;
    d.capture("Different file refused")?;
    d.command("relink")?;
    d.wait_for("Original restored", |app| {
        app.recovery
            .report()
            .is_some_and(|report| report.missing().count() == 0)
            && !app.service.is_busy()
    })?;
    d.settled()?;
    d.check(
        "The identical file restores the Original without a new revision",
        object.is_file()
            && d.revision() == revision
            && d.app()
                .message
                .as_deref()
                .is_some_and(|message| message.contains("found and verified")),
        json!({"object":true,"revision":revision}),
        json!({"object":object.is_file(),"revision":d.revision(),"message":d.app().message}),
    )?;
    d.wait_for("Beat thumbnails decoded again", |app| {
        app.thumbnails
            .rendered_for_revision(app.workspace.as_deref())
            > 0
    })?;
    d.capture("Original restored")?;
    Ok(())
}

/// A refused save never reads as saved, keeps an alert up, and clears after
/// a real save.
pub(super) fn storage_failure(d: &mut Driver<'_>) -> Result<(), String> {
    denied_other_project(d)?;
    super::transcript::focus_your_edit(d)?;
    d.chord(&[Key::G, Key::G, Key::Num1, Key::Num0, Key::L])?;
    let before = d.revision();
    d.app().service.inject_storage_failure_for_check();
    d.command("split")?;
    d.wait_for("Refusal delivered", |app| {
        app.project_error.is_some() && !app.service.is_busy()
    })?;
    d.step("Alert painted", true)?;
    let error = d.app().project_error.clone().unwrap_or_default();
    d.check(
        "A full disk is reported as not saved, with the kept state and an action",
        d.revision() == before
            && error.contains("Not saved: the disk is full.")
            && error.contains("last saved edit is intact")
            && d.app()
                .message
                .as_deref()
                .is_none_or(|message| !message.contains("saved"))
            && d.app().recovery.storage.as_ref().is_some_and(|alert| alert.code == "DiskFull"),
        json!({"revision":before,"error":"Not saved: the disk is full…","alert":"DiskFull"}),
        json!({"revision":d.revision(),"error":error,"message":d.app().message,"alert":format!("{:?}", d.app().recovery.storage)}),
    )?;
    let shown = |text: &str| {
        d.harness.root().children_recursive().any(|node| {
            let access = node.accesskit_node();
            access.label().as_deref() == Some(text) || access.value().as_deref() == Some(text)
        })
    };
    let (not_saved, saved) = (shown("Not saved"), shown("Saved"));
    d.check(
        "The header says Not saved instead of Saved",
        not_saved && !saved,
        json!({"not_saved":true,"saved":false}),
        json!({"not_saved":not_saved,"saved":saved}),
    )?;
    d.capture("Disk full: not saved")?;
    d.key(Key::L)?;
    d.step("Navigation keeps the alert", true)?;
    d.check(
        "The alert persists through navigation and the editor stays usable",
        d.app().recovery.storage.is_some(),
        json!(true),
        json!(d.app().recovery.storage.is_some()),
    )?;
    d.command("split")?;
    d.changed(&before)?;
    d.check(
        "The next successful save clears the alert",
        d.app().recovery.storage.is_none() && d.app().project_error.is_none(),
        json!({"alert":null}),
        json!({"alert":format!("{:?}", d.app().recovery.storage),"error":d.app().project_error}),
    )?;
    d.capture("Saved again after space returned")?;
    close_with_preview(d)
}

/// A real candidate-package EACCES must leave the retained edit marked Saved.
fn denied_other_project(d: &mut Driver<'_>) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt as _;
    let workspace = d.app().workspace.clone().ok_or("No project")?;
    let path = workspace
        .path
        .parent()
        .ok_or("No fixture parent")?
        .join("denied-open.deadpan");
    let document = deadpan_core::ProjectDocument::new_automatic(
        deadpan_core::ProjectId::new("denied-open").map_err(|e| e.to_string())?,
        deadpan_core::RevisionId::new("initial").map_err(|e| e.to_string())?,
        deadpan_core::NodeId::new("root").map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    drop(ProjectStore::create(&path, &document).map_err(|e| e.to_string())?);
    let lock = path.join(".writer.lock");
    let permissions = std::fs::metadata(&lock)
        .map_err(|e| e.to_string())?
        .permissions();
    std::fs::set_permissions(&lock, std::fs::Permissions::from_mode(0o000))
        .map_err(|e| e.to_string())?;
    d.app_mut().submit(ProjectRequest::Open(path));
    let delivered = d.wait_for("Permission-denied Open delivered", |app| {
        !app.service.is_busy()
            && app.project_error.as_ref().is_some_and(|error| {
                crate::recovery::storage_code(error) == Some("PermissionDenied")
            })
    });
    std::fs::set_permissions(lock, permissions).map_err(|e| e.to_string())?;
    delivered?;
    d.step("Retained saved workspace after denied Open", true)?;
    let shown = |label: &str| {
        d.harness.root().children_recursive().any(|node| {
            let access = node.accesskit_node();
            access.label().as_deref() == Some(label) || access.value().as_deref() == Some(label)
        })
    };
    let (saved, not_saved) = (shown("Saved"), shown("Not saved"));
    d.check(
        "Failed Open keeps the existing project, revision and Saved header without a storage alert",
        d.app().workspace.as_ref().is_some_and(|current| {
            current.session == workspace.session
                && current.document.revision_id() == workspace.document.revision_id()
        }) && d.app().recovery.storage.is_none() && saved && !not_saved,
        json!({"session":workspace.session,"saved":true,"not_saved":false,"alert":null}),
        json!({"session":d.app().workspace.as_ref().map(|w| w.session),"saved":saved,"not_saved":not_saved,"alert":format!("{:?}",d.app().recovery.storage)}),
    )?;
    d.capture("Another project cannot open; current edit remains saved")
}

/// A window close with an open Camera draft asks first; Escape keeps it.
fn close_with_preview(d: &mut Driver<'_>) -> Result<(), String> {
    d.chord(&[Key::Comma, Key::F])?;
    d.wait_for("Camera draft open", |app| app.camera.is_some())?;
    d.harness
        .input_mut()
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .ok_or("Missing replay viewport")?
        .events
        .push(egui::ViewportEvent::Close);
    d.step("Window close requested", true)?;
    d.step("Close prompt focused", true)?;
    d.check(
        "Closing with a Camera draft asks first and focuses Keep editing",
        d.app().recovery.close_prompt.as_deref() == Some(&["Camera framing"][..])
            && !d.app().close_pending
            && focused(d).as_deref() == Some("Keep editing  Esc"),
        json!({"prompt":["Camera framing"],"closing":false,"focused":"Keep editing  Esc"}),
        json!({"prompt":d.app().recovery.close_prompt,"closing":d.app().close_pending,"focused":focused(d)}),
    )?;
    d.capture("Close with unsaved previews")?;
    d.key(Key::Escape)?;
    d.check(
        "Escape keeps editing with the draft intact",
        d.app().recovery.close_prompt.is_none()
            && !d.app().close_pending
            && d.app().camera.is_some(),
        json!({"prompt":null,"camera":true}),
        json!({"prompt":d.app().recovery.close_prompt,"camera":d.app().camera.is_some()}),
    )?;
    d.key(Key::Escape)?;
    d.settled()
}
