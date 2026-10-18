use super::*;
use deadpan_core::{
    Anchor, SemanticOriginalContext, SemanticVisualSelection, SourceFrameId, SourceMoment,
    SourceStream,
};

#[test]
fn native_original_macro_receipts_retain_both_clocks_and_mark_only_history() {
    let scratch = tempfile::tempdir().unwrap();
    let service = ProjectService::start(
        Arc::new(|| {}),
        Some(ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap()),
    )
    .unwrap();
    service
        .submit(ProjectRequest::CreateFromSource {
            ownership: OriginalOwnership::Managed,
            path: fixture("vfr.mp4"),
        })
        .unwrap();
    let initialized = wait(&service, |update| {
        update.import.as_ref().is_some_and(|status| {
            matches!(status.stage, ImportStage::Complete | ImportStage::Failed)
        })
    });
    assert!(initialized.error.is_none(), "{:?}", initialized.error);
    let opened = command(
        &service,
        ProjectRequest::Open(initialized.workspace.as_ref().unwrap().path.clone()),
    );
    let workspace = opened.workspace.as_ref().unwrap();
    let Some(SingleSourceState::Ready {
        asset,
        qualification,
        node,
        ..
    }) = &workspace.single_source
    else {
        panic!()
    };
    let original = SemanticOriginalContext {
        asset: asset.clone(),
        qualification: qualification.clone(),
        ordinal: SourceFrameId(3),
    };
    let index = workspace.sources[asset]
        .video_index
        .as_ref()
        .unwrap()
        .clone();
    let baseline = workspace.document.clone();
    let selected = node.clone();
    let visual = Some(SemanticVisualSelection::Time {
        anchor: ProjectFrame(10),
        head: ProjectFrame(20),
        extending: false,
    });
    let saved = send(
        &service,
        save(
            &opened,
            1,
            'r',
            program(vec![
                SemanticInstruction::SetMark { letter: 'K' },
                movement(5),
                SemanticInstruction::JumpMark { letter: 'K' },
            ]),
        ),
    );
    let mut operation = run(&saved, 2, 'r', 1, 25);
    let Operation::Run { context, .. } = &mut operation else {
        unreachable!()
    };
    context.original = Some(original.clone());
    context.selected_child = Some(selected.clone());
    context.visual_selection = visual.clone();
    let performed = send(&service, operation);
    let execution = receipt(&performed);
    assert_eq!(execution.original(), Some(&original));
    assert!(execution.committed().is_some());
    assert!(
        performed.committed.is_none(),
        "An Original mark receipt must never publish a generic Edit cursor continuation"
    );
    let Outcome::Executed {
        cursor,
        selected: result_selected,
        visual_selection,
        ..
    } = &execution.outcome
    else {
        panic!()
    };
    assert_eq!(*cursor, ProjectFrame(25));
    assert_eq!(result_selected.as_ref(), Some(&selected));
    assert_eq!(*visual_selection, visual);
    let after = performed.workspace.as_ref().unwrap();
    let metadata = performed.marks.metadata.as_ref().unwrap();
    assert!(metadata.matches(after));
    assert_eq!(&metadata.before, baseline.revision_id());
    assert_eq!(after.document.nodes(), baseline.nodes());
    assert_eq!(after.document.sounds(), baseline.sounds());
    let mark = &after.document.marks()[&deadpan_core::native_mark_id('K').unwrap()];
    assert_eq!(&mark.owner, baseline.root());
    assert!(
        matches!(&mark.boundary.coordinate, Anchor::Source { asset, moment: SourceMoment::Timestamp { stream: SourceStream::Video, timestamp } }
        if asset == &original.asset && timestamp.time_base == index.time_base() && timestamp.ticks == index.frames()[3].pts)
    );
    assert_eq!(
        performed.registers.as_ref().unwrap().version,
        saved.registers.as_ref().unwrap().version
    );

    let saved = send(
        &service,
        save(&performed, 3, 'n', program(vec![movement(2)])),
    );
    assert!(
        saved.committed.is_none(),
        "A later bank save must not resurrect the prior Original mark as an Edit continuation"
    );
    let mut operation = run(&saved, 4, 'n', 1, 25);
    let Operation::Run { context, .. } = &mut operation else {
        unreachable!()
    };
    context.original = Some(original.clone());
    let navigated = send(&service, operation);
    assert!(receipt(&navigated).committed().is_none());
    assert_eq!(
        receipt(&navigated).original().unwrap().ordinal,
        SourceFrameId(5)
    );
    assert_eq!(
        navigated.workspace.as_ref().unwrap().document,
        after.document
    );

    let undone = command(
        &service,
        ProjectRequest::Undo {
            expected_revision: after.document.revision_id().clone(),
        },
    );
    let restored = &undone.workspace.as_ref().unwrap().document;
    assert_eq!(restored.nodes(), baseline.nodes());
    assert_eq!(restored.marks(), baseline.marks());
    let redone = command(
        &service,
        ProjectRequest::Redo {
            expected_revision: restored.revision_id().clone(),
        },
    );
    assert_eq!(
        redone.workspace.as_ref().unwrap().document.marks(),
        after.document.marks()
    );
}
