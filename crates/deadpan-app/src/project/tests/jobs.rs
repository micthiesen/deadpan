//! Several background job kinds at once, under the shared coordinator,
//! while edits are made (DP-18 concurrency coverage).
//!
//! AI pause generation and tracking are the real service jobs with their
//! scripted test seams (conditioning, allocation, durable records, range
//! resolution and revision-guarded saves stay real). Transcription, shot
//! detection and a seek proxy are stub jobs that register with the same
//! coordinator and honor its admission and yield rules exactly as the
//! production jobs do, without decoding: they stand in for the UI-side
//! threads, whose own contracts are covered by their own tests.

use std::sync::atomic::AtomicUsize;

use deadpan_core::{AttentionTarget, SourceSpan, SourceTimestamp, TargetId, TargetRegion};

use super::*;
use crate::jobs::{CancelOutcome, Foreground, JobKind, JobSpec, Jobs, RowState, Stopped};
use crate::project::generation::{
    Backend as AiBackend, GenerationOperation, Job as AiJob, Outcome as AiOutcome, Phase,
    Script as AiScript, ScriptEnding as AiEnding, ScriptQueue as AiQueue,
};
use crate::project::targets::{
    Backend as TrackBackend, Job as TrackJob, Operation, Outcome as TrackOutcome,
    Script as TrackScript, ScriptEnding as TrackEnding, ScriptQueue as TrackQueue, TrackMode,
};

struct Fixture {
    _scratch: tempfile::TempDir,
    service: ProjectService,
    workspace: Arc<Workspace>,
    hold: NodeId,
}

fn ai_waiting() -> AiScript {
    AiScript {
        unavailable: None,
        steps: 4,
        step_interval: Duration::from_millis(2),
        ending: AiEnding::WaitForCancel,
    }
}

fn track_script(ending: TrackEnding, steps: u8, interval: u64) -> TrackScript {
    TrackScript {
        unavailable: None,
        steps,
        step_interval: Duration::from_millis(interval),
        ending,
    }
}

/// The Original with a 12-frame pause at frame 10 and a drawn target.
fn project(ai: Vec<AiScript>, tracking: Vec<TrackScript>) -> Fixture {
    let scratch = tempfile::tempdir().unwrap();
    let service = ProjectService::start_with_tracking(
        Arc::new(|| {}),
        Some(ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap()),
        AiBackend::Scripted(Arc::new(AiQueue::new(ai))),
        TrackBackend::Scripted(Arc::new(TrackQueue::new(tracking))),
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
        }) && !service.is_busy()
    });
    let workspace = initialized.workspace.unwrap();
    let paused = command(
        &service,
        edit_request_in(
            &workspace,
            SequenceScope::default(),
            ProjectFrame(10),
            ProjectEdit::InsertTime {
                at: ProjectFrame(10),
                duration: FrameDuration::new(12).unwrap(),
            },
        ),
    );
    assert!(paused.error.is_none(), "{:?}", paused.error);
    let hold = paused.committed.unwrap().selected_node.unwrap();
    let workspace = paused.workspace.unwrap();
    let Some(SingleSourceState::Ready { asset, .. }) = &workspace.single_source else {
        panic!("original not initialized")
    };
    let video = workspace.document.assets()[asset].video.unwrap();
    let first = workspace.sources[asset]
        .video_index
        .as_ref()
        .unwrap()
        .frames()[2]
        .pts;
    let target = AttentionTarget {
        label: "Target 1".into(),
        asset: asset.clone(),
        span: SourceSpan::new(
            SourceTimestamp {
                ticks: first,
                time_base: video.start().time_base,
            },
            video.end(),
        )
        .unwrap(),
        region: TargetRegion {
            center: [400_000, 500_000],
            size: [200_000, 200_000],
        },
        samples: Vec::new(),
        corrections: Vec::new(),
        provenance: None,
    };
    let saved = command(
        &service,
        ProjectRequest::Target(Operation::Save {
            ticket: 1,
            session: workspace.session,
            revision: workspace.document.revision_id().clone(),
            id: TargetId::new("target-1").unwrap(),
            target: Box::new(target),
        }),
    );
    assert_eq!(saved.error, None);
    Fixture {
        _scratch: scratch,
        service,
        workspace: saved.workspace.unwrap(),
        hold,
    }
}

