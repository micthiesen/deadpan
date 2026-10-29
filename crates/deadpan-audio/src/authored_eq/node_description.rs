//! Preserve the Authored EQ node's exact direct owner resolution and ordering.
//! The source input stays admitted through the final Original sharing decision.

use super::*;

pub(super) struct Request<'request, 'plan> {
    pub(super) incoming: &'request Context<'plan>,
    pub(super) owner: &'request AudioOwnerClock<'plan>,
    pub(super) at: AudioSample,
    pub(super) stage: usize,
}

pub(super) enum Source<'query, 'context, 'plan, I, L> {
    Direct,
    SpanStart {
        candidates:
            &'query deadpan_plan::AdmittedAudioOwnerSpanStartCandidates<'context, 'plan, I, L>,
        span: usize,
    },
}

pub(super) struct Selection<'query, 'context, 'plan, I, L> {
    pub(super) request: Request<'query, 'plan>,
    pub(super) source: Source<'query, 'context, 'plan, I, L>,
}

pub(super) struct DescriptionValues<'plan, I: Copy + Eq, L> {
    pub(super) context: Context<'plan>,
    pub(super) boundary: deadpan_plan::AdmittedAudioTreatmentBoundary<'plan, I, L>,
    pub(super) recipe: deadpan_core::EqualizerRecipe,
}

// Owning handles drop before the carrier that admits their shared storage.
pub(super) struct Description<'plan, I: Copy + Eq, L> {
    pub(super) values: Option<DescriptionValues<'plan, I, L>>,
    pub(super) original: Option<deadpan_plan::AdmittedOriginalContext<'plan, I, L>>,
}

struct ResolvedOriginal<'plan, I, L> {
    context: Context<'plan>,
    carrier: Option<deadpan_plan::AdmittedOriginalContext<'plan, I, L>>,
}

impl<'plan, I: Copy + Eq, L> std::ops::Deref for Description<'plan, I, L> {
    type Target = DescriptionValues<'plan, I, L>;

    fn deref(&self) -> &Self::Target {
        self.values
            .as_ref()
            .expect("unpublished Authored description")
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

#[cfg(test)]
pub(super) fn describe_admitted<'plan, A>(
    plan: &'plan RenderPlan,
    request: Request<'_, 'plan>,
    control: WorkControl<'_>,
    admission: &mut A,
) -> Result<Description<'plan, A::Issuer, A::Lease>, StageAudioError>
where
    A: deadpan_plan::AudioSoundContextAdmission<Error = StageAudioError>,
{
    describe_selected_admitted(
        plan,
        Selection {
            request,
            source: Source::Direct,
        },
        control,
        admission,
    )
}

pub(super) fn describe_selected_admitted<'plan, A>(
    plan: &'plan RenderPlan,
    selection: Selection<'_, '_, 'plan, A::Issuer, A::Lease>,
    control: WorkControl<'_>,
    admission: &mut A,
) -> Result<Description<'plan, A::Issuer, A::Lease>, StageAudioError>
where
    A: deadpan_plan::AudioSoundContextAdmission<Error = StageAudioError>,
{
    let Selection { request, source } = selection;
    let Request {
        incoming,
        owner,
        at,
        stage,
    } = request;
    let original = match incoming {
        // Preserve the actual retained consumer and the old no-query path.
        Context::Retained(input) => ResolvedOriginal {
            context: Context::Retained(input.clone()),
            carrier: None,
        },
        Context::Root | Context::Domain(_) => {
            let limits = control.query_limits()?;
            match source {
                Source::Direct => {
                    let capture = match incoming {
                        Context::Root => {
                            plan.audio_owner_input_admitted(owner, at, limits, admission)?
                        }
                        Context::Domain(domain) => {
                            domain.owner_input_admitted(owner, at, limits, admission)?
                        }
                        Context::Retained(_) => unreachable!(),
                    };
                    if capture.was_forwarded() != A::METERED {
                        return Err(PlanError::InvalidPlan(
                            "Authored input accounting mode differs",
                        )
                        .into());
                    }
                    let input = capture.input().ok_or(PlanError::InvalidPlan(
                        "audio input owner is not current at sample",
                    ))?;
                    #[cfg(test)]
                    span_start::record_direct_point_query(
                        capture.query_canonical_units(),
                        capture.query_additional_units(),
                    );
                    finish_input(input, owner, at, control, admission)?
                }
                Source::SpanStart { candidates, span } => {
                    let request = deadpan_plan::AudioOwnerSpanStartRequest {
                        span_index: span,
                        requested: owner,
                        at,
                    };
                    let capture = match incoming {
                        Context::Root => {
                            candidates.capture_root(plan, request, limits, admission)?
                        }
                        Context::Domain(domain) => {
                            candidates.capture_domain(domain, request, limits, admission)?
                        }
                        Context::Retained(_) => unreachable!(),
                    };
                    if capture.was_forwarded() != A::METERED {
                        return Err(PlanError::InvalidPlan(
                            "Authored input accounting mode differs",
                        )
                        .into());
                    }
                    #[cfg(test)]
                    span_start::record_certified_capture(capture.work());
                    // The receipt borrows the live wide query. Its Input and
                    // independent lease remain live through probe and sharing.
                    finish_input(capture.input(), owner, at, control, admission)?
                }
            }
        }
    };
    let recipe = owner
        .treatments()
        .and_then(|recipe| recipe.equalizer())
        .ok_or(PlanError::InvalidPlan("EQ recipe absent"))?;
    let limits = if A::METERED {
        control.query_limits()?
    } else {
        deadpan_plan::AudioQueryLimits::default()
    };
    let boundary = owner.treatment_boundary_admitted(stage, limits, admission)?;
    if boundary.was_forwarded() != A::METERED {
        return Err(PlanError::InvalidPlan("Authored boundary accounting mode differs").into());
    }
    let issuer = admission.issuer();
    if boundary.issuer() != issuer
        || original
            .carrier
            .as_ref()
            .is_some_and(|value| value.issuer() != issuer)
    {
        return Err(PlanError::AudioMetadataIssuerMismatch.into());
    }
    Ok(Description {
        values: Some(DescriptionValues {
            context: original.context,
            boundary,
            recipe,
        }),
        original: original.carrier,
    })
}

fn finish_input<'plan, A>(
    input: &deadpan_plan::AudioOwnerInput<'plan>,
    owner: &AudioOwnerClock<'plan>,
    at: AudioSample,
    control: WorkControl<'_>,
    admission: &mut A,
) -> Result<ResolvedOriginal<'plan, A::Issuer, A::Lease>, StageAudioError>
where
    A: deadpan_plan::AudioSoundContextAdmission<Error = StageAudioError>,
{
    if !A::METERED {
        control.spend_plan_work(input.work())?;
    }
    let carrier = Context::finish_paid_owner_input_admitted(input, owner, at, control, admission)?;
    let context = if let Some(domain) = carrier.domain() {
        Context::Domain(Arc::clone(domain))
    } else {
        Context::Retained(carrier.retained().expect("shared retained").clone())
    };
    Ok(ResolvedOriginal {
        context,
        carrier: Some(carrier),
    })
}
