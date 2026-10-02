//! Intermediate transaction copies remain usable through native reopen and paste.

use super::*;
use deadpan_core::{
    AudioTimingId, CapturedEditSlice, LeafEdit, MarkId, OccurrenceIdentities, RegisterName,
    ResolvedStep, ResolvedTransaction, SliceCaptureSelection, SlicePasteIdentities,
};

fn revision(name: &str) -> RevisionId {
    RevisionId::new(name).unwrap()
}

#[test]
fn intermediate_original_repeat_copy_reopens_after_undo_and_pastes_in_native_service() {
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
    let Some(SingleSourceState::Ready { node: original, .. }) = &before.single_source else {
        panic!("Original baseline missing");
    };
    command(&harness.service, ProjectRequest::Close);
    let mut store = ProjectStore::open(&before.path, AccessMode::ReadWrite).unwrap();
    let root = before.document.root().clone();
    let wrapper = node("compound-repeat");
    let repeat_revision = revision("compound-repeat-step");
    let repeat = Command::WrapRepeat {
        node: original.clone(),
        id: wrapper.clone(),
        plays: 3,
        gap: None,
        anchor_policy: Default::default(),
    };
    let repeated = deadpan_core::apply(
        &before.document,
        &CommandRequest {
            project_id: before.document.project_id().clone(),
            expected_revision: before.document.revision_id().clone(),
            new_revision: repeat_revision.clone(),
            command: repeat.clone(),
        },
    )
    .unwrap()
    .forward
    .apply(&before.document)
    .unwrap();
    let slice = Arc::new(
        CapturedEditSlice::capture_selection(
            &repeated,
            &root,
            &SliceCaptureSelection::Child {
                node: wrapper.clone(),
            },
            AudioTimingId {
                allocation: revision("compound-copy-clock"),
                ordinal: 0,
            },
        )
        .unwrap(),
    );
    let cut_revision = revision("compound-cut-step");
    let paste_revision = revision("compound-paste-step");
    let required = slice.identity_requirements().unwrap();
    let identities = SlicePasteIdentities {
        authored: OccurrenceIdentities {
            nodes: (0..required.nodes)
                .map(|i| node(&format!("compound-paste-node-{i}")))
                .collect(),
            marks: (0..required.marks)
                .map(|i| MarkId::new(format!("compound-paste-mark-{i}")).unwrap())
                .collect(),
        },
        aliases: (0..required.aliases)
            .map(|i| node(&format!("compound-paste-alias-{i}")))
            .collect(),
    };
    let name = RegisterName::new('a').unwrap();
    let transaction = ResolvedTransaction::new(
        store.register_version().unwrap(),
        BTreeMap::new(),
        vec![
            ResolvedStep::Edit {
                edit: LeafEdit::new(repeat_revision.clone(), repeat).unwrap(),
            },
            ResolvedStep::Cut {
                name,
                slice: slice.clone(),
                delete: LeafEdit::new(
                    cut_revision.clone(),
                    Command::DeleteRipple {
                        node: wrapper,
                        timing: AudioTimingId {
                            allocation: cut_revision,
                            ordinal: 0,
                        },
                    },
                )
                .unwrap(),
            },
            ResolvedStep::Paste {
                name,
                edit: LeafEdit::new(
                    paste_revision.clone(),
                    Command::SpliceSlice {
                        parent: root.clone(),
                        index: 0,
                        slice: (*slice).clone(),
                        identities,
                        timing: AudioTimingId {
                            allocation: paste_revision,
                            ordinal: 0,
                        },
                    },
                )
                .unwrap(),
            },
        ],
    )
    .unwrap();
    let initial_rows = counts(&before.path);
    let outcome = store
        .commit(&CommandRequest {
            project_id: before.document.project_id().clone(),
            expected_revision: before.document.revision_id().clone(),
            new_revision: revision("compound-saved"),
            command: Command::Compound { transaction },
        })
        .unwrap();
    assert_eq!(
        counts(&before.path),
        (initial_rows.0 + 1, initial_rows.1 + 1)
    );
    assert_eq!(store.snapshot().unwrap().duration().unwrap().frames(), 360);
    assert_eq!(outcome.register_bank.as_ref().unwrap().version, 1);
    assert!(store.snapshot_at(&repeat_revision).is_err());
    assert_eq!(
        store.capture_snapshot_at(&repeat_revision).unwrap(),
        repeated
    );
    store
        .undo(&outcome.revision_id, revision("compound-undone"))
        .unwrap();
    assert_eq!(store.snapshot().unwrap().nodes(), before.document.nodes());
    drop(store);

    let reopened = command(&harness.service, ProjectRequest::Open(before.path.clone()));
    assert!(reopened.error.is_none(), "{:?}", reopened.error);
    let workspace = reopened.workspace.unwrap();
    let bank = reopened.registers.unwrap();
    let restored = copied(&bank, 'a').clone();
    assert_eq!(restored.slice(), &slice);
    assert_eq!(restored.slice().revision_id(), &repeat_revision);
    assert_eq!(restored.id().persisted_version, Some(1));
    assert_eq!(
        restored.bounds(),
        FrameRange::new(ProjectFrame(0), ProjectFrame(360)).unwrap()
    );
    assert_eq!(restored.child_label(), Some("Repeat"));
    assert_eq!(restored.id().session, workspace.session);
    assert!(Arc::ptr_eq(copied(&bank, 'a'), copied(&bank, '"')));
    let restored_rows = counts(&before.path);
    let rejected = command(
        &harness.service,
        ProjectRequest::CaptureEditSlice(CaptureRequest {
            id: restored.id().clone(),
            register: Some('b'),
            scope: restored.scope().clone(),
            parent: root.clone(),
            selection: restored.slice().selection().clone(),
        }),
    );
    assert!(rejected.captured_slice.unwrap().result.is_err());
    assert_eq!(counts(&before.path), restored_rows);
    assert_eq!(rejected.registers.unwrap().version, bank.version);
    let rejected = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: repeat_revision,
        },
    );
    assert!(rejected.error.is_some());
    assert_eq!(counts(&before.path), restored_rows);
    let pasted = command(
        &harness.service,
        ProjectRequest::PasteEditedSlice(crate::project::slice::Paste {
            expected_session: workspace.session,
            expected_revision: workspace.document.revision_id().clone(),
            copied: restored,
            scope: SequenceScope::default(),
            parent: root,
            destination: crate::project::splice::Destination::Slot(1),
        }),
    );
    assert!(pasted.error.is_none(), "{:?}", pasted.error);
    assert_eq!(pasted.workspace.unwrap().plan.duration().frames(), 480);
    assert_eq!(
        counts(&before.path),
        (restored_rows.0 + 1, restored_rows.1 + 1)
    );
}
