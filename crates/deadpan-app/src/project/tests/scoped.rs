use super::*;
use crate::project::scoped::{Commit, Target};
use deadpan_core::{
    AudioTreatments, ClipGain, GainDb, InstancePath, IterationId, PitchPolicy, RepeatEditBranch,
    RepeatEditStep, RepeatInstance, ScopedNodeEdit, ScopedNodeTarget,
};

fn seed(path: &Path) {
    let mut store = seed_holds(path, &["a", "b"]);
    seed_command(
        &mut store,
        Command::WrapRepeat {
            node: node("a"),
            id: node("inner"),
            plays: 2,
            gap: None,
            anchor_policy: Default::default(),
        },
        "inner-plays",
    );
    seed_command(
        &mut store,
        Command::WrapRetime {
            node: node("inner"),
            id: node("retime"),
            duration: FrameDuration::new(15).unwrap(),
            pitch: PitchPolicy::Preserve,
        },
        "retime",
    );
    seed_command(
        &mut store,
        Command::WrapRepeat {
            node: node("retime"),
            id: node("outer"),
            plays: 2,
            gap: None,
            anchor_policy: Default::default(),
        },
        "outer-plays",
    );
}

fn iteration(revision: &str, ordinal: u32) -> IterationId {
    IterationId {
        allocation: RevisionId::new(revision).unwrap(),
        ordinal,
    }
}

fn capture(workspace: &Workspace, outer: RepeatEditBranch, inner: RepeatEditBranch) -> Target {
    Target {
        session: workspace.session,
        project: workspace.document.project_id().clone(),
        revision: workspace.document.revision_id().clone(),
        scope: SequenceScope::default(),
        root: node("outer"),
        target: ScopedNodeTarget {
            node: node("a"),
            repeats: vec![
                RepeatEditStep {
                    repeat: node("outer"),
                    branch: outer,
                },
                RepeatEditStep {
                    repeat: node("inner"),
                    branch: inner,
                },
            ],
        },
        presentation: None,
        cursor: ProjectFrame(12),
    }
}

fn play(revision: &str, ordinal: u32) -> RepeatEditBranch {
    RepeatEditBranch::Play {
        iteration: iteration(revision, ordinal),
    }
}

fn picture(outer: u32, inner: u32) -> InstancePath {
    InstancePath {
        node: node("a"),
        repeats: vec![
            RepeatInstance {
                node: node("outer"),
                iteration: iteration("outer-plays", outer),
            },
            RepeatInstance {
                node: node("inner"),
                iteration: iteration("inner-plays", inner),
            },
        ],
    }
}

fn gain(trim: i32) -> AudioTreatments {
    AudioTreatments::from_clip_gain(
        ClipGain::new(GainDb::new(trim).unwrap(), false, vec![], vec![]).unwrap(),
    )
}

fn request(target: &Target, edit: ScopedNodeEdit) -> ProjectRequest {
    ProjectRequest::Edit {
        expected_session: target.session,
        expected_revision: target.revision.clone(),
        scope: target.scope.clone(),
        cursor: target.cursor,
        edit: ProjectEdit::Scoped {
            target: target.clone(),
            edit,
        },
    }
}

