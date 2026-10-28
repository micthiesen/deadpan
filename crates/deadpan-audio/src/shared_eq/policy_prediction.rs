//! Current-chunk policy availability and prediction, before numerical input.
//! Complete policy is not intrinsic-processing or recursive-resource eligibility.

use super::*;
use deadpan_plan::AudioOwnerQuery;

pub(super) struct Original<'a, 'plan> {
    pub(super) queries: RootReadQueriesRef<'a, 'plan>,
    pub(super) owners: &'a AudioOwnerQuery<'plan>,
    pub(super) routes: &'a [Option<shared_mix::Route<'plan>>],
    pub(super) id: usize,
}

pub(super) struct Inputs<'a, 'plan, 'history> {
    pub(super) original: Option<Original<'a, 'plan>>,
    pub(super) roots: &'a [sound_events::PreparedSoundQuery<'plan>],
    pub(super) root_range: Range<AudioSample>,
    pub(super) root_ids: &'a [usize],
    pub(super) root_routes: &'a [shared_mix::Route<'plan>],
    pub(super) beats: &'a [beat_sounds::PreparedBeatSound<'plan, 'history>],
    pub(super) beat_routes: &'a [(usize, shared_mix::Route<'plan>)],
}

pub(super) fn validate_original(
    request: &original_request::PreparedOriginalRequest<'_, '_>,
    context: &Context<'_>,
    plan: &RenderPlan,
    start: AudioSample,
    frames: u32,
    control: WorkControl<'_>,
) -> Result<(), StageAudioError> {
    control.spend_plan_work(1)?;
    let matches = match context {
        Context::Root => request.matches_root(plan, start, frames, true),
        Context::Domain(domain) => request.matches_domain(plan, domain, start, frames, true),
        Context::Retained(_) => false,
    };
    if !matches {
        return Err(StageAudioError::ForeignDomain);
    }
    Ok(())
}

pub(super) fn available(
    original: &Context<'_>,
    roots: &[sound_events::PreparedSoundQuery<'_>],
    beats: &[beat_sounds::PreparedBeatSound<'_, '_>],
    control: WorkControl<'_>,
) -> Result<bool, StageAudioError> {
    control.spend_plan_work(1)?;
    if matches!(original, Context::Retained(_)) {
        return Ok(false);
    }
    for root in roots {
        control.spend_plan_work(1)?;
        if root.input.policy_support() != policy_ranges::Support::Complete {
            return Ok(false);
        }
    }
    for beat in beats {
        for occurrence in &beat.occurrences {
            control.spend_plan_work(1)?;
            if occurrence.input.policy_support() != policy_ranges::Support::Complete {
                return Ok(false);
            }
        }
    }
    control.check()?;
    Ok(true)
}

pub(super) fn original_allowed(
    selected: bool,
    support: AudioOwnerSupport,
    raw_allowed: impl FnOnce() -> bool,
) -> bool {
    selected && support == AudioOwnerSupport::Active && raw_allowed()
}

pub(super) fn sound_allowed(
    selected: Option<usize>,
    gate_length: u64,
    raw_allowed: impl FnOnce() -> bool,
) -> Option<usize> {
    if gate_length != 0 && raw_allowed() {
        selected
    } else {
        None
    }
}

fn exclude(
    mix: &mut SharedMix<'_>,
    route: &shared_mix::Route<'_>,
    support: &Range<AudioSample>,
    mask: &Range<AudioSample>,
    control: WorkControl<'_>,
) -> Result<(), StageAudioError> {
    control.spend_plan_work(1)?;
    let range = support.start.max(mask.start)..support.end.min(mask.end);
    if range.start < range.end {
        mix.predict_range(route, range, false, control)?;
    }
    Ok(())
}

pub(super) fn populate(
    mix: &mut SharedMix<'_>,
    inputs: Inputs<'_, '_, '_>,
    admitted: impl Fn(usize) -> bool,
    control: WorkControl<'_>,
) -> Result<(), StageAudioError> {
    control.check()?;
    if let Some(original) = inputs.original {
        if original.owners.spans().len() != original.routes.len() {
            return Err(PlanError::InvalidPlan("Original prediction routes differ").into());
        }
        for (span, route) in original.owners.spans().iter().zip(original.routes) {
            control.spend_plan_work(1)?;
            if let Some(route) = route
                && original_allowed(admitted(original.id), span.support(), || true)
            {
                mix.predict_range(route, span.samples(), true, control)?;
            }
        }
        let samples = &original.queries.processing.samples;
        raw_policy::visit(
            original.queries,
            samples.start,
            count(samples)? as usize,
            control,
            || control.spend_plan_work(1),
            |event| {
                let mask = match event {
                    raw_policy::Event::SilentHold { range, .. }
                    | raw_policy::Event::Suppressed { range, .. }
                    | raw_policy::Event::Exhausted(range) => range,
                    raw_policy::Event::RetainedEnvelope { .. } => return Ok(()),
                };
                for (span, route) in original.owners.spans().iter().zip(original.routes) {
                    if let Some(route) = route {
                        exclude(mix, route, &span.samples(), &mask, control)?;
                    }
                }
                Ok(())
            },
        )?;
    }
    if inputs.roots.len() != inputs.root_ids.len() || inputs.roots.len() != inputs.root_routes.len()
    {
        return Err(PlanError::InvalidPlan("root sound prediction routes differ").into());
    }
    for ((root, id), route) in inputs
        .roots
        .iter()
        .zip(inputs.root_ids)
        .zip(inputs.root_routes)
    {
        control.spend_plan_work(1)?;
        let selected = admitted(*id).then_some(*id);
        // Selection alone proves an all-false row. Do not inspect gates or
        // raw policy that cannot change it; the actual read still admits media.
        if selected.is_none() {
            continue;
        }
        for gate in &root.fades {
            control.spend_plan_work(1)?;
            if sound_allowed(selected, gate.length, || true).is_some() {
                mix.predict_range(route, gate.samples.clone(), true, control)?;
            }
        }
        let support = root.input.visit_policy(control, |mask| {
            exclude(mix, route, &inputs.root_range, &mask, control)
        })?;
        if support != policy_ranges::Support::Complete {
            return Err(
                PlanError::InvalidPlan("root policy support changed during prediction").into(),
            );
        }
    }
    let mut routes = inputs.beat_routes.iter();
    for beat in inputs.beats {
        for occurrence in &beat.occurrences {
            control.spend_plan_work(1)?;
            let (id, route) = routes
                .next()
                .ok_or(PlanError::InvalidPlan("beat sound prediction route absent"))?;
            let selected = admitted(*id).then_some(*id);
            if selected.is_none() {
                continue;
            }
            for gate in &occurrence.fades {
                control.spend_plan_work(1)?;
                if sound_allowed(selected, gate.length, || true).is_some() {
                    mix.predict_range(route, gate.samples.clone(), true, control)?;
                }
            }
            let support =
                occurrence
                    .input
                    .visit_policy(occurrence.samples.clone(), control, |mask| {
                        exclude(mix, route, &occurrence.samples, &mask, control)
                    })?;
            if support != policy_ranges::Support::Complete {
                return Err(PlanError::InvalidPlan(
                    "beat policy support changed during prediction",
                )
                .into());
            }
        }
    }
    if routes.next().is_some() {
        return Err(PlanError::InvalidPlan("unused beat sound prediction route").into());
    }
    control.check()?;
    Ok(())
}

#[cfg(test)]
#[path = "policy_prediction_tests.rs"]
mod tests;
