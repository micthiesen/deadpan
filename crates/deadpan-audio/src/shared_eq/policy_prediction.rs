//! Current-chunk policy availability and prediction, before numerical input.
//! Complete policy is not intrinsic-processing or recursive-resource eligibility.

use super::*;
use deadpan_plan::AudioOwnerQuery;

/// Only this producer can create the completion capability, after its entire
/// fallible population walk. Allocated/all-false flags alone grant nothing.
pub(in crate::stages) struct PopulationComplete(());

pub(in crate::stages) enum OriginalPolicy<'a, 'plan> {
    Queries(RootReadQueriesRef<'a, 'plan>),
    Cached(&'a ReadBlock),
}

pub(in crate::stages) struct Original<'a, 'plan> {
    pub(super) policy: OriginalPolicy<'a, 'plan>,
    pub(super) owners: &'a AudioOwnerQuery<'plan>,
    pub(super) routes: &'a [Option<shared_mix::Route<'plan>>],
    pub(super) id: usize,
}

pub(in crate::stages) struct Inputs<'a, 'plan, 'history> {
    pub(in crate::stages) original: Option<Original<'a, 'plan>>,
    pub(in crate::stages) roots: &'a [sound_events::PreparedSoundQuery<'plan>],
    pub(in crate::stages) root_range: Range<AudioSample>,
    pub(in crate::stages) root_ids: &'a [usize],
    pub(in crate::stages) root_routes: &'a [shared_mix::Route<'plan>],
    pub(in crate::stages) beats: &'a [beat_sounds::PreparedBeatSound<'plan, 'history>],
    pub(in crate::stages) beat_routes: &'a [(usize, shared_mix::Route<'plan>)],
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
    mix: &mut SharedMix<'_, '_>,
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
    mix: &mut SharedMix<'_, '_>,
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
        match &original.policy {
            OriginalPolicy::Queries(queries) => {
                let samples = &queries.processing.samples;
                raw_policy::visit(
                    *queries,
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
            OriginalPolicy::Cached(block) => {
                control.spend_plan_work(1)?;
                let samples = original.owners.samples();
                let frames = count(&samples)? as usize;
                if block.start != samples.start || block.samples.len() != frames {
                    return Err(
                        PlanError::InvalidPlan("cached Original prediction range differs").into(),
                    );
                }
                // These are the exact successful outer read's policy masks,
                // not zero PCM or recursively inherited child suppression.
                for ranges in [&block.suppressed, &block.exhausted] {
                    policy_ranges::visit(
                        block.start,
                        frames,
                        ranges,
                        |sample| sample.0,
                        || control.spend_plan_work(1),
                        |mask, _| {
                            for (span, route) in original.owners.spans().iter().zip(original.routes)
                            {
                                if let Some(route) = route {
                                    exclude(mix, route, &span.samples(), mask, control)?;
                                }
                            }
                            Ok(())
                        },
                    )?;
                }
            }
        }
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

/// Cold planning explicitly requests and pays for completion validation.
/// Scalar numerical reads only populate their prediction and later compare
/// independent observations; they do not need an unused completion witness.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "Cold admission awaits recursive resource proof")
)]
pub(super) fn populate_completed<'mix, 'plan>(
    mix: &'mix mut SharedMix<'plan, '_>,
    inputs: Inputs<'_, '_, '_>,
    admitted: impl Fn(usize) -> bool,
    control: WorkControl<'_>,
) -> Result<shared_components::CompletedPolicy<'mix, 'plan>, StageAudioError> {
    populate(mix, inputs, admitted, control)?;
    mix.complete_policy(PopulationComplete(()), control)
}

#[cfg(test)]
pub(in crate::stages) fn populate_roots_for_test<'mix, 'plan>(
    mix: &'mix mut SharedMix<'plan, '_>,
    roots: &[sound_events::PreparedSoundQuery<'plan>],
    samples: Range<AudioSample>,
    ids: &[usize],
    routes: &[shared_mix::Route<'plan>],
    control: WorkControl<'_>,
) -> Result<shared_components::CompletedPolicy<'mix, 'plan>, StageAudioError> {
    populate_completed(
        mix,
        Inputs {
            original: None,
            roots,
            root_range: samples,
            root_ids: ids,
            root_routes: routes,
            beats: &[],
            beat_routes: &[],
        },
        |_| true,
        control,
    )
}

#[cfg(test)]
pub(in crate::stages) fn populate_inputs_for_test<'mix, 'plan>(
    mix: &'mix mut SharedMix<'plan, '_>,
    inputs: Inputs<'_, '_, '_>,
    admitted: impl Fn(usize) -> bool,
    control: WorkControl<'_>,
) -> Result<shared_components::CompletedPolicy<'mix, 'plan>, StageAudioError> {
    populate_completed(mix, inputs, admitted, control)
}

#[cfg(test)]
#[path = "policy_prediction_tests.rs"]
mod tests;
