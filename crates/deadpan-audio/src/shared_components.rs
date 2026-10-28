//! Contributions retained through shared stages when their later clocks differ.
//! A shared saturator supplies one channel gain, tanh(drive * sum) / sum, to
//! every input. Its continuous limit at zero is the drive. EQ is linear and
//! reads each contribution through the complete, jointly driven input prefix.

use super::shared_mix::{Contribution, MixedBlock, OwnerKey, Permissions, Temporal};
use super::*;
use deadpan_core::AudioTreatmentStage;
use deadpan_plan::AudioOwnerClock;

#[path = "shared_components/program.rs"]
mod program;
use program::Program;

#[path = "shared_components/block.rs"]
mod block;

#[path = "shared_components/policy.rs"]
mod policy;

#[path = "shared_components/permissions.rs"]
mod permissions;
#[path = "shared_components/visitors.rs"]
mod visitors;

#[path = "shared_components/member_phases.rs"]
mod member_phases;

#[path = "shared_components/completion.rs"]
mod completion;
pub(super) use completion::CompletedPolicy;

#[path = "shared_components/outputs.rs"]
mod outputs;
pub(super) use outputs::{Output, OutputSamples, OutputSelection};

/// Inspect every requested selector. A single-output call keeps its previous
/// selection work; additional comparisons are admitted before inspecting them.
fn requested(
    outputs: &[Output<'_>],
    voice: usize,
    control: WorkControl<'_>,
) -> Result<bool, StageAudioError> {
    if outputs.len() > 1 {
        control.spend_plan_work(outputs.len() - 1)?;
    }
    Ok(outputs.iter().fold(false, |selected, output| {
        selected | output.selection.selects(voice)
    }))
}

#[cfg(test)]
#[path = "shared_components/prediction_tests.rs"]
mod prediction_tests;

#[derive(Clone, Copy, Default)]
struct Sample {
    value: [f64; 2],
    send_input: [f64; 2],
}

struct Route<'plan> {
    voice: usize,
    original: bool,
    sends: deadpan_core::TailSends,
    layers: Vec<(AudioOwnerClock<'plan>, usize)>,
    program: Option<Program>,
    samples: Range<usize>,
    pcm: Vec<Sample>,
    event_gain: Option<i32>,
}

impl Route<'_> {
    fn apply_gain(
        &mut self,
        gain: &mut authored_gain::GainSum,
        value: [f64; 2],
        control: WorkControl<'_>,
    ) -> Result<[f64; 2], StageAudioError> {
        if let Some(program) = &mut self.program {
            program.apply(value, control)
        } else {
            std::mem::take(gain).apply_wide(value)
        }
    }

    fn send_sample(
        &mut self,
        gain: &authored_gain::GainSum,
        value: [f64; 2],
        level: deadpan_core::GainDb,
        control: WorkControl<'_>,
    ) -> Result<[f64; 2], StageAudioError> {
        if let Some(program) = &mut self.program {
            program.apply(value, control)
        } else {
            gain.send_sample(value, level)
        }
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
    permitted: permissions::SamplePermissions,
    predicted: Option<permissions::SamplePermissions>,
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
            permitted: permissions::SamplePermissions::new(frames as usize),
            predicted: None,
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
            if let Some(recipe) = clock.treatments() {
                recipe.order().get(..until).ok_or(PlanError::InvalidPlan(
                    "contribution treatment slice out of range",
                ))?;
            }
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
            program: None,
            samples,
            pcm: Vec::new(),
            event_gain: None,
        });
        Ok(id)
    }

    pub(super) fn resident_frames(&self) -> Result<u64, StageAudioError> {
        // Reserve the complete future programs before PCM allocation. Their
        // actual capacities may be smaller for inactive or unselected routes.
        let prepared_gain_bytes = self.routes.iter().try_fold(0usize, |total, route| {
            total
                .checked_add(Program::resident_bytes(route, &self.owners, self.send)?)
                .ok_or(StageAudioError::Limit("contribution gain residency"))
        })?;
        let policy_bytes = self.permitted.resident_bytes(self.routes.len())?;
        let prediction_header = size_of::<Option<permissions::SamplePermissions>>();
        let prediction_bytes = if let Some(predicted) = &self.predicted {
            predicted
                .resident_bytes(self.routes.len())?
                .checked_sub(size_of::<permissions::SamplePermissions>())
                .and_then(|payload| payload.checked_add(prediction_header))
                .ok_or(StageAudioError::Range)?
        } else {
            prediction_header
        };
        let per_route = self
            .frames
            .checked_mul(std::mem::size_of::<Sample>())
            .and_then(|n| n.checked_add(authored_gain::GainSum::maximum_bytes()))
            .ok_or(StageAudioError::Limit("contribution mix residency"))?;
        let bytes = self
            .routes
            .len()
            .checked_mul(per_route)
            .and_then(|n| n.checked_add(prepared_gain_bytes))
            .and_then(|n| n.checked_add(policy_bytes))
            .and_then(|n| n.checked_add(prediction_bytes))
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

    /// Admit resident_frames before allocating policy or PCM. Policy can be
    /// populated independently first; allocating PCM preserves those marks.
    pub(super) fn allocate_policy(&mut self) {
        self.permitted.allocate(self.routes.len());
        if let Some(predicted) = &mut self.predicted {
            predicted.allocate(self.routes.len());
        }
    }

    /// Configure empty prediction metadata before admitting either payload.
    /// Actual add() observations always stay in the independent carrier.
    pub(super) fn begin_prediction(&mut self) -> Result<(), StageAudioError> {
        if self.predicted.is_some() || self.permitted.payload_frames()? != 0 {
            return Err(
                PlanError::InvalidPlan("contribution prediction must precede allocation").into(),
            );
        }
        self.predicted = Some(permissions::SamplePermissions::new(self.frames));
        Ok(())
    }

    pub(super) fn complete_policy(
        &self,
        complete: shared_eq::PopulationComplete,
        control: WorkControl<'_>,
    ) -> Result<CompletedPolicy<'_, 'plan>, StageAudioError> {
        completion::seal(self, complete, control)
    }

    pub(super) fn predict_range(
        &mut self,
        route: usize,
        range: Range<usize>,
        allowed: bool,
        control: WorkControl<'_>,
    ) -> Result<(), StageAudioError> {
        let support = &self
            .routes
            .get(route)
            .ok_or(StageAudioError::Range)?
            .samples;
        if range.start < support.start || range.end > support.end {
            return Err(PlanError::InvalidPlan("prediction outside contribution route").into());
        }
        self.predicted
            .as_mut()
            .ok_or(PlanError::InvalidPlan(
                "contribution prediction not started",
            ))?
            .assign_range(route, range, allowed, control)
    }

    /// Verify the complete untouched prediction against independently observed
    /// positive AND negative bits, including samples omitted by producer loops.
    /// Return only the dropped payload's actual residency for caller cleanup.
    pub(super) fn verify_prediction(
        &mut self,
        control: WorkControl<'_>,
    ) -> Result<u64, StageAudioError> {
        let Some(predicted) = &self.predicted else {
            return Ok(0);
        };
        predicted.validate(self.routes.len())?;
        self.permitted.validate(self.routes.len())?;
        predicted.verify_matches(&self.permitted, control)?;
        let released = predicted.payload_frames()?;
        self.predicted = None;
        Ok(released)
    }

    pub(super) fn allocate(&mut self) {
        self.allocate_policy();
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
        let id = route;
        let route = &mut self.routes[id];
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
        self.permitted.mark(id, at)?;
        Ok(())
    }

    pub(super) fn finish(
        self,
        output_voice: Option<usize>,
        output_aliases: Permissions,
        control: WorkControl<'_>,
        equalize: impl FnMut(
            &AudioOwnerClock<'plan>,
            Temporal,
            AudioSample,
            Permissions,
            Option<usize>,
        ) -> Result<[f64; 2], StageAudioError>,
    ) -> Result<MixedBlock, StageAudioError> {
        let mut samples = vec![[0.0; 2]; self.frames];
        let mut suppressed = Vec::new();
        self.finish_into(
            &mut [Output {
                selection: OutputSelection {
                    voice: output_voice,
                    aliases: output_aliases,
                },
                samples: OutputSamples::Interleaved(&mut samples),
                suppressed: Some(&mut suppressed),
            }],
            control,
            equalize,
        )?;
        Ok(MixedBlock {
            samples,
            suppressed: merged_suppression(suppressed),
        })
    }

    /// Execute the common contribution driver once, then emit each requested
    /// target in the original route order. Buffers are borrowed, never cached.
    /// The caller must admit its output buffers and selector headers as well
    /// as `resident_frames()`. Shape validation is not recursive allocation
    /// admission; the normal reader still supplies its one existing output.
    /// A failed execution leaves scratch output unspecified; callers must
    /// discard it and propagate the error, without rerunning or refunding work.
    pub(super) fn finish_into(
        mut self,
        outputs: &mut [Output<'_>],
        control: WorkControl<'_>,
        mut equalize: impl FnMut(
            &AudioOwnerClock<'plan>,
            Temporal,
            AudioSample,
            Permissions,
            Option<usize>,
        ) -> Result<[f64; 2], StageAudioError>,
    ) -> Result<(), StageAudioError> {
        if self.predicted.is_some() {
            return Err(PlanError::InvalidPlan("contribution prediction not verified").into());
        }
        if outputs.is_empty() || outputs.len() > super::shared_mix::MAX_VOICES {
            return Err(StageAudioError::Limit("contribution output count"));
        }
        self.start
            .0
            .checked_add(i64::try_from(self.frames).map_err(|_| StageAudioError::Range)?)
            .ok_or(TimeError::Overflow)?;
        if outputs.len() > 1 {
            control.spend_plan_work(outputs.len())?;
        }
        for output in outputs.iter() {
            output.validate(self.frames, outputs.len() > 1)?;
        }
        for route in &mut self.routes {
            let selected = requested(outputs, route.voice, control)?;
            route.program = Program::prepare(route, &self.owners, self.send, selected, control)?;
        }
        if self
            .routes
            .iter()
            .all(|route| route.event_gain.is_none() || route.program.is_some())
        {
            return self.finish_constant_into(outputs, control, equalize);
        }
        let owners = policy::ordered_owners(&self.owners);
        let mut gains = vec![authored_gain::GainSum::default(); self.routes.len()];
        for at in 0..self.frames {
            control.check()?;
            let sample = AudioSample(self.start.0 + at as i64);
            let mut sent = [0.0; 2];
            for (id, route) in self.routes.iter_mut().enumerate() {
                if let Some(program) = &mut route.program {
                    program.restart();
                } else {
                    gains[id] = authored_gain::beat_sound_gain(
                        &[],
                        sample,
                        route.event_gain.unwrap_or(0),
                        control,
                        false,
                    )?;
                }
                if self.permitted.contains(id, at)
                    && let Some(level) = self.send.and_then(|effect| route.sends.level(effect))
                {
                    control.spend_plan_work(1)?;
                    let value =
                        route.send_sample(&gains[id], route.pcm[at].send_input, level, control)?;
                    for ch in 0..2 {
                        sent[ch] += value[ch];
                    }
                }
            }
            for owner in &owners {
                member_phases::run_owner(
                    &mut self.routes,
                    member_phases::OwnerPhase {
                        permitted: &self.permitted,
                        owner,
                        start: self.start,
                        frames: self.frames,
                        send: self.send,
                    },
                    member_phases::Numerics::Sample {
                        at,
                        gains: &mut gains,
                        sent: &mut sent,
                    },
                    control,
                    &mut equalize,
                )?;
            }
            if outputs.len() == 1 {
                // Preserve the single-output path's exact final operation and
                // error order. It also provides the separate scalar oracle.
                let output = &mut outputs[0];
                let mut sum = [0.0; 2];
                let mut permitted = false;
                for (id, route) in self.routes.iter_mut().enumerate() {
                    if self.permitted.contains(id, at) && output.selection.selects(route.voice) {
                        permitted = true;
                        let value =
                            route.apply_gain(&mut gains[id], route.pcm[at].value, control)?;
                        for channel in 0..2 {
                            sum[channel] += value[channel];
                        }
                    }
                }
                if sum.iter().any(|value| !value.is_finite()) {
                    return Err(PreparationError::InvalidSamples.into());
                }
                if self.send.is_some() && sent.iter().any(|value| !value.is_finite()) {
                    return Err(PreparationError::InvalidSamples.into());
                }
                output
                    .samples
                    .set(at, if self.send.is_some() { sent } else { sum });
                if !permitted {
                    output.mark_suppressed(sample);
                }
            } else {
                // Final gains belong only to requested routes, but a route
                // selected by several targets consumes its final transfer once.
                for (id, route) in self.routes.iter_mut().enumerate() {
                    if self.permitted.contains(id, at) && requested(outputs, route.voice, control)?
                    {
                        route.pcm[at].value =
                            route.apply_gain(&mut gains[id], route.pcm[at].value, control)?;
                    }
                }
                for (index, output) in outputs.iter_mut().enumerate() {
                    if index != 0 {
                        control.spend_plan_work(self.routes.len())?;
                    }
                    let mut sum = [0.0; 2];
                    let mut permitted = false;
                    for (id, route) in self.routes.iter().enumerate() {
                        if self.permitted.contains(id, at) && output.selection.selects(route.voice)
                        {
                            permitted = true;
                            for (out, value) in sum.iter_mut().zip(route.pcm[at].value) {
                                *out += value;
                            }
                        }
                    }
                    if sum.iter().any(|value| !value.is_finite())
                        || (self.send.is_some() && sent.iter().any(|value| !value.is_finite()))
                    {
                        return Err(PreparationError::InvalidSamples.into());
                    }
                    output
                        .samples
                        .set(at, if self.send.is_some() { sent } else { sum });
                    if !permitted {
                        output.mark_suppressed(sample);
                    }
                }
            }
        }
        Ok(())
    }
}
