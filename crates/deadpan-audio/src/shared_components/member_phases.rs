//! One owner-phase sequence for numerical execution and completed cold policy.
//! Cold dispatch never evaluates PCM/gain or invokes a numerical callback.
//! Its sealed completed-policy witness is supplied by the actual producer.

use super::*;

pub(super) enum Numerics<'a, 'samples> {
    Block {
        output: &'a mut OutputSamples<'samples>,
    },
    Sample {
        at: usize,
        gains: &'a mut [authored_gain::GainSum],
        sent: &'a mut [f64; 2],
    },
}

pub(super) struct OwnerPhase<'a, 'plan> {
    pub(super) permitted: &'a permissions::SamplePermissions,
    pub(super) owner: &'a Owner<'plan>,
    pub(super) start: AudioSample,
    pub(super) frames: usize,
    pub(super) send: Option<deadpan_core::TailEffect>,
}

/// The producer owns completion. This read-only schedule binds that witness
/// to its exact execution order without retaining an activity Vec or queries.
pub(super) struct ColdOrder<'a, 'view, 'plan> {
    completed: &'a CompletedPolicy<'view, 'plan>,
    block: bool,
}

impl ColdOrder<'_, '_, '_> {
    pub(super) fn is_block(&self) -> bool {
        self.block
    }
}

pub(super) fn cold_order<'a, 'view, 'plan>(
    routes: &[Route<'plan>],
    owners: &BTreeMap<OwnerKey, Owner<'plan>>,
    completed: &'a CompletedPolicy<'view, 'plan>,
    samples: (AudioSample, usize),
    control: WorkControl<'_>,
) -> Result<ColdOrder<'a, 'view, 'plan>, StageAudioError> {
    control.check()?;
    completed.validate(routes, samples.0, samples.1, control)?;
    let mut block = true;
    for (id, route) in routes.iter().enumerate() {
        if completed.active(id, control)? {
            // The constant predicate is shared with Program preparation.
            // Parent supplies its inspected form for exact short-circuit work.
            if !Program::constant_inspected(route, owners, || control.spend_plan_work(1))? {
                block = false;
                break;
            }
        }
    }
    Ok(ColdOrder { completed, block })
}

/// Existing numerical callers retain their callback and error boundaries.
pub(super) fn run_owner<'plan>(
    routes: &mut [Route<'plan>],
    phase: OwnerPhase<'_, 'plan>,
    numerics: Numerics<'_, '_>,
    control: WorkControl<'_>,
    equalize: &mut impl FnMut(
        &AudioOwnerClock<'plan>,
        Temporal,
        AudioSample,
        Permissions,
        Option<usize>,
    ) -> Result<[f64; 2], StageAudioError>,
) -> Result<(), StageAudioError> {
    drive_owner(
        Routes::Numeric(routes),
        phase,
        Mode::Numeric(numerics),
        control,
        Dispatch::Numeric(equalize),
        None,
    )
}

/// A cold caller supplies a real completed-policy witness and uses this order's
/// block/single-sample adapter. No row allocation or numerical observation can
/// construct that witness in this module.
pub(super) fn run_cold_owner<'plan>(
    routes: &[Route<'plan>],
    phase: OwnerPhase<'_, 'plan>,
    order: &ColdOrder<'_, '_, 'plan>,
    at: Option<usize>,
    control: WorkControl<'_>,
    demand: &mut impl for<'phase> FnMut(
        visitors::TemporalDemand<'phase, 'plan>,
    ) -> Result<(), StageAudioError>,
) -> Result<(), StageAudioError> {
    control.check()?;
    order
        .completed
        .validate(routes, phase.start, phase.frames, control)?;
    control.spend_plan_work(1)?;
    if !std::ptr::eq(phase.permitted, order.completed.flags()) {
        return Err(PlanError::InvalidPlan("cold contribution policy differs").into());
    }
    control.spend_plan_work(1)?;
    if (order.block && at.is_some()) || (!order.block && at.is_none_or(|at| at >= phase.frames)) {
        return Err(PlanError::InvalidPlan("cold contribution extent differs").into());
    }
    drive_owner(
        Routes::Cold(routes),
        phase,
        Mode::Cold { at },
        control,
        Dispatch::Demand(demand),
        Some(order.completed),
    )
}

enum Routes<'a, 'plan> {
    Numeric(&'a mut [Route<'plan>]),
    Cold(&'a [Route<'plan>]),
}

impl<'routes, 'plan> Routes<'routes, 'plan> {
    fn get(&self) -> &[Route<'plan>] {
        match self {
            Self::Numeric(routes) => routes,
            Self::Cold(routes) => routes,
        }
    }

