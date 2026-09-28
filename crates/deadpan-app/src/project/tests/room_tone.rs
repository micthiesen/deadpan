//! Real qualified source ranges and captured Hold edits through the writer service.

use deadpan_core::{
    Anchor, AnchorLossPolicy, BoundaryAnchor, ExactRatio, InsertionBias, MarkId, SourceSpan,
    SourceTimeBase, SourceTimestamp,
};

use super::*;

fn initialize(service: &ProjectService) -> Arc<Workspace> {
    service
        .submit(ProjectRequest::CreateFromSource {
            path: fixture("offset-bframes.mp4"),
        })
        .unwrap();
    let update = wait(service, |update| {
        update.import.as_ref().is_some_and(|status| {
            matches!(status.stage, ImportStage::Complete | ImportStage::Failed)
        })
    });
    assert!(update.error.is_none(), "{:?}", update.error);
    assert_eq!(update.import.as_ref().unwrap().stage, ImportStage::Complete);
    update.workspace.unwrap()
}

fn selection(workspace: &Workspace) -> RoomToneSelection {
    let source = workspace
        .sources
        .values()
        .find(|source| source.video_index.is_some())
        .unwrap();
    RoomToneSelection::Original {
        asset: source.asset.clone(),
        qualification: source.receipt.id().clone(),
        ordinals: 1..4,
    }
}

fn preparation(workspace: &Workspace, ticket: u64, selection: RoomToneSelection) -> ProjectRequest {
    ProjectRequest::PrepareRoomTone {
        expected_session: workspace.session,
        expected_revision: workspace.document.revision_id().clone(),
        ticket,
        selection,
    }
}

fn prepare(service: &ProjectService, workspace: &Workspace) -> PreparedRoomTone {
    let update = command(service, preparation(workspace, 41, selection(workspace)));
    assert!(update.error.is_none(), "{:?}", update.error);
    assert!(update.room_tone_error.is_none());
    assert!(update.committed.is_none());
    assert_eq!(*update.workspace.unwrap().document, *workspace.document);
    update.room_tone.unwrap()
}

fn assert_rejected(service: &ProjectService, workspace: &Workspace, request: ProjectRequest) {
    let expected_context = match &request {
        ProjectRequest::PrepareRoomTone {
            expected_session,
            expected_revision,
            ticket,
            ..
        } => Some((*ticket, *expected_session, expected_revision.clone())),
        _ => None,
    };
    let update = command(service, request);
    assert!(update.error.is_some());
    assert_eq!(
        update.room_tone_error,
        expected_context.map(|(ticket, session, revision)| RoomToneFailure {
            ticket,
            session,
            revision,
            error: update.error.clone().unwrap(),
        })
    );
    assert!(update.committed.is_none());
    assert!(update.room_tone.is_none());
    assert_eq!(*update.workspace.unwrap().document, *workspace.document);
}

#[test]
fn native_room_tone_failures_keep_request_identity_and_generic_errors_stay_separate() {
    let scratch = tempfile::tempdir().unwrap();
    let service = ProjectService::start(
        Arc::new(|| {}),
        Some(ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap()),
    )
    .unwrap();
    let absent_revision = RevisionId::new("absent-project-revision").unwrap();
    let rejected = command(
        &service,
        ProjectRequest::PrepareRoomTone {
            expected_session: 91,
            expected_revision: absent_revision.clone(),
            ticket: 73,
            selection: RoomToneSelection::Original {
                asset: AssetId::new("absent-source").unwrap(),
                qualification: SourceQualificationId::new("c".repeat(64)).unwrap(),
                ordinals: 0..1,
            },
        },
    );
    assert!(rejected.workspace.is_none());
    assert!(rejected.room_tone.is_none());
    assert_eq!(
        rejected.room_tone_error,
        Some(RoomToneFailure {
            ticket: 73,
            session: 91,
            revision: absent_revision,
            error: rejected.error.unwrap(),
        })
    );

    let workspace = initialize(&service);
    let failed_range = command(
        &service,
        ProjectRequest::PrepareRoomTone {
            expected_session: workspace.session + 1,
            expected_revision: workspace.document.revision_id().clone(),
            ticket: 74,
            selection: selection(&workspace),
        },
    );
    assert_eq!(failed_range.room_tone_error.as_ref().unwrap().ticket, 74);
    // Retain this generic error update while another range request completes.
    // It must not look like that request's failure, nor retain failure 74.
    let older_error = command(
        &service,
        ProjectRequest::Undo {
            expected_revision: RevisionId::new("stale-undo-revision").unwrap(),
        },
    );
    assert!(older_error.error.is_some());
    assert!(older_error.room_tone.is_none());
    assert!(older_error.room_tone_error.is_none());
    assert_eq!(
        *older_error.workspace.as_ref().unwrap().document,
        *workspace.document
    );
    let current = command(&service, preparation(&workspace, 75, selection(&workspace)));
    assert!(current.error.is_none(), "{:?}", current.error);
    assert!(current.room_tone_error.is_none());
    let prepared = current.room_tone.unwrap();
    assert_eq!(prepared.ticket, 75);
    assert_eq!(prepared.session, workspace.session);
    assert_eq!(&prepared.revision, workspace.document.revision_id());
    assert_eq!(*current.workspace.unwrap().document, *workspace.document);
    let closed = command(&service, ProjectRequest::Close);
    assert!(closed.room_tone.is_none());
    assert!(closed.room_tone_error.is_none());
}

