//! One supervised AI Hold attempt, split into steps so a host can keep store
//! writes on its writer thread and the long worker run on a job thread:
//!
//! 1. [`allocate`] (writer): record the request and begin its attempt.
//! 2. [`run_worker`] (job thread, no store): write the conditioning inputs into
//!    a fresh private workspace, pin it, capture the inputs, launch and
//!    supervise the worker, then qualify its bundle after clean teardown.
//!    Durable lifecycle transitions are handed to the caller as
//!    [`AttemptRecord`]s, which it applies with [`record`] on the writer;
//!    progress stays in memory.
//! 3. [`finish`] (writer): publish every retained object and record Ready, or record
//!    the failure or cancellation truthfully.
//!
//! Nothing here edits the project; acceptance is a separate explicit edit.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use deadpan_core::{NodeId, RevisionId, ScopedNodeTarget};
use deadpan_jobs::artifact::ArtifactWorkspace;
use deadpan_jobs::supervisor::{ProcessEvent, ProcessLimits, ProcessSpec, WorkerProcess};
use deadpan_jobs::{
    AttemptId, CancellationToken, ContextArtifact, Diagnostic, GenerationPlan, HoldTarget,
    HostFailure, HostFailureCode, HostMessage, JobFailure, JobLifecycle, JobState, MessageIdentity,
    NativeCandidateManifest, ProtocolVersion, RequestId, TargetBinding, WorkerMessage, WorkerStage,
    WorkspaceArtifact, WorkspaceRef,
};
use deadpan_media::protocol::ConversionLimits;
use deadpan_models::{
    BridgeQualification, ConditioningLimits, ExtensionQualification, QualificationLimits,
    QualifiedBridgeBundle, QualifiedExtensionBundle, SelectedBridgeProvider,
    SelectedExtensionProvider, capture_bridge_conditioning, capture_extension_conditioning,
    qualify_bridge, qualify_extension,
};
use deadpan_store::ProjectStore;
use deadpan_store::StoreError;
use deadpan_store::generated_media::GeneratedMediaLimits;
use deadpan_store::generation::{GenerationRequestInput, StoredGenerationRequest};
use deadpan_store::generation_attempts::{
    BeginGenerationAttempt, BundleAdmissionEvidence, BundleInputObjects, BundleValidationReceipt,
    ValidatorIdentity,
};

use super::conditioning::{LEFT, MANIFEST, PreparedInputs, RIGHT};
use super::runtime::{BridgeRuntime, SelectedGenerationProvider, WorkerMode};

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
        // Retained raw landmark observations can exceed 1 MiB for several
        // faces across the supported 97 native pictures. Match stored admission.
        maximum_host_provenance_bytes: 32 * 1024 * 1024,
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
    pub inputs: PreparedInputs,
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
    inputs: PreparedInputs,
}

impl Allocated {
    /// The conditioning inputs this attempt's request was recorded with.
    /// Every later variant of the request reuses them unchanged.
    pub fn inputs(&self) -> &PreparedInputs {
        &self.inputs
    }

    /// The 1-based attempt ordinal within the request.
    pub fn ordinal(&self) -> u64 {
        self.ordinal
    }

    /// The exact provider of this attempt, a seeded variant of the request.
    pub fn provider(&self) -> &deadpan_jobs::ProviderSelection {
        match &self.host_message {
            HostMessage::GenerateBridge { provider, .. }
            | HostMessage::GenerateExtension { provider, .. } => provider,
            _ => unreachable!("allocation builds a planned generation request"),
        }
    }

    pub fn protocol(&self) -> ProtocolVersion {
        match &self.inputs {
            PreparedInputs::Bridge(_) => ProtocolVersion::V2,
            PreparedInputs::Extension(_) => ProtocolVersion::V3,
        }
    }
}

/// Record a bridge request for `input.hold` and begin its first attempt.
/// A new request makes the Hold's previous requests stale.
pub fn allocate(
    store: &mut ProjectStore,
    input: AllocateInput,
) -> Result<Allocated, GenerationError> {
    if !matches!(input.inputs, PreparedInputs::Bridge(_)) {
        return Err(invalid(
            "Extension allocation requires an explicitly selected provider.",
        ));
    }
    let provider = super::development_provider(input.seed);
    allocate_with_provider(store, input, provider)
}

