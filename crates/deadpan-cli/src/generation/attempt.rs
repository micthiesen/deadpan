//! One supervised AI Hold attempt, split into steps so a host can keep store
//! writes on its writer thread and the long worker run on a job thread:
//!
//! 1. [`allocate`] (writer): record the bridge request and begin its attempt.
//! 2. [`run_worker`] (job thread, no store): write the conditioning inputs into
//!    a fresh private workspace, pin it, capture the inputs, launch and
//!    supervise the worker, then qualify its bundle after clean teardown.
//!    Durable lifecycle transitions are handed to the caller as
//!    [`AttemptRecord`]s, which it applies with [`record`] on the writer;
//!    progress stays in memory.
//! 3. [`finish`] (writer): publish the six objects and record Ready, or record
//!    the failure or cancellation truthfully.
//!
//! Nothing here edits the project; acceptance is a separate explicit edit.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use deadpan_core::{NodeId, RevisionId};
use deadpan_jobs::artifact::ArtifactWorkspace;
use deadpan_jobs::supervisor::{ProcessEvent, ProcessLimits, ProcessSpec, WorkerProcess};
use deadpan_jobs::{
    AttemptId, CancellationToken, ContextArtifact, Diagnostic, HoldTarget, HostFailure,
    HostFailureCode, HostMessage, JobFailure, JobLifecycle, JobState, MessageIdentity,
    NativeCandidateManifest, ProtocolVersion, RequestId, TargetBinding, WorkerMessage, WorkerStage,
    WorkspaceArtifact, WorkspaceRef,
};
use deadpan_media::protocol::ConversionLimits;
use deadpan_models::{
    BridgeQualification, ConditioningLimits, QualificationLimits, QualifiedBridgeBundle,
    SelectedBridgeProvider, capture_bridge_conditioning, qualify_bridge,
};
use deadpan_store::ProjectStore;
use deadpan_store::StoreError;
use deadpan_store::generated_media::GeneratedMediaLimits;
use deadpan_store::generation::{GenerationRequestInput, StoredGenerationRequest};
use deadpan_store::generation_attempts::{
    BeginGenerationAttempt, BundleAdmissionEvidence, BundleInputObjects, BundleValidationReceipt,
    ValidatorIdentity,
};

use super::conditioning::{BridgeInputs, LEFT, MANIFEST, RIGHT};
use super::runtime::BridgeRuntime;

/// The worker's whole-attempt deadline.
pub const WORKER_DEADLINE: Duration = Duration::from_secs(30 * 60);
const CANCELLATION_GRACE: Duration = Duration::from_secs(5);
const EXIT_GRACE: Duration = Duration::from_secs(10);
const INPUT_SCOPE: &str = "inputs";
const OUTPUT_SCOPE: &str = "outputs";
/// The largest object the store will publish or reread for one bundle.
pub const OBJECT_BUDGET: u64 = 512 * 1024 * 1024;

fn conditioning_limits() -> ConditioningLimits {
    ConditioningLimits::new(1024 * 1024, 64 * 1024 * 1024, 30_000)
        .expect("constant conditioning limits")
}

fn qualification_limits() -> QualificationLimits {
    QualificationLimits {
        media: ConversionLimits {
            max_input_bytes: 128 * 1024 * 1024,
            max_output_bytes: 128 * 1024 * 1024,
            max_scratch_bytes: 64 * 1024 * 1024,
            timeout_ms: 120_000,
        },
        maximum_worker_provenance_bytes: 4 * 1024 * 1024,
        maximum_host_provenance_bytes: 1024 * 1024,
    }
}

pub fn object_limits() -> GeneratedMediaLimits {
    GeneratedMediaLimits::new(OBJECT_BUDGET).expect("positive constant budget")
}

#[derive(Debug, thiserror::Error)]
pub enum GenerationError {
    #[error(transparent)]
    Runtime(#[from] super::runtime::RuntimeError),
    #[error("{0}")]
    Inputs(String),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("{0}")]
    Invalid(String),
    #[error("The AI pause was cancelled.")]
    Cancelled,
    #[error("The AI pause could not be generated: {0}")]
    Failed(String),
}

