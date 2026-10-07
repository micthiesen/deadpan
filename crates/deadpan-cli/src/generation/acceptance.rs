//! Explicit acceptance of a request's selected Ready bundle: one undoable
//! edit that registers the two masters and makes them the Hold's picture.

use deadpan_core::{AssetId, ProjectDocument, RevisionId};
use deadpan_jobs::RequestId;
use deadpan_store::generation::{GenerationContextResolver, RelevanceObservation, RelevancePlan};
use deadpan_store::generation_acceptance::GenerationAcceptance;
use deadpan_store::{CommitOutcome, ProjectStore};

use super::attempt::{GenerationError, object_limits};
use crate::generation_context::BoundaryContextResolver;

/// Observe every current request across `before` → `after` with the same
/// resolver ordinary writes use.
pub fn relevance_plan(
    store: &ProjectStore,
    before: &ProjectDocument,
    after: &ProjectDocument,
) -> Result<RelevancePlan, GenerationError> {
    relevance_plan_for_requests(store, before, after, store.current_generation_requests()?)
}

/// The store supplies these requests after applying the exact previewed
/// identity mapping. The worker binding remains its immutable origin.
pub fn relevance_plan_for_requests(
    store: &ProjectStore,
    before: &ProjectDocument,
    after: &ProjectDocument,
    requests: Vec<deadpan_store::generation::StoredGenerationRequest>,
) -> Result<RelevancePlan, GenerationError> {
    let resolver = BoundaryContextResolver::default();
    let prepared = (!requests.is_empty())
        .then(|| resolver.prepare_transition(after))
        .flatten();
    let resolver = prepared.as_deref().unwrap_or(&resolver);
    let mut observations = Vec::new();
    for request in requests {
        let origin = store.snapshot_at(&request.origin_revision)?;
        observations.push(RelevanceObservation {
            request_id: request.request_id.clone(),
            binding: request.binding.clone(),
            target: request.target.clone(),
            after_context: resolver.observe(&origin, after, &request),
        });
    }
    Ok(RelevancePlan {
        from_revision: before.revision_id().clone(),
        to_revision: after.revision_id().clone(),
        observations,
    })
}

/// The acceptance of `request`'s selected Ready bundle against the current
/// head, with fresh asset IDs derived from `new_revision`.
pub fn acceptance_for(
    store: &ProjectStore,
    request: &RequestId,
    new_revision: RevisionId,
) -> Result<GenerationAcceptance, GenerationError> {
    let selected = store.selected_generation_bundle(request)?.ok_or_else(|| {
        GenerationError::Invalid(
            "This AI pause has no Ready pictures to accept; generate it again.".into(),
        )
    })?;
    let asset = |kind: &str| {
        AssetId::new(format!("ai-hold-{}-{kind}", new_revision.as_str()))
            .map_err(|error| GenerationError::Invalid(error.to_string()))
    };
    Ok(GenerationAcceptance {
        expected_revision: store.head_revision()?,
        native_asset: asset("native")?,
        sampled_asset: asset("sampled")?,
        new_revision,
        identity: selected.identity,
        expected_receipt: selected.receipt,
    })
}

/// Accept `request`'s selected Ready bundle as one undoable edit. The
/// accepted request must stay current: its boundary context is unchanged.
pub fn accept(
    store: &mut ProjectStore,
    request: &RequestId,
    new_revision: RevisionId,
) -> Result<CommitOutcome, GenerationError> {
    let acceptance = acceptance_for(store, request, new_revision)?;
    let limits = object_limits();
    let before = store.snapshot()?;
    let (edit, requests) = store.preview_generation_acceptance_contexts(&acceptance, limits)?;
    let after = edit
        .forward
        .apply(&before)
        .map_err(|error| GenerationError::Invalid(error.to_string()))?;
    let relevance = relevance_plan_for_requests(store, &before, &after, requests)?;
    Ok(store.accept_generation_bundle(&acceptance, &relevance, limits)?)
}