/// Record a new request with the provider captured by its selected runtime.
/// This identity becomes durable request and candidate provenance, so a later
/// pack rollback cannot change what an existing request claims to use.
pub fn allocate_with_provider(
    store: &mut ProjectStore,
    input: AllocateInput,
    provider: deadpan_jobs::ProviderSelection,
) -> Result<Allocated, GenerationError> {
    let target = ScopedNodeTarget {
        node: input.hold.clone(),
        repeats: Vec::new(),
    };
    allocate_scoped_with_provider(store, input, target, provider)
}

/// Allocate one explicit authoring scope. Other plays sharing its physical
/// Hold retain their own request, version clock and chosen Ready variant.
pub fn allocate_scoped_with_provider(
    store: &mut ProjectStore,
    input: AllocateInput,
    target: ScopedNodeTarget,
    mut provider: deadpan_jobs::ProviderSelection,
) -> Result<Allocated, GenerationError> {
    let request_id =
        RequestId::new(format!("ai-hold-{}", uuid::Uuid::new_v4().simple())).map_err(invalid)?;
    provider.seed = input.seed;
    let request = store.record_scoped_generation_request(
        GenerationRequestInput {
            request_id: request_id.clone(),
            expected_revision: input.expected_revision.clone(),
            hold_id: input.hold.clone(),
            context_sha256: input.inputs.manifest_sha256().clone(),
            constraints: input.inputs.constraints().clone(),
            provider,
        },
        target,
        input.inputs.plan(),
    )?;
    allocate_variant(store, request, input.inputs)
}

/// Admit one claimed replacement preparation, its fully bound request and its
/// first attempt in one store transaction. A stale claim creates none of them.
pub fn allocate_preparation_with_provider(
    store: &mut ProjectStore,
    claim: &deadpan_store::generation_preparations::PreparationClaim,
    inputs: PreparedInputs,
    provider: deadpan_jobs::ProviderSelection,
) -> Result<Allocated, GenerationError> {
    let request_id =
        RequestId::new(format!("ai-hold-{}", uuid::Uuid::new_v4().simple())).map_err(invalid)?;
    let identity = MessageIdentity::new(
        request_id.clone(),
        AttemptId::new(uuid::Uuid::new_v4().simple().to_string()).map_err(invalid)?,
    );
    let cancellation_token =
        CancellationToken::new(uuid::Uuid::new_v4().simple().to_string()).map_err(invalid)?;
    let preparation = &claim.preparation;
    let (request, begun) = store.fulfil_generation_preparation(
        claim,
        GenerationRequestInput {
            request_id,
            expected_revision: preparation.current_revision.clone(),
            hold_id: preparation.target.node.clone(),
            context_sha256: inputs.manifest_sha256().clone(),
            constraints: inputs.constraints().clone(),
            provider,
        },
        inputs.plan(),
        BeginGenerationAttempt {
            identity: identity.clone(),
            cancellation_token: cancellation_token.clone(),
        },
    )?;
    allocated(request, inputs, identity, cancellation_token, begun.ordinal)
}

/// The Hold's current bridge request, whose attempts are its variants.
pub fn current_bridge_request(
    store: &ProjectStore,
    hold: &NodeId,
) -> Result<Option<StoredGenerationRequest>, StoreError> {
    current_scoped_bridge_request(
        store,
        &ScopedNodeTarget {
            node: hold.clone(),
            repeats: Vec::new(),
        },
    )
}

pub fn current_scoped_bridge_request(
    store: &ProjectStore,
    target: &ScopedNodeTarget,
) -> Result<Option<StoredGenerationRequest>, StoreError> {
    Ok(current_scoped_request(store, target)?.filter(|request| request.bridge_plan().is_some()))
}

pub fn current_request(
    store: &ProjectStore,
    hold: &NodeId,
) -> Result<Option<StoredGenerationRequest>, StoreError> {
    current_scoped_request(
        store,
        &ScopedNodeTarget {
            node: hold.clone(),
            repeats: Vec::new(),
        },
    )
}

