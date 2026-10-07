//! AI pause jobs through the native project service. The scripted backend
//! keeps conditioning, allocation and every durable transition real and
//! replaces only the model worker; the real worker runs with
//! DEADPAN_BRIDGE_REAL=1.

mod preparations;

use super::*;
use crate::project::generation::{
    Backend, GenerationOperation, Job, Outcome, Phase, Script, ScriptEnding, ScriptQueue,
};
use deadpan_core::ScopedNodeTarget;
use deadpan_jobs::JobState;

fn ordinary(hold: &NodeId) -> ScopedNodeTarget {
    ScopedNodeTarget {
        node: hold.clone(),
        repeats: Vec::new(),
    }
}

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
        authoring: None,
        ticket,
        session: fixture.workspace.session,
        revision: fixture.workspace.document.revision_id().clone(),
        hold: fixture.hold.clone(),
        variants,
        options: None,
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
fn controls_survive_a_runtime_failure_before_the_request_is_recorded() {
    let fixture = project_with_pause(Backend::Scripted(Arc::new(ScriptQueue::new([
        Script {
            unavailable: Some("runtime missing".into()),
            ..waiting(0)
        },
        Script {
            ending: ScriptEnding::Fail("planned failure".into()),
            ..waiting(1)
        },
    ]))));
    let controls = deadpan_jobs::GenerationOptions {
        motion: deadpan_jobs::MotionAmount::Moderate,
        instructions: Some(deadpan_jobs::HoldInstructions::new("Keep the eyes open.").unwrap()),
        region_target: deadpan_jobs::GenerationTarget::None,
    };
    let mut operation = start(&fixture, 1);
    if let GenerationOperation::Start { options, .. } = &mut operation {
        *options = Some(controls.clone());
    }
    let unavailable = generation(&fixture.service, operation);
    let job = unavailable.generation.unwrap().job.unwrap();
    assert!(matches!(job.outcome, Some(Outcome::Unavailable(_))));
    assert!(job.request.is_none());
    assert_eq!(job.options, controls);
    generation(&fixture.service, start(&fixture, 2));
    let done = job_until(&fixture.service, |job| !job.running());
    let job = done.generation.unwrap().job.unwrap();
    let request = reader(&fixture.workspace)
        .generation_request(&job.request.unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(
        deadpan_jobs::GenerationOptions::from_constraints(&request.constraints),
        controls
    );
    assert_eq!(job.options, controls);
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
fn generation_controls_survive_retries_and_reopen_and_changes_start_a_new_request() {
    use deadpan_jobs::{GenerationOptions, HoldInstructions, MotionAmount};
    let mut fixture = project_with_pause(scripted(Script {
        ending: ScriptEnding::Fail("planned failure".into()),
        ..waiting(1)
    }));
    let choices = GenerationOptions {
        motion: MotionAmount::Subtle,
        instructions: Some(HoldInstructions::new("Keep the hands still.").unwrap()),
        region_target: deadpan_jobs::GenerationTarget::None,
    };
    let original = fixture.workspace.document.clone();
    let run = |fixture: &Fixture, ticket, options| {
        let mut operation = start(fixture, ticket);
        if let GenerationOperation::Start {
            options: stored, ..
        } = &mut operation
        {
            *stored = options;
        }
        let started = generation(&fixture.service, operation);
        assert_eq!(refusal(&started), None);
        let done = job_until(&fixture.service, |job| !job.running());
        assert!(matches!(outcome(&done), Some(Outcome::Failed(_))));
        let request = done
            .generation
            .as_ref()
            .unwrap()
            .job
            .as_ref()
            .unwrap()
            .request
            .clone()
            .unwrap();
        let stored = reader(&fixture.workspace)
            .generation_request(&request)
            .unwrap()
            .unwrap();
        assert_eq!(done.workspace.unwrap().document.as_ref(), original.as_ref());
        (
            request,
            GenerationOptions::from_constraints(&stored.constraints),
        )
    };
    let (first, actual) = run(&fixture, 1, Some(choices.clone()));
    assert_eq!(actual, choices);
    let (retry, actual) = run(&fixture, 2, None);
    assert_eq!(first, retry);
    assert_eq!(actual, choices);
    assert_eq!(attempt_states(&fixture.workspace, &first).len(), 2);

    let path = fixture.workspace.path.clone();
    command(&fixture.service, ProjectRequest::Close);
    let reopened = command(&fixture.service, ProjectRequest::Open(path));
    assert_eq!(
        reopened
            .generation
            .as_ref()
            .unwrap()
            .options
            .get(&ordinary(&fixture.hold)),
        Some(&choices)
    );
    fixture.workspace = reopened.workspace.unwrap();
    let (changed, actual) = run(&fixture, 3, Some(GenerationOptions::default()));
    assert_ne!(first, changed);
    assert_eq!(
        actual,
        GenerationOptions {
            region_target: deadpan_jobs::GenerationTarget::None,
            ..GenerationOptions::default()
        }
    );
    let (retry, actual) = run(&fixture, 4, None);
    assert_eq!(changed, retry);
    assert_eq!(
        actual,
        GenerationOptions {
            region_target: deadpan_jobs::GenerationTarget::None,
            ..GenerationOptions::default()
        }
    );
}

fn save_generation_target(fixture: &mut Fixture, name: &str, center: [u32; 2]) {
    use deadpan_core::{AttentionTarget, TargetId, TargetRegion};
    let (asset, record) = fixture
        .workspace
        .document
        .assets()
        .iter()
        .find(|(_, record)| record.video.is_some())
        .unwrap();
    let target = AttentionTarget {
        label: format!("Subject {name}"),
        asset: asset.clone(),
        span: record.video.unwrap(),
        region: TargetRegion {
            center,
            size: [200_000, 200_000],
        },
        samples: vec![],
        corrections: vec![],
        provenance: None,
    };
    let update = command(
        &fixture.service,
        ProjectRequest::Target(crate::project::targets::Operation::Save {
            ticket: 100,
            session: fixture.workspace.session,
            revision: fixture.workspace.document.revision_id().clone(),
            id: TargetId::new(name).unwrap(),
            target: Box::new(target),
        }),
    );
    assert!(update.error.is_none(), "{:?}", update.error);
    assert!(
        update
            .targets
            .as_ref()
            .unwrap()
            .reply
            .as_ref()
            .unwrap()
            .1
            .is_none(),
        "{:?}",
        update.targets
    );
    fixture.workspace = update.workspace.unwrap();
}

#[test]
fn region_choice_is_retained_when_omitted_and_explicit_none_stays_empty() {
    use deadpan_jobs::{GenerationOptions, GenerationTarget, MotionAmount};
    let mut fixture = project_with_pause(scripted(Script {
        ending: ScriptEnding::Fail("planned failure".into()),
        ..waiting(1)
    }));
    save_generation_target(&mut fixture, "subject", [400_000, 500_000]);
    let chosen = deadpan_core::TargetId::new("subject").unwrap();
    let run = |fixture: &Fixture, ticket, choices| {
        let mut operation = start(fixture, ticket);
        if let GenerationOperation::Start { options, .. } = &mut operation {
            *options = choices;
        }
        let started = generation(&fixture.service, operation);
        assert_eq!(refusal(&started), None);
        let done = job_until(&fixture.service, |job| !job.running());
        let job = done.generation.unwrap().job.unwrap();
        assert!(
            matches!(job.outcome, Some(Outcome::Failed(_))),
            "{:?}",
            job.outcome
        );
        let request = reader(&fixture.workspace)
            .generation_request(&job.request.unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(
            GenerationOptions::from_constraints(&request.constraints),
            job.options
        );
        request
    };
    let first = run(
        &fixture,
        1,
        Some(GenerationOptions {
            region_target: GenerationTarget::Saved(chosen.clone()),
            ..GenerationOptions::default()
        }),
    );
    assert_eq!(first.constraints.region_target, Some(chosen.clone()));
    let changed = run(
        &fixture,
        2,
        Some(GenerationOptions {
            motion: MotionAmount::Subtle,
            ..GenerationOptions::default()
        }),
    );
    assert_ne!(first.request_id, changed.request_id);
    assert_eq!(changed.constraints.region_target, Some(chosen));
    let retry = run(&fixture, 3, None);
    assert_eq!(changed.request_id, retry.request_id);
    let cleared = run(
        &fixture,
        4,
        Some(GenerationOptions {
            region_target: GenerationTarget::None,
            ..GenerationOptions::default()
        }),
    );
    assert_ne!(cleared.request_id, retry.request_id);
    assert!(cleared.constraints.region_target.is_none());
    save_generation_target(&mut fixture, "later", [600_000, 500_000]);
    let retry = run(&fixture, 5, None);
    assert_eq!(cleared.request_id, retry.request_id);
    assert!(retry.constraints.region_target.is_none());
    let mut invalid = start(&fixture, 6);
    if let GenerationOperation::Start { options, .. } = &mut invalid {
        *options = Some(GenerationOptions {
            region_target: GenerationTarget::Saved(deadpan_core::TargetId::new("missing").unwrap()),
            ..GenerationOptions::default()
        });
    }
    let rejected = generation(&fixture.service, invalid);
    assert!(
        refusal(&rejected)
            .unwrap()
            .contains("no longer in this project")
    );
    assert_eq!(
        reader(&fixture.workspace)
            .current_generation_requests()
            .unwrap()[0]
            .request_id,
        cleared.request_id
    );
}

#[test]
fn target_arriving_after_job_start_does_not_fill_the_captured_absence() {
    let mut fixture = project_with_pause(scripted(Script {
        ending: ScriptEnding::Fail("planned late failure".into()),
        ..waiting(300)
    }));
    generation(&fixture.service, start(&fixture, 1));
    let running = job_until(&fixture.service, |job| job.request.is_some());
    let request_id = running.generation.unwrap().job.unwrap().request.unwrap();
    save_generation_target(&mut fixture, "later", [400_000, 500_000]);
    let done = job_until(&fixture.service, |job| !job.running());
    let job = done.generation.unwrap().job.unwrap();
    assert_eq!(
        job.options.region_target,
        deadpan_jobs::GenerationTarget::None
    );
    let request = reader(&fixture.workspace)
        .generation_request(&request_id)
        .unwrap()
        .unwrap();
    assert!(request.constraints.region_target.is_none());
    assert_eq!(request.relevance, deadpan_jobs::Relevance::Current);
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
            authoring: None,
            ticket: 1,
            session,
            revision: RevisionId::new("not-current").unwrap(),
            hold: fixture.hold.clone(),
            variants: 1,
            options: None,
        },
    );
    assert_eq!(
        refusal(&stale_revision).as_deref(),
        Some("Project changed before the request")
    );
    let stale_session = generation(
        &fixture.service,
        GenerationOperation::Start {
            authoring: None,
            ticket: 1,
            session: session + 1,
            revision: revision.clone(),
            hold: fixture.hold.clone(),
            variants: 1,
            options: None,
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
            authoring: None,
            ticket: 1,
            session,
            revision: revision.clone(),
            hold: root,
            variants: 1,
            options: None,
        },
    );
    assert!(refusal(&not_a_pause).unwrap().contains("Select a pause"));
    for variants in [0, crate::project::generation::MAX_VARIANTS + 1] {
        let too_many = generation(
            &fixture.service,
            GenerationOperation::Start {
                authoring: None,
                ticket: 2,
                session,
                revision: revision.clone(),
                hold: fixture.hold.clone(),
                variants,
                options: None,
            },
        );
        assert!(refusal(&too_many).unwrap().contains("1 to 4 AI variants"));
    }
    let request = deadpan_jobs::RequestId::new("ai-hold-unknown").unwrap();
    let attempt = deadpan_jobs::AttemptId::new("unknown").unwrap();
    let preview = generation(
        &fixture.service,
        GenerationOperation::Preview {
            presentation: None,
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
            scoped: None,
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
    let candidate = generation_state.candidates[&ordinary(&fixture.hold)].clone();
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
            presentation: None,
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
            scoped: None,
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
    let offered = accepted.generation.unwrap().candidates[&ordinary(&fixture.hold)].clone();
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
            authoring: None,
            ticket: 3,
            session: workspace.session,
            revision: workspace.document.revision_id().clone(),
            hold: fixture.hold.clone(),
            variants: 1,
            options: None,
        },
    );
    assert!(refusal(&again).is_none(), "{:?}", refusal(&again));
    let finished = job_until(&fixture.service, |job| job.ticket == 3 && !job.running());
    let state = finished.generation.unwrap();
    assert_eq!(
        state.job.unwrap().outcome,
        Some(Outcome::Ready(request.clone()))
    );
    assert_eq!(state.candidates[&ordinary(&fixture.hold)].request, request);
    assert_eq!(state.candidates[&ordinary(&fixture.hold)].variants.len(), 2);
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
        failed.generation.unwrap().candidates[&ordinary(&fixture.hold)]
            .variants
            .len(),
        1
    );

    // Cancelling while a later variant runs keeps the Ready ones too.
    let workspace = failed.workspace.unwrap();
    generation(
        &fixture.service,
        GenerationOperation::Start {
            authoring: None,
            ticket: 2,
            session: workspace.session,
            revision: workspace.document.revision_id().clone(),
            hold: fixture.hold.clone(),
            variants: 2,
            options: None,
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
        cancelled.generation.unwrap().candidates[&ordinary(&fixture.hold)]
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
    let candidate = generation_state.candidates[&ordinary(&fixture.hold)].clone();
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
            presentation: None,
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
            scoped: None,
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
        accepted.generation.unwrap().candidates[&ordinary(&fixture.hold)]
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
                        instructions: None,
                        region_target: None,
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
        let candidate = update
            .generation
            .as_ref()?
            .candidates
            .get(&ordinary(hold))?
            .clone();
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
        assert_eq!(candidates[&ordinary(&hold)].request, request);
        assert_eq!(candidates[&ordinary(&hold)].frames, 12);
        assert_eq!(candidates[&ordinary(&hold)].variants.len(), 1);
        let attempt = attempts[0].clone();

        let session = workspace.session;
        let revision = workspace.document.revision_id().clone();
        let stale = generation(
            &service,
            GenerationOperation::Preview {
                presentation: None,
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
                presentation: None,
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
                presentation: None,
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
                presentation: None,
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
        let candidate = opened.generation.as_ref().unwrap().candidates[&ordinary(&hold)].clone();
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
                presentation: None,
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
        assert_eq!(state.candidates[&ordinary(&hold)].selected, attempts[1]);
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

    /// `select-hold`, `keep-hold` and `discard-hold` against the open app go
    /// through its live endpoint, change exactly what `:pick-ai`, `:keep-ai`
    /// and `:discard-ai` change and refresh the native inspector; the same
    /// operation on the closed project runs on its own writer.
    #[test]
    fn live_variant_choices_refresh_the_inspector_and_match_the_closed_command() {
        use deadpan_cli::generation::variants::VariantAction;
        use deadpan_cli::live_project::{self, Operation, Reply, ShortOperation};
        let scratch = tempfile::tempdir().unwrap();
        let path = scratch.path().join("live-variants.deadpan");
        let (hold, request, attempts) = seed(&path, 3);
        let (service, workspace, opened) = open(&path);
        assert_eq!(offered(&opened, &hold).unwrap().1, attempts[2]);
        let project = workspace.document.project_id().clone();
        let mut client = deadpan_cli::host::Client::discover(&path)
            .unwrap()
            .expect("native owner discovery");
        let mut remote = |attempt: &AttemptId, action: VariantAction| {
            live_project::request(
                &mut client,
                Operation::Execute {
                    project_id: project.clone(),
                    command: Box::new(ShortOperation::GenerationVariant {
                        request: request.clone(),
                        attempt: attempt.clone(),
                        action,
                    }),
                },
            )
        };
        let output = |reply: Reply| match reply {
            Reply::Completed {
                output,
                committed_revision: None,
                committed_registers: None,
                refresh_error: None,
            } => output,
            other => panic!("expected an operational receipt, got {other:?}"),
        };

        let chosen = output(remote(&attempts[0], VariantAction::Select).unwrap());
        assert_eq!(chosen["changed"], true, "{chosen}");
        assert_eq!(
            chosen["offered"]["selected_attempt"],
            serde_json::json!(attempts[0])
        );
        wait(&service, |update| {
            offered(update, &hold).is_some_and(|(_, selected)| selected == attempts[0])
        });
        let kept = output(remote(&attempts[1], VariantAction::Keep).unwrap());
        assert_eq!(kept["offered"]["variants"][1]["kept"], true, "{kept}");
        let shown = wait(&service, |update| {
            update
                .generation
                .as_ref()
                .is_some_and(|generation| generation.candidates[&ordinary(&hold)].variants[1].kept)
        });
        assert!(shown.message.unwrap().contains("command line"));
        let discarded = output(remote(&attempts[2], VariantAction::Discard).unwrap());
        assert_eq!(
            discarded["offered"]["variants"].as_array().unwrap().len(),
            2,
            "{discarded}"
        );
        wait(&service, |update| {
            offered(update, &hold).is_some_and(|(variants, _)| variants == attempts[..2])
        });
        // A discarded variant is no longer offered, so nothing can change it.
        let refused = remote(&attempts[2], VariantAction::Keep).unwrap_err();
        assert_eq!(refused.code, "GenerationVariantUnavailable");
        // None of these is an edit.
        assert_eq!(
            reader(&workspace).snapshot().unwrap().revision_id(),
            workspace.document.revision_id()
        );

        service.shutdown();
        let deadline = Instant::now() + TIMEOUT;
        while !service.is_shutdown_complete() {
            assert!(Instant::now() < deadline, "service shutdown timed out");
            std::thread::yield_now();
        }
        drop(service);
        // Closed: the same operation on the project's own writer.
        let released = live_project::dispatch_short(
            &path,
            None,
            ShortOperation::GenerationVariant {
                request: request.clone(),
                attempt: attempts[1].clone(),
                action: VariantAction::Release,
            },
        )
        .unwrap();
        assert_eq!(released["changed"], true, "{released}");
        assert_eq!(released["offered"]["variants"][1]["kept"], false);
        let store = ProjectStore::open(&path, AccessMode::ReadOnly).unwrap();
        let listed =
            deadpan_cli::generation::variants::offered(&store, &store.snapshot().unwrap()).unwrap();
        assert_eq!(listed[&ordinary(&hold)].selected, attempts[0]);
        assert_eq!(listed[&ordinary(&hold)].variants.len(), 2);
        assert!(!listed[&ordinary(&hold)].variants[1].kept);
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
                    scope: None,
                    expected_revision: fixture.workspace.document.revision_id().clone(),
                    variants,
                    seed: Some(40),
                    options: None,
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
        // The UI reads the published state first, as it does between frames.
        while fixture.service.take_update().is_some() {}
        let refused = live_project::request(
            &mut client,
            Operation::Generate {
                project_id: project.clone(),
                request: GenerateRequest {
                    hold: fixture.hold.clone(),
                    scope: None,
                    expected_revision: RevisionId::new("stale").unwrap(),
                    variants: 1,
                    seed: None,
                    options: None,
                },
            },
        )
        .unwrap_err();
        assert_eq!(refused.code, "GenerationRefused");
        assert!(refused.message.contains("Project changed"), "{refused}");

        let started = generate(&fixture, &mut client, 1);
        assert!(started.job >= 1 << 62, "remote jobs use their own tickets");
        assert_eq!(started.scope, ordinary(&fixture.hold));
        let done = finished(&fixture, &mut client, started.job);
        assert_eq!(done.outcome, Some(GenerationOutcome::Ready {}));
        assert_eq!((done.variants, done.ready), (1, 1));
        let request = done.request_id.clone().unwrap();
        // The app shows the same job and offers its Ready variant.
        let native = wait(&fixture.service, |update| {
            update.generation.as_ref().is_some_and(|generation| {
                generation.candidates.contains_key(&ordinary(&fixture.hold))
            })
        });
        let generation_state = native.generation.unwrap();
        assert_eq!(
            generation_state.job.as_ref().map(|job| job.ticket),
            Some(started.job)
        );
        let candidate = &generation_state.candidates[&ordinary(&fixture.hold)];
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

    /// While the app's AI job runs, a remote variant change is still
    /// admitted and checked against the offered set (the running attempt is
    /// not offered), the job keeps running, and storage cleanup waits.
    #[test]
    fn remote_variant_and_storage_requests_while_a_job_runs() {
        use deadpan_cli::generation::variants::VariantAction;
        // One step, then the job waits for cancellation without publishing
        // progress, so the UI's mailbox can be read empty.
        let fixture = project_with_pause(scripted(waiting(1)));
        let started = generation(&fixture.service, start(&fixture, 7));
        assert_eq!(refusal(&started), None);
        let running = job_until(&fixture.service, |job| {
            job.running() && job.request.is_some() && job.phase.steps() == Some((1, 1))
        });
        let request = running.generation.unwrap().job.unwrap().request.unwrap();
        let attempt = reader(&fixture.workspace)
            .generation_attempts(&request, 0, 8)
            .unwrap()[0]
            .checkpoint
            .identity
            .attempt_id
            .clone();
        let mut client = Client::discover(&fixture.workspace.path)
            .unwrap()
            .expect("native owner discovery");
        let project = fixture.workspace.document.project_id().clone();
        let refused = live_project::request(
            &mut client,
            Operation::Execute {
                project_id: project.clone(),
                command: Box::new(ShortOperation::GenerationVariant {
                    request: request.clone(),
                    attempt,
                    action: VariantAction::Keep,
                }),
            },
        )
        .unwrap_err();
        assert_eq!(refused.code, "GenerationVariantUnavailable", "{refused}");
        let busy = live_project::request(
            &mut client,
            Operation::Execute {
                project_id: project,
                command: Box::new(ShortOperation::CleanStorage {
                    grace_seconds: 24 * 60 * 60,
                    expire_variants: false,
                    dry_run: false,
                    plan: None,
                }),
            },
        )
        .unwrap_err();
        assert_eq!(busy.code, "StorageBusy", "{busy}");
        // The job was not disturbed: it still ends by cancellation.
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
    }

    /// While the AI job keeps publishing progress, a native edit receipt
    /// blocks remote edits only until the UI has taken it once; background
    /// publishes that merely re-send it do not keep remote edits out.
    #[test]
    fn a_consumed_native_receipt_does_not_block_remote_edits_during_job_progress() {
        let fixture = project_with_pause(scripted(waiting(1_000_000)));
        let started = generation(&fixture.service, start(&fixture, 7));
        assert_eq!(refusal(&started), None);
        job_until(&fixture.service, |job| {
            job.running() && job.request.is_some()
        });
        // A native edit whose receipt the UI has not read yet.
        fixture
            .service
            .submit(edit_request_in(
                &fixture.workspace,
                SequenceScope::default(),
                ProjectFrame(30),
                ProjectEdit::InsertTime {
                    at: ProjectFrame(30),
                    duration: FrameDuration::new(3).unwrap(),
                },
            ))
            .unwrap();
        let deadline = Instant::now() + TIMEOUT;
        while fixture.service.is_busy() {
            assert!(Instant::now() < deadline, "native edit timed out");
            std::thread::yield_now();
        }
        let head = reader(&fixture.workspace).head_revision().unwrap();
        assert_ne!(&head, fixture.workspace.document.revision_id());
        let project = fixture.workspace.document.project_id().clone();
        let mut client = Client::discover(&fixture.workspace.path)
            .unwrap()
            .expect("native owner discovery");
        let undo = |revision: &str| Operation::Execute {
            project_id: project.clone(),
            command: Box::new(ShortOperation::History {
                direction: live_project::HistoryDirection::Undo,
                expected_revision: head.clone(),
                new_revision: RevisionId::new(revision).unwrap(),
                dry_run: false,
            }),
        };
        let blocked = live_project::request(&mut client, undo("blocked-undo")).unwrap_err();
        assert_eq!(blocked.code, "HostBusy", "{blocked}");
        assert!(blocked.message.contains("unread"), "{blocked}");
        // The UI takes the receipt once; progress keeps re-sending it.
        let taken = wait(&fixture.service, |update| {
            update
                .committed
                .as_ref()
                .is_some_and(|committed| committed.revision == head)
        });
        assert!(taken.generation.unwrap().job.unwrap().running());
        let progressed = wait(&fixture.service, |update| {
            update
                .committed
                .as_ref()
                .is_some_and(|committed| committed.revision == head)
        });
        assert!(progressed.generation.unwrap().job.unwrap().running());
        let reply = live_project::request(&mut client, undo("remote-undo")).unwrap();
        let Reply::Completed {
            committed_revision, ..
        } = reply
        else {
            panic!("expected a committed undo")
        };
        assert_eq!(committed_revision.unwrap().as_str(), "remote-undo");
        generation(
            &fixture.service,
            GenerationOperation::Cancel {
                ticket: 11,
                session: fixture.workspace.session,
                job: 7,
            },
        );
        job_until(&fixture.service, |job| !job.running());
    }

    #[test]
    fn live_status_distinguishes_scopes_that_share_a_hold() {
        let fixture = scoped_fixture(
            scripted(Script {
                ending: ScriptEnding::Fail("expected failure".into()),
                ..waiting(1)
            }),
            false,
        );
        let mut client = Client::discover(&fixture.workspace.path)
            .unwrap()
            .expect("native owner discovery");
        let mut jobs = Vec::new();
        for target in [
            scoped_owner(&fixture, None),
            scoped_owner(&fixture, Some(1)),
        ] {
            let started = status(live_project::request(
                &mut client,
                Operation::Generate {
                    project_id: fixture.workspace.document.project_id().clone(),
                    request: GenerateRequest {
                        hold: fixture.hold.clone(),
                        scope: Some(target.clone()),
                        expected_revision: fixture.workspace.document.revision_id().clone(),
                        variants: 1,
                        seed: None,
                        options: None,
                    },
                },
            ));
            assert_eq!(started.hold, fixture.hold);
            assert_eq!(started.scope, target);
            let done = finished(&fixture, &mut client, started.job);
            assert_eq!(done.scope, target);
            assert!(matches!(
                done.outcome,
                Some(GenerationOutcome::Failed { .. })
            ));
            jobs.push((done.job, target));
        }
        assert_ne!(jobs[0].0, jobs[1].0);
        for (job, target) in jobs {
            assert_eq!(observe(&fixture, &mut client, job).scope, target);
        }
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
    let candidate = finished.generation.unwrap().candidates[&ordinary(&fixture.hold)].clone();
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
    let variant = &kept.generation.unwrap().candidates[&ordinary(&fixture.hold)].variants[0];
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
        let status = super::retention::quiet_status(&fixture.service, |status| {
            status.session == session && matches!(status.state, RetentionPassState::Done { .. })
        });
        // A no-op operational request publishes the current candidates.
        let update = generation(
            &fixture.service,
            GenerationOperation::Keep {
                ticket: 100,
                session,
                request: candidate.request.clone(),
                attempt: second.attempt.clone(),
                keep: false,
            },
        );
        (status, update)
    };
    let (status, reopened) = age_all();
    let RetentionPassState::Done { expired, .. } = status.state else {
        unreachable!()
    };
    assert_eq!(expired, 0);
    assert_eq!(
        reopened.generation.unwrap().candidates[&ordinary(&fixture.hold)]
            .variants
            .len(),
        2
    );

    // Released, the old variant expires on the next session's pass.
    let session = reopened.workspace.unwrap().session;
    let released = generation(&fixture.service, keep(4, session, false));
    assert!(refusal(&released).is_none(), "{:?}", refusal(&released));
    let (status, reopened) = age_all();
    let RetentionPassState::Done { expired, .. } = status.state else {
        unreachable!()
    };
    assert_eq!(expired, 1);
    let offered = reopened.generation.unwrap().candidates[&ordinary(&fixture.hold)].clone();
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
            authoring: None,
            ticket: 5,
            session: workspace.session,
            revision: workspace.document.revision_id().clone(),
            hold: fixture.hold.clone(),
            variants: 1,
            options: None,
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
    let candidates = picked.generation.unwrap().candidates[&ordinary(&fixture.hold)].clone();
    let variant = candidates
        .variants
        .iter()
        .find(|variant| variant.attempt == second.attempt)
        .unwrap();
    assert!(variant.picked && variant.expires_at.is_none());
}

fn scoped_fixture(backend: Backend, retime: bool) -> Fixture {
    let mut fixture = project_with_pause(backend);
    let path = fixture.workspace.path.clone();
    command(&fixture.service, ProjectRequest::Close);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite).unwrap();
    let document = store.snapshot().unwrap();
    let NodeKind::Sequence { children } = &document.nodes()[document.root()].kind else {
        panic!("root")
    };
    seed_command(
        &mut store,
        Command::Group {
            parent: document.root().clone(),
            start: 0,
            end: children.len(),
            id: node("scope-local"),
            label: "Local".into(),
        },
        "scope-grouped",
    );
    let wrapper = if retime {
        Command::WrapRetime {
            node: node("scope-local"),
            id: node("scope-owner"),
            duration: FrameDuration::new(document.duration().unwrap().frames() / 2).unwrap(),
            pitch: deadpan_core::PitchPolicy::Preserve,
        }
    } else {
        Command::WrapRepeat {
            node: node("scope-local"),
            id: node("scope-owner"),
            plays: 2,
            gap: None,
            anchor_policy: Default::default(),
        }
    };
    seed_command(&mut store, wrapper, "scope-wrapped");
    drop(store);
    let opened = command(&fixture.service, ProjectRequest::Open(path));
    assert!(opened.error.is_none(), "{:?}", opened.error);
    fixture.workspace = opened.workspace.unwrap();
    fixture
}

fn scoped_owner(fixture: &Fixture, play: Option<u32>) -> ScopedNodeTarget {
    use deadpan_core::{IterationId, RepeatEditBranch, RepeatEditStep};
    ScopedNodeTarget {
        node: fixture.hold.clone(),
        repeats: vec![RepeatEditStep {
            repeat: node("scope-owner"),
            branch: play.map_or(RepeatEditBranch::Default, |ordinal| {
                RepeatEditBranch::Play {
                    iteration: IterationId {
                        allocation: RevisionId::new("scope-wrapped").unwrap(),
                        ordinal,
                    },
                }
            }),
        }],
    }
}

fn scoped_start(
    fixture: &Fixture,
    target: &ScopedNodeTarget,
    ticket: u64,
    variants: u8,
) -> GenerationOperation {
    GenerationOperation::Start {
        ticket,
        session: fixture.workspace.session,
        revision: fixture.workspace.document.revision_id().clone(),
        hold: target.node.clone(),
        authoring: Some(target.clone()),
        variants,
        options: None,
    }
}

fn scoped_capture(
    fixture: &Fixture,
    target: &ScopedNodeTarget,
    instance: Option<deadpan_core::InstancePath>,
    cursor: ProjectFrame,
) -> crate::project::scoped::Target {
    crate::project::scoped::Target {
        session: fixture.workspace.session,
        project: fixture.workspace.document.project_id().clone(),
        revision: fixture.workspace.document.revision_id().clone(),
        scope: SequenceScope::default(),
        root: node("scope-owner"),
        target: target.clone(),
        presentation: instance,
        cursor,
        also: Vec::new(),
    }
}

#[test]
fn scoped_requests_and_options_are_independent_for_default_and_this_play() {
    use deadpan_jobs::{GenerationOptions, GenerationTarget, MotionAmount};
    let fixture = scoped_fixture(
        scripted(Script {
            ending: ScriptEnding::Fail("expected failure".into()),
            ..waiting(1)
        }),
        false,
    );
    let before = fixture.workspace.document.clone();
    let default = scoped_owner(&fixture, None);
    let play = scoped_owner(&fixture, Some(1));
    let mut requests = Vec::new();
    for (ticket, target, motion) in [
        (1, &default, MotionAmount::Subtle),
        (2, &play, MotionAmount::Still),
    ] {
        let mut operation = scoped_start(&fixture, target, ticket, 1);
        if let GenerationOperation::Start { options, .. } = &mut operation {
            *options = Some(GenerationOptions {
                motion,
                instructions: None,
                region_target: GenerationTarget::None,
            });
        }
        let started = generation(&fixture.service, operation);
        assert_eq!(refusal(&started), None);
        let done = job_until(&fixture.service, |job| {
            job.ticket == ticket && !job.running()
        });
        let job = done.generation.as_ref().unwrap().job.as_ref().unwrap();
        assert!(matches!(job.outcome, Some(Outcome::Failed(_))));
        assert_eq!(&job.target, target);
        let request = reader(&fixture.workspace)
            .generation_request(job.request.as_ref().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(&request.target, target);
        assert_eq!(&request.origin_target, target);
        assert_eq!(request.constraints.video.frames().frames(), 12);
        assert_eq!(request.constraints.motion, motion);
        assert_eq!(&*done.workspace.unwrap().document, &*before);
        requests.push(request.request_id);
    }
    assert_ne!(requests[0], requests[1]);
    assert_eq!(
        reader(&fixture.workspace)
            .current_generation_requests()
            .unwrap()
            .len(),
        2
    );
    let retry = generation(&fixture.service, scoped_start(&fixture, &default, 3, 1));
    assert_eq!(refusal(&retry), None);
    let done = job_until(&fixture.service, |job| job.ticket == 3 && !job.running());
    let state = done.generation.unwrap();
    assert_eq!(
        state.job.as_ref().unwrap().request.as_ref(),
        Some(&requests[0])
    );
    assert_eq!(
        state.job.as_ref().unwrap().options.motion,
        MotionAmount::Subtle
    );
    assert_eq!(state.options[&default].motion, MotionAmount::Subtle);
    assert_eq!(state.options[&play].motion, MotionAmount::Still);
    let omitted = generation(&fixture.service, start(&fixture, 4));
    assert!(
        refusal(&omitted).is_some(),
        "Repeat ancestry cannot be inferred"
    );
    let mut mismatched = scoped_start(&fixture, &default, 5, 1);
    if let GenerationOperation::Start { hold, .. } = &mut mismatched {
        *hold = node("scope-owner");
    }
    assert!(
        refusal(&generation(&fixture.service, mismatched))
            .unwrap()
            .contains("captured authoring")
    );
    assert_eq!(reader(&fixture.workspace).snapshot().unwrap(), *before);
}

#[test]
fn scoped_cancellation_records_no_authored_edit() {
    let fixture = scoped_fixture(scripted(waiting(2)), false);
    let target = scoped_owner(&fixture, Some(1));
    let before = fixture.workspace.document.clone();
    assert_eq!(
        refusal(&generation(
            &fixture.service,
            scoped_start(&fixture, &target, 1, 1)
        )),
        None
    );
    job_until(&fixture.service, |job| {
        matches!(job.phase, Phase::Step { .. })
    });
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
    assert_eq!(&*cancelled.workspace.unwrap().document, &*before);
    assert!(cancelled.generation.unwrap().candidates.is_empty());
}

#[test]
fn interrupted_jobs_distinguish_nested_defaults_and_plays_of_the_same_hold() {
    use deadpan_core::{RepeatEditBranch, RepeatEditStep};
    let mut fixture = scoped_fixture(scripted(waiting(1)), false);
    let path = fixture.workspace.path.clone();
    command(&fixture.service, ProjectRequest::Close);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite).unwrap();
    seed_command(
        &mut store,
        Command::WrapRepeat {
            node: node("scope-owner"),
            id: node("scope-outer"),
            plays: 2,
            gap: None,
            anchor_policy: Default::default(),
        },
        "scope-outer-wrapped",
    );
    seed_command(
        &mut store,
        Command::Rename {
            node: node("scope-owner"),
            label: "Inner".into(),
        },
        "scope-inner-named",
    );
    seed_command(
        &mut store,
        Command::Rename {
            node: node("scope-outer"),
            label: "Outer".into(),
        },
        "scope-outer-named",
    );
    drop(store);
    fixture.workspace = command(&fixture.service, ProjectRequest::Open(path))
        .workspace
        .unwrap();
    let play = |owner: &str| {
        let NodeKind::Repeat { iterations, .. } =
            &fixture.workspace.document.nodes()[&node(owner)].kind
        else {
            panic!("Repeat")
        };
        RepeatEditBranch::Play {
            iteration: iterations.at(1).unwrap(),
        }
    };
    let targets = [
        ScopedNodeTarget {
            node: fixture.hold.clone(),
            repeats: vec![
                RepeatEditStep {
                    repeat: node("scope-outer"),
                    branch: RepeatEditBranch::Default,
                },
                RepeatEditStep {
                    repeat: node("scope-owner"),
                    branch: RepeatEditBranch::Default,
                },
            ],
        },
        ScopedNodeTarget {
            node: fixture.hold.clone(),
            repeats: vec![
                RepeatEditStep {
                    repeat: node("scope-outer"),
                    branch: play("scope-outer"),
                },
                RepeatEditStep {
                    repeat: node("scope-owner"),
                    branch: play("scope-owner"),
                },
            ],
        },
    ];
    let hold_label = fixture.workspace.document.nodes()[&fixture.hold]
        .label
        .clone();
    let mut listed = None;
    for (index, target) in targets.iter().enumerate() {
        let ticket = u64::try_from(index + 1).unwrap();
        assert_eq!(
            refusal(&generation(
                &fixture.service,
                scoped_start(&fixture, target, ticket, 1)
            )),
            None
        );
        job_until(&fixture.service, |job| {
            job.ticket == ticket && matches!(job.phase, Phase::Step { .. })
        });
        let copy = fixture
            ._scratch
            .path()
            .join(format!("interrupted-scope-{index}.deadpan"));
        crate::jobs::crash_copy(&fixture.workspace.path, &copy).unwrap();
        let opened = command(&fixture.service, ProjectRequest::Open(copy));
        assert!(opened.error.is_none(), "{:?}", opened.error);
        let has_rows = |update: &ProjectUpdate| {
            update
                .generation
                .as_ref()
                .is_some_and(|state| state.interrupted.len() == index + 1)
        };
        let update = if has_rows(&opened) {
            opened
        } else {
            wait(&fixture.service, has_rows)
        };
        fixture.workspace = update.workspace.unwrap();
        listed = update.generation;
    }
    let rows = listed.unwrap().interrupted.clone();
    let default = rows
        .iter()
        .find(|row| row.target.as_ref() == Some(&targets[0]))
        .unwrap();
    let selected = rows
        .iter()
        .find(|row| row.target.as_ref() == Some(&targets[1]))
        .unwrap();
    assert_eq!(
        default.hold, selected.hold,
        "both requests still share one authored Hold"
    );
    assert_eq!(
        default.pause.as_deref(),
        Some(format!("Outer: Default › Inner: Default › {hold_label}").as_str())
    );
    assert_eq!(
        selected.pause.as_deref(),
        Some(format!("Outer: play 2 › Inner: play 2 › {hold_label}").as_str())
    );
    assert_ne!(
        default.pause, selected.pause,
        "Jobs must visibly distinguish the retry scopes"
    );
    assert!(fixture.workspace.document.overrides().is_empty());
}

#[test]
fn this_play_preview_acceptance_and_history_keep_the_exact_authoring_target() {
    use crate::project::generation::GenerationPresentation;
    use deadpan_core::{FrameRange, InstancePath, IterationId, RepeatInstance};
    if !synthetic_ready_available() {
        eprintln!("skipped: synthetic Ready tools unavailable");
        return;
    }
    let fixture = scoped_fixture(scripted(ready_script()), false);
    let target = scoped_owner(&fixture, Some(1));
    let instance = InstancePath {
        node: fixture.hold.clone(),
        repeats: vec![RepeatInstance {
            node: node("scope-owner"),
            iteration: IterationId {
                allocation: RevisionId::new("scope-wrapped").unwrap(),
                ordinal: 1,
            },
        }],
    };
    let local_duration = fixture.workspace.plan.duration().frames() / 2;
    let range = FrameRange::new(
        ProjectFrame(local_duration + 10),
        ProjectFrame(local_duration + 22),
    )
    .unwrap();
    let captured = scoped_capture(&fixture, &target, Some(instance.clone()), range.start());
    assert_eq!(
        refusal(&generation(
            &fixture.service,
            scoped_start(&fixture, &target, 1, 2)
        )),
        None
    );
    let ready = job_until(&fixture.service, |job| !job.running());
    assert!(
        matches!(outcome(&ready), Some(Outcome::Ready(_))),
        "{:?}",
        outcome(&ready)
    );
    let candidate = ready.generation.unwrap().candidates[&target].clone();
    let chosen = candidate.variants[0].attempt.clone();
    let preview = |ticket, presentation| GenerationOperation::Preview {
        ticket,
        session: fixture.workspace.session,
        revision: fixture.workspace.document.revision_id().clone(),
        request: candidate.request.clone(),
        attempt: chosen.clone(),
        draft: 100 + ticket,
        presentation,
    };
    assert!(
        refusal(&generation(&fixture.service, preview(2, None)))
            .unwrap()
            .contains("captured visible")
    );
    let wrong = GenerationPresentation {
        target: target.clone(),
        instance: instance.clone(),
        range: FrameRange::new(ProjectFrame(range.start().0 + 1), range.end()).unwrap(),
    };
    assert!(
        refusal(&generation(&fixture.service, preview(3, Some(wrong))))
            .unwrap()
            .contains("window differs")
    );
    let presentation = GenerationPresentation {
        target: target.clone(),
        instance: instance.clone(),
        range,
    };
    let proposed = generation(&fixture.service, preview(4, Some(presentation)));
    assert_eq!(refusal(&proposed), None);
    let proposed = proposed.generation.unwrap().preview.unwrap();
    assert_eq!(proposed.target(), &target);
    assert_eq!(proposed.range(), range);
    assert_ne!(proposed.hold(), &fixture.hold);
    assert!(fixture.workspace.document.overrides().is_empty());
    let accept = |scoped| GenerationOperation::Accept {
        session: fixture.workspace.session,
        revision: fixture.workspace.document.revision_id().clone(),
        request: candidate.request.clone(),
        attempt: chosen.clone(),
        hold: fixture.hold.clone(),
        cursor: range.start(),
        scope: SequenceScope::default(),
        scoped: Some(scoped),
    };
    let mut wrong_target = captured.clone();
    wrong_target.target = scoped_owner(&fixture, None);
    let refused = generation(&fixture.service, accept(wrong_target));
    assert!(refused.error.unwrap().contains("authored pause"));
    let mut stale = captured.clone();
    stale.revision = RevisionId::new("stale-capture").unwrap();
    assert!(generation(&fixture.service, accept(stale)).error.is_some());
    let mut multiple = captured.clone();
    multiple.also.push(scoped_owner(&fixture, Some(0)));
    let refused = generation(&fixture.service, accept(multiple));
    assert!(refused.error.unwrap().contains("several selected Plays"));
    let accepted = generation(&fixture.service, accept(captured.clone()));
    assert!(accepted.error.is_none(), "{:?}", accepted.error);
    let receipt = accepted
        .committed
        .as_ref()
        .unwrap()
        .scoped
        .as_ref()
        .unwrap();
    assert_eq!(receipt.before, captured);
    assert_eq!(
        accepted.committed.as_ref().unwrap().selected_node.as_ref(),
        Some(&node("scope-owner"))
    );
    assert_ne!(receipt.target.node, fixture.hold);
    assert_eq!(
        receipt.presentation.as_ref().unwrap().repeats,
        instance.repeats
    );
    let mapped = receipt.target.clone();
    let after = accepted.workspace.unwrap();
    assert!(
        matches!(&after.document.nodes()[&mapped.node].kind, NodeKind::Hold { recipe } if matches!(recipe.video, HoldVideo::Generated { .. }))
    );
    assert_eq!(
        after.document.nodes()[&fixture.hold],
        fixture.workspace.document.nodes()[&fixture.hold]
    );
    assert_eq!(
        accepted.generation.unwrap().candidates[&mapped].origin_target,
        target
    );
    let undone = command(
        &fixture.service,
        ProjectRequest::Undo {
            expected_revision: after.document.revision_id().clone(),
        },
    );
    assert!(undone.error.is_none(), "{:?}", undone.error);
    assert_eq!(
        undone.generation.as_ref().unwrap().candidates[&target].request,
        candidate.request
    );
    let current = reader(undone.workspace.as_ref().unwrap())
        .generation_request(&candidate.request)
        .unwrap()
        .unwrap();
    assert_eq!(current.target, target);
    assert_eq!(current.origin_target, target);
    let redone = command(
        &fixture.service,
        ProjectRequest::Redo {
            expected_revision: undone.workspace.unwrap().document.revision_id().clone(),
        },
    );
    assert!(redone.error.is_none(), "{:?}", redone.error);
    assert_eq!(
        redone.generation.as_ref().unwrap().candidates[&mapped].request,
        candidate.request
    );
    let after = redone.workspace.unwrap();
    let again = GenerationOperation::Start {
        ticket: 5,
        session: after.session,
        revision: after.document.revision_id().clone(),
        hold: mapped.node.clone(),
        authoring: Some(mapped.clone()),
        variants: 1,
        options: None,
    };
    assert_eq!(refusal(&generation(&fixture.service, again)), None);
    let finished = job_until(&fixture.service, |job| job.ticket == 5 && !job.running());
    assert_eq!(
        outcome(&finished),
        Some(Outcome::Ready(candidate.request.clone()))
    );
    assert_eq!(
        finished.generation.unwrap().candidates[&mapped].origin_target,
        target
    );
}

#[test]
fn retimed_hold_generates_intrinsic_duration_and_previews_its_visible_root_window() {
    use crate::project::generation::GenerationPresentation;
    use deadpan_core::{FrameRange, InstancePath};
    if !synthetic_ready_available() {
        eprintln!("skipped: synthetic Ready tools unavailable");
        return;
    }
    let fixture = scoped_fixture(scripted(ready_script()), true);
    let target = ordinary(&fixture.hold);
    assert_eq!(
        refusal(&generation(
            &fixture.service,
            scoped_start(&fixture, &target, 1, 1)
        )),
        None
    );
    let ready = job_until(&fixture.service, |job| !job.running());
    assert!(
        matches!(outcome(&ready), Some(Outcome::Ready(_))),
        "{:?}",
        outcome(&ready)
    );
    let candidate = ready.generation.unwrap().candidates[&target].clone();
    assert_eq!(candidate.frames, 12);
    let range = FrameRange::new(ProjectFrame(5), ProjectFrame(11)).unwrap();
    let operation = |ticket, presentation| GenerationOperation::Preview {
        ticket,
        session: fixture.workspace.session,
        revision: fixture.workspace.document.revision_id().clone(),
        request: candidate.request.clone(),
        attempt: candidate.selected.clone(),
        draft: 200 + ticket,
        presentation,
    };
    assert!(refusal(&generation(&fixture.service, operation(2, None))).is_some());
    let instance = InstancePath {
        node: fixture.hold.clone(),
        repeats: Vec::new(),
    };
    let result = generation(
        &fixture.service,
        operation(
            3,
            Some(GenerationPresentation {
                target: target.clone(),
                instance: instance.clone(),
                range,
            }),
        ),
    );
    assert_eq!(refusal(&result), None);
    let preview = result.generation.unwrap().preview.unwrap();
    assert_eq!(preview.range().duration().frames(), 6);
    assert_eq!(preview.target(), &target);
    let captured = scoped_capture(&fixture, &target, Some(instance), range.start());
    let accepted = generation(
        &fixture.service,
        GenerationOperation::Accept {
            session: fixture.workspace.session,
            revision: fixture.workspace.document.revision_id().clone(),
            request: candidate.request,
            attempt: candidate.selected,
            hold: fixture.hold.clone(),
            cursor: range.start(),
            scope: SequenceScope::default(),
            scoped: Some(captured),
        },
    );
    assert!(accepted.error.is_none(), "{:?}", accepted.error);
    assert_eq!(accepted.committed.unwrap().scoped.unwrap().target, target);
    assert_eq!(
        accepted.workspace.unwrap().plan.duration(),
        fixture.workspace.plan.duration()
    );
}
