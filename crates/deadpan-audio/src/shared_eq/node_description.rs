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

pub(super) struct DescriptionValues<'plan> {
    pub(super) context: Contexts<'plan>,
    pub(super) boundary: AudioTreatmentBoundary<'plan>,
    pub(super) stop: Boundary,
    pub(super) kind: Temporal,
}

/// Owning context clones drop before the admitted carrier and its final lease.
pub(super) struct Description<'plan, I: Copy + Eq, L> {
    pub(super) values: Option<DescriptionValues<'plan>>,
    pub(super) original: Option<deadpan_plan::AdmittedOriginalContext<'plan, I, L>>,
    pub(super) sound: Option<deadpan_plan::AdmittedSoundContext<'plan, I, L>>,
}
impl<'plan, I: Copy + Eq, L> std::ops::Deref for Description<'plan, I, L> {
    type Target = DescriptionValues<'plan>;
    fn deref(&self) -> &Self::Target {
        self.values.as_ref().expect("unpublished description")
    }
}

#[cfg(test)]
pub(super) fn describe<'plan>(
    plan: &'plan RenderPlan,
    request: Request<'_, 'plan>,
    control: WorkControl<'_>,
) -> Result<Description<'plan, (), ()>, StageAudioError> {
    describe_admitted(
        plan,
        request,
        control,
        &mut sound_context_admission::Unmetered,
    )
}

/// No renderer, Graph mutation, provider, native processor or cache belongs
/// here. Root/Domain capture reuses its actual membership query; remaining
/// context-selection phases keep their checked numerical order.
pub(super) fn describe_admitted<'plan, A>(
    plan: &'plan RenderPlan,
    request: Request<'_, 'plan>,
    control: WorkControl<'_>,
    admission: &mut A,
) -> Result<Description<'plan, A::Issuer, A::Lease>, StageAudioError>
where
    A: deadpan_plan::AudioSoundContextAdmission<Error = StageAudioError>,
{
    let Request {
        incoming,
        owner,
        at,
        kind,
        stage,
        original_only,
    } = request;
    let key = OwnerKey::new(owner);
    let mut sound = if !original_only && owner.kind() == deadpan_plan::AudioOwnerKind::Node {
        let context = plan.audio_sound_instance_context_admitted(
            owner.instance(),
            control.query_limits()?,
            admission,
        )?;
        if !A::METERED {
            control.spend_plan_work(context.context().construction_work())?;
        }
        Some(context.into_shared(admission)?)
    } else {
        None
    };
    let range = incoming.original.support(
        plan,
        at..AudioSample(at.0.checked_add(1).ok_or(TimeError::Overflow)?),
    )?;
    let original = if let Some(original) = original_context_admitted(
        plan,
        &incoming.original,
        &key,
        range,
        at,
        control,
        admission,
    )? {
        original
    } else if let Some(context) = sound.take() {
        sound = Some(context.with_processing_domain(admission)?);
        ResolvedOriginal {
            context: Context::Domain(Arc::clone(
                sound
                    .as_ref()
                    .expect("created fallback")
                    .processing_domain()
                    .expect("processing Domain"),
            )),
            carrier: None,
        }
    } else {
        return Err(PlanError::InvalidPlan("missing shared EQ Original context").into());
    };
    let boundary = owner.treatment_boundary(stage)?;
    Ok(Description {
        values: Some(DescriptionValues {
            context: Contexts {
                original: original.context,
                sounds: sound.as_ref().map_or(Sounds::None, |context| {
                    Sounds::Owner(Arc::clone(context.shared()))
                }),
            },
            boundary,
            stop: Boundary {
                owner: key,
                before: stage,
            },
            kind,
        }),
        original: original.carrier,
        sound,
    })
}

/// Reuse the first actual Root/Domain membership query for capture. The
/// selected clock still follows the inherited first OwnerKey match.
struct ResolvedOriginal<'plan, I, L> {
    context: Context<'plan>,
    carrier: Option<deadpan_plan::AdmittedOriginalContext<'plan, I, L>>,
}

type OriginalResult<'plan, I, L> = Result<Option<ResolvedOriginal<'plan, I, L>>, StageAudioError>;

#[cfg(test)]
fn original_context<'plan>(
    plan: &'plan RenderPlan,
    incoming: &Context<'plan>,
    key: &OwnerKey,
    range: Range<AudioSample>,
    at: AudioSample,
    control: WorkControl<'_>,
) -> Result<Option<Context<'plan>>, StageAudioError> {
    Ok(original_context_admitted(
        plan,
        incoming,
        key,
        range,
        at,
        control,
        &mut sound_context_admission::Unmetered,
    )?
    .map(|value| value.context))
}

fn original_context_admitted<'plan, A>(
    plan: &'plan RenderPlan,
    incoming: &Context<'plan>,
    key: &OwnerKey,
    range: Range<AudioSample>,
    at: AudioSample,
    control: WorkControl<'_>,
    admission: &mut A,
) -> OriginalResult<'plan, A::Issuer, A::Lease>
where
    A: deadpan_plan::AudioSoundContextAdmission<Error = StageAudioError>,
{
    if range.start >= range.end {
        return Ok(None);
    }
    let candidates = match incoming {
        Context::Root => {
            plan.audio_owner_input_candidates_at_admitted(at, control.query_limits()?, admission)?
        }
        Context::Domain(domain) => {
            domain.owner_input_candidates_at_admitted(at, control.query_limits()?, admission)?
        }
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
                Ok(Some(ResolvedOriginal {
                    context: incoming.owner_context(plan, &clock, at, control)?,
                    carrier: None,
                }))
            } else {
                Ok(None)
            };
        }
    };
    if candidates.was_forwarded() != A::METERED {
        return Err(PlanError::InvalidPlan("owner query accounting mode differs").into());
    }
    if A::METERED {
        // Q was streamed by the actual Plan/Core kernel, including inspections.
        Context::validate_owner_query_layers(candidates.query())?;
    } else {
        Context::admit_owner_query(candidates.query(), control)?;
    }
    let selected = candidates.query().spans().first().and_then(|span| {
        span.owners()
            .iter()
            .position(|clock| OwnerKey::new(clock) == *key)
    });
    let capture = if let Some(index) = selected {
        // Q stays charged through the actual full capture. Metered C has
        // already reached the controller at its real construction sites.
        let capture = candidates.capture(index, control.query_limits()?, admission)?;
        if capture.was_forwarded() != A::METERED {
            return Err(PlanError::InvalidPlan("owner capture accounting mode differs").into());
        }
        if !capture.was_forwarded() {
            control.spend_plan_work(capture.capture_work())?;
        }
        capture
    } else {
        // Absence has only the already-paid query, and must not request fresh
        // capture limits or spend a synthetic zero-work operation.
        candidates.without_selection(admission)?
    };
    // The source Input remains charged throughout the same probe and final
    // sharing decision; the returned context owns its independent storage.
    if let Some(input) = capture.input() {
        let carrier = Context::finish_paid_owner_input_admitted(
            input,
            input.owner(),
            at,
            control,
            admission,
        )?;
        let context = if let Some(domain) = carrier.domain() {
            Context::Domain(Arc::clone(domain))
        } else {
            Context::Retained(Arc::clone(carrier.retained().expect("shared retained")))
        };
        Ok(Some(ResolvedOriginal {
            context,
            carrier: Some(carrier),
        }))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
#[path = "node_description_tests.rs"]
mod tests;