pub fn current_scoped_request(
    store: &ProjectStore,
    target: &ScopedNodeTarget,
) -> Result<Option<StoredGenerationRequest>, StoreError> {
    Ok(store
        .current_generation_requests()?
        .into_iter()
        .find(|request| &request.target == target && request.plan.is_some()))
}

/// Begin another attempt of an existing current `request`: a new seeded
/// variant ([`deadpan_jobs::ProviderSelection::for_attempt`]) with the
/// request's exact constraints, plan and conditioning `inputs`. Earlier Ready
/// variants stay available for explicit selection.
pub fn allocate_variant(
    store: &mut ProjectStore,
    request: StoredGenerationRequest,
    inputs: PreparedInputs,
) -> Result<Allocated, GenerationError> {
    require_planned_request(&request)?;
    if inputs.manifest_sha256() != &request.binding.context_sha256
        || inputs.constraints() != &request.constraints
        || request.plan.as_ref() != Some(&inputs.plan())
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
    allocated(request, inputs, identity, cancellation_token, begun.ordinal)
}

fn allocated(
    request: StoredGenerationRequest,
    inputs: PreparedInputs,
    identity: MessageIdentity,
    cancellation_token: CancellationToken,
    ordinal: u64,
) -> Result<Allocated, GenerationError> {
    require_planned_request(&request)?;
    let provider = request.provider.for_attempt(ordinal);
    let target = HoldTarget {
        hold_id: request.binding.hold_id.clone(),
        request_version: request.binding.request_version,
    };
    let input = ContextArtifact {
        manifest: WorkspaceRef::new(MANIFEST).map_err(invalid)?,
        sha256: inputs.manifest_sha256().clone(),
    };
    let output_workspace = WorkspaceRef::new(OUTPUT_SCOPE).map_err(invalid)?;
    let host_message = match inputs.plan() {
        GenerationPlan::Bridge(plan) => HostMessage::GenerateBridge {
            protocol: ProtocolVersion::V2,
            identity: identity.clone(),
            cancellation_token: cancellation_token.clone(),
            project_id: request.binding.project_id.clone(),
            revision_id: request.origin_revision.clone(),
            target,
            input,
            output_workspace,
            constraints: inputs.constraints().clone(),
            provider: Box::new(provider),
            plan: Box::new(plan),
        },
        GenerationPlan::Extension(plan) => HostMessage::GenerateExtension {
            protocol: ProtocolVersion::V3,
            identity: identity.clone(),
            cancellation_token: cancellation_token.clone(),
            project_id: request.binding.project_id.clone(),
            revision_id: request.origin_revision.clone(),
            target,
            input,
            output_workspace,
            constraints: inputs.constraints().clone(),
            provider: Box::new(provider),
            plan: Box::new(plan),
        },
    };
    host_message.validate().map_err(invalid)?;
    Ok(Allocated {
        request,
        identity,
        cancellation_token,
        host_message,
        ordinal,
        inputs,
    })
}

fn require_planned_request(request: &StoredGenerationRequest) -> Result<(), GenerationError> {
    if request
        .plan
        .as_ref()
        .is_none_or(|plan| plan.conditioning() != request.constraints.conditioning)
    {
        return Err(invalid(
            "Generation requires a retained plan matching its resolved conditioning operation.",
        ));
    }
    Ok(())
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
    pub bundle: QualifiedBundle,
    pub receipt: BundleValidationReceipt,
    pub declaration: NativeCandidateManifest,
}

pub enum QualifiedBundle {
    Bridge(Box<QualifiedBridgeBundle>),
    Extension(Box<QualifiedExtensionBundle>),
}

