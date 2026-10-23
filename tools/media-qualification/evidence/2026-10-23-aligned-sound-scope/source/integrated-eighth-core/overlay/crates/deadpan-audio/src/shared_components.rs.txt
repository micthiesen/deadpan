//! Contributions retained through shared stages when their later clocks differ.
//! A shared saturator supplies one channel gain, tanh(drive * sum) / sum, to
//! every input. Its continuous limit at zero is the drive. EQ is linear and
//! reads each contribution through the complete, jointly driven input prefix.

use super::shared_mix::{Contribution, MixedBlock, OwnerKey, Permissions, Temporal};
use super::*;
use deadpan_core::AudioTreatmentStage;
use deadpan_plan::AudioOwnerClock;

#[derive(Clone, Copy, Default)]
struct Sample {
    value: [f64; 2],
    send_input: [f64; 2],
    permitted: bool,
}

struct Route<'plan> {
    voice: usize,
    original: bool,
    sends: deadpan_core::TailSends,
    layers: Vec<(AudioOwnerClock<'plan>, usize)>,
    samples: Range<usize>,
    pcm: Vec<Sample>,
    event_gain: Option<i32>,
}

impl Route<'_> {
    fn wet_allowed(&self, layer: usize) -> bool {
        !self.original
            || !self.layers[layer + 1..]
                .iter()
                .any(|(clock, _)| clock.is_explicit_tail_provider())
    }
}

struct Owner<'plan> {
    parent: Option<OwnerKey>,
    depth: usize,
    members: Vec<(usize, usize)>,
    clock: AudioOwnerClock<'plan>,
    until: usize,
    skipped: bool,
    wet: bool,
}

pub(super) struct Components<'plan> {
    start: AudioSample,
    frames: usize,
    routes: Vec<Route<'plan>>,
    owners: BTreeMap<OwnerKey, Owner<'plan>>,
    send: Option<deadpan_core::TailEffect>,
    skip: Vec<authored_gain::OwnerIdentity>,
}

impl<'plan> Components<'plan> {
    pub(super) fn new(
        start: AudioSample,
        frames: u32,
        send: Option<deadpan_core::TailEffect>,
        skip: &[authored_gain::OwnerIdentity],
    ) -> Self {
        Self {
            start,
            frames: frames as usize,
            routes: Vec::new(),
            owners: BTreeMap::new(),
            send,
            skip: skip.to_vec(),
        }
    }

