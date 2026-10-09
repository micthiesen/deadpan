//! Actual corrupt database, verified backups and production keyboard recovery.

use super::*;
use deadpan_store::backups::{BackupLimits, BackupPolicy, BackupReason, create_backup};
use egui::{Key, Modifiers};
use std::sync::atomic::AtomicBool;

fn focus(d: &mut Driver<'_>, label: &str) -> Result<(), String> {
    for _ in 0..32 {
        if d.harness.root().children_recursive().any(|node| {
            let access = node.accesskit_node();
            access.is_focused() && !access.is_disabled() && access.label().as_deref() == Some(label)
        }) {
            return Ok(());
        }
        d.key(Key::Tab)?;
        d.step("Settle recovery control focus", false)?;
    }
    Err(format!("Tab did not reach {label}"))
}

fn activate(d: &mut Driver<'_>, label: &str) -> Result<(), String> {
    focus(d, label)?;
    d.key(Key::Enter)
}

fn prepare(
    d: &mut Driver<'_>,
    damage_manifest: bool,
) -> Result<(std::path::PathBuf, String, String), String> {
    let workspace = d.app().workspace.as_ref().ok_or("No project")?;
    let path = workspace.path.clone();
    let revision = workspace.document.revision_id().to_string();
    let project = workspace.document.project_id().to_string();
    create_backup(
        &path,
        BackupReason::Manual,
        &BackupPolicy::default(),
        BackupLimits::default(),
        &AtomicBool::new(false),
    )
    .map_err(|error| error.to_string())?;
    d.app_mut().submit(ProjectRequest::Close);
    d.wait_for(
        "Close and drain owned backups before damaging the fixture",
        |app| {
            app.workspace.is_none()
                && !app.service.is_busy()
                && !app.storage.backups.owned_workers_active_for_check
        },
    )?;
    // Read-only backup connections can leave a valid WAL after they close.
    // Remove that recovery route only in this disposable corruption fixture.
    for name in [
        "project.sqlite-wal",
        "project.sqlite-shm",
        "project.sqlite-journal",
    ] {
        match std::fs::remove_file(path.join(name)) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    std::fs::write(path.join("project.sqlite"), b"unreadable replay database")
        .map_err(|error| error.to_string())?;
    if damage_manifest {
        std::fs::write(path.join("manifest.json"), b"unreadable replay manifest")
            .map_err(|error| error.to_string())?;
    }
    // Only choosing the package is scripted. Open admission, inspection,
    // confirmation, replacement and the new workspace are production paths.
    d.app_mut().submit(ProjectRequest::Open(path.clone()));
    d.wait_for("Failed Open offers its backups", |app| {
        app.damaged.open() && !app.service.is_busy()
    })?;
    d.step("Paint and focus failed-open recovery", false)?;
    Ok((path, revision, project))
}

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    for damaged_manifest in [false, true] {
        let (path, revision, project) = prepare(d, damaged_manifest)?;
        d.check(
            "A corrupt project offers recovery without opening it",
            d.app().workspace.is_none() && d.rect("Check selected backup").is_ok(),
            json!({"workspace":false,"recovery":true}),
            d.widgets(),
        )?;
        if !damaged_manifest {
            for (width, height) in [(960.0, 640.0), (1280.0, 820.0)] {
                let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height));
                let input = d.harness.input_mut();
                input.screen_rect = Some(rect);
                input
                    .viewports
                    .get_mut(&egui::ViewportId::ROOT)
                    .ok_or("Missing viewport")?
                    .inner_rect = Some(rect);
                d.step("Resize failed-open recovery", true)?;
                d.step("Settle resized recovery", false)?;
                for label in [
                    "Recover a project from backup",
                    "Check selected backup",
                    "Close recovery  Esc",
                ] {
                    let paint = scenarios::text_paint_visibility(d, label);
                    d.check(
                        "Recovery controls remain fully painted",
                        !paint.is_empty() && paint.iter().all(|item| item["fully_visible"] == true),
                        json!(label),
                        json!(paint),
                    )?;
                }
                d.capture(&format!("Failed-open recovery at {width}x{height}"))?;
            }
        }
        activate(d, "Check selected backup")?;
        d.wait_for("Selected recovery backup verified", |app| {
            app.damaged.inspected_for_check() && !app.service.is_busy()
        })?;
        d.step("Paint verified backup", false)?;
        d.step("Settle focus after backup inspection", false)?;
        let check_focused = d.harness.root().children_recursive().any(|node| {
            let access = node.accesskit_node();
            access.is_focused() && access.label().as_deref() == Some("Check selected backup")
        });
        d.check(
            "Inspection keeps keyboard focus on Check selected backup",
            check_focused,
            json!("Check selected backup"),
            d.widgets(),
        )?;
        d.key(Key::Tab)?;
        d.step("Settle one Tab after inspection", false)?;
        let next = if damaged_manifest {
            "Confirm project ID"
        } else {
            "Restore checked backup"
        };
        let next_focused = d.harness.root().children_recursive().any(|node| {
            let access = node.accesskit_node();
            access.is_focused() && access.label().as_deref() == Some(next)
        });
        d.check(
            "One Tab after inspection reaches the next recovery action",
            next_focused,
            json!(next),
            d.widgets(),
        )?;
        d.check(
            "Inspection leaves the damaged database untouched",
            std::fs::read(path.join("project.sqlite")).map_err(|error| error.to_string())?
                == b"unreadable replay database",
            json!("unreadable replay database"),
            json!("inspection completed"),
        )?;
        if damaged_manifest {
            let disabled = d.harness.root().children_recursive().any(|node| {
                let access = node.accesskit_node();
                access.label().as_deref() == Some("Restore checked backup") && access.is_disabled()
            });
            d.check(
                "Unreadable manifest requires an explicit typed identity",
                disabled && d.rect("Confirm project ID").is_ok(),
                json!("disabled restore and identity field"),
                d.widgets(),
            )?;
            focus(d, "Confirm project ID")?;
            d.events(
                "Type the confirmed backup project identity",
                vec![egui::Event::Text(project)],
            )?;
        }
        focus(d, "Restore checked backup")?;
        for label in [
            "Recover a project from backup",
            "Restore checked backup",
            "Close recovery  Esc",
        ] {
            let paint = scenarios::text_paint_visibility(d, label);
            d.check(
                "Verified recovery keeps its heading and final actions visible",
                !paint.is_empty() && paint.iter().all(|item| item["fully_visible"] == true),
                json!(label),
                json!(paint),
            )?;
        }
        d.events(
            "IME confirmation cannot restore a database",
            vec![
                egui::Event::Ime(egui::ImeEvent::Commit("候補".into())),
                egui::Event::Key {
                    key: Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                },
                egui::Event::Key {
                    key: Key::Enter,
                    physical_key: None,
                    pressed: false,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                },
            ],
        )?;
        d.check(
            "IME confirmation leaves the original damaged files intact",
            d.app().workspace.is_none()
                && std::fs::read(path.join("project.sqlite")).map_err(|error| error.to_string())?
                    == b"unreadable replay database",
            json!("no restored workspace"),
            d.snapshot(),
        )?;
        d.key(Key::Enter)?;
        d.wait_for("Restored backup opens as a new native session", |app| {
            app.damaged.restored_for_check() && app.workspace.is_some() && !app.service.is_busy()
        })?;
        d.check(
            "Recovery opens the exact saved revision",
            d.revision() == revision,
            json!(revision),
            json!(d.revision()),
        )?;
        d.capture(if damaged_manifest {
            "Recovered after explicit project identity confirmation"
        } else {
            "Recovered project and preserved damaged files"
        })?;
        activate(d, "Close recovery  Esc")?;
        d.wait_for("Recovery closed", |app| {
            !app.damaged.open() && !app.service.is_busy()
        })?;
        d.step("Restore editor focus", false)?;
        d.settled()?;
        d.check(
            "Closing recovery restores editor keyboard focus",
            d.harness.ctx.memory(|memory| memory.focused()) == Some(pane_id(d.app().pane)),
            json!(format!("{:?}", pane_id(d.app().pane))),
            d.snapshot(),
        )?;
    }
    Ok(())
}
