//! The Storage panel through real keys: `:storage` opens a labelled dialog
//! whose project and cache rows are accessible "label: value" text, P
//! previews a cleanup on a read-only open off the writer, R removes nothing without a
//! removable preview, editor keys never reach the project, S saves a
//! verified portable copy through the (scripted) save sheet, C cleans only
//! the replay's private cache root, E reviews then confirms the clock after a
//! long gap since the last AI variant retention check (and offers nothing
//! when the clock is behind the project's records), and Escape closes it.

use egui::Key;

use super::*;
use crate::dialogs::{DialogKind, Dialogs};

fn labels(d: &Driver<'_>) -> Vec<(String, String, bool)> {
    d.harness
        .root()
        .children_recursive()
        .take(4096)
        .map(|node| {
            let access = node.accesskit_node();
            (
                format!("{:?}", access.role()),
                access
                    .label()
                    .map(|label| label.to_string())
                    .or_else(|| access.value().map(|value| value.to_string()))
                    .unwrap_or_default(),
                access.is_focused(),
            )
        })
        .collect()
}

fn row(d: &Driver<'_>, prefix: &str) -> Option<String> {
    labels(d)
        .into_iter()
        .map(|(_, text, _)| text)
        .find(|text| text.starts_with(prefix))
}

const ROWS: [&str; 6] = [
    "Database: ",
    "Originals: ",
    "AI pause media: ",
    "Removable now: ",
    "Proxies: ",
    "Downloader staging: ",
];

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.report.skipped.push(
        "The save sheet is scripted and per-user caches use a private replay root; the copy, its verification and cache cleanup are real.".into(),
    );
    let root = d.options.output.join("storage");
    let _ = std::fs::remove_dir_all(&root);
    let user = deadpan_cli::storage::UserStorage {
        caches: root.join("Caches/Deadpan"),
        support: root.join("Application Support/Deadpan"),
    };
    let abandoned = user.support.join("helpers/.staging/abandoned");
    std::fs::create_dir_all(&abandoned).map_err(|error| error.to_string())?;
    std::fs::write(abandoned.join("download"), b"partial").map_err(|error| error.to_string())?;
    // Nothing in the abandoned download has changed for three days.
    for path in [abandoned.join("download"), abandoned.clone()] {
        std::fs::File::open(&path)
            .and_then(|file| {
                file.set_modified(
                    std::time::SystemTime::now() - std::time::Duration::from_secs(3 * 86_400),
                )
            })
            .map_err(|error| error.to_string())?;
    }
    std::fs::File::open(&abandoned)
        .and_then(|directory| {
            directory.set_modified(
                std::time::SystemTime::now() - std::time::Duration::from_secs(3 * 86_400),
            )
        })
        .map_err(|error| error.to_string())?;
    d.app_mut().storage.user = Some(user);
    let revision = d.revision();

    d.command("storage")?;
    d.wait_for("Storage measured", |app| app.storage.snapshot.is_some())?;
    d.settled()?;
    let all = labels(d);
    let dialog = all
        .iter()
        .any(|(role, label, _)| role == "Dialog" && label == "Storage");
    let focus = all
        .iter()
        .find(|(_, _, focused)| *focused)
        .map(|(_, label, _)| label.clone());
    let rows: Vec<Option<String>> = ROWS.iter().map(|prefix| row(d, prefix)).collect();
    d.check(
        "`:storage` opens a labelled dialog, focus on Preview cleanup, every project and cache row accessible",
        d.app().storage.open
            && dialog
            && focus.as_deref() == Some("Preview cleanup  P")
            && rows.iter().all(Option::is_some),
        json!({"dialog":"Storage","focus":"Preview cleanup  P","rows":ROWS}),
        json!({"open":d.app().storage.open,"dialog":dialog,"focus":focus,"rows":rows}),
    )?;
    d.capture("Storage beside the picture")?;

    d.key(Key::P)?;
    d.wait_for("Cleanup previewed", |app| {
        app.storage
            .status
            .as_deref()
            .is_some_and(|status| status == "Nothing is removable now.")
    })?;
    d.key(Key::R)?;
    d.settled()?;
    d.check(
        "P previews off the writer; R removes nothing when the preview found nothing",
        d.app().storage.status.as_deref() == Some("Nothing is removable now.")
            && d.revision() == revision,
        json!("Nothing is removable now."),
        json!(d.app().storage.status),
    )?;

    d.chord(&[Key::X, Key::J, Key::D, Key::D])?;
    d.settled()?;
    d.check(
        "Editor keys neither edit nor move while Storage is open",
        d.app().storage.open && d.revision() == revision && d.app().transport.is_none(),
        json!({"open":true,"revision":revision}),
        json!({"open":d.app().storage.open,"revision":d.revision()}),
    )?;

    d.key(Key::C)?;
    d.wait_for("Caches cleaned", |app| {
        app.storage
            .status
            .as_deref()
            .is_some_and(|status| status.starts_with("Removed 1 cache"))
    })?;
    d.check(
        "C removes only the abandoned download in the private cache root",
        !abandoned.exists(),
        json!("abandoned staging removed"),
        json!({"exists":abandoned.exists(),"status":d.app().storage.status}),
    )?;

    let destination = root.join("Portable.deadpan");
    d.app_mut().dialogs =
        Dialogs::scripted(vec![(DialogKind::PortableCopy, Some(destination.clone()))]);
    d.key(Key::S)?;
    d.wait_for("Portable copy saved", |app| {
        app.storage.status.as_deref().is_some_and(|status| {
            status.starts_with("Saved a portable copy")
                || status.starts_with("The portable copy failed")
        })
    })?;
    let status = d.app().storage.status.clone().unwrap_or_default();
    let reopened =
        deadpan_store::ProjectStore::open(&destination, deadpan_store::AccessMode::ReadOnly)
            .and_then(|store| {
                store.validate_full()?;
                store.head_revision()
            });
    d.check(
        "S saves a verified portable copy with the current revision",
        status.starts_with("Saved a portable copy")
            && reopened
                .as_ref()
                .is_ok_and(|head| head.as_str() == revision),
        json!({"status":"Saved a portable copy …","revision":revision}),
        json!({"status":status,"reopened":format!("{reopened:?}")}),
    )?;
    d.capture("Storage after cleanup and a portable copy")?;
    clock(d)?;

    d.key(Key::Escape)?;
    d.settled()?;
    for frame in 0..2 {
        d.step(&format!("Storage closed {frame}"), false)?;
    }
    d.check(
        "Escape closes Storage without an edit",
        !d.app().storage.open && d.revision() == revision && row(d, "Database: ").is_none(),
        json!({"open":false,"revision":revision}),
        json!({"open":d.app().storage.open,"revision":d.revision()}),
    )?;
    d.capture("Storage closed")
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as i64)
}