fn counts(path: &Path) -> (i64, i64) {
    rusqlite::Connection::open(path.join("project.sqlite"))
        .unwrap()
        .query_row(
            "SELECT (SELECT count(*) FROM revisions), (SELECT count(*) FROM history)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap()
}

fn continued(workspace: &Workspace, receipt: &Commit) -> Target {
    Target {
        revision: workspace.document.revision_id().clone(),
        target: receipt.target.clone(),
        presentation: receipt.presentation.clone(),
        ..receipt.before.clone()
    }
}

fn assert_same_content(actual: &ProjectDocument, expected: &ProjectDocument) {
    let mut actual: serde_json::Value = serde_json::from_str(&actual.to_json().unwrap()).unwrap();
    let mut expected: serde_json::Value =
        serde_json::from_str(&expected.to_json().unwrap()).unwrap();
    actual.as_object_mut().unwrap().remove("revision_id");
    expected.as_object_mut().unwrap().remove("revision_id");
    assert_eq!(actual, expected);
}

#[test]
fn scoped_commit_maps_nested_plays_keeps_root_cursor_and_survives_undo_redo_reopen() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("scoped.deadpan");
    seed(&path);
    let service = ProjectService::new(Arc::new(|| {})).unwrap();
    let before = command(&service, ProjectRequest::Open(path.clone()))
        .workspace
        .unwrap();
    let mut target = capture(&before, play("outer-plays", 1), play("inner-plays", 1));
    target.presentation = Some(picture(1, 1));
    let rows = counts(&path);
    let bank = ProjectStore::open(&path, AccessMode::ReadOnly)
        .unwrap()
        .registers()
        .unwrap();
    let updated = command(
        &service,
        request(
            &target,
            ScopedNodeEdit::SetAudioTreatments {
                treatments: gain(-6000),
            },
        ),
    );
    assert!(updated.error.is_none(), "{:?}", updated.error);
    let commit = updated.committed.unwrap();
    assert!(commit.preserve_cursor);
    assert_eq!(commit.selected_node, Some(node("outer")));
    assert_eq!(commit.cursor, Some(target.cursor));
    assert_eq!(commit.scope, target.scope);
    let receipt = commit.scoped.unwrap();
    assert_eq!(receipt.before, target);
    assert_eq!(receipt.revision, commit.revision);
    assert_ne!(receipt.target.node, node("a"));
    assert_ne!(receipt.target.repeats[1].repeat, node("inner"));
    let after = updated.workspace.unwrap();
    assert_eq!(after.document.revision_id(), &receipt.revision);
    continued(&after, &receipt).validate(&after).unwrap();
    let mut default = capture(&after, RepeatEditBranch::Default, RepeatEditBranch::Default);
    default.presentation = Some(picture(1, 1));
    assert!(default.validate(&after).is_err());
    default.presentation = Some(picture(0, 1));
    default.validate(&after).unwrap();
    assert!(
        receipt
            .target
            .matches_instance(&after.document, receipt.presentation.as_ref().unwrap())
            .unwrap()
    );
    assert_eq!(
        after.document.nodes()[&receipt.target.node].audio_treatments,
        gain(-6000)
    );
    assert_eq!(
        after.document.nodes()[&node("a")],
        before.document.nodes()[&node("a")]
    );
    assert_eq!(after.plan.duration(), before.plan.duration());
    assert_eq!(counts(&path), (rows.0 + 1, rows.1 + 1));
    assert_eq!(
        ProjectStore::open(&path, AccessMode::ReadOnly)
            .unwrap()
            .registers()
            .unwrap(),
        bank
    );
    let unchanged = command(
        &service,
        request(
            &continued(&after, &receipt),
            ScopedNodeEdit::SetAudioTreatments {
                treatments: gain(-6000),
            },
        ),
    );
    assert!(unchanged.error.is_none());
    assert!(unchanged.committed.is_none());
    assert_eq!(counts(&path), (rows.0 + 1, rows.1 + 1));
    let undone = command(
        &service,
        ProjectRequest::Undo {
            expected_revision: receipt.revision,
        },
    )
    .workspace
    .unwrap();
    assert_same_content(&undone.document, &before.document);
    let redone = command(
        &service,
        ProjectRequest::Redo {
            expected_revision: undone.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    assert_same_content(&redone.document, &after.document);
    command(&service, ProjectRequest::Close);
    let reopened = command(&service, ProjectRequest::Open(path))
        .workspace
        .unwrap();
    assert_eq!(*reopened.document, *redone.document);
}

#[test]
fn scoped_default_and_play_capture_rejects_forged_context_before_noop_or_write() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("scoped-target.deadpan");
    seed(&path);
    let service = ProjectService::new(Arc::new(|| {})).unwrap();
    let before = command(&service, ProjectRequest::Open(path.clone()))
        .workspace
        .unwrap();
    let target = capture(&before, RepeatEditBranch::Default, play("inner-plays", 1));
    let rows = counts(&path);
    for case in 0..12 {
        let mut wrong = target.clone();
        match case {
            0 => wrong.session += 1,
            1 => wrong.project = ProjectId::new("wrong").unwrap(),
            2 => wrong.revision = RevisionId::new("stale").unwrap(),
            3 => wrong.root = node("b"),
            4 => wrong.target.node = node("b"),
            5 => wrong.target.repeats[1].branch = play("inner-plays", 9),
            6 => wrong.presentation = Some(picture(0, 0)),
            7 => {
                wrong.presentation = Some(InstancePath {
                    node: node("b"),
                    repeats: vec![],
                })
            }
            8 => wrong.scope = SequenceScope::test_path(vec![node("missing")]),
            _ => {}
        }
        let mut submitted = request(&wrong, ScopedNodeEdit::SetFraming { framing: None });
        if let ProjectRequest::Edit {
            expected_session,
            cursor,
            scope,
            ..
        } = &mut submitted
        {
            match case {
                9 => *cursor = ProjectFrame(13),
                10 => *expected_session += 1,
                11 => *scope = SequenceScope::test_path(vec![node("missing")]),
                _ => {}
            }
        }
        let refused = command(&service, submitted);
        assert!(refused.error.is_some(), "case {case}");
        assert!(refused.committed.is_none(), "case {case}");
        assert_eq!(*refused.workspace.unwrap().document, *before.document);
        assert_eq!(counts(&path), rows);
    }
    let unchanged = command(
        &service,
        request(&target, ScopedNodeEdit::SetFraming { framing: None }),
    );
    assert!(unchanged.error.is_none());
    assert!(unchanged.committed.is_none());
    assert_eq!(counts(&path), rows);
    assert_eq!(*unchanged.workspace.unwrap().document, *before.document);
}

#[test]
fn scoped_saved_refresh_failure_retains_remapped_receipt_and_blocks_stale_noop() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("scoped-refresh.deadpan");
    seed(&path);
    let service = ProjectService::new(Arc::new(|| {})).unwrap();
    let before = command(&service, ProjectRequest::Open(path.clone()))
        .workspace
        .unwrap();
    let target = capture(&before, RepeatEditBranch::Default, play("inner-plays", 1));
    service
        .shared
        .workspace_refresh_failure
        .store(true, Ordering::Release);
    let updated = command(
        &service,
        request(
            &target,
            ScopedNodeEdit::Rename {
                label: "Chosen inner play".into(),
            },
        ),
    );
    let error = updated.error.as_ref().unwrap();
    assert!(
        error.contains("Scoped edit saved") && error.contains("Reopen this project"),
        "{error}"
    );
    assert_eq!(
        *updated.workspace.as_ref().unwrap().document,
        *before.document
    );
    let receipt = updated.committed.unwrap().scoped.unwrap();
    assert_eq!(receipt.before, target);
    assert!(receipt.presentation.is_none());
    let saved = ProjectStore::open(&path, AccessMode::ReadOnly)
        .unwrap()
        .snapshot()
        .unwrap();
    assert_eq!(saved.revision_id(), &receipt.revision);
    assert_eq!(
        saved.nodes()[&receipt.target.node].label,
        "Chosen inner play"
    );
    assert_eq!(receipt.target.repeats[0].branch, RepeatEditBranch::Default);
    assert_eq!(receipt.target.repeats[1].repeat, node("inner"));
    let rows = counts(&path);
    let retry = command(
        &service,
        request(&target, ScopedNodeEdit::SetFraming { framing: None }),
    );
    assert!(retry.error.is_some());
    assert!(retry.committed.is_none());
    assert_eq!(counts(&path), rows);
    command(&service, ProjectRequest::Close);
    let reopened = command(&service, ProjectRequest::Open(path))
        .workspace
        .unwrap();
    assert_eq!(*reopened.document, saved);
    let undone = command(
        &service,
        ProjectRequest::Undo {
            expected_revision: receipt.revision,
        },
    )
    .workspace
    .unwrap();
    assert_same_content(&undone.document, &before.document);
}

