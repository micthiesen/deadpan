//! Injected sleep epochs exercise the real writer and attempt lifecycle.
//! Physical sleep and native notification delivery are separate qualification.

use super::*;

fn begin(fixture: &Fixture, ticket: u64, queued: bool) -> ProjectUpdate {
    let started = command(
        &fixture.service,
        ProjectRequest::Generation(GenerationOperation::Start {
            ticket,
            session: fixture.workspace.session,
            revision: fixture.workspace.document.revision_id().clone(),
            hold: fixture.hold.clone(),
            authoring: None,
            variants: 1,
            options: None,
        }),
    );
    assert_eq!(
        started
            .generation
            .as_ref()
            .unwrap()
            .reply
            .as_ref()
            .unwrap()
            .1,
        None
    );
    let ready = |update: &ProjectUpdate| {
        ai_job(update).is_some_and(|job| {
            job.ticket == ticket
                && if queued {
                    job.phase == Phase::Queued
                } else {
                    matches!(job.phase, Phase::Step { .. })
                }
        })
    };
    if ready(&started) {
        started
    } else {
        wait(&fixture.service, ready)
    }
}

#[test]
fn sleep_interrupts_running_generation_and_explicit_retry_creates_a_new_attempt() {
    interrupted_generation(false);
}

#[test]
fn coalesced_sleep_and_wake_interrupt_queued_generation_without_resuming_it() {
    interrupted_generation(true);
}

#[test]
fn user_cancellation_before_sleep_remains_cancelled_without_retry() {
    let fixture = project(vec![ai_waiting()], vec![]);
    let active = begin(&fixture, 2, false);
    let request = ai_job(&active).unwrap().request.clone().unwrap();
    let cancelled = command(
        &fixture.service,
        ProjectRequest::Generation(GenerationOperation::Cancel {
            ticket: 3,
            session: fixture.workspace.session,
            job: 2,
        }),
    );
    let lifecycle = fixture.service.jobs().lifecycle();
    lifecycle.will_sleep();
    lifecycle.did_wake();
    let stopped = if ai_job(&cancelled).is_some_and(|job| !job.running()) {
        cancelled
    } else {
        wait(&fixture.service, |update| {
            ai_job(update).is_some_and(|job| !job.running())
        })
    };
    assert_eq!(
        ai_job(&stopped).unwrap().outcome,
        Some(AiOutcome::Cancelled)
    );
    let stored = ProjectStore::open(&fixture.workspace.path, AccessMode::ReadOnly).unwrap();
    let attempts = stored.generation_attempts(&request, 0, 4).unwrap();
    assert_eq!(
        attempts[0].checkpoint.state,
        deadpan_jobs::JobState::Cancelled
    );
    assert!(
        stored
            .interrupted_generation_attempts()
            .unwrap()
            .attempts
            .is_empty()
    );
}

fn interrupted_generation(queued: bool) {
    let fixture = project(vec![ai_waiting(), ai_waiting()], vec![]);
    let board = fixture.service.jobs().clone();
    let holder = queued.then(|| stub(&board, JobKind::Transcription, fixture.workspace.session));
    if let Some(holder) = &holder {
        until("the inference holder", || {
            holder.admitted.load(Ordering::Acquire)
        });
    }
    let active = begin(&fixture, 2, queued);
    let request = ai_job(&active).unwrap().request.clone().unwrap();
    let lifecycle = board.lifecycle();
    lifecycle.will_sleep();
    if queued {
        // Even when the writer misses the sleeping state, the epoch retains it.
        lifecycle.did_wake();
    }
    let stopped = wait(&fixture.service, |update| {
        ai_job(update).is_some_and(|job| !job.running())
    });
    assert!(matches!(
        ai_job(&stopped).unwrap().outcome,
        Some(AiOutcome::Failed(_))
    ));
    let stored = ProjectStore::open(&fixture.workspace.path, AccessMode::ReadOnly).unwrap();
    let attempts = stored.generation_attempts(&request, 0, 4).unwrap();
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0].checkpoint.state, deadpan_jobs::JobState::Failed);
    assert!(matches!(
        attempts[0].checkpoint.failure.as_ref(),
        Some(deadpan_jobs::JobFailure::Host(failure))
            if failure.code == deadpan_jobs::HostFailureCode::Interrupted
    ));
    let interrupted = stored.interrupted_generation_attempts().unwrap();
    assert_eq!(interrupted.attempts.len(), 1);
    let first_attempt = interrupted.attempts[0].attempt_id.clone();
    // No AI result or provider acceptance is an authored edit.
    assert_eq!(
        stored.head_revision().unwrap(),
        *fixture.workspace.document.revision_id()
    );
    drop(stored);
    if !queued {
        let refused = command(
            &fixture.service,
            ProjectRequest::Generation(GenerationOperation::Start {
                ticket: 3,
                session: fixture.workspace.session,
                revision: fixture.workspace.document.revision_id().clone(),
                hold: fixture.hold.clone(),
                authoring: None,
                variants: 1,
                options: None,
            }),
        );
        assert!(refused.generation.unwrap().reply.unwrap().1.is_some());
        lifecycle.did_wake();
    }
    if let Some(holder) = holder {
        holder.cancel.store(true, Ordering::Release);
        assert!(join(holder).is_err());
    }
    // A writer round trip after wake cannot create an implicit retry.
    let refreshed = command(
        &fixture.service,
        ProjectRequest::AcknowledgeRecovery {
            expected_session: fixture.workspace.session,
        },
    );
    assert!(refreshed.error.is_none());
    let stored = ProjectStore::open(&fixture.workspace.path, AccessMode::ReadOnly).unwrap();
    assert_eq!(stored.generation_attempts(&request, 0, 4).unwrap().len(), 1);
    drop(stored);
    let retried = begin(&fixture, 4, false);
    assert_eq!(ai_job(&retried).unwrap().request.as_ref(), Some(&request));
    let stored = ProjectStore::open(&fixture.workspace.path, AccessMode::ReadOnly).unwrap();
    let attempts = stored.generation_attempts(&request, 0, 4).unwrap();
    assert_eq!(attempts.len(), 2);
    assert_ne!(
        attempts[1].checkpoint.identity.attempt_id.as_str(),
        first_attempt
    );
    drop(stored);
    let cancelled = command(
        &fixture.service,
        ProjectRequest::Generation(GenerationOperation::Cancel {
            ticket: 5,
            session: fixture.workspace.session,
            job: 4,
        }),
    );
    if !ai_job(&cancelled).is_some_and(|job| !job.running()) {
        wait(&fixture.service, |update| {
            ai_job(update).is_some_and(|job| !job.running())
        });
    }
}