#[test]
fn native_room_tone_preparation_is_sample_exact_and_creates_no_history_or_target() {
    let scratch = tempfile::tempdir().unwrap();
    let service = ProjectService::start(
        Arc::new(|| {}),
        Some(ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap()),
    )
    .unwrap();
    let workspace = initialize(&service);
    let prepared = prepare(&service, &workspace);
    assert_eq!(prepared.ticket, 41);
    assert_eq!(prepared.session, workspace.session);
    assert_eq!(&prepared.revision, workspace.document.revision_id());
    // Retained fixture PTS are [61061,64064) / 30000 seconds. At 48 kHz
    // inward snapping gives [ceil(97697.6), floor(102502.4)) source samples.
    assert_eq!(prepared.source.span.start().ticks, 97_698);
    assert_eq!(prepared.source.span.end().ticks, 102_502);
    assert_eq!(
        prepared.source.span.start().time_base,
        SourceTimeBase::new(1, 48_000).unwrap()
    );
    assert_eq!(prepared.audition.span(), prepared.source.span);
    assert_eq!(prepared.audition.duration_samples(), AudioSample(4804));
    let source = &workspace.sources[&prepared.source.asset];
    let exact = command(
        &service,
        preparation(
            &workspace,
            42,
            RoomToneSelection::Exact {
                source: prepared.source.clone(),
                qualification: source.receipt.id().clone(),
            },
        ),
    );
    assert!(exact.error.is_none(), "{:?}", exact.error);
    assert!(exact.committed.is_none());
    assert_eq!(
        *exact.workspace.as_ref().unwrap().document,
        *workspace.document
    );
    assert_eq!(
        exact.workspace.as_ref().unwrap().can_undo,
        workspace.can_undo
    );
    let exact = exact.room_tone.unwrap();
    assert_eq!(exact.ticket, 42);
    assert_eq!(exact.source, prepared.source);
    assert_eq!(exact.audition, prepared.audition);
    let reader = ProjectStore::open(&workspace.path, AccessMode::ReadOnly).unwrap();
    assert_eq!(reader.snapshot().unwrap(), *workspace.document);
    reader.validate().unwrap();
    drop(reader);
    let closed = command(&service, ProjectRequest::Close);
    assert!(closed.workspace.is_none() && closed.room_tone.is_none());
}