    fn cold(&self) -> Result<&'routes [Route<'plan>], StageAudioError> {
        match self {
            Self::Cold(routes) => Ok(*routes),
            Self::Numeric(_) => {
                Err(PlanError::InvalidPlan("numerical contribution requested cold storage").into())
            }
        }
    }

    fn numeric(&mut self) -> Result<&mut [Route<'plan>], StageAudioError> {
        match self {
            Self::Numeric(routes) => Ok(routes),
            Self::Cold(_) => {
                Err(PlanError::InvalidPlan("cold contribution requested numerical state").into())
            }
        }
    }
}

enum Mode<'a, 'samples> {
    Numeric(Numerics<'a, 'samples>),
    Cold { at: Option<usize> },
}

impl Mode<'_, '_> {
    fn samples(&self, frames: usize) -> Range<usize> {
        match self {
            Self::Numeric(Numerics::Block { .. }) | Self::Cold { at: None } => 0..frames,
            Self::Numeric(Numerics::Sample { at, .. }) | Self::Cold { at: Some(at) } => {
                *at..*at + 1
            }
        }
    }

    fn is_block(&self) -> bool {
        matches!(
            self,
            Self::Numeric(Numerics::Block { .. }) | Self::Cold { at: None }
        )
    }

    fn is_cold(&self) -> bool {
        matches!(self, Self::Cold { .. })
    }

    fn at(&self) -> Option<usize> {
        match self {
            Self::Numeric(Numerics::Sample { at, .. }) | Self::Cold { at: Some(at) } => Some(*at),
            _ => None,
        }
    }
}

type NumericCallback<'callback, 'plan> = dyn FnMut(
        &AudioOwnerClock<'plan>,
        Temporal,
        AudioSample,
        Permissions,
        Option<usize>,
    ) -> Result<[f64; 2], StageAudioError>
    + 'callback;

enum Dispatch<'a, 'plan> {
    Numeric(&'a mut NumericCallback<'a, 'plan>),
    Demand(
        &'a mut dyn for<'phase> FnMut(
            visitors::TemporalDemand<'phase, 'plan>,
        ) -> Result<(), StageAudioError>,
    ),
}

impl<'plan> Dispatch<'_, 'plan> {
    fn call(
        &mut self,
        request: visitors::TemporalDemand<'_, 'plan>,
    ) -> Result<Option<[f64; 2]>, StageAudioError> {
        match self {
            Self::Numeric(read) => Ok(Some(request.read(read)?)),
            Self::Demand(visit) => {
                visit(request)?;
                Ok(None)
            }
        }
    }
}

fn inspect(cold: bool, control: WorkControl<'_>) -> Result<(), StageAudioError> {
    if cold {
        control.spend_plan_work(1)?;
    }
    Ok(())
}

