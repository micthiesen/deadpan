//! Public automatic SDR rendering. Serialized requests and observations do not
//! grant media, publication, or process-cleanup authority.

use crate::{
    encoded_render::{
        EncodedWorkerLimits,
        verification::VerificationLimits,
        workflow::{
            PublicationRequest, ReconcileRender, RenderWorkflow, RetryRender, StartRender,
            WorkflowConfig, WorkflowError, WorkflowIdentity,
        },
    },
    render_worker::RenderWorkerRuntime,
};
use deadpan_core::{ProjectDocument, ProjectFrame};
use deadpan_jobs::{
    AttemptId, CancellationToken, RequestId,
    render::{
        RenderAutomaticAlgorithm, RenderAutomaticPolicy, RenderAutomaticSelection, RenderPolicy,
        publication::PublicationIntent,
    },
};
use deadpan_store::{
    ProjectStore, StoreError,
    render_media::{MAX_RENDER_MANIFEST_BYTES, MAX_RENDER_MOVIE_BYTES, RenderMediaLimits},
};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    time::{Duration, Instant},
};

mod cli;
mod types;
pub(crate) use cli::{report_error, run};
pub use types::*;

#[cfg(test)]
mod tests;

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_REQUEST_BYTES: usize = 16 * 1024;
pub const MAX_REPLY_BYTES: usize = 256 * 1024;
pub const STATUS_PAGE_SIZE: u32 = 8;
pub const RENDER_TIMEOUT: Duration = Duration::from_secs(24 * 60 * 60);
pub const RETAINED_NAMESPACE_BYTES: u64 = 128 * 1024 * 1024 * 1024;
pub const RETAINED_NAMESPACE_ENTRIES: u32 = 4096;

#[derive(Debug, Clone)]
pub struct RenderLimits {
    pub encode: EncodedWorkerLimits,
    pub verification: VerificationLimits,
    pub media: RenderMediaLimits,
    pub timeout: Duration,
}

/// Pure host policy. These bounds are not a free-space reservation or a promise
/// that every project inside them is supported by the current media path.
pub fn default_limits() -> Result<RenderLimits, PublicRenderError> {
    Ok(RenderLimits {
        encode: EncodedWorkerLimits::default(),
        verification: VerificationLimits::default(),
        media: RenderMediaLimits::new(
            MAX_RENDER_MOVIE_BYTES,
            MAX_RENDER_MANIFEST_BYTES,
            MAX_RENDER_MOVIE_BYTES + MAX_RENDER_MANIFEST_BYTES,
            RETAINED_NAMESPACE_BYTES,
            RETAINED_NAMESPACE_ENTRIES,
        )
        .map_err(|error| PublicRenderError::new("RenderInvalidLimits", error))?,
        timeout: RENDER_TIMEOUT,
    })
}

/// Call on the service/CLI owner, never from native UI callbacks.
pub fn current_runtime_config(package: PathBuf) -> Result<WorkflowConfig, PublicRenderError> {
    let limits = default_limits()?;
    Ok(WorkflowConfig {
        package,
        runtime: RenderWorkerRuntime {
            executable: std::env::current_exe().map_err(PublicRenderError::io)?,
            arguments: Vec::new(),
            environment: BTreeMap::new(),
        },
        encode_limits: limits.encode,
        verification_limits: limits.verification,
        media_limits: limits.media,
    })
}