#[test]
fn scoped_gain_proposals_preview_the_same_branches_and_commit_without_reusing_draft_ids() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("scoped-gain.deadpan");
    seed(&path);
    let service = ProjectService::new(Arc::new(|| {})).unwrap();
    let before = command(&service, ProjectRequest::Open(path.clone()))
        .workspace
        .unwrap();
    let scoped = capture(&before, RepeatEditBranch::Default, play("inner-plays", 1));
    let target = crate::project::gain::Target {
        session: before.session,
        project: scoped.project.clone(),
        revision: scoped.revision.clone(),
        scope: scoped.scope.clone(),
        node: scoped.target.node.clone(),
        cursor: scoped.cursor,
        entry: before.document.nodes()[&scoped.target.node]
            .audio_treatments
            .clone(),
        scoped: Some(scoped.clone()),
    };
    let rows = counts(&path);
    let unchanged = command(
        &service,
        ProjectRequest::PrepareGain(crate::project::gain::Proposal {
            target: target.clone(),
            draft: 31,
            change: 1,
            treatments: target.entry.clone(),
        }),
    );
    let unchanged = unchanged.gain.unwrap().result.unwrap();
    assert_ne!(
        unchanged.document.revision_id(),
        before.document.revision_id()
    );
    assert_same_content(&unchanged.document, &before.document);
    assert_eq!(counts(&path), rows);
    let proposed = command(
        &service,
        ProjectRequest::PrepareGain(crate::project::gain::Proposal {
            target: target.clone(),
            draft: 31,
            change: 2,
            treatments: gain(-4500),
        }),
    );
    assert!(proposed.error.is_none());
    assert!(proposed.committed.is_none());
    let preview = proposed.gain.unwrap().result.unwrap();
    assert_eq!(counts(&path), rows);
    assert_eq!(
        ProjectStore::open(&path, AccessMode::ReadOnly)
            .unwrap()
            .snapshot()
            .unwrap(),
        *before.document
    );
    let preview_node = preview.document.overrides()[&node("inner")]
        .get(&iteration("inner-plays", 1))
        .unwrap();
    assert_eq!(
        preview.document.nodes()[preview_node].audio_treatments,
        gain(-4500)
    );
    assert_eq!(
        preview.document.nodes()[&node("a")],
        before.document.nodes()[&node("a")]
    );
    assert_eq!(preview.document.duration().unwrap(), before.plan.duration());
    let updated = command(
        &service,
        edit_request_in(
            &before,
            target.scope.clone(),
            target.cursor,
            target.edit(gain(-4500)),
        ),
    );
    assert!(updated.error.is_none(), "{:?}", updated.error);
    let receipt = updated.committed.unwrap().scoped.unwrap();
    let after = updated.workspace.unwrap();
    assert_ne!(&receipt.target.node, preview_node);
    assert_ne!(after.document.revision_id(), preview.document.revision_id());
    assert_eq!(
        after.document.nodes()[&receipt.target.node],
        preview.document.nodes()[preview_node]
    );
    assert_eq!(
        after.document.nodes()[&node("a")],
        before.document.nodes()[&node("a")]
    );
    assert_eq!(counts(&path), (rows.0 + 1, rows.1 + 1));
    let mut mismatched = target.clone();
    mismatched.cursor = ProjectFrame(13);
    assert!(mismatched.validate(&before).is_err());
    let stale = command(
        &service,
        ProjectRequest::PrepareGain(crate::project::gain::Proposal {
            target,
            draft: 31,
            change: 3,
            treatments: gain(-3000),
        }),
    );
    assert!(stale.gain.unwrap().result.is_err());
    assert_eq!(*stale.workspace.unwrap().document, *after.document);
}
