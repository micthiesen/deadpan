use super::*;
use deadpan_core::{ExactRatio, FrameRange, PitchPolicy, RetimePurpose};

fn change(target: &NodeId, speed: ExactRatio, pitch: PitchPolicy, wrap: bool) -> ProjectEdit {
    ProjectEdit::Retime {
        node: target.clone(),
        speed,
        pitch,
        wrap,
    }
}

#[test]
fn native_original_retime_adjusts_one_wrapper_and_has_durable_undo() {
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
    let expected = i64::try_from(
        ExactRatio::integer(before.plan.duration().frames())
            .checked_mul(ExactRatio::new(4, 3).unwrap())
            .unwrap()
            .round_even()
            .unwrap(),
    )
    .unwrap();
    let wrapped = edited(
        &service,
        &before,
        change(
            &original,
            ExactRatio::new(3, 4).unwrap(),
            PitchPolicy::Preserve,
            false,
        ),
    );
    let id = wrapped.committed.unwrap().selected_node.unwrap();
    let wrapped = wrapped.workspace.unwrap();
    assert_ne!(id, original);
    assert_eq!(wrapped.plan.duration().frames(), expected);
    assert_eq!(
        wrapped.document.nodes()[&original],
        before.document.nodes()[&original]
    );
    assert_eq!(wrapped.document.assets(), before.document.assets());
    assert_eq!(wrapped.single_source, before.single_source);
    assert!(
        matches!(&wrapped.document.nodes()[&id].kind, NodeKind::Retime { child, pitch: PitchPolicy::Preserve, purpose: RetimePurpose::Edit, .. } if child == &original)
    );
    let invalid = command(
        &service,
        edit_request(
            &before,
            change(&original, ExactRatio::ONE, PitchPolicy::Preserve, false),
        ),
    );
    assert!(invalid.error.unwrap().contains("changed"));
    assert_eq!(*invalid.workspace.unwrap().document, *wrapped.document);

    let updated = edited(
        &service,
        &wrapped,
        change(&id, ExactRatio::integer(2), PitchPolicy::FollowSpeed, false),
    );
    assert_eq!(updated.committed.unwrap().selected_node, Some(id.clone()));
    let updated = updated.workspace.unwrap();
    assert_eq!(
        updated.plan.duration().frames(),
        before.plan.duration().frames() / 2
    );
    assert_eq!(
        updated.document.nodes().len(),
        wrapped.document.nodes().len()
    );
    let same = command(
        &service,
        edit_request(
            &updated,
            change(&id, ExactRatio::integer(2), PitchPolicy::FollowSpeed, false),
        ),
    );
    assert!(same.committed.is_none());
    assert_eq!(*same.workspace.unwrap().document, *updated.document);
    let path = updated.path.clone();
    command(&service, ProjectRequest::Close);
    let opened = command(&service, ProjectRequest::Open(path))
        .workspace
        .unwrap();
    assert_eq!(*opened.document, *updated.document);
    let undone = command(
        &service,
        ProjectRequest::Undo {
            expected_revision: opened.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    assert_eq!(undone.document.nodes(), wrapped.document.nodes());
    let undone_again = command(
        &service,
        ProjectRequest::Undo {
            expected_revision: undone.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    assert_eq!(undone_again.document.nodes(), before.document.nodes());
    assert!(
        !undone_again.can_undo,
        "the pinned Original baseline remains protected"
    );
    let redone = command(
        &service,
        ProjectRequest::Redo {
            expected_revision: undone_again.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    assert_eq!(redone.document.nodes(), wrapped.document.nodes());
}

#[test]
fn retime_resolves_once_and_rejects_empty_zero_and_overflow_without_commits() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("speed.deadpan");
    drop(seed_holds(&path, &["a"]));
    let service = ProjectService::new(Arc::new(|| {})).unwrap();
    let before = command(&service, ProjectRequest::Open(path))
        .workspace
        .unwrap();
    for speed in [
        ExactRatio::ZERO,
        ExactRatio::integer(-1),
        ExactRatio::integer(21),
        ExactRatio::new(1, i128::MAX).unwrap(),
    ] {
        let failed = command(
            &service,
            edit_request(
                &before,
                change(&node("a"), speed, PitchPolicy::Preserve, false),
            ),
        );
        assert!(failed.error.is_some(), "{speed:?}");
        assert!(failed.committed.is_none());
        assert_eq!(*failed.workspace.unwrap().document, *before.document);
    }
    let wrapped = edited(
        &service,
        &before,
        change(
            &node("a"),
            ExactRatio::integer(4),
            PitchPolicy::FollowSpeed,
            false,
        ),
    );
    let id = wrapped.committed.unwrap().selected_node.unwrap();
    let wrapped = wrapped.workspace.unwrap();
    assert_eq!(
        wrapped.plan.duration().frames(),
        2,
        "2.5 frames ties to even"
    );
    let updated = edited(
        &service,
        &wrapped,
        change(&id, ExactRatio::integer(2), PitchPolicy::FollowSpeed, false),
    );
    let updated = updated.workspace.unwrap();
    assert_eq!(
        updated.plan.duration().frames(),
        5,
        "speed is relative to the retained 10-frame input, not the previously rounded output"
    );
    let nested = edited(
        &service,
        &updated,
        change(&id, ExactRatio::integer(2), PitchPolicy::FollowSpeed, true),
    );
    let outer = nested.committed.unwrap().selected_node.unwrap();
    let nested = nested.workspace.unwrap();
    assert_ne!(outer, id);
    assert_eq!(nested.plan.duration().frames(), 2);
    assert_eq!(nested.document.nodes()[&id], updated.document.nodes()[&id]);
}

#[test]
fn nested_scope_and_split_partition_are_preserved_by_retime() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("fragment.deadpan");
    let mut store = seed_holds(&path, &[]);
    let fragment = BeatNode {
        label: "Fragment".into(),
        audio_edges: Default::default(),
        framing: None,
        kind: NodeKind::Retime {
            child: node("a"),
            duration: FrameDuration::new(5).unwrap(),
            mapping: FrameRange::new(ProjectFrame(2), ProjectFrame(7)).unwrap(),
            pitch: PitchPolicy::FollowSpeed,
            purpose: RetimePurpose::Partition,
        },
    };
    seed_command(
        &mut store,
        Command::Insert {
            parent: node("root"),
            index: 0,
            subtree: Subtree {
                root: node("group"),
                nodes: BTreeMap::from([
                    (
                        node("group"),
                        BeatNode::sequence("Group", vec![node("fragment"), node("empty")]),
                    ),
                    (node("empty"), BeatNode::sequence("Empty", vec![])),
                    (node("fragment"), fragment.clone()),
                    (node("a"), BeatNode::hold("Input", hold(10))),
                ]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
        "insert-fragment",
    );
    drop(store);
    let service = ProjectService::new(Arc::new(|| {})).unwrap();
    let before = command(&service, ProjectRequest::Open(path))
        .workspace
        .unwrap();
    let scope = SequenceScope::default()
        .descend(&before, &node("group"))
        .unwrap();
    for (target, attempted_scope) in [
        (node("fragment"), SequenceScope::default()),
        (node("a"), scope.clone()),
        (node("empty"), scope.clone()),
    ] {
        let rejected = command(
            &service,
            edit_request_in(
                &before,
                attempted_scope,
                ProjectFrame(0),
                change(
                    &target,
                    ExactRatio::new(1, 2).unwrap(),
                    PitchPolicy::Preserve,
                    false,
                ),
            ),
        );
        assert!(rejected.error.is_some());
        assert_eq!(*rejected.workspace.unwrap().document, *before.document);
    }
    let edited = command(
        &service,
        edit_request_in(
            &before,
            scope.clone(),
            ProjectFrame(3),
            change(
                &node("fragment"),
                ExactRatio::new(1, 2).unwrap(),
                PitchPolicy::Preserve,
                false,
            ),
        ),
    );
    assert!(edited.error.is_none(), "{:?}", edited.error);
    let commit = edited.committed.unwrap();
    assert_eq!(commit.scope, scope);
    assert_ne!(commit.selected_node.as_ref(), Some(&node("fragment")));
    let after = edited.workspace.unwrap();
    assert_eq!(after.document.nodes()[&node("fragment")], fragment);
    assert_eq!(after.plan.duration().frames(), 10);
}
