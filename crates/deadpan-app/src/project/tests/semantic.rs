//! Semantic intent follows durable actor commits, independently of UI refresh.

use super::*;
use crate::project::marks;
use crate::project::registers::{Bank, Value};
use crate::project::semantic::{CutAttempt, LastEdit, RepeatableCut, RepeatableEdit, Snapshot};
use crate::project::slice::{CaptureRequest, Captured, CopyId};
use deadpan_cli::host::Client;
use deadpan_cli::live_project::{self, HistoryDirection, Operation, Reply, ShortOperation};
use deadpan_core::{FrameCut, SliceCaptureSelection};

mod selectors;

fn setup(path: &Path) -> (Harness, ProjectUpdate) {
    drop(seed_holds(path, &["a", "b", "c"]));
    let harness = Harness::new();
    let opened = command(&harness.service, ProjectRequest::Open(path.into()));
    assert!(opened.error.is_none(), "{:?}", opened.error);
    assert!(opened.semantic.as_ref().unwrap().edit.is_none());
    (harness, opened)
}

fn capture(
    workspace: &Workspace,
    ticket: u64,
    register: Option<char>,
    start: i64,
    end: i64,
) -> CaptureRequest {
    CaptureRequest {
        id: CopyId {
            session: workspace.session,
            project: workspace.document.project_id().clone(),
            source_revision: workspace.document.revision_id().clone(),
            request: ticket,
            persisted_version: None,
        },
        register,
        scope: SequenceScope::default(),
        parent: workspace.document.root().clone(),
        selection: SliceCaptureSelection::Range {
            range: FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap(),
        },
    }
}

fn frame_cut(
    workspace: &Workspace,
    ticket: u64,
    register: Option<char>,
    cursor: i64,
    count: u32,
    repeat_version: Option<u64>,
) -> ProjectRequest {
    let operation = FrameCut::new(count).unwrap();
    let range = operation
        .resolve(
            &workspace.document,
            workspace.document.root(),
            ProjectFrame(cursor),
        )
        .unwrap();
    ProjectRequest::CutFrames {
        capture: capture(workspace, ticket, register, range.start().0, range.end().0),
        attempt: CutAttempt {
            operation: RepeatableCut::Frames(operation),
            repeat_version,
        },
    }
}

fn cut(service: &ProjectService, request: ProjectRequest) -> ProjectUpdate {
    let update = command(service, request);
    assert!(update.error.is_none(), "{:?}", update.error);
    assert!(
        update.cut_slice.as_ref().unwrap().result.is_ok(),
        "{:?}",
        update.cut_slice
    );
    update
}

fn snapshot(update: &ProjectUpdate) -> &Snapshot {
    let snapshot = update.semantic.as_ref().unwrap();
    assert!(snapshot.error.is_none(), "{:?}", snapshot.error);
    snapshot
}

fn intent(update: &ProjectUpdate, count: u32, register: Option<char>) -> &Snapshot {
    let snapshot = snapshot(update);
    assert_eq!(
        snapshot.edit,
        Some(LastEdit {
            operation: RepeatableEdit::Cut(RepeatableCut::Frames(FrameCut::new(count).unwrap())),
            register
        })
    );
    snapshot
}

fn copied(bank: &Bank, name: char) -> &Arc<Captured> {
    let Value::Edited(copied) = &bank.entries[&name] else {
        panic!("expected edited copy")
    };
    copied
}

