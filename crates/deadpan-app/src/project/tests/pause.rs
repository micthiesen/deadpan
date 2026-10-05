use super::*;

#[test]
fn native_capture_keeps_lower_clips_and_inherits_root_motion_once() {
    use deadpan_core::{ExactRatio, Framing, FramingCurve, FramingPose};
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
    let source = initialized.committed.unwrap().selected_node.unwrap();
    let original = initialized.workspace.unwrap();
    let curve = Framing::creep(
        FramingPose::identity(),
        FramingPose {
            center_x: ExactRatio::new(1, 3).unwrap(),
            center_y: ExactRatio::new(1, 2).unwrap(),
            scale: ExactRatio::new(27, 20).unwrap(),
        },
        FramingCurve::Smoothstep,
    )
    .unwrap();
    let framed = edited(
        &service,
        &original,
        ProjectEdit::SetFraming {
            node: source.clone(),
            framing: Some(curve.clone()),
        },
    )
    .workspace
    .unwrap();
    let split = edited(
        &service,
        &framed,
        ProjectEdit::Split {
            node: source,
            at: FrameDuration::new(60).unwrap(),
        },
    );
    let fragment = split.committed.unwrap().selected_node.unwrap();
    let split = split.workspace.unwrap();
    let outer = FramingPose {
        scale: ExactRatio::new(4, 5).unwrap(),
        ..Default::default()
    };
    let layered = edited(
        &service,
        &split,
        ProjectEdit::SetFraming {
            node: fragment,
            framing: Some(Framing::static_pose(outer).unwrap()),
        },
    )
    .workspace
    .unwrap();
    command(&service, ProjectRequest::Close);
    let root_curve = Framing::creep(
        FramingPose::identity(),
        FramingPose {
            center_x: ExactRatio::new(3, 5).unwrap(),
            center_y: ExactRatio::new(1, 2).unwrap(),
            scale: ExactRatio::new(6, 5).unwrap(),
        },
        FramingCurve::Linear,
    )
    .unwrap();
    {
        // Author the root through the shared headless command path; native
        // Camera deliberately targets selected root children rather than root.
        let mut store = ProjectStore::open(&layered.path, AccessMode::ReadWrite).unwrap();
        store
            .commit(&CommandRequest {
                project_id: layered.document.project_id().clone(),
                expected_revision: layered.document.revision_id().clone(),
                new_revision: RevisionId::new("root-motion").unwrap(),
                command: Command::SetFraming {
                    node: layered.document.root().clone(),
                    framing: Some(root_curve.clone()),
                },
            })
            .unwrap();
    }
    let before = command(&service, ProjectRequest::Open(layered.path.clone()))
        .workspace
        .unwrap();
    let inserted = edited(
        &service,
        &before,
        ProjectEdit::InsertTime {
            at: ProjectFrame(70),
            duration: FrameDuration::new(11).unwrap(),
        },
    );
    let hold = inserted.committed.unwrap().selected_node.unwrap();
    let after = inserted.workspace.unwrap();
    let NodeKind::Hold { recipe } = &after.document.nodes()[&hold].kind else {
        panic!("pause")
    };
    let context = recipe.picture_context.clone().unwrap();
    assert_eq!(context.canvases.len(), 1);
    assert_eq!(
        context.canvases[0].layers,
        vec![
            Some(
                curve
                    .evaluate(
                        ExactRatio::new(139, 2).unwrap(),
                        FrameDuration::new(120).unwrap()
                    )
                    .unwrap()
            ),
            Some(outer),
        ]
    );
    let picture = after.plan.picture(ProjectFrame(75)).unwrap();
    assert_eq!(picture.picture_context.as_deref(), Some(&context));
    assert_eq!(picture.framing.len(), 2);
    assert_eq!(picture.framing[0].instance.node, hold);
    assert!(picture.framing[0].pose.is_none());
    assert_eq!(&picture.framing[1].instance.node, after.document.root());
    assert_eq!(
        picture.framing[1].pose,
        Some(
            root_curve
                .evaluate(
                    ExactRatio::new(151, 2).unwrap(),
                    FrameDuration::new(131).unwrap(),
                )
                .unwrap()
        )
    );
    let index = after
        .sources
        .values()
        .next()
        .unwrap()
        .video_index
        .as_ref()
        .unwrap();
    assert_eq!(
        picture.picture.select_source_frame(index).unwrap(),
        before
            .plan
            .picture(ProjectFrame(69))
            .unwrap()
            .picture
            .select_source_frame(index)
            .unwrap()
    );
    let local = edited(
        &service,
        &after,
        ProjectEdit::SetFraming {
            node: hold.clone(),
            framing: Some(
                Framing::static_pose(FramingPose {
                    scale: ExactRatio::new(21, 20).unwrap(),
                    ..Default::default()
                })
                .unwrap(),
            ),
        },
    )
    .workspace
    .unwrap();
    let NodeKind::Hold { recipe } = &local.document.nodes()[&hold].kind else {
        panic!("pause")
    };
    assert_eq!(recipe.picture_context.as_ref(), Some(&context));
    assert_eq!(local.plan.duration(), after.plan.duration());
}