impl GenerationError {
    pub fn code(&self) -> &str {
        match self {
            Self::Runtime(_) => "GenerationUnavailable",
            Self::Inputs(_) => "GenerationInputsUnavailable",
            Self::Store(error) => error.code(),
            Self::Invalid(_) => "GenerationRefused",
            Self::Cancelled => "GenerationCancelled",
            Self::Failed(_) => "GenerationFailed",
        }
    }
}

fn invalid(error: impl std::fmt::Display) -> GenerationError {
    GenerationError::Invalid(error.to_string())
}

/// What a new attempt is for.
#[derive(Debug, Clone)]
pub struct AllocateInput {
    pub hold: NodeId,
    /// The revision `inputs` were prepared from; allocation refuses another.
    pub expected_revision: RevisionId,
    pub seed: u64,
    pub inputs: BridgeInputs,
}

/// A recorded request and its begun attempt, with everything the worker run
/// needs. Owns no store or process.
#[derive(Debug, Clone)]
pub struct Allocated {
    pub request: StoredGenerationRequest,
    pub identity: MessageIdentity,
    pub cancellation_token: CancellationToken,
    pub host_message: HostMessage,
    ordinal: u64,
    inputs: BridgeInputs,
}

impl Allocated {
    /// The conditioning inputs this attempt's request was recorded with.
    /// Every later variant of the request reuses them unchanged.
    pub fn inputs(&self) -> &BridgeInputs {
        &self.inputs
    }

    /// The 1-based attempt ordinal within the request.
    pub fn ordinal(&self) -> u64 {
        self.ordinal
    }

    /// The exact provider of this attempt, a seeded variant of the request.
    pub fn provider(&self) -> &deadpan_jobs::ProviderSelection {
        let HostMessage::GenerateBridge { provider, .. } = &self.host_message else {
            unreachable!("allocation builds a bridge request");
        };
        provider
    }
}

/// Record a bridge request for `input.hold` and begin its first attempt.
/// A new request makes the Hold's previous requests stale.
pub fn allocate(
    store: &mut ProjectStore,
    input: AllocateInput,
) -> Result<Allocated, GenerationError> {
    let request_id =
        RequestId::new(format!("ai-hold-{}", uuid::Uuid::new_v4().simple())).map_err(invalid)?;
    let provider = super::development_provider(input.seed);
    let request = store.record_bridge_generation_request(
        GenerationRequestInput {
            request_id: request_id.clone(),
            expected_revision: input.expected_revision.clone(),
            hold_id: input.hold.clone(),
            context_sha256: input.inputs.manifest_sha256.clone(),
            constraints: input.inputs.constraints.clone(),
            provider,
        },
        input.inputs.plan.clone(),
    )?;
    allocate_variant(store, request, input.inputs)
}

/// The Hold's current bridge request, whose attempts are its variants.
pub fn current_bridge_request(
    store: &ProjectStore,
    hold: &NodeId,
) -> Result<Option<StoredGenerationRequest>, StoreError> {
    Ok(store
        .current_generation_requests()?
        .into_iter()
        .find(|request| &request.binding.hold_id == hold && request.bridge_plan.is_some()))
}