fn counts(path: &Path) -> (i64, i64) {
    rusqlite::Connection::open(path.join("project.sqlite"))
        .unwrap()
        .query_row(
            "SELECT (SELECT count(*) FROM revisions), (SELECT count(*) FROM history)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap()
}

fn saved(path: &Path) -> ProjectDocument {
    ProjectStore::open(path, AccessMode::ReadOnly)
        .unwrap()
        .snapshot()
        .unwrap()
}

fn fail_refresh(service: &ProjectService) {
    service
        .shared
        .workspace_refresh_failure
        .store(true, Ordering::Release);
}

#[test]
fn repeat_uses_the_new_cursor_and_original_count_with_one_undo_and_durable_copies() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("repeat.deadpan");
    let (harness, opened) = setup(&path);
    let before = opened.workspace.unwrap();
    let initial_rows = counts(&path);
    let first = cut(
        &harness.service,
        frame_cut(&before, 1, Some('a'), 28, 7, None),
    );
    let first_state = intent(&first, 7, Some('a')).clone();
    let first_view = first.workspace.as_ref().unwrap();
    assert_eq!(first_view.plan.duration().frames(), 28);
    assert_eq!(
        first_state.edit_for(first_view).unwrap().operation,
        RepeatableEdit::Cut(RepeatableCut::Frames(FrameCut::new(7).unwrap()))
    );
    assert_eq!(
        copied(first.registers.as_ref().unwrap(), 'a')
            .slice()
            .duration()
            .frames(),
        2
    );
    assert_eq!(counts(&path), (initial_rows.0 + 1, initial_rows.1 + 1));

    // A new explicit register replaces the destination retained by dot-repeat.
    let second = cut(
        &harness.service,
        frame_cut(first_view, 2, Some('z'), 3, 7, Some(first_state.version)),
    );
    let second_state = intent(&second, 7, Some('z')).clone();
    assert!(second_state.version > first_state.version);
    let second_view = second.workspace.as_ref().unwrap();
    assert_eq!(second_view.plan.duration().frames(), 21);
    let bank = second.registers.as_ref().unwrap();
    assert_eq!(copied(bank, 'a').slice().duration().frames(), 2);
    assert_eq!(
        copied(bank, 'z').slice().range(),
        FrameRange::new(ProjectFrame(3), ProjectFrame(10)).unwrap()
    );
    assert!(Arc::ptr_eq(copied(bank, 'z'), copied(bank, '"')));
    assert_eq!(counts(&path), (initial_rows.0 + 2, initial_rows.1 + 2));

    let undone = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: second_view.document.revision_id().clone(),
        },
    );
    intent(&undone, 7, Some('z'));
    let undone_view = undone.workspace.as_ref().unwrap();
    assert_eq!(undone_view.document.nodes(), first_view.document.nodes());
    assert_eq!(undone_view.plan.duration().frames(), 28);
    assert_ne!(
        undone_view.document.revision_id(),
        first_view.document.revision_id()
    );
    assert!(Arc::ptr_eq(undone.registers.as_ref().unwrap(), bank));
    let baseline = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: undone_view.document.revision_id().clone(),
        },
    );
    intent(&baseline, 7, Some('z'));
    assert_eq!(
        baseline.workspace.as_ref().unwrap().document.nodes(),
        before.document.nodes()
    );
    assert!(Arc::ptr_eq(baseline.registers.as_ref().unwrap(), bank));
    assert_eq!(counts(&path), (initial_rows.0 + 4, initial_rows.1 + 2));
}

