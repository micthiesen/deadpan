//! Backups and the read-only view of a newer package through real keys:
//! `:backups` opens Storage on the project's backups, B backs up now, J/K
//! choose a backup whose revision, beats and length are read from it, O asks
//! and a second O restores it as a new session after backing up the current
//! state. Reopening the package after marking it as written by a newer
//! Deadpan shows **Read-only** and refuses edits without writing. Only the
//! schema number and an extra table are changed on disk to imitate a newer
//! build; backups, verification and restore are real.

use egui::Key;

use super::*;
use deadpan_store::backups::{BackupReason, list_backups};

fn labels(d: &Driver<'_>) -> Vec<String> {
    d.harness
        .root()
        .children_recursive()
        .take(4096)
        .filter_map(|node| {
            let access = node.accesskit_node();
            access
                .label()
                .map(|label| label.to_string())
                .or_else(|| access.value().map(|value| value.to_string()))
        })
        .collect()
}

fn status(d: &Driver<'_>) -> String {
    d.app().storage.backups.status.clone().unwrap_or_default()
}

fn split(d: &mut Driver<'_>) -> Result<String, String> {
    super::transcript::focus_your_edit(d)?;
    d.chord(&[Key::G, Key::G, Key::Num1, Key::Num0, Key::L])?;
    let before = d.revision();
    d.command("split")?;
    d.changed(&before)?;
    Ok(d.revision())
}

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.report.skipped.push(
        "The newer package is imitated by raising its schema number and adding a table; no newer build is run.".into(),
    );
    let path = d.app().workspace.as_ref().ok_or("No project")?.path.clone();
    // Per-user caches are measured from a private root, never the person's.
    let root = d.options.output.join("backups-caches");
    let _ = std::fs::remove_dir_all(&root);
    d.app_mut().storage.user = Some(deadpan_cli::storage::UserStorage {
        caches: root.join("Caches/Deadpan"),
        support: root.join("Application Support/Deadpan"),
    });
    let saved = d.revision();
    let session = d
        .app()
        .workspace
        .as_ref()
        .map(|workspace| workspace.session);

    d.command("backups")?;
    d.wait_for("Backups listed", |app| {
        app.storage.open && app.storage.backups.listed()
    })?;
    d.settled()?;
    d.check(
        "`:backups` opens Storage with a BACKUPS section and its keys",
        d.app().storage.open
            && d.rect("Back up now  B").is_ok()
            && labels(d).iter().any(|label| label == "Restore…  O")
            && labels(d).iter().any(|label| label.starts_with("None yet.")),
        json!({"open":true,"buttons":["Back up now  B","Restore…  O (disabled without a backup)"],"list":"None yet."}),
        d.widgets(),
    )?;
    d.key(Key::B)?;
    d.wait_for("Backed up", |app| {
        app.storage
            .backups
            .status
            .as_deref()
            .is_some_and(|status| status.starts_with("Backed up") || status.contains("not"))
            && app.storage.backups.count() >= 1
    })?;
    d.settled()?;
    let listed = list_backups(&path).map_err(|error| error.to_string())?;
    d.check(
        "B makes a verified manual backup of the saved revision without editing",
        listed
            .first()
            .is_some_and(|backup| backup.reason == BackupReason::Manual)
            && d.revision() == saved
            && labels(d)
                .iter()
                .any(|label| label.starts_with("Chosen backup: ")),
        json!({"reason":"manual","revision":saved}),
        json!({"status":status(d),"listed":listed.len(),"revision":d.revision()}),
    )?;
    d.capture("A manual backup in Storage")?;
    d.key(Key::Escape)?;
    d.settled()?;

    let edited = split(d)?;
    d.command("backups")?;
    d.wait_for("Backup contents read", |app| {
        app.storage.open && app.storage.backups.previewed()
    })?;
    d.settled()?;
    let preview = labels(d)
        .into_iter()
        .find(|label| label.starts_with("Revision "))
        .unwrap_or_default();
    d.check(
        "The chosen backup shows the revision, beats and length it holds",
        preview.starts_with(&format!("Revision {saved} · ")) && preview.contains("long"),
        json!(format!("Revision {saved} · … beats · … long · … edits")),
        json!(preview),
    )?;
    d.key(Key::O)?;
    d.settled()?;
    d.check(
        "The first O asks before restoring and changes nothing",
        status(d).contains("Press O again") && d.revision() == edited,
        json!("… Press O again to restore …"),
        json!({"status":status(d),"revision":d.revision()}),
    )?;
    d.capture("Restore asks first")?;
    d.key(Key::O)?;
    d.wait_for("Restored", |app| {
        app.workspace
            .as_ref()
            .is_some_and(|workspace| Some(workspace.session) != session)
            && !app.service.is_busy()
    })?;
    d.settled()?;
    let safety = list_backups(&path)
        .map_err(|error| error.to_string())?
        .into_iter()
        .any(|backup| backup.reason == BackupReason::BeforeRestore);
    d.check(
        "The second O restores the backup's revision as a new session after backing up the edit",
        d.revision() == saved && safety && status(d).starts_with("Restored"),
        json!({"revision":saved,"before_restore_backup":true}),
        json!({"revision":d.revision(),"before_restore_backup":safety,"status":status(d)}),
    )?;
    d.capture("Restored from a backup")?;
    d.key(Key::Escape)?;
    d.settled()?;

    // A package a newer Deadpan saved opens read-only.
    d.app_mut().submit(ProjectRequest::Close);
    d.wait_for("Project closed", |app| {
        app.workspace.is_none() && !app.service.is_busy()
    })?;
    {
        let connection = rusqlite::Connection::open(path.join("project.sqlite"))
            .map_err(|error| error.to_string())?;
        connection
            .execute_batch("CREATE TABLE future_feature(id INTEGER PRIMARY KEY) STRICT;")
            .map_err(|error| error.to_string())?;
        connection
            .pragma_update(
                None,
                "user_version",
                deadpan_store::DATABASE_SCHEMA_VERSION + 1,
            )
            .map_err(|error| error.to_string())?;
        connection
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
            .map_err(|error| error.to_string())?;
    }
    let before = std::fs::read(path.join("project.sqlite")).map_err(|error| error.to_string())?;
    d.app_mut().submit(ProjectRequest::Open(path.clone()));
    d.wait_for("Newer project opened", |app| {
        app.workspace.is_some() && !app.service.is_busy()
    })?;
    d.settled()?;
    let header = labels(d)
        .into_iter()
        .find(|label| label.starts_with("Read-only"))
        .unwrap_or_default();
    d.check(
        "A newer package opens read-only and the header says why instead of Saved",
        header.contains("newer Deadpan") && !labels(d).iter().any(|label| label == "Saved"),
        json!("Read-only: This project was saved by a newer Deadpan …"),
        json!(header),
    )?;
    d.capture("A newer project, read-only")?;
    let revision = d.revision();
    super::transcript::focus_your_edit(d)?;
    d.chord(&[Key::G, Key::G, Key::Num1, Key::Num0, Key::L])?;
    d.command("split")?;
    d.wait_for("Edit answered", |app| {
        !app.service.is_busy() && app.project_error.is_some()
    })?;
    d.settled()?;
    let after = std::fs::read(path.join("project.sqlite")).map_err(|error| error.to_string())?;
    d.check(
        "Edits are refused with the explanation and nothing is written",
        d.revision() == revision
            && d.app()
                .project_error
                .as_deref()
                .is_some_and(|error| error.starts_with("Not saved") && error.contains("newer Deadpan"))
            && after == before,
        json!({"revision":revision,"error":"Not saved: … newer Deadpan …","database":"unchanged"}),
        json!({"revision":d.revision(),"error":d.app().project_error,"database_unchanged":after == before}),
    )?;
    d.capture("An edit refused in a read-only project")
}
