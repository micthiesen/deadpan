//! Ordered shared owner buses with explicit per-contribution permissions.
//! A temporal stage re-evaluates its complete prefix using the voices permitted
//! at the consuming sample. Nonlinear stages still process one shared sum.
//! Linear input prefixes retain each contribution's own automation clock.

use deadpan_core::AudioTreatmentStage;
use deadpan_plan::AudioOwnerClock;

use super::*;

pub(super) const MAX_VOICES: usize = 4096;
const WORDS: usize = MAX_VOICES / 64;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Permissions([u64; WORDS]);

impl Default for Permissions {
    fn default() -> Self {
        Self([0; WORDS])
    }
}

impl Permissions {
    pub(super) fn contains(&self, voice: usize) -> bool {
        voice < MAX_VOICES && self.0[voice / 64] & (1 << (voice % 64)) != 0
    }
    pub(super) fn insert(&mut self, voice: usize) -> Result<(), StageAudioError> {
        if voice >= MAX_VOICES {
            return Err(StageAudioError::Limit("shared EQ voice count"));
        }
        self.0[voice / 64] |= 1 << (voice % 64);
        Ok(())
    }
    fn union(&mut self, other: &Self) {
        for (target, value) in self.0.iter_mut().zip(other.0) {
            *target |= value;
        }
    }
    pub(super) fn is_empty(&self) -> bool {
        self.0.iter().all(|word| *word == 0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct OwnerKey {
    instance: InstancePath,
    gap: bool,
    after: Option<IterationId>,
}

impl OwnerKey {
    pub(super) fn new(owner: &AudioOwnerClock<'_>) -> Self {
        Self {
            instance: owner.instance().clone(),
            gap: owner.kind() == deadpan_plan::AudioOwnerKind::DefaultGap,
            after: owner.gap_after().cloned(),
        }
    }
}

#[derive(Clone)]
pub(super) struct Boundary {
    pub(super) owner: OwnerKey,
    pub(super) before: usize,
}

#[derive(Clone)]
struct Layer<'plan> {
    owner: AudioOwnerClock<'plan>,
    from: usize,
    until: usize,
    escalation: bool,
}

pub(super) struct Route<'plan> {
    target: Option<usize>,
    layers: Vec<Layer<'plan>>,
    ignored: bool,
    component: Option<usize>,
}

struct Run<'plan> {
    samples: Range<usize>,
    layers: Vec<Layer<'plan>>,
}

struct Bus<'plan> {
    parent: Option<usize>,
    depth: usize,
    runs: Vec<Run<'plan>>,
    samples: Vec<[f64; 2]>,
    permissions: Vec<Permissions>,
}

pub(super) struct SharedMix<'plan> {
    pub(super) start: AudioSample,
    frames: usize,
    stop: Option<Boundary>,
    by_owner: BTreeMap<OwnerKey, usize>,
    buses: Vec<Bus<'plan>>,
    sum: Vec<[f64; 2]>,
    permissions: Vec<Permissions>,
    components: shared_components::Components<'plan>,
    separate: bool,
    output_voice: Option<usize>,
}

pub(super) struct MixedBlock {
    pub(super) samples: Vec<[f64; 2]>,
    pub(super) suppressed: Vec<Range<AudioSample>>,
}