#[test]
fn stale_repeat_versions_and_mismatched_operations_or_ranges_preserve_all_saved_state() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("rejected.deadpan");
    let (harness, opened) = setup(&path);
    let before = opened.workspace.unwrap();
    let initial = frame_cut(&before, 1, Some('a'), 0, 3, None);
    let ProjectRequest::CutFrames {
        capture: original_capture,
        attempt: original_attempt,
    } = initial
    else {
        panic!()
    };
    let first = cut(
        &harness.service,
        ProjectRequest::CutFrames {
            capture: original_capture.clone(),
            attempt: original_attempt.clone(),
        },
    );
    let state = intent(&first, 3, Some('a')).clone();
    let view = first.workspace.as_ref().unwrap();
    let bank = first.registers.as_ref().unwrap();
    let rows = counts(&path);
    let document = saved(&path);

    let requests = [
        frame_cut(view, 2, Some('b'), 5, 3, Some(state.version - 1)),
        frame_cut(view, 3, Some('b'), 5, 4, Some(state.version)),
        ProjectRequest::CutFrames {
            capture: capture(view, 4, Some('b'), 5, 7),
            attempt: CutAttempt {
                operation: RepeatableCut::Frames(FrameCut::new(3).unwrap()),
                repeat_version: None,
            },
        },
        frame_cut(&before, 5, Some('b'), 5, 3, Some(state.version)),
        ProjectRequest::CutFrames {
            capture: original_capture.clone(),
            attempt: CutAttempt {
                operation: RepeatableCut::Frames(FrameCut::new(4).unwrap()),
                repeat_version: None,
            },
        },
    ];
    for request in requests {
        let refused = command(&harness.service, request);
        assert!(refused.cut_slice.as_ref().unwrap().result.is_err());
        assert_eq!(snapshot(&refused), &state);
        assert!(Arc::ptr_eq(refused.workspace.as_ref().unwrap(), view));
        assert!(Arc::ptr_eq(refused.registers.as_ref().unwrap(), bank));
        assert_eq!(saved(&path), document);
        assert_eq!(counts(&path), rows);
    }
    let duplicate = cut(
        &harness.service,
        ProjectRequest::CutFrames {
            capture: original_capture,
            attempt: original_attempt,
        },
    );
    assert_eq!(snapshot(&duplicate), &state);
    assert_eq!(counts(&path), rows);
    assert_eq!(
        duplicate.saved_cut.unwrap().committed,
        first.saved_cut.unwrap().committed
    );
}

#[test]
fn saved_frame_cut_keeps_intent_after_refresh_failure_and_refuses_the_stale_view() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("saved-cut.deadpan");
    let (harness, opened) = setup(&path);
    let before = opened.workspace.unwrap();
    fail_refresh(&harness.service);
    let update = cut(
        &harness.service,
        frame_cut(&before, 1, Some('f'), 4, 6, None),
    );
    let receipt = update.saved_cut.as_ref().unwrap();
    assert!(
        receipt
            .refresh_error
            .as_ref()
            .unwrap()
            .contains("Cut saved and copied")
    );
    assert!(receipt.needs_refresh(update.workspace.as_deref()));
    assert!(Arc::ptr_eq(update.workspace.as_ref().unwrap(), &before));
    let state = intent(&update, 6, Some('f')).clone();
    assert_eq!(state.head.as_ref(), Some(&receipt.committed.revision));
    assert!(
        state
            .edit_for(&before)
            .unwrap_err()
            .contains("saved project changed")
    );
    assert_eq!(saved(&path).revision_id(), &receipt.committed.revision);
    assert_eq!(saved(&path).duration().unwrap().frames(), 24);
    let rows = counts(&path);
    let refused = command(
        &harness.service,
        frame_cut(&before, 2, Some('f'), 12, 6, Some(state.version)),
    );
    assert!(refused.cut_slice.as_ref().unwrap().result.is_err());
    assert_eq!(snapshot(&refused), &state);
    assert_eq!(counts(&path), rows);
    command(&harness.service, ProjectRequest::Close);
    let reopened = command(&harness.service, ProjectRequest::Open(path));
    assert!(snapshot(&reopened).edit.is_none());
    assert_eq!(
        reopened
            .workspace
            .as_ref()
            .unwrap()
            .plan
            .duration()
            .frames(),
        24
    );
    assert_eq!(
        copied(reopened.registers.as_ref().unwrap(), 'f')
            .slice()
            .duration()
            .frames(),
        6
    );
}

