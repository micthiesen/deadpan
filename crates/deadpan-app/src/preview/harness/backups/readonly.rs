//! Real pending reducers in the readable-newer-package replay fixture.

use super::*;
use sha2::Digest;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub(super) fn seed(d: &mut Driver<'_>) -> Result<(), String> {
    d.command("source")?;
    d.chord(&[
        Key::G,
        Key::G,
        Key::Num1,
        Key::Num0,
        Key::L,
        Key::V,
        Key::Num1,
        Key::Num4,
        Key::L,
        Key::Y,
        Key::Escape,
    ])?;
    d.wait_for("Readonly fixture copy saved", |app| {
        !app.service.is_busy() && !app.copied.is_pending()
    })?;
    d.command("sequence")?;
    super::super::transcript::focus_your_edit(d)?;
    d.chord(&[Key::G, Key::G])?;
    let before = d.revision();
    d.key_modified(Key::P, egui::Modifiers::SHIFT)?;
    d.changed(&before)?;
    d.chord(&[Key::G, Key::G, Key::Num3, Key::L])?;
    let before = d.revision();
    d.chord(&[Key::M, Key::A])?;
    d.changed(&before)?;
    d.chord(&[Key::G, Key::G, Key::Num3, Key::Num0, Key::L])?;
    let before = d.revision();
    d.command("hold 11f")?;
    d.changed(&before)?;
    Ok(())
}

pub(super) fn orphan(path: &Path) -> Result<(), String> {
    let path = path.join("Media/Generated/.pending-readonly-replay");
    std::fs::write(&path, b"unfinished generated artifact").map_err(|error| error.to_string())?;
    std::fs::File::open(path)
        .and_then(|file| {
            file.set_modified(
                std::time::SystemTime::now() - std::time::Duration::from_secs(3 * 86_400),
            )
        })
        .map_err(|error| error.to_string())
}

