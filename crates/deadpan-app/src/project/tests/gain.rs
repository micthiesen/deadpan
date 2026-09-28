use super::*;
use crate::project::gain::{Proposal, Target};
use deadpan_core::{
    AudioTreatments, ClipGain, ExactRatio, GainClock, GainCurve, GainDb, GainEnvelope, GainRange,
    GainSegment,
};

fn recipe(trim: i32) -> AudioTreatments {
    AudioTreatments::from_clip_gain(
        ClipGain::new(
            GainDb::new(trim).unwrap(),
            true,
            vec![
                GainEnvelope::new(
                    GainClock::OwnerOutput,
                    GainRange::new(ExactRatio::integer(2), ExactRatio::integer(8)).unwrap(),
                    GainDb::UNITY,
                    vec![
                        GainSegment::new(
                            ExactRatio::integer(8),
                            GainDb::new(-1250).unwrap(),
                            GainCurve::Cubic {
                                control1: GainDb::new(3500).unwrap(),
                                control2: GainDb::new(-7000).unwrap(),
                            },
                        )
                        .unwrap(),
                    ],
                )
                .unwrap(),
            ],
            vec![GainRange::new(ExactRatio::new(1, 3).unwrap(), ExactRatio::integer(1)).unwrap()],
        )
        .unwrap(),
    )
}

fn target(workspace: &Workspace) -> Target {
    Target {
        session: workspace.session,
        project: workspace.document.project_id().clone(),
        revision: workspace.document.revision_id().clone(),
        scope: SequenceScope::default(),
        node: node("a"),
        cursor: ProjectFrame(7),
        entry: workspace.document.nodes()[&node("a")]
            .audio_treatments
            .clone(),
    }
}

#[test]
fn gain_proposals_leave_store_history_unchanged_and_apply_once_with_exact_undo() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("gain.deadpan");
    let mut store = seed_holds(&path, &["a", "b"]);
    seed_command(
        &mut store,
        Command::SetAudioTreatments {
            node: node("a"),
            treatments: recipe(-3000),
        },
        "entry-gain",
    );
    drop(store);
    let service = ProjectService::new(Arc::new(|| {})).unwrap();
    let before = command(&service, ProjectRequest::Open(path.clone()))
        .workspace
        .unwrap();
    let captured = target(&before);
    let mut revisions = Vec::new();
    for (change, trim) in [(1, 3000), (2, 6000)] {
        let proposal = Proposal {
            target: captured.clone(),
            draft: 77,
            change,
            treatments: recipe(trim),
        };
        let id = proposal.id();
        let update = command(&service, ProjectRequest::PrepareGain(proposal));
        assert!(update.error.is_none(), "{:?}", update.error);
        assert!(update.committed.is_none());
        assert!(Arc::ptr_eq(update.workspace.as_ref().unwrap(), &before));
        let gain = update.gain.unwrap();
        assert_eq!(gain.id, id);
        let proposed = gain.result.unwrap();
        assert_ne!(
            proposed.document.revision_id(),
            before.document.revision_id()
        );
        assert_eq!(
            proposed.document.nodes()[&node("a")].audio_treatments,
            recipe(trim)
        );
        assert_eq!(
            proposed.document.nodes()[&node("b")],
            before.document.nodes()[&node("b")]
        );
        assert_eq!(
            proposed.content,
            deadpan_playback::ContentIdentity::Proposed {
                base_revision: before.document.revision_id().clone(),
                draft: 77,
                change
            }
        );
        revisions.push(proposed.document.revision_id().clone());
        let read = ProjectStore::open(&path, AccessMode::ReadOnly).unwrap();
        assert_eq!(read.snapshot().unwrap(), *before.document);
    }
    assert_ne!(revisions[0], revisions[1]);
    let applied = command(
        &service,
        edit_request_in(
            &before,
            captured.scope.clone(),
            captured.cursor,
            ProjectEdit::SetAudioTreatments {
                node: captured.node.clone(),
                treatments: recipe(6000),
            },
        ),
    );
    assert!(applied.error.is_none(), "{:?}", applied.error);
    let committed = applied.committed.unwrap();
    assert!(committed.preserve_cursor);
    assert_eq!(committed.cursor, Some(ProjectFrame(7)));
    assert_eq!(committed.selected_node, Some(node("a")));
    let after = applied.workspace.unwrap();
    assert!(!revisions.contains(after.document.revision_id()));
    let no_op = command(
        &service,
        edit_request(
            &after,
            ProjectEdit::SetAudioTreatments {
                node: node("a"),
                treatments: recipe(6000),
            },
        ),
    );
    assert!(no_op.committed.is_none());
    assert_eq!(*no_op.workspace.unwrap().document, *after.document);
    let stale = command(
        &service,
        ProjectRequest::PrepareGain(Proposal {
            target: captured,
            draft: 77,
            change: 3,
            treatments: recipe(9000),
        }),
    );
    let failure = stale.gain.unwrap();
    assert_eq!(failure.id.change, 3);
    assert!(failure.result.is_err());
    assert_eq!(*stale.workspace.unwrap().document, *after.document);
    let undone = command(
        &service,
        ProjectRequest::Undo {
            expected_revision: after.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    assert_eq!(undone.document.nodes(), before.document.nodes());
    assert_eq!(undone.document.sounds(), before.document.sounds());
    command(&service, ProjectRequest::Close);
    let reopened = command(&service, ProjectRequest::Open(path))
        .workspace
        .unwrap();
    assert_eq!(*reopened.document, *undone.document);
}

#[test]
fn gain_proposal_failures_retain_identity_and_cannot_retarget_scope_recipe_or_session() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("gain-target.deadpan");
    drop(seed_holds(&path, &["a"]));
    let service = ProjectService::new(Arc::new(|| {})).unwrap();
    let before = command(&service, ProjectRequest::Open(path))
        .workspace
        .unwrap();
    for case in 0..7 {
        let mut captured = target(&before);
        match case {
            0 => captured.session += 1,
            1 => captured.project = ProjectId::new("another-project").unwrap(),
            2 => captured.node = node("root"),
            3 => captured.cursor = ProjectFrame(11),
            4 => captured.entry = recipe(0),
            5 => captured.scope = SequenceScope::test_path(vec![node("missing")]),
            _ => {}
        }
        let proposal = Proposal {
            target: captured,
            draft: if case == 6 { 0 } else { 1 },
            change: 1,
            treatments: recipe(3000),
        };
        let id = proposal.id();
        let update = command(&service, ProjectRequest::PrepareGain(proposal));
        assert!(update.error.is_none(), "case {case}");
        assert!(update.committed.is_none());
        let failure = update.gain.unwrap();
        assert_eq!(failure.id, id);
        assert!(failure.result.is_err());
        assert_eq!(*update.workspace.unwrap().document, *before.document);
    }
}
