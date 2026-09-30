//! Bounded native browsing of operational render history. Stored observations
//! are display data; recovery always obtains fresh worker capabilities.

use std::path::PathBuf;

use deadpan_core::{FrameRange, RevisionId};
use deadpan_jobs::{
    AttemptId, RequestId,
    render::{
        RenderAttemptState, RenderDiagnostic,
        publication::{PublicationOutcome, PublicationPhase},
    },
};

use super::{ProjectRenderContext, ProjectRenderError};

pub const PAGE_SIZE: u32 = 8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Query {
    Jobs { after: Option<RequestId> },
    Attempts { job: RequestId, after_ordinal: u64 },
    Publications { after: Option<RequestId> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub ticket: u64,
    pub context: ProjectRenderContext,
    pub query: Query,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Update {
    pub ticket: u64,
    pub context: ProjectRenderContext,
    pub query: Query,
    pub result: Result<Page, ProjectRenderError>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Page {
    Jobs {
        items: Vec<JobSummary>,
        next_after: Option<RequestId>,
    },
    Attempts {
        job: JobSummary,
        items: Vec<AttemptSummary>,
        next_after_ordinal: Option<u64>,
    },
    Publications {
        items: Vec<PublicationSummary>,
        next_after: Option<RequestId>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobSummary {
    pub job_id: RequestId,
    pub revision_id: RevisionId,
    pub range: FrameRange,
    pub automatic: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttemptSummary {
    pub attempt_id: AttemptId,
    pub ordinal: u64,
    pub state: RenderAttemptState,
    /// The original encoding owner, including when this row is a later retry.
    pub checkpoint_attempt_id: Option<AttemptId>,
    pub cancellation_requested: bool,
    pub diagnostic: Option<RenderDiagnostic>,
    pub verification_recorded: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicationSummary {
    pub publication_id: RequestId,
    pub job_id: RequestId,
    pub revision_id: RevisionId,
    pub destination: PathBuf,
    pub automatic: bool,
    pub encoding_attempt_id: AttemptId,
    pub phase: PublicationPhase,
    pub outcome: PublicationOutcome,
    pub operation_active: bool,
    pub observed_movie_commit: bool,
    pub diagnostic: Option<RenderDiagnostic>,
}

impl PublicationSummary {
    /// Availability is only a UI hint. Admission rechecks the current journal.
    pub fn can_reconcile(&self) -> bool {
        self.automatic
            && !self.operation_active
            && matches!(
                self.outcome,
                PublicationOutcome::Interrupted
                    | PublicationOutcome::Unresolved
                    | PublicationOutcome::PublishedUnconfirmed
                    | PublicationOutcome::Published
            )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Recovery {
    Retry {
        job_id: RequestId,
        /// Some reuses this encoding owner's saved bytes; None renders the
        /// immutable historical intent again through fresh encoder admission.
        checkpoint_attempt_id: Option<AttemptId>,
        destination: PathBuf,
    },
    Reconcile {
        publication_id: RequestId,
    },
}
