use super::*;
use deadpan_core::{AudioTimingId, ColorPolicy, FrameRate, PresentationBasis};

fn assert_restored(actual: &ProjectDocument, expected: &ProjectDocument) {
    let mut expected = serde_json::to_value(expected).unwrap();
    expected["revision_id"] = serde_json::to_value(actual.revision_id()).unwrap();
    assert_eq!(serde_json::to_value(actual).unwrap(), expected);
}

fn stored_request(workspace: &Workspace) -> CommandRequest {
    let database = rusqlite::Connection::open(workspace.path.join("project.sqlite")).unwrap();
    let encoded: String = database
        .query_row(
            "SELECT request FROM history WHERE revision_id=?1",
            [workspace.document.revision_id().as_str()],
            |row| row.get(0),
        )
        .unwrap();
    serde_json::from_str(&encoded).unwrap()
}

#[test]
fn native_nested_delete_retains_suffix_timing_in_one_transaction_and_undo() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("nested-ripple.deadpan");
    let initial = ProjectDocument::new(
        ProjectId::new("native-ripple").unwrap(),
        RevisionId::new("baseline").unwrap(),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30_000, 1_001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )
    .unwrap();
    let mut wire = serde_json::to_value(initial).unwrap();
    wire["nodes"] = serde_json::to_value(BTreeMap::from([
        (
            node("root"),
            BeatNode::sequence("Root", vec![node("leading"), node("group"), node("tail")]),
        ),
        (node("leading"), BeatNode::hold("Leading", hold(2))),
        (
            node("group"),
            BeatNode::sequence("Group", vec![node("cut"), node("suffix")]),
        ),
        (node("cut"), BeatNode::hold("Cut", hold(1))),
        (node("suffix"), BeatNode::hold("Suffix", hold(4))),
        (node("tail"), BeatNode::hold("Tail", hold(3))),
    ]))
    .unwrap();
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    drop(ProjectStore::create(&path, &document).unwrap());
    let service = ProjectService::new(Arc::new(|| {})).unwrap();
    let before = command(&service, ProjectRequest::Open(path))
        .workspace
        .unwrap();
    let scope = SequenceScope::default()
        .descend(&before, &node("group"))
        .unwrap();
    let update = command(
        &service,
        edit_request_in(
            &before,
            scope.clone(),
            ProjectFrame(2),
            ProjectEdit::Delete { node: node("cut") },
        ),
    );
    assert!(update.error.is_none(), "{:?}", update.error);
    let receipt = update.committed.unwrap();
    assert_eq!(receipt.scope, scope);
    assert_eq!(receipt.selected_node, Some(node("suffix")));
    let after = update.workspace.unwrap();
    assert_eq!(after.document.duration().unwrap().frames(), 9);
    let timing = AudioTimingId {
        allocation: after.document.revision_id().clone(),
        ordinal: 0,
    };
    assert_eq!(
        stored_request(&after).command,
        Command::DeleteRipple {
            node: node("cut"),
            timing: timing.clone(),
        }
    );
    let bindings = after.document.audio_bindings();
    assert!(bindings.timings().contains_key(&timing));
    assert!(bindings.bindings()[&node("leading")].reanchors.is_empty());
    assert!(!bindings.bindings().contains_key(&node("cut")));
    // Both moved owners keep the lattice captured before deletion. Their
    // unsplit entries already are their anchors, so no inert reanchor step is
    // stored (docs/TIMING_STORAGE.md).
    for owner in ["suffix", "tail"] {
        let binding = &bindings.bindings()[&node(owner)];
        assert_eq!(binding.lattice.reference.timing, timing);
        assert!(binding.reanchors.is_empty());
    }
    let database = rusqlite::Connection::open(after.path.join("project.sqlite")).unwrap();
    assert_eq!(
        database
            .query_row("SELECT count(*) FROM history", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    let restored = command(
        &service,
        ProjectRequest::Undo {
            expected_revision: after.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    assert_restored(&restored.document, &before.document);
    assert!(!restored.can_undo);
    assert!(restored.can_redo);
}

#[test]
fn native_delete_empty_terminal_group_keeps_time_and_allows_full_deletion() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("empty-ripple.deadpan");
    let mut store = seed_holds(&path, &["beat"]);
    seed_command(
        &mut store,
        Command::Insert {
            parent: node("root"),
            index: 1,
            subtree: Subtree {
                root: node("empty"),
                nodes: BTreeMap::from([(node("empty"), BeatNode::sequence("Empty", vec![]))]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
        "empty",
    );
    drop(store);
    let service = ProjectService::new(Arc::new(|| {})).unwrap();
    let before = command(&service, ProjectRequest::Open(path))
        .workspace
        .unwrap();
    let update = command(
        &service,
        edit_request_in(
            &before,
            SequenceScope::default(),
            ProjectFrame(10),
            ProjectEdit::Delete {
                node: node("empty"),
            },
        ),
    );
    assert!(update.error.is_none(), "{:?}", update.error);
    assert_eq!(update.committed.unwrap().selected_node, Some(node("beat")));
    let after = update.workspace.unwrap();
    assert_eq!(after.document.duration().unwrap().frames(), 10);
    assert_eq!(
        after.document.audio_bindings(),
        before.document.audio_bindings()
    );
    let update = edited(&service, &after, ProjectEdit::Delete { node: node("beat") });
    assert_eq!(update.committed.unwrap().selected_node, None);
    let empty = update.workspace.unwrap();
    assert_eq!(empty.document.duration().unwrap().frames(), 0);
    assert_eq!(empty.document.nodes().len(), 1);
    assert!(empty.document.audio_bindings().is_empty());
}

#[test]
fn native_full_original_delete_reopens_and_undo_restores_the_protected_baseline() {
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
    let before = complete(&harness.service);
    let original = before
        .document
        .children(before.document.root())
        .next()
        .unwrap()
        .clone();
    let update = edited(
        &harness.service,
        &before,
        ProjectEdit::Delete { node: original },
    );
    assert_eq!(update.committed.unwrap().selected_node, None);
    let empty = update.workspace.unwrap();
    assert_eq!(empty.document.duration().unwrap().frames(), 0);
    assert_eq!(empty.single_source, before.single_source);
    assert_eq!(empty.document.assets(), before.document.assets());
    command(&harness.service, ProjectRequest::Close);
    let reopened = command(&harness.service, ProjectRequest::Open(before.path.clone()))
        .workspace
        .unwrap();
    assert_eq!(*reopened.document, *empty.document);
    assert_eq!(reopened.single_source, before.single_source);
    let restored = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: reopened.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    assert_restored(&restored.document, &before.document);
    assert_eq!(restored.single_source, before.single_source);
    assert!(!restored.can_undo);
    assert!(restored.can_redo);
    let denied = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: restored.document.revision_id().clone(),
        },
    );
    assert!(denied.error.is_some());
    assert_eq!(*denied.workspace.unwrap().document, *restored.document);
}
