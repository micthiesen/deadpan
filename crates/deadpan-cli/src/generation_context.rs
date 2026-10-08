//! Whether a generation request still describes the same context after an
//! edit, decided without decoding media.
//!
//! A bridge Hold's context is its duration, the project rate and canvas, and
//! the raw pictures on each side in its authored definition clock. Two
//! documents agree when that identity is equal, even if the Hold moved or
//! was isolated into one Repeat play. Outer retiming and editorial Camera
//! do not change the pictures supplied to the model.

use std::sync::{Arc, Mutex};

use deadpan_core::{
    BoundaryQueryLimits, NodeId, NodeKind, ProjectDocument, ProjectId, RevisionId,
    ScopedNodeTarget, TargetId,
};
use deadpan_plan::{DefinitionPictureSample, RenderPlan};
use deadpan_store::generation::{
    ContextObservation, GenerationContextResolver, StoredGenerationRequest,
};
use deadpan_store::generation_pictures::GenerationPictures;
use deadpan_store::generation_preparations::StoredGenerationPreparation;
use sha2::{Digest, Sha256};

/// The context identity of `hold` in `document`, or `None` when the Hold is
/// absent, not a Hold, or needs an explicit Repeat scope.
pub fn context_identity(document: &ProjectDocument, hold: &NodeId) -> Option<[u8; 32]> {
    scoped_context_identity(
        document,
        &ScopedNodeTarget {
            node: hold.clone(),
            repeats: Vec::new(),
        },
    )
}

pub fn scoped_context_identity(
    document: &ProjectDocument,
    target: &ScopedNodeTarget,
) -> Option<[u8; 32]> {
    let plan = RenderPlan::compile(document).ok()?;
    identity_in(document, &plan, target, None, None)
}

fn identity_in(
    document: &ProjectDocument,
    plan: &RenderPlan,
    scope: &ScopedNodeTarget,
    target: Option<&TargetId>,
    pictures: Option<&dyn GenerationPictures>,
) -> Option<[u8; 32]> {
    let NodeKind::Hold { recipe } = &document.nodes().get(&scope.node)?.kind else {
        return None;
    };
    let boundaries = plan
        .scoped_hold_boundaries(scope, BoundaryQueryLimits::default())
        .ok()?;
    let basis = document.presentation_basis();
    let mut identity = serde_json::json!({
        "duration": recipe.duration.frames(),
        "rate": [basis.frame_rate.numerator(), basis.frame_rate.denominator()],
        "canvas": [basis.width, basis.height],
        "left": boundary_identity(document, boundaries.left.as_ref(), pictures)?,
        "right": boundary_identity(document, boundaries.right.as_ref(), pictures)?,
    });
    if let Some(target) = target {
        // A correction can change the seed without changing either picture.
        // Bind the entire saved target conservatively, including its path,
        // corrections and provenance. Absence never selects another target.
        identity["region_target"] = serde_json::json!({
            "id": target,
            "record": document.targets().get(target)?,
        });
    }
    Some(Sha256::digest(serde_json::to_vec(&identity).ok()?).into())
}

/// Model input precedes editorial composition. Captured framing, gain and
/// captions may change without invalidating the same raw conditioning picture.
fn boundary_identity(
    document: &ProjectDocument,
    sample: Option<&DefinitionPictureSample>,
    pictures: Option<&dyn GenerationPictures>,
) -> Option<serde_json::Value> {
    let Some(sample) = sample else {
        return Some(serde_json::Value::Null);
    };
    match pictures {
        Some(pictures) => {
            serde_json::to_value(pictures.identity(document, &sample.picture).ok()?).ok()
        }
        None => Some(serde_json::json!({ "picture": sample.picture })),
    }
}

/// The store resolver used by the app and CLI writers.
#[derive(Default)]
pub struct BoundaryContextResolver {
    /// Origins are immutable stored revisions and repeat across edits.
    origin: PlanCache,
}

type PlanCache = Mutex<Option<(ProjectId, RevisionId, Arc<RenderPlan>)>>;

/// A prospective document has no persistent cache identity. Borrow the exact
/// snapshot for one observation batch, including an unsuccessful compilation.
struct PreparedBoundaryContext<'a> {
    origin: &'a PlanCache,
    after: &'a ProjectDocument,
    plan: Option<RenderPlan>,
}

fn cached_plan(cache: &PlanCache, document: &ProjectDocument) -> Option<Arc<RenderPlan>> {
    let mut cached = cache.lock().ok()?;
    if cached.as_ref().is_none_or(|(project, revision, _)| {
        project != document.project_id() || revision != document.revision_id()
    }) {
        let plan = Arc::new(RenderPlan::compile(document).ok()?);
        *cached = Some((
            document.project_id().clone(),
            document.revision_id().clone(),
            plan,
        ));
    }
    Some(Arc::clone(&cached.as_ref()?.2))
}

impl GenerationContextResolver for BoundaryContextResolver {
    fn preparation_is_relevant_with_pictures(
        &self,
        origin: &ProjectDocument,
        after: &ProjectDocument,
        preparation: &StoredGenerationPreparation,
        pictures: &dyn GenerationPictures,
    ) -> bool {
        preparation_with_plan(
            &self.origin,
            origin,
            after,
            RenderPlan::compile(after).ok().as_ref(),
            preparation,
            Some(pictures),
        )
    }