#[test]
fn unsupported_saved_edit_clears_intent_even_when_the_workspace_cannot_refresh() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("unsupported.deadpan");
    let (harness, opened) = setup(&path);
    let first = cut(
        &harness.service,
        frame_cut(opened.workspace.as_ref().unwrap(), 1, None, 0, 2, None),
    );
    let state = intent(&first, 2, None).clone();
    let view = first.workspace.as_ref().unwrap();
    fail_refresh(&harness.service);
    let changed = command(
        &harness.service,
        edit_request(
            view,
            ProjectEdit::HoldDuration {
                node: node("c"),
                duration: FrameDuration::new(11).unwrap(),
            },
        ),
    );
    assert!(changed.error.as_ref().unwrap().contains("Injected failure"));
    assert!(Arc::ptr_eq(changed.workspace.as_ref().unwrap(), view));
    let cleared = snapshot(&changed);
    assert!(cleared.version > state.version);
    assert!(cleared.edit.is_none());
    assert_eq!(
        cleared.head.as_ref(),
        Some(&changed.committed.as_ref().unwrap().revision)
    );
    assert_eq!(cleared.head.as_ref(), Some(saved(&path).revision_id()));
    assert_ne!(cleared.head.as_ref(), Some(view.document.revision_id()));
    assert_eq!(saved(&path).node_duration(&node("c")).unwrap().frames(), 11);
}

fn mark(workspace: &Workspace, ticket: u64) -> ProjectRequest {
    ProjectRequest::Marks(marks::Request {
        id: marks::Id {
            ticket,
            session: workspace.session,
            project: workspace.document.project_id().clone(),
            revision: workspace.document.revision_id().clone(),
        },
        operation: marks::Operation::Set {
            letter: 'm',
            location: marks::Location::Edit {
                scope: SequenceScope::default(),
                at: ProjectFrame(3),
                selected: None,
            },
        },
    })
}

fn remote_edit(
    document: &ProjectDocument,
    revision: &str,
    command: Command,
    dry_run: bool,
) -> Operation {
    Operation::Execute {
        project_id: document.project_id().clone(),
        command: Box::new(ShortOperation::Edit {
            request: Box::new(CommandRequest {
                project_id: document.project_id().clone(),
                expected_revision: document.revision_id().clone(),
                new_revision: RevisionId::new(revision).unwrap(),
                command,
            }),
            dry_run,
        }),
    }
}

fn remote_saved(
    service: &ProjectService,
    client: &mut Client,
    operation: Operation,
    revision: &str,
) -> ProjectUpdate {
    let Reply::Completed {
        committed_revision,
        refresh_error,
        ..
    } = live_project::request(client, operation).unwrap()
    else {
        panic!("expected committed headless edit")
    };
    assert_eq!(
        committed_revision.as_ref().map(RevisionId::as_str),
        Some(revision)
    );
    assert!(refresh_error.is_none(), "{refresh_error:?}");
    wait(service, |update| {
        snapshot(update)
            .head
            .as_ref()
            .is_some_and(|head| head.as_str() == revision)
    })
}

fn remote_history(
    document: &ProjectDocument,
    revision: &str,
    direction: HistoryDirection,
) -> Operation {
    Operation::Execute {
        project_id: document.project_id().clone(),
        command: Box::new(ShortOperation::History {
            direction,
            expected_revision: document.revision_id().clone(),
            new_revision: RevisionId::new(revision).unwrap(),
            dry_run: false,
        }),
    }
}

