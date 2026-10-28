//! Metadata for the existing numerical new-node path. This preserves the
//! current context selection; it is not an exact cold ancestry certificate.

use super::*;

pub(super) struct Request<'request, 'plan> {
    pub(super) incoming: &'request Contexts<'plan>,
    pub(super) owner: &'request AudioOwnerClock<'plan>,
    pub(super) at: AudioSample,
    pub(super) kind: Temporal,
    // Compute this before the inherited cache lookup, exactly as before.
    pub(super) stage: usize,
    pub(super) original_only: bool,
}

pub(super) struct Description<'plan> {
    pub(super) context: Contexts<'plan>,
    pub(super) boundary: AudioTreatmentBoundary<'plan>,
    pub(super) stop: Boundary,
    pub(super) kind: Temporal,
}

/// No renderer, Graph mutation, provider, native processor or cache belongs
/// here. Query, spend, clone and error order is the inherited numerical body.
pub(super) fn describe<'plan>(
    plan: &'plan RenderPlan,
    request: Request<'_, 'plan>,
    control: WorkControl<'_>,
) -> Result<Description<'plan>, StageAudioError> {
    let Request {
        incoming,
        owner,
        at,
        kind,
        stage,
        original_only,
    } = request;
    let key = OwnerKey::new(owner);
    let sounds = if !original_only && owner.kind() == deadpan_plan::AudioOwnerKind::Node {
        let context =
            plan.audio_sound_instance_context(owner.instance(), control.query_limits()?)?;
        control.spend_plan_work(context.construction_work())?;
        Sounds::Owner(Arc::new(context))
    } else {
        Sounds::None
    };
    let range = incoming.original.support(
        plan,
        at..AudioSample(at.0.checked_add(1).ok_or(TimeError::Overflow)?),
    )?;
    let original = if let Some(original) =
        original_context(plan, &incoming.original, &key, range, at, control)?
    {
        original
    } else if let Sounds::Owner(context) = &sounds {
        Context::Domain(Arc::new(context.processing_domain().clone()))
    } else {
        return Err(PlanError::InvalidPlan("missing shared EQ Original context").into());
    };
    let boundary = owner.treatment_boundary(stage)?;
    Ok(Description {
        context: Contexts { original, sounds },
        boundary,
        stop: Boundary {
            owner: key,
            before: stage,
        },
        kind,
    })
}

/// Reuse the first actual Root/Domain membership query for capture. The
/// selected clock still follows the inherited first OwnerKey match.
fn original_context<'plan>(
    plan: &'plan RenderPlan,
    incoming: &Context<'plan>,
    key: &OwnerKey,
    range: Range<AudioSample>,
    at: AudioSample,
    control: WorkControl<'_>,
) -> Result<Option<Context<'plan>>, StageAudioError> {
    if range.start >= range.end {
        return Ok(None);
    }
    let candidates = match incoming {
        Context::Root => plan.audio_owner_input_candidates_at(at, control.query_limits()?)?,
        Context::Domain(domain) => domain.owner_input_candidates_at(at, control.query_limits()?)?,
        Context::Retained(_) => {
            // This branch already has no repeated membership query. Keep its
            // actual retained-grid query, clock clone and immediate Arc clone.
            let observed = {
                let query = incoming.owners(plan, range, control)?;
                query
                    .spans()
                    .iter()
                    .flat_map(|span| span.owners())
                    .find(|clock| OwnerKey::new(clock) == *key)
                    .cloned()
            };
            return if let Some(clock) = observed {
                Ok(Some(incoming.owner_context(plan, &clock, at, control)?))
            } else {
                Ok(None)
            };
        }
    };
    Context::admit_owner_query(candidates.query(), control)?;
    let selected = candidates.query().spans().first().and_then(|span| {
        span.owners()
            .iter()
            .position(|clock| OwnerKey::new(clock) == *key)
    });
    let capture = if let Some(index) = selected {
        let capture = candidates.capture(index, control.query_limits()?)?;
        control.spend_plan_work(capture.capture_work())?;
        capture
    } else {
        // Absence has only the already-paid query, and must not request fresh
        // capture limits or spend a synthetic zero-work operation.
        candidates.without_selection()
    };
    if let Some(input) = capture.into_input() {
        Ok(Some(Context::finish_paid_owner_input(
            &input,
            input.owner(),
            at,
            control,
        )?))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
#[path = "node_description_tests.rs"]
mod tests;