    fn observe_with_pictures(
        &self,
        origin: &ProjectDocument,
        after: &ProjectDocument,
        request: &StoredGenerationRequest,
        pictures: &dyn GenerationPictures,
    ) -> ContextObservation {
        observe_with_plan(
            &self.origin,
            origin,
            after,
            RenderPlan::compile(after).ok().as_ref(),
            request,
            Some(pictures),
        )
    }

    fn preparation_is_relevant(
        &self,
        origin: &ProjectDocument,
        after: &ProjectDocument,
        preparation: &StoredGenerationPreparation,
    ) -> bool {
        preparation_with_plan(
            &self.origin,
            origin,
            after,
            RenderPlan::compile(after).ok().as_ref(),
            preparation,
            None,
        )
    }

    fn prepare_transition<'a>(
        &'a self,
        after: &'a ProjectDocument,
    ) -> Option<Box<dyn GenerationContextResolver + 'a>> {
        Some(Box::new(PreparedBoundaryContext {
            origin: &self.origin,
            after,
            plan: RenderPlan::compile(after).ok(),
        }))
    }

    fn observe(
        &self,
        origin: &ProjectDocument,
        after: &ProjectDocument,
        request: &StoredGenerationRequest,
    ) -> ContextObservation {
        observe_with_plan(
            &self.origin,
            origin,
            after,
            RenderPlan::compile(after).ok().as_ref(),
            request,
            None,
        )
    }
}

impl GenerationContextResolver for PreparedBoundaryContext<'_> {
    fn preparation_is_relevant_with_pictures(
        &self,
        origin: &ProjectDocument,
        after: &ProjectDocument,
        preparation: &StoredGenerationPreparation,
        pictures: &dyn GenerationPictures,
    ) -> bool {
        std::ptr::eq(self.after, after)
            && preparation_with_plan(
                self.origin,
                origin,
                after,
                self.plan.as_ref(),
                preparation,
                Some(pictures),
            )
    }

    fn observe_with_pictures(
        &self,
        origin: &ProjectDocument,
        after: &ProjectDocument,
        request: &StoredGenerationRequest,
        pictures: &dyn GenerationPictures,
    ) -> ContextObservation {
        if !std::ptr::eq(self.after, after) {
            return ContextObservation::Unresolved;
        }
        observe_with_plan(
            self.origin,
            origin,
            after,
            self.plan.as_ref(),
            request,
            Some(pictures),
        )
    }

    fn preparation_is_relevant(
        &self,
        origin: &ProjectDocument,
        after: &ProjectDocument,
        preparation: &StoredGenerationPreparation,
    ) -> bool {
        std::ptr::eq(self.after, after)
            && preparation_with_plan(
                self.origin,
                origin,
                after,
                self.plan.as_ref(),
                preparation,
                None,
            )
    }

    fn observe(
        &self,
        origin: &ProjectDocument,
        after: &ProjectDocument,
        request: &StoredGenerationRequest,
    ) -> ContextObservation {
        if !std::ptr::eq(self.after, after) {
            return ContextObservation::Unresolved;
        }
        observe_with_plan(
            self.origin,
            origin,
            after,
            self.plan.as_ref(),
            request,
            None,
        )
    }
}

fn preparation_with_plan(
    origin_cache: &PlanCache,
    origin: &ProjectDocument,
    after: &ProjectDocument,
    after_plan: Option<&RenderPlan>,
    preparation: &StoredGenerationPreparation,
    pictures: Option<&dyn GenerationPictures>,
) -> bool {
    let target = match preparation.origin.options() {
        Some(options) => options.region_target.resolve(None),
        None => {
            // Its exact saved target is in verified provenance, read later on
            // the preparation worker. No target changes may cross this gap.
            if origin.targets() != after.targets() {
                return false;
            }
            None
        }
    };
    let before = cached_plan(origin_cache, origin).and_then(|plan| {
        identity_in(
            origin,
            &plan,
            &preparation.origin_target,
            target.as_ref(),
            pictures,
        )
    });
    let after = after_plan
        .and_then(|plan| identity_in(after, plan, &preparation.target, target.as_ref(), pictures));
    matches!((before, after), (Some(before), Some(after)) if before == after)
}

fn observe_with_plan(
    origin_cache: &PlanCache,
    origin: &ProjectDocument,
    after: &ProjectDocument,
    after_plan: Option<&RenderPlan>,
    request: &StoredGenerationRequest,
    pictures: Option<&dyn GenerationPictures>,
) -> ContextObservation {
    let target = request.constraints.region_target.as_ref();
    let before = cached_plan(origin_cache, origin)
        .and_then(|plan| identity_in(origin, &plan, &request.origin_target, target, pictures));
    let after =
        after_plan.and_then(|plan| identity_in(after, plan, &request.target, target, pictures));
    match (before, after) {
        (Some(before), Some(after)) if before == after => {
            ContextObservation::Resolved(request.binding.context_sha256.clone())
        }
        _ => ContextObservation::Unresolved,
    }
}

#[cfg(test)]
mod tests;
