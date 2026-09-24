use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, SyncSender};
use std::time::{Duration, Instant};

use deadpan_core::{
    BeatNode, Command, CommandRequest, HoldAudio, HoldRecipe, HoldVideo, NodeKind, ProjectId,
    Subtree,
};
use deadpan_store::{AccessMode, ProjectStore};

use super::*;

mod pause;

const TIMEOUT: Duration = Duration::from_secs(20);

fn node(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}

fn hold(frames: i64) -> HoldRecipe {
    HoldRecipe {
        duration: FrameDuration::new(frames).unwrap(),
        video: HoldVideo::Background,
        audio: HoldAudio::Silence,
    }
}

fn seed_command(store: &mut ProjectStore, command: Command, revision: &str) {
    let document = store.snapshot().unwrap();
    store
        .commit(&CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: RevisionId::new(revision).unwrap(),
            command,
        })
        .unwrap();
}

fn seed_holds(path: &Path, names: &[&str]) -> ProjectStore {
    let document = ProjectDocument::new_automatic(
        ProjectId::new("edits").unwrap(),
        RevisionId::new("initial").unwrap(),
        node("root"),
    )
    .unwrap();
    let mut store = ProjectStore::create(path, &document).unwrap();
    for (index, name) in names.iter().enumerate() {
        seed_command(
            &mut store,
            Command::Insert {
                parent: node("root"),
                index,
                subtree: Subtree {
                    root: node(name),
                    nodes: BTreeMap::from([(node(name), BeatNode::hold(*name, hold(10)))]),
                    overrides: BTreeMap::new(),
                },
            },
            &format!("insert-{name}"),
        );
    }
    store
}

fn edit_request(workspace: &Workspace, edit: ProjectEdit) -> ProjectRequest {
    ProjectRequest::Edit {
        expected_session: workspace.session,
        expected_revision: workspace.document.revision_id().clone(),
        edit,
    }
}

fn edited(service: &ProjectService, workspace: &Workspace, edit: ProjectEdit) -> ProjectUpdate {
    let update = command(service, edit_request(workspace, edit));
    assert!(update.error.is_none(), "{:?}", update.error);
    assert_eq!(
        &update.committed.as_ref().unwrap().revision,
        update.workspace.as_ref().unwrap().document.revision_id()
    );
    update
}

