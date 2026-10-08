//! Whether a generation request still describes the same context after an
//! edit, decided without decoding media.
//!
//! A Hold's context includes its resolved operation's raw input samples and
//! complete structural support in the authored definition clock. Two
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
use deadpan_store::generation_inputs::{
    GenerationCaptureSpec, GenerationInputBinding, InputCaptureBudget,
};
use deadpan_store::generation_intents::IntentInputBinding;
use deadpan_store::generation_pictures::GenerationPictures;
use deadpan_store::generation_preparations::StoredGenerationPreparation;
use sha2::{Digest, Sha256};

/// Legacy structural Bridge endpoint identity, without measured receipts or
/// temporal coverage. Returns `None` for a missing Hold or unresolved scope.
/// Production relevance uses the shared measured input binding below.
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
    identity_in(document, &plan, target, None)
}

fn identity_in(
    document: &ProjectDocument,
    plan: &RenderPlan,
    scope: &ScopedNodeTarget,
    target: Option<&TargetId>,
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
        "left": boundary_identity(boundaries.left.as_ref()),
        "right": boundary_identity(boundaries.right.as_ref()),
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
fn boundary_identity(sample: Option<&DefinitionPictureSample>) -> serde_json::Value {
    sample.map_or(
        serde_json::Value::Null,
        |sample| serde_json::json!({ "picture": sample.picture }),
    )
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
    budget: Mutex<InputCaptureBudget>,
}

struct Capture<'a> {
    pictures: Option<&'a dyn GenerationPictures>,
    budget: &'a mut InputCaptureBudget,
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
            Capture {
                pictures: Some(pictures),
                budget: &mut InputCaptureBudget::default(),
            },
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
            Capture {
                pictures: Some(pictures),
                budget: &mut InputCaptureBudget::default(),
            },
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
            Capture {
                pictures: None,
                budget: &mut InputCaptureBudget::default(),
            },
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
            budget: Mutex::new(InputCaptureBudget::default()),
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
            Capture {
                pictures: None,
                budget: &mut InputCaptureBudget::default(),
            },
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
        let Ok(mut budget) = self.budget.lock() else {
            return false;
        };
        std::ptr::eq(self.after, after)
            && preparation_with_plan(
                self.origin,
                origin,
                after,
                self.plan.as_ref(),
                preparation,
                Capture {
                    pictures: Some(pictures),
                    budget: &mut budget,
                },
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
        let Ok(mut budget) = self.budget.lock() else {
            return ContextObservation::Unresolved;
        };
        observe_with_plan(
            self.origin,
            origin,
            after,
            self.plan.as_ref(),
            request,
            Capture {
                pictures: Some(pictures),
                budget: &mut budget,
            },
        )
    }

    fn preparation_is_relevant(
        &self,
        origin: &ProjectDocument,
        after: &ProjectDocument,
        preparation: &StoredGenerationPreparation,
    ) -> bool {
        let Ok(mut budget) = self.budget.lock() else {
            return false;
        };
        std::ptr::eq(self.after, after)
            && preparation_with_plan(
                self.origin,
                origin,
                after,
                self.plan.as_ref(),
                preparation,
                Capture {
                    pictures: None,
                    budget: &mut budget,
                },
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
        let Ok(mut budget) = self.budget.lock() else {
            return ContextObservation::Unresolved;
        };
        observe_with_plan(
            self.origin,
            origin,
            after,
            self.plan.as_ref(),
            request,
            Capture {
                pictures: None,
                budget: &mut budget,
            },
        )
    }
}

fn preparation_with_plan(
    origin_cache: &PlanCache,
    origin: &ProjectDocument,
    after: &ProjectDocument,
    after_plan: Option<&RenderPlan>,
    preparation: &StoredGenerationPreparation,
    capture: Capture<'_>,
) -> bool {
    let binding = match &preparation.intent.input_binding {
        IntentInputBinding::Measured { binding } => Some(binding.as_ref()),
        IntentInputBinding::Unavailable { .. } => None,
    };
    if let Some(pictures) = capture.pictures {
        let Some(binding) = binding else {
            return false;
        };
        if preparation.intent.capture != Some(binding.capture_spec()) {
            return false;
        }
        let Some(origin_plan) = cached_plan(origin_cache, origin) else {
            return false;
        };
        let Some(after_plan) = after_plan else {
            return false;
        };
        let region = binding.region.as_ref().map(|region| &region.id);
        let before = GenerationInputBinding::capture_with_plan(
            origin,
            &origin_plan,
            &preparation.origin_target,
            binding.capture_spec(),
            region,
            pictures,
            capture.budget,
        )
        .ok();
        let after = GenerationInputBinding::capture_with_plan(
            after,
            after_plan,
            &preparation.target,
            binding.capture_spec(),
            region,
            pictures,
            capture.budget,
        )
        .ok();
        return before.as_ref() == Some(binding) && after.as_ref() == Some(binding);
    }
    // Structural-only compatibility exists for legacy Bridge test callers.
    // It cannot establish temporal Extension inputs or an unresolved mode.
    let bridge = binding.map_or_else(
        || {
            preparation.origin.options().is_some_and(|options| {
                options.mode == deadpan_jobs::GenerationModePreference::Bridge
            })
        },
        |binding| binding.capture_spec() == GenerationCaptureSpec::Bridge,
    );
    if !bridge {
        return false;
    }
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
    let before = cached_plan(origin_cache, origin)
        .and_then(|plan| identity_in(origin, &plan, &preparation.origin_target, target.as_ref()));
    let after =
        after_plan.and_then(|plan| identity_in(after, plan, &preparation.target, target.as_ref()));
    matches!((before, after), (Some(before), Some(after)) if before == after)
}

fn observe_with_plan(
    origin_cache: &PlanCache,
    origin: &ProjectDocument,
    after: &ProjectDocument,
    after_plan: Option<&RenderPlan>,
    request: &StoredGenerationRequest,
    capture: Capture<'_>,
) -> ContextObservation {
    let target = request.constraints.region_target.as_ref();
    let same = if let Some(pictures) = capture.pictures {
        let spec = match &request.plan {
            Some(plan) if plan.conditioning() == request.constraints.conditioning => {
                GenerationCaptureSpec::from_plan(plan)
            }
            None if request.constraints.conditioning == deadpan_jobs::ConditioningMode::Bridge
                && request.input_binding.is_none() =>
            {
                GenerationCaptureSpec::Bridge
            }
            _ => return ContextObservation::Unresolved,
        };
        let Some(origin_plan) = cached_plan(origin_cache, origin) else {
            return ContextObservation::Unresolved;
        };
        let Some(after_plan) = after_plan else {
            return ContextObservation::Unresolved;
        };
        let before = GenerationInputBinding::capture_with_plan(
            origin,
            &origin_plan,
            &request.origin_target,
            spec,
            target,
            pictures,
            capture.budget,
        )
        .ok();
        let after = GenerationInputBinding::capture_with_plan(
            after,
            after_plan,
            &request.target,
            spec,
            target,
            pictures,
            capture.budget,
        )
        .ok();
        matches!((&before, &after), (Some(before), Some(after)) if before == after)
            && (request.plan.is_none() || request.input_binding.as_ref() == before.as_ref())
    } else {
        if request.constraints.conditioning != deadpan_jobs::ConditioningMode::Bridge
            || matches!(
                request.plan,
                Some(deadpan_jobs::GenerationPlan::Extension(_))
            )
        {
            return ContextObservation::Unresolved;
        }
        let before = cached_plan(origin_cache, origin)
            .and_then(|plan| identity_in(origin, &plan, &request.origin_target, target));
        let after = after_plan.and_then(|plan| identity_in(after, plan, &request.target, target));
        matches!((before, after), (Some(before), Some(after)) if before == after)
    };
    if same {
        ContextObservation::Resolved(request.binding.context_sha256.clone())
    } else {
        ContextObservation::Unresolved
    }
}

#[cfg(test)]
mod tests;