/// Begin another attempt of an existing current `request`: a new seeded
/// variant ([`deadpan_jobs::ProviderSelection::for_attempt`]) with the
/// request's exact constraints, plan and conditioning `inputs`. Earlier Ready
/// variants stay available for explicit selection.
pub fn allocate_variant(
    store: &mut ProjectStore,
    request: StoredGenerationRequest,
    inputs: BridgeInputs,
) -> Result<Allocated, GenerationError> {
    if inputs.manifest_sha256 != request.binding.context_sha256
        || inputs.constraints != request.constraints
        || request.bridge_plan.as_ref() != Some(&inputs.plan)
    {
        return Err(invalid(
            "the pause's boundary pictures changed since its AI pictures were requested; generate again",
        ));
    }
    let identity = MessageIdentity::new(
        request.request_id.clone(),
        AttemptId::new(uuid::Uuid::new_v4().simple().to_string()).map_err(invalid)?,
    );
    let cancellation_token =
        CancellationToken::new(uuid::Uuid::new_v4().simple().to_string()).map_err(invalid)?;
    let begun = store.begin_generation_attempt(BeginGenerationAttempt {
        identity: identity.clone(),
        cancellation_token: cancellation_token.clone(),
    })?;
    let provider = request.provider.for_attempt(begun.ordinal);
    let host_message = HostMessage::GenerateBridge {
        protocol: ProtocolVersion::V2,
        identity: identity.clone(),
        cancellation_token: cancellation_token.clone(),
        project_id: request.binding.project_id.clone(),
        revision_id: request.origin_revision.clone(),
        target: HoldTarget {
            hold_id: request.binding.hold_id.clone(),
            request_version: request.binding.request_version,
        },
        input: ContextArtifact {
            manifest: WorkspaceRef::new(MANIFEST).map_err(invalid)?,
            sha256: inputs.manifest_sha256.clone(),
        },
        output_workspace: WorkspaceRef::new(OUTPUT_SCOPE).map_err(invalid)?,
        constraints: inputs.constraints.clone(),
        provider: Box::new(provider),
        plan: Box::new(inputs.plan.clone()),
    };
    host_message.validate().map_err(invalid)?;
    Ok(Allocated {
        request,
        identity,
        cancellation_token,
        host_message,
        ordinal: begun.ordinal,
        inputs,
    })
}

/// A durable lifecycle transition the caller applies with [`record`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttemptRecord {
    /// A worker Stage or terminal message (never Progress).
    Worker(Box<WorkerMessage>),
    /// The host began cancelling the attempt.
    CancelRequested,
}

/// Apply one [`AttemptRecord`] to the store.
pub fn record(
    store: &mut ProjectStore,
    allocated: &Allocated,
    record: &AttemptRecord,
) -> Result<(), StoreError> {
    match record {
        AttemptRecord::Worker(message) => {
            store.record_generation_worker_message(message)?;
        }
        AttemptRecord::CancelRequested => {
            store.request_generation_attempt_cancel(
                &allocated.identity,
                &allocated.cancellation_token,
            )?;
        }
    }
    Ok(())
}

/// In-memory progress for display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttemptProgress {
    /// Writing and capturing the conditioning inputs.
    Preparing,
    Stage(WorkerStage),
    Step {
        stage: WorkerStage,
        completed: u64,
        total: u64,
    },
    /// The worker exited cleanly; the host is qualifying its bundle.
    Qualifying,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RunTimings {
    /// Workspace creation and conditioning capture.
    pub preparation: Duration,
    /// Worker launch through clean exit.
    pub worker: Duration,
    /// Host qualification of the native footage and sampled master.
    pub qualification: Duration,
}

/// A bundle qualified after clean teardown, ready for publication.
pub struct QualifiedRun {
    pub bundle: QualifiedBridgeBundle,
    pub receipt: BundleValidationReceipt,
    pub declaration: NativeCandidateManifest,
}

pub enum RunResult {
    Qualified(Box<QualifiedRun>),
    Failed(JobFailure),
    Cancelled,
}

/// The result of [`run_worker`]. The private workspace is removed when this
/// is dropped; publication reads only the qualified, retained copies.
pub struct WorkerRun {
    pub result: RunResult,
    pub timings: RunTimings,
    /// The bounded tail of the worker's diagnostic output.
    pub worker_log: String,
    pub worker_log_discarded_bytes: u64,
    _directory: Option<tempfile::TempDir>,
}

impl WorkerRun {
    /// A run that concluded without a worker workspace: a host whose job
    /// thread stopped before or instead of [`run_worker`], or a test seam that
    /// substitutes the worker. Only a failure or cancellation is meaningful;
    /// [`finish`] records it truthfully.
    pub fn without_workspace(result: RunResult, timings: RunTimings) -> Self {
        Self::early(result, timings)
    }