fn equalizer(layers: &[Layer<'_>]) -> Option<usize> {
    let last = layers.last()?;
    if last.owner.treatments()?.equalizer()?.is_unity() {
        return None;
    }
    last.owner.treatments()?.order()[last.from..last.until]
        .iter()
        .position(|stage| *stage == AudioTreatmentStage::Equalizer)
        .map(|stage| last.from + stage)
}

/// Build the delivered memoryless suffix. A temporal boundary evaluates its
/// complete prefix recursively; mute and automation cannot cross that filter.
fn transfer(
    layers: &[Layer<'_>],
    at: AudioSample,
    control: WorkControl<'_>,
) -> Result<authored_gain::GainSum, StageAudioError> {
    let eq = equalizer(layers);
    let mut result = authored_gain::GainSum::default();
    for (index, layer) in layers.iter().enumerate().rev() {
        let last = index + 1 == layers.len();
        let (start, end) = match eq {
            Some(stage) if last => (stage + 1, layer.until),
            _ => (layer.from, layer.until),
        };
        result.owner_slice(
            &layer.owner,
            at,
            start..end,
            start == layer.from && layer.escalation,
            control,
        )?;
    }
    Ok(result)
}

impl<'plan> SharedMix<'plan> {
    pub(super) fn new(
        start: AudioSample,
        frames: u32,
        stop: Option<Boundary>,
        output_voice: Option<usize>,
        separate: bool,
    ) -> Self {
        Self {
            start,
            frames: frames as usize,
            stop,
            by_owner: BTreeMap::new(),
            buses: Vec::new(),
            sum: Vec::new(),
            permissions: Vec::new(),
            components: shared_components::Components::new(start, frames),
            separate: separate || output_voice.is_some(),
            output_voice,
        }
    }

    pub(super) fn route(
        &mut self,
        owners: &[AudioOwnerClock<'plan>],
        range: Range<AudioSample>,
        voice: usize,
        control: WorkControl<'_>,
    ) -> Result<Route<'plan>, StageAudioError> {
        deadpan_core::validate_audio_treatment_layers(
            owners.iter().filter_map(|owner| owner.treatments()),
        )?;
        let first = if let Some(stop) = &self.stop {
            let Some(index) = owners
                .iter()
                .position(|owner| OwnerKey::new(owner) == stop.owner)
            else {
                return Ok(Route {
                    target: None,
                    layers: Vec::new(),
                    ignored: true,
                    component: None,
                });
            };
            index
        } else {
            0
        };
        let start =
            usize::try_from(range.start.0 - self.start.0).map_err(|_| StageAudioError::Range)?;
        let end =
            usize::try_from(range.end.0 - self.start.0).map_err(|_| StageAudioError::Range)?;
        if start >= end || end > self.frames {
            return Err(StageAudioError::Range);
        }
        let mut layers = Vec::new();
        let mut parent = None;
        let mut component_layers = Vec::new();
        for (index, owner) in owners.iter().enumerate().skip(first) {
            control.spend_plan_work(1)?;
            let until = if index == first {
                self.stop.as_ref().map(|stop| stop.before)
            } else {
                None
            }
            .unwrap_or_else(|| owner.treatments().map_or(0, |recipe| recipe.order().len()));
            component_layers.push((owner.clone(), until));
            let complex = if let Some(recipe) = owner.treatments() {
                recipe
                    .order()
                    .get(..until)
                    .ok_or(PlanError::InvalidPlan(
                        "shared mix boundary exceeds its owner",
                    ))?
                    .iter()
                    .position(|stage| {
                        *stage == AudioTreatmentStage::Saturation
                            || (*stage == AudioTreatmentStage::Equalizer
                                && recipe.equalizer().is_some_and(|eq| !eq.is_unity()))
                    })
            } else {
                None
            };
            layers.push(Layer {
                owner: owner.clone(),
                from: complex.unwrap_or(0),
                until,
                escalation: complex.is_none(),
            });
            let Some(first_stage) = complex else {
                continue;
            };
            let key = OwnerKey::new(owner);
            let bus = if let Some(&id) = self.by_owner.get(&key) {
                if self.buses[id].parent != parent {
                    return Err(PlanError::InvalidPlan("inconsistent shared mix ancestry").into());
                }
                id
            } else {
                let id = self.buses.len();
                self.by_owner.insert(key, id);
                self.buses.push(Bus {
                    parent,
                    depth: parent.map_or(0, |parent| self.buses[parent].depth + 1),
                    runs: Vec::new(),
                    samples: Vec::new(),
                    permissions: Vec::new(),
                });
                id
            };
            self.register(bus, start..end, &layers, control)?;
            parent = Some(bus);
            layers.clear();
            layers.push(Layer {
                owner: owner.clone(),
                from: 0,
                until: first_stage,
                escalation: true,
            });
        }
        Ok(Route {
            target: parent,
            layers,
            ignored: false,
            component: Some(
                self.components
                    .route(voice, component_layers, start..end, control)?,
            ),
        })
    }

    fn register(
        &mut self,
        bus: usize,
        range: Range<usize>,
        layers: &[Layer<'plan>],
        control: WorkControl<'_>,
    ) -> Result<(), StageAudioError> {
        let mut cursor = range.start;
        let mut uncovered = Vec::new();
        for run in &self.buses[bus].runs {
            control.spend_plan_work(1)?;
            let overlap = range.start.max(run.samples.start)..range.end.min(run.samples.end);
            if overlap.is_empty() {
                continue;
            }
            if run.layers.len() != layers.len() {
                return Err(PlanError::InvalidPlan("inconsistent shared mix owners").into());
            }
            let mut same = true;
            for (a, b) in run.layers.iter().zip(layers) {
                control.spend_plan_work(1)?;
                if a.from != b.from
                    || a.until != b.until
                    || a.escalation != b.escalation
                    || OwnerKey::new(&a.owner) != OwnerKey::new(&b.owner)
                {
                    return Err(PlanError::InvalidPlan("inconsistent shared mix programs").into());
                }
                if a.owner.treatments().and_then(|r| r.equalizer())
                    != b.owner.treatments().and_then(|r| r.equalizer())
                {
                    return Err(StageAudioError::Unsupported(
                        "different EQ recipes on independently timed shared inputs",
                    ));
                }
                let first = AudioSample(self.start.0 + overlap.start as i64);
                let last = AudioSample(self.start.0 + overlap.end as i64 - 1);
                same &= a.owner.escalation_millidecibels() == b.owner.escalation_millidecibels()
                    && a.owner.sampling().local_frames_per_sample()
                        == b.owner.sampling().local_frames_per_sample()
                    && a.owner.sampling().local_at(first)? == b.owner.sampling().local_at(first)?
                    && a.owner.sampling().local_at(last)? == b.owner.sampling().local_at(last)?;
            }
            if !same && !self.separate {
                for offset in overlap.clone() {
                    control.check()?;
                    let at = AudioSample(self.start.0 + offset as i64);
                    // A filter prepares its complete input prefix recursively
                    // using each voice's own clock. Only its delivered suffix
                    // must agree here; comparing pre-EQ gain rejects valid
                    // independently timed input automation.
                    if !transfer(&run.layers, at, control)?
                        .same_transfer(&transfer(layers, at, control)?)
                    {
                        return Err(PlanError::InvalidPlan(
                            "shared automation missing contribution plan",
                        )
                        .into());
                    }
                }
            }
            if cursor < overlap.start {
                uncovered.push(cursor..overlap.start);
            }
            cursor = cursor.max(overlap.end);
        }
        if cursor < range.end {
            uncovered.push(cursor..range.end);
        }
        self.buses[bus]
            .runs
            .extend(uncovered.into_iter().map(|samples| Run {
                samples,
                layers: layers.to_vec(),
            }));
        self.buses[bus].runs.sort_by_key(|run| run.samples.start);
        Ok(())
    }

    /// f32 stereo frame equivalents, including fixed-size permission bitsets.
    /// The caller separately reserves its raw/transient reader buffers.
    pub(super) fn resident_frames(&self) -> Result<u64, StageAudioError> {
        if self.separate {
            return self.components.resident_frames();
        }
        (self.buses.len() as u64 + 1)
            .checked_mul(self.frames as u64)
            .and_then(|n| n.checked_mul(2 + WORDS as u64))
            .ok_or(StageAudioError::Limit("shared mix residency"))
    }
    pub(super) fn allocate(&mut self) {
        if self.separate {
            self.components.allocate();
            return;
        }
        self.sum = vec![[0.0; 2]; self.frames];
        self.permissions = vec![Permissions::default(); self.frames];
        for bus in &mut self.buses {
            bus.samples = vec![[0.0; 2]; self.frames];
            bus.permissions = vec![Permissions::default(); self.frames];
        }
    }

    pub(super) fn add(
        &mut self,
        route: &Route<'_>,
        offset: usize,
        sample: [f32; 2],
        event_gain: i32,
        voice: Option<usize>,
        control: WorkControl<'_>,
    ) -> Result<(), StageAudioError> {
        if route.ignored {
            return Ok(());
        }
        let Some(voice) = voice else {
            return Ok(());
        };
        if self.separate {
            return self.components.add(
                route
                    .component
                    .ok_or(PlanError::InvalidPlan("missing contribution route"))?,
                offset,
                sample,
                event_gain,
                control,
            );
        }
        let mut gain = authored_gain::beat_sound_gain(
            &[],
            AudioSample(self.start.0 + offset as i64),
            event_gain,
            control,
            false,
        )?;
        for layer in route.layers.iter().rev() {
            gain.owner_slice(
                &layer.owner,
                AudioSample(self.start.0 + offset as i64),
                layer.from..layer.until,
                layer.escalation,
                control,
            )?;
        }
        let mut permissions = Permissions::default();
        permissions.insert(voice)?;
        self.add_value(route.target, offset, gain.apply(sample)?, permissions)
    }

    fn add_value(
        &mut self,
        bus: Option<usize>,
        at: usize,
        value: [f64; 2],
        permissions: Permissions,
    ) -> Result<(), StageAudioError> {
        let (samples, masks) = match bus {
            Some(id) => {
                let bus = &mut self.buses[id];
                (&mut bus.samples, &mut bus.permissions)
            }
            None => (&mut self.sum, &mut self.permissions),
        };
        for (out, input) in samples[at].iter_mut().zip(value) {
            *out += input;
            if !out.is_finite() {
                return Err(PreparationError::InvalidSamples.into());
            }
        }
        masks[at].union(&permissions);
        Ok(())
    }

    pub(super) fn finish(
        mut self,
        control: WorkControl<'_>,
        mut equalize: impl FnMut(
            &AudioOwnerClock<'plan>,
            usize,
            AudioSample,
            Permissions,
            Option<usize>,
        ) -> Result<[f64; 2], StageAudioError>,
    ) -> Result<MixedBlock, StageAudioError> {
        if self.separate {
            return self.components.finish(self.output_voice, control, equalize);
        }
        let mut order: Vec<_> = self.by_owner.values().copied().collect();
        order.sort_by_key(|&id| std::cmp::Reverse(self.buses[id].depth));
        for id in order {
            let samples = std::mem::take(&mut self.buses[id].samples);
            let permissions = std::mem::take(&mut self.buses[id].permissions);
            for run in std::mem::take(&mut self.buses[id].runs) {
                let eq = equalizer(&run.layers);
                for at in run.samples {
                    control.check()?;
                    if permissions[at].is_empty() {
                        continue;
                    }
                    let clock = AudioSample(self.start.0 + at as i64);
                    let input = if let Some(stage) = eq {
                        let owner = &run
                            .layers
                            .last()
                            .ok_or(PlanError::InvalidPlan("shared EQ owner absent"))?
                            .owner;
                        equalize(owner, stage, clock, permissions[at], None)?
                    } else {
                        samples[at]
                    };
                    let output = transfer(&run.layers, clock, control)?.apply_wide(input)?;
                    self.add_value(self.buses[id].parent, at, output, permissions[at])?;
                }
            }
        }
        let suppressed = self
            .permissions
            .iter()
            .enumerate()
            .filter_map(|(index, mask)| {
                let at = AudioSample(self.start.0 + index as i64);
                mask.is_empty().then_some(at..AudioSample(at.0 + 1))
            })
            .collect();
        Ok(MixedBlock {
            samples: self.sum,
            suppressed: merged_suppression(suppressed),
        })
    }
}