#[test]
fn native_pause_retains_framing_and_repeated_capture_does_not_grow_it() {
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
    let saved = edited(
        &service,
        &framed,
        ProjectEdit::InsertTime {
            at: ProjectFrame(5),
            duration: FrameDuration::new(11).unwrap(),
        },
    );
    let inserted = saved.committed.unwrap().selected_node.unwrap();
    let after = saved.workspace.unwrap();
    let NodeKind::Hold { recipe } = &after.document.nodes()[&inserted].kind else {
        panic!("selected pause")
    };
    let context = recipe.picture_context.as_ref().unwrap().clone();
    let selected_picture = framed.plan.picture(ProjectFrame(4)).unwrap();
    let index = framed
        .sources
        .values()
        .next()
        .unwrap()
        .video_index
        .as_ref()
        .unwrap();
    for frame in 5..16 {
        let sample = after.plan.picture(ProjectFrame(frame)).unwrap();
        assert_eq!(sample.picture_context.as_deref(), Some(&context));
        assert_eq!(
            sample.picture.select_source_frame(index).unwrap(),
            selected_picture.picture.select_source_frame(index).unwrap()
        );
    }
    assert_eq!(context.canvases.len(), 1);
    assert_eq!(
        context.canvases[0].layers,
        vec![selected_picture.framing[0].pose]
    );
    assert_eq!(after.document.assets(), framed.document.assets());
    assert_eq!(after.single_source, framed.single_source);
    assert_eq!(
        after.plan.duration().frames(),
        framed.plan.duration().frames() + 11
    );
    let again = edited(
        &service,
        &after,
        ProjectEdit::InsertTime {
            at: ProjectFrame(8),
            duration: FrameDuration::new(2).unwrap(),
        },
    );
    let again_id = again.committed.unwrap().selected_node.unwrap();
    let again = again.workspace.unwrap();
    let NodeKind::Hold { recipe } = &again.document.nodes()[&again_id].kind else {
        panic!("second selected pause")
    };
    assert_eq!(recipe.picture_context.as_ref(), Some(&context));
    command(&service, ProjectRequest::Close);
    let reopened = command(&service, ProjectRequest::Open(again.path.clone()))
        .workspace
        .unwrap();
    assert_eq!(*reopened.document, *again.document);
    let undone = command(
        &service,
        ProjectRequest::Undo {
            expected_revision: reopened.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    assert_eq!(undone.document.nodes(), after.document.nodes());
    let undone = command(
        &service,
        ProjectRequest::Undo {
            expected_revision: undone.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    assert_eq!(undone.document.nodes(), framed.document.nodes());
    assert_eq!(
        undone.document.audio_bindings(),
        framed.document.audio_bindings()
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
        // Identity-only pictures still retain the old canvas/letterboxing for
        // later aspect changes; omitting context would fit the raw source anew.
        let context = recipe.picture_context.as_ref().unwrap();
        assert_eq!(context.canvases.len(), 1);
        assert_eq!(
            context.canvases[0].width,
            before.document.presentation_basis().width
        );
        assert_eq!(
            context.canvases[0].height,
            before.document.presentation_basis().height
        );
        assert_eq!(context.canvases[0].fit, deadpan_core::CapturedFit::Fit);
        assert!(context.canvases[0].layers.is_empty());
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
                cursor: ProjectFrame(at),
                scope: SequenceScope::default(),
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
fn native_pause_supports_empty_sequence_and_repeat_seams_but_refuses_repeat_interiors() {
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
    let shifted = edited(
        &service,
        &repeated,
        ProjectEdit::InsertTime {
            at: ProjectFrame(0),
            duration: FrameDuration::new(2).unwrap(),
        },
    );
    let selected = shifted.committed.unwrap().selected_node.unwrap();
    let shifted = shifted.workspace.unwrap();
    assert_eq!(shifted.plan.duration().frames(), 22);
    assert_eq!(
        shifted.document.children(shifted.document.root()).count(),
        2
    );
    assert!(
        matches!(&shifted.document.nodes()[&selected].kind, NodeKind::Hold { recipe } if recipe.duration.frames() == 2)
    );
    let rejected = command(
        &service,
        edit_request(
            &shifted,
            ProjectEdit::InsertTime {
                at: ProjectFrame(3),
                duration: FrameDuration::new(2).unwrap(),
            },
        ),
    );
    assert!(rejected.error.is_some());
    assert!(rejected.committed.is_none());
    assert_eq!(*rejected.workspace.unwrap().document, *shifted.document);
    command(&service, ProjectRequest::Close);
    let reopened = command(&service, ProjectRequest::Open(shifted.path.clone()))
        .workspace
        .unwrap();
    assert_eq!(*reopened.document, *shifted.document);
    let undone = command(
        &service,
        ProjectRequest::Undo {
            expected_revision: reopened.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    assert_eq!(undone.document.nodes(), repeated.document.nodes());
    assert_eq!(
        undone.document.audio_bindings(),
        repeated.document.audio_bindings()
    );
}

#[test]
fn native_pause_inside_a_source_fragment_preserves_the_following_repeat_and_durable_history() {
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
    let source = initialized.committed.unwrap().selected_node.unwrap();
    let original = initialized.workspace.unwrap();
    let framed = edited(
        &service,
        &original,
        ProjectEdit::SetFraming {
            node: source.clone(),
            framing: Some(
                deadpan_core::Framing::creep(
                    deadpan_core::FramingPose::identity(),
                    deadpan_core::FramingPose {
                        scale: deadpan_core::ExactRatio::new(27, 20).unwrap(),
                        ..Default::default()
                    },
                    deadpan_core::FramingCurve::Smoothstep,
                )
                .unwrap(),
            ),
        },
    )
    .workspace
    .unwrap();
    let seam = framed.plan.duration().frames() / 2;
    let at = 17;
    assert!(seam > at);
    let split = edited(
        &service,
        &framed,
        ProjectEdit::Split {
            node: source,
            at: FrameDuration::new(seam).unwrap(),
        },
    );
    let right = split.committed.unwrap().selected_node.unwrap();
    let split = split.workspace.unwrap();
    let repeated = edited(
        &service,
        &split,
        ProjectEdit::WrapRepeat {
            node: right,
            plays: 3,
        },
    );
    let repeat = repeated.committed.unwrap().selected_node.unwrap();
    let before = repeated.workspace.unwrap();
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
    let entry = before.plan.picture(ProjectFrame(at - 1)).unwrap();
    let expected_frame = entry.picture.select_source_frame(index).unwrap();
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
    assert_eq!(
        after.plan.duration().frames(),
        before.plan.duration().frames() + 11
    );
    assert_eq!(after.document.children(after.document.root()).count(), 4);
    assert_eq!(
        after.document.nodes()[&repeat],
        before.document.nodes()[&repeat]
    );
    assert_eq!(after.document.assets(), before.document.assets());
    assert_eq!(after.single_source, original.single_source);
    let NodeKind::Hold { recipe } = &after.document.nodes()[&selected].kind else {
        panic!("new pause is selected")
    };
    assert_eq!(recipe.duration.frames(), 11);
    assert_eq!(recipe.audio, HoldAudio::Silence);
    assert_eq!(
        recipe.video,
        HoldVideo::Freeze {
            asset,
            timestamp: deadpan_core::SourceTimestamp {
                ticks: expected_frame.pts,
                time_base: index.time_base(),
            },
        }
    );
    let captured = recipe.picture_context.as_ref().unwrap();
    assert_eq!(captured.canvases.len(), 1);
    assert_eq!(
        captured.canvases[0].layers,
        entry.framing[..entry.framing.len() - 1]
            .iter()
            .map(|layer| layer.pose)
            .collect::<Vec<_>>()
    );
    for frame in 0..after.plan.duration().frames() {
        let actual = after.plan.picture(ProjectFrame(frame)).unwrap();
        if (at..at + 11).contains(&frame) {
            assert_eq!(actual.instance.node, selected);
            assert_eq!(
                actual.picture.select_source_frame(index).unwrap(),
                expected_frame
            );
            assert_eq!(actual.picture_context.as_deref(), Some(captured));
        } else {
            let old_frame = if frame < at { frame } else { frame - 11 };
            let expected = before.plan.picture(ProjectFrame(old_frame)).unwrap();
            assert_eq!(actual.picture, expected.picture, "picture at {frame}");
            if old_frame >= seam {
                assert_eq!(actual.instance, expected.instance, "Repeat play at {frame}");
                assert_eq!(actual.gap_after, expected.gap_after);
                assert_eq!(
                    actual.framing[..actual.framing.len() - 1],
                    expected.framing[..expected.framing.len() - 1],
                    "Repeat retains its framing clocks at {frame}"
                );
            }
        }
    }
    command(&service, ProjectRequest::Close);
    let reopened = command(&service, ProjectRequest::Open(after.path.clone()))
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
    assert_eq!(undone.plan.duration(), before.plan.duration());
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
}

#[test]
fn native_pause_descends_to_nested_sequence_and_keeps_its_framing_live() {
    use deadpan_core::{ExactRatio, Framing, FramingPose};

    let scratch = tempfile::tempdir().unwrap();
    let library = ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap();
    let service = ProjectService::start(Arc::new(|| {}), Some(library)).unwrap();
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
    let source = initialized.committed.unwrap().selected_node.unwrap();
    let baseline = initialized.workspace.unwrap();

    // Make two physical Sources, then retain them beneath two live Sequence
    // owners. A cut inside the first Source must land in the inner Sequence.
    let split = edited(
        &service,
        &baseline,
        ProjectEdit::Split {
            node: source.clone(),
            at: FrameDuration::new(40).unwrap(),
        },
    );
    let split = split.workspace.unwrap();
    command(&service, ProjectRequest::Close);

    let inner = NodeId::new("pause-inner-group").unwrap();
    let outer = NodeId::new("pause-outer-group").unwrap();
    let child_pose = FramingPose {
        center_x: ExactRatio::new(1, 3).unwrap(),
        scale: ExactRatio::new(3, 2).unwrap(),
        ..Default::default()
    };
    let inner_pose = FramingPose {
        center_x: ExactRatio::new(1, 4).unwrap(),
        scale: ExactRatio::new(5, 4).unwrap(),
        ..Default::default()
    };
    let outer_pose = FramingPose {
        center_y: ExactRatio::new(2, 3).unwrap(),
        scale: ExactRatio::new(6, 5).unwrap(),
        ..Default::default()
    };
    let root_pose = FramingPose {
        center_y: ExactRatio::new(1, 3).unwrap(),
        scale: ExactRatio::new(4, 5).unwrap(),
        ..Default::default()
    };
    let path = split.path.clone();
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite).unwrap();
    let snapshot = store.snapshot().unwrap();
    let NodeKind::Sequence { children } = &snapshot.nodes()[snapshot.root()].kind else {
        panic!("project root")
    };
    assert_eq!(children.len(), 2);
    let left_partition = children[0].clone();
    let left_source = match &snapshot.nodes()[&left_partition].kind {
        NodeKind::Retime { child, .. } => child.clone(),
        kind => panic!("expected transparent left partition, got {kind:?}"),
    };
    seed_command(
        &mut store,
        Command::SetFraming {
            node: left_source,
            framing: Some(Framing::static_pose(child_pose).unwrap()),
        },
        "pause-child-crop",
    );
    seed_command(
        &mut store,
        Command::Group {
            parent: snapshot.root().clone(),
            start: 0,
            end: 2,
            id: inner.clone(),
            label: "Inner group".into(),
        },
        "pause-inner-group",
    );
    seed_command(
        &mut store,
        Command::SetFraming {
            node: inner.clone(),
            framing: Some(Framing::static_pose(inner_pose).unwrap()),
        },
        "pause-inner-framing",
    );
    seed_command(
        &mut store,
        Command::Group {
            parent: snapshot.root().clone(),
            start: 0,
            end: 1,
            id: outer.clone(),
            label: "Outer group".into(),
        },
        "pause-outer-group",
    );
    seed_command(
        &mut store,
        Command::SetFraming {
            node: outer.clone(),
            framing: Some(Framing::static_pose(outer_pose).unwrap()),
        },
        "pause-outer-framing",
    );
    seed_command(
        &mut store,
        Command::SetFraming {
            node: snapshot.root().clone(),
            framing: Some(Framing::static_pose(root_pose).unwrap()),
        },
        "pause-root-framing",
    );
    drop(store);

    let before = command(&service, ProjectRequest::Open(path.clone()))
        .workspace
        .unwrap();
    let at = ProjectFrame(17);
    let duration = FrameDuration::new(5).unwrap();
    let target = before.document.insert_time_target(at).unwrap();
    assert_eq!(target.parent, inner);
    let split_target = target.split.as_ref().unwrap();
    assert_eq!(split_target.target, left_partition);
    assert_eq!(split_target.at, FrameDuration::new(17).unwrap());
    assert!(split_target.required_ids > 0);

    // Native preparation allocates exactly the IDs core says this split needs,
    // rather than estimating from the containing root subtree.
    let mut allocated = 0usize;
    super::super::pause::prepare(
        &before,
        at,
        duration,
        RevisionId::new("prepared-nested-pause").unwrap(),
        NodeId::new("prepared-nested-hold").unwrap(),
        false,
        || {
            let id = NodeId::new(format!("prepared-split-{allocated}")).unwrap();
            allocated += 1;
            id
        },
    )
    .unwrap();
    assert_eq!(allocated, split_target.required_ids);

    let index = before
        .sources
        .values()
        .find_map(|source| source.video_index.as_ref())
        .unwrap();
    let expected_left = before
        .plan
        .picture(ProjectFrame(at.0 - 1))
        .unwrap()
        .picture
        .select_source_frame(index)
        .unwrap()
        .clone();
    let first_insert = edited(&service, &before, ProjectEdit::InsertTime { at, duration });
    let first_commit = first_insert.committed.as_ref().unwrap();
    assert_eq!(first_commit.cursor, Some(at));
    let first_selection = first_commit.selected_node.clone().unwrap();
    let first_hold = first_selection.clone();
    let after_first = first_insert.workspace.unwrap();
    let NodeKind::Sequence { children } = &after_first.document.nodes()[&inner].kind else {
        panic!("nested insertion owner remains a Sequence")
    };
    assert_eq!(children.len(), 4);
    assert_eq!(children[1], first_hold);
    assert_eq!(
        after_first
            .document
            .children(after_first.document.root())
            .count(),
        1
    );
    assert_eq!(
        after_first
            .plan
            .picture(at)
            .unwrap()
            .picture
            .select_source_frame(index)
            .unwrap(),
        &expected_left,
        "the inserted freeze uses the exact picture immediately to its left"
    );

    let NodeKind::Hold {
        recipe: first_recipe,
    } = &after_first.document.nodes()[&first_hold].kind
    else {
        panic!("first pause")
    };
    let first_context = first_recipe.picture_context.as_ref().unwrap();
    assert_eq!(first_context.canvases.len(), 1);
    assert_eq!(
        first_context.canvases[0].layers,
        vec![Some(child_pose), None],
        "the cropped Source remains below its transparent partition"
    );
    let held = after_first.plan.picture(at).unwrap();
    let expected_scopes = [
        first_hold.clone(),
        inner.clone(),
        outer.clone(),
        after_first.document.root().clone(),
    ];
    assert_eq!(
        held.framing
            .iter()
            .map(|scope| scope.instance.node.clone())
            .collect::<Vec<_>>(),
        expected_scopes,
        "the descendant crop is captured while each live owner appears once"
    );
    assert_eq!(held.picture_context.as_deref(), Some(first_context));

    // The old split seam moved right by five frames. It is now a seam in the
    // nested group, so the second preparation needs no Split identities.
    let moved_seam = ProjectFrame(45);
    let seam_target = after_first.document.insert_time_target(moved_seam).unwrap();
    assert_eq!(seam_target.parent, inner);
    assert_eq!(seam_target.index, 3);
    assert!(seam_target.split.is_none());
    let mut seam_allocations = 0usize;
    super::super::pause::prepare(
        &after_first,
        moved_seam,
        FrameDuration::new(3).unwrap(),
        RevisionId::new("prepared-nested-seam").unwrap(),
        NodeId::new("prepared-seam-hold").unwrap(),
        false,
        || {
            seam_allocations += 1;
            NodeId::new(format!("unexpected-seam-split-{seam_allocations}")).unwrap()
        },
    )
    .unwrap();
    assert_eq!(seam_allocations, 0);

    let expected_seam = after_first
        .plan
        .picture(ProjectFrame(moved_seam.0 - 1))
        .unwrap()
        .picture
        .select_source_frame(index)
        .unwrap()
        .clone();
    let second_insert = edited(
        &service,
        &after_first,
        ProjectEdit::InsertTime {
            at: moved_seam,
            duration: FrameDuration::new(3).unwrap(),
        },
    );
    let second_commit = second_insert.committed.as_ref().unwrap();
    assert_eq!(second_commit.cursor, Some(moved_seam));
    let second_selection = second_commit.selected_node.clone();
    let second_hold = second_selection.clone().unwrap();
    let after = second_insert.workspace.unwrap();
    assert_eq!(after.document.children(after.document.root()).count(), 1);
    let NodeKind::Sequence { children } = &after.document.nodes()[&inner].kind else {
        panic!("nested insertion owner remains a Sequence")
    };
    assert_eq!(children.len(), 5);
    assert_eq!(children[1], first_hold);
    assert_eq!(children[3], second_hold);
    assert_eq!(
        after.plan.duration().frames(),
        before.plan.duration().frames() + 8
    );
    assert_eq!(
        after
            .plan
            .picture(moved_seam)
            .unwrap()
            .picture
            .select_source_frame(index)
            .unwrap(),
        &expected_seam
    );
    assert_eq!(
        after
            .plan
            .picture(ProjectFrame(48))
            .unwrap()
            .picture
            .select_source_frame(index)
            .unwrap(),
        before
            .plan
            .picture(ProjectFrame(40))
            .unwrap()
            .picture
            .select_source_frame(index)
            .unwrap(),
        "speech after the nested seams keeps its original source position"
    );
    let NodeKind::Hold {
        recipe: second_recipe,
    } = &after.document.nodes()[&second_hold].kind
    else {
        panic!("second pause")
    };
    let captured = second_recipe.picture_context.as_ref().unwrap().clone();
    assert_eq!(captured.canvases[0].layers, vec![Some(child_pose), None]);
    assert_eq!(second_selection.as_ref(), Some(&second_hold));

    // Edit the live group and the Hold's own Camera framing through
    // the same typed command path. Neither edit replaces the captured crop.
    command(&service, ProjectRequest::Close);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite).unwrap();
    let moved_inner_pose = FramingPose {
        center_x: ExactRatio::new(3, 4).unwrap(),
        scale: ExactRatio::new(7, 5).unwrap(),
        ..Default::default()
    };
    let hold_pose = FramingPose {
        center_y: ExactRatio::new(1, 5).unwrap(),
        scale: ExactRatio::new(9, 8).unwrap(),
        ..Default::default()
    };
    seed_command(
        &mut store,
        Command::SetFraming {
            node: inner.clone(),
            framing: Some(Framing::static_pose(moved_inner_pose).unwrap()),
        },
        "pause-inner-framing-edited",
    );
    seed_command(
        &mut store,
        Command::SetFraming {
            node: second_hold.clone(),
            framing: Some(Framing::static_pose(hold_pose).unwrap()),
        },
        "pause-hold-camera-framing",
    );
    drop(store);
    let edited_framing = command(&service, ProjectRequest::Open(path))
        .workspace
        .unwrap();
    let NodeKind::Hold { recipe } = &edited_framing.document.nodes()[&second_hold].kind else {
        panic!("Hold remains in the document")
    };
    assert_eq!(recipe.picture_context.as_ref(), Some(&captured));
    let held = edited_framing.plan.picture(moved_seam).unwrap();
    assert_eq!(held.picture_context.as_deref(), Some(&captured));
    assert_eq!(
        held.framing
            .iter()
            .map(|scope| scope.instance.node.clone())
            .collect::<Vec<_>>(),
        [
            second_hold.clone(),
            inner.clone(),
            outer.clone(),
            edited_framing.document.root().clone()
        ]
    );
    assert_eq!(held.framing[0].pose, Some(hold_pose));
    assert_eq!(held.framing[1].pose, Some(moved_inner_pose));
    assert_eq!(held.framing[2].pose, Some(outer_pose));
    assert_eq!(held.framing[3].pose, Some(root_pose));
    command(&service, ProjectRequest::Close);
}

#[test]
fn black_pause_inserts_background_picture_and_silence_as_one_undo() {
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
    let before = initialized.workspace.unwrap();
    let total = before.plan.duration().frames();
    let inserted = edited(
        &service,
        &before,
        ProjectEdit::InsertBlack {
            at: ProjectFrame(30),
            duration: FrameDuration::new(6).unwrap(),
        },
    );
    assert!(inserted.error.is_none(), "{:?}", inserted.error);
    assert_eq!(
        inserted.message.as_deref(),
        Some("Inserted a 6 frame silent black pause at boundary 30 and saved")
    );
    let hold = inserted.committed.unwrap().selected_node.unwrap();
    let after = inserted.workspace.unwrap();
    let NodeKind::Hold { recipe } = &after.document.nodes()[&hold].kind else {
        panic!("pause")
    };
    assert_eq!(recipe.video, deadpan_core::HoldVideo::Background);
    assert_eq!(recipe.audio, deadpan_core::HoldAudio::Silence);
    assert!(recipe.picture_context.is_none());
    assert_eq!(after.plan.duration().frames(), total + 6);
    // The shared plan shows black inside the pause and the Original around it.
    for (frame, black) in [(29, false), (30, true), (35, true), (36, false)] {
        let picture = after.plan.picture(ProjectFrame(frame)).unwrap().picture;
        assert_eq!(
            matches!(picture, deadpan_plan::Picture::Background),
            black,
            "frame {frame}"
        );
    }
    let restored = command(
        &service,
        ProjectRequest::Undo {
            expected_revision: after.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    assert_eq!(restored.plan.duration().frames(), total);
}
