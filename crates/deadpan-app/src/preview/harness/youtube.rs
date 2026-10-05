//! Start a project from a YouTube URL on the empty start surface.
//!
//! Only the downloader is scripted (no yt-dlp, Deno or network). The URL
//! step, live validation, job thread, explicit helper install, confirmation,
//! progress, cancellation, project creation from the fixture through the real
//! single-Original path, and the ordinary Open are all production code.

use super::*;
use egui::{Event, Key, Modifiers};

const PLAYLIST: &str = "https://www.youtube.com/playlist?list=PLx0sYbCqOb8TBPRdmBHs5Iftvv9TPboYG";
const URL: &str = "https://youtu.be/Z4C82eyhwgU?si=share";

fn state(d: &Driver<'_>) -> Value {
    let app = d.app();
    json!({
        "step": app.youtube.step_name(),
        "failure": app.youtube.failure_code(),
        "url": app.youtube.url,
        "cookies": app.youtube.cookies,
        "modal": app.youtube.modal,
        "field_focused": field_focused(d),
        "workspace": app.workspace.as_ref().map(|workspace| workspace.path.clone()),
        "message": app.message,
        "error": app.error,
        "requests": d.downloader().map(|downloader| downloader.requests.lock().map(|requests| requests.len()).unwrap_or(0)),
        "installed": d.downloader().map(|downloader| downloader.installed()),
    })
}

fn field_focused(d: &Driver<'_>) -> bool {
    d.harness
        .ctx
        .memory(|memory| memory.has_focus(egui::Id::new(super::super::youtube::URL_ID)))
}

impl Driver<'_> {
    fn downloader(&self) -> Option<Arc<crate::youtube::scripted::Scripted>> {
        self.app().feedback.youtube.clone()
    }
}

