//! AI pause jobs through the native project service. The scripted backend
//! keeps conditioning, allocation and every durable transition real and
//! replaces only the model worker; the real worker runs with
//! DEADPAN_BRIDGE_REAL=1.

use super::*;
use crate::project::generation::{
    Backend, GenerationOperation, Job, Outcome, Phase, Script, ScriptEnding, ScriptQueue,
};
use deadpan_jobs::JobState;

struct Fixture {
    _scratch: tempfile::TempDir,
    service: ProjectService,
    workspace: Arc<Workspace>,
    hold: NodeId,
}

fn scripted(script: Script) -> Backend {
    Backend::Scripted(Arc::new(ScriptQueue::new([script])))
}

fn waiting(steps: u64) -> Script {
    Script {
        unavailable: None,
        steps,
        step_interval: Duration::from_millis(1),
        ending: ScriptEnding::WaitForCancel,
    }
}

/// A full Original with a 12-frame pause at frame 10, pictures on both sides.
fn project_with_pause(backend: Backend) -> Fixture {
    let scratch = tempfile::tempdir().unwrap();
    let service = ProjectService::start_with(
        Arc::new(|| {}),
        Some(ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap()),
        backend,
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
    Fixture {
        _scratch: scratch,
        service,
        workspace: paused.workspace.unwrap(),
        hold,
    }
}

fn start(fixture: &Fixture, ticket: u64) -> GenerationOperation {
    start_variants(fixture, ticket, 1)
}

fn start_variants(fixture: &Fixture, ticket: u64, variants: u8) -> GenerationOperation {
    GenerationOperation::Start {
        ticket,
        session: fixture.workspace.session,
        revision: fixture.workspace.document.revision_id().clone(),
        hold: fixture.hold.clone(),
        variants,
    }
}

fn generation(service: &ProjectService, operation: GenerationOperation) -> ProjectUpdate {
    command(service, ProjectRequest::Generation(operation))
}

fn job_until(service: &ProjectService, predicate: impl Fn(&Job) -> bool) -> ProjectUpdate {
    wait(service, |update| {
        update
            .generation
            .as_ref()
            .and_then(|generation| generation.job.as_ref())
            .is_some_and(&predicate)
    })
}

fn refusal(update: &ProjectUpdate) -> Option<String> {
    update.generation.as_ref()?.reply.as_ref()?.1.clone()
}

fn outcome(update: &ProjectUpdate) -> Option<Outcome> {
    update.generation.as_ref()?.job.as_ref()?.outcome.clone()
}

fn reader(workspace: &Workspace) -> ProjectStore {
    ProjectStore::open(&workspace.path, AccessMode::ReadOnly).unwrap()
}

/// Every attempt of every request recorded for the project.
fn attempt_states(workspace: &Workspace, request: &deadpan_jobs::RequestId) -> Vec<JobState> {
    reader(workspace)
        .generation_attempts(request, 0, 16)
        .unwrap()
        .into_iter()
        .map(|attempt| attempt.checkpoint.state)
        .collect()
}

#[test]
fn an_unavailable_runtime_reports_its_reason_and_records_nothing() {
    let fixture = project_with_pause(scripted(Script {
        unavailable: Some(
            "AI pauses need the development model runtime: Python (DEADPAN_BRIDGE_PYTHON)".into(),
        ),
        ..waiting(0)
    }));
    let update = generation(&fixture.service, start(&fixture, 1));
    assert!(update.error.is_none(), "{:?}", update.error);
    assert_eq!(
        outcome(&update),
        Some(Outcome::Unavailable(
            "AI pauses need the development model runtime: Python (DEADPAN_BRIDGE_PYTHON)".into()
        ))
    );
    let workspace = update.workspace.unwrap();
    assert_eq!(
        workspace.document.revision_id(),
        fixture.workspace.document.revision_id()
    );
    assert!(
        reader(&workspace)
            .current_generation_requests()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn a_running_job_reports_progress_and_cancels_to_a_recorded_cancellation() {
    let fixture = project_with_pause(scripted(waiting(4)));
    let started = generation(&fixture.service, start(&fixture, 7));
    assert_eq!(refusal(&started), None);
    let progressed = job_until(&fixture.service, |job| {
        job.phase.steps() == Some((4, 4)) && job.request.is_some()
    });
    let job = progressed.generation.unwrap().job.unwrap();
    assert_eq!(job.hold, fixture.hold);
    assert!(job.running());
    assert_eq!(job.phase.label(), "Generating pictures");
    let request = job.request.unwrap();

    // A second start and a stale cancel are refused by identity.
    let busy = generation(&fixture.service, start(&fixture, 8));
    assert!(refusal(&busy).unwrap().contains("already generating"));
    let stale = generation(
        &fixture.service,
        GenerationOperation::Cancel {
            ticket: 10,
            session: fixture.workspace.session,
            job: 99,
        },
    );
    assert!(refusal(&stale).is_some());

    generation(
        &fixture.service,
        GenerationOperation::Cancel {
            ticket: 11,
            session: fixture.workspace.session,
            job: 7,
        },
    );
    let cancelled = job_until(&fixture.service, |job| !job.running());
    assert_eq!(outcome(&cancelled), Some(Outcome::Cancelled));
    let workspace = cancelled.workspace.unwrap();
    assert_eq!(
        workspace.document.revision_id(),
        fixture.workspace.document.revision_id(),
        "generation never edits"
    );
    assert_eq!(attempt_states(&workspace, &request), [JobState::Cancelled]);
    assert!(cancelled.generation.unwrap().candidates.is_empty());
}

#[test]
fn cancelling_immediately_never_leaves_a_live_attempt() {
    let fixture = project_with_pause(scripted(waiting(1_000)));
    fixture
        .service
        .submit(ProjectRequest::Generation(start(&fixture, 1)))
        .unwrap();
    wait(&fixture.service, |_| !fixture.service.is_busy());
    generation(
        &fixture.service,
        GenerationOperation::Cancel {
            ticket: 2,
            session: fixture.workspace.session,
            job: 1,
        },
    );
    let cancelled = job_until(&fixture.service, |job| !job.running());
    assert_eq!(outcome(&cancelled), Some(Outcome::Cancelled));
    let job = cancelled.generation.unwrap().job.unwrap();
    if let Some(request) = &job.request {
        assert_eq!(
            attempt_states(&fixture.workspace, request),
            [JobState::Cancelled]
        );
    }
    // A concluded job never blocks the next start.
    let restarted = generation(&fixture.service, start(&fixture, 3));
    assert_eq!(refusal(&restarted), None);
    generation(
        &fixture.service,
        GenerationOperation::Cancel {
            ticket: 4,
            session: fixture.workspace.session,
            job: 3,
        },
    );
    job_until(&fixture.service, |job| job.ticket == 3 && !job.running());
}

#[test]
fn cancelling_during_conditioning_shows_cancelling_at_once() {
    let fixture = project_with_pause(scripted(waiting(1_000)));
    generation(&fixture.service, start(&fixture, 1));
    let cancelled = generation(
        &fixture.service,
        GenerationOperation::Cancel {
            ticket: 2,
            session: fixture.workspace.session,
            job: 1,
        },
    );
    let job = cancelled.generation.unwrap().job.unwrap();
    assert!(
        job.phase == Phase::Cancelling || !job.running(),
        "{:?}",
        job
    );
    job_until(&fixture.service, |job| !job.running());
}

#[test]
fn a_worker_failure_is_recorded_and_shown_without_editing() {
    let fixture = project_with_pause(scripted(Script {
        ending: ScriptEnding::Fail("the scripted worker failed".into()),
        ..waiting(2)
    }));
    generation(&fixture.service, start(&fixture, 3));
    let failed = job_until(&fixture.service, |job| !job.running());
    assert_eq!(
        outcome(&failed),
        Some(Outcome::Failed("the scripted worker failed".into()))
    );
    let job = failed.generation.unwrap().job.unwrap();
    assert_eq!(
        attempt_states(&fixture.workspace, &job.request.unwrap()),
        [JobState::Failed]
    );
    assert_eq!(
        failed.workspace.unwrap().document.revision_id(),
        fixture.workspace.document.revision_id()
    );
}

#[test]
fn stale_and_invalid_generation_requests_are_refused_by_identity() {
    let fixture = project_with_pause(scripted(waiting(0)));
    let session = fixture.workspace.session;
    let revision = fixture.workspace.document.revision_id().clone();
    let stale_revision = generation(
        &fixture.service,
        GenerationOperation::Start {
            ticket: 1,
            session,
            revision: RevisionId::new("not-current").unwrap(),
            hold: fixture.hold.clone(),
            variants: 1,
        },
    );
    assert_eq!(
        refusal(&stale_revision).as_deref(),
        Some("Project changed before the request")
    );
    let stale_session = generation(
        &fixture.service,
        GenerationOperation::Start {
            ticket: 1,
            session: session + 1,
            revision: revision.clone(),
            hold: fixture.hold.clone(),
            variants: 1,
        },
    );
    assert_eq!(
        refusal(&stale_session).as_deref(),
        Some("Project session changed before the request")
    );
    let root = fixture.workspace.document.root().clone();
    let not_a_pause = generation(
        &fixture.service,
        GenerationOperation::Start {
            ticket: 1,
            session,
            revision: revision.clone(),
            hold: root,
            variants: 1,
        },
    );
    assert!(refusal(&not_a_pause).unwrap().contains("Select a pause"));
    for variants in [0, crate::project::generation::MAX_VARIANTS + 1] {
        let too_many = generation(
            &fixture.service,
            GenerationOperation::Start {
                ticket: 2,
                session,
                revision: revision.clone(),
                hold: fixture.hold.clone(),
                variants,
            },
        );
        assert!(refusal(&too_many).unwrap().contains("1 to 4 AI variants"));
    }
    let request = deadpan_jobs::RequestId::new("ai-hold-unknown").unwrap();
    let attempt = deadpan_jobs::AttemptId::new("unknown").unwrap();
    let preview = generation(
        &fixture.service,
        GenerationOperation::Preview {
            ticket: 4,
            session,
            revision: revision.clone(),
            request: request.clone(),
            attempt: attempt.clone(),
            draft: 41,
        },
    );
    assert!(refusal(&preview).unwrap().contains("no longer offered"));
    assert!(preview.generation.unwrap().preview.is_none());
    let accept = generation(
        &fixture.service,
        GenerationOperation::Accept {
            session,
            revision: revision.clone(),
            request: request.clone(),
            attempt: attempt.clone(),
            hold: fixture.hold.clone(),
            cursor: ProjectFrame(10),
            scope: SequenceScope::default(),
        },
    );
    assert!(accept.error.unwrap().contains("no longer offered"));
    for operation in [
        GenerationOperation::Select {
            ticket: 5,
            session,
            request: request.clone(),
            attempt: attempt.clone(),
        },
        GenerationOperation::Discard {
            ticket: 6,
            session,
            request: request.clone(),
            attempt: attempt.clone(),
        },
    ] {
        let refused = generation(&fixture.service, operation);
        assert!(refusal(&refused).unwrap().contains("no longer offered"));
    }
    let foreign = generation(
        &fixture.service,
        GenerationOperation::Discard {
            ticket: 7,
            session: session + 1,
            request: request.clone(),
            attempt,
        },
    );
    assert_eq!(
        refusal(&foreign).as_deref(),
        Some("Project session changed before the request")
    );
    assert!(accept.committed.is_none());
    assert_eq!(accept.workspace.unwrap().document.revision_id(), &revision);
    assert!(
        reader(&fixture.workspace)
            .current_generation_requests()
            .unwrap()
            .is_empty(),
        "refused requests record nothing"
    );
}

fn ready_script() -> Script {
    Script {
        unavailable: None,
        steps: 2,
        step_interval: Duration::from_millis(1),
        ending: ScriptEnding::Ready,
    }
}

/// Whether this machine can run the synthetic Ready worker.
fn synthetic_ready_available() -> bool {
    crate::project::generation::synthetic_tools().is_ok()
}

/// Variants through the real job thread: conditioning, allocation, the
/// synthetic worker's footage, host qualification, publication and Ready are
/// all production code; only the model is replaced. Two variants of one
/// request, one accepted, then another added to the same request.
#[test]
fn generated_variants_share_a_request_and_one_is_accepted() {
    if !synthetic_ready_available() {
        eprintln!("skipped: needs ffmpeg with libx264rgb and a built deadpan-media-worker");
        return;
    }
    let fixture = project_with_pause(Backend::Scripted(Arc::new(ScriptQueue::new([
        ready_script(),
    ]))));
    let started = generation(&fixture.service, start_variants(&fixture, 1, 2));
    assert!(refusal(&started).is_none(), "{:?}", refusal(&started));
    let finished = job_until(&fixture.service, |job| !job.running());
    let generation_state = finished.generation.clone().unwrap();
    let job = generation_state.job.clone().unwrap();
    let Some(Outcome::Ready(request)) = job.outcome.clone() else {
        panic!("expected Ready, got {:?}", job.outcome);
    };
    assert_eq!((job.variants, job.variant, job.ready), (2, 2, 2));
    let candidate = generation_state.candidates[&fixture.hold].clone();
    assert_eq!(candidate.request, request);
    assert_eq!(candidate.variants.len(), 2);
    assert_eq!(
        candidate.variants[1].seed,
        (candidate.variants[0].seed + 1) % (1 << 32)
    );
    assert_ne!(candidate.variants[0].sampled, candidate.variants[1].sampled);
    assert_eq!(candidate.selected, candidate.variants[1].attempt);
    assert_eq!(
        attempt_states(&fixture.workspace, &request),
        vec![JobState::Ready, JobState::Ready]
    );
    let workspace = finished.workspace.unwrap();
    assert_eq!(
        workspace.document.revision_id(),
        fixture.workspace.document.revision_id(),
        "Ready never edits"
    );

    // Preview and accept the first variant.
    let first = candidate.variants[0].attempt.clone();
    let previewed = generation(
        &fixture.service,
        GenerationOperation::Preview {
            ticket: 2,
            session: workspace.session,
            revision: workspace.document.revision_id().clone(),
            request: request.clone(),
            attempt: first.clone(),
            draft: 42,
        },
    );
    let preview = previewed.generation.unwrap().preview.expect("preview");
    assert_eq!(preview.attempt(), &first);
    let accepted = generation(
        &fixture.service,
        GenerationOperation::Accept {
            session: workspace.session,
            revision: workspace.document.revision_id().clone(),
            request: request.clone(),
            attempt: first.clone(),
            hold: fixture.hold.clone(),
            cursor: ProjectFrame(10),
            scope: SequenceScope::default(),
        },
    );
    assert!(accepted.error.is_none(), "{:?}", accepted.error);
    let workspace = accepted.workspace.clone().unwrap();
    let NodeKind::Hold { recipe } = &workspace.document.nodes()[&fixture.hold].kind else {
        panic!("the pause is a Hold");
    };
    let HoldVideo::Generated { accepted: artifact } = &recipe.video else {
        panic!("accepted pictures are generated");
    };
    assert_eq!(
        artifact.artifact.sampled_object,
        candidate.variants[0].sampled
    );
    // The other variant is still offered for the accepted pause.
    let offered = accepted.generation.unwrap().candidates[&fixture.hold].clone();
    assert_eq!(offered.request, request);
    assert_eq!(
        offered
            .variants
            .iter()
            .map(|variant| variant.attempt.clone())
            .collect::<Vec<_>>(),
        vec![candidate.variants[1].attempt.clone()]
    );

    // Generating again with unchanged boundary pictures adds a variant to
    // the same request.
    let again = generation(
        &fixture.service,
        GenerationOperation::Start {
            ticket: 3,
            session: workspace.session,
            revision: workspace.document.revision_id().clone(),
            hold: fixture.hold.clone(),
            variants: 1,
        },
    );
    assert!(refusal(&again).is_none(), "{:?}", refusal(&again));
    let finished = job_until(&fixture.service, |job| job.ticket == 3 && !job.running());
    let state = finished.generation.unwrap();
    assert_eq!(
        state.job.unwrap().outcome,
        Some(Outcome::Ready(request.clone()))
    );
    assert_eq!(state.candidates[&fixture.hold].request, request);
    assert_eq!(state.candidates[&fixture.hold].variants.len(), 2);
    assert_eq!(
        attempt_states(&fixture.workspace, &request),
        vec![JobState::Ready, JobState::Ready, JobState::Ready]
    );
}

/// A later variant that fails or is cancelled keeps the earlier Ready ones
/// and says so; nothing is reported as all-or-nothing.
#[test]
fn partial_variants_are_kept_and_reported() {
    if !synthetic_ready_available() {
        eprintln!("skipped: needs ffmpeg with libx264rgb and a built deadpan-media-worker");
        return;
    }
    let fixture = project_with_pause(Backend::Scripted(Arc::new(ScriptQueue::new([
        ready_script(),
        Script {
            ending: ScriptEnding::Fail("variant two stopped".into()),
            ..waiting(1)
        },
        ready_script(),
        waiting(2),
    ]))));
    generation(&fixture.service, start_variants(&fixture, 1, 2));
    let failed = job_until(&fixture.service, |job| job.ticket == 1 && !job.running());
    let job = failed.generation.as_ref().unwrap().job.clone().unwrap();
    assert!(matches!(job.outcome, Some(Outcome::Failed(_))), "{job:?}");
    assert_eq!((job.ready, job.variant), (1, 2));
    assert!(
        job.note
            .as_deref()
            .unwrap()
            .contains("1 earlier AI variant")
    );
    assert!(
        failed
            .message
            .as_deref()
            .unwrap()
            .contains("variant 2 failed")
    );
    assert_eq!(
        failed.generation.unwrap().candidates[&fixture.hold]
            .variants
            .len(),
        1
    );

    // Cancelling while a later variant runs keeps the Ready ones too.
    let workspace = failed.workspace.unwrap();
    generation(
        &fixture.service,
        GenerationOperation::Start {
            ticket: 2,
            session: workspace.session,
            revision: workspace.document.revision_id().clone(),
            hold: fixture.hold.clone(),
            variants: 2,
        },
    );
    job_until(&fixture.service, |job| {
        job.ticket == 2 && job.variant == 2 && job.phase.steps() == Some((2, 2))
    });
    generation(
        &fixture.service,
        GenerationOperation::Cancel {
            ticket: 3,
            session: workspace.session,
            job: 2,
        },
    );
    let cancelled = job_until(&fixture.service, |job| job.ticket == 2 && !job.running());
    let job = cancelled.generation.as_ref().unwrap().job.clone().unwrap();
    assert_eq!(job.outcome, Some(Outcome::Cancelled));
    assert_eq!(job.ready, 1);
    assert!(job.note.as_deref().unwrap().contains("was cancelled"));
    assert_eq!(
        cancelled.generation.unwrap().candidates[&fixture.hold]
            .variants
            .len(),
        2
    );
}

#[test]
fn closing_the_project_cancels_and_drains_the_job_before_releasing_the_writer() {
    let fixture = project_with_pause(scripted(waiting(2)));
    generation(&fixture.service, start(&fixture, 5));
    let running = job_until(&fixture.service, |job| job.request.is_some());
    let request = running.generation.unwrap().job.unwrap().request.unwrap();
    fixture.service.submit(ProjectRequest::Close).unwrap();
    let closed = wait(&fixture.service, |update| {
        update.workspace.is_none() && !fixture.service.is_busy()
    });
    assert!(closed.error.is_none(), "{:?}", closed.error);
    // The writer was released only after the cancelled attempt was recorded.
    let store = ProjectStore::open(&fixture.workspace.path, AccessMode::ReadWrite).unwrap();
    assert_eq!(
        store
            .generation_attempts(&request, 0, 16)
            .unwrap()
            .into_iter()
            .map(|attempt| attempt.checkpoint.state)
            .collect::<Vec<_>>(),
        [JobState::Cancelled]
    );
    store.head_revision().unwrap();
}

#[test]
fn shutdown_drains_a_running_job() {
    let fixture = project_with_pause(scripted(waiting(1)));
    generation(&fixture.service, start(&fixture, 2));
    let running = job_until(&fixture.service, |job| {
        job.request.is_some() && job.phase != Phase::Conditioning
    });
    let request = running.generation.unwrap().job.unwrap().request.unwrap();
    fixture.service.shutdown();
    let deadline = Instant::now() + TIMEOUT;
    while !fixture.service.is_shutdown_complete() {
        assert!(Instant::now() < deadline, "shutdown did not drain the job");
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(
        attempt_states(&fixture.workspace, &request),
        [JobState::Cancelled]
    );
}

/// The real job thread and worker generating two seeded variants, then
/// explicit acceptance of the first. Opt in with DEADPAN_BRIDGE_REAL=1 (about
/// 100 s per variant and 14 GB on an M5 Max). Set DEADPAN_BRIDGE_REAL_PROJECT
/// to an absolute scratch copy of a project to use it instead of the small
/// fixture; a 30-frame pause is inserted mid-edit.
#[test]
#[ignore = "runs the local AI model; set DEADPAN_BRIDGE_REAL=1"]
fn real_worker_generates_a_candidate_that_acceptance_commits() {
    if std::env::var_os("DEADPAN_BRIDGE_REAL").as_deref() != Some(std::ffi::OsStr::new("1")) {
        eprintln!("skipped: set DEADPAN_BRIDGE_REAL=1");
        return;
    }
    let scratch = tempfile::tempdir().unwrap();
    let service = ProjectService::start_with(
        Arc::new(|| {}),
        Some(ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap()),
        Backend::Environment,
    )
    .unwrap();
    let opened = match std::env::var_os("DEADPAN_BRIDGE_REAL_PROJECT") {
        Some(path) => command(&service, ProjectRequest::Open(PathBuf::from(path))),
        None => {
            service
                .submit(ProjectRequest::CreateFromSource {
                    path: fixture("cfr-bframes.mp4"),
                })
                .unwrap();
            wait(&service, |update| {
                update.import.as_ref().is_some_and(|status| {
                    matches!(status.stage, ImportStage::Complete | ImportStage::Failed)
                }) && !service.is_busy()
            })
        }
    };
    assert!(opened.error.is_none(), "{:?}", opened.error);
    let workspace = opened.workspace.unwrap();
    let at = ProjectFrame(workspace.plan.duration().frames() / 2);
    let paused = command(
        &service,
        edit_request_in(
            &workspace,
            SequenceScope::default(),
            at,
            ProjectEdit::InsertTime {
                at,
                duration: FrameDuration::new(30).unwrap(),
            },
        ),
    );
    assert!(paused.error.is_none(), "{:?}", paused.error);
    let fixture = Fixture {
        _scratch: scratch,
        service,
        workspace: paused.workspace.unwrap(),
        hold: paused.committed.unwrap().selected_node.unwrap(),
    };
    let started = Instant::now();
    generation(&fixture.service, start_variants(&fixture, 1, 2));
    let mut last = None;
    let finished = wait_long(&fixture.service, |update| {
        let job = update.generation.as_ref().and_then(|g| g.job.as_ref());
        if let Some(job) = job
            && last.as_ref() != Some(&(job.variant, job.phase.clone()))
        {
            eprintln!(
                "{:>6.1}s variant {} {:?}",
                started.elapsed().as_secs_f64(),
                job.variant,
                job.phase
            );
            last = Some((job.variant, job.phase.clone()));
        }
        job.is_some_and(|job| !job.running())
    });
    let job = finished.generation.as_ref().unwrap().job.clone().unwrap();
    eprintln!(
        "outcome after {:.1}s: {:?}",
        started.elapsed().as_secs_f64(),
        job.outcome
    );
    let Some(Outcome::Ready(request)) = job.outcome else {
        panic!("expected Ready, got {:?}", job.outcome);
    };
    let generation_state = finished.generation.unwrap();
    let candidate = generation_state.candidates[&fixture.hold].clone();
    assert_eq!(candidate.request, request);
    assert_eq!(job.ready, 2);
    assert_eq!(candidate.variants.len(), 2);
    assert_eq!(
        candidate.variants[1].seed,
        (candidate.variants[0].seed + 1) % (1 << 32)
    );
    assert_ne!(
        candidate.variants[0].sampled, candidate.variants[1].sampled,
        "different seeds give different pictures"
    );
    for variant in &candidate.variants {
        eprintln!(
            "variant attempt {} seed {} sampled {}",
            variant.attempt,
            variant.seed,
            variant.sampled.content()
        );
    }
    let workspace = finished.workspace.unwrap();
    assert_eq!(
        workspace.document.revision_id(),
        fixture.workspace.document.revision_id(),
        "Ready never edits"
    );
    let attempt = candidate.variants[0].attempt.clone();
    let previewed = generation(
        &fixture.service,
        GenerationOperation::Preview {
            ticket: 2,
            session: workspace.session,
            revision: workspace.document.revision_id().clone(),
            request: request.clone(),
            attempt: attempt.clone(),
            draft: 43,
        },
    );
    let preview = previewed.generation.unwrap().preview.expect("preview");
    assert!(matches!(
        &preview.document().nodes()[&fixture.hold].kind,
        NodeKind::Hold { recipe } if matches!(recipe.video, HoldVideo::Generated { .. })
    ));
    let accepted = generation(
        &fixture.service,
        GenerationOperation::Accept {
            session: workspace.session,
            revision: workspace.document.revision_id().clone(),
            request,
            attempt,
            hold: fixture.hold.clone(),
            cursor: at,
            scope: SequenceScope::default(),
        },
    );
    assert!(accepted.error.is_none(), "{:?}", accepted.error);
    let committed = accepted.committed.unwrap();
    assert_eq!(committed.selected_node.as_ref(), Some(&fixture.hold));
    let workspace = accepted.workspace.unwrap();
    assert_eq!(workspace.document.revision_id(), &committed.revision);
    assert!(workspace.can_undo);
    assert!(matches!(
        &workspace.document.nodes()[&fixture.hold].kind,
        NodeKind::Hold { recipe } if matches!(recipe.video, HoldVideo::Generated { .. })
    ));
    // The other variant stays offered for the accepted pause.
    assert_eq!(
        accepted.generation.unwrap().candidates[&fixture.hold]
            .variants
            .iter()
            .map(|variant| variant.attempt.clone())
            .collect::<Vec<_>>(),
        vec![candidate.variants[1].attempt.clone()]
    );
}

fn wait_long(
    service: &ProjectService,
    mut predicate: impl FnMut(&ProjectUpdate) -> bool,
) -> ProjectUpdate {
    let deadline = Instant::now() + Duration::from_secs(40 * 60);
    loop {
        if let Some(update) = service.take_update()
            && predicate(&update)
        {
            return update;
        }
        assert!(Instant::now() < deadline, "real generation timed out");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Real store Ready variants (fixture bytes, no model or media decoding):
/// the service discovers them, selects and previews them through the store's
/// acceptance preview without an edit, refuses stale identities, discards
/// them durably and stops offering them when the pause changes.
mod ready_fixture {
    use super::*;
    use deadpan_core::{
        ColorPolicy, FrameRate, GeneratedContentId, GeneratedObjectRef, PresentationBasis,
        SourceSpan, SourceTimeBase, SourceTimestamp,
    };
    use deadpan_jobs::{
        AttemptId, AxisLimits, BridgeCapability, BridgeGenerationPlan, CancellationToken,
        ConditioningMode, DimensionLimits, FrameCountFormula, HoldConstraints, MessageIdentity,
        MotionAmount, NativeCandidateManifest, NativeDimensions, ProtocolVersion, ProviderPackId,
        ProviderPackVersion, ProviderSelection, RequestId, RuntimeId, RuntimeVersion, Sha256,
        VideoSpec, WorkerMessage, WorkerStage, WorkspaceArtifact, WorkspaceRef,
    };
    use deadpan_store::generated_media::GeneratedMediaLimits;
    use deadpan_store::generation::GenerationRequestInput;
    use deadpan_store::generation_attempts::{
        BeginGenerationAttempt, BundleAdmissionEvidence, BundleInputObjects,
        BundleValidationReceipt, ValidatorIdentity,
    };

    const OBJECTS: [&[u8]; 6] = [
        b"native fixture",
        b"sampled fixture",
        b"provenance fixture",
        b"context fixture",
        b"left fixture",
        b"right fixture",
    ];

    fn rate() -> FrameRate {
        FrameRate::new(30, 1).unwrap()
    }

    fn object(bytes: &[u8]) -> GeneratedObjectRef {
        GeneratedObjectRef::new(
            GeneratedContentId::new(blake3::hash(bytes).to_hex().to_string()).unwrap(),
            bytes.len() as u64,
        )
        .unwrap()
    }

    fn span(end: i64) -> SourceSpan {
        let time_base = SourceTimeBase::new(1, 1000).unwrap();
        SourceSpan::new(
            SourceTimestamp {
                ticks: 0,
                time_base,
            },
            SourceTimestamp {
                ticks: end,
                time_base,
            },
        )
        .unwrap()
    }

    fn sha(character: char) -> Sha256 {
        Sha256::new(character.to_string().repeat(64)).unwrap()
    }

    /// A one-Hold project whose current request has `variants` Ready
    /// variants, each with its own seed and sampled master.
    fn seed(path: &Path, variants: u64) -> (NodeId, RequestId, Vec<AttemptId>) {
        let hold = node("hold");
        let document = ProjectDocument::new(
            ProjectId::new("project").unwrap(),
            RevisionId::new("initial").unwrap(),
            PresentationBasis {
                width: 512,
                height: 320,
                frame_rate: rate(),
                color_policy: ColorPolicy::SdrRec709,
            },
            node("root"),
        )
        .unwrap();
        let mut store = ProjectStore::create(path, &document).unwrap();
        seed_command(
            &mut store,
            Command::Insert {
                parent: node("root"),
                index: 0,
                subtree: Subtree {
                    root: hold.clone(),
                    nodes: BTreeMap::from([(
                        hold.clone(),
                        BeatNode::hold("Pause", super::hold(12)),
                    )]),
                    overrides: BTreeMap::new(),
                    gap_overrides: BTreeMap::new(),
                },
            },
            "setup",
        );
        let plan = BridgeGenerationPlan::new(
            FrameDuration::new(12).unwrap(),
            rate(),
            &BridgeCapability::new(
                true,
                FrameRate::new(24, 1).unwrap(),
                FrameCountFormula::new(1, 0, 2, 97).unwrap(),
                DimensionLimits::new(
                    AxisLimits::new(512, 512, 1).unwrap(),
                    AxisLimits::new(320, 320, 1).unwrap(),
                ),
            ),
            NativeDimensions::new(512, 320).unwrap(),
        )
        .unwrap();
        let video = VideoSpec::new(FrameDuration::new(12).unwrap(), rate(), 512, 320).unwrap();
        let provider = ProviderSelection {
            pack_id: ProviderPackId::new("pack").unwrap(),
            pack_version: ProviderPackVersion::new("v1").unwrap(),
            runtime_id: RuntimeId::new("runtime").unwrap(),
            runtime_version: RuntimeVersion::new("v1").unwrap(),
            seed: 1,
        };
        let request = store
            .record_bridge_generation_request(
                GenerationRequestInput {
                    request_id: RequestId::new("request").unwrap(),
                    expected_revision: store.snapshot().unwrap().revision_id().clone(),
                    hold_id: hold.clone(),
                    context_sha256: sha('a'),
                    constraints: HoldConstraints {
                        video: video.clone(),
                        conditioning: ConditioningMode::Bridge,
                        motion: MotionAmount::Still,
                    },
                    provider: provider.clone(),
                },
                plan.clone(),
            )
            .unwrap();
        let limits = GeneratedMediaLimits::new(1024 * 1024).unwrap();
        for bytes in OBJECTS {
            store
                .promote_generated_object(&mut std::io::Cursor::new(bytes), &object(bytes), limits)
                .unwrap();
        }
        let mut attempts = Vec::new();
        for ordinal in 1..=variants {
            let attempt = AttemptId::new(format!("attempt-{ordinal}")).unwrap();
            let identity = MessageIdentity::new(request.request_id.clone(), attempt.clone());
            store
                .begin_generation_attempt(BeginGenerationAttempt {
                    identity: identity.clone(),
                    cancellation_token: CancellationToken::new(format!("cancel-{ordinal}"))
                        .unwrap(),
                })
                .unwrap();
            let candidate = NativeCandidateManifest {
                native: WorkspaceArtifact::new(
                    WorkspaceRef::new("outputs/native.mp4").unwrap(),
                    sha('c'),
                    101,
                )
                .unwrap(),
                provenance: WorkspaceArtifact::new(
                    WorkspaceRef::new("outputs/provenance.json").unwrap(),
                    sha('d'),
                    202,
                )
                .unwrap(),
                video: VideoSpec::new(
                    FrameDuration::new(i64::from(plan.native_frame_count())).unwrap(),
                    plan.native_frame_rate(),
                    512,
                    320,
                )
                .unwrap(),
                provider: provider.for_attempt(ordinal),
            };
            for stage in [WorkerStage::Preflight, WorkerStage::Inference] {
                store
                    .record_generation_worker_message(&WorkerMessage::Stage {
                        protocol: ProtocolVersion::V2,
                        identity: identity.clone(),
                        stage,
                    })
                    .unwrap();
            }
            store
                .record_generation_worker_message(&WorkerMessage::CompletedBridge {
                    protocol: ProtocolVersion::V2,
                    identity: identity.clone(),
                    candidate: candidate.clone(),
                })
                .unwrap();
            // Each variant has its own sampled master.
            let sampled = format!("sampled fixture {ordinal}").into_bytes();
            store
                .promote_generated_object(
                    &mut std::io::Cursor::new(&sampled),
                    &object(&sampled),
                    limits,
                )
                .unwrap();
            let receipt = BundleValidationReceipt::new(
                &candidate,
                object(OBJECTS[0]),
                object(&sampled),
                object(OBJECTS[2]),
                video.clone(),
                plan.clone(),
                ValidatorIdentity::new("deadpan-media", "bridge-1").unwrap(),
            )
            .unwrap()
            .with_admission(
                BundleAdmissionEvidence::new(
                    span(458),
                    span(400),
                    BundleInputObjects::new(
                        request.binding.context_sha256.clone(),
                        object(OBJECTS[3]),
                        object(OBJECTS[4]),
                        object(OBJECTS[5]),
                    )
                    .unwrap(),
                )
                .unwrap(),
            )
            .unwrap();
            store
                .record_generation_bundle_ready(&identity, &candidate, receipt, limits)
                .unwrap();
            attempts.push(attempt);
        }
        (hold, request.request_id, attempts)
    }

    fn open(path: &Path) -> (ProjectService, Arc<Workspace>, ProjectUpdate) {
        let service =
            ProjectService::start_with(Arc::new(|| {}), None, scripted(waiting(0))).unwrap();
        let opened = command(&service, ProjectRequest::Open(path.to_path_buf()));
        assert!(opened.error.is_none(), "{:?}", opened.error);
        let workspace = opened.workspace.clone().unwrap();
        (service, workspace, opened)
    }

    fn offered(update: &ProjectUpdate, hold: &NodeId) -> Option<(Vec<AttemptId>, AttemptId)> {
        let candidate = update.generation.as_ref()?.candidates.get(hold)?.clone();
        Some((
            candidate
                .variants
                .iter()
                .map(|variant| variant.attempt.clone())
                .collect(),
            candidate.selected,
        ))
    }

    #[test]
    fn ready_bundles_are_discovered_previewed_without_an_edit_and_discarded_durably() {
        let scratch = tempfile::tempdir().unwrap();
        let path = scratch.path().join("ready.deadpan");
        let (hold, request, attempts) = seed(&path, 1);
        let (service, workspace, opened) = open(&path);
        let candidates = opened.generation.unwrap().candidates;
        assert_eq!(candidates[&hold].request, request);
        assert_eq!(candidates[&hold].frames, 12);
        assert_eq!(candidates[&hold].variants.len(), 1);
        let attempt = attempts[0].clone();

        let session = workspace.session;
        let revision = workspace.document.revision_id().clone();
        let stale = generation(
            &service,
            GenerationOperation::Preview {
                ticket: 1,
                session,
                revision: RevisionId::new("stale").unwrap(),
                request: request.clone(),
                attempt: attempt.clone(),
                draft: 44,
            },
        );
        assert_eq!(
            refusal(&stale).as_deref(),
            Some("Project changed before the request")
        );
        let previewed = generation(
            &service,
            GenerationOperation::Preview {
                ticket: 2,
                session,
                revision: revision.clone(),
                request: request.clone(),
                attempt: attempt.clone(),
                draft: 45,
            },
        );
        let state = previewed.generation.unwrap();
        assert_eq!(state.reply, Some((2, None)));
        let preview = state.preview.expect("an issued preview");
        assert_eq!(preview.request(), &request);
        assert_eq!(preview.attempt(), &attempt);
        assert_eq!(preview.base(), &revision);
        assert_eq!(preview.session(), session);
        assert_eq!(
            (preview.range().start().0, preview.range().end().0),
            (0, 12)
        );
        assert_ne!(preview.document().revision_id(), &revision);
        assert!(matches!(
            &preview.document().nodes()[&hold].kind,
            NodeKind::Hold { recipe } if matches!(recipe.video, HoldVideo::Generated { .. })
        ));
        // The audition snapshot is the same proposed document, admitted
        // against the exact committed base, under the caller's draft
        // identity (the workspace's shared proposal counter), never the
        // command ticket: a Gain or Trim draft numbered like this command's
        // ticket cannot share its playback identity.
        let audio = preview.audio();
        assert!(Arc::ptr_eq(&audio.document, preview.document()));
        assert_eq!(
            audio.content,
            deadpan_playback::ContentIdentity::Proposed {
                base_revision: revision.clone(),
                draft: 45,
                change: 1,
            }
        );
        assert_eq!(audio.session, session);
        // Another draft identity on the same base is a distinct proposal,
        // whatever the ticket; a missing identity is refused.
        let again = generation(
            &service,
            GenerationOperation::Preview {
                ticket: 2,
                session,
                revision: revision.clone(),
                request: request.clone(),
                attempt: attempt.clone(),
                draft: 46,
            },
        );
        let other = again.generation.unwrap().preview.unwrap();
        assert_ne!(other.audio().content, preview.audio().content);
        let unnumbered = generation(
            &service,
            GenerationOperation::Preview {
                ticket: 3,
                session,
                revision: revision.clone(),
                request: request.clone(),
                attempt: attempt.clone(),
                draft: 0,
            },
        );
        assert!(
            refusal(&unnumbered)
                .unwrap()
                .contains("fresh proposal identity")
        );
        // Preview never edits or adds history.
        let workspace = previewed.workspace.unwrap();
        assert_eq!(workspace.document.revision_id(), &revision);
        assert!(matches!(
            &workspace.document.nodes()[&hold].kind,
            NodeKind::Hold { recipe } if matches!(recipe.video, HoldVideo::Background)
        ));

        let discarded = generation(
            &service,
            GenerationOperation::Discard {
                ticket: 3,
                session,
                request: request.clone(),
                attempt: attempt.clone(),
            },
        );
        let state = discarded.generation.unwrap();
        assert_eq!(state.reply, Some((3, None)));
        assert!(state.candidates.is_empty());
        assert!(state.preview.is_none());
        assert_eq!(
            discarded.workspace.unwrap().document.revision_id(),
            &revision
        );
        // Discard is durable: the variant is unavailable, also after reopening.
        service.shutdown();
        let deadline = Instant::now() + TIMEOUT;
        while !service.is_shutdown_complete() {
            assert!(Instant::now() < deadline, "service shutdown timed out");
            std::thread::yield_now();
        }
        drop(service);
        let store = ProjectStore::open(&path, AccessMode::ReadOnly).unwrap();
        assert!(
            store
                .selected_generation_bundle(&request)
                .unwrap()
                .is_none()
        );
        let stored = store
            .generation_attempt(&MessageIdentity::new(request.clone(), attempt))
            .unwrap()
            .unwrap();
        assert_eq!(
            stored.bundle_receipt.unwrap().availability(),
            deadpan_store::generation_attempts::CandidateAvailability::Evicted
        );
        drop(store);
        let (_service, _workspace, reopened) = open(&path);
        assert!(reopened.generation.unwrap().candidates.is_empty());
    }

    #[test]
    fn variants_are_chosen_previewed_discarded_and_go_stale_with_their_pause() {
        let scratch = tempfile::tempdir().unwrap();
        let path = scratch.path().join("variants.deadpan");
        let (hold, request, attempts) = seed(&path, 3);
        let (service, workspace, opened) = open(&path);
        // Oldest first; the newest Ready variant is selected.
        assert_eq!(
            offered(&opened, &hold),
            Some((attempts.clone(), attempts[2].clone()))
        );
        let candidate = opened.generation.as_ref().unwrap().candidates[&hold].clone();
        let seeds: Vec<_> = candidate
            .variants
            .iter()
            .map(|variant| variant.seed)
            .collect();
        assert_eq!(seeds, vec![1, 2, 3]);
        let sampled: std::collections::BTreeSet<_> = candidate
            .variants
            .iter()
            .map(|variant| variant.sampled.clone())
            .collect();
        assert_eq!(sampled.len(), 3);
        let session = workspace.session;
        let revision = workspace.document.revision_id().clone();

        // Choosing is operational: the store's selection changes, the edit not.
        let chosen = generation(
            &service,
            GenerationOperation::Select {
                ticket: 1,
                session,
                request: request.clone(),
                attempt: attempts[0].clone(),
            },
        );
        assert_eq!(chosen.generation.as_ref().unwrap().reply, Some((1, None)));
        assert_eq!(
            offered(&chosen, &hold).unwrap().1,
            attempts[0],
            "the chosen variant is offered as selected"
        );
        assert_eq!(chosen.workspace.unwrap().document.revision_id(), &revision);
        assert_eq!(
            reader(&workspace)
                .selected_generation_bundle(&request)
                .unwrap()
                .unwrap()
                .identity
                .attempt_id,
            attempts[0]
        );

        // Previewing another variant selects it and shows its own pictures.
        let previewed = generation(
            &service,
            GenerationOperation::Preview {
                ticket: 2,
                session,
                revision: revision.clone(),
                request: request.clone(),
                attempt: attempts[1].clone(),
                draft: 46,
            },
        );
        let state = previewed.generation.unwrap();
        let preview = state.preview.expect("preview of the second variant");
        assert_eq!(preview.attempt(), &attempts[1]);
        assert_eq!(state.candidates[&hold].selected, attempts[1]);
        let NodeKind::Hold { recipe } = &preview.document().nodes()[&hold].kind else {
            panic!("the pause is a Hold");
        };
        let HoldVideo::Generated { accepted } = &recipe.video else {
            panic!("the preview shows generated pictures");
        };
        assert_eq!(
            accepted.artifact.sampled_object,
            candidate.variants[1].sampled
        );

        // Discarding the chosen variant chooses the newest remaining one and
        // ends its preview.
        let discarded = generation(
            &service,
            GenerationOperation::Discard {
                ticket: 3,
                session,
                request: request.clone(),
                attempt: attempts[1].clone(),
            },
        );
        let state = discarded.generation.as_ref().unwrap();
        assert_eq!(state.reply, Some((3, None)));
        assert!(state.preview.is_none());
        assert_eq!(
            offered(&discarded, &hold),
            Some((
                vec![attempts[0].clone(), attempts[2].clone()],
                attempts[2].clone()
            ))
        );
        // A discarded variant cannot be chosen again.
        let refused = generation(
            &service,
            GenerationOperation::Select {
                ticket: 4,
                session,
                request: request.clone(),
                attempt: attempts[1].clone(),
            },
        );
        assert!(refusal(&refused).unwrap().contains("no longer offered"));

        // Changing the pause's duration changes its context: every variant
        // goes stale together and nothing is offered.
        let lengthened = command(
            &service,
            edit_request(
                &workspace,
                ProjectEdit::HoldDuration {
                    node: hold.clone(),
                    duration: FrameDuration::new(18).unwrap(),
                },
            ),
        );
        assert!(lengthened.error.is_none(), "{:?}", lengthened.error);
        assert!(offered(&lengthened, &hold).is_none());
        let store = reader(&workspace);
        assert!(
            store
                .selected_generation_bundle(&request)
                .unwrap()
                .is_none()
        );
        assert!(
            store
                .current_generation_requests()
                .unwrap()
                .iter()
                .all(|current| current.request_id != request)
        );
        // Undo restores the duration, but not the stale request's relevance.
        let workspace = lengthened.workspace.unwrap();
        let undone = command(
            &service,
            ProjectRequest::Undo {
                expected_revision: workspace.document.revision_id().clone(),
            },
        );
        assert!(undone.error.is_none(), "{:?}", undone.error);
        assert!(offered(&undone, &hold).is_none());
    }
}

/// `generate-hold` and `accept-hold` against an open project: the live
/// endpoint starts the app's own AI job, observes and cancels exactly it,
/// and accepts a Ready variant in one owner transaction.
mod live {
    use super::*;
    use deadpan_cli::host::Client;
    use deadpan_cli::live_project::generation::{
        GenerateRequest, GenerationOutcome, GenerationStatus,
    };
    use deadpan_cli::live_project::{self, LiveError, Operation, Reply, ShortOperation};

    fn status(reply: Result<Reply, LiveError>) -> GenerationStatus {
        match reply {
            Ok(Reply::Generation { status }) => *status,
            other => panic!("expected a generation status, got {other:?}"),
        }
    }

    fn generate(fixture: &Fixture, client: &mut Client, variants: u8) -> GenerationStatus {
        status(live_project::request(
            client,
            Operation::Generate {
                project_id: fixture.workspace.document.project_id().clone(),
                request: GenerateRequest {
                    hold: fixture.hold.clone(),
                    expected_revision: fixture.workspace.document.revision_id().clone(),
                    variants,
                    seed: Some(40),
                },
            },
        ))
    }

    fn observe(fixture: &Fixture, client: &mut Client, job: u64) -> GenerationStatus {
        status(live_project::request(
            client,
            Operation::GenerationStatus {
                project_id: fixture.workspace.document.project_id().clone(),
                job,
            },
        ))
    }

    fn finished(fixture: &Fixture, client: &mut Client, job: u64) -> GenerationStatus {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let current = observe(fixture, client, job);
            if current.finished() {
                return current;
            }
            assert!(Instant::now() < deadline, "live generation timed out");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn a_live_request_runs_the_apps_job_and_accepts_its_variant() {
        if !synthetic_ready_available() {
            eprintln!("skipped: needs ffmpeg with libx264rgb and a built deadpan-media-worker");
            return;
        }
        let fixture = project_with_pause(Backend::Scripted(Arc::new(ScriptQueue::new([
            ready_script(),
        ]))));
        let mut client = Client::discover(&fixture.workspace.path)
            .unwrap()
            .expect("native owner discovery");
        let project = fixture.workspace.document.project_id().clone();
        let refused = live_project::request(
            &mut client,
            Operation::Generate {
                project_id: project.clone(),
                request: GenerateRequest {
                    hold: fixture.hold.clone(),
                    expected_revision: RevisionId::new("stale").unwrap(),
                    variants: 1,
                    seed: None,
                },
            },
        )
        .unwrap_err();
        assert_eq!(refused.code, "GenerationRefused");
        assert!(refused.message.contains("Project changed"), "{refused}");

        let started = generate(&fixture, &mut client, 1);
        assert!(started.job >= 1 << 62, "remote jobs use their own tickets");
        let done = finished(&fixture, &mut client, started.job);
        assert_eq!(done.outcome, Some(GenerationOutcome::Ready {}));
        assert_eq!((done.variants, done.ready), (1, 1));
        let request = done.request_id.clone().unwrap();
        // The app shows the same job and offers its Ready variant.
        let native = wait(&fixture.service, |update| {
            update
                .generation
                .as_ref()
                .is_some_and(|generation| generation.candidates.contains_key(&fixture.hold))
        });
        let generation_state = native.generation.unwrap();
        assert_eq!(
            generation_state.job.as_ref().map(|job| job.ticket),
            Some(started.job)
        );
        let candidate = &generation_state.candidates[&fixture.hold];
        assert_eq!(candidate.request, request);
        assert_eq!(candidate.variants[0].seed, 40, "the caller's seed");

        let unknown = live_project::request(
            &mut client,
            Operation::GenerationStatus {
                project_id: project.clone(),
                job: started.job + 1,
            },
        )
        .unwrap_err();
        assert_eq!(unknown.code, "GenerationUnknown");

        // An explicit seed cannot apply to a request whose variants derive
        // theirs from its own seed: the job refuses instead of ignoring it.
        let seeded = generate(&fixture, &mut client, 1);
        let refused = finished(&fixture, &mut client, seeded.job);
        assert!(
            matches!(&refused.outcome, Some(GenerationOutcome::Failed { reason }) if reason.contains("derive their seeds")),
            "{refused:?}"
        );
        // The earlier job's result outlives the newer job until released.
        let retained = observe(&fixture, &mut client, started.job);
        assert_eq!(retained.outcome, Some(GenerationOutcome::Ready {}));
        assert_eq!(retained.request_id, Some(request.clone()));
        assert!(matches!(
            live_project::request(
                &mut client,
                Operation::ReleaseGenerationStatus {
                    project_id: project.clone(),
                    job: started.job,
                },
            ),
            Ok(Reply::Released)
        ));
        let released = live_project::request(
            &mut client,
            Operation::GenerationStatus {
                project_id: project.clone(),
                job: started.job,
            },
        )
        .unwrap_err();
        assert_eq!(released.code, "GenerationUnknown");

        // The UI reads the jobs' published state before another edit.
        while fixture.service.take_update().is_some() {}
        // Acceptance names the head it observed; a different head refuses.
        let stale = live_project::request(
            &mut client,
            Operation::Execute {
                project_id: project.clone(),
                command: Box::new(ShortOperation::AcceptHold {
                    request: request.clone(),
                    attempt: Some(candidate.variants[0].attempt.clone()),
                    expected_revision: Some(RevisionId::new("not-the-head").unwrap()),
                    new_revision: RevisionId::new("never").unwrap(),
                }),
            },
        )
        .unwrap_err();
        assert_eq!(stale.code, "RevisionConflict");

        let reply = live_project::request(
            &mut client,
            Operation::Execute {
                project_id: project.clone(),
                command: Box::new(ShortOperation::AcceptHold {
                    request: request.clone(),
                    attempt: Some(candidate.variants[0].attempt.clone()),
                    expected_revision: Some(fixture.workspace.document.revision_id().clone()),
                    new_revision: RevisionId::new("live-accepted").unwrap(),
                }),
            },
        )
        .unwrap();
        let Reply::Completed {
            committed_revision,
            refresh_error,
            ..
        } = reply
        else {
            panic!("expected a committed acceptance")
        };
        assert_eq!(committed_revision.unwrap().as_str(), "live-accepted");
        assert!(refresh_error.is_none(), "{refresh_error:?}");
        let refreshed = wait(&fixture.service, |update| {
            update.workspace.as_ref().is_some_and(|workspace| {
                workspace.document.revision_id().as_str() == "live-accepted"
            })
        });
        let workspace = refreshed.workspace.unwrap();
        assert!(matches!(
            &workspace.document.nodes()[&fixture.hold].kind,
            NodeKind::Hold { recipe } if matches!(recipe.video, HoldVideo::Generated { .. })
        ));
        assert!(workspace.can_undo);
    }

    #[test]
    fn a_live_cancellation_names_exactly_the_observed_job() {
        let fixture = project_with_pause(scripted(waiting(3)));
        let mut client = Client::discover(&fixture.workspace.path)
            .unwrap()
            .expect("native owner discovery");
        let project = fixture.workspace.document.project_id().clone();
        let started = generate(&fixture, &mut client, 2);
        assert_eq!(started.variants, 2);
        let deadline = Instant::now() + TIMEOUT;
        while observe(&fixture, &mut client, started.job)
            .request_id
            .is_none()
        {
            assert!(Instant::now() < deadline, "allocation timed out");
            std::thread::sleep(Duration::from_millis(2));
        }
        let wrong = live_project::request(
            &mut client,
            Operation::CancelGeneration {
                project_id: project.clone(),
                job: started.job + 7,
            },
        )
        .unwrap_err();
        assert_eq!(wrong.code, "GenerationUnknown");
        assert!(
            observe(&fixture, &mut client, started.job)
                .outcome
                .is_none()
        );
        let cancelling = status(live_project::request(
            &mut client,
            Operation::CancelGeneration {
                project_id: project,
                job: started.job,
            },
        ));
        assert_eq!(cancelling.job, started.job);
        let done = finished(&fixture, &mut client, started.job);
        assert_eq!(done.outcome, Some(GenerationOutcome::Cancelled {}));
        assert_eq!(
            attempt_states(&fixture.workspace, done.request_id.as_ref().unwrap()),
            vec![JobState::Cancelled]
        );
    }
}

/// Keep pins a variant against the retention policy; the automatic pass of a
/// later session expires an old, unkept, unchosen variant, which is then no
/// longer offered. Selection never expires.
#[test]
fn kept_variants_survive_and_the_automatic_pass_expires_old_ones() {
    use crate::project::RetentionPassState;
    if !synthetic_ready_available() {
        eprintln!("skipped: needs ffmpeg with libx264rgb and a built deadpan-media-worker");
        return;
    }
    let fixture = project_with_pause(Backend::Scripted(Arc::new(ScriptQueue::new([
        ready_script(),
    ]))));
    generation(&fixture.service, start_variants(&fixture, 1, 2));
    let finished = job_until(&fixture.service, |job| !job.running());
    let candidate = finished.generation.unwrap().candidates[&fixture.hold].clone();
    let (first, second) = (&candidate.variants[0], &candidate.variants[1]);
    assert_eq!(candidate.selected, second.attempt);
    assert!(!first.kept && first.expires_at.is_some());
    assert_eq!(
        first.expires_at,
        Some(first.ready_at + deadpan_store::generation_retention::DEFAULT_VARIANT_RETENTION)
    );
    assert_eq!(second.expires_at, None, "the selection never expires");
    let session = fixture.workspace.session;
    let keep = |ticket, session, keep| GenerationOperation::Keep {
        ticket,
        session,
        request: candidate.request.clone(),
        attempt: first.attempt.clone(),
        keep,
    };
    let stale = generation(&fixture.service, keep(2, session + 1, true));
    assert!(refusal(&stale).is_some());
    let kept = generation(&fixture.service, keep(3, session, true));
    assert!(refusal(&kept).is_none(), "{:?}", refusal(&kept));
    let variant = &kept.generation.unwrap().candidates[&fixture.hold].variants[0];
    assert!(variant.kept && variant.expires_at.is_none());

    // Make every variant old, as if generated weeks ago, and reopen: the
    // kept one stays.
    let path = fixture.workspace.path.clone();
    let age_all = || {
        command(&fixture.service, ProjectRequest::Close);
        let database = rusqlite::Connection::open(path.join("project.sqlite")).unwrap();
        database
            .execute("UPDATE generation_variant_retention SET ready_at_ms=0", [])
            .unwrap();
        drop(database);
        let opened = command(&fixture.service, ProjectRequest::Open(path.clone()));
        let session = opened.workspace.unwrap().session;
        wait(&fixture.service, |update| {
            update.storage_retention.as_ref().is_some_and(|status| {
                status.session == session && matches!(status.state, RetentionPassState::Done { .. })
            })
        })
    };
    let reopened = age_all();
    let RetentionPassState::Done { expired, .. } = reopened.storage_retention.unwrap().state else {
        unreachable!()
    };
    assert_eq!(expired, 0);
    assert_eq!(
        reopened.generation.unwrap().candidates[&fixture.hold]
            .variants
            .len(),
        2
    );

    // Released, the old variant expires on the next session's pass.
    let session = reopened.workspace.unwrap().session;
    let released = generation(&fixture.service, keep(4, session, false));
    assert!(refusal(&released).is_none(), "{:?}", refusal(&released));
    let reopened = age_all();
    let RetentionPassState::Done { expired, .. } = reopened.storage_retention.unwrap().state else {
        unreachable!()
    };
    assert_eq!(expired, 1);
    let offered = reopened.generation.unwrap().candidates[&fixture.hold].clone();
    assert_eq!(
        offered
            .variants
            .iter()
            .map(|variant| variant.attempt.clone())
            .collect::<Vec<_>>(),
        vec![second.attempt.clone()]
    );

    // A newer variant takes the selection from one only the selection
    // protected: the job and message say it now expires.
    let workspace = reopened.workspace.unwrap();
    generation(
        &fixture.service,
        GenerationOperation::Start {
            ticket: 5,
            session: workspace.session,
            revision: workspace.document.revision_id().clone(),
            hold: fixture.hold.clone(),
            variants: 1,
        },
    );
    let finished = job_until(&fixture.service, |job| job.ticket == 5 && !job.running());
    let job = finished.generation.as_ref().unwrap().job.clone().unwrap();
    assert_eq!(job.selected_before, Some(second.attempt.clone()));
    assert_eq!(job.unprotected, Some(second.attempt.clone()));
    assert!(
        finished
            .message
            .as_deref()
            .unwrap()
            .contains("is no longer the chosen one"),
        "{:?}",
        finished.message
    );
    // Choosing it explicitly protects it again, though another is newer.
    let picked = generation(
        &fixture.service,
        GenerationOperation::Select {
            ticket: 6,
            session: workspace.session,
            request: candidate.request.clone(),
            attempt: second.attempt.clone(),
        },
    );
    let candidates = picked.generation.unwrap().candidates[&fixture.hold].clone();
    let variant = candidates
        .variants
        .iter()
        .find(|variant| variant.attempt == second.attempt)
        .unwrap();
    assert!(variant.picked && variant.expires_at.is_none());
}