#[test]
fn native_room_tone_preparation_rejects_stale_context_receipts_and_invalid_ranges() {
    let scratch = tempfile::tempdir().unwrap();
    let service = ProjectService::start(
        Arc::new(|| {}),
        Some(ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap()),
    )
    .unwrap();
    let workspace = initialize(&service);
    let prepared = prepare(&service, &workspace);
    let source = &workspace.sources[&prepared.source.asset];
    let qualification = source.receipt.id().clone();
    let count = source.video_index.as_ref().unwrap().frames().len() as u64;
    for (start, end) in [(0, 0), (4, 1), (count, count + 1)] {
        assert_rejected(
            &service,
            &workspace,
            preparation(
                &workspace,
                42,
                RoomToneSelection::Original {
                    asset: prepared.source.asset.clone(),
                    qualification: qualification.clone(),
                    ordinals: start..end,
                },
            ),
        );
    }
    for selection in [
        RoomToneSelection::Original {
            asset: prepared.source.asset.clone(),
            qualification: SourceQualificationId::new("b".repeat(64)).unwrap(),
            ordinals: 1..4,
        },
        RoomToneSelection::Original {
            asset: AssetId::new("missing").unwrap(),
            qualification: qualification.clone(),
            ordinals: 1..4,
        },
        RoomToneSelection::Exact {
            source: prepared.source.clone(),
            qualification: SourceQualificationId::new("b".repeat(64)).unwrap(),
        },
        RoomToneSelection::Exact {
            source: SourceAudio {
                asset: prepared.source.asset.clone(),
                span: SourceSpan::new(
                    SourceTimestamp {
                        ticks: 195_397,
                        time_base: SourceTimeBase::new(1, 96_000).unwrap(),
                    },
                    SourceTimestamp {
                        ticks: 205_004,
                        time_base: SourceTimeBase::new(1, 96_000).unwrap(),
                    },
                )
                .unwrap(),
            },
            qualification: qualification.clone(),
        },
    ] {
        assert_rejected(&service, &workspace, preparation(&workspace, 43, selection));
    }
    let updated = edited(
        &service,
        &workspace,
        ProjectEdit::InsertTime {
            at: ProjectFrame(0),
            duration: FrameDuration::new(3).unwrap(),
        },
    )
    .workspace
    .unwrap();
    assert_rejected(
        &service,
        &updated,
        preparation(&workspace, 44, selection(&workspace)),
    );
    command(&service, ProjectRequest::Close);
    let reopened = command(&service, ProjectRequest::Open(updated.path.clone()))
        .workspace
        .unwrap();
    assert_eq!(
        reopened.document.revision_id(),
        updated.document.revision_id()
    );
    assert_ne!(reopened.session, updated.session);
    assert_rejected(
        &service,
        &reopened,
        preparation(&updated, 45, selection(&updated)),
    );
}

