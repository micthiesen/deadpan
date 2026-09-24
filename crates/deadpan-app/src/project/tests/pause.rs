use super::*;

#[test]
fn native_pause_never_silently_discards_the_frozen_pictures_framing() {
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
    let selected = initialized.committed.unwrap().selected_node.unwrap();
    let before = initialized.workspace.unwrap();
    let framing = deadpan_core::Framing::static_pose(deadpan_core::FramingPose {
        scale: deadpan_core::ExactRatio::new(27, 20).unwrap(),
        ..Default::default()
    })
    .unwrap();
    let framed = edited(
        &service,
        &before,
        ProjectEdit::SetFraming {
            node: selected,
            framing: Some(framing),
        },
    )
    .workspace
    .unwrap();
    let failed = command(
        &service,
        edit_request(
            &framed,
            ProjectEdit::InsertTime {
                at: ProjectFrame(5),
                duration: FrameDuration::new(11).unwrap(),
            },
        ),
    );
    assert!(failed.error.unwrap().contains("composition snapshot"));
    assert!(failed.committed.is_none());
    assert_eq!(*failed.workspace.unwrap().document, *framed.document);
}

#[test]
fn native_pause_freezes_measured_vfr_picture_and_keeps_one_undo_step() {
    let scratch = tempfile::tempdir().unwrap();
    let service = ProjectService::start(
        Arc::new(|| {}),
        Some(ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap()),
    )
    .unwrap();
    service
        .submit(ProjectRequest::CreateFromSource {
            path: fixture("vfr.mp4"),
        })
        .unwrap();
    let initialized = wait(&service, |update| {
        update.import.as_ref().is_some_and(|status| {
            matches!(status.stage, ImportStage::Complete | ImportStage::Failed)
        })
    });
    assert!(initialized.error.is_none(), "{:?}", initialized.error);
    let baseline = initialized.workspace.unwrap();
    let total = baseline.plan.duration().frames();
    for at in [0, 37, total] {
        let before = command(&service, ProjectRequest::Open(baseline.path.clone()))
            .workspace
            .unwrap();
        let left = before
            .plan
            .picture(ProjectFrame(if at == 0 { 0 } else { at - 1 }))
            .unwrap()
            .picture;
        let (asset, index) = before
            .sources
            .iter()
            .find_map(|(asset, source)| {
                source
                    .video_index
                    .as_ref()
                    .map(|index| (asset.clone(), index))
            })
            .unwrap();
        let expected = left.select_source_frame(index).unwrap().clone();
        let update = edited(
            &service,
            &before,
            ProjectEdit::InsertTime {
                at: ProjectFrame(at),
                duration: FrameDuration::new(11).unwrap(),
            },
        );
        let selected = update.committed.unwrap().selected_node.unwrap();
        let after = update.workspace.unwrap();
        assert_eq!(after.plan.duration().frames(), total + 11);
        assert_eq!(after.single_source, baseline.single_source);
        assert_eq!(after.document.assets(), baseline.document.assets());
        let NodeKind::Hold { recipe } = &after.document.nodes()[&selected].kind else {
            panic!("selected inserted pause")
        };
        assert_eq!(recipe.audio, HoldAudio::Silence);
        assert_eq!(
            recipe.video,
            HoldVideo::Freeze {
                asset,
                timestamp: deadpan_core::SourceTimestamp {
                    ticks: expected.pts,
                    time_base: index.time_base()
                }
            }
        );
        for frame in 0..(total + 11) {
            let picture = after.plan.picture(ProjectFrame(frame)).unwrap().picture;
            if (at..at + 11).contains(&frame) {
                assert_eq!(picture.select_source_frame(index).unwrap(), &expected);
            } else {
                let original = if frame < at { frame } else { frame - 11 };
                assert_eq!(
                    picture,
                    before.plan.picture(ProjectFrame(original)).unwrap().picture
                );
            }
        }
        for request in [
            edit_request(
                &before,
                ProjectEdit::InsertTime {
                    at: ProjectFrame(at),
                    duration: FrameDuration::new(2).unwrap(),
                },
            ),
            ProjectRequest::Edit {
                expected_session: after.session + 1,
                expected_revision: after.document.revision_id().clone(),
                edit: ProjectEdit::InsertTime {
                    at: ProjectFrame(at),
                    duration: FrameDuration::new(2).unwrap(),
                },
            },
        ] {
            let stale = command(&service, request);
            assert!(stale.error.is_some());
            assert!(stale.committed.is_none());
            assert_eq!(*stale.workspace.unwrap().document, *after.document);
        }
        let zero = command(
            &service,
            edit_request(
                &after,
                ProjectEdit::InsertTime {
                    at: ProjectFrame(at),
                    duration: FrameDuration::ZERO,
                },
            ),
        );
        assert!(zero.error.is_none());
        assert!(zero.message.unwrap().contains("no edit"));
        assert!(zero.committed.is_none());
        assert_eq!(*zero.workspace.unwrap().document, *after.document);
        command(&service, ProjectRequest::Close);
        let reopened = command(&service, ProjectRequest::Open(baseline.path.clone()))
            .workspace
            .unwrap();
        assert_eq!(*reopened.document, *after.document);
        let undone = command(
            &service,
            ProjectRequest::Undo {
                expected_revision: reopened.document.revision_id().clone(),
            },
        )
        .workspace
        .unwrap();
        assert_eq!(undone.document.nodes(), before.document.nodes());
        assert_eq!(
            undone.document.audio_bindings(),
            before.document.audio_bindings()
        );
        assert!(
            !undone.can_undo,
            "one edit stops at protected original baseline"
        );
        let redone = command(
            &service,
            ProjectRequest::Redo {
                expected_revision: undone.document.revision_id().clone(),
            },
        )
        .workspace
        .unwrap();
        assert_eq!(redone.document.nodes(), after.document.nodes());
        assert_eq!(
            redone.document.audio_bindings(),
            after.document.audio_bindings()
        );
        let reset = command(
            &service,
            ProjectRequest::Undo {
                expected_revision: redone.document.revision_id().clone(),
            },
        );
        assert!(reset.error.is_none());
    }
}

#[test]
fn native_pause_supports_empty_legacy_sequence_and_refuses_shifted_repeat_atomically() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("pause.deadpan");
    drop(seed_holds(&path, &[]));
    let service = ProjectService::start(Arc::new(|| {}), None).unwrap();
    let empty = command(&service, ProjectRequest::Open(path))
        .workspace
        .unwrap();
    let inserted = edited(
        &service,
        &empty,
        ProjectEdit::InsertTime {
            at: ProjectFrame(0),
            duration: FrameDuration::new(10).unwrap(),
        },
    );
    let pause = inserted.committed.unwrap().selected_node.unwrap();
    let inserted = inserted.workspace.unwrap();
    assert!(
        matches!(&inserted.document.nodes()[&pause].kind, NodeKind::Hold { recipe } if recipe.video == HoldVideo::Background)
    );
    let repeated = edited(
        &service,
        &inserted,
        ProjectEdit::WrapRepeat {
            node: pause,
            plays: 2,
        },
    )
    .workspace
    .unwrap();
    let rejected = command(
        &service,
        edit_request(
            &repeated,
            ProjectEdit::InsertTime {
                at: ProjectFrame(0),
                duration: FrameDuration::new(2).unwrap(),
            },
        ),
    );
    assert!(rejected.error.is_some());
    assert!(rejected.committed.is_none());
    assert_eq!(*rejected.workspace.unwrap().document, *repeated.document);
}