    fn early(result: RunResult, timings: RunTimings) -> Self {
        Self {
            result,
            timings,
            worker_log: String::new(),
            worker_log_discarded_bytes: 0,
            _directory: None,
        }
    }
}

fn diagnostic(reason: &str) -> Diagnostic {
    let detail: String = reason
        .chars()
        .filter(|value| *value != '\0')
        .take(1_000)
        .collect();
    Diagnostic::new(if detail.is_empty() {
        "worker failed".into()
    } else {
        detail
    })
    .expect("at most 1000 Unicode characters fit the diagnostic budget")
}

/// A host failure with a bounded diagnostic.
pub fn host_failure(code: HostFailureCode, reason: &str) -> HostFailure {
    HostFailure {
        code,
        detail: diagnostic(reason),
    }
}

/// Fail a nonterminal lifecycle, keeping the first terminal reason.
fn fail(lifecycle: &mut JobLifecycle, code: HostFailureCode, reason: &str) {
    if lifecycle.state().is_terminal() {
        return;
    }
    lifecycle
        .host_failed(&lifecycle.identity().clone(), host_failure(code, reason))
        .expect("same attempt, nonterminal lifecycle");
}

/// Conclude the lifecycle once the worker has been reaped.
fn reaped(lifecycle: &mut JobLifecycle, clean: bool) {
    if lifecycle.state() == JobState::Cancelling {
        lifecycle
            .host_cancelled(
                &lifecycle.identity().clone(),
                &lifecycle.cancellation_token().clone(),
            )
            .expect("same cancelling attempt, worker has been reaped");
    } else if !lifecycle.state().is_terminal()
        && (!clean || lifecycle.state() != JobState::Validating)
    {
        fail(
            lifecycle,
            HostFailureCode::WorkerExited,
            "worker exited without a clean candidate",
        );
    }
}

fn concluded(lifecycle: &JobLifecycle) -> RunResult {
    match (lifecycle.state(), lifecycle.failure()) {
        (JobState::Cancelled, _) => RunResult::Cancelled,
        (_, Some(failure)) => RunResult::Failed(failure.clone()),
        _ => RunResult::Failed(JobFailure::Host(host_failure(
            HostFailureCode::WorkerExited,
            "worker exited without a clean candidate",
        ))),
    }
}