fn ai_job(update: &ProjectUpdate) -> Option<&AiJob> {
    update.generation.as_ref()?.job.as_ref()
}

fn track_job(update: &ProjectUpdate) -> Option<&TrackJob> {
    update.targets.as_ref()?.job.as_ref()
}

fn start_ai(fixture: &Fixture, workspace: &Workspace, ticket: u64) {
    let update = command(
        &fixture.service,
        ProjectRequest::Generation(GenerationOperation::Start {
            ticket,
            session: workspace.session,
            revision: workspace.document.revision_id().clone(),
            hold: fixture.hold.clone(),
            variants: 1,
        }),
    );
    assert_eq!(update.generation.unwrap().reply.unwrap().1, None);
}

fn start_tracking(fixture: &Fixture, workspace: &Workspace, ticket: u64) {
    let update = command(
        &fixture.service,
        ProjectRequest::Target(Operation::Track {
            ticket,
            session: workspace.session,
            revision: workspace.document.revision_id().clone(),
            id: TargetId::new("target-1").unwrap(),
            mode: TrackMode::Track {
                through_shots: true,
            },
        }),
    );
    assert_eq!(update.targets.unwrap().reply.unwrap().1, None);
}

/// One ordinary edit, committed through the writer; returns the saved
/// workspace. `command` times out (20 s) if anything holds the writer.
fn edit(service: &ProjectService, workspace: &Workspace) -> Arc<Workspace> {
    let update = command(
        service,
        edit_request_in(
            workspace,
            SequenceScope::default(),
            ProjectFrame(1),
            ProjectEdit::InsertTime {
                at: ProjectFrame(1),
                duration: FrameDuration::new(1).unwrap(),
            },
        ),
    );
    assert!(update.error.is_none(), "{:?}", update.error);
    update.workspace.unwrap()
}

fn state_of(board: &Jobs, kind: JobKind) -> Option<RowState> {
    board
        .snapshot()
        .into_iter()
        .find(|row| row.kind == kind)
        .map(|row| row.state)
}

