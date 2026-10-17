//! Several AI generations per project, admission conditions (power, heat,
//! memory, playback) and a killed worker process while other jobs and
//! edits run (DP-18).

use std::sync::atomic::AtomicU32;

use super::*;
use crate::jobs::{Conditions, MemoryPressure, Waiting};

type AiUpdate = crate::project::generation::Update;

/// Commands, edits and waits that never lose a generation state: a
/// command's own reply may already carry the state a later wait looks for.
struct Observer<'a> {
    service: &'a ProjectService,
    last: Option<AiUpdate>,
    tracking: Option<crate::project::targets::Update>,
}

impl<'a> Observer<'a> {
    fn new(service: &'a ProjectService) -> Self {
        Self {
            service,
            last: None,
            tracking: None,
        }
    }

    fn seen(&mut self, update: &ProjectUpdate) {
        if let Some(generation) = &update.generation {
            self.last = Some(generation.clone());
        }
        if let Some(targets) = &update.targets {
            self.tracking = Some(targets.clone());
        }
    }

    /// Start tracking and wait until it has bound its revision (it then
    /// tracks until cancelled; an edit before binding would stale it).
    fn track(&mut self, workspace: &Workspace, ticket: u64) {
        let reply = self.command(ProjectRequest::Target(Operation::Track {
            ticket,
            session: workspace.session,
            revision: workspace.document.revision_id().clone(),
            id: TargetId::new("target-1").unwrap(),
            mode: TrackMode::Track {
                through_shots: true,
            },
        }));
        assert_eq!(reply.targets.unwrap().reply.unwrap().1, None);
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let tracking = self
                .tracking
                .as_ref()
                .and_then(|targets| targets.job.as_ref());
            if tracking.is_some_and(|job| {
                job.ticket == ticket
                    && matches!(job.phase, crate::project::targets::Phase::Tracking(_))
            }) {
                return;
            }
            if let Some(update) = self.service.take_update() {
                self.seen(&update);
                continue;
            }
            assert!(Instant::now() < deadline, "timed out waiting for tracking");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn command(&mut self, request: ProjectRequest) -> ProjectUpdate {
        let update = command(self.service, request);
        self.seen(&update);
        update
    }

    /// One ordinary edit; `command` times out if anything holds the writer.
    fn edit(&mut self, workspace: &Workspace) -> Arc<Workspace> {
        let update = self.command(edit_request_in(
            workspace,
            SequenceScope::default(),
            ProjectFrame(1),
            ProjectEdit::InsertTime {
                at: ProjectFrame(1),
                duration: FrameDuration::new(1).unwrap(),
            },
        ));
        assert!(update.error.is_none(), "{:?}", update.error);
        update.workspace.unwrap()
    }

    fn until(&mut self, what: &str, predicate: impl Fn(&AiUpdate) -> bool) -> AiUpdate {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            if let Some(last) = self.last.as_ref().filter(|last| predicate(last)) {
                return last.clone();
            }
            if let Some(update) = self.service.take_update() {
                self.seen(&update);
                continue;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {what}: {:?}",
                self.last.as_ref().map(|last| last
                    .job
                    .iter()
                    .chain(&last.others)
                    .map(|job| (
                        job.ticket,
                        job.phase.clone(),
                        job.outcome.clone(),
                        job.note.clone()
                    ))
                    .collect::<Vec<_>>())
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// Start generation for `hold`; the service's refusal, if any.
    fn start(&mut self, workspace: &Workspace, hold: &NodeId, ticket: u64) -> Option<String> {
        self.command(ProjectRequest::Generation(GenerationOperation::Start {
            ticket,
            session: workspace.session,
            revision: workspace.document.revision_id().clone(),
            hold: hold.clone(),
            authoring: None,
            variants: 1,
            options: None,
        }))
        .generation
        .unwrap()
        .reply
        .unwrap()
        .1
    }

    /// The Jobs panel's Cancel for an AI row: its flag, then the request
    /// for exactly its ticket.
    fn cancel_from_panel(&mut self, board: &Jobs, session: u64, ticket: u64) {
        let row = row_of(board, ticket);
        assert!(matches!(
            board.cancel(row.id),
            CancelOutcome::Signalled(JobKind::AiPause, _)
        ));
        let reply = self
            .command(ProjectRequest::Generation(GenerationOperation::Cancel {
                ticket: 0,
                session,
                job: ticket,
            }))
            .generation
            .unwrap()
            .reply
            .unwrap()
            .1;
        // A queued job may already have concluded from its flag alone.
        assert!(
            reply.is_none() || reply.as_deref() == Some("No matching AI pause is generating."),
            "{reply:?}"
        );
    }

    fn concluded(&mut self, ticket: u64) -> AiJob {
        let update = self.until("the job to conclude", |update| {
            job_of(update, ticket).is_some_and(|job| !job.running())
        });
        job_of(&update, ticket).unwrap().clone()
    }

    fn phase(&mut self, ticket: u64, what: &str, predicate: impl Fn(&AiJob) -> bool) -> AiJob {
        let update = self.until(what, |update| {
            job_of(update, ticket).is_some_and(&predicate)
        });
        job_of(&update, ticket).unwrap().clone()
    }
}

/// A second 12-frame pause at frame 40; returns it and the saved workspace.
fn second_pause(observer: &mut Observer<'_>, fixture: &Fixture) -> (NodeId, Arc<Workspace>) {
    let paused = observer.command(edit_request_in(
        &fixture.workspace,
        SequenceScope::default(),
        ProjectFrame(40),
        ProjectEdit::InsertTime {
            at: ProjectFrame(40),
            duration: FrameDuration::new(12).unwrap(),
        },
    ));
    assert!(paused.error.is_none(), "{:?}", paused.error);
    let hold = paused.committed.unwrap().selected_node.unwrap();
    assert_ne!(hold, fixture.hold);
    (hold, paused.workspace.unwrap())
}

/// The job with `ticket`, presented or further.
fn job_of(update: &AiUpdate, ticket: u64) -> Option<&AiJob> {
    update
        .job
        .iter()
        .chain(&update.others)
        .filter(|job| job.ticket == ticket)
        .max_by_key(|job| job.started)
}

fn row_of(board: &Jobs, ticket: u64) -> crate::jobs::JobRow {
    board
        .snapshot()
        .into_iter()
        .find(|row| row.kind == JobKind::AiPause && row.tag == Some(ticket))
        .unwrap_or_else(|| panic!("no AI row for ticket {ticket}"))
}

fn stepping(job: &AiJob) -> bool {
    matches!(job.phase, Phase::Step { .. })
}

fn attempt_states(
    path: &std::path::Path,
    request: &deadpan_jobs::RequestId,
) -> Vec<deadpan_jobs::JobState> {
    ProjectStore::open(path, AccessMode::ReadOnly)
        .unwrap()
        .generation_attempts(request, 0, 8)
        .unwrap()
        .into_iter()
        .map(|attempt| attempt.checkpoint.state)
        .collect()
}

/// A second pause's generation queues behind a running one with its
/// attempt recorded; each is cancelled individually from the panel; a
/// second job for the same pause is refused; the next queued one is
/// admitted when the model frees; edits commit throughout.
#[test]
fn several_generations_queue_for_the_model_and_cancel_individually() {
    let fixture = project(vec![ai_waiting()], vec![]);
    let board = fixture.service.jobs().clone();
    let session = fixture.workspace.session;
    let mut ai = Observer::new(&fixture.service);
    let (other, mut workspace) = second_pause(&mut ai, &fixture);

    assert_eq!(ai.start(&workspace, &fixture.hold, 2), None);
    let first = ai.phase(2, "the first worker", stepping).request.unwrap();
    let refused = ai.start(&workspace, &fixture.hold, 3).expect("refused");
    assert!(refused.contains("already generating"), "{refused}");

    assert_eq!(ai.start(&workspace, &other, 4), None);
    let second = ai
        .phase(4, "the second to queue", |job| {
            job.phase == Phase::Queued && job.request.is_some()
        })
        .request
        .unwrap();
    assert_ne!(first, second);
    assert_eq!(row_of(&board, 2).state, RowState::Running);
    assert_eq!(
        row_of(&board, 4).state,
        RowState::Queued {
            position: 1,
            waiting_for: "AI pause pictures".into()
        }
    );
    assert_eq!(
        board.waiting_tagged(JobKind::AiPause, 4),
        Some(Waiting::Holder("AI pause pictures".into()))
    );
    for _ in 0..3 {
        workspace = ai.edit(&workspace);
    }

    // Cancel only the queued one: the running one keeps the model.
    ai.cancel_from_panel(&board, session, 4);
    assert_eq!(ai.concluded(4).outcome, Some(AiOutcome::Cancelled));
    assert_eq!(
        attempt_states(&workspace.path, &second),
        [deadpan_jobs::JobState::Cancelled]
    );
    assert_eq!(row_of(&board, 2).state, RowState::Running);

    // Queue it again (a new attempt of the same request), then cancel the
    // running one: the queued one is admitted and its worker runs.
    assert_eq!(ai.start(&workspace, &other, 5), None);
    ai.phase(5, "the retry to queue", |job| job.phase == Phase::Queued);
    ai.cancel_from_panel(&board, session, 2);
    assert_eq!(ai.concluded(2).outcome, Some(AiOutcome::Cancelled));
    ai.phase(5, "the queued job to run", stepping);
    assert_eq!(row_of(&board, 5).state, RowState::Running);
    // The first slot is now empty, but the secondary slot still owns this
    // pause. Reusing the first slot must not bypass project-wide admission.
    let refused = ai.start(&workspace, &other, 6).expect("refused");
    assert!(refused.contains("already generating"), "{refused}");
    workspace = ai.edit(&workspace);
    ai.cancel_from_panel(&board, session, 5);
    ai.concluded(5);
    assert_eq!(
        attempt_states(&workspace.path, &first),
        [deadpan_jobs::JobState::Cancelled]
    );
    assert_eq!(
        attempt_states(&workspace.path, &second),
        [
            deadpan_jobs::JobState::Cancelled,
            deadpan_jobs::JobState::Cancelled
        ],
        "one nonterminal attempt per request: the retry is a second attempt"
    );
    until("the coordinator to empty", || board.is_empty());
}

/// Model jobs wait at admission on battery power, during playback and
/// under memory pressure, visibly; Run anyway admits a held job; nothing
/// holds an edit.
#[test]
fn model_jobs_wait_for_power_playback_and_memory_and_can_run_anyway() {
    let fixture = project(vec![ai_waiting()], vec![]);
    let board = fixture.service.jobs().clone();
    let session = fixture.workspace.session;
    let mut ai = Observer::new(&fixture.service);
    let (other, mut workspace) = second_pause(&mut ai, &fixture);

    board.set_conditions(Conditions {
        power: Some("running on battery".into()),
        memory: MemoryPressure::Normal,
    });
    assert_eq!(ai.start(&workspace, &fixture.hold, 2), None);
    ai.phase(2, "the job to wait", |job| {
        job.phase == Phase::Queued && job.request.is_some()
    });
    assert_eq!(
        row_of(&board, 2).state,
        RowState::Held("running on battery".into())
    );
    assert_eq!(
        board.waiting_tagged(JobKind::AiPause, 2),
        Some(Waiting::Condition("running on battery".into()))
    );
    for _ in 0..2 {
        workspace = ai.edit(&workspace);
    }
    assert!(board.run_anyway(row_of(&board, 2).id));
    ai.phase(2, "the forced job to run", stepping);
    // Conditions never stop a running model.
    assert_eq!(row_of(&board, 2).state, RowState::Running);

    // Under memory pressure a second pause waits for the model first, then
    // for the pressure once the model frees.
    board.set_conditions(Conditions {
        power: None,
        memory: MemoryPressure::Warning,
    });
    assert_eq!(ai.start(&workspace, &other, 3), None);
    ai.phase(3, "the second to queue", |job| job.phase == Phase::Queued);
    assert!(matches!(row_of(&board, 3).state, RowState::Queued { .. }));
    ai.cancel_from_panel(&board, session, 2);
    ai.concluded(2);
    until("the waiting job to be held by memory pressure", || {
        row_of(&board, 3).state == RowState::Held("memory is under pressure".into())
    });
    workspace = ai.edit(&workspace);
    // Playback holds it too; when both clear, it starts.
    board.set_foreground(Foreground { playback: true });
    board.set_conditions(Conditions::default());
    until("the waiting job to be held by playback", || {
        row_of(&board, 3).state == RowState::Held("the edit is playing".into())
    });
    ai.edit(&workspace);
    board.set_foreground(Foreground::default());
    ai.phase(3, "the job to run after playback", stepping);
    ai.cancel_from_panel(&board, session, 3);
    ai.concluded(3);
    until("the coordinator to empty", || board.is_empty());
}

/// Chaos under load: the model worker process is killed while a second
/// generation waits, tracking runs, a transcription queues and a shot scan
/// yields, and while edits are made. The killed attempt fails durably, the
/// waiting generation is admitted and runs, and every edit commits within
/// budget.
#[test]
fn a_killed_worker_process_fails_durably_while_other_jobs_and_edits_run() {
    /// Far above an ordinary edit, far below the test timeout: an edit
    /// held by any job would exceed it.
    const EDIT_BUDGET: Duration = Duration::from_secs(5);
    let pid = Arc::new(AtomicU32::new(0));
    let fixture = project(
        vec![
            AiScript {
                unavailable: None,
                steps: 2,
                step_interval: Duration::from_millis(2),
                ending: AiEnding::Process(pid.clone()),
            },
            ai_waiting(),
        ],
        vec![track_script(TrackEnding::WaitForCancel, 2, 2)],
    );
    let board = fixture.service.jobs().clone();
    let session = fixture.workspace.session;
    let mut ai = Observer::new(&fixture.service);
    let (other, mut workspace) = second_pause(&mut ai, &fixture);

    assert_eq!(ai.start(&workspace, &fixture.hold, 2), None);
    until("the worker process to start", || {
        pid.load(Ordering::Acquire) != 0
    });
    let killed_request = ai
        .phase(2, "the attempt to be recorded", |job| job.request.is_some())
        .request
        .unwrap();
    assert_eq!(ai.start(&workspace, &other, 3), None);
    ai.phase(3, "the second to queue", |job| job.phase == Phase::Queued);
    ai.track(&workspace, 4);
    let transcription = stub(&board, JobKind::Transcription, session);
    let shots = stub(&board, JobKind::Shots, session);
    until("the shot scan to yield to tracking", || {
        state_of(&board, JobKind::Shots) == Some(RowState::Paused("tracking is running".into()))
    });

    let mut slowest = Duration::ZERO;
    let mut timed_edit = |ai: &mut Observer<'_>, workspace: &Workspace| {
        let started = Instant::now();
        let saved = ai.edit(workspace);
        slowest = slowest.max(started.elapsed());
        saved
    };
    workspace = timed_edit(&mut ai, &workspace);
    // SIGKILL, as the OS or a crash would.
    let status = deadpan_native_process::spawn(
        std::process::Command::new("/bin/kill")
            .arg("-9")
            .arg(pid.load(Ordering::Acquire).to_string()),
    )
    .unwrap()
    .wait()
    .unwrap();
    assert!(status.success());
    for _ in 0..4 {
        workspace = timed_edit(&mut ai, &workspace);
    }
    let outcome = ai.concluded(2).outcome;
    assert!(
        matches!(&outcome, Some(AiOutcome::Failed(reason)) if reason.contains("exited unexpectedly")),
        "{outcome:?}"
    );
    assert_eq!(
        attempt_states(&workspace.path, &killed_request),
        [deadpan_jobs::JobState::Failed]
    );
    // The waiting generation takes the model (it outranks the automatic
    // transcription) and runs; edits still commit.
    ai.phase(3, "the waiting generation to run", stepping);
    assert!(!transcription.admitted.load(Ordering::Acquire));
    for _ in 0..2 {
        workspace = timed_edit(&mut ai, &workspace);
    }
    assert!(slowest < EDIT_BUDGET, "slowest edit {slowest:?}");

    ai.cancel_from_panel(&board, session, 3);
    ai.concluded(3);
    until("the transcription to be admitted", || {
        transcription.admitted.load(Ordering::Acquire)
    });
    let tracking = board
        .snapshot()
        .into_iter()
        .find(|row| row.kind == JobKind::Tracking)
        .unwrap()
        .id;
    board.cancel(tracking);
    board.shutdown();
    assert!(join(transcription).is_err());
    assert!(join(shots).is_err());
    fixture.service.shutdown();
    until("the service to shut down", || {
        fixture.service.is_shutdown_complete()
    });
    until("the coordinator to empty", || board.is_empty());
}
