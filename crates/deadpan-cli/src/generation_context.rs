//! Whether a generation request still describes the same context after an
//! edit, decided without decoding media.
//!
//! A bridge Hold's context is its duration, the project rate and canvas, and
//! the pictures on each side of it: the exact Original picture (asset and
//! source time, or a freeze), its captured geometry and every framing pose
//! that composes it. Two documents agree when that identity is equal, even
//! if the Hold moved in time. Anything else, including a Hold that no longer
//! has exactly one plain occurrence, makes the request stale; the conditioning
//! that produced its candidate would no longer match the edit.

use std::sync::Mutex;

use deadpan_core::{NodeId, NodeKind, ProjectDocument, ProjectFrame, RevisionId};
use deadpan_plan::{PictureSample, RenderPlan};
use deadpan_store::generation::{
    ContextObservation, GenerationContextResolver, StoredGenerationRequest,
};
use sha2::{Digest, Sha256};

/// The context identity of `hold` in `document`, or `None` when the Hold is
/// absent, not a Hold, or has no single plain occurrence.
pub fn context_identity(document: &ProjectDocument, hold: &NodeId) -> Option<[u8; 32]> {
    let plan = RenderPlan::compile(document).ok()?;
    identity_in(document, &plan, hold)
}

fn identity_in(document: &ProjectDocument, plan: &RenderPlan, hold: &NodeId) -> Option<[u8; 32]> {
    let NodeKind::Hold { recipe } = &document.nodes().get(hold)?.kind else {
        return None;
    };
    let range = plan.single_occurrence_range(hold)?;
    let total = plan.duration().frames();
    let boundary = |frame: i64| -> Option<serde_json::Value> {
        if frame < 0 || frame >= total {
            return Some(serde_json::Value::Null);
        }
        let sample = plan.picture(ProjectFrame(frame)).ok()?;
        Some(boundary_identity(&sample))
    };
    let basis = document.presentation_basis();
    let identity = serde_json::json!({
        "duration": recipe.duration.frames(),
        "rate": [basis.frame_rate.numerator(), basis.frame_rate.denominator()],
        "canvas": [basis.width, basis.height],
        "left": boundary(range.start().0 - 1)?,
        "right": boundary(range.end().0)?,
    });
    Some(Sha256::digest(serde_json::to_vec(&identity).ok()?).into())
}

/// The picture and its composition, without positions or revision ids.
fn boundary_identity(sample: &PictureSample) -> serde_json::Value {
    serde_json::json!({
        "picture": sample.picture,
        "picture_context": sample.picture_context,
        "gap": sample.gap_after.is_some(),
        "framing": sample
            .framing
            .iter()
            .map(|layer| serde_json::json!({"pose": layer.pose, "escalation": layer.escalation}))
            .collect::<Vec<_>>(),
    })
}

/// The store resolver used by the app and CLI writers.
#[derive(Default)]
pub struct BoundaryContextResolver {
    /// The most recent origin plan, which repeats across edits.
    origin: Mutex<Option<(RevisionId, RenderPlan)>>,
}

impl GenerationContextResolver for BoundaryContextResolver {
    fn observe(
        &self,
        origin: &ProjectDocument,
        after: &ProjectDocument,
        request: &StoredGenerationRequest,
    ) -> ContextObservation {
        let hold = &request.binding.hold_id;
        let before = {
            let Ok(mut cached) = self.origin.lock() else {
                return ContextObservation::Unresolved;
            };
            if cached
                .as_ref()
                .is_none_or(|(revision, _)| revision != origin.revision_id())
            {
                let Ok(plan) = RenderPlan::compile(origin) else {
                    return ContextObservation::Unresolved;
                };
                *cached = Some((origin.revision_id().clone(), plan));
            }
            let (_, plan) = cached.as_ref().expect("cached origin plan");
            identity_in(origin, plan, hold)
        };
        match (before, context_identity(after, hold)) {
            (Some(before), Some(after)) if before == after => {
                ContextObservation::Resolved(request.binding.context_sha256.clone())
            }
            _ => ContextObservation::Unresolved,
        }
    }
}

#[cfg(test)]
mod tests;