fn current_step(d: &Driver<'_>) -> &'static str {
    d.app().youtube.step_name()
}

/// Entries in the private replay library, including hidden staging packages.
fn library(root: &Path) -> Vec<String> {
    std::fs::read_dir(root)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// Every text run containing `needle` is painted inside its clip and viewport.
fn painted(d: &mut Driver<'_>, name: &str, needle: &str) -> Result<(), String> {
    let paint = scenarios::text_paint_visibility(d, needle);
    d.check(
        name,
        !paint.is_empty() && paint.iter().all(|part| part["fully_visible"] == true),
        json!({"text": needle, "fully_visible": true}),
        json!(paint),
    )
}

fn text(d: &mut Driver<'_>, label: &str, events: Vec<Event>) -> Result<(), String> {
    d.events(label, events)
}

fn command_shift_n() -> Modifiers {
    Modifiers::MAC_CMD | Modifiers::COMMAND | Modifiers::SHIFT
}

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    start_card(d)?;
    validation(d)?;
    install(d)?;
    decline_and_cancel(d)?;
    success(d)?;
    sheet(d)
}

/// The board's "Choose one Original" card at default and minimum size.
fn start_card(d: &mut Driver<'_>) -> Result<(), String> {
    d.capture("Start card: choose a video, paste a YouTube URL or open")?;
    for needle in ["choose video", "YouTube URL", "open project"] {
        painted(d, "The start footer shows only start keys", needle)?;
    }
    let editor_keys = scenarios::text_paint_visibility(d, "select moment");
    d.check(
        "The start footer omits editor keys that cannot act without a project",
        editor_keys.is_empty(),
        json!([]),
        json!(editor_keys),
    )?;
    for needle in [
        "Start with one video.",
        "Make it weird.",
        "Choose video…",
        "Start project",
        "Library location: Documents / Deadpan",
        "Open project…",
    ] {
        painted(d, "Start card text is fully painted", needle)?;
    }
    let field = d.rect("YOUTUBE URL")?;
    let choose = d.rect("Choose video…  ⌘N")?;
    let open = d.rect("Open project…  ⌘O")?;
    d.check(
        "The URL field sits between Choose video and Open project, as in the board",
        choose.bottom() < field.top() && field.bottom() < open.top(),
        json!("choose < url < open"),
        json!({"choose": [choose.min.y, choose.max.y], "url": [field.min.y, field.max.y], "open": [open.min.y, open.max.y]}),
    )?;
    d.harness.set_size(egui::vec2(960.0, 640.0));
    d.step("Paint the minimum start surface", false)?;
    d.capture("Start card at 960×640")?;
    let viewport = d.harness.ctx.content_rect();
    let mut hits = Vec::new();
    for label in ["Choose video…  ⌘N", "YOUTUBE URL", "Open project…  ⌘O"] {
        let rect = d.rect(label)?;
        hits.push(
            json!({"label": label, "rect": [rect.min.x, rect.min.y, rect.max.x, rect.max.y]}),
        );
        d.check(
            "Every start control remains inside the minimum window",
            viewport.contains_rect(rect),
            json!({"label": label, "inside": [viewport.min.x, viewport.min.y, viewport.max.x, viewport.max.y]}),
            json!(hits),
        )?;
    }
    for needle in [
        "Start with one video.",
        "Choose video…",
        "Start project",
        "Choose cookies file…",
        "Open project…",
    ] {
        painted(
            d,
            "The minimum start card paints every control without scrolling",
            needle,
        )?;
    }
    d.harness.set_size(egui::vec2(1280.0, 820.0));
    d.step("Paint the default start surface", false)?;
    Ok(())
}

/// ⌘⇧N focuses the field; a playlist without a video is refused live and
/// Enter starts nothing; select-all plus paste of a share URL validates.
fn validation(d: &mut Driver<'_>) -> Result<(), String> {
    d.key_modified(Key::N, command_shift_n())?;
    d.step("Focus the URL field", false)?;
    d.check(
        "⌘⇧N focuses the YouTube URL field on the start surface",
        field_focused(d) && !d.app().youtube.modal,
        json!({"field_focused": true, "modal": false}),
        state(d),
    )?;
    text(d, "Type a playlist URL", vec![Event::Text(PLAYLIST.into())])?;
    // Typed editor keys stay in the field: no command entry, no navigation.
    text(d, "Type editor keys", vec![Event::Text(" :j".into())])?;
    let typed = d.app().youtube.url.clone();
    d.check(
        "Native text entry owns typed characters, including editor keys",
        typed == format!("{PLAYLIST} :j") && !d.app().command_open && field_focused(d),
        json!({"url": format!("{PLAYLIST} :j"), "command_open": false}),
        json!({"url": typed, "command_open": d.app().command_open, "state": state(d)}),
    )?;
    for _ in 0..3 {
        d.key(Key::Backspace)?;
    }
    painted(
        d,
        "The playlist refusal is shown while typing",
        "This is a playlist; choose a specific video from it.",
    )?;
    let start = d
        .harness
        .root()
        .children_recursive()
        .find(|node| node.accesskit_node().label().as_deref() == Some("Start project  Enter"));
    let disabled = start.is_some_and(|node| node.accesskit_node().is_disabled());
    d.capture("Playlist URL refused before any work")?;
    d.key(Key::Enter)?;
    d.step("Enter on a refused URL", false)?;
    d.check(
        "Enter on a refused URL starts no job and keeps the field",
        disabled && current_step(d) == "idle" && field_focused(d) && state(d)["requests"] == 0,
        json!({"start_disabled": true, "step": "idle", "requests": 0, "field_focused": true}),
        json!({"start_disabled": disabled, "state": state(d)}),
    )?;
    d.key_modified(Key::A, Modifiers::MAC_CMD | Modifiers::COMMAND)?;
    text(
        d,
        "Paste a share URL over the selection",
        vec![Event::Paste(URL.into())],
    )?;
    painted(
        d,
        "A supported URL shows its normalized video",
        "YouTube video Z4C82eyhwgU · Enter fetches its details",
    )?;
    d.check(
        "Paste replaces the selected text with the share URL",
        d.app().youtube.url == URL,
        json!(URL),
        state(d),
    )?;
    painted(
        d,
        "The focused field's footer teaches its own keys",
        "fetch details",
    )?;
    d.capture("Share URL normalized to one video")?;
    // Command chords beside the field keep their ordinary meaning.
    for (key, kind, label) in [
        (
            Key::N,
            DialogKind::CreateProject,
            "⌘N from the focused URL field chooses a video",
        ),
        (
            Key::O,
            DialogKind::OpenProject,
            "⌘O from the focused URL field opens a project",
        ),
    ] {
        d.key_modified(key, Modifiers::MAC_CMD | Modifiers::COMMAND)?;
        let opened = d.app().dialogs.is_open();
        d.step("Scripted picker cancelled", false)?;
        d.check(
            label,
            opened
                && !d.app().dialogs.is_open()
                && d.app().youtube.url == URL
                && current_step(d) == "idle",
            json!({"picker": format!("{kind:?}"), "url": URL}),
            json!({"picker_opened": opened, "state": state(d)}),
        )?;
    }
    d.key_modified(Key::N, command_shift_n())?;
    d.step("Refocus the URL field", false)?;
    d.check(
        "⌘⇧N from the start surface returns to the URL field with its text",
        field_focused(d) && d.app().youtube.url == URL,
        json!({"field_focused": true, "url": URL}),
        state(d),
    )?;
    Ok(())
}

/// A missing downloader is explained and installed only on request.
fn install(d: &mut Driver<'_>) -> Result<(), String> {
    d.key(Key::Enter)?;
    d.wait_for("Install offer", |app| {
        app.youtube.step_name() == "needs-downloader"
    })?;
    d.step("Paint the install offer", false)?;
    painted(
        d,
        "The install offer names its size",
        "Install downloader (75.6 MB)",
    )?;
    painted(
        d,
        "The install offer explains verification",
        "pinned SHA-256",
    )?;
    d.capture("Downloader install offer")?;
    d.check(
        "Nothing is installed without the explicit action",
        state(d)["installed"] == false && state(d)["requests"] == 1,
        json!({"installed": false, "requests": 1}),
        state(d),
    )?;
    d.key(Key::Escape)?;
    d.step("Decline the install", false)?;
    d.step("Refocus the field", false)?;
    d.check(
        "Escape declines the install and returns to the field",
        current_step(d) == "idle" && field_focused(d) && state(d)["installed"] == false,
        json!({"step": "idle", "field_focused": true, "installed": false}),
        state(d),
    )?;
    d.key(Key::Enter)?;
    d.wait_for("Install offer again", |app| {
        app.youtube.step_name() == "needs-downloader"
    })?;
    d.step("Paint the install offer", false)?;
    d.click("Install downloader (75.6 MB)  Enter")?;
    d.wait_for("Details after the install", |app| {
        app.youtube.step_name() == "confirm"
    })?;
    d.check(
        "The explicit install continues the same import to its details",
        state(d)["installed"] == true && state(d)["requests"] == 3,
        json!({"installed": true, "requests": 3}),
        state(d),
    )?;
    Ok(())
}

/// Check the confirmation; returns the library directory.
fn confirm_details(d: &mut Driver<'_>) -> Result<std::path::PathBuf, String> {
    d.step("Paint the details", false)?;
    for needle in [
        "Caminandes 2: Gran Dillama",
        "Blender · 2:26 · uploaded 2014-11-30",
        "1920 × 1080 · 24 fps · H.264 (avc1.640028)",
        "AAC (mp4a.40.2) · 44.1 kHz · 130 kb/s",
        "Documents/Deadpan/Caminandes 2 Gran Dillama.deadpan",
        "You are responsible for having the rights to use this video.",
        "Download and create",
    ] {
        painted(
            d,
            "Confirmation shows title, length, streams and destination",
            needle,
        )?;
    }
    let crate::youtube::Status::Confirm { destination, .. } = d.app().youtube.jobs.status().clone()
    else {
        return Err("No confirmation".into());
    };
    let root = destination
        .parent()
        .ok_or("Destination has no library")?
        .to_path_buf();
    d.check(
        "Nothing is downloaded or created before confirmation",
        library(&root).is_empty(),
        json!({"library_entries": []}),
        json!({"library": library(&root), "destination": destination}),
    )?;
    Ok(root)
}

/// Escape declines at confirmation; cancelling a held transfer removes it.
fn decline_and_cancel(d: &mut Driver<'_>) -> Result<(), String> {
    let root = confirm_details(d)?;
    d.capture("Details before any transfer")?;
    d.key(Key::Escape)?;
    d.wait_for("Declined", |app| app.youtube.step_name() == "cancelled")?;
    d.step("Paint the cancelled notice", false)?;
    painted(
        d,
        "Declining says nothing was created",
        "Import cancelled. No project was created.",
    )?;
    d.capture("Declined at confirmation")?;
    d.key(Key::Enter)?;
    d.wait_for("Details again", |app| app.youtube.step_name() == "confirm")?;
    d.key(Key::Enter)?;
    d.wait_for("Held transfer at 40%", |app| {
        matches!(
            app.youtube.jobs.status(),
            crate::youtube::Status::Working(crate::youtube::Stage::Downloading { downloaded, .. })
                if *downloaded > 0
        )
    })?;
    d.step("Paint the transfer", false)?;
    painted(
        d,
        "Download progress shows percent and bytes",
        "40% · 22.0 MB of 55.1 MB",
    )?;
    painted(
        d,
        "Pending stages are listed",
        "Checking and retaining the Original",
    )?;
    d.capture("Downloading 40%")?;
    d.key(Key::Escape)?;
    d.wait_for("Cancelled transfer", |app| {
        app.youtube.step_name() == "cancelled" && !app.youtube.jobs.running()
    })?;
    d.step("Paint the cancelled transfer", false)?;
    let workspace = d.app().workspace.is_some();
    d.check(
        "Cancelling a transfer leaves no project and no workspace",
        !workspace && library(&root).is_empty(),
        json!({"workspace": false, "library_entries": []}),
        json!({"state": state(d), "library": library(&root)}),
    )?;
    d.capture("Transfer cancelled")?;
    Ok(())
}

/// Choose cookies, confirm by pointer, finish and open like a local project.
fn success(d: &mut Driver<'_>) -> Result<(), String> {
    d.check(
        "After cancellation the field has focus again",
        field_focused(d),
        json!({"field_focused": true}),
        state(d),
    )?;
    d.downloader()
        .ok_or("No scripted downloader")?
        .fail_next(crate::youtube::Failure::new(
            "YouTubeAgeRestricted",
            "Sign in to confirm your age. This video may be inappropriate for some users.",
        ));
    d.key(Key::Enter)?;
    d.wait_for("Age-restricted refusal", |app| {
        app.youtube.failure_code() == Some("YouTubeAgeRestricted")
    })?;
    d.step("Paint the refusal", false)?;
    painted(
        d,
        "The refusal has a plain title",
        "This video is age-restricted",
    )?;
    painted(
        d,
        "The refusal says what to do next in app terms",
        "Choose a cookies file exported from a signed-in browser, then try again.",
    )?;
    painted(d, "The stable code stays visible", "YouTubeAgeRestricted")?;
    d.capture("Age-restricted video asks for an explicit cookies file")?;
    d.click("Choose cookies file…")?;
    d.wait_for("Cookies chosen", |app| app.youtube.cookies.is_some())?;
    d.step("Paint the cookies choice", false)?;
    painted(
        d,
        "The chosen cookies file is named",
        "Cookies: cookies.txt",
    )?;
    d.click("Start project  Enter")?;
    d.wait_for("Details with cookies", |app| {
        app.youtube.step_name() == "confirm"
    })?;
    let cookies = d.downloader().and_then(|downloader| {
        downloader
            .requests
            .lock()
            .ok()
            .and_then(|requests| requests.last().and_then(|request| request.cookies.clone()))
    });
    d.check(
        "The explicit cookies file reaches the import unchanged",
        cookies
            .as_ref()
            .is_some_and(|path| path.ends_with("cookies.txt")),
        json!("…/cookies.txt"),
        json!({"cookies": cookies}),
    )?;
    let destination = match d.app().youtube.jobs.status() {
        crate::youtube::Status::Confirm { destination, .. } => destination.clone(),
        _ => return Err("No confirmation".into()),
    };
    d.click("Download and create  Enter")?;
    d.wait_for("Held transfer", |app| {
        app.youtube.step_name() == "downloading"
    })?;
    d.downloader().ok_or("No scripted downloader")?.release();
    d.wait_for("Project created and opened", |app| {
        app.workspace.as_ref().is_some_and(|workspace| {
            matches!(
                workspace.single_source,
                Some(SingleSourceState::Ready { .. })
            )
        }) && app.youtube.step_name() == "idle"
            && app.presentation.has_displayed()
            && !app.presentation.loading()
    })?;
    d.settled()?;
    let workspace = d.app().workspace.clone().ok_or("No workspace")?;
    let canonical = destination
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let label = workspace
        .document
        .assets()
        .values()
        .map(|asset| asset.label.clone())
        .collect::<Vec<_>>();
    d.check(
        "The created package opens like a local new project, labelled by its title",
        workspace.path == canonical
            && label == ["Caminandes 2: Gran Dillama"]
            && d.app().message.as_deref()
                == Some("Created “Caminandes 2: Gran Dillama” from YouTube as Caminandes 2 Gran Dillama")
            && d.app().youtube.url.is_empty()
            && d.app().youtube.cookies.is_none(),
        json!({"path": canonical, "labels": ["Caminandes 2: Gran Dillama"], "message": "Created “Caminandes 2: Gran Dillama” from YouTube as Caminandes 2 Gran Dillama"}),
        json!({"path": workspace.path, "labels": label, "state": state(d)}),
    )?;
    d.capture("Opened the project created from YouTube")?;
    Ok(())
}

/// Over an open project the same step is a sheet that owns the keyboard.
fn sheet(d: &mut Driver<'_>) -> Result<(), String> {
    let revision = d.revision();
    let cursor = d.app().source_cursor;
    d.key_modified(Key::N, command_shift_n())?;
    d.step("Open the sheet", false)?;
    d.step("Focus the sheet field", false)?;
    painted(d, "The sheet names its purpose", "NEW PROJECT FROM YOUTUBE")?;
    text(
        d,
        "Type editor keys into the sheet",
        vec![Event::Text("ll".into())],
    )?;
    d.check(
        "The sheet's field owns editor keys; the project is unchanged",
        d.app().youtube.modal
            && field_focused(d)
            && d.app().youtube.url == "ll"
            && d.app().source_cursor == cursor
            && d.revision() == revision,
        json!({"modal": true, "url": "ll", "cursor": cursor, "revision": revision}),
        json!({"state": state(d), "cursor": d.app().source_cursor, "revision": d.revision()}),
    )?;
    painted(d, "The sheet's footer teaches Escape as close", "close")?;
    d.capture("New-from-URL sheet over the open project")?;
    d.key(Key::Escape)?;
    d.step("Close the sheet", false)?;
    d.check(
        "Escape closes the idle sheet and returns to the project",
        !d.app().youtube.modal && !field_focused(d) && d.revision() == revision,
        json!({"modal": false, "field_focused": false}),
        state(d),
    )?;
    d.capture("Sheet closed")?;
    d.report.skipped.extend([
        "yt-dlp, Deno, network transfer and stream assembly: the downloader is scripted; docs/YOUTUBE_IMPORT.md records the real headless run".into(),
        "Native cookies picker: only its selected path is scripted".into(),
        "Thumbnail: not fetched or shown".into(),
    ]);
    Ok(())
}
