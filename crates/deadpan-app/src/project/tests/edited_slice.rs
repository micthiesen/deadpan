//! Historical native copy, independent source previews and atomic placement.

use super::*;
use crate::project::slice::{CaptureRequest, Captured, CopyId, Paste};
use crate::project::splice::{Destination, Operation, PreparedMedia, Proposal, ProposalId, Source};
use deadpan_core::{CapturedEditSlice, FrameRange};

#[path = "edited_slice/move_range.rs"]
mod move_range;

fn range(start: i64, end: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap()
}

fn setup(path: &Path) -> (Harness, Arc<Workspace>) {
    drop(seed_holds(path, &["a", "b", "c"]));
    let harness = Harness::new();
    let workspace = command(&harness.service, ProjectRequest::Open(path.into()))
        .workspace
        .unwrap();
    (harness, workspace)
}

fn capture_request(workspace: &Workspace, request: u64, range: FrameRange) -> CaptureRequest {
    CaptureRequest {
        id: CopyId {
            session: workspace.session,
            project: workspace.document.project_id().clone(),
            source_revision: workspace.document.revision_id().clone(),
            request,
        },
        scope: SequenceScope::default(),
        parent: workspace.document.root().clone(),
        range,
    }
}

fn capture(service: &ProjectService, request: CaptureRequest) -> Arc<Captured> {
    let id = request.id.clone();
    let update = command(service, ProjectRequest::CaptureEditSlice(request));
    let reply = update.captured_slice.unwrap();
    assert_eq!(reply.id, id);
    reply.result.unwrap()
}

fn proposal(
    workspace: &Workspace,
    copied: Arc<Captured>,
    draft: u64,
    change: u64,
    destination: Destination,
) -> Proposal {
    Proposal {
        operation: Operation::Copy,
        id: ProposalId {
            session: workspace.session,
            project: workspace.document.project_id().clone(),
            base_revision: workspace.document.revision_id().clone(),
            draft,
            change,
        },
        source: Source::Edited {
            range: copied.slice().range(),
            copied,
        },
        scope: SequenceScope::default(),
        parent: workspace.document.root().clone(),
        destination,
    }
}

fn paste(workspace: &Workspace, copied: Arc<Captured>, destination: Destination) -> ProjectRequest {
    ProjectRequest::PasteEditedSlice(Paste {
        expected_session: workspace.session,
        expected_revision: workspace.document.revision_id().clone(),
        copied,
        scope: SequenceScope::default(),
        parent: workspace.document.root().clone(),
        destination,
    })
}