/// Set the retention watermark directly, as an earlier session would have.
fn seed_watermark(d: &Driver<'_>, last_pass_ms: i64) -> Result<(), String> {
    let path = d
        .app()
        .workspace
        .as_ref()
        .ok_or("no project")?
        .path
        .join("project.sqlite");
    rusqlite::Connection::open(path)
        .and_then(|connection| {
            connection.execute(
                "INSERT INTO generation_retention_state(singleton,last_pass_ms) VALUES (1,?1)
                 ON CONFLICT(singleton) DO UPDATE SET last_pass_ms=excluded.last_pass_ms",
                [last_pass_ms],
            )
        })
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn watermark(d: &Driver<'_>) -> Result<Option<i64>, String> {
    let path = d
        .app()
        .workspace
        .as_ref()
        .ok_or("no project")?
        .path
        .join("project.sqlite");
    rusqlite::Connection::open(path)
        .and_then(|connection| {
            connection.query_row(
                "SELECT max(last_pass_ms) FROM generation_retention_state",
                [],
                |row| row.get(0),
            )
        })
        .map_err(|error| error.to_string())
}

fn anomaly(app: &DeadpanApp) -> Option<Option<deadpan_store::generation_retention::ClockAnomaly>> {
    match &app.storage.snapshot {
        Some(super::super::storage::Snapshot {
            project: Some(Ok(report)),
            ..
        }) => Some(report.variant_retention.clock_anomaly),
        _ => None,
    }
}

fn clock(d: &mut Driver<'_>) -> Result<(), String> {
    use deadpan_store::generation_retention::ClockAnomaly;
    const DAY_MS: i64 = 24 * 60 * 60 * 1000;
    // The session's own automatic check runs first; it must not race the seed.
    d.wait_for("Automatic retention check finished", |app| {
        app.storage.retention.as_ref().is_some_and(|status| {
            matches!(
                status.state,
                crate::project::RetentionPassState::Done { .. }
            )
        })
    })?;
    let revision = d.revision();

    // A long gap since the last check: E reviews, a second E confirms.
    let seeded = now_ms() - 30 * DAY_MS;
    seed_watermark(d, seeded)?;
    d.key(Key::U)?;
    d.wait_for("Long gap measured", |app| {
        matches!(anomaly(app), Some(Some(ClockAnomaly::Ahead { .. })))
    })?;
    d.settled()?;
    let clock_row = row(d, "Clock: ");
    let button = labels(d)
        .iter()
        .any(|(_, label, _)| label == "Confirm clock  E");
    d.check(
        "A long gap since the last retention check shows a Clock row and offers Confirm clock (E)",
        clock_row
            .as_deref()
            .is_some_and(|text| text.contains("long gap") && text.contains("E reviews"))
            && button,
        json!({"clock":"long gap … E reviews and confirms the clock","button":"Confirm clock  E"}),
        json!({"clock":clock_row,"button":button}),
    )?;
    d.key(Key::E)?;
    d.wait_for("Clock review planned", |app| {
        app.storage
            .status
            .as_deref()
            .is_some_and(|status| status.starts_with("Confirm this Mac's clock?"))
    })?;
    let review = d.app().storage.status.clone().unwrap_or_default();
    d.check(
        "The first E shows exactly what would stop being offered, with its bytes, and writes nothing",
        review.contains("0 AI variants (0 B) would stop being offered")
            && review.contains("Press E again")
            && watermark(d)? == Some(seeded),
        json!({"status":"Confirm this Mac's clock? 0 AI variants (0 B) would stop being offered now; … Press E again to confirm.","watermark":seeded}),
        json!({"status":review,"watermark":watermark(d)?}),
    )?;
    d.capture("Storage reviewing the clock after a long gap")?;
    d.key(Key::E)?;
    d.wait_for("Clock confirmed", |app| {
        app.storage.status.as_deref().is_some_and(|status| {
            status.starts_with("Confirmed the clock") || !status.starts_with("Confirm")
        }) && matches!(anomaly(app), Some(None))
    })?;
    let status = d.app().storage.status.clone().unwrap_or_default();
    let advanced = watermark(d)?;
    d.check(
        "The second E confirms on the writer: the watermark advances, the anomaly clears, nothing is edited",
        status.starts_with("Confirmed the clock: 0 AI variants (0 B) stopped being offered")
            && advanced.is_some_and(|ms| ms > seeded + 29 * DAY_MS)
            && d.revision() == revision
            && row(d, "Clock: ").is_none(),
        json!({"status":"Confirmed the clock: 0 AI variants (0 B) stopped being offered; …","watermark":"now","revision":revision}),
        json!({"status":status,"watermark":advanced,"revision":d.revision(),"clock":row(d, "Clock: ")}),
    )?;

    // A clock behind the project's records: shown, never confirmed.
    let ahead = now_ms() + 10 * DAY_MS;
    seed_watermark(d, ahead)?;
    d.key(Key::U)?;
    d.wait_for("Clock behind measured", |app| {
        matches!(anomaly(app), Some(Some(ClockAnomaly::Behind { .. })))
    })?;
    d.settled()?;
    d.key(Key::E)?;
    d.settled()?;
    let status = d.app().storage.status.clone().unwrap_or_default();
    let button = labels(d)
        .iter()
        .any(|(_, label, _)| label == "Confirm clock  E");
    let clock_row = row(d, "Clock: ");
    d.check(
        "A clock behind the project's records is shown but offers no confirmation",
        status.contains("earlier than times this project recorded")
            && status.contains("nothing to confirm")
            && !button
            && clock_row.as_deref().is_some_and(|text| text.starts_with("Clock: earlier"))
            && watermark(d)? == Some(ahead)
            && d.app().storage.clock.is_none(),
        json!({"status":"… earlier than times this project recorded … nothing to confirm","button":false,"watermark":ahead}),
        json!({"status":status,"button":button,"clock":clock_row,"watermark":watermark(d)?}),
    )?;
    d.capture("Storage with a clock behind the project's records")?;
    // Leave the project as the replay found it.
    seed_watermark(d, now_ms())
}