fn permissions(
    routes: &[Route<'_>],
    owner: &Owner<'_>,
    flags: &permissions::SamplePermissions,
    at: usize,
    wet: bool,
    cold: bool,
    control: WorkControl<'_>,
) -> Result<Permissions, StageAudioError> {
    if cold {
        policy::permissions_inspected(
            routes,
            owner,
            |id| flags.contains(id, at),
            wet,
            || control.spend_plan_work(1),
        )
    } else {
        policy::permissions(routes, owner, |id| flags.contains(id, at), wet)
    }
}

/// Every owner-stage/sample/member/effect loop is shared. Numeric work remains
/// inside synchronous branches, so its failures stop before later demands.
fn drive_owner<'plan>(
    mut routes: Routes<'_, 'plan>,
    phase: OwnerPhase<'_, 'plan>,
    mut mode: Mode<'_, '_>,
    control: WorkControl<'_>,
    mut dispatch: Dispatch<'_, 'plan>,
    completed: Option<&CompletedPolicy<'_, 'plan>>,
) -> Result<(), StageAudioError> {
    let OwnerPhase {
        permitted,
        owner,
        start,
        frames,
        send,
    } = phase;
    let cold = mode.is_cold();
    inspect(cold, control)?; // owner probe
    let sample_permissions = if let Some(at) = mode.at() {
        let mask = permissions(routes.get(), owner, permitted, at, false, cold, control)?;
        if mask.is_empty() {
            return Ok(());
        }
        if let Mode::Numeric(Numerics::Sample { gains, .. }) = &mut mode {
            let sample = AudioSample(start.0 + at as i64);
            let routes = routes.numeric()?;
            for &(id, layer) in &owner.members {
                if permitted.contains(id, at) && !owner.skipped && routes[id].program.is_none() {
                    gains[id].owner_slice(
                        &routes[id].layers[layer].0,
                        sample,
                        0..0,
                        true,
                        control,
                    )?;
                }
            }
        }
        Some(mask)
    } else {
        control.check()?;
        None
    };
    let recipe = owner.clock.treatments();
    for stage in 0..owner.until {
        inspect(cold, control)?;
        let kind = recipe
            .and_then(|recipe| recipe.order().get(stage))
            .copied()
            .ok_or(PlanError::InvalidPlan("contribution stage absent"))?;
        let eq = policy::is_equalizer(kind, recipe);
        if kind == AudioTreatmentStage::Saturation || eq {
            if matches!(mode, Mode::Numeric(Numerics::Block { .. })) {
                let routes = routes.numeric()?;
                for &(id, _) in &owner.members {
                    routes[id].transfer_block(permitted.row(id), control)?;
                }
            }
            for at in mode.samples(frames) {
                inspect(cold, control)?;
                if mode.is_block() {
                    control.check()?;
                }
                let mask = if let Some(mask) = sample_permissions {
                    mask
                } else {
                    permissions(routes.get(), owner, permitted, at, false, cold, control)?
                };
                if mode.is_block() && mask.is_empty() {
                    continue;
                }
                let saturation = recipe.and_then(|recipe| recipe.saturation());
                if !cold {
                    let routes = routes.numeric()?;
                    let mut sum = [0.0; 2];
                    let mut count = 0;
                    for &(id, _) in &owner.members {
                        if permitted.contains(id, at) {
                            count += 1;
                            let route = &mut routes[id];
                            match &mut mode {
                                Mode::Numeric(Numerics::Block { .. }) => {
                                    for (out, value) in sum.iter_mut().zip(route.pcm[at].value) {
                                        *out += value;
                                    }
                                }
                                Mode::Numeric(Numerics::Sample { gains, .. }) => {
                                    let frame = route.apply_gain(
                                        &mut gains[id],
                                        route.pcm[at].value,
                                        control,
                                    )?;
                                    route.pcm[at].value = frame;
                                    for channel in 0..2 {
                                        sum[channel] += frame[channel];
                                    }
                                }
                                Mode::Cold { .. } => unreachable!("checked numerical mode"),
                            }
                        }
                    }
                    if sum.iter().any(|value| !value.is_finite()) {
                        return Err(PreparationError::InvalidSamples.into());
                    }
                    let factor = if kind == AudioTreatmentStage::Saturation {
                        let saturation = saturation
                            .ok_or(PlanError::InvalidPlan("contribution saturation absent"))?;
                        sum.map(|value| {
                            if value == 0.0 {
                                saturation.drive_factor()
                            } else {
                                saturation.shape(value) / value
                            }
                        })
                    } else {
                        [1.0; 2]
                    };
                    if !eq {
                        for &(id, _) in &owner.members {
                            let route = &mut routes[id];
                            if permitted.contains(id, at) {
                                let frame = &mut route.pcm[at];
                                frame.value = if count == 1 {
                                    let saturation = saturation.ok_or(PlanError::InvalidPlan(
                                        "contribution saturation absent",
                                    ))?;
                                    sum.map(|value| saturation.shape(value))
                                } else {
                                    std::array::from_fn(|ch| frame.value[ch] * factor[ch])
                                };
                                if frame.value.iter().any(|value| !value.is_finite()) {
                                    return Err(PreparationError::InvalidSamples.into());
                                }
                            }
                        }
                    }
                } else if kind == AudioTreatmentStage::Saturation {
                    // Validate structure without evaluating a numerical factor.
                    inspect(true, control)?;
                    saturation.ok_or(PlanError::InvalidPlan("contribution saturation absent"))?;
                }
                if eq {
                    // Numeric clones once at its existing point; Cold borrows.
                    let owned_clock;
                    let clock = if cold {
                        policy::first_clock_ref_inspected(
                            routes.cold()?,
                            owner,
                            |id| permitted.contains(id, at),
                            false,
                            || control.spend_plan_work(1),
                        )?
                    } else {
                        owned_clock = policy::first_clock(
                            routes.get(),
                            owner,
                            |id| permitted.contains(id, at),
                            false,
                        );
                        owned_clock.as_ref()
                    };
                    let phase = visitors::EqualizerPhase {
                        owner,
                        permitted,
                        at,
                        sample: AudioSample(start.0 + at as i64),
                        stage,
                        permissions: &mask,
                        clock,
                    };
                    inspect(cold, control)?; // visitor carrier-shape validation
                    visitors::visit_equalizer_members(
                        routes.get().len(),
                        &phase,
                        || inspect(cold, control),
                        |id| {
                            let request = phase.demand(routes.get()[id].voice)?;
                            if let Some(frame) = dispatch.call(request)? {
                                let route = &mut routes.numeric()?[id];
                                route.pcm[at].value = frame;
                                if frame.iter().any(|value| !value.is_finite()) {
                                    return Err(PreparationError::InvalidSamples.into());
                                }
                            }
                            Ok(())
                        },
                    )?;
                }
            }
        } else if let Mode::Numeric(Numerics::Sample { at, gains, .. }) = &mut mode {
            let sample = AudioSample(start.0 + *at as i64);
            let routes = routes.numeric()?;
            for &(id, layer) in &owner.members {
                if permitted.contains(id, *at) && routes[id].program.is_none() {
                    gains[id].owner_slice(
                        &routes[id].layers[layer].0,
                        sample,
                        stage..stage + 1,
                        false,
                        control,
                    )?;
                }
            }
        }
    }
    if let Some(effect) = send {
        // Cold skips this purely numerical send accumulator altogether.
        if !cold {
            let routes = routes.numeric()?;
            for &(id, layer) in &owner.members {
                match &mut mode {
                    Mode::Numeric(Numerics::Block { output }) => {
                        if routes[id].layers[layer]
                            .0
                            .treatments()
                            .and_then(|recipe| recipe.tail_sends().level(effect))
                            .is_some()
                        {
                            routes[id].send_block(output, false, permitted.row(id), control)?;
                        }
                    }
                    Mode::Numeric(Numerics::Sample { at, gains, sent }) => {
                        let route = &mut routes[id];
                        if permitted.contains(id, *at)
                            && let Some(level) = route.layers[layer]
                                .0
                                .treatments()
                                .and_then(|recipe| recipe.tail_sends().level(effect))
                        {
                            control.spend_plan_work(1)?;
                            let value = route.send_sample(
                                &gains[id],
                                route.pcm[*at].value,
                                level,
                                control,
                            )?;
                            for ch in 0..2 {
                                sent[ch] += value[ch];
                            }
                        }
                    }
                    Mode::Cold { .. } => unreachable!("checked numerical mode"),
                }
            }
        }
    }
    inspect(cold, control)?; // wet owner eligibility
    if owner.wet {
        let owned_context = if !cold && let Some(at) = mode.at() {
            Some(policy::wet_context(routes.get(), owner, |id| {
                permitted.contains(id, at)
            })?)
        } else {
            None
        };
        let context = if cold && let Some(at) = mode.at() {
            Some(policy::wet_context_ref_inspected(
                routes.cold()?,
                owner,
                |id| permitted.contains(id, at),
                || control.spend_plan_work(1),
            )?)
        } else {
            owned_context.as_ref().map(|owned| policy::WetContextRef {
                permissions: owned.permissions,
                clock: owned.clock.as_ref(),
            })
        };
        for &(id, layer) in &owner.members {
            inspect(cold, control)?;
            let route = &routes.get()[id];
            if let Some(at) = mode.at()
                && !permitted.contains(id, at)
            {
                continue;
            }
            let allowed = if cold {
                route.wet_allowed_inspected(layer, || control.spend_plan_work(1))?
            } else {
                route.wet_allowed(layer)
            };
            if !allowed {
                continue;
            }
            if mode.is_block() {
                let active = if cold {
                    completed
                        .ok_or(PlanError::InvalidPlan(
                            "cold contribution completion absent",
                        ))?
                        .active(id, control)?
                } else {
                    route.program.is_some()
                };
                if !active {
                    continue;
                }
            }
            inspect(cold, control)?;
            let sends = route.layers[layer]
                .0
                .treatments()
                .map(|recipe| recipe.wet_sends())
                .unwrap_or_default();
            if sends.is_empty() {
                continue;
            }
            let samples = mode.samples(frames);
            let block = mode.is_block();
            match &mut mode {
                Mode::Cold { .. } => {
                    wet_samples_cold_or_numeric(
                        Routes::Cold(routes.cold()?),
                        WetSamples {
                            permitted,
                            owner,
                            id,
                            start,
                            samples,
                            sends,
                            context: context.as_ref(),
                            check_samples: block,
                        },
                        WetNumerics::Cold,
                        control,
                        &mut dispatch,
                    )?;
                }
                Mode::Numeric(Numerics::Block { .. }) => {
                    let routes = routes.numeric()?;
                    routes[id].transfer_block(permitted.row(id), control)?;
                    let mut program = routes[id].program.take().ok_or(PlanError::InvalidPlan(
                        "constant contribution program absent",
                    ))?;
                    let count =
                        usize::from(sends.level(deadpan_core::TailEffect::Reverb).is_some())
                            + usize::from(sends.level(deadpan_core::TailEffect::Delay).is_some());
                    let transfers = program.next_transfers(count, control)?;
                    wet_samples_cold_or_numeric(
                        Routes::Numeric(routes),
                        WetSamples {
                            permitted,
                            owner,
                            id,
                            start,
                            samples,
                            sends,
                            context: None,
                            check_samples: true,
                        },
                        WetNumerics::Block(transfers),
                        control,
                        &mut dispatch,
                    )?;
                    routes[id].program = Some(program);
                }
                Mode::Numeric(Numerics::Sample { gains, .. }) => {
                    wet_samples_cold_or_numeric(
                        Routes::Numeric(routes.numeric()?),
                        WetSamples {
                            permitted,
                            owner,
                            id,
                            start,
                            samples,
                            sends,
                            context: context.as_ref(),
                            check_samples: false,
                        },
                        WetNumerics::Sample(&mut gains[id]),
                        control,
                        &mut dispatch,
                    )?;
                }
            }
        }
    }
    Ok(())
}

