//! AI pause generation through the open project's own job.
//!
//! The native owner runs the request with its single bounded AI job, exactly
//! as `,a` / `:generate` would, and the app shows its progress. These wire
//! types carry no capability: a status is an observation of that job, and a
//! cancellation names the exact job it observed.

use deadpan_core::{NodeId, RevisionId, ScopedNodeTarget};
use deadpan_jobs::RequestId;
use serde::{Deserialize, Serialize};

use super::LiveError;

/// The most variants one request generates; equals the app's limit.
pub const MAX_VARIANTS: u8 = 4;
/// Status text is display-only; longer text is truncated by the owner.
pub const MAX_STATUS_TEXT: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerateRequest {
    pub hold: NodeId,
    /// Explicit authoring address. Absence selects an ordinary Hold with no
    /// Repeat ancestors, never a play chosen from the owner's current cursor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<ScopedNodeTarget>,
    /// The revision the caller observed; a different head is refused.
    pub expected_revision: RevisionId,
    pub variants: u8,
    /// The seed of a new request; variants of an existing request derive
    /// theirs from its own seed. None lets the owner choose.
    pub seed: Option<u64>,
    /// None retains the current request's controls; Some explicitly replaces them.
    #[serde(default)]
    pub options: Option<deadpan_jobs::GenerationOptions>,
}

impl GenerateRequest {
    pub fn target(&self) -> ScopedNodeTarget {
        self.scope.clone().unwrap_or_else(|| ScopedNodeTarget {
            node: self.hold.clone(),
            repeats: Vec::new(),
        })
    }

    pub fn validate(&self) -> Result<(), LiveError> {
        if self
            .scope
            .as_ref()
            .is_some_and(|scope| scope.node != self.hold)
        {
            return Err(LiveError::new(
                "GenerationRefused",
                "The scope names a different Hold",
            ));
        }
        if !(1..=MAX_VARIANTS).contains(&self.variants) {
            return Err(LiveError::new(
                "GenerationRefused",
                format!("Generate 1 to {MAX_VARIANTS} AI variants at a time"),
            ));
        }
        if self.seed.is_some_and(|seed| seed >= 1 << 32) {
            return Err(LiveError::new(
                "GenerationRefused",
                "The seed must be below 2^32",
            ));
        }
        Ok(())
    }
}

/// How an observed job ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum GenerationOutcome {
    Ready {},
    Cancelled {},
    Failed { reason: String },
    Unavailable { reason: String },
}

/// One observation of the owner's AI job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerationStatus {
    pub job: u64,
    pub hold: NodeId,
    pub scope: ScopedNodeTarget,
    pub options: deadpan_jobs::GenerationOptions,
    /// The recorded request, once allocated.
    pub request_id: Option<RequestId>,
    pub variants: u8,
    /// The 1-based variant in progress (the last one once finished).
    pub variant: u8,
    pub ready: u8,
    pub stage: String,
    /// Completed and total worker steps, when reported.
    pub steps: Option<[u64; 2]>,
    pub elapsed_ms: u64,
    /// None while the job runs.
    pub outcome: Option<GenerationOutcome>,
    /// What else the outcome means, such as Ready variants kept after a
    /// later one failed, was cancelled or could not start.
    pub note: Option<String>,
}

impl GenerationStatus {
    pub fn finished(&self) -> bool {
        self.outcome.is_some()
    }
}

/// Bound display text on the owner side.
pub fn bounded(text: &str) -> String {
    if text.len() <= MAX_STATUS_TEXT {
        return text.to_owned();
    }
    let mut end = MAX_STATUS_TEXT;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}