fn fresh_job() -> Result<RequestId, PublicRenderError> {
    RequestId::new(uuid::Uuid::new_v4().to_string()).map_err(PublicRenderError::invalid)
}
fn fresh_attempt() -> Result<AttemptId, PublicRenderError> {
    AttemptId::new(uuid::Uuid::new_v4().to_string()).map_err(PublicRenderError::invalid)
}
fn fresh_token() -> Result<CancellationToken, PublicRenderError> {
    CancellationToken::new(uuid::Uuid::new_v4().to_string()).map_err(PublicRenderError::invalid)
}
fn identity(job_id: RequestId) -> Result<WorkflowIdentity, PublicRenderError> {
    Ok(WorkflowIdentity {
        job_id,
        attempt_id: fresh_attempt()?,
        cancellation_token: fresh_token()?,
    })
}
fn deadline(now: Instant) -> Result<Instant, PublicRenderError> {
    now.checked_add(RENDER_TIMEOUT)
        .ok_or_else(|| PublicRenderError::invalid("render deadline exceeds monotonic clock"))
}
fn publication(
    destination: PathBuf,
    identity: &WorkflowIdentity,
) -> Result<PublicationRequest, PublicRenderError> {
    let publication_id = fresh_job()?;
    PublicationIntent {
        schema_version: 1,
        publication_id: publication_id.clone(),
        job_id: identity.job_id.clone(),
        verified_attempt_id: identity.attempt_id.clone(),
        destination: destination.clone(),
    }
    .validate()
    .map_err(PublicRenderError::invalid)?;
    Ok(PublicationRequest {
        destination,
        publication_id,
        operation_id: fresh_attempt()?,
        cancellation_token: fresh_token()?,
    })
}

/// Pure UI-safe construction. Start admission still checks the captured revision
/// against the owning writer. No filesystem, hashing, or media work occurs here.
pub fn start_request(
    context: &RenderContext,
    destination: PathBuf,
    now: Instant,
) -> Result<StartRender, PublicRenderError> {
    let identity = identity(fresh_job()?)?;
    let publication = publication(destination, &identity)?;
    Ok(StartRender {
        revision: context.revision_id.clone(),
        range: None,
        identity,
        policy: RenderPolicy::Automatic(RenderAutomaticPolicy {
            schema_version: 1,
            selection: RenderAutomaticSelection::Automatic,
            algorithm: RenderAutomaticAlgorithm::AutomaticSdrV1,
        }),
        publication,
        deadline: deadline(now)?,
    })
}

fn check_context(
    store: &ProjectStore,
    context: &RenderContext,
    current: bool,
) -> Result<(), PublicRenderError> {
    let document = store.snapshot().map_err(PublicRenderError::store)?;
    if document.project_id() != &context.project_id {
        return Err(PublicRenderError::new(
            "RenderProjectChanged",
            "The request belongs to a different project",
        ));
    }
    if current && document.revision_id() != &context.revision_id {
        return Err(PublicRenderError {
            current_revision: Some(document.revision_id().clone()),
            ..PublicRenderError::new(
                "RenderRevisionChanged",
                "The committed revision changed before render admission",
            )
        });
    }
    Ok(())
}

fn require_automatic(policy: &RenderPolicy) -> Result<(), PublicRenderError> {
    if !policy.is_automatic() {
        return Err(PublicRenderError::new(
            "RenderEngineeringJob",
            "Public recovery requires an automatic render job; this historical job used an explicit engineering policy",
        ));
    }
    Ok(())
}

/// Resolve historical intent only on the project-service/CLI store owner.
pub fn retry_request(
    store: &ProjectStore,
    context: &RenderContext,
    job: &RequestId,
    checkpoint: Option<&AttemptId>,
    destination: PathBuf,
    now: Instant,
) -> Result<RetryRender, PublicRenderError> {
    check_context(store, context, false)?;
    let intent = store.render_job(job).map_err(PublicRenderError::store)?;
    require_automatic(&intent.policy)?;
    if intent.project_id != context.project_id {
        return Err(PublicRenderError::new(
            "RenderProjectChanged",
            "The retained job belongs to a different project",
        ));
    }
    if let Some(checkpoint) = checkpoint {
        store
            .render_checkpoint(job, checkpoint)
            .map_err(PublicRenderError::store)?;
    }
    let identity = identity(job.clone())?;
    let publication = publication(destination, &identity)?;
    Ok(RetryRender {
        identity,
        checkpoint_attempt_id: checkpoint.cloned(),
        publication,
        deadline: deadline(now)?,
    })
}

pub fn reconcile_request(
    store: &ProjectStore,
    context: &RenderContext,
    publication_id: &RequestId,
    now: Instant,
) -> Result<ReconcileRender, PublicRenderError> {
    check_context(store, context, false)?;
    let retained = store
        .render_publication(publication_id)
        .map_err(PublicRenderError::store)?;
    require_automatic(&retained.render_intent.policy)?;
    if retained.render_intent.project_id != context.project_id {
        return Err(PublicRenderError::new(
            "RenderProjectChanged",
            "The publication belongs to a different project",
        ));
    }
    Ok(ReconcileRender {
        publication_id: publication_id.clone(),
        identity: identity(retained.intent.job_id)?,
        operation_id: fresh_attempt()?,
        cancellation_token: fresh_token()?,
        deadline: deadline(now)?,
    })
}

