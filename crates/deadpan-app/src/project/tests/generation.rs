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
    GenerationOperation::Start {
        ticket,
        session: fixture.workspace.session,
        revision: fixture.workspace.document.revision_id().clone(),
        hold: fixture.hold.clone(),
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
        },
    );
    assert!(refusal(&not_a_pause).unwrap().contains("Select a pause"));
    let request = deadpan_jobs::RequestId::new("ai-hold-unknown").unwrap();
    let preview = generation(
        &fixture.service,
        GenerationOperation::Preview {
            ticket: 4,
            session,
            revision: revision.clone(),
            request: request.clone(),
        },
    );
    assert!(refusal(&preview).unwrap().contains("no longer offered"));
    assert!(preview.generation.unwrap().preview.is_none());
    let accept = generation(
        &fixture.service,
        GenerationOperation::Accept {
            session,
            revision: revision.clone(),
            request,
            hold: fixture.hold.clone(),
            cursor: ProjectFrame(10),
            scope: SequenceScope::default(),
        },
    );
    assert!(accept.error.unwrap().contains("no longer offered"));
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

/// The real job thread and worker, then explicit acceptance. Opt in with
/// DEADPAN_BRIDGE_REAL=1 (about 100 s and 14 GB on an M5 Max). Set
/// DEADPAN_BRIDGE_REAL_PROJECT to an absolute scratch copy of a project to use
/// it instead of the small fixture; a 30-frame pause is inserted mid-edit.
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
    generation(&fixture.service, start(&fixture, 1));
    let mut last = None;
    let finished = wait_long(&fixture.service, |update| {
        let job = update.generation.as_ref().and_then(|g| g.job.as_ref());
        if let Some(job) = job
            && last.as_ref() != Some(&job.phase)
        {
            eprintln!("{:>6.1}s {:?}", started.elapsed().as_secs_f64(), job.phase);
            last = Some(job.phase.clone());
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
    assert_eq!(generation_state.candidates[&fixture.hold].request, request);
    let workspace = finished.workspace.unwrap();
    assert_eq!(
        workspace.document.revision_id(),
        fixture.workspace.document.revision_id(),
        "Ready never edits"
    );
    let previewed = generation(
        &fixture.service,
        GenerationOperation::Preview {
            ticket: 2,
            session: workspace.session,
            revision: workspace.document.revision_id().clone(),
            request: request.clone(),
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
    assert!(accepted.generation.unwrap().candidates.is_empty());
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

/// A real store Ready bundle (fixture bytes, no model or media decoding):
/// the service discovers it, previews it through the store's acceptance
/// preview without an edit, refuses stale preview identities and discards it
/// for the session only.
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

    /// A one-Hold project whose current request has a selected Ready bundle.
    fn seed(path: &Path) -> (NodeId, RequestId) {
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
        let identity = MessageIdentity::new(
            request.request_id.clone(),
            AttemptId::new("attempt").unwrap(),
        );
        store
            .begin_generation_attempt(BeginGenerationAttempt {
                identity: identity.clone(),
                cancellation_token: CancellationToken::new("cancel").unwrap(),
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
            provider,
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
        let limits = GeneratedMediaLimits::new(1024 * 1024).unwrap();
        for bytes in OBJECTS {
            store
                .promote_generated_object(&mut std::io::Cursor::new(bytes), &object(bytes), limits)
                .unwrap();
        }
        let receipt = BundleValidationReceipt::new(
            &candidate,
            object(OBJECTS[0]),
            object(OBJECTS[1]),
            object(OBJECTS[2]),
            video,
            plan,
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
        (hold, request.request_id)
    }

    #[test]
    fn ready_bundles_are_discovered_previewed_without_an_edit_and_discarded() {
        let scratch = tempfile::tempdir().unwrap();
        let path = scratch.path().join("ready.deadpan");
        let (hold, request) = seed(&path);
        let service =
            ProjectService::start_with(Arc::new(|| {}), None, scripted(waiting(0))).unwrap();
        let opened = command(&service, ProjectRequest::Open(path));
        assert!(opened.error.is_none(), "{:?}", opened.error);
        let workspace = opened.workspace.unwrap();
        let candidates = opened.generation.unwrap().candidates;
        assert_eq!(candidates[&hold].request, request);
        assert_eq!(candidates[&hold].frames, 12);

        let session = workspace.session;
        let revision = workspace.document.revision_id().clone();
        let stale = generation(
            &service,
            GenerationOperation::Preview {
                ticket: 1,
                session,
                revision: RevisionId::new("stale").unwrap(),
                request: request.clone(),
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
            },
        );
        let state = previewed.generation.unwrap();
        assert_eq!(state.reply, Some((2, None)));
        let preview = state.preview.expect("an issued preview");
        assert_eq!(preview.request(), &request);
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
        // Discard writes nothing: the bundle is still Ready and current.
        let store = ProjectStore::open(&workspace.path, AccessMode::ReadOnly).unwrap();
        assert!(
            store
                .selected_generation_bundle(&request)
                .unwrap()
                .is_some()
        );
    }
}
