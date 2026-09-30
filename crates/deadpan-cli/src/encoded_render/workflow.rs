//! One owner-thread coordinator and one bounded media/filesystem stage worker.
//!
//! The owner borrows the writable store only to advance operational journals.
//! The worker retains live candidates and destination locks between stages; it
//! never receives a SQLite connection. Call `drain` before closing the writer.

use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
    },
    thread::{self, JoinHandle},
    time::Instant,
};

use deadpan_core::{FrameRange, ProjectId, RevisionId};
use deadpan_jobs::{
    AttemptId, CancellationToken, RequestId,
    render::{
        RenderAttemptState, RenderDiagnostic, RenderEngineeringPolicy, RenderIntent,
        RenderVerificationObservation,
        publication::{
            PreparedPublicationEvidence, PublicationCompletion, PublicationIntent,
            PublicationOperationKind, PublicationPhase, PublicationReconciliation,
            StoredPublication,
        },
    },
};
use deadpan_store::{
    ProjectStore, StoreError,
    publication::PublicationPermit,
    render_jobs::{
        BeginRenderAttempt, RenderAttemptTransition, StoredRenderAttempt, StoredRenderCheckpoint,
    },
    render_media::{
        PreparedRenderRetention, RenderMediaLimits, RenderReadHandle, RenderWorkflowLease,
        RenderWriteHandle,
    },
};
use serde::Serialize;

use super::{
    EncodedRenderError, EncodedWorkerLimits,
    jobs::{self, CaptureRenderIntent, RenderStageRequest},
    publication::{
        PublicationDiagnostic, PublicationOutcome, PublicationReceipt, PublicationStage,
        RetainedPublicationArtifacts,
        journal::{self, PreparedPublication, RecoveryInspection, RecoveryOutcome},
    },
    verification::{VerificationLimits, VerificationProgress, VerifiedCandidate},
};
use crate::render_worker::RenderWorkerRuntime;

mod owner;
mod types;
mod worker;
pub use types::*;
use worker::{Command, ExpectedReply, Reply, StageReply, Worker};

#[derive(Clone)]
enum Destination {
    Publish(PublicationRequest),
    Reconcile {
        publication_id: RequestId,
        operation_id: AttemptId,
        cancellation_token: CancellationToken,
    },
}

struct Active {
    identity: WorkflowIdentity,
    destination: Destination,
    deadline: Instant,
    cancelled: Arc<AtomicBool>,
    pending: bool,
    expected_reply: Option<ExpectedReply>,
    blocked: bool,
    released: bool,
    lease: Option<RenderWorkflowLease>,
}

/// Call methods on the same store-owner thread. It is safe to edit the project
/// while a workflow runs: the captured revision and all operation IDs stay fixed.
pub struct RenderWorkflow {
    config: WorkflowConfig,
    owner: RenderReadHandle,
    project_id: ProjectId,
    commands: Option<SyncSender<Command>>,
    replies: Receiver<Reply>,
    progress: Arc<Mutex<Option<(WorkflowIdentity, WorkflowProgress)>>>,
    worker: Option<JoinHandle<()>>,
    active: Option<Active>,
    status: WorkflowStatus,
}

fn diagnostic(code: &str, detail: impl ToString) -> RenderDiagnostic {
    let mut detail = detail.to_string().replace('\0', "�");
    if detail.is_empty() {
        detail = code.to_owned();
    }
    let mut end = detail
        .len()
        .min(deadpan_jobs::render::MAX_RENDER_DIAGNOSTIC_BYTES);
    while !detail.is_char_boundary(end) {
        end -= 1;
    }
    detail.truncate(end);
    RenderDiagnostic {
        code: code.into(),
        detail,
    }
}

impl Drop for RenderWorkflow {
    fn drop(&mut self) {
        // Drop is not a successful shutdown report. A host must drain while its
        // writer is alive; otherwise the journal remains active for reopen.
        if let Some(active) = &self.active {
            active.cancelled.store(true, Ordering::Release);
        }
        self.commands.take();
        // The stage worker owns all potentially heavy media cleanup. Dropping
        // this JoinHandle detaches it; no UI thread waits on a destructor.
        self.worker.take();
    }
}