/// Exact target cancellation through the live owner. Persist/revoke precedes
/// worker signaling inside the shared coordinator.
pub fn cancel_request(
    workflow: &mut RenderWorkflow,
    store: &mut ProjectStore,
    context: &RenderContext,
    target: &WorkflowTarget,
) -> Result<(), PublicRenderError> {
    check_context(store, context, false)?;
    workflow
        .cancel(store, &target.clone().into())
        .map_err(PublicRenderError::workflow)
}

/// Informational geometry from the authored document, without media admission.
pub fn output_summary(
    document: &ProjectDocument,
) -> Result<RenderOutputSummary, PublicRenderError> {
    let basis = document.presentation_basis();
    let frames = document
        .duration()
        .map_err(PublicRenderError::invalid)?
        .frames();
    if frames <= 0 {
        return Err(PublicRenderError::new(
            "RenderEmpty",
            "The committed edit has no picture time",
        ));
    }
    if basis.color_policy != deadpan_core::ColorPolicy::SdrRec709 {
        return Err(PublicRenderError::new(
            "RenderColorUnsupported",
            "This output path requires a committed SDR Rec.709 document",
        ));
    }
    let raster = deadpan_jobs::render::admission::RenderPictureContract::nearest_even_raster([
        basis.width,
        basis.height,
    ])
    .map_err(PublicRenderError::invalid)?;
    Ok(RenderOutputSummary {
        canvas: [basis.width, basis.height],
        raster,
        frame_rate: basis.frame_rate,
        frame_count: u64::try_from(frames).map_err(PublicRenderError::invalid)?,
        audio_samples: basis
            .frame_rate
            .audio_boundary(ProjectFrame(frames))
            .map_err(PublicRenderError::invalid)?
            .0,
        algorithm: RenderAutomaticAlgorithm::AutomaticSdrV1,
    })
}

impl PublicRenderError {
    pub fn new(code: impl Into<String>, message: impl ToString) -> Self {
        fn bounded(value: String, maximum: usize) -> String {
            let value = value.replace('\0', "�");
            let mut end = value.len().min(maximum);
            while !value.is_char_boundary(end) {
                end -= 1;
            }
            value[..end].to_owned()
        }
        Self {
            code: bounded(code.into(), 128),
            message: bounded(message.to_string(), 16 * 1024),
            current_revision: None,
        }
    }
    fn invalid(error: impl ToString) -> Self {
        Self::new("RenderInvalidRequest", error)
    }
    fn io(error: impl ToString) -> Self {
        Self::new("RenderIoFailure", error)
    }
    fn store(error: StoreError) -> Self {
        if matches!(error, StoreError::AlreadyOpen) {
            return Self::new(
                "ProjectLocked",
                "Another process owns this project. Close it before headless rendering; local host routing is not implemented yet.",
            );
        }
        let mut value = Self::new(error.code(), &error);
        if let StoreError::RevisionConflict { current, .. } = error {
            value.current_revision = deadpan_core::RevisionId::new(current).ok();
        }
        value
    }
    fn workflow(error: WorkflowError) -> Self {
        match error {
            WorkflowError::Store(error) => Self::store(error),
            WorkflowError::Busy => Self::new(
                "RenderBusy",
                "A render still owns this project's execution slot",
            ),
            WorkflowError::Identity => Self::new(
                "RenderIdentityChanged",
                "This request does not identify the current render",
            ),
            WorkflowError::StaleRevision => Self::new(
                "RenderRevisionChanged",
                "The revision changed before render admission",
            ),
            WorkflowError::Unresolved(error) => Self::new("RenderRecoveryRequired", error),
            WorkflowError::Configuration(error) => Self::invalid(error),
            WorkflowError::Io(error) => Self::io(error),
        }
    }
}