impl QualifiedBundle {
    fn into_parts(
        self,
    ) -> (
        deadpan_media::CanonicalMedia,
        deadpan_media::CanonicalMedia,
        deadpan_models::QualifiedProvenance,
        Vec<deadpan_models::ConditioningObject>,
    ) {
        match self {
            Self::Bridge(bundle) => {
                let (native, sampled, provenance, conditioning) = (*bundle).into_parts();
                let (manifest, left, right) = conditioning.into_parts();
                (native, sampled, provenance, vec![manifest, left, right])
            }
            Self::Extension(bundle) => {
                let (native, sampled, provenance, conditioning) = (*bundle).into_parts();
                let (manifest, context, opposite, signatures) = conditioning.into_parts();
                let inputs = std::iter::once(manifest)
                    .chain(context)
                    .chain(opposite)
                    .chain(std::iter::once(signatures))
                    .collect();
                (native, sampled, provenance, inputs)
            }
        }
    }
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

fn native_frame_count(plan: &GenerationPlan) -> u32 {
    match plan {
        GenerationPlan::Bridge(plan) => plan.native_frame_count(),
        GenerationPlan::Extension(plan) => plan.native_frame_count(),
    }
}

/// A candidate's operation, timing and provider must match the host's retained
/// plan before its completion can be acknowledged to the store.
fn completion_matches(
    allocated: &Allocated,
    message: &WorkerMessage,
    candidate: &NativeCandidateManifest,
) -> bool {
    let plan = allocated.inputs.plan();
    let operation_matches = matches!(
        (&plan, message),
        (
            GenerationPlan::Bridge(_),
            WorkerMessage::CompletedBridge {
                protocol: ProtocolVersion::V2,
                ..
            }
        ) | (
            GenerationPlan::Extension(_),
            WorkerMessage::CompletedExtension {
                protocol: ProtocolVersion::V3,
                ..
            }
        )
    );
    let dimensions = plan.native_dimensions();
    operation_matches
        && candidate.video.frames().frames() == i64::from(native_frame_count(&plan))
        && candidate.video.frame_rate() == plan.native_frame_rate()
        && candidate.video.width() == dimensions.width()
        && candidate.video.height() == dimensions.height()
        && &candidate.provider == allocated.provider()
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
    pub(super) conditioning: RetainedInputs,
}

/// Each immutable capture keeps the independently selected capability that
/// admitted it, rather than reconstructing authority from worker output.
pub(super) enum RetainedInputs {
    Bridge {
        selected: SelectedBridgeProvider,
        inputs: Box<deadpan_models::RetainedConditioning>,
    },
    Extension {
        selected: SelectedExtensionProvider,
        inputs: Box<deadpan_models::RetainedExtensionConditioning>,
    },
}

pub(super) fn prepare_workspace(
    allocated: &Allocated,
    runtime_config: Option<&[u8]>,
    selected: SelectedGenerationProvider,
    cancelled: &AtomicBool,
) -> Result<PreparedWorkspace, String> {
    if selected.selection() != allocated.provider() {
        return Err("The selected runtime provider differs from the allocated attempt.".into());
    }
    match (&allocated.inputs, &selected) {
        (PreparedInputs::Bridge(inputs), SelectedGenerationProvider::Bridge(provider)) => inputs
            .plan
            .validate_for(provider.capability())
            .map_err(|error| error.to_string())?,
        (PreparedInputs::Extension(inputs), SelectedGenerationProvider::Extension(provider)) => {
            inputs
                .plan
                .validate_for(provider.capability())
                .map_err(|error| error.to_string())?
        }
        _ => return Err("The selected runtime operation differs from the prepared inputs.".into()),
    }
    validate_input_sizes(&allocated.inputs)?;
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
    match inputs {
        PreparedInputs::Bridge(inputs) => {
            write_new(&worker.join(LEFT), &inputs.left_png).map_err(io)?;
            write_new(&worker.join(RIGHT), &inputs.right_png).map_err(io)?;
        }
        PreparedInputs::Extension(inputs) => {
            for (index, png) in inputs.context_pngs.iter().enumerate() {
                write_new(&worker.join(format!("inputs/context-{index:03}.png")), png)
                    .map_err(io)?;
            }
            if let Some(png) = &inputs.opposite_png {
                write_new(&worker.join("inputs/opposite.png"), png).map_err(io)?;
            }
            write_new(
                &worker.join("inputs/continuity.bin"),
                &inputs.continuity_signatures,
            )
            .map_err(io)?;
        }
    }
    write_new(&worker.join(MANIFEST), inputs.manifest()).map_err(io)?;
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
    let manifest = workspace_artifact(MANIFEST, inputs.manifest(), inputs.manifest_sha256())?;
    let scope = WorkspaceRef::new(INPUT_SCOPE).map_err(|error| error.to_string())?;
    let conditioning = match selected {
        SelectedGenerationProvider::Bridge(selected) => RetainedInputs::Bridge {
            selected,
            inputs: Box::new(
                capture_bridge_conditioning(
                    &pinned,
                    &allocated.host_message,
                    &manifest,
                    &scope,
                    conditioning_limits(),
                    cancelled,
                )
                .map_err(|error| error.to_string())?,
            ),
        },
        SelectedGenerationProvider::Extension(selected) => RetainedInputs::Extension {
            selected,
            inputs: Box::new(
                capture_extension_conditioning(
                    &pinned,
                    &allocated.host_message,
                    &manifest,
                    &scope,
                    conditioning_limits(),
                    cancelled,
                )
                .map_err(|error| error.to_string())?,
            ),
        },
    };
    Ok(PreparedWorkspace {
        directory,
        worker,
        runtime_config,
        pinned,
        conditioning,
    })
}

/// Bound bytes before writing, including bytes which a malformed manifest might
/// otherwise omit. The pinned capture then checks every declared hash and path.
fn validate_input_sizes(inputs: &PreparedInputs) -> Result<(), String> {
    let limits = conditioning_limits();
    let bounded = |length: usize, maximum: u64| {
        u64::try_from(length).is_ok_and(|length| length > 0 && length <= maximum)
    };
    if !bounded(inputs.manifest().len(), limits.maximum_manifest_bytes) {
        return Err("The conditioning manifest exceeds the capture bounds.".into());
    }
    match inputs {
        PreparedInputs::Bridge(inputs) => {
            if !bounded(inputs.left_png.len(), limits.maximum_frame_bytes)
                || !bounded(inputs.right_png.len(), limits.maximum_frame_bytes)
            {
                return Err("A Bridge conditioning picture exceeds the capture bounds.".into());
            }
        }
        PreparedInputs::Extension(inputs) => {
            let context: deadpan_models::ExtensionContext =
                serde_json::from_slice(&inputs.manifest).map_err(|error| error.to_string())?;
            if inputs.context_pngs.len() != inputs.plan.context_frame_count() as usize
                || inputs.context_pngs.len() > deadpan_models::MAXIMUM_EXTENSION_CONTEXT_FRAMES
                || context.plan() != &inputs.plan
                || context.context().len() != inputs.context_pngs.len()
                || matches!(
                    context.opposite(),
                    deadpan_models::ExtensionOppositeSeam::PresentUnconditioned { .. }
                ) != inputs.opposite_png.is_some()
            {
                return Err("The Extension input objects differ from the manifest or plan.".into());
            }
            let mut total = 0_u64;
            for png in inputs.context_pngs.iter().chain(inputs.opposite_png.iter()) {
                let length = u64::try_from(png.len()).map_err(|error| error.to_string())?;
                total = total
                    .checked_add(length)
                    .filter(|total| {
                        length > 0 && *total <= deadpan_models::MAXIMUM_EXTENSION_INPUT_BYTES
                    })
                    .ok_or(
                        "Extension conditioning pictures exceed the aggregate capture bounds.",
                    )?;
            }
            if !bounded(
                inputs.continuity_signatures.len(),
                deadpan_analysis::MAX_CONTEXT_SIGNATURE_BYTES as u64,
            ) {
                return Err("Extension continuity signatures exceed the capture bounds.".into());
            }
        }
    }
    Ok(())
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
            context_sha256: allocated.inputs.manifest_sha256().clone(),
        },
        allocated.protocol(),
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