fn write_new(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = std::fs::File::create_new(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

fn workspace_artifact(
    reference: &str,
    bytes: &[u8],
    sha256: &deadpan_jobs::Sha256,
) -> Result<WorkspaceArtifact, String> {
    WorkspaceArtifact::new(
        WorkspaceRef::new(reference).map_err(|error| error.to_string())?,
        sha256.clone(),
        bytes.len() as u64,
    )
    .map_err(|error| error.to_string())
}

/// A private attempt workspace: `runtime.json` beside `worker/{inputs,outputs}`,
/// pinned, with the conditioning inputs captured outside the worker's control.
pub(super) struct PreparedWorkspace {
    pub(super) directory: tempfile::TempDir,
    pub(super) worker: std::path::PathBuf,
    pub(super) runtime_config: Option<std::path::PathBuf>,
    pub(super) pinned: ArtifactWorkspace,
    pub(super) conditioning: deadpan_models::RetainedConditioning,
}

pub(super) fn prepare_workspace(
    allocated: &Allocated,
    runtime_config: Option<&[u8]>,
    cancelled: &AtomicBool,
) -> Result<PreparedWorkspace, String> {
    let directory = tempfile::Builder::new()
        .prefix("deadpan-ai-hold-")
        .tempdir()
        .map_err(|error| error.to_string())?;
    let worker = directory.path().join("worker");
    let io = |error: std::io::Error| error.to_string();
    std::fs::create_dir(&worker).map_err(io)?;
    std::fs::create_dir(worker.join(INPUT_SCOPE)).map_err(io)?;
    std::fs::create_dir(worker.join(OUTPUT_SCOPE)).map_err(io)?;
    let inputs = &allocated.inputs;
    write_new(&worker.join(LEFT), &inputs.left_png).map_err(io)?;
    write_new(&worker.join(RIGHT), &inputs.right_png).map_err(io)?;
    write_new(&worker.join(MANIFEST), &inputs.manifest).map_err(io)?;
    let runtime_config = match runtime_config {
        Some(bytes) => {
            let path = directory.path().join("runtime.json");
            write_new(&path, bytes).map_err(io)?;
            Some(path)
        }
        None => None,
    };
    // Pin before launch; capture the inputs outside the worker's control.
    let pinned = ArtifactWorkspace::open(&worker).map_err(|error| error.to_string())?;
    let manifest = workspace_artifact(MANIFEST, &inputs.manifest, &inputs.manifest_sha256)?;
    let conditioning = capture_bridge_conditioning(
        &pinned,
        &allocated.host_message,
        &manifest,
        &WorkspaceRef::new(INPUT_SCOPE).map_err(|error| error.to_string())?,
        conditioning_limits(),
        cancelled,
    )
    .map_err(|error| error.to_string())?;
    Ok(PreparedWorkspace {
        directory,
        worker,
        runtime_config,
        pinned,
        conditioning,
    })
}

/// Run the worker for `allocated` and qualify its bundle. Blocks for the whole
/// attempt; call it on a job thread. Never touches the store: durable
/// transitions go to `records` in order, and an `Err` from it cancels the
/// worker and fails the attempt.
pub fn run_worker(
    allocated: &Allocated,
    runtime: &BridgeRuntime,
    mut progress: impl FnMut(AttemptProgress),
    mut records: impl FnMut(AttemptRecord) -> Result<(), String>,
    cancelled: &AtomicBool,
) -> WorkerRun {
    let mut timings = RunTimings::default();
    let started = Instant::now();
    let identity = &allocated.identity;
    let token = &allocated.cancellation_token;
    let mut lifecycle = JobLifecycle::new_with_protocol(
        identity.clone(),
        token.clone(),
        TargetBinding {
            project_id: allocated.request.binding.project_id.clone(),
            hold_id: allocated.request.binding.hold_id.clone(),
            request_version: allocated.request.binding.request_version,
            context_sha256: allocated.inputs.manifest_sha256.clone(),
        },
        ProtocolVersion::V2,
    );
    progress(AttemptProgress::Preparing);
    let cancel_early = |records: &mut dyn FnMut(AttemptRecord) -> Result<(), String>| {
        // Cancelled before launch: nothing to reap. `finish` concludes it.
        let _ = records(AttemptRecord::CancelRequested);
        RunResult::Cancelled
    };
    if cancelled.load(Ordering::Acquire) {
        return WorkerRun::early(cancel_early(&mut records), timings);
    }

    let prepared = prepare_workspace(allocated, Some(&runtime.worker_configuration()), cancelled);
    timings.preparation = started.elapsed();
    let (directory, worker, runtime_config, pinned, conditioning) = match prepared {
        Ok(prepared) => (
            prepared.directory,
            prepared.worker,
            prepared
                .runtime_config
                .expect("a real worker run writes its runtime configuration"),
            prepared.pinned,
            prepared.conditioning,
        ),
        Err(_) if cancelled.load(Ordering::Acquire) => {
            return WorkerRun::early(cancel_early(&mut records), timings);
        }
        Err(error) => {
            return WorkerRun::early(
                RunResult::Failed(JobFailure::Host(host_failure(
                    HostFailureCode::Io,
                    &format!("could not prepare the AI pause inputs: {error}"),
                ))),
                timings,
            );
        }
    };

    let environment = [
        ("HF_HUB_OFFLINE", "1"),
        ("TRANSFORMERS_OFFLINE", "1"),
        ("HF_HUB_DISABLE_TELEMETRY", "1"),
        ("DO_NOT_TRACK", "1"),
        ("PYTHONNOUSERSITE", "1"),
        ("PYTHONUNBUFFERED", "1"),
        ("TOKENIZERS_PARALLELISM", "false"),
    ]
    .into_iter()
    .map(|(key, value)| (key.into(), value.into()))
    .collect::<BTreeMap<_, _>>();
    let launched = Instant::now();
    let mut process = match WorkerProcess::spawn(
        ProcessSpec {
            executable: runtime.python.clone(),
            // -B: the bundled runtime is signed and read-only; its bytecode
            // is precompiled.
            arguments: vec![
                "-I".into(),
                "-B".into(),
                runtime.worker_script.clone().into_os_string(),
                "--runtime-config".into(),
                runtime_config.into_os_string(),
            ],
            environment,
            workspace: worker,
            limits: ProcessLimits {
                maximum_duration: WORKER_DEADLINE,
                cancellation_grace: CANCELLATION_GRACE,
                exit_grace: EXIT_GRACE,
            },
        },
        allocated.host_message.clone(),
    ) {
        Ok(process) => process,
        Err(error) => {
            let mut run = WorkerRun::early(
                RunResult::Failed(JobFailure::Host(host_failure(
                    HostFailureCode::SpawnFailed,
                    &format!("could not start the AI worker: {error}"),
                ))),
                timings,
            );
            run._directory = Some(directory);
            return run;
        }
    };

    let mut cancellation_sent = false;
    let mut clean_exit = false;
    let mut faulted = false;
    let cancel = |lifecycle: &mut JobLifecycle,
                  process: &mut WorkerProcess,
                  records: &mut dyn FnMut(AttemptRecord) -> Result<(), String>,
                  sent: &mut bool| {
        if *sent || lifecycle.state().is_terminal() {
            return;
        }
        *sent = true;
        let _ = lifecycle.request_cancel(&identity.clone(), &token.clone());
        let _ = process.request_cancel(Instant::now());
        let _ = records(AttemptRecord::CancelRequested);
    };
    while !process.is_finished() {
        if cancelled.load(Ordering::Acquire) {
            cancel(
                &mut lifecycle,
                &mut process,
                &mut records,
                &mut cancellation_sent,
            );
        }
        let events = match process.poll(Instant::now()) {
            Ok(events) => events,
            Err(error) => {
                // Supervision lost the worker: stop and reap it explicitly.
                let cleanup = process.finish_owned_work(Instant::now() + EXIT_GRACE);
                let detail = match cleanup {
                    Ok(_) => format!("worker supervision failed: {error}"),
                    Err(cleanup) => format!(
                        "worker supervision failed: {error}; cleanup unconfirmed: {cleanup}"
                    ),
                };
                fail(&mut lifecycle, HostFailureCode::WorkerExited, &detail);
                faulted = true;
                break;
            }
        };
        for event in events {
            match event {
                ProcessEvent::Message(message) => {
                    if lifecycle.state().is_terminal() {
                        continue;
                    }
                    if let Err(error) = lifecycle.apply_worker_message(&message) {
                        fail(
                            &mut lifecycle,
                            HostFailureCode::ProtocolViolation,
                            &error.to_string(),
                        );
                        let _ = process.request_cancel(Instant::now());
                        continue;
                    }
                    match message.as_ref() {
                        WorkerMessage::Progress {
                            stage,
                            progress: step,
                            ..
                        } => {
                            progress(AttemptProgress::Step {
                                stage: *stage,
                                completed: step.completed(),
                                total: step.total(),
                            });
                            continue;
                        }
                        WorkerMessage::Stage { stage, .. } => {
                            progress(AttemptProgress::Stage(*stage));
                        }
                        WorkerMessage::CompletedBridge { candidate, .. } => {
                            let HostMessage::GenerateBridge { plan, provider, .. } =
                                &allocated.host_message
                            else {
                                unreachable!("allocation builds a bridge request");
                            };
                            let dimensions = plan.native_dimensions();
                            if candidate.video.frames().frames()
                                != i64::from(plan.native_frame_count())
                                || candidate.video.frame_rate() != plan.native_frame_rate()
                                || candidate.video.width() != dimensions.width()
                                || candidate.video.height() != dimensions.height()
                                || &candidate.provider != provider.as_ref()
                            {
                                fail(
                                    &mut lifecycle,
                                    HostFailureCode::OutputValidationFailed,
                                    "native candidate differs from the request plan or provider",
                                );
                                continue;
                            }
                        }
                        _ => {}
                    }
                    if let Err(error) = records(AttemptRecord::Worker(message)) {
                        fail(
                            &mut lifecycle,
                            HostFailureCode::Io,
                            &format!("could not save the attempt's progress: {error}"),
                        );
                        let _ = process.request_cancel(Instant::now());
                    }
                }
                ProcessEvent::Fault(reason) => {
                    faulted = true;
                    fail(&mut lifecycle, HostFailureCode::WorkerExited, &reason);
                }
                ProcessEvent::Exited {
                    status,
                    cancellation_escalated,
                } => {
                    clean_exit = status.success() && !cancellation_escalated;
                    reaped(&mut lifecycle, clean_exit);
                }
            }
        }
        if !process.is_finished() {
            thread::park_timeout(Duration::from_millis(10));
        }
    }
    timings.worker = launched.elapsed();
    let logs = process.log_tail();
    drop(process);
    let worker_log = String::from_utf8_lossy(&logs.bytes).into_owned();

    let result = if clean_exit && !faulted && lifecycle.state() == JobState::Validating {
        let declaration = lifecycle
            .candidate_bundle()
            .cloned()
            .expect("a validating bridge attempt has a declared bundle");
        progress(AttemptProgress::Qualifying);
        let qualifying = Instant::now();
        let qualified = qualify_declared(
            allocated,
            &runtime.media_worker,
            &pinned,
            conditioning,
            declaration,
            cancelled,
        );
        timings.qualification = qualifying.elapsed();
        match qualified {
            // A cancel that lands after the worker exited still wins: the
            // user asked for no candidate.
            _ if cancelled.load(Ordering::Acquire) => cancel_early(&mut records),
            Ok(run) => RunResult::Qualified(Box::new(run)),
            Err(error) => RunResult::Failed(JobFailure::Host(host_failure(
                HostFailureCode::OutputValidationFailed,
                &error,
            ))),
        }
    } else {
        concluded(&lifecycle)
    };
    WorkerRun {
        result,
        timings,
        worker_log,
        worker_log_discarded_bytes: logs.discarded_bytes,
        _directory: Some(directory),
    }
}

/// Qualify a clean worker's declared bundle with the media worker after
/// teardown: provenance, both masters and the captured conditioning inputs.
pub(super) fn qualify_declared(
    allocated: &Allocated,
    media_worker: &Path,
    pinned: &ArtifactWorkspace,
    conditioning: deadpan_models::RetainedConditioning,
    declaration: NativeCandidateManifest,
    cancelled: &AtomicBool,
) -> Result<QualifiedRun, String> {
    let selected = SelectedBridgeProvider::new(
        declaration.provider.clone(),
        super::development_capability(),
    );
    let bundle = qualify_bridge(
        media_worker,
        pinned,
        BridgeQualification {
            request: &allocated.host_message,
            declaration: &declaration,
            selected_provider: &selected,
            conditioning,
        },
        qualification_limits(),
        cancelled,
    )
    .map_err(|error| error.to_string())?;
    let receipt = validation_receipt(&bundle, &declaration)?;
    Ok(QualifiedRun {
        bundle,
        receipt,
        declaration,
    })
}

/// The host's validation receipt for a qualified bundle, with its measured
/// spans and retained input objects.
fn validation_receipt(
    bundle: &QualifiedBridgeBundle,
    declaration: &NativeCandidateManifest,
) -> Result<BundleValidationReceipt, String> {
    let binding = bundle.binding();
    let inputs = bundle.conditioning().receipt();
    let text = |error: deadpan_store::generation_attempts::AttemptValueError| error.to_string();
    let admission = BundleAdmissionEvidence::new(
        bundle.native_span(),
        bundle.sampled_span(),
        BundleInputObjects::new(
            binding.input.sha256.clone(),
            inputs.manifest().object().clone(),
            inputs.left().object().clone(),
            inputs.right().object().clone(),
        )
        .map_err(text)?,
    )
    .map_err(text)?;
    BundleValidationReceipt::new(
        declaration,
        bundle.native().object().clone(),
        bundle.sampled().object().clone(),
        bundle.provenance().object().clone(),
        binding.constraints.video.clone(),
        binding.plan.clone(),
        ValidatorIdentity::new("native-ffv1", "bridge-3").map_err(text)?,
    )
    .map_err(text)?
    .with_admission(admission)
    .map_err(text)
}

/// The attempt's durable outcome.
#[derive(Debug, Clone)]
pub struct Finished {
    pub state: JobState,
    pub receipt: Option<BundleValidationReceipt>,
    pub failure: Option<JobFailure>,
    /// Object publication and Ready recording.
    pub publication: Duration,
}

/// Record `run`'s outcome: publish the six objects and record Ready, or the
/// failure or cancellation. A publication failure is recorded as a host
/// failure before the error is returned.
pub fn finish(
    store: &mut ProjectStore,
    allocated: &Allocated,
    run: WorkerRun,
) -> Result<Finished, GenerationError> {
    let identity = &allocated.identity;
    let started = Instant::now();
    let stored_state = |store: &ProjectStore| -> Result<JobState, GenerationError> {
        Ok(store
            .generation_attempt(identity)?
            .ok_or_else(|| invalid("the attempt is missing from the project"))?
            .checkpoint
            .state)
    };
    match run.result {
        RunResult::Qualified(qualified) => {
            let QualifiedRun {
                bundle,
                receipt,
                declaration,
            } = *qualified;
            let published = (|| -> Result<(), StoreError> {
                let limits = object_limits();
                let (mut native, mut sampled, mut provenance, conditioning) = bundle.into_parts();
                store.promote_generated_object(&mut native, receipt.native_object(), limits)?;
                store.promote_generated_object(&mut sampled, receipt.sampled_object(), limits)?;
                store.promote_generated_object(
                    &mut provenance,
                    receipt.provenance_object(),
                    limits,
                )?;
                let (manifest, left, right) = conditioning.into_parts();
                for mut input in [manifest, left, right] {
                    let object = input.object().clone();
                    store.promote_generated_object(&mut input, &object, limits)?;
                }
                store.record_generation_bundle_ready(
                    identity,
                    &declaration,
                    receipt.clone(),
                    limits,
                )?;
                Ok(())
            })();
            if let Err(error) = published {
                if !stored_state(store)?.is_terminal() {
                    store.fail_generation_attempt(
                        identity,
                        host_failure(
                            HostFailureCode::OutputValidationFailed,
                            &format!("could not publish the AI pictures: {error}"),
                        ),
                    )?;
                }
                return Err(error.into());
            }
            Ok(Finished {
                state: JobState::Ready,
                receipt: Some(receipt),
                failure: None,
                publication: started.elapsed(),
            })
        }
        RunResult::Failed(failure) => {
            if !stored_state(store)?.is_terminal() {
                let host = match &failure {
                    JobFailure::Host(host) => host.clone(),
                    JobFailure::Worker(worker) => host_failure(
                        HostFailureCode::WorkerExited,
                        &format!("{:?}: {}", worker.code, worker.detail.as_str()),
                    ),
                };
                store.fail_generation_attempt(identity, host)?;
            }
            let stored = store
                .generation_attempt(identity)?
                .ok_or_else(|| invalid("the attempt is missing from the project"))?;
            Ok(Finished {
                state: stored.checkpoint.state,
                receipt: None,
                failure: stored.checkpoint.failure.or(Some(failure)),
                publication: started.elapsed(),
            })
        }
        RunResult::Cancelled => {
            let state = stored_state(store)?;
            if !state.is_terminal() {
                if state != JobState::Cancelling {
                    store.request_generation_attempt_cancel(
                        identity,
                        &allocated.cancellation_token,
                    )?;
                }
                store
                    .finish_generation_attempt_cancelled(identity, &allocated.cancellation_token)?;
            }
            Ok(Finished {
                state: stored_state(store)?,
                receipt: None,
                failure: None,
                publication: started.elapsed(),
            })
        }
    }
}

#[cfg(any(test, feature = "synthetic-worker"))]
pub mod synthetic;

#[cfg(test)]
mod tests;
