//! Explicit acceptance of one retained local word-timing review.
use super::{OriginalCorrections, OriginalTranscript, Workspace};
use deadpan_analysis::refinement::RefinementReview;
use deadpan_core::RevisionId;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct Request {
    pub expected_session: u64,
    pub expected_revision: RevisionId,
    pub attempt: u64,
    pub lifecycle_epoch: u64,
    pub transcript: Arc<OriginalTranscript>,
    pub corrections: Arc<OriginalCorrections>,
    pub review: Arc<RefinementReview>,
    pub selected_words: Vec<usize>,
}

impl Request {
    pub fn matches(&self, workspace: &Workspace) -> bool {
        workspace.session == self.expected_session
            && workspace.document.revision_id() == &self.expected_revision
            && workspace
                .transcript
                .as_ref()
                .is_some_and(|value| Arc::ptr_eq(value, &self.transcript))
            && workspace
                .corrections
                .as_ref()
                .is_some_and(|value| Arc::ptr_eq(value, &self.corrections))
    }
}
