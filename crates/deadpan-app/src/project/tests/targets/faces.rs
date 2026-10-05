//! Face proposals through the native project service. The scripted face
//! seam keeps picture resolution against the qualified index, the verified
//! Original copy and the host's face admission real; it replaces only the
//! Vision worker's reported faces.

use deadpan_analysis::NormalizedRect;
use deadpan_cli::faces::DetectedFace;
use deadpan_core::{ExactRatio, Framing, FramingClock, FramingPose, FramingValue};

use super::*;
use crate::project::targets::{FaceJob, FaceOutcome, FaceRun, FaceScript};

fn face(x: f64, y: f64) -> DetectedFace {
    DetectedFace {
        region: NormalizedRect::new(x, y, 0.2, 0.3).unwrap(),
        confidence: 0.8,
    }
}

fn scripted(runs: Vec<FaceRun>) -> Fixture {
    project_with(Backend::ScriptedFaces(Arc::new(FaceScript::new(runs))))
}

fn found(faces: Vec<DetectedFace>) -> FaceRun {
    FaceRun::Faces {
        faces,
        delay: Duration::ZERO,
    }
}

fn detect(fixture: &Fixture, workspace: &Workspace, ticket: u64, pts: i64) -> ProjectUpdate {
    targets(
        &fixture.service,
        Operation::DetectFaces {
            ticket,
            session: workspace.session,
            revision: workspace.document.revision_id().clone(),
            asset: fixture.asset.clone(),
            pts,
        },
    )
}

fn faces_done(service: &ProjectService, ticket: u64) -> (FaceJob, ProjectUpdate) {
    let update = wait(service, |update| {
        update
            .targets
            .as_ref()
            .and_then(|targets| targets.faces.as_ref())
            .is_some_and(|job| job.ticket == ticket && job.outcome.is_some())
            && !service.is_busy()
    });
    let job = update.targets.as_ref().unwrap().faces.clone().unwrap();
    (job, update)
}

fn save_framed(
    fixture: &Fixture,
    workspace: &Workspace,
    ticket: u64,
    target: AttentionTarget,
    framing: Option<Framing>,
) -> ProjectUpdate {
    let node = workspace
        .document
        .children(workspace.document.root())
        .next()
        .unwrap()
        .clone();
    targets(
        &fixture.service,
        Operation::SaveFramed {
            ticket,
            session: workspace.session,
            revision: workspace.document.revision_id().clone(),
            scope: SequenceScope::default(),
            cursor: ProjectFrame(2),
            node,
            id: TargetId::new("face-1").unwrap(),
            target: Box::new(target),
            framing: framing.map(Box::new),
        },
    )
}

fn follow() -> Framing {
    Framing {
        clock: FramingClock::OwnerOutput,
        value: FramingValue::Follow {
            target: TargetId::new("face-1").unwrap(),
            scale: ExactRatio::new(27, 20).unwrap(),
            fallback: FramingPose {
                scale: ExactRatio::new(27, 20).unwrap(),
                ..Default::default()
            },
        },
    }
}