#[test]
fn native_split_refines_the_original_and_restores_it_through_durable_history() {
    let scratch = tempfile::tempdir().unwrap();
    let service = ProjectService::start(
        Arc::new(|| {}),
        Some(ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap()),
    )
    .unwrap();
    service
        .submit(ProjectRequest::CreateFromSource {
            path: fixture("cfr-bframes.mp4"),
        })
        .unwrap();
    let initialized = wait(&service, |update| {
        update.import.as_ref().is_some_and(|status| {
            matches!(status.stage, ImportStage::Complete | ImportStage::Failed)
        })
    });
    assert!(initialized.error.is_none(), "{:?}", initialized.error);
    let original = initialized.committed.unwrap().selected_node.unwrap();
    let before = initialized.workspace.unwrap();
    let first = edited(
        &service,
        &before,
        ProjectEdit::Split {
            node: original.clone(),
            at: FrameDuration::new(37).unwrap(),
        },
    );
    let right = first.committed.unwrap().selected_node.unwrap();
    let first = first.workspace.unwrap();
    let NodeKind::Sequence { children } = &first.document.nodes()[first.document.root()].kind
    else {
        panic!("root")
    };
    assert_eq!(children.len(), 2);
    assert_eq!(children[1], right);
    assert_eq!(first.plan.node_duration(&children[0]).unwrap().frames(), 37);
    let stale = command(
        &service,
        edit_request(
            &before,
            ProjectEdit::Split {
                node: original.clone(),
                at: FrameDuration::new(20).unwrap(),
            },
        ),
    );
    assert!(stale.error.unwrap().contains("changed"));
    assert_eq!(*stale.workspace.unwrap().document, *first.document);
    for at in [0, 83, 84] {
        let invalid = command(
            &service,
            edit_request(
                &first,
                ProjectEdit::Split {
                    node: right.clone(),
                    at: FrameDuration::new(at).unwrap(),
                },
            ),
        );
        assert!(invalid.error.is_some());
        assert!(invalid.committed.is_none());
        assert_eq!(*invalid.workspace.unwrap().document, *first.document);
    }
    let refined = edited(
        &service,
        &first,
        ProjectEdit::Split {
            node: right,
            at: FrameDuration::new(29).unwrap(),
        },
    );
    let selected = refined.committed.unwrap().selected_node.unwrap();
    let refined = refined.workspace.unwrap();
    let NodeKind::Sequence { children } = &refined.document.nodes()[refined.document.root()].kind
    else {
        panic!("root")
    };
    assert_eq!(children.len(), 3);
    assert_eq!(children[2], selected);
    assert_eq!(refined.document.nodes().len(), 7);
    for workspace in [&first, &refined] {
        assert_eq!(workspace.single_source, before.single_source);
        assert_eq!(workspace.document.assets(), before.document.assets());
        assert_eq!(
            workspace.document.duration().unwrap(),
            before.document.duration().unwrap()
        );
        for frame in 0..120 {
            assert_eq!(
                workspace
                    .plan
                    .picture(deadpan_core::ProjectFrame(frame))
                    .unwrap()
                    .picture,
                before
                    .plan
                    .picture(deadpan_core::ProjectFrame(frame))
                    .unwrap()
                    .picture
            );
        }
    }
    command(&service, ProjectRequest::Close);
    let reopened = command(&service, ProjectRequest::Open(before.path.clone()))
        .workspace
        .unwrap();
    assert_eq!(*reopened.document, *refined.document);
    let undone = command(
        &service,
        ProjectRequest::Undo {
            expected_revision: reopened.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    assert_eq!(undone.document.nodes(), first.document.nodes());
    let baseline = command(
        &service,
        ProjectRequest::Undo {
            expected_revision: undone.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    assert_eq!(baseline.document.nodes(), before.document.nodes());
    assert_eq!(baseline.single_source, before.single_source);
    assert!(!baseline.can_undo);
    let redone = command(
        &service,
        ProjectRequest::Redo {
            expected_revision: baseline.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    assert_eq!(redone.document.nodes(), first.document.nodes());
}

#[test]
fn repeat_setter_preserves_gap_and_operator_wrap_is_distinct_and_durable() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("repeat.deadpan");
    let mut store = seed_holds(&path, &["a"]);
    seed_command(
        &mut store,
        Command::WrapRepeat {
            node: node("a"),
            id: node("repeat"),
            plays: 2,
            gap: Some(hold(3)),
            anchor_policy: Default::default(),
        },
        "seed-repeat",
    );
    drop(store);
    let service = ProjectService::new(Arc::new(|| {})).unwrap();
    let before = command(&service, ProjectRequest::Open(path.clone()))
        .workspace
        .unwrap();
    let update = edited(
        &service,
        &before,
        ProjectEdit::Repeat {
            node: node("repeat"),
            plays: 3,
        },
    );
    assert_eq!(
        update.committed.unwrap().selected_node,
        Some(node("repeat"))
    );
    let set = update.workspace.unwrap();
    assert_eq!(set.document.nodes().len(), before.document.nodes().len());
    assert_eq!(set.document.duration().unwrap().frames(), 36);
    assert!(
        matches!(&set.document.nodes()[&node("repeat")].kind, NodeKind::Repeat { gap: Some(gap), .. } if gap == &hold(3))
    );
    let wrapped = edited(
        &service,
        &set,
        ProjectEdit::WrapRepeat {
            node: node("repeat"),
            plays: 2,
        },
    );
    let wrapper = wrapped.committed.unwrap().selected_node.unwrap();
    let wrapped = wrapped.workspace.unwrap();
    assert_ne!(wrapper, node("repeat"));
    assert!(
        matches!(&wrapped.document.nodes()[&wrapper].kind, NodeKind::Repeat { child, gap: None, .. } if child == &node("repeat"))
    );
    assert_eq!(wrapped.document.duration().unwrap().frames(), 72);
    let hidden = command(
        &service,
        edit_request(
            &wrapped,
            ProjectEdit::Delete {
                node: node("repeat"),
            },
        ),
    );
    assert!(hidden.error.unwrap().contains("root beat"));
    assert!(hidden.committed.is_none());
    assert_eq!(*hidden.workspace.unwrap().document, *wrapped.document);
    let undone = command(
        &service,
        ProjectRequest::Undo {
            expected_revision: wrapped.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    assert_eq!(undone.document.nodes(), set.document.nodes());
    assert_ne!(undone.document.revision_id(), set.document.revision_id());
    let redone = command(
        &service,
        ProjectRequest::Redo {
            expected_revision: undone.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    assert_eq!(redone.document.nodes(), wrapped.document.nodes());
    assert_ne!(
        redone.document.revision_id(),
        wrapped.document.revision_id()
    );
    command(&service, ProjectRequest::Close);
    let reopened = command(&service, ProjectRequest::Open(path))
        .workspace
        .unwrap();
    assert_eq!(*reopened.document, *redone.document);
}

#[test]
fn delete_selects_successor_then_predecessor_and_explicitly_clears_empty_selection() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("delete.deadpan");
    drop(seed_holds(&path, &["a", "b", "c"]));
    let service = ProjectService::new(Arc::new(|| {})).unwrap();
    let mut workspace = command(&service, ProjectRequest::Open(path))
        .workspace
        .unwrap();
    for (target, selected, frames) in [("b", Some("c"), 20), ("c", Some("a"), 10), ("a", None, 0)] {
        let update = edited(
            &service,
            &workspace,
            ProjectEdit::Delete { node: node(target) },
        );
        assert_eq!(update.committed.unwrap().selected_node, selected.map(node));
        workspace = update.workspace.unwrap();
        assert_eq!(workspace.document.duration().unwrap().frames(), frames);
    }
    assert_eq!(workspace.document.nodes().len(), 1);
}

#[test]
fn edit_rejects_stale_session_revision_and_invalid_targets_without_authored_changes() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("stale.deadpan");
    drop(seed_holds(&path, &["a"]));
    let service = ProjectService::new(Arc::new(|| {})).unwrap();
    let before = command(&service, ProjectRequest::Open(path.clone()))
        .workspace
        .unwrap();
    let updated = edited(
        &service,
        &before,
        ProjectEdit::HoldDuration {
            node: node("a"),
            duration: FrameDuration::new(24).unwrap(),
        },
    )
    .workspace
    .unwrap();
    assert_eq!(updated.document.duration().unwrap().frames(), 24);
    let stale = command(
        &service,
        edit_request(&before, ProjectEdit::Delete { node: node("a") }),
    );
    assert!(stale.error.unwrap().contains("changed before"));
    assert!(stale.committed.is_none());
    assert_eq!(*stale.workspace.unwrap().document, *updated.document);
    command(&service, ProjectRequest::Close);
    let reopened = command(&service, ProjectRequest::Open(path))
        .workspace
        .unwrap();
    assert_eq!(
        updated.document.revision_id(),
        reopened.document.revision_id()
    );
    assert_ne!(updated.session, reopened.session);
    let stale = command(
        &service,
        edit_request(&updated, ProjectEdit::Delete { node: node("a") }),
    );
    assert!(stale.error.unwrap().contains("session changed"));
    assert_eq!(*stale.workspace.unwrap().document, *reopened.document);
    for edit in [
        ProjectEdit::Repeat {
            node: node("a"),
            plays: 0,
        },
        ProjectEdit::Delete {
            node: node("missing"),
        },
        ProjectEdit::Delete { node: node("root") },
    ] {
        let failed = command(&service, edit_request(&reopened, edit));
        assert!(failed.error.is_some());
        assert!(failed.committed.is_none());
        assert_eq!(*failed.workspace.unwrap().document, *reopened.document);
    }
    let wrapped = edited(
        &service,
        &reopened,
        ProjectEdit::Repeat {
            node: node("a"),
            plays: 1,
        },
    );
    let wrapper = wrapped.committed.unwrap().selected_node.unwrap();
    let wrapped = wrapped.workspace.unwrap();
    assert_eq!(wrapped.document.duration().unwrap().frames(), 24);
    let wrong_kind = command(
        &service,
        edit_request(
            &wrapped,
            ProjectEdit::HoldDuration {
                node: wrapper,
                duration: FrameDuration::new(20).unwrap(),
            },
        ),
    );
    assert!(wrong_kind.error.is_some());
    assert!(wrong_kind.committed.is_none());
    assert_eq!(*wrong_kind.workspace.unwrap().document, *wrapped.document);
}

#[test]
fn native_framing_preserves_time_and_cursor_with_one_durable_undoable_commit() {
    use deadpan_core::{ExactRatio, Framing, FramingCurve, FramingPose};
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("framing.deadpan");
    drop(seed_holds(&path, &["a", "b"]));
    let service = ProjectService::new(Arc::new(|| {})).unwrap();
    let before = command(&service, ProjectRequest::Open(path.clone()))
        .workspace
        .unwrap();
    let end = FramingPose {
        scale: ExactRatio::new(27, 20).unwrap(),
        ..Default::default()
    };
    let framing = Framing::creep(Default::default(), end, FramingCurve::Smoothstep).unwrap();
    let outcome = edited(
        &service,
        &before,
        ProjectEdit::SetFraming {
            node: node("a"),
            framing: Some(framing.clone()),
        },
    );
    let committed = outcome.committed.unwrap();
    assert!(committed.preserve_cursor);
    assert_eq!(committed.selected_node, Some(node("a")));
    let current = outcome.workspace.unwrap();
    assert_eq!(
        current.document.duration().unwrap(),
        before.document.duration().unwrap()
    );
    assert_eq!(
        current.document.nodes()[&node("a")].kind,
        before.document.nodes()[&node("a")].kind
    );
    assert_eq!(
        current.document.nodes()[&node("b")],
        before.document.nodes()[&node("b")]
    );
    assert_eq!(
        current.document.nodes()[&node("a")].framing,
        Some(framing.clone())
    );
    let stale = command(
        &service,
        edit_request(
            &before,
            ProjectEdit::SetFraming {
                node: node("b"),
                framing: Some(framing.clone()),
            },
        ),
    );
    assert!(stale.error.unwrap().contains("changed"));
    assert!(stale.committed.is_none());
    let undone = command(
        &service,
        ProjectRequest::Undo {
            expected_revision: current.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    assert_eq!(undone.document.nodes(), before.document.nodes());
    assert_ne!(undone.document.revision_id(), before.document.revision_id());
    let redone = command(
        &service,
        ProjectRequest::Redo {
            expected_revision: undone.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    assert_eq!(redone.document.nodes(), current.document.nodes());
    assert_ne!(
        redone.document.revision_id(),
        current.document.revision_id()
    );
    command(&service, ProjectRequest::Close);
    let reopened = command(&service, ProjectRequest::Open(path))
        .workspace
        .unwrap();
    assert_eq!(reopened.document.nodes(), current.document.nodes());
    let stale_session = command(
        &service,
        edit_request(
            &redone,
            ProjectEdit::SetFraming {
                node: node("a"),
                framing: None,
            },
        ),
    );
    assert!(stale_session.error.unwrap().contains("session changed"));
    assert_eq!(
        *stale_session.workspace.unwrap().document,
        *reopened.document
    );
    let invalid = command(
        &service,
        edit_request(
            &reopened,
            ProjectEdit::SetFraming {
                node: node("root"),
                framing: Some(framing),
            },
        ),
    );
    assert!(invalid.error.unwrap().contains("root beat"));
    assert_eq!(*invalid.workspace.unwrap().document, *reopened.document);
}

#[test]
fn structural_edit_during_import_preparation_survives_coalesced_progress() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("edit-during-import.deadpan");
    drop(seed_holds(&path, &["a"]));
    let harness = Harness::new();
    let before = command(&harness.service, ProjectRequest::Open(path))
        .workspace
        .unwrap();
    import(&harness.service, "cfr-bframes.mp4");
    let retained = harness.job();
    harness
        .service
        .submit(edit_request(
            &before,
            ProjectEdit::Repeat {
                node: node("a"),
                plays: 3,
            },
        ))
        .unwrap();
    let deadline = Instant::now() + TIMEOUT;
    while harness.service.is_busy() {
        assert!(
            Instant::now() < deadline,
            "structural edit blocked behind import"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    // The commit's first update is deliberately unread until both import phases
    // have replaced it. Import registration creates a different later revision.
    harness.finish(retained);
    harness.finish(harness.job());
    let completed = wait(&harness.service, |update| {
        update
            .import
            .as_ref()
            .is_some_and(|status| status.stage == ImportStage::Complete)
    });
    assert!(completed.error.is_none(), "{:?}", completed.error);
    let marker = completed.committed.unwrap();
    let workspace = completed.workspace.unwrap();
    assert_ne!(&marker.revision, workspace.document.revision_id());
    let wrapper = marker.selected_node.unwrap();
    assert_eq!(
        workspace
            .document
            .children(workspace.document.root())
            .collect::<Vec<_>>(),
        vec![&wrapper]
    );
    assert_eq!(workspace.document.duration().unwrap().frames(), 30);
    assert_eq!(workspace.sources.len(), 1);
    let undone = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: workspace.document.revision_id().clone(),
        },
    );
    assert!(undone.committed.is_none());
    assert_eq!(
        undone
            .workspace
            .unwrap()
            .document
            .duration()
            .unwrap()
            .frames(),
        30
    );
}

#[test]
fn shutdown_finishes_an_admitted_command_before_releasing_the_store() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("admitted.deadpan");
    let shared = Arc::new(Shared {
        busy: AtomicBool::new(false),
        stopping: AtomicBool::new(false),
        update: Mutex::new(None),
        wake: Arc::new(|| {}),
    });
    let (requests, receive) = mpsc::sync_channel(1);
    let service = ProjectService {
        requests,
        shared: shared.clone(),
    };
    service
        .submit(ProjectRequest::Create(path.clone()))
        .unwrap();
    service.shutdown();
    assert!(service.submit(ProjectRequest::Close).is_err());
    let (jobs, _receive_jobs) = mpsc::sync_channel(1);
    let (_replies, results) = mpsc::sync_channel(1);
    service::run(
        shared,
        receive,
        jobs,
        results,
        std::thread::spawn(|| {}),
        None,
    );
    assert!(!service.is_busy());
    let update = service
        .take_update()
        .expect("accepted command has a result");
    assert!(update.error.is_none(), "{:?}", update.error);
    let writer = ProjectStore::open(&path, AccessMode::ReadWrite).unwrap();
    assert_eq!(
        writer.snapshot().unwrap(),
        *update.workspace.unwrap().document
    );
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/fixtures")
        .join(name)
        .canonicalize()
        .unwrap()
}

fn previous_native_project(path: &Path) -> rusqlite::Connection {
    std::fs::create_dir(path).unwrap();
    std::fs::create_dir(path.join("Snapshots")).unwrap();
    let connection = rusqlite::Connection::open(path.join("project.sqlite")).unwrap();
    connection
        .pragma_update(None, "foreign_keys", false)
        .unwrap();
    connection
        .execute_batch(include_str!(
            "../../../deadpan-store/tests/fixtures/v16-history.sql"
        ))
        .unwrap();
    connection
}

fn schema_version(connection: &rusqlite::Connection) -> u32 {
    connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap()
}

#[test]
fn native_open_migrates_authentic_schema16_with_a_backup_and_keeps_it_generic() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("previous-native.deadpan");
    let database = previous_native_project(&path);
    let original_json: String = database.query_row("SELECT document FROM revisions WHERE id=(SELECT head_revision FROM state WHERE singleton=1)", [], |row| row.get(0)).unwrap();
    let original = deadpan_core::legacy_v11::Document::from_json(&original_json)
        .unwrap()
        .upgrade()
        .unwrap();
    let history_count: i64 = database
        .query_row("SELECT count(*) FROM history", [], |row| row.get(0))
        .unwrap();
    assert_eq!(schema_version(&database), 16);
    let service = ProjectService::new(Arc::new(|| {})).unwrap();
    let opened = command(&service, ProjectRequest::Open(path.clone()));
    assert!(opened.error.is_none(), "{:?}", opened.error);
    assert!(opened.message.unwrap().contains("original database backup"));
    let workspace = opened.workspace.unwrap();
    assert_eq!(*workspace.document, original);
    assert!(workspace.single_source.is_none());
    assert!(workspace.original_duration.is_none());
    assert_eq!(
        schema_version(&database),
        deadpan_store::DATABASE_SCHEMA_VERSION
    );
    let backups = std::fs::read_dir(path.join("Snapshots"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(backups.len(), 1);
    let backup = rusqlite::Connection::open(&backups[0]).unwrap();
    assert_eq!(schema_version(&backup), 16);
    let saved_json: String = backup.query_row("SELECT document FROM revisions WHERE id=(SELECT head_revision FROM state WHERE singleton=1)", [], |row| row.get(0)).unwrap();
    assert_eq!(saved_json, original_json);
    assert_eq!(
        database
            .query_row("SELECT count(*) FROM history", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        history_count
    );
    command(&service, ProjectRequest::Close);
    let reopened = command(&service, ProjectRequest::Open(path.clone()));
    assert_eq!(reopened.message.as_deref(), Some("Project opened"));
    assert_eq!(*reopened.workspace.unwrap().document, original);
    assert_eq!(
        std::fs::read_dir(path.join("Snapshots")).unwrap().count(),
        1
    );
}

#[test]
fn failed_native_migration_retains_the_current_project_and_its_active_preparation() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("invalid-previous.deadpan");
    let database = previous_native_project(&path);
    database.execute("UPDATE history SET request=json_set(request,'$.command.type','unrecognized_command') WHERE id=(SELECT min(id) FROM history)", []).unwrap();
    let invalid_request: String = database
        .query_row(
            "SELECT request FROM history ORDER BY id LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let harness = Harness::new();
    let current = create(&harness.service, &scratch.path().join("current.deadpan"));
    import(&harness.service, "cfr-bframes.mp4");
    let active = harness.job();
    let failed = command(&harness.service, ProjectRequest::Open(path.clone()));
    assert!(
        failed
            .error
            .unwrap()
            .contains("Migration failed; retained backup")
    );
    let retained = failed.workspace.unwrap();
    assert_eq!(retained.session, current.session);
    assert_eq!(*retained.document, *current.document);
    assert!(!active.cancelled.load(Ordering::Acquire));
    assert_eq!(schema_version(&database), 16);
    assert_eq!(
        database
            .query_row(
                "SELECT request FROM history ORDER BY id LIMIT 1",
                [],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
        invalid_request
    );
    assert_eq!(
        std::fs::read_dir(path.join("Snapshots")).unwrap().count(),
        1
    );
    harness.finish(active);
    harness.finish(harness.job());
    let ready = complete(&harness.service);
    assert_eq!(ready.session, current.session);
    assert_eq!(ready.sources.len(), 1);
}

fn wait(service: &ProjectService, predicate: impl Fn(&ProjectUpdate) -> bool) -> ProjectUpdate {
    let deadline = Instant::now() + TIMEOUT;
    let mut last = None;
    loop {
        if let Some(update) = service.take_update() {
            if predicate(&update) {
                return update;
            }
            last = Some((update.error, update.import, update.message));
        }
        assert!(
            Instant::now() < deadline,
            "project update timed out: {last:?}"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn command(service: &ProjectService, request: ProjectRequest) -> ProjectUpdate {
    assert!(!service.is_busy());
    let _ = service.take_update();
    service.submit(request).unwrap();
    wait(service, |_| !service.is_busy())
}

fn create(service: &ProjectService, path: &Path) -> Arc<Workspace> {
    let update = command(service, ProjectRequest::Create(path.into()));
    assert!(update.error.is_none(), "{:?}", update.error);
    update.workspace.unwrap()
}

fn import(service: &ProjectService, name: &str) {
    service
        .submit(ProjectRequest::Import {
            path: fixture(name),
            media: ImportMedia::Video,
            ownership: OriginalOwnership::Managed,
        })
        .unwrap();
    let deadline = Instant::now() + TIMEOUT;
    while service.is_busy() {
        assert!(Instant::now() < deadline, "import command timed out");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn complete(service: &ProjectService) -> Arc<Workspace> {
    let update = wait(service, |update| {
        update.import.as_ref().is_some_and(|status| {
            matches!(status.stage, ImportStage::Complete | ImportStage::Failed)
        })
    });
    assert_eq!(
        update.import.as_ref().unwrap().stage,
        ImportStage::Complete,
        "{:?}",
        update.import
    );
    update.workspace.unwrap()
}

fn insert(service: &ProjectService, workspace: &Workspace, asset: &AssetId) -> ProjectUpdate {
    command(
        service,
        ProjectRequest::Insert {
            expected_revision: workspace.document.revision_id().clone(),
            asset: asset.clone(),
            parent: workspace.document.root().clone(),
            index: 0,
        },
    )
}

/// Real service actor with a manually scheduled worker. Tests hold actual prepared
/// results at phase boundaries instead of relying on filesystem/decoder timing.
struct Harness {
    service: ProjectService,
    jobs: Receiver<worker::Job>,
    replies: SyncSender<worker::Reply>,
}

impl Harness {
    fn new() -> Self {
        Self::with_library(None)
    }

    fn with_library(library: Option<ProjectLibrary>) -> Self {
        let shared = Arc::new(Shared {
            busy: AtomicBool::new(false),
            stopping: AtomicBool::new(false),
            update: Mutex::new(None),
            wake: Arc::new(|| {}),
        });
        let (requests, receive) = mpsc::sync_channel(1);
        let (sender, jobs) = mpsc::sync_channel(1);
        let (replies, results) = mpsc::sync_channel(1);
        let state = shared.clone();
        let worker = std::thread::spawn(|| {});
        std::thread::spawn(move || service::run(state, receive, sender, results, worker, library));
        Self {
            service: ProjectService { requests, shared },
            jobs,
            replies,
        }
    }

    fn job(&self) -> worker::Job {
        self.jobs.recv_timeout(TIMEOUT).unwrap()
    }

    fn finish(&self, job: worker::Job) {
        let result = worker::prepare(&job);
        assert!(
            result.is_ok(),
            "worker preparation failed: {:?}",
            result.as_ref().err()
        );
        self.replies
            .send(worker::Reply { id: job.id, result })
            .unwrap();
    }

    fn imported(&self, name: &str) -> Arc<Workspace> {
        import(&self.service, name);
        self.finish(self.job());
        self.finish(self.job());
        complete(&self.service)
    }
}

#[test]
fn native_new_starts_with_the_full_original_and_history_stops_at_that_baseline() {
    let scratch = tempfile::tempdir().unwrap();
    let service = ProjectService::start(
        Arc::new(|| {}),
        Some(ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap()),
    )
    .unwrap();
    service
        .submit(ProjectRequest::CreateFromSource {
            path: fixture("cfr-bframes.mp4"),
        })
        .unwrap();
    let initialized = wait(&service, |update| {
        update.import.as_ref().is_some_and(|status| {
            matches!(status.stage, ImportStage::Complete | ImportStage::Failed)
        })
    });
    assert!(initialized.error.is_none(), "{:?}", initialized.error);
    assert_eq!(
        initialized.import.as_ref().unwrap().stage,
        ImportStage::Complete,
        "{:?}",
        initialized.import
    );
    let before = initialized.workspace.unwrap();
    let Some(SingleSourceState::Ready { asset, node, .. }) = &before.single_source else {
        panic!("original not initialized")
    };
    assert_eq!(
        before.path,
        scratch
            .path()
            .join("Documents/Deadpan/cfr-bframes.deadpan")
            .canonicalize()
            .unwrap()
    );
    assert_eq!(before.document.nodes().len(), 2);
    assert_eq!(before.document.duration().unwrap().frames(), 120);
    assert_eq!(before.sources.len(), 1);
    assert_eq!(&initialized.committed.unwrap().selected_node.unwrap(), node);
    assert!(!before.can_undo);
    let asset = asset.clone();
    let node = node.clone();
    let rejected = command(
        &service,
        ProjectRequest::Import {
            path: fixture("offset-bframes.mp4"),
            media: ImportMedia::Video,
            ownership: OriginalOwnership::Managed,
        },
    );
    assert!(rejected.error.unwrap().contains("one Original"));
    assert_eq!(*rejected.workspace.unwrap().document, *before.document);
    let repeated = edited(
        &service,
        &before,
        ProjectEdit::WrapRepeat { node, plays: 3 },
    )
    .workspace
    .unwrap();
    assert_eq!(repeated.document.duration().unwrap().frames(), 360);
    let undone = command(
        &service,
        ProjectRequest::Undo {
            expected_revision: repeated.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    assert_eq!(undone.document.nodes(), before.document.nodes());
    assert!(!undone.can_undo);
    let reused = insert(&service, &undone, &asset);
    assert!(reused.error.is_none(), "{:?}", reused.error);
    assert_eq!(
        reused
            .workspace
            .unwrap()
            .document
            .duration()
            .unwrap()
            .frames(),
        240
    );
    command(&service, ProjectRequest::Close);
    let reopened = command(&service, ProjectRequest::Open(before.path.clone()))
        .workspace
        .unwrap();
    assert_eq!(reopened.single_source, before.single_source);
    assert_eq!(reopened.document.duration().unwrap().frames(), 240);
}

#[test]
fn interrupted_initialization_is_recoverable_and_never_overlaps_after_project_switch() {
    let scratch = tempfile::tempdir().unwrap();
    let library = ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap();
    let harness = Harness::with_library(Some(library.clone()));
    let created = command(
        &harness.service,
        ProjectRequest::CreateFromSource {
            path: fixture("cfr-bframes.mp4"),
        },
    )
    .workspace
    .unwrap();
    assert!(matches!(
        created.single_source,
        Some(SingleSourceState::AwaitingSource { .. })
    ));
    let active = harness.job();
    let other = create(&harness.service, &scratch.path().join("legacy.deadpan"));
    assert!(active.cancelled.load(Ordering::Acquire));
    let rejected = command(
        &harness.service,
        ProjectRequest::CreateFromSource {
            path: fixture("offset-bframes.mp4"),
        },
    );
    assert!(rejected.error.unwrap().contains("current import"));
    assert_eq!(rejected.workspace.unwrap().session, other.session);
    assert_eq!(std::fs::read_dir(library.root()).unwrap().count(), 1);
    harness
        .replies
        .send(worker::Reply {
            id: active.id,
            result: Err("cancelled".into()),
        })
        .unwrap();
    // Wait until the reply has drained by reopening, then retrying initialization.
    let reopened = command(&harness.service, ProjectRequest::Open(created.path.clone()))
        .workspace
        .unwrap();
    assert!(matches!(
        reopened.single_source,
        Some(SingleSourceState::AwaitingSource { .. })
    ));
    let stale = command(
        &harness.service,
        ProjectRequest::InitializeSource {
            expected_session: created.session,
            expected_revision: created.document.revision_id().clone(),
            path: fixture("cfr-bframes.mp4"),
        },
    );
    assert!(stale.error.unwrap().contains("session changed"));
    let retry = command(
        &harness.service,
        ProjectRequest::InitializeSource {
            expected_session: reopened.session,
            expected_revision: reopened.document.revision_id().clone(),
            path: fixture("cfr-bframes.mp4"),
        },
    );
    assert!(retry.error.is_none(), "{:?}", retry.error);
    harness.finish(harness.job());
    harness.finish(harness.job());
    let ready = complete(&harness.service);
    assert!(matches!(
        ready.single_source,
        Some(SingleSourceState::Ready { .. })
    ));
    assert_eq!(ready.document.duration().unwrap().frames(), 120);
}

#[test]
fn sounds_are_audio_only_catalog_entries_and_bad_streams_leave_the_edit_intact() {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::with_library(Some(
        ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap(),
    ));
    command(
        &harness.service,
        ProjectRequest::CreateFromSource {
            path: fixture("cfr-bframes.mp4"),
        },
    );
    harness.finish(harness.job());
    harness.finish(harness.job());
    let ready = complete(&harness.service);
    let source = fixture("../audio-fixtures/pcm-stereo-48000.wav");
    let request = |stream| ProjectRequest::ImportSound {
        expected_session: ready.session,
        expected_revision: ready.document.revision_id().clone(),
        path: source.clone(),
        stream,
        ownership: OriginalOwnership::Managed,
    };
    assert!(command(&harness.service, request(None)).error.is_none());
    harness.finish(harness.job());
    harness.finish(harness.job());
    let sound = wait(&harness.service, |update| {
        update
            .import
            .as_ref()
            .is_some_and(|status| status.stage == ImportStage::Complete)
    });
    assert_eq!(sound.message.as_deref(), Some("Sound added to the catalog"));
    let sound = sound.workspace.unwrap();
    assert_eq!(
        sound.document.duration().unwrap(),
        ready.document.duration().unwrap()
    );
    assert_eq!(sound.document.nodes(), ready.document.nodes());
    assert_eq!(sound.sources.len(), 2);
    assert_eq!(
        sound
            .sources
            .values()
            .filter(|source| source.receipt.snapshot().video().is_none()
                && source.receipt.snapshot().audio().is_some())
            .count(),
        1
    );
    let stale = command(&harness.service, request(Some(1)));
    assert!(stale.error.unwrap().contains("Project changed"));
    let failed = command(
        &harness.service,
        ProjectRequest::ImportSound {
            expected_session: sound.session,
            expected_revision: sound.document.revision_id().clone(),
            path: source,
            stream: Some(1),
            ownership: OriginalOwnership::Managed,
        },
    );
    assert!(failed.error.is_none());
    harness.finish(harness.job());
    let job = harness.job();
    let result = worker::prepare(&job);
    assert!(result.is_err());
    harness
        .replies
        .send(worker::Reply { id: job.id, result })
        .unwrap();
    let failed = wait(&harness.service, |update| {
        update
            .import
            .as_ref()
            .is_some_and(|status| status.stage == ImportStage::Failed)
    });
    assert_eq!(*failed.workspace.unwrap().document, *sound.document);
    assert!(
        command(
            &harness.service,
            ProjectRequest::ImportSound {
                expected_session: sound.session,
                expected_revision: sound.document.revision_id().clone(),
                path: fixture("cfr-bframes.mp4"),
                stream: None,
                ownership: OriginalOwnership::Managed,
            }
        )
        .error
        .is_none()
    );
    harness.finish(harness.job());
    harness.finish(harness.job());
    let mp4_sound = complete(&harness.service);
    assert_eq!(mp4_sound.document.nodes(), sound.document.nodes());
    assert!(mp4_sound.sources.values().any(|source| {
        source.receipt.snapshot().video().is_none()
            && source
                .receipt
                .snapshot()
                .audio()
                .is_some_and(|audio| audio.stream().stream_index == 1)
    }));
}

#[test]
fn failed_original_qualification_reopens_as_an_incomplete_project_for_retry() {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::with_library(Some(
        ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap(),
    ));
    let created = command(
        &harness.service,
        ProjectRequest::CreateFromSource {
            path: fixture("../audio-fixtures/pcm-stereo-48000.wav"),
        },
    )
    .workspace
    .unwrap();
    harness.finish(harness.job());
    let job = harness.job();
    let result = worker::prepare(&job);
    assert!(
        result.is_err(),
        "audio alone cannot establish an Original video"
    );
    harness
        .replies
        .send(worker::Reply { id: job.id, result })
        .unwrap();
    let failed = wait(&harness.service, |update| {
        update
            .import
            .as_ref()
            .is_some_and(|status| status.stage == ImportStage::Failed)
    });
    let workspace = failed.workspace.unwrap();
    assert!(matches!(
        workspace.single_source,
        Some(SingleSourceState::AwaitingSource { .. })
    ));
    assert_eq!(*workspace.document, *created.document);
    command(&harness.service, ProjectRequest::Close);
    let reopened = command(
        &harness.service,
        ProjectRequest::Open(workspace.path.clone()),
    )
    .workspace
    .unwrap();
    assert!(matches!(
        reopened.single_source,
        Some(SingleSourceState::AwaitingSource { .. })
    ));
    assert!(
        command(
            &harness.service,
            ProjectRequest::InitializeSource {
                expected_session: reopened.session,
                expected_revision: reopened.document.revision_id().clone(),
                path: fixture("cfr-bframes.mp4")
            }
        )
        .error
        .is_none()
    );
    harness.finish(harness.job());
    harness.finish(harness.job());
    assert_eq!(
        complete(&harness.service)
            .document
            .duration()
            .unwrap()
            .frames(),
        120
    );
}

#[test]
fn real_import_registers_without_inserting_and_round_trips_entire_history() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("native.deadpan");
    let service = ProjectService::new(Arc::new(|| {})).unwrap();
    let initial = create(&service, &path);
    assert!(!initial.can_undo && !initial.can_redo);
    assert!(ProjectStore::open(&path, AccessMode::ReadWrite).is_err());
    let reader = ProjectStore::open(&path, AccessMode::ReadOnly).unwrap();
    assert_eq!(reader.snapshot().unwrap(), *initial.document);
    drop(reader);

    import(&service, "offset-bframes.mp4");
    let registered = complete(&service);
    assert_eq!(registered.document.nodes().len(), 1);
    assert_eq!(registered.document.duration().unwrap().frames(), 0);
    assert_eq!(registered.sources.len(), 1);
    let source = registered.sources.values().next().unwrap();
    assert!(source.receipt.snapshot().video().is_some());
    assert!(source.receipt.snapshot().audio().is_some());
    assert!(source.video_index.is_some());
    assert!(source.original.managed());
    let asset = source.asset.clone();
    let inserted = insert(&service, &registered, &asset);
    assert!(inserted.error.is_none(), "{:?}", inserted.error);
    let inserted = inserted.workspace.unwrap();
    assert!(inserted.document.duration().unwrap().frames() > 0);
    let duration = inserted.document.duration().unwrap();
    let undone = command(
        &service,
        ProjectRequest::Undo {
            expected_revision: inserted.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    assert_eq!(undone.document.nodes().len(), 1);
    assert_eq!(undone.sources.len(), 1);
    let unregistered = command(
        &service,
        ProjectRequest::Undo {
            expected_revision: undone.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    assert!(unregistered.sources.is_empty());
    assert!(!unregistered.can_undo && unregistered.can_redo);
    let redone = command(
        &service,
        ProjectRequest::Redo {
            expected_revision: unregistered.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    let redone = command(
        &service,
        ProjectRequest::Redo {
            expected_revision: redone.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    assert_eq!(redone.document.duration().unwrap(), duration);
    assert_ne!(
        redone.document.revision_id(),
        inserted.document.revision_id()
    );
    assert!(command(&service, ProjectRequest::Close).workspace.is_none());
    let reopened = command(&service, ProjectRequest::Open(path.clone()))
        .workspace
        .unwrap();
    assert_eq!(*reopened.document, *redone.document);
    assert_eq!(reopened.sources.len(), 1);
    assert!(reopened.can_undo && !reopened.can_redo);
    let same = command(&service, ProjectRequest::Open(path))
        .workspace
        .unwrap();
    assert_eq!(same.session, reopened.session);
    let preparing = insert(&service, &same, &asset);
    let completed = if preparing.committed.is_some() {
        preparing
    } else {
        wait(&service, |update| update.committed.is_some())
    };
    let CommittedEdit {
        revision,
        selected_node,
        ..
    } = completed.committed.unwrap();
    let node = selected_node.unwrap();
    let workspace = completed.workspace.unwrap();
    assert_eq!(&revision, workspace.document.revision_id());
    assert!(workspace.document.nodes().contains_key(&node));
    assert_eq!(workspace.document.nodes().len(), 3);
}

#[test]
fn project_commands_and_undo_continue_during_preparation_and_errors_survive_progress() {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::new();
    create(&harness.service, &scratch.path().join("active.deadpan"));
    let first = harness.imported("cfr-bframes.mp4");
    let asset = first.sources.keys().next().unwrap().clone();
    import(&harness.service, "offset-bframes.mp4");
    let retained = harness.job();
    let inserted = insert(&harness.service, &first, &asset);
    assert!(inserted.error.is_none(), "{:?}", inserted.error);
    assert_eq!(inserted.import.unwrap().stage, ImportStage::Retaining);
    let inserted = inserted.workspace.unwrap();
    let undone = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: inserted.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    assert_eq!(undone.document.nodes().len(), 1);
    let failed = command(
        &harness.service,
        ProjectRequest::Open(scratch.path().join("missing.deadpan")),
    );
    let failure = failed.error.unwrap();
    assert_eq!(failed.workspace.unwrap().session, first.session);
    harness.finish(retained);
    let decoding = wait(&harness.service, |update| {
        update
            .import
            .as_ref()
            .is_some_and(|status| status.stage == ImportStage::Decoding)
    });
    assert_eq!(decoding.error.as_deref(), Some(failure.as_str()));
    harness.finish(harness.job());
    let complete = wait(&harness.service, |update| {
        update
            .import
            .as_ref()
            .is_some_and(|status| status.stage == ImportStage::Complete)
    });
    assert_eq!(complete.error.as_deref(), Some(failure.as_str()));
    assert_eq!(complete.workspace.unwrap().sources.len(), 2);
}

#[test]
fn cancel_and_switch_reject_late_prepared_results_and_release_writer_lock() {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::new();
    let old_path = scratch.path().join("old.deadpan");
    let old = create(&harness.service, &old_path);
    import(&harness.service, "cfr-bframes.mp4");
    let job = harness.job();
    let prepared = worker::prepare(&job).unwrap();
    let cancelled = command(&harness.service, ProjectRequest::CancelImport);
    assert_eq!(cancelled.import.unwrap().stage, ImportStage::Cancelled);
    command(&harness.service, ProjectRequest::Close);
    let reopened = ProjectStore::open(&old_path, AccessMode::ReadWrite).unwrap();
    assert!(reopened.original_records(None, 100).unwrap().is_empty());
    assert!(
        old.originals
            .prepare_retention(
                &fixture("cfr-bframes.mp4"),
                OriginalOwnership::Managed,
                worker::original_limits(),
                &AtomicBool::new(false)
            )
            .is_err()
    );
    // Durable publication precedes inventory; cancellation does not erase it.
    let originals = std::fs::read_dir(old_path.join("Media/Originals")).unwrap();
    assert!(originals.count() > 0);
    drop(reopened);
    let current = create(&harness.service, &scratch.path().join("new.deadpan"));
    harness
        .replies
        .send(worker::Reply {
            id: job.id,
            result: Ok(prepared),
        })
        .unwrap();
    let late = wait(&harness.service, |update| {
        update
            .workspace
            .as_ref()
            .is_some_and(|workspace| workspace.session == current.session)
    });
    assert!(late.workspace.unwrap().sources.is_empty());
    assert!(late.import.is_none());
    assert!(late.committed.is_none());
}

#[test]
fn prepared_insertion_retains_captured_revision_and_never_retargets_after_undo() {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::new();
    create(&harness.service, &scratch.path().join("stale.deadpan"));
    let first = harness.imported("cfr-bframes.mp4");
    let asset = first.sources.keys().next().unwrap().clone();
    let second = harness.imported("offset-bframes.mp4");
    let preparing = insert(&harness.service, &second, &asset);
    assert!(preparing.error.is_none());
    assert_eq!(
        preparing.import.unwrap().stage,
        ImportStage::PreparingInsertion
    );
    let job = harness.job();
    let undone = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: second.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    assert_eq!(undone.sources.len(), 1);
    harness.finish(job);
    let failed = wait(&harness.service, |update| {
        update
            .import
            .as_ref()
            .is_some_and(|status| status.stage == ImportStage::Failed)
    });
    assert!(
        failed
            .import
            .unwrap()
            .error
            .unwrap()
            .contains("Revision conflict")
    );
    let current = failed.workspace.unwrap();
    assert!(failed.committed.is_none());
    assert_eq!(
        current.document.revision_id(),
        undone.document.revision_id()
    );
    assert_eq!(current.document.nodes().len(), 1);
}

#[test]
fn insertion_completion_survives_import_progress_and_replaced_mailbox_updates() {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::new();
    create(&harness.service, &scratch.path().join("completion.deadpan"));
    let registered = harness.imported("cfr-bframes.mp4");
    let asset = registered.sources.keys().next().unwrap().clone();
    import(&harness.service, "offset-bframes.mp4");
    let retained = harness.job();
    harness
        .service
        .submit(ProjectRequest::Insert {
            expected_revision: registered.document.revision_id().clone(),
            asset,
            parent: registered.document.root().clone(),
            index: 0,
        })
        .unwrap();
    let deadline = Instant::now() + TIMEOUT;
    while harness.service.is_busy() {
        assert!(Instant::now() < deadline, "cached insertion timed out");
        std::thread::sleep(Duration::from_millis(2));
    }
    // Deliberately leave the insertion update unread. A stale former job reply
    // and both current import phases replace the mailbox before the UI reads it.
    harness
        .replies
        .send(worker::Reply {
            id: retained.id - 1,
            result: Err("late previous import".into()),
        })
        .unwrap();
    harness.finish(retained);
    harness.finish(harness.job());
    let completed = wait(&harness.service, |update| {
        update
            .import
            .as_ref()
            .is_some_and(|status| status.stage == ImportStage::Complete)
    });
    assert!(completed.error.is_none(), "{:?}", completed.error);
    let CommittedEdit {
        revision: insertion_revision,
        selected_node,
        ..
    } = completed.committed.unwrap();
    let node = selected_node.unwrap();
    let workspace = completed.workspace.unwrap();
    assert_eq!(workspace.sources.len(), 2);
    assert_eq!(workspace.document.nodes().len(), 2);
    assert_ne!(&node, workspace.document.root());
    assert!(workspace.document.nodes().contains_key(&node));
    assert_ne!(&insertion_revision, registered.document.revision_id());
    assert_ne!(&insertion_revision, workspace.document.revision_id());
    let undone = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: workspace.document.revision_id().clone(),
        },
    );
    assert!(undone.committed.is_none());
}

#[test]
fn consecutive_cached_insertions_report_distinct_committed_nodes() {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::new();
    create(
        &harness.service,
        &scratch.path().join("consecutive.deadpan"),
    );
    let registered = harness.imported("cfr-bframes.mp4");
    let asset = registered.sources.keys().next().unwrap();
    let first = insert(&harness.service, &registered, asset);
    assert_eq!(first.import.unwrap().stage, ImportStage::Complete);
    let first_completion = first.committed.unwrap();
    let first_workspace = first.workspace.unwrap();
    assert_eq!(
        &first_completion.revision,
        first_workspace.document.revision_id()
    );
    let second = insert(&harness.service, &first_workspace, asset);
    assert_eq!(second.import.unwrap().stage, ImportStage::Complete);
    let second_completion = second.committed.unwrap();
    let second_workspace = second.workspace.unwrap();
    assert_eq!(
        &second_completion.revision,
        second_workspace.document.revision_id()
    );
    assert_ne!(first_completion.revision, second_completion.revision);
    assert_ne!(
        first_completion.selected_node,
        second_completion.selected_node
    );
    assert!(
        second_workspace
            .document
            .nodes()
            .contains_key(second_completion.selected_node.as_ref().unwrap())
    );
}

#[test]
fn shutdown_does_not_wait_for_import_and_rejects_new_commands() {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::new();
    let path = scratch.path().join("shutdown.deadpan");
    create(&harness.service, &path);
    import(&harness.service, "offset-bframes.mp4");
    let job = harness.job();
    harness.service.shutdown();
    assert!(harness.service.submit(ProjectRequest::Close).is_err());
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if ProjectStore::open(&path, AccessMode::ReadWrite).is_ok() {
            break;
        }
        assert!(Instant::now() < deadline, "shutdown retained writer lock");
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(job.cancelled.load(Ordering::Acquire));
}

#[test]
fn explicit_audio_import_is_measured_and_invalid_selected_stream_fails() {
    let scratch = tempfile::tempdir().unwrap();
    let service = ProjectService::new(Arc::new(|| {})).unwrap();
    let initial = create(&service, &scratch.path().join("audio.deadpan"));
    let audio = fixture("../audio-fixtures/pcm-stereo-48000.wav");
    service
        .submit(ProjectRequest::Import {
            path: audio.clone(),
            media: ImportMedia::Audio { stream: 0 },
            ownership: OriginalOwnership::Managed,
        })
        .unwrap();
    let registered = complete(&service);
    let source = registered.sources.values().next().unwrap();
    assert!(source.receipt.snapshot().video().is_none());
    assert!(source.video_index.is_none());
    assert_eq!(
        source
            .receipt
            .snapshot()
            .audio()
            .unwrap()
            .stream()
            .sample_rate,
        48_000
    );
    assert_eq!(
        registered.document.presentation_basis(),
        initial.document.presentation_basis()
    );
    assert_eq!(registered.document.nodes().len(), 1);
    let revision = registered.document.revision_id().clone();
    service
        .submit(ProjectRequest::Import {
            path: audio,
            media: ImportMedia::Audio { stream: 99 },
            ownership: OriginalOwnership::Managed,
        })
        .unwrap();
    let failed = wait(&service, |update| {
        update
            .import
            .as_ref()
            .is_some_and(|status| status.stage == ImportStage::Failed)
    });
    assert!(failed.import.unwrap().error.is_some());
    assert_eq!(failed.workspace.unwrap().document.revision_id(), &revision);
}

#[test]
fn unavailable_linked_original_invalidates_cached_insertion_without_losing_registration() {
    let scratch = tempfile::tempdir().unwrap();
    let source = scratch.path().join("linked.mp4");
    std::fs::copy(fixture("cfr-bframes.mp4"), &source).unwrap();
    let service = ProjectService::new(Arc::new(|| {})).unwrap();
    let path = scratch.path().join("linked.deadpan");
    create(&service, &path);
    service
        .submit(ProjectRequest::Import {
            path: source.clone(),
            media: ImportMedia::Video,
            ownership: OriginalOwnership::Linked { bookmark: None },
        })
        .unwrap();
    let registered = complete(&service);
    let asset = registered.sources.keys().next().unwrap();
    std::fs::remove_file(source).unwrap();
    let preparing = insert(&service, &registered, asset);
    assert!(preparing.error.is_none());
    let is_failed = |update: &ProjectUpdate| {
        update
            .import
            .as_ref()
            .is_some_and(|status| status.stage == ImportStage::Failed)
    };
    let failed = if is_failed(&preparing) {
        preparing
    } else {
        wait(&service, is_failed)
    };
    assert_eq!(
        failed.workspace.unwrap().document.revision_id(),
        registered.document.revision_id()
    );
    command(&service, ProjectRequest::Close);
    let reopened = command(&service, ProjectRequest::Open(path));
    assert!(reopened.error.is_none());
    assert_eq!(reopened.workspace.unwrap().sources.len(), 1);
}

#[test]
fn lost_import_worker_reports_failure_without_closing_project() {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::new();
    let initial = create(
        &harness.service,
        &scratch.path().join("lost-worker.deadpan"),
    );
    import(&harness.service, "cfr-bframes.mp4");
    let job = harness.job();
    let Harness {
        service,
        jobs,
        replies,
    } = harness;
    drop(replies);
    let failed = wait(&service, |update| {
        update
            .import
            .as_ref()
            .is_some_and(|status| status.stage == ImportStage::Failed)
    });
    assert!(
        failed
            .import
            .unwrap()
            .error
            .unwrap()
            .contains("worker stopped")
    );
    assert_eq!(
        failed.workspace.unwrap().document.revision_id(),
        initial.document.revision_id()
    );
    assert!(job.cancelled.load(Ordering::Acquire));
    assert!(command(&service, ProjectRequest::Close).workspace.is_none());
    drop(jobs);
}