    pub(super) fn route(
        &mut self,
        voice: (usize, Contribution),
        layers: Vec<(AudioOwnerClock<'plan>, usize)>,
        samples: Range<usize>,
        stop: Option<&OwnerKey>,
        control: WorkControl<'_>,
    ) -> Result<usize, StageAudioError> {
        let id = self.routes.len();
        let mut parent = None;
        for (depth, (clock, until)) in layers.iter().enumerate() {
            control.spend_plan_work(1)?;
            let skipped = self.skip.contains(&authored_gain::identity(clock));
            let until = if skipped { 0 } else { *until };
            let key = OwnerKey::new(clock);
            let wet = !skipped
                && stop != Some(&key)
                && until == clock.treatments().map_or(0, |r| r.order().len());
            if let Some(owner) = self.owners.get_mut(&key) {
                if owner.parent != parent || owner.depth != depth {
                    return Err(PlanError::InvalidPlan("inconsistent contribution ancestry").into());
                }
                let recipe = clock.treatments();
                let old = owner.clock.treatments();
                // Only gain clocks may differ. Common stages need one recipe
                // and stage position, even when no delivery ranges overlap.
                if owner.until != until
                    || owner.skipped != skipped
                    || owner.wet != wet
                    || old.map(|r| r.order()) != recipe.map(|r| r.order())
                    || old.and_then(|r| r.saturation()) != recipe.and_then(|r| r.saturation())
                    || old.and_then(|r| r.equalizer()) != recipe.and_then(|r| r.equalizer())
                {
                    return Err(PlanError::InvalidPlan("inconsistent contribution stages").into());
                }
                owner.members.push((id, depth));
            } else {
                self.owners.insert(
                    key.clone(),
                    Owner {
                        parent,
                        depth,
                        members: vec![(id, depth)],
                        clock: clock.clone(),
                        until,
                        skipped,
                        wet,
                    },
                );
            }
            parent = Some(key);
        }
        self.routes.push(Route {
            voice: voice.0,
            original: matches!(voice.1, Contribution::Original),
            sends: match voice.1 {
                Contribution::Original => Default::default(),
                Contribution::Sound(sends) => sends,
            },
            layers,
            samples,
            pcm: Vec::new(),
            event_gain: None,
        });
        Ok(id)
    }

    pub(super) fn resident_frames(&self) -> Result<u64, StageAudioError> {
        let per_route = self
            .frames
            .checked_mul(std::mem::size_of::<Sample>())
            .and_then(|n| n.checked_add(authored_gain::GainSum::maximum_bytes()))
            .ok_or(StageAudioError::Limit("contribution mix residency"))?;
        let bytes = self
            .routes
            .len()
            .checked_mul(per_route)
            .and_then(|n| n.checked_add(self.frames * std::mem::size_of::<[f64; 2]>()))
            .and_then(|n| {
                n.checked_add(if self.send.is_some() {
                    authored_gain::GainSum::maximum_bytes()
                } else {
                    0
                })
            })
            .ok_or(StageAudioError::Limit("contribution mix residency"))?;
        Ok((bytes as u64).div_ceil(std::mem::size_of::<[f32; 2]>() as u64))
    }

    pub(super) fn allocate(&mut self) {
        for route in &mut self.routes {
            route.pcm = vec![Sample::default(); self.frames];
        }
    }

    pub(super) fn add(
        &mut self,
        route: usize,
        at: usize,
        (dry, wet): ([f64; 2], [f64; 2]),
        event_gain: i32,
        control: WorkControl<'_>,
    ) -> Result<(), StageAudioError> {
        let route = &mut self.routes[route];
        if !route.samples.contains(&at) {
            return Err(PlanError::InvalidPlan("contribution outside its route").into());
        }
        control.check()?;
        if route.event_gain.is_some_and(|gain| gain != event_gain) {
            return Err(PlanError::InvalidPlan("inconsistent contribution event gain").into());
        }
        route.event_gain = Some(event_gain);
        let output = &mut route.pcm[at];
        for ch in 0..2 {
            output.value[ch] += dry[ch] + wet[ch];
            output.send_input[ch] += dry[ch];
            if !output.value[ch].is_finite() || !output.send_input[ch].is_finite() {
                return Err(PreparationError::InvalidSamples.into());
            }
        }
        output.permitted = true;
        Ok(())
    }

    pub(super) fn finish(
        mut self,
        output_voice: Option<usize>,
        output_aliases: Permissions,
        control: WorkControl<'_>,
        mut equalize: impl FnMut(
            &AudioOwnerClock<'plan>,
            Temporal,
            AudioSample,
            Permissions,
            Option<usize>,
        ) -> Result<[f64; 2], StageAudioError>,
    ) -> Result<MixedBlock, StageAudioError> {
        let mut owners: Vec<_> = self.owners.values().collect();
        owners.sort_by_key(|owner| std::cmp::Reverse(owner.depth));
        let mut gains = vec![authored_gain::GainSum::default(); self.routes.len()];
        let mut output = vec![[0.0; 2]; self.frames];
        let mut suppressed = Vec::new();
        for (at, result) in output.iter_mut().enumerate() {
            control.check()?;
            let sample = AudioSample(self.start.0 + at as i64);
            let mut sent = [0.0; 2];
            for (id, route) in self.routes.iter().enumerate() {
                gains[id] = authored_gain::beat_sound_gain(
                    &[],
                    sample,
                    route.event_gain.unwrap_or(0),
                    control,
                    false,
                )?;
                if route.pcm[at].permitted
                    && let Some(level) = self.send.and_then(|effect| route.sends.level(effect))
                {
                    control.spend_plan_work(1)?;
                    let value = gains[id].send_sample(route.pcm[at].send_input, level)?;
                    for ch in 0..2 {
                        sent[ch] += value[ch];
                    }
                }
            }
            for owner in &owners {
                let active = |id: usize| self.routes[id].pcm[at].permitted;
                let mut permissions = Permissions::default();
                for &(id, _) in &owner.members {
                    if active(id) {
                        permissions.insert(self.routes[id].voice)?;
                    }
                }
                if permissions.is_empty() {
                    continue;
                }
                for &(id, layer) in &owner.members {
                    if active(id) && !owner.skipped {
                        gains[id].owner_slice(
                            &self.routes[id].layers[layer].0,
                            sample,
                            0..0,
                            true,
                            control,
                        )?;
                    }
                }
                let recipe = owner.clock.treatments();
                for stage in 0..owner.until {
                    let kind = recipe
                        .and_then(|r| r.order().get(stage))
                        .copied()
                        .ok_or(PlanError::InvalidPlan("contribution stage absent"))?;
                    let eq = kind == AudioTreatmentStage::Equalizer
                        && recipe
                            .and_then(|r| r.equalizer())
                            .is_some_and(|r| !r.is_unity());
                    if kind == AudioTreatmentStage::Saturation || eq {
                        let mut sum = [0.0; 2];
                        let mut count = 0;
                        for &(id, _) in &owner.members {
                            if self.routes[id].pcm[at].permitted {
                                count += 1;
                                let frame = &mut self.routes[id].pcm[at].value;
                                *frame = std::mem::take(&mut gains[id]).apply_wide(*frame)?;
                                for channel in 0..2 {
                                    sum[channel] += frame[channel];
                                }
                            }
                        }
                        if sum.iter().any(|value| !value.is_finite()) {
                            return Err(PreparationError::InvalidSamples.into());
                        }
                        let saturation = recipe.and_then(|r| r.saturation());
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
                        // All selected outputs read one shared input context.
                        // Original routes are registered first; use its active
                        // clock when present, then an admitted sound's clock.
                        // Per-contribution gain clocks remain independent.
                        let eq_clock = if eq {
                            owner.members.iter().find_map(|&(id, layer)| {
                                self.routes[id].pcm[at]
                                    .permitted
                                    .then(|| self.routes[id].layers[layer].0.clone())
                            })
                        } else {
                            None
                        };
                        for &(id, _) in &owner.members {
                            let route = &mut self.routes[id];
                            let frame = &mut route.pcm[at];
                            if frame.permitted {
                                frame.value = if eq {
                                    equalize(
                                        eq_clock.as_ref().ok_or(PlanError::InvalidPlan(
                                            "shared EQ has no admitted clock",
                                        ))?,
                                        Temporal::Equalizer(stage),
                                        sample,
                                        permissions,
                                        Some(route.voice),
                                    )?
                                } else if count == 1 {
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
                    } else {
                        for &(id, layer) in &owner.members {
                            if self.routes[id].pcm[at].permitted {
                                gains[id].owner_slice(
                                    &self.routes[id].layers[layer].0,
                                    sample,
                                    stage..stage + 1,
                                    false,
                                    control,
                                )?;
                            }
                        }
                    }
                }
                if let Some(effect) = self.send {
                    for &(id, layer) in &owner.members {
                        let route = &self.routes[id];
                        if route.pcm[at].permitted
                            && let Some(level) = route.layers[layer]
                                .0
                                .treatments()
                                .and_then(|recipe| recipe.tail_sends().level(effect))
                        {
                            control.spend_plan_work(1)?;
                            let value = gains[id].send_sample(route.pcm[at].value, level)?;
                            for ch in 0..2 {
                                sent[ch] += value[ch];
                            }
                        }
                    }
                }
                if owner.wet {
                    // The owner's return joins each contribution before its
                    // parent sees the shared sum. Its temporal prefix stops
                    // after this owner's dry chain, so it cannot feed itself.
                    // A selected Tail provider blocks other incoming Original
                    // returns. Keep allowed sounds independent, including the
                    // voices driving nonlinear stages in their wet history.
                    let mut wet_permissions = Permissions::default();
                    let mut clock = None;
                    for &(id, layer) in &owner.members {
                        let route = &self.routes[id];
                        if route.pcm[at].permitted && route.wet_allowed(layer) {
                            wet_permissions.insert(route.voice)?;
                            clock.get_or_insert_with(|| route.layers[layer].0.clone());
                        }
                    }
                    for &(id, layer) in &owner.members {
                        let route = &mut self.routes[id];
                        if !route.pcm[at].permitted || !route.wet_allowed(layer) {
                            continue;
                        }
                        let sends = route.layers[layer]
                            .0
                            .treatments()
                            .map(|r| r.wet_sends())
                            .unwrap_or_default();
                        if sends.is_empty() {
                            continue;
                        }
                        route.pcm[at].value =
                            std::mem::take(&mut gains[id]).apply_wide(route.pcm[at].value)?;
                        for effect in [
                            deadpan_core::TailEffect::Reverb,
                            deadpan_core::TailEffect::Delay,
                        ] {
                            if let Some(level) = sends.level(effect) {
                                let wet = equalize(
                                    clock.as_ref().ok_or(PlanError::InvalidPlan(
                                        "wet return has no admitted clock",
                                    ))?,
                                    Temporal::Wet(effect),
                                    sample,
                                    wet_permissions,
                                    Some(route.voice),
                                )?;
                                let wet =
                                    authored_gain::GainSum::default().send_sample(wet, level)?;
                                for (out, input) in route.pcm[at].value.iter_mut().zip(wet) {
                                    *out += input;
                                }
                                if route.pcm[at].value.iter().any(|x| !x.is_finite()) {
                                    return Err(PreparationError::InvalidSamples.into());
                                }
                            }
                        }
                    }
                }
            }
            let mut permitted = false;
            for (id, route) in self.routes.iter().enumerate() {
                let gain = std::mem::take(&mut gains[id]);
                if route.pcm[at].permitted
                    && (output_voice.is_none_or(|voice| voice == route.voice)
                        || output_aliases.contains(route.voice))
                {
                    permitted = true;
                    let value = gain.apply_wide(route.pcm[at].value)?;
                    for channel in 0..2 {
                        result[channel] += value[channel];
                    }
                }
            }
            if result.iter().any(|value| !value.is_finite()) {
                return Err(PreparationError::InvalidSamples.into());
            }
            if self.send.is_some() {
                if sent.iter().any(|value| !value.is_finite()) {
                    return Err(PreparationError::InvalidSamples.into());
                }
                *result = sent;
            }
            if !permitted {
                suppressed.push(sample..AudioSample(sample.0 + 1));
            }
        }
        Ok(MixedBlock {
            samples: output,
            suppressed: merged_suppression(suppressed),
        })
    }
}
