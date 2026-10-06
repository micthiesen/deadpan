//! The Storage panel through real keys: `:storage` opens a labelled dialog
//! whose project and cache rows are accessible "label: value" text, P
//! previews a cleanup on a read-only open off the writer, R removes nothing without a
//! removable preview, editor keys never reach the project, S saves a
//! verified portable copy through the (scripted) save sheet, C cleans only
//! the replay's private cache root, and Escape closes it.

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