struct WetSamples<'a, 'plan> {
    permitted: &'a permissions::SamplePermissions,
    owner: &'a Owner<'plan>,
    id: usize,
    start: AudioSample,
    samples: Range<usize>,
    sends: deadpan_core::TailSends,
    context: Option<&'a policy::WetContextRef<'a, 'plan>>,
    check_samples: bool,
}

enum WetNumerics<'a> {
    Block(&'a [authored_gain::PreparedTransfer]),
    Sample(&'a mut authored_gain::GainSum),
    Cold,
}

fn wet_samples_cold_or_numeric<'plan>(
    mut routes: Routes<'_, 'plan>,
    phase: WetSamples<'_, 'plan>,
    mut numerics: WetNumerics<'_>,
    control: WorkControl<'_>,
    dispatch: &mut Dispatch<'_, 'plan>,
) -> Result<(), StageAudioError> {
    let WetSamples {
        permitted,
        owner,
        id,
        start,
        samples,
        sends,
        context,
        check_samples,
    } = phase;
    let cold = matches!(numerics, WetNumerics::Cold);
    for at in samples {
        inspect(cold, control)?;
        if check_samples {
            control.check()?;
            if !permitted.contains(id, at) {
                continue;
            }
        }
        let local;
        let owned_clock;
        let context = if let Some(context) = context {
            context
        } else {
            // Block keeps separate permission and first-clock walks.
            let mask = permissions(routes.get(), owner, permitted, at, true, cold, control)?;
            let clock = if cold {
                policy::first_clock_ref_inspected(
                    routes.cold()?,
                    owner,
                    |member| permitted.contains(member, at),
                    true,
                    || control.spend_plan_work(1),
                )?
            } else {
                owned_clock = policy::first_clock(
                    routes.get(),
                    owner,
                    |member| permitted.contains(member, at),
                    true,
                );
                owned_clock.as_ref()
            };
            local = policy::WetContextRef {
                permissions: mask,
                clock,
            };
            &local
        };
        if let WetNumerics::Sample(gain) = &mut numerics {
            let route = &mut routes.numeric()?[id];
            route.pcm[at].value = route.apply_gain(gain, route.pcm[at].value, control)?;
        }
        let mut effect = 0;
        visitors::visit_wet_effects(
            visitors::WetPhase {
                sample: AudioSample(start.0 + at as i64),
                permissions: &context.permissions,
                clock: context.clock,
                voice: routes.get()[id].voice,
                sends,
            },
            || inspect(cold, control),
            |level, request| {
                if let Some(wet) = dispatch.call(request)? {
                    let current = effect;
                    effect += 1;
                    let route = &mut routes.numeric()?[id];
                    let wet = match &mut numerics {
                        WetNumerics::Block(transfers) => transfers
                            .get(current)
                            .ok_or(PlanError::InvalidPlan("wet gain checkpoint absent"))?
                            .apply_wide(wet, control)?,
                        WetNumerics::Sample(_) => route.send_sample(
                            &authored_gain::GainSum::default(),
                            wet,
                            level,
                            control,
                        )?,
                        WetNumerics::Cold => unreachable!("demand dispatch never returns PCM"),
                    };
                    for (out, input) in route.pcm[at].value.iter_mut().zip(wet) {
                        *out += input;
                    }
                    if route.pcm[at].value.iter().any(|value| !value.is_finite()) {
                        return Err(PreparationError::InvalidSamples.into());
                    }
                }
                Ok(())
            },
        )?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "member_phases_tests.rs"]
mod tests;
