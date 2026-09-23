use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, SyncSender};
use std::time::{Duration, Instant};

use deadpan_core::{
    BeatNode, Command, CommandRequest, HoldAudio, HoldRecipe, HoldVideo, NodeKind, ProjectId,
    Subtree,
};
use deadpan_store::{AccessMode, ProjectStore};

use super::*;

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
    service::run(shared, receive, jobs, results, std::thread::spawn(|| {}));
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
        std::thread::spawn(move || service::run(state, receive, sender, results, worker));
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