#[test]
fn proposals_never_edit_and_a_chosen_face_saves_its_target_and_framing_as_one_undo() {
    let fixture = scripted(vec![found(vec![face(0.1, 0.2), face(0.6, 0.1)])]);
    let before = fixture.workspace.clone();
    let pts = fixture.pictures[2];
    assert_eq!(refusal(&detect(&fixture, &before, 1, pts)), None);
    let (job, update) = faces_done(&fixture.service, 1);
    assert_eq!(
        (job.pts, &job.revision),
        (pts, before.document.revision_id())
    );
    let Some(FaceOutcome::Found(faces)) = job.outcome else {
        panic!("expected faces, got {:?}", job.outcome)
    };
    assert_eq!(faces.len(), 2);
    // Detection is a proposal: no revision, no target, no selection.
    let head = update.workspace.unwrap_or_else(|| before.clone());
    assert_eq!(head.document.revision_id(), before.document.revision_id());
    assert!(head.document.targets().is_empty());

    let span_end = before.document.assets()[&fixture.asset]
        .video
        .unwrap()
        .end();
    let target = deadpan_cli::faces::face_target(
        &faces[1],
        "Face 2".into(),
        fixture.asset.clone(),
        span_end.time_base,
        pts,
        span_end.ticks,
    )
    .unwrap();
    let saved = save_framed(&fixture, &before, 2, target.clone(), Some(follow()));
    assert_eq!(refusal(&saved), None);
    let committed = saved.committed.clone().unwrap();
    assert!(committed.preserve_cursor);
    assert_eq!(committed.cursor, Some(ProjectFrame(2)));
    let workspace = saved.workspace.unwrap();
    let id = TargetId::new("face-1").unwrap();
    assert_eq!(workspace.document.targets()[&id], target);
    let node = workspace
        .document
        .children(workspace.document.root())
        .next()
        .unwrap();
    assert_eq!(workspace.document.nodes()[node].framing, Some(follow()));
    assert!(
        saved
            .message
            .as_deref()
            .is_some_and(|message| message.contains("following Face 2")
                && message.contains("one Undo removes both")),
        "{:?}",
        saved.message
    );
    // The previous head is now stale: the same command refuses.
    let again = save_framed(&fixture, &before, 3, target, Some(follow()));
    assert!(refusal(&again).is_some());

    // One Undo removes both the target and the framing.
    let undone = command(
        &fixture.service,
        ProjectRequest::Undo {
            expected_revision: workspace.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    assert!(undone.document.targets().is_empty());
    assert_eq!(undone.document.nodes(), before.document.nodes());
}

#[test]
fn stale_unindexed_and_unordered_detections_propose_nothing() {
    let fixture = scripted(vec![
        found(vec![face(0.6, 0.1), face(0.1, 0.2)]),
        found(Vec::new()),
    ]);
    let before = fixture.workspace.clone();
    // Unordered worker output is refused by the host's admission.
    assert_eq!(
        refusal(&detect(&fixture, &before, 1, fixture.pictures[1])),
        None
    );
    let (job, _) = faces_done(&fixture.service, 1);
    assert!(
        matches!(&job.outcome, Some(FaceOutcome::Failed(reason)) if reason.contains("ordered")),
        "{:?}",
        job.outcome
    );
    // A PTS between pictures is not the displayed picture's indexed PTS.
    assert_eq!(
        refusal(&detect(&fixture, &before, 2, fixture.pictures[1] + 1)),
        None
    );
    let (job, _) = faces_done(&fixture.service, 2);
    assert!(
        matches!(&job.outcome, Some(FaceOutcome::Failed(reason)) if reason.contains("indexed")),
        "{:?}",
        job.outcome
    );
    // No faces is a result, not a failure.
    assert_eq!(
        refusal(&detect(&fixture, &before, 3, fixture.pictures[0])),
        None
    );
    let (job, _) = faces_done(&fixture.service, 3);
    assert_eq!(job.outcome, Some(FaceOutcome::Found(Vec::new())));

    // A detection entered against an older head is refused before it starts.
    let saved = save(&fixture, &before, "target-1", drawn(&fixture, 2));
    assert!(saved.workspace.is_some());
    assert!(refusal(&detect(&fixture, &before, 4, fixture.pictures[0])).is_some());
}

#[test]
fn closing_cancels_a_running_detection_and_drains_it() {
    let fixture = scripted(vec![FaceRun::Faces {
        faces: vec![face(0.1, 0.2)],
        delay: Duration::from_secs(30),
    }]);
    let before = fixture.workspace.clone();
    assert_eq!(
        refusal(&detect(&fixture, &before, 1, fixture.pictures[0])),
        None
    );
    // A second detection is refused while the first runs.
    assert!(refusal(&detect(&fixture, &before, 2, fixture.pictures[0])).is_some());
    let started = std::time::Instant::now();
    let closed = command(&fixture.service, ProjectRequest::Close);
    assert!(closed.workspace.is_none());
    assert!(started.elapsed() < Duration::from_secs(10));
}