#[test]
fn native_and_authenticated_mark_saves_and_history_preserve_the_last_semantic_edit() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("marks.deadpan");
    let (harness, opened) = setup(&path);
    let first = cut(
        &harness.service,
        frame_cut(
            opened.workspace.as_ref().unwrap(),
            1,
            Some('r'),
            28,
            4,
            None,
        ),
    );
    let original_version = intent(&first, 4, Some('r')).version;
    let marked = command(&harness.service, mark(first.workspace.as_ref().unwrap(), 1));
    assert!(marked.marks.reply.as_ref().unwrap().result.is_ok());
    assert!(intent(&marked, 4, Some('r')).version > original_version);
    let mut client = Client::discover(&path).unwrap().unwrap();
    let mut current = remote_saved(
        &harness.service,
        &mut client,
        remote_edit(
            &marked.workspace.as_ref().unwrap().document,
            "remote-delete-mark",
            Command::DeleteMark {
                id: marks::mark_id('m').unwrap(),
            },
            false,
        ),
        "remote-delete-mark",
    );
    assert!(
        current
            .workspace
            .as_ref()
            .unwrap()
            .document
            .marks()
            .is_empty()
    );
    intent(&current, 4, Some('r'));
    let retained_mark =
        &marked.workspace.as_ref().unwrap().document.marks()[&marks::mark_id('m').unwrap()];
    current = remote_saved(
        &harness.service,
        &mut client,
        remote_edit(
            &current.workspace.as_ref().unwrap().document,
            "remote-set-mark",
            Command::SetMark {
                id: marks::mark_id('m').unwrap(),
                owner: retained_mark.owner.clone(),
                label: retained_mark.label.clone(),
                boundary: retained_mark.boundary.clone(),
                loss_policy: retained_mark.loss_policy,
            },
            false,
        ),
        "remote-set-mark",
    );
    intent(&current, 4, Some('r'));
    assert_eq!(
        current.workspace.as_ref().unwrap().document.marks()[&marks::mark_id('m').unwrap()],
        *retained_mark
    );
    for (revision, direction) in [
        ("remote-undo-mark", HistoryDirection::Undo),
        ("remote-redo-mark", HistoryDirection::Redo),
    ] {
        current = remote_saved(
            &harness.service,
            &mut client,
            remote_history(
                &current.workspace.as_ref().unwrap().document,
                revision,
                direction,
            ),
            revision,
        );
        intent(&current, 4, Some('r'));
    }
    for undo in [true, false] {
        let expected_revision = current
            .workspace
            .as_ref()
            .unwrap()
            .document
            .revision_id()
            .clone();
        current = command(
            &harness.service,
            if undo {
                ProjectRequest::Undo { expected_revision }
            } else {
                ProjectRequest::Redo { expected_revision }
            },
        );
        assert!(current.error.is_none(), "{:?}", current.error);
        intent(&current, 4, Some('r'));
    }
    let repeated = cut(
        &harness.service,
        frame_cut(
            current.workspace.as_ref().unwrap(),
            2,
            Some('r'),
            12,
            4,
            Some(snapshot(&current).version),
        ),
    );
    intent(&repeated, 4, Some('r'));
    assert_eq!(
        repeated
            .workspace
            .as_ref()
            .unwrap()
            .plan
            .duration()
            .frames(),
        24
    );
}

#[test]
fn failed_and_dry_run_headless_requests_preserve_intent_but_an_authored_mutation_clears_it() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("headless.deadpan");
    let (harness, opened) = setup(&path);
    let before = opened.workspace.unwrap();
    let first = cut(&harness.service, frame_cut(&before, 1, None, 0, 2, None));
    let state = intent(&first, 2, None).clone();
    let view = first.workspace.as_ref().unwrap();
    let mut client = Client::discover(&path).unwrap().unwrap();
    let rename = || Command::Rename {
        node: node("c"),
        label: "Renamed".into(),
    };
    let rows = counts(&path);
    let Reply::Completed {
        committed_revision, ..
    } = live_project::request(
        &mut client,
        remote_edit(&view.document, "preview", rename(), true),
    )
    .unwrap()
    else {
        panic!("expected dry run")
    };
    assert!(committed_revision.is_none());
    let stale = live_project::request(
        &mut client,
        remote_edit(&before.document, "stale", rename(), false),
    )
    .unwrap_err();
    assert_eq!(stale.code, "RevisionConflict");
    assert_eq!(
        stale.current_revision.as_ref(),
        Some(view.document.revision_id())
    );
    assert!(stale.committed_revision.is_none());
    let observed = command(&harness.service, ProjectRequest::CancelImport);
    assert_eq!(snapshot(&observed), &state);
    assert_eq!(counts(&path), rows);
    assert_eq!(saved(&path), *view.document);
    harness
        .service
        .shared
        .host_refresh_failure
        .store(true, Ordering::Release);
    let Reply::Completed {
        committed_revision,
        refresh_error,
        ..
    } = live_project::request(
        &mut client,
        remote_edit(&view.document, "remote-rename", rename(), false),
    )
    .unwrap()
    else {
        panic!("expected saved headless receipt")
    };
    assert_eq!(
        committed_revision.as_ref().map(RevisionId::as_str),
        Some("remote-rename")
    );
    assert!(refresh_error.as_ref().unwrap().contains("Injected failure"));
    let changed = wait(&harness.service, |update| {
        snapshot(update).head.as_ref() == committed_revision.as_ref()
    });
    assert!(snapshot(&changed).edit.is_none());
    assert!(snapshot(&changed).version > state.version);
    assert!(Arc::ptr_eq(changed.workspace.as_ref().unwrap(), view));
    assert_eq!(saved(&path).nodes()[&node("c")].label, "Renamed");
}