fn counts(path: &Path) -> (i64, i64) {
    let database = rusqlite::Connection::open(path.join("project.sqlite")).unwrap();
    database
        .query_row(
            "SELECT (SELECT count(*) FROM revisions), (SELECT count(*) FROM history)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap()
}

fn restored(actual: &ProjectDocument, expected: &ProjectDocument) {
    let mut expected = serde_json::to_value(expected).unwrap();
    expected["revision_id"] = serde_json::to_value(actual.revision_id()).unwrap();
    assert_eq!(serde_json::to_value(actual).unwrap(), expected);
}

#[test]
fn capture_is_history_neutral_and_reads_the_named_revision_after_edit_delete_and_undo() {
    let scratch = tempfile::tempdir().unwrap();
    let (harness, initial) = setup(&scratch.path().join("capture.deadpan"));
    let requested = capture_request(&initial, 1, range(3, 16));
    let changed = edited(
        &harness.service,
        &initial,
        ProjectEdit::HoldDuration {
            node: node("a"),
            duration: FrameDuration::new(20).unwrap(),
        },
    );
    let receipt = changed.committed.clone();
    let changed = changed.workspace.unwrap();
    let before = counts(&initial.path);
    let update = command(
        &harness.service,
        ProjectRequest::CaptureEditSlice(requested.clone()),
    );
    assert_eq!(update.committed, receipt);
    assert!(Arc::ptr_eq(update.workspace.as_ref().unwrap(), &changed));
    let copied = update.captured_slice.unwrap().result.unwrap();
    assert_eq!(copied.id(), &requested.id);
    assert_eq!(copied.bounds(), range(0, 30));
    assert_eq!(copied.slice().duration().frames(), 13);
    assert_eq!(copied.slice().revision_id(), initial.document.revision_id());
    assert_eq!(counts(&initial.path), before);
    copied.slice().validate_capture(&initial.document).unwrap();
    assert!(copied.slice().validate_capture(&changed.document).is_err());

    let removed = edited(
        &harness.service,
        &changed,
        ProjectEdit::Delete { node: node("a") },
    )
    .workspace
    .unwrap();
    let old_again = capture(&harness.service, capture_request(&initial, 2, range(3, 16)));
    assert_eq!(old_again.bounds(), copied.bounds());
    assert_ne!(
        old_again.slice(),
        copied.slice(),
        "fresh scratch timing identity"
    );
    let undone = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: removed.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    let placed = command(
        &harness.service,
        paste(&undone, copied.clone(), Destination::Slot(3)),
    );
    assert!(placed.error.is_none(), "{:?}", placed.error);
    assert_eq!(
        placed.workspace.unwrap().plan.duration().frames(),
        undone.plan.duration().frames() + 13
    );

    command(&harness.service, ProjectRequest::Close);
    let reopened = command(&harness.service, ProjectRequest::Open(initial.path.clone()))
        .workspace
        .unwrap();
    let rejected = command(
        &harness.service,
        paste(&reopened, copied, Destination::Slot(0)),
    );
    assert!(rejected.error.unwrap().contains("session"));
    assert!(rejected.captured_slice.is_none());
}

#[test]
fn all_edited_destinations_preview_exactly_one_commit_with_fresh_paste_identities() {
    let scratch = tempfile::tempdir().unwrap();
    let (harness, initial) = setup(&scratch.path().join("destinations.deadpan"));
    let copied = capture(&harness.service, capture_request(&initial, 1, range(3, 16)));
    let mut current = initial.clone();
    let mut inserted = std::collections::BTreeSet::new();
    let destinations = [
        Destination::Slot(1),
        Destination::Interior {
            target: node("b"),
            at: FrameDuration::new(4).unwrap(),
        },
        Destination::Replace { range: range(2, 6) },
        Destination::Replace {
            range: range(2, 15),
        },
        Destination::Replace {
            range: range(2, 27),
        },
        Destination::Replace {
            range: range(0, 30),
        },
    ];
    for (index, destination) in destinations.into_iter().enumerate() {
        let request = proposal(&current, copied.clone(), index as u64 + 1, 1, destination);
        let before_counts = counts(&current.path);
        let update = command(
            &harness.service,
            ProjectRequest::PrepareSplice(request.clone()),
        );
        let reply = update.splice.unwrap();
        let endpoints = reply.source_view.unwrap().result.unwrap();
        assert_eq!(
            endpoints
                .media()
                .admitted()
                .document()
                .duration()
                .unwrap()
                .frames(),
            13
        );
        let prepared = reply.result.unwrap();
        let PreparedMedia::Edited(media) = &prepared.media else {
            panic!("edited media")
        };
        prepared
            .snapshot
            .validate_edit_slice_view(media.admitted())
            .unwrap();
        assert_eq!(
            prepared
                .plan
                .node_duration(&prepared.node)
                .unwrap()
                .frames(),
            13
        );
        assert!(matches!(
            prepared.snapshot.document.nodes()[&prepared.node].kind,
            NodeKind::Sequence { .. }
        ));
        assert!(inserted.insert(prepared.node.clone()));
        assert_eq!(counts(&current.path), before_counts);
        let saved = command(
            &harness.service,
            ProjectRequest::CommitSplice(request.id.clone()),
        );
        let receipt = saved.splice_commit.unwrap().result.unwrap();
        assert_eq!(receipt.selected_node, Some(prepared.node.clone()));
        let after = saved.workspace.unwrap();
        assert_eq!(*after.document, *prepared.snapshot.document);
        assert_eq!(
            counts(&current.path),
            (before_counts.0 + 1, before_counts.1 + 1)
        );
        let duplicate = command(&harness.service, ProjectRequest::CommitSplice(request.id));
        assert_eq!(duplicate.splice_commit.unwrap().result.unwrap(), receipt);
        assert_eq!(
            counts(&current.path),
            (before_counts.0 + 1, before_counts.1 + 1)
        );
        current = command(
            &harness.service,
            ProjectRequest::Undo {
                expected_revision: after.document.revision_id().clone(),
            },
        )
        .workspace
        .unwrap();
        restored(&current.document, &initial.document);
    }
}

#[test]
fn refinement_stays_historical_and_source_view_survives_invalid_destination_and_reuses_media() {
    let scratch = tempfile::tempdir().unwrap();
    let (harness, initial) = setup(&scratch.path().join("refine.deadpan"));
    let copied = capture(&harness.service, capture_request(&initial, 1, range(3, 16)));
    let original = copied.slice().clone();
    let current = edited(
        &harness.service,
        &initial,
        ProjectEdit::Delete { node: node("a") },
    )
    .workspace
    .unwrap();
    let mut request = proposal(&current, copied.clone(), 1, 1, Destination::Slot(99));
    request.source = Source::Edited {
        copied: copied.clone(),
        range: range(1, 19),
    };
    let failed = command(
        &harness.service,
        ProjectRequest::PrepareSplice(request.clone()),
    );
    let reply = failed.splice.unwrap();
    assert!(reply.result.is_err());
    let view = reply.source_view.unwrap().result.unwrap();
    assert_eq!(view.id().range, range(1, 19));
    assert_eq!(
        view.media().admitted().capture_revision(),
        initial.document.revision_id()
    );
    assert_eq!(
        view.media()
            .admitted()
            .document()
            .duration()
            .unwrap()
            .frames(),
        18
    );
    assert_eq!(copied.slice(), &original);

    request.id.change += 1;
    request.destination = Destination::Slot(1);
    let ready = command(
        &harness.service,
        ProjectRequest::PrepareSplice(request.clone()),
    )
    .splice
    .unwrap();
    assert!(ready.result.is_ok());
    assert!(Arc::ptr_eq(
        &view,
        &ready.source_view.unwrap().result.unwrap()
    ));
    let revision = current.document.revision_id().clone();
    command(
        &harness.service,
        ProjectRequest::AbandonSplice(request.id.clone()),
    );
    assert_eq!(copied.slice(), &original);
    assert_eq!(
        ProjectStore::open(&current.path, AccessMode::ReadOnly)
            .unwrap()
            .snapshot()
            .unwrap()
            .revision_id(),
        &revision
    );

    request.id.change += 1;
    request.source = Source::Edited {
        copied,
        range: range(0, 31),
    };
    let failed = command(&harness.service, ProjectRequest::PrepareSplice(request))
        .splice
        .unwrap();
    assert!(failed.result.is_err());
    assert!(failed.source_view.unwrap().result.is_err());
}

#[test]
fn historical_qualified_media_remains_available_after_registration_undo() {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::new();
    // Undoing automatic first-source adoption also changes the presentation
    // basis. Keep this fixture focused on historical media with a fixed basis.
    let path = scratch.path().join("historical.deadpan");
    let document = ProjectDocument::new(
        ProjectId::new("historical-edited-copy").unwrap(),
        RevisionId::new("initial").unwrap(),
        deadpan_core::PresentationBasis {
            width: 320,
            height: 180,
            frame_rate: deadpan_core::FrameRate::new(30000, 1001).unwrap(),
            color_policy: deadpan_core::ColorPolicy::SdrRec709,
        },
        node("root"),
    )
    .unwrap();
    drop(ProjectStore::create(&path, &document).unwrap());
    let initial = command(&harness.service, ProjectRequest::Open(path))
        .workspace
        .unwrap();
    let registered = harness.imported("cfr-bframes.mp4");
    let asset = registered.sources.keys().next().unwrap().clone();
    let inserted = insert(&harness.service, &registered, &asset)
        .workspace
        .unwrap();
    let copied = capture(
        &harness.service,
        capture_request(&inserted, 1, range(3, 16)),
    );
    let removed = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: inserted.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    let current = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: removed.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    assert_eq!(current.document.assets(), initial.document.assets());
    assert!(current.sources.is_empty());
    let request = proposal(&current, copied, 1, 1, Destination::Slot(0));
    let reply = command(
        &harness.service,
        ProjectRequest::PrepareSplice(request.clone()),
    )
    .splice
    .unwrap();
    let source = reply.source_view.unwrap().result.unwrap();
    let ready = reply.result.unwrap();
    assert!(ready.snapshot.sources.contains_key(&asset));
    assert!(source.media().sources().contains_key(&asset));
    let PreparedMedia::Edited(media) = &ready.media else {
        panic!("edited media")
    };
    assert!(Arc::ptr_eq(
        &media.sources()[&asset].receipt,
        &ready.snapshot.sources[&asset].receipt
    ));
    assert_eq!(
        media.sources()[&asset].receipt.id(),
        registered.sources[&asset].receipt.id()
    );
    let committed = command(&harness.service, ProjectRequest::CommitSplice(request.id));
    assert!(committed.error.is_none(), "{:?}", committed.error);
    let after = committed.workspace.unwrap();
    assert_eq!(*after.document, *ready.snapshot.document);
    assert!(after.sources.contains_key(&asset));
}

#[test]
fn invalid_source_scope_forged_capture_and_stale_destinations_never_write_history() {
    let scratch = tempfile::tempdir().unwrap();
    let (harness, initial) = setup(&scratch.path().join("reject.deadpan"));
    let before_counts = counts(&initial.path);
    for (index, selected) in [range(3, 3), range(0, 31)].into_iter().enumerate() {
        let reply = command(
            &harness.service,
            ProjectRequest::CaptureEditSlice(capture_request(&initial, index as u64 + 1, selected)),
        );
        assert!(reply.captured_slice.unwrap().result.is_err());
    }
    let mut wrong = capture_request(&initial, 3, range(3, 16));
    wrong.parent = node("a");
    assert!(
        command(&harness.service, ProjectRequest::CaptureEditSlice(wrong))
            .captured_slice
            .unwrap()
            .result
            .is_err()
    );
    let mut foreign = capture_request(&initial, 4, range(3, 16));
    foreign.id.session += 1;
    assert!(
        command(&harness.service, ProjectRequest::CaptureEditSlice(foreign))
            .captured_slice
            .unwrap()
            .result
            .is_err()
    );
    assert_eq!(counts(&initial.path), before_counts);

    let copied = capture(&harness.service, capture_request(&initial, 5, range(3, 16)));
    let mut forged = copied.as_ref().clone();
    let mut wire = serde_json::to_value(forged.slice().as_ref()).unwrap();
    wire["nodes"]["a"]["label"] = serde_json::json!("forged historical ownership");
    forged.slice = Arc::new(serde_json::from_value::<CapturedEditSlice>(wire).unwrap());
    let request = proposal(&initial, Arc::new(forged), 1, 1, Destination::Slot(0));
    let rejected = command(&harness.service, ProjectRequest::PrepareSplice(request))
        .splice
        .unwrap();
    assert!(rejected.result.is_err());
    assert!(rejected.source_view.unwrap().result.is_err());
    assert_eq!(counts(&initial.path), before_counts);

    let stale = proposal(&initial, copied.clone(), 2, 1, Destination::Slot(0));
    let current = edited(
        &harness.service,
        &initial,
        ProjectEdit::Delete { node: node("c") },
    )
    .workspace
    .unwrap();
    let before_counts = counts(&initial.path);
    assert!(
        command(&harness.service, ProjectRequest::PrepareSplice(stale))
            .splice
            .unwrap()
            .result
            .is_err()
    );
    let rejected = command(
        &harness.service,
        paste(&initial, copied, Destination::Slot(0)),
    );
    assert!(rejected.error.is_some());
    assert_eq!(*rejected.workspace.unwrap().document, *current.document);
    assert_eq!(counts(&initial.path), before_counts);
}

fn saved_refresh_failure(fast: bool) {
    let scratch = tempfile::tempdir().unwrap();
    let (harness, before) = setup(&scratch.path().join("refresh.deadpan"));
    let copied = capture(&harness.service, capture_request(&before, 1, range(3, 16)));
    let before_counts = counts(&before.path);
    let request = proposal(
        &before,
        copied.clone(),
        1,
        1,
        Destination::Replace {
            range: range(5, 23),
        },
    );
    if !fast {
        let ready = command(
            &harness.service,
            ProjectRequest::PrepareSplice(request.clone()),
        );
        assert!(ready.splice.unwrap().result.is_ok());
    }
    harness
        .service
        .shared
        .splice_commit_refresh_failure
        .store(true, Ordering::Release);
    let saved = command(
        &harness.service,
        if fast {
            paste(&before, copied.clone(), request.destination.clone())
        } else {
            ProjectRequest::CommitSplice(request.id.clone())
        },
    );
    let committed = saved.committed.clone().unwrap();
    assert!(Arc::ptr_eq(saved.workspace.as_ref().unwrap(), &before));
    assert_eq!(
        counts(&before.path),
        (before_counts.0 + 1, before_counts.1 + 1)
    );
    assert!(saved.error.is_none());
    assert!(saved.message.as_ref().unwrap().contains("saved, but"));
    assert!(saved.message.as_ref().unwrap().contains("Reopen"));
    if !fast {
        assert_eq!(
            saved
                .splice_commit
                .as_ref()
                .unwrap()
                .result
                .as_ref()
                .unwrap(),
            &committed
        );
    }
    let queried = command(
        &harness.service,
        ProjectRequest::CaptureEditSlice(capture_request(&before, 2, range(0, 10))),
    );
    assert_eq!(queried.committed, saved.committed);
    assert_eq!(queried.error, saved.error);
    assert_eq!(queried.message, saved.message);
    if !fast {
        let repeated = command(&harness.service, ProjectRequest::CommitSplice(request.id));
        assert_eq!(repeated.splice_commit.unwrap().result.unwrap(), committed);
    } else {
        let repeated = command(
            &harness.service,
            paste(&before, copied, Destination::Slot(0)),
        );
        assert!(
            repeated.error.is_some(),
            "stale visible revision cannot repeat saved paste"
        );
    }
    assert_eq!(
        counts(&before.path),
        (before_counts.0 + 1, before_counts.1 + 1)
    );
    command(&harness.service, ProjectRequest::Close);
    let reopened = command(&harness.service, ProjectRequest::Open(before.path.clone()))
        .workspace
        .unwrap();
    assert_eq!(reopened.document.revision_id(), &committed.revision);
    let undone = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: reopened.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    restored(&undone.document, &before.document);
}

#[test]
fn edited_fast_paste_retains_its_saved_receipt_when_refresh_fails() {
    saved_refresh_failure(true);
}

#[test]
fn edited_proposal_retains_its_saved_receipt_and_duplicate_commit_when_refresh_fails() {
    saved_refresh_failure(false);
}

#[test]
fn nested_copy_excludes_parent_framing_and_pasted_windows_accept_interior_placement() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("group.deadpan");
    let mut store = seed_holds(&path, &["a", "b", "c"]);
    seed_command(
        &mut store,
        Command::Group {
            parent: node("root"),
            start: 0,
            end: 2,
            id: node("group"),
            label: "framed group".into(),
        },
        "group",
    );
    seed_command(
        &mut store,
        Command::SetFraming {
            node: node("group"),
            framing: Some(
                deadpan_core::Framing::static_pose(deadpan_core::FramingPose::default()).unwrap(),
            ),
        },
        "framing",
    );
    drop(store);
    let harness = Harness::new();
    let before = command(&harness.service, ProjectRequest::Open(path))
        .workspace
        .unwrap();
    let scope = SequenceScope::default()
        .descend(&before, &node("group"))
        .unwrap();
    let mut request = capture_request(&before, 1, range(3, 16));
    request.scope = scope.clone();
    request.parent = node("group");
    let copied = capture(&harness.service, request);
    assert_eq!(copied.bounds(), range(0, 20));
    let mut request = proposal(&before, copied.clone(), 1, 1, Destination::Slot(2));
    request.scope = scope.clone();
    request.parent = node("group");
    let prepared = command(
        &harness.service,
        ProjectRequest::PrepareSplice(request.clone()),
    )
    .splice
    .unwrap();
    let source = prepared.source_view.unwrap().result.unwrap();
    assert!(
        source
            .media()
            .admitted()
            .document()
            .nodes()
            .values()
            .all(|entry| entry.framing.is_none())
    );
    let exact = prepared.result.unwrap();
    assert_eq!(exact.range, range(20, 33));
    let pasted = command(&harness.service, ProjectRequest::CommitSplice(request.id))
        .workspace
        .unwrap();
    let scope = scope.descend(&pasted, &exact.node).unwrap();
    let first_window = pasted
        .document
        .children(&exact.node)
        .next()
        .unwrap()
        .clone();
    assert!(matches!(
        pasted.document.nodes()[&first_window].kind,
        NodeKind::Retime { .. }
    ));
    let mut request = proposal(
        &pasted,
        copied,
        2,
        1,
        Destination::Interior {
            target: first_window,
            at: FrameDuration::new(2).unwrap(),
        },
    );
    request.scope = scope;
    request.parent = exact.node.clone();
    let ready = command(
        &harness.service,
        ProjectRequest::PrepareSplice(request.clone()),
    )
    .splice
    .unwrap()
    .result
    .unwrap();
    assert_eq!(ready.range.start(), ProjectFrame(22));
    let committed = command(&harness.service, ProjectRequest::CommitSplice(request.id))
        .workspace
        .unwrap();
    assert_eq!(*committed.document, *ready.snapshot.document);
}