fn bytes(root: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>, String> {
    fn visit(
        root: &Path,
        path: &Path,
        files: &mut BTreeMap<PathBuf, Vec<u8>>,
    ) -> std::io::Result<()> {
        for entry in std::fs::read_dir(path)? {
            let path = entry?.path();
            if path.is_dir() {
                visit(root, &path, files)?;
            } else if path
                .file_name()
                .is_some_and(|name| name != "project.sqlite-shm")
            {
                files.insert(
                    path.strip_prefix(root).expect("descendant").into(),
                    std::fs::read(path)?,
                );
            }
        }
        Ok(())
    }
    let mut files = BTreeMap::new();
    visit(root, root, &mut files).map_err(|error| error.to_string())?;
    Ok(files)
}

fn refusal(app: &DeadpanApp) -> bool {
    app.error
        .as_deref()
        .into_iter()
        .chain(app.project_error.as_deref())
        .any(|error| error.starts_with("Not saved:") && error.contains("newer Deadpan"))
}

fn byte_differences(
    before: &BTreeMap<PathBuf, Vec<u8>>,
    after: &BTreeMap<PathBuf, Vec<u8>>,
) -> Value {
    let changed: std::collections::BTreeSet<_> = before
        .keys()
        .chain(after.keys())
        .filter(|path| before.get(*path) != after.get(*path))
        .collect();
    let identity = |bytes: Option<&Vec<u8>>| {
        bytes.map(|bytes| {
            let hash = sha2::Sha256::digest(bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            json!({"bytes":bytes.len(),"sha256":hash})
        })
    };
    json!({"count":changed.len(),"paths":changed.iter().take(12).map(|path| {
        json!({"path":path,"before":identity(before.get(*path)),"after":identity(after.get(*path))})
    }).collect::<Vec<_>>()})
}

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let workspace = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No readonly workspace")?
        .clone();
    let before = bytes(&workspace.path)?;
    let revision = d.revision();
    d.chord(&[Key::M, Key::B])?;
    d.wait_for(
        "Readonly Mark Set settles its captured pending request",
        |app| !app.service.is_busy() && !app.marks.is_pending() && refusal(app),
    )?;
    d.check(
        "Readonly Mark Set clears pending and preserves the saved marks",
        !d.app().marks.is_pending()
            && !d
                .app()
                .workspace
                .as_ref()
                .unwrap()
                .document
                .marks()
                .contains_key(&crate::project::marks::mark_id('b')?),
        json!({"pending":false,"new_mark":false}),
        d.snapshot(),
    )?;
    d.chord(&[Key::Quote, Key::A])?;
    d.wait_for("Readonly mark jump settles normally", |app| {
        !app.service.is_busy() && !app.marks.is_pending() && app.sequence_cursor == 3
    })?;
    d.check(
        "Readonly mark Jump remains usable at the exact saved boundary",
        d.app().sequence_cursor == 3 && d.revision() == revision,
        json!({"cursor":3,"revision":revision}),
        d.snapshot(),
    )?;

    d.command("duplicate")?;
    d.wait_for(
        "Readonly semantic edit clears its pending macro receipt",
        |app| !app.service.is_busy() && !app.macros.is_pending() && refusal(app),
    )?;
    d.check(
        "Readonly semantic refusal clears pending without moving the captured cursor",
        !d.app().macros.is_pending() && d.app().sequence_cursor == 3 && d.revision() == revision,
        json!({"pending":false,"cursor":3}),
        d.snapshot(),
    )?;

    d.command("source")?;
    d.chord(&[Key::G, Key::G, Key::V, Key::Num3, Key::L, Key::Y])?;
    d.wait_for(
        "Readonly Original copy clears its register pending state",
        |app| !app.service.is_busy() && !app.copied.is_pending() && refusal(app),
    )?;
    d.check(
        "Refused Original copy preserves the previous durable register",
        d.app()
            .copied
            .original()
            .is_some_and(|copy| copy.ordinals == (10..24)),
        json!([10, 24]),
        d.snapshot(),
    )?;
    d.key(Key::Escape)?;
    d.command("sequence")?;
    super::super::transcript::focus_your_edit(d)?;
    d.chord(&[Key::G, Key::G, Key::Num3, Key::L])?;
    d.command("slip +2f")?;
    d.wait_for("Readonly Slip prepares a real private picture", |app| {
        app.slip
            .as_ref()
            .is_some_and(|draft| draft.ready_for_check())
    })?;
    // Slip records its admitted GPU ticket after painting the action row.
    // The first ready state therefore precedes its enabled Apply paint.
    d.step(
        "Paint Apply after the proposed Slip picture is admitted",
        false,
    )?;
    d.check(
        "Readonly Slip exposes the enabled Apply control after the picture is displayed",
        d.app()
            .slip
            .as_ref()
            .is_some_and(|draft| draft.ready_for_check())
            && d.rect("Apply Slip  Enter").is_ok(),
        json!({"picture_ready":true,"apply_enabled":true}),
        d.widgets(),
    )?;
    let captured = d
        .app()
        .slip
        .as_ref()
        .ok_or("No ready Slip draft")?
        .proposal_for_check()
        .clone();
    d.click("Apply Slip  Enter")?;
    d.wait_for("Readonly Slip Apply settles its captured commit", |app| {
        !app.service.is_busy()
            && app.slip.as_ref().is_some_and(|draft| {
                !draft.applying_for_check()
                    && draft
                        .error_for_check()
                        .is_some_and(|error| error.starts_with("Not saved:"))
            })
    })?;
    d.step(
        "Paint the refused Slip draft and its editable controls",
        false,
    )?;
    d.check(
        "Refused Slip retains its exact target and amount while invalidating the prepared commit",
        d.app().slip.as_ref().is_some_and(|draft| {
            !draft.applying_for_check()
                && draft.proposal_for_check() == &captured
                && draft.prepared_for_check().is_none()
                && !draft.ready_for_check()
                && draft.error_for_check().is_some_and(|error| error.starts_with("Not saved:"))
        }) && d.revision() == revision
            && d.rect("Apply Slip  Enter").is_err()
            && d.rect("+1f  l").is_ok()
            && d.rect("Signed Slip amount in project frames, for example +5f").is_ok(),
        json!({"applying":false,"captured_proposal":format!("{:?}", captured.id()),"requested":2,"prepared":false,"apply_enabled":false,"amount_editable":true,"revision":revision}),
        d.snapshot(),
    )?;
    d.click("+1f  l")?;
    d.wait_for(
        "The refused Slip can prepare a new amount on its retained target",
        |app| {
            app.slip
                .as_ref()
                .is_some_and(|draft| draft.ready_for_check())
        },
    )?;
    d.step(
        "Paint Apply for the new Slip amount after its picture is admitted",
        false,
    )?;
    d.check(
        "A native nudge after refusal produces a fresh usable preview without changing the project",
        d.app().slip.as_ref().is_some_and(|draft| {
            let proposal = draft.proposal_for_check();
            proposal.target == captured.target
                && proposal.draft == captured.draft
                && proposal.change > captured.change
                && proposal.delta_frames == 3
                && !draft.applying_for_check()
                && draft.error_for_check().is_none()
                && draft.ready_for_check()
                && draft.prepared_for_check().is_some_and(|prepared| {
                    prepared.target == captured.target
                        && prepared.resolution.requested_delta_frames == 3
                        && prepared.resolution.applied_delta_frames == 3
                })
        }) && d.revision() == revision
            && d.app().sequence_cursor == 3
            && d.rect("Apply Slip  Enter").is_ok(),
        json!({"same_target":true,"same_draft":captured.draft,"new_change":true,"requested":3,"applied":3,"apply_enabled":true,"revision":revision}),
        d.snapshot(),
    )?;
    d.key(Key::Escape)?;

    let ticket = d
        .app_mut()
        .submit_target(crate::project::targets::Operation::Track {
            ticket: 0,
            session: workspace.session,
            revision: workspace.document.revision_id().clone(),
            id: deadpan_core::TargetId::new("readonly-captured-target")
                .map_err(|error| error.to_string())?,
            mode: crate::project::targets::TrackMode::Track {
                through_shots: true,
            },
        })
        .ok_or("Readonly target request did not enqueue")?;
    d.wait_for(
        "Readonly target refusal clears the actual awaiting ticket",
        |app| {
            !app.service.is_busy()
                && !app.targets.pending_for_check()
                && app.targets.reply().is_some_and(|reply| {
                    reply.0 == ticket
                        && reply
                            .1
                            .as_deref()
                            .is_some_and(|error| error.starts_with("Not saved:"))
                })
        },
    )?;
    d.check(
        "Readonly target reply is the submitted ticket and leaves no job",
        !d.app().targets.pending_for_check() && d.app().targets.running().is_none(),
        json!({"ticket":ticket,"pending":false,"job":false}),
        d.snapshot(),
    )?;

    d.command("storage")?;
    // Unknown future tables could retain objects that this build cannot see.
    // Storage refuses even a cleanup preview, so the native R path cannot
    // obtain a removal plan in this session. Its typed service refusal is
    // covered separately by project::tests::readonly.
    for attempt in 1..=2 {
        let expected_starts = d
            .app()
            .storage
            .preview_workers_started_for_check()
            .checked_add(1)
            .ok_or("Storage cleanup preview counter overflow")?;
        d.key(Key::P)?;
        d.check(
            "P starts a fresh storage cleanup preview worker",
            d.app().storage.preview_workers_started_for_check() == expected_starts,
            json!({"attempt":attempt,"workers_started":expected_starts}),
            json!({"workers_started":d.app().storage.preview_workers_started_for_check(),"state":d.snapshot()}),
        )?;
        d.wait_for(
            "Newer-package cleanup preview answers with its schema refusal",
            |app| {
                app.storage.preview_workers_started_for_check() == expected_starts
                    && !app.storage.pending_for_check()
                    && app
                        .storage
                        .status
                        .as_deref()
                        .is_some_and(|status| status.contains("newer Deadpan"))
            },
        )?;
        d.check(
            "Cleanup preview refusal clears pending and leaves retry available without offering removal",
            !d.app().storage.pending_for_check()
                && d.rect("Preview cleanup  P").is_ok()
                && d.rect("Remove  R").is_err()
                && d.revision() == revision,
            json!({"attempt":attempt,"pending":false,"preview_enabled":true,"remove_enabled":false,"revision":revision}),
            d.snapshot(),
        )?;
    }
    d.key(Key::R)?;
    d.check(
        "R cannot submit removal without a valid preview and retains the unknown artifact",
        !d.app().service.is_busy()
            && !d.app().storage.pending_for_check()
            && d.app().storage.status.as_deref() == Some("Preview the cleanup with P first.")
            && workspace
                .path
                .join("Media/Generated/.pending-readonly-replay")
                .is_file(),
        json!({"pending":false,"status":"Preview the cleanup with P first.","artifact_retained":true}),
        d.snapshot(),
    )?;
    d.key(Key::Escape)?;

    super::super::transcript::focus_your_edit(d)?;
    d.chord(&[Key::G, Key::G, Key::Num3, Key::Num0, Key::L])?;
    d.command("room-tone")?;
    d.wait_for(
        "Readonly room-tone source preparation completes normally",
        |app| {
            !app.service.is_busy()
                && app
                    .room_tone
                    .as_ref()
                    .is_some_and(|draft| draft.prepared.is_some())
        },
    )?;
    d.check(
        "Readonly room-tone exposes the prepared source without an authored edit",
        d.app()
            .room_tone
            .as_ref()
            .is_some_and(|draft| draft.prepared.is_some())
            && d.revision() == revision,
        json!({"prepared":true,"revision":revision}),
        d.snapshot(),
    )?;
    d.key(Key::Escape)?;
    let after = bytes(&workspace.path)?;
    d.check(
        "Readonly pending workflows preserve DB, WAL, managed media and registers byte for byte",
        after == before && d.revision() == revision,
        json!({"package_bytes":"unchanged","revision":revision}),
        json!({"editor":d.snapshot(),"package_differences":byte_differences(&before, &after)}),
    )
}