#[test]
fn an_untyped_slice_cut_clears_repeat_instead_of_replaying_an_older_frame_cut() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("ordinary-cut.deadpan");
    let (harness, opened) = setup(&path);
    let first = cut(
        &harness.service,
        frame_cut(opened.workspace.as_ref().unwrap(), 1, None, 0, 2, None),
    );
    intent(&first, 2, None);
    let ordinary = cut(
        &harness.service,
        ProjectRequest::CutEditSlice(capture(
            first.workspace.as_ref().unwrap(),
            2,
            Some('o'),
            12,
            15,
        )),
    );
    assert!(snapshot(&ordinary).edit.is_none());
    assert_eq!(
        copied(ordinary.registers.as_ref().unwrap(), 'o')
            .slice()
            .duration()
            .frames(),
        3
    );
}

#[test]
fn failed_open_preserves_the_session_while_replacement_close_and_reopen_clear_repeat() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("first.deadpan");
    let second_path = scratch.path().join("second.deadpan");
    drop(seed_holds(&second_path, &["other"]));
    let (harness, opened) = setup(&path);
    let first = cut(
        &harness.service,
        frame_cut(opened.workspace.as_ref().unwrap(), 1, None, 0, 2, None),
    );
    let state = intent(&first, 2, None).clone();
    let view = first.workspace.as_ref().unwrap();
    let failed = command(
        &harness.service,
        ProjectRequest::Open(scratch.path().join("missing.deadpan")),
    );
    assert!(failed.error.is_some());
    assert_eq!(snapshot(&failed), &state);
    assert!(Arc::ptr_eq(failed.workspace.as_ref().unwrap(), view));
    let replaced = command(&harness.service, ProjectRequest::Open(second_path));
    assert!(replaced.error.is_none(), "{:?}", replaced.error);
    assert_ne!(snapshot(&replaced).session, state.session);
    assert!(snapshot(&replaced).edit.is_none());
    assert!(
        state
            .edit_for(replaced.workspace.as_ref().unwrap())
            .is_err()
    );
    let replacement_state = snapshot(&replaced).clone();
    let replacement_path = &replaced.workspace.as_ref().unwrap().path;
    let replacement_rows = counts(replacement_path);
    let stale = command(
        &harness.service,
        frame_cut(view, 2, None, 4, 2, Some(state.version)),
    );
    assert!(stale.cut_slice.as_ref().unwrap().result.is_err());
    assert_eq!(snapshot(&stale), &replacement_state);
    assert_eq!(counts(replacement_path), replacement_rows);
    let closed = command(&harness.service, ProjectRequest::Close);
    assert!(closed.semantic.is_none());
    let reopened = command(&harness.service, ProjectRequest::Open(path));
    assert!(snapshot(&reopened).edit.is_none());
    assert_ne!(snapshot(&reopened).session, state.session);
    assert_eq!(
        reopened
            .workspace
            .as_ref()
            .unwrap()
            .plan
            .duration()
            .frames(),
        28
    );
}