fn until(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + TIMEOUT;
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// A stub of a coordinator-registered job: waits for admission, then
/// "works" in small steps with a checkpoint before each, until cancelled.
struct Stub {
    admitted: Arc<AtomicBool>,
    steps: Arc<AtomicUsize>,
    cancel: Arc<AtomicBool>,
    thread: std::thread::JoinHandle<Result<(), Stopped>>,
}

fn stub(board: &Jobs, kind: JobKind, session: u64) -> Stub {
    let cancel = Arc::new(AtomicBool::new(false));
    let spec = JobSpec::new(kind, Some(session)).cancel_flag(&cancel);
    // Production transcription runs its CPU phase without the model slot
    // and claims it only for whisper; this stub claims it at once.
    let deferred = kind == JobKind::Transcription;
    let handle = board.register(if deferred { spec.deferred() } else { spec });
    let admitted = Arc::new(AtomicBool::new(false));
    let steps = Arc::new(AtomicUsize::new(0));
    let thread = {
        let (admitted, steps, cancel) = (admitted.clone(), steps.clone(), cancel.clone());
        std::thread::spawn(move || {
            if deferred {
                handle.acquire(Some(&cancel))?;
            } else {
                handle.wait_admitted(Some(&cancel))?;
            }
            admitted.store(true, Ordering::Release);
            loop {
                if kind == JobKind::Proxy {
                    // Proxies suspend their worker process from a monitor.
                    if handle.pause_reason().is_none() {
                        steps.fetch_add(1, Ordering::AcqRel);
                    }
                    if cancel.load(Ordering::Acquire) {
                        return Err(Stopped::Cancelled);
                    }
                } else {
                    handle.checkpoint(Some(&cancel))?;
                    steps.fetch_add(1, Ordering::AcqRel);
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        })
    };
    Stub {
        admitted,
        steps,
        cancel,
        thread,
    }
}

fn join(stub: Stub) -> Result<(), Stopped> {
    let deadline = Instant::now() + Duration::from_secs(2);
    while !stub.thread.is_finished() {
        assert!(Instant::now() < deadline, "a stub job did not drain");
        std::thread::sleep(Duration::from_millis(2));
    }
    stub.thread.join().unwrap()
}

/// Edits commit through the writer while generation, tracking, a queued
/// transcription, a yielding shot scan and a suspended proxy are all live
/// (no job holds the writer or blocks on it);
/// priorities and yield rules hold; cancellation drains each job; a stale
/// tracking result is refused; shutdown releases everything.
#[test]
fn edits_stay_immediate_while_every_kind_of_job_runs_and_cancellation_drains() {
    let fixture = project(
        vec![ai_waiting()],
        vec![
            track_script(TrackEnding::WaitForCancel, 2, 2),
            // A slow run that finishes after edits: its result is stale.
            track_script(TrackEnding::Moving { step: 0.001 }, 30, 15),
        ],
    );
    let board = fixture.service.jobs().clone();
    let session = fixture.workspace.session;
    let mut workspace = fixture.workspace.clone();

    // Generation holds the one inference slot once its worker starts.
    start_ai(&fixture, &workspace, 2);
    let running = wait(&fixture.service, |update| {
        ai_job(update).is_some_and(|job| matches!(job.phase, Phase::Step { .. }))
    });
    assert_eq!(state_of(&board, JobKind::AiPause), Some(RowState::Running));
    let request = ai_job(&running).unwrap().request.clone().unwrap();

    // An automatic transcription queues behind it rather than being refused.
    let transcription = stub(&board, JobKind::Transcription, session);
    let shots = stub(&board, JobKind::Shots, session);
    let proxy = stub(&board, JobKind::Proxy, session);
    until("the shot scan to start", || {
        shots.steps.load(Ordering::Acquire) > 2
    });
    assert_eq!(
        state_of(&board, JobKind::Transcription),
        Some(RowState::Queued {
            position: 1,
            waiting_for: "AI pause pictures".into()
        })
    );
    assert!(!transcription.admitted.load(Ordering::Acquire));
    until("the proxy to yield to the AI model", || {
        state_of(&board, JobKind::Proxy) == Some(RowState::Paused("an AI model is running".into()))
    });

    // Tracking (requested work) is never queued; the scan yields to it.
    start_tracking(&fixture, &workspace, 3);
    // Tracking binds its revision once its pictures are prepared; edit after.
    wait(&fixture.service, |update| {
        track_job(update).is_some_and(|job| {
            job.ticket == 3 && matches!(job.phase, crate::project::targets::Phase::Tracking(_))
        })
    });
    assert_eq!(state_of(&board, JobKind::Tracking), Some(RowState::Running));
    until("the shot scan to yield to tracking", || {
        state_of(&board, JobKind::Shots) == Some(RowState::Paused("tracking is running".into()))
    });
    let paused_at = shots.steps.load(Ordering::Acquire);

    // Edits commit while all five jobs are live: the model run never
    // ends and the scan stays paused, so a job that held or waited on the
    // writer would time this out.
    for _ in 0..6 {
        workspace = edit(&fixture.service, &workspace);
    }
    for kind in [JobKind::AiPause, JobKind::Tracking] {
        assert_eq!(state_of(&board, kind), Some(RowState::Running), "{kind:?}");
    }
    assert!(!transcription.admitted.load(Ordering::Acquire));
    assert_eq!(
        shots.steps.load(Ordering::Acquire),
        paused_at,
        "the scan stayed paused"
    );

    // Cancel tracking from the coordinator: its own flag stops the worker.
    let tracking_id = board
        .snapshot()
        .into_iter()
        .find(|row| row.kind == JobKind::Tracking)
        .unwrap()
        .id;
    assert_eq!(
        board.cancel(tracking_id),
        CancelOutcome::Signalled(JobKind::Tracking, Some(session))
    );
    let cancelled = wait(&fixture.service, |update| {
        track_job(update).is_some_and(|job| job.ticket == 3 && !job.running())
    });
    assert_eq!(
        track_job(&cancelled).unwrap().outcome,
        Some(TrackOutcome::Cancelled)
    );
    until("the scan to resume after tracking", || {
        shots.steps.load(Ordering::Acquire) > paused_at
    });

    // The foreground's playback pauses the scan at its next boundary.
    board.set_foreground(Foreground { playback: true });
    until("the scan to yield to playback", || {
        state_of(&board, JobKind::Shots) == Some(RowState::Paused("the edit is playing".into()))
    });
    board.set_foreground(Foreground::default());

    // Cancelling generation drains it durably and admits the transcription.
    let ai_id = board
        .snapshot()
        .into_iter()
        .find(|row| row.kind == JobKind::AiPause)
        .unwrap()
        .id;
    board.cancel(ai_id);
    let concluded = wait(&fixture.service, |update| {
        ai_job(update).is_some_and(|job| !job.running())
    });
    assert_eq!(
        ai_job(&concluded).unwrap().outcome,
        Some(AiOutcome::Cancelled)
    );
    let attempts = ProjectStore::open(&workspace.path, AccessMode::ReadOnly)
        .unwrap()
        .generation_attempts(&request, 0, 4)
        .unwrap();
    assert_eq!(attempts.len(), 1);
    assert_eq!(
        attempts[0].checkpoint.state,
        deadpan_jobs::JobState::Cancelled
    );
    until("the transcription to be admitted", || {
        transcription.admitted.load(Ordering::Acquire)
    });
    // Whisper is a model too: the proxy keeps yielding until it ends.
    assert_eq!(
        state_of(&board, JobKind::Proxy),
        Some(RowState::Paused("an AI model is running".into()))
    );
    let transcription_id = board
        .snapshot()
        .into_iter()
        .find(|row| row.kind == JobKind::Transcription)
        .unwrap()
        .id;
    board.cancel(transcription_id);
    until("the proxy to resume", || {
        state_of(&board, JobKind::Proxy) == Some(RowState::Running)
    });
    let resumed_at = proxy.steps.load(Ordering::Acquire);
    until("the proxy to make progress", || {
        proxy.steps.load(Ordering::Acquire) > resumed_at
    });

    // A tracking result for a revision that edits replaced is refused.
    start_tracking(&fixture, &workspace, 4);
    wait(&fixture.service, |update| {
        track_job(update).is_some_and(|job| {
            job.ticket == 4 && matches!(job.phase, crate::project::targets::Phase::Tracking(_))
        })
    });
    let edited = edit(&fixture.service, &workspace);
    let stale = wait(&fixture.service, |update| {
        track_job(update).is_some_and(|job| job.ticket == 4 && !job.running())
    });
    let outcome = track_job(&stale).unwrap().outcome.clone();
    assert!(
        matches!(&outcome, Some(TrackOutcome::Failed(reason)) if reason.contains("changed")),
        "{outcome:?}"
    );
    let target = &stale
        .workspace
        .as_ref()
        .unwrap_or(&edited)
        .document
        .targets()[&TargetId::new("target-1").unwrap()];
    assert!(target.samples.is_empty(), "nothing stale was saved");

    // Shutdown releases every waiter; each job drains.
    assert_eq!(join(transcription), Err(Stopped::Cancelled));
    let stray = stub(&board, JobKind::Transcription, session);
    board.shutdown();
    assert_eq!(join(stray), Err(Stopped::ShuttingDown));
    for stub in [shots, proxy] {
        assert!(stub.cancel.load(Ordering::Acquire));
        assert!(join(stub).is_err());
    }
    until("the coordinator to empty", || board.is_empty());
    fixture.service.shutdown();
    until("the service to shut down", || {
        fixture.service.is_shutdown_complete()
    });
}

/// Generation queued behind another model records its attempt, shows the
/// queued phase, never blocks edits and is durably cancelled while queued
/// without starting a worker.
#[test]
fn generation_queues_for_the_model_and_cancels_durably_while_queued() {
    let fixture = project(vec![ai_waiting()], vec![]);
    let board = fixture.service.jobs().clone();
    let session = fixture.workspace.session;
    // Another model (a transcription) holds the inference slot.
    let holder = stub(&board, JobKind::Transcription, session);
    until("the holder to be admitted", || {
        holder.admitted.load(Ordering::Acquire)
    });
    start_ai(&fixture, &fixture.workspace, 2);
    let queued = wait(&fixture.service, |update| {
        ai_job(update).is_some_and(|job| job.phase == Phase::Queued)
    });
    let job = ai_job(&queued).unwrap();
    assert_eq!(job.phase.label(), "Queued for the AI model");
    let request = job.request.clone().expect("the attempt is recorded first");
    assert_eq!(
        state_of(&board, JobKind::AiPause),
        Some(RowState::Queued {
            position: 1,
            waiting_for: "Transcription".into()
        })
    );
    // Edits proceed while the job waits; the recorded request stays current.
    let mut workspace = fixture.workspace.clone();
    for _ in 0..3 {
        workspace = edit(&fixture.service, &workspace);
    }
    let ai_id = board
        .snapshot()
        .into_iter()
        .find(|row| row.kind == JobKind::AiPause)
        .unwrap()
        .id;
    board.cancel(ai_id);
    let concluded = wait(&fixture.service, |update| {
        ai_job(update).is_some_and(|job| !job.running())
    });
    assert_eq!(
        ai_job(&concluded).unwrap().outcome,
        Some(AiOutcome::Cancelled)
    );
    let attempts = ProjectStore::open(&workspace.path, AccessMode::ReadOnly)
        .unwrap()
        .generation_attempts(&request, 0, 4)
        .unwrap();
    assert_eq!(
        attempts[0].checkpoint.state,
        deadpan_jobs::JobState::Cancelled
    );
    // The holder kept its slot throughout.
    assert_eq!(
        state_of(&board, JobKind::Transcription),
        Some(RowState::Running)
    );
    holder.cancel.store(true, Ordering::Release);
    assert!(join(holder).is_err());
    until("the coordinator to empty", || board.is_empty());
}

/// An attempt interrupted by a crash is offered after reopening; Discard
/// removes it durably and a retry supersedes it.
#[test]
fn interrupted_attempts_are_listed_after_reopen_and_can_be_discarded() {
    let fixture = project(vec![ai_waiting()], vec![]);
    let path = fixture.workspace.path.clone();
    let hold = fixture.hold.clone();
    // Simulate a crash: record a running attempt, then reopen writable.
    start_ai(&fixture, &fixture.workspace, 2);
    let running = wait(&fixture.service, |update| {
        ai_job(update).is_some_and(|job| matches!(job.phase, Phase::Step { .. }))
    });
    let request = ai_job(&running).unwrap().request.clone().unwrap();
    let copy = fixture._scratch.path().join("crashed.deadpan");
    // A backup-API copy taken while the attempt is live is what a crash
    // leaves: a nonterminal attempt and the writer's session marker.
    crate::jobs::crash_copy(&path, &copy).unwrap();
    let opened = command(&fixture.service, ProjectRequest::Open(copy.clone()));
    assert_eq!(opened.error, None);
    let has_list = |update: &ProjectUpdate| {
        update
            .generation
            .as_ref()
            .is_some_and(|generation| !generation.interrupted.is_empty())
    };
    let listed = if has_list(&opened) {
        opened
    } else {
        wait(&fixture.service, has_list)
    };
    let generation = listed.generation.unwrap();
    let interrupted = generation.interrupted[0].clone();
    assert_eq!(interrupted.request, request.as_str());
    assert_eq!(interrupted.hold, hold.as_str());
    assert!(
        interrupted.pause.is_some(),
        "the pause is still in the edit"
    );
    let session = listed.workspace.unwrap().session;
    let dismissed = command(
        &fixture.service,
        ProjectRequest::Generation(GenerationOperation::DismissInterrupted {
            ticket: 9,
            session,
            request: interrupted.request.clone(),
            attempt: interrupted.attempt.clone(),
        }),
    );
    let generation = dismissed.generation.unwrap();
    assert_eq!(generation.reply, Some((9, None)));
    assert!(generation.interrupted.is_empty());
    assert!(
        ProjectStore::open(&copy, AccessMode::ReadOnly)
            .unwrap()
            .interrupted_generation_attempts()
            .unwrap()
            .attempts
            .is_empty()
    );
}