    let prepared = super::runtime::validate_constraints_for_manifest(
        &runtime.model_manifest,
        allocated.inputs.constraints(),
    )
    .and_then(|()| runtime.selected_provider(&allocated.inputs.plan(), allocated.provider().seed))
    .and_then(|selected| {
        prepare_workspace(
            allocated,
            Some(&runtime.worker_configuration()),
            selected,
            cancelled,
        )
    });
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
    let launch = match runtime.worker_launch(&runtime_config, WorkerMode::Inference) {
        Ok(launch) => launch,
        Err(error) => {
            let mut run = WorkerRun::early(
                RunResult::Failed(JobFailure::Host(host_failure(
                    HostFailureCode::SpawnFailed,
                    &format!("could not isolate the AI worker: {error}"),
                ))),
                timings,
            );
            run._directory = Some(directory);
            return run;
        }
    };
    let launched = Instant::now();
    let mut process = match WorkerProcess::spawn(
        ProcessSpec {
            executable: launch.executable,
            arguments: launch.arguments,
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
                        WorkerMessage::CompletedBridge { candidate, .. }
                        | WorkerMessage::CompletedExtension { candidate, .. }
                            if !completion_matches(allocated, &message, candidate) =>
                        {
                            fail(
                                &mut lifecycle,
                                HostFailureCode::OutputValidationFailed,
                                "native candidate differs from the request plan or provider",
                            );
                            continue;
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
        let declaration = match lifecycle.completion() {
            Some(deadpan_jobs::CandidateDeclaration::NativeBridgeV2(declaration))
            | Some(deadpan_jobs::CandidateDeclaration::NativeExtensionV3(declaration)) => {
                declaration.clone()
            }
            _ => unreachable!("a validating planned attempt has a declared native bundle"),
        };
        progress(AttemptProgress::Qualifying);
        let qualifying = Instant::now();
        let qualified = qualify_declared(
            allocated,
            &runtime.media_worker,
            &runtime.landmark_worker,
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
    landmark_worker: &Path,
    pinned: &ArtifactWorkspace,
    conditioning: RetainedInputs,
    declaration: NativeCandidateManifest,
    cancelled: &AtomicBool,
) -> Result<QualifiedRun, String> {
    let (bundle, receipt) = match conditioning {
        RetainedInputs::Bridge { selected, inputs } => {
            let bundle = qualify_bridge(
                media_worker,
                landmark_worker,
                pinned,
                BridgeQualification {
                    request: &allocated.host_message,
                    declaration: &declaration,
                    selected_provider: &selected,
                    conditioning: *inputs,
                },
                qualification_limits(),
                cancelled,
            )
            .map_err(|error| error.to_string())?;
            let receipt = validation_receipt(&bundle, &declaration)?;
            (QualifiedBundle::Bridge(Box::new(bundle)), receipt)
        }
        RetainedInputs::Extension { selected, inputs } => {
            let operation_declaration =
                deadpan_jobs::CandidateDeclaration::NativeExtensionV3(declaration.clone());
            let bundle = qualify_extension(
                media_worker,
                landmark_worker,
                pinned,
                ExtensionQualification {
                    request: &allocated.host_message,
                    declaration: &operation_declaration,
                    selected_provider: &selected,
                    conditioning: *inputs,
                },
                qualification_limits(),
                cancelled,
            )
            .map_err(|error| error.to_string())?;
            let receipt = extension_validation_receipt(&bundle, &declaration)?;
            (QualifiedBundle::Extension(Box::new(bundle)), receipt)
        }
    };
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
        ValidatorIdentity::new("native-ffv1", "bridge-8").map_err(text)?,
    )
    .map_err(text)?
    .with_admission(admission)
    .map_err(text)
}

fn extension_validation_receipt(
    bundle: &QualifiedExtensionBundle,
    declaration: &NativeCandidateManifest,
) -> Result<BundleValidationReceipt, String> {
    let binding = bundle.binding();
    let inputs = bundle.conditioning().receipt();
    let text = |error: deadpan_store::generation_attempts::AttemptValueError| error.to_string();
    BundleValidationReceipt::new_extension(
        declaration,
        bundle.native().object().clone(),
        bundle.sampled().object().clone(),
        bundle.provenance().object().clone(),
        binding.constraints.video.clone(),
        binding.plan.clone(),
        ValidatorIdentity::new("native-ffv1", "extension-1").map_err(text)?,
        BundleAdmissionEvidence::new(
            bundle.native_span(),
            bundle.sampled_span(),
            BundleInputObjects::new_extension(
                binding.input.sha256.clone(),
                inputs.manifest().object().clone(),
                inputs
                    .context()
                    .iter()
                    .map(|input| input.object().clone())
                    .collect(),
                inputs.opposite().map(|input| input.object().clone()),
                inputs.signatures().object().clone(),
            )
            .map_err(text)?,
        )
        .map_err(text)?,
    )
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

/// Record `run`'s outcome: publish every retained object and record Ready, or the
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
                for mut input in conditioning {
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
