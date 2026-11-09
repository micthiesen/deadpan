//! Injected sleep epochs stop tracking through the real service job, its
//! scripted worker seam and the revision-guarded save. Physical sleep and
//! native notification delivery are separate qualification.

use super::*;

fn start(fixture: &Fixture, workspace: &Workspace, ticket: u64) {
    let started = targets(
        &fixture.service,
        track(
            workspace,
            ticket,
            "target-1",
            TrackMode::Track {
                through_shots: true,
            },
        ),
    );
    assert_eq!(refusal(&started), None);
}

fn untracked(workspace: &Workspace) {
    let store = ProjectStore::open(&workspace.path, AccessMode::ReadOnly).unwrap();
    assert_eq!(
        store.head_revision().unwrap(),
        *workspace.document.revision_id(),
        "an interrupted run creates no history"
    );
    let head = store.snapshot().unwrap();
    let target = &head.targets()[&TargetId::new("target-1").unwrap()];
    assert!(target.samples.is_empty() && target.provenance.is_none());
}

#[test]
fn sleep_interrupts_tracking_without_saving_and_only_an_explicit_track_retries() {
    let fixture = project(vec![
        script(ScriptEnding::WaitForCancel, 2, 1),
        script(ScriptEnding::Moving { step: 0.002 }, 2, 1),
    ]);
    let workspace = save(&fixture, &fixture.workspace, "target-1", drawn(&fixture, 2))
        .workspace
        .unwrap();
    start(&fixture, &workspace, 2);
    job_until(&fixture.service, |job| {
        job.ticket == 2 && matches!(job.phase, crate::project::targets::Phase::Tracking(_))
    });
    // Coalesced: the writer may observe the pair only after wake.
    let lifecycle = fixture.service.jobs().lifecycle();
    lifecycle.will_sleep();
    lifecycle.did_wake();
    let (outcome, update) = concluded(&fixture.service, 2);
    assert_eq!(outcome, Outcome::Interrupted);
    assert!(
        update
            .message
            .as_deref()
            .is_some_and(|message| message.contains("slept") && message.contains(":track")),
        "{:?}",
        update.message
    );
    untracked(&workspace);
    assert!(
        !fixture
            .service
            .jobs()
            .snapshot()
            .iter()
            .any(|row| row.kind == crate::jobs::JobKind::Tracking),
        "the worker drained and released its row"
    );

    // Editing stays immediate, and wake alone never restarts tracking.
    let edited = command(
        &fixture.service,
        edit_request_in(
            &workspace,
            SequenceScope::default(),
            ProjectFrame(1),
            ProjectEdit::InsertTime {
                at: ProjectFrame(1),
                duration: FrameDuration::new(1).unwrap(),
            },
        ),
    );
    assert!(edited.error.is_none(), "{:?}", edited.error);
    let edited = edited.workspace.unwrap();
    let job = edited_job(&fixture);
    assert_eq!((job.ticket, job.outcome), (2, Some(Outcome::Interrupted)));

    start(&fixture, &edited, 3);
    let (outcome, update) = concluded(&fixture.service, 3);
    assert!(
        matches!(outcome, Outcome::Saved { samples, .. } if samples > 0),
        "{outcome:?}"
    );
    let saved = update.workspace.unwrap();
    let target = &saved.document.targets()[&TargetId::new("target-1").unwrap()];
    assert!(!target.samples.is_empty());
}

fn edited_job(fixture: &Fixture) -> Job {
    let update = command(
        &fixture.service,
        ProjectRequest::AcknowledgeRecovery {
            expected_session: fixture.workspace.session,
        },
    );
    update.targets.unwrap().job.unwrap()
}

#[test]
fn cancellation_requested_before_sleep_keeps_its_cancelled_outcome() {
    let fixture = project(vec![script(ScriptEnding::WaitForCancel, 2, 1)]);
    let workspace = save(&fixture, &fixture.workspace, "target-1", drawn(&fixture, 2))
        .workspace
        .unwrap();
    start(&fixture, &workspace, 2);
    job_until(&fixture.service, |job| {
        matches!(job.phase, crate::project::targets::Phase::Tracking(_))
    });
    let cancelled = targets(
        &fixture.service,
        Operation::Cancel {
            ticket: 3,
            session: workspace.session,
            job: 2,
        },
    );
    assert_eq!(refusal(&cancelled), None);
    let lifecycle = fixture.service.jobs().lifecycle();
    lifecycle.will_sleep();
    lifecycle.did_wake();
    let (outcome, _) = concluded(&fixture.service, 2);
    assert_eq!(outcome, Outcome::Cancelled);
    untracked(&workspace);
}

#[test]
fn tracking_started_while_asleep_stops_at_once_without_saving() {
    let fixture = project(vec![script(ScriptEnding::Moving { step: 0.002 }, 2, 1)]);
    let workspace = save(&fixture, &fixture.workspace, "target-1", drawn(&fixture, 2))
        .workspace
        .unwrap();
    fixture.service.jobs().lifecycle().will_sleep();
    start(&fixture, &workspace, 2);
    let (outcome, _) = concluded(&fixture.service, 2);
    assert_eq!(outcome, Outcome::Interrupted);
    untracked(&workspace);
}