#[test]
fn native_hold_audio_commits_once_preserves_scope_cursor_picture_and_reopens_history() {
    let scratch = tempfile::tempdir().unwrap();
    let service = ProjectService::start(
        Arc::new(|| {}),
        Some(ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap()),
    )
    .unwrap();
    let initial = initialize(&service);
    let paused = edited(
        &service,
        &initial,
        ProjectEdit::InsertTime {
            at: ProjectFrame(0),
            duration: FrameDuration::new(6).unwrap(),
        },
    );
    let hold = paused.committed.unwrap().selected_node.unwrap();
    let path = paused.workspace.unwrap().path.clone();
    command(&service, ProjectRequest::Close);
    {
        let mut store = ProjectStore::open(&path, AccessMode::ReadWrite).unwrap();
        let root = store.snapshot().unwrap().root().clone();
        seed_command(
            &mut store,
            Command::Group {
                parent: root,
                start: 0,
                end: 1,
                id: node("group"),
                label: "Pause group".into(),
            },
            "group-pause",
        );
        seed_command(
            &mut store,
            Command::SetMark {
                id: MarkId::new("hold-mark").unwrap(),
                owner: hold.clone(),
                label: "Retain point".into(),
                boundary: BoundaryAnchor {
                    coordinate: Anchor::Local {
                        node: hold.clone(),
                        position: ExactRatio::new(5, 2).unwrap(),
                    },
                    bias: InsertionBias::Right,
                },
                loss_policy: AnchorLossPolicy::KeepUnresolved,
            },
            "mark-pause",
        );
    }
    let before = command(&service, ProjectRequest::Open(path.clone()))
        .workspace
        .unwrap();
    let scope = SequenceScope::default()
        .descend(&before, &node("group"))
        .unwrap();
    let prepared = prepare(&service, &before);
    let cursor = ProjectFrame(4);
    assert_rejected(
        &service,
        &before,
        edit_request(
            &before,
            ProjectEdit::HoldAudio {
                node: hold.clone(),
                audio: HoldAudio::Silence,
            },
        ),
    );
    let apply = edit_request_in(
        &before,
        scope.clone(),
        cursor,
        ProjectEdit::HoldAudio {
            node: hold.clone(),
            audio: HoldAudio::RoomTone {
                source: prepared.source.clone(),
            },
        },
    );
    let update = command(&service, apply);
    assert!(update.error.is_none(), "{:?}", update.error);
    assert!(update.room_tone.is_none());
    let commit = update.committed.unwrap();
    assert!(commit.preserve_cursor);
    assert_eq!(commit.cursor, Some(cursor));
    assert_eq!(commit.scope, scope);
    assert_eq!(commit.selected_node, Some(hold.clone()));
    let after = update.workspace.unwrap();
    let mut expected = before.document.nodes().clone();
    let NodeKind::Hold { recipe } = &mut expected.get_mut(&hold).unwrap().kind else {
        panic!("Hold")
    };
    recipe.audio = HoldAudio::RoomTone {
        source: prepared.source,
    };
    assert_eq!(after.document.nodes(), &expected);
    assert_eq!(after.document.marks(), before.document.marks());
    assert_eq!(
        after.document.audio_bindings(),
        before.document.audio_bindings()
    );
    assert_eq!(after.document.assets(), before.document.assets());
    assert_eq!(after.plan.duration(), before.plan.duration());
    for frame in 0..before.plan.duration().frames() {
        let mut previous = before.plan.picture(ProjectFrame(frame)).unwrap();
        previous.revision_id = after.document.revision_id().clone();
        assert_eq!(after.plan.picture(ProjectFrame(frame)).unwrap(), previous);
    }
    assert_rejected(
        &service,
        &after,
        edit_request_in(
            &before,
            scope.clone(),
            cursor,
            ProjectEdit::HoldAudio {
                node: hold.clone(),
                audio: HoldAudio::Silence,
            },
        ),
    );
    let undone = command(
        &service,
        ProjectRequest::Undo {
            expected_revision: after.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    assert_eq!(undone.document.nodes(), before.document.nodes());
    assert_eq!(undone.document.marks(), before.document.marks());
    command(&service, ProjectRequest::Close);
    let reopened = command(&service, ProjectRequest::Open(path))
        .workspace
        .unwrap();
    let redone = command(
        &service,
        ProjectRequest::Redo {
            expected_revision: reopened.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    assert_eq!(redone.document.nodes(), after.document.nodes());
    let silent = command(
        &service,
        edit_request_in(
            &redone,
            scope,
            cursor,
            ProjectEdit::HoldAudio {
                node: hold,
                audio: HoldAudio::Silence,
            },
        ),
    );
    assert!(silent.error.is_none(), "{:?}", silent.error);
    assert_eq!(
        silent.workspace.unwrap().document.nodes(),
        before.document.nodes()
    );
}

#[test]
fn native_hold_audio_rejects_nonholds_wrong_scope_and_stale_session_without_retargeting() {
    let scratch = tempfile::tempdir().unwrap();
    let service = ProjectService::start(
        Arc::new(|| {}),
        Some(ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap()),
    )
    .unwrap();
    let initial = initialize(&service);
    let original = initial
        .document
        .children(initial.document.root())
        .next()
        .unwrap()
        .clone();
    let paused = edited(
        &service,
        &initial,
        ProjectEdit::InsertTime {
            at: ProjectFrame(0),
            duration: FrameDuration::new(3).unwrap(),
        },
    );
    let hold = paused.committed.unwrap().selected_node.unwrap();
    let workspace = paused.workspace.unwrap();
    for target in [original, workspace.document.root().clone(), node("missing")] {
        assert_rejected(
            &service,
            &workspace,
            edit_request(
                &workspace,
                ProjectEdit::HoldAudio {
                    node: target,
                    audio: HoldAudio::Silence,
                },
            ),
        );
    }
    assert_rejected(
        &service,
        &workspace,
        edit_request_in(
            &workspace,
            SequenceScope::test_path(vec![hold.clone()]),
            ProjectFrame(0),
            ProjectEdit::HoldAudio {
                node: hold.clone(),
                audio: HoldAudio::Silence,
            },
        ),
    );
    assert_rejected(
        &service,
        &workspace,
        edit_request_in(
            &workspace,
            SequenceScope::default(),
            ProjectFrame(-1),
            ProjectEdit::HoldAudio {
                node: hold.clone(),
                audio: HoldAudio::Silence,
            },
        ),
    );
    command(&service, ProjectRequest::Close);
    let reopened = command(&service, ProjectRequest::Open(workspace.path.clone()))
        .workspace
        .unwrap();
    assert_rejected(
        &service,
        &reopened,
        edit_request(
            &workspace,
            ProjectEdit::HoldAudio {
                node: hold,
                audio: HoldAudio::Silence,
            },
        ),
    );
}
