//! Route-local constant checkpoints. A route's concrete clocks and stage
//! slices are immutable; no identity-based lookup or sample-value memo is used.

use super::*;

pub(super) struct Program {
    checkpoints: Vec<authored_gain::PreparedTransfer>,
    next: usize,
}

impl Program {
    /// Block execution consumes a checkpoint once for the whole admitted
    /// buffer, instead of restarting and dispatching it for every sample.
    pub(super) fn next_transfer(
        &mut self,
        control: WorkControl<'_>,
    ) -> Result<&authored_gain::PreparedTransfer, StageAudioError> {
        Ok(&self.next_transfers(1, control)?[0])
    }

    pub(super) fn next_transfers(
        &mut self,
        count: usize,
        control: WorkControl<'_>,
    ) -> Result<&[authored_gain::PreparedTransfer], StageAudioError> {
        control.spend_plan_work(count)?;
        let end = self.next.checked_add(count).ok_or(StageAudioError::Range)?;
        let transfers = self
            .checkpoints
            .get(self.next..end)
            .ok_or(PlanError::InvalidPlan(
                "contribution gain checkpoint absent",
            ))?;
        self.next = end;
        Ok(transfers)
    }

    pub(super) fn restart(&mut self) {
        self.next = 0;
    }

    pub(super) fn apply(
        &mut self,
        value: [f64; 2],
        control: WorkControl<'_>,
    ) -> Result<[f64; 2], StageAudioError> {
        let transfer = self
            .checkpoints
            .get(self.next)
            .ok_or(PlanError::InvalidPlan(
                "contribution gain checkpoint absent",
            ))?;
        self.next += 1;
        transfer.apply_wide(value, control)
    }

    pub(super) fn constant(route: &Route<'_>, owners: &BTreeMap<OwnerKey, Owner<'_>>) -> bool {
        route.layers.iter().all(|(clock, _)| {
            let owner = &owners[&OwnerKey::new(clock)];
            Self::constant_layer(clock, owner, &mut || Ok(()))
                .expect("no-op constant-program inspection cannot fail")
        })
    }

    pub(super) fn constant_inspected(
        route: &Route<'_>,
        owners: &BTreeMap<OwnerKey, Owner<'_>>,
        mut inspect: impl FnMut() -> Result<(), StageAudioError>,
    ) -> Result<bool, StageAudioError> {
        for (clock, _) in &route.layers {
            inspect()?;
            let mut found = None;
            for (key, owner) in owners {
                inspect()?;
                if key.matches(clock) {
                    found = Some(owner);
                    break;
                }
            }
            let owner =
                found.ok_or(PlanError::InvalidPlan("constant contribution owner absent"))?;
            if !Self::constant_layer(clock, owner, &mut inspect)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn constant_layer(
        clock: &AudioOwnerClock<'_>,
        owner: &Owner<'_>,
        inspect: &mut impl FnMut() -> Result<(), StageAudioError>,
    ) -> Result<bool, StageAudioError> {
        let Some(recipe) = clock.treatments() else {
            return Ok(true);
        };
        for stage in &recipe.order()[..owner.until] {
            inspect()?;
            if *stage == AudioTreatmentStage::ClipGain {
                inspect()?;
                return Ok(recipe.clip_gain().is_some_and(|gain| {
                    gain.envelopes().is_empty() && gain.mute_ranges().is_empty()
                }));
            }
        }
        Ok(true)
    }

    /// The exact number of checkpoints and an upper bound on their retained
    /// channel stages. Mute may discard stages during preparation. Reserve the
    /// final checkpoint even if this invocation selects another output voice.
    fn layout(
        route: &Route<'_>,
        owners: &BTreeMap<OwnerKey, Owner<'_>>,
        send: Option<deadpan_core::TailEffect>,
    ) -> Result<(usize, usize), StageAudioError> {
        let mut count = 0usize;
        let mut slots = 0usize;
        let mut pending = 0usize;
        let mut checkpoint = |stages: usize| -> Result<(), StageAudioError> {
            count = count.checked_add(1).ok_or(StageAudioError::Range)?;
            slots = slots.checked_add(stages).ok_or(StageAudioError::Range)?;
            Ok(())
        };
        if send.and_then(|effect| route.sends.level(effect)).is_some() {
            checkpoint(0)?;
        }
        for (layer, (clock, _)) in route.layers.iter().enumerate().rev() {
            let owner = &owners[&OwnerKey::new(clock)];
            let recipe = clock.treatments();
            if let Some(recipe) = recipe {
                for kind in &recipe.order()[..owner.until] {
                    if flushes(*kind, recipe) {
                        checkpoint(pending)?;
                        pending = 0;
                    } else if *kind == AudioTreatmentStage::StereoBalance
                        && recipe
                            .balance()
                            .is_some_and(|balance| balance != deadpan_core::StereoBalance::CENTER)
                    {
                        pending = pending.checked_add(1).ok_or(StageAudioError::Range)?;
                    }
                }
                if send
                    .and_then(|effect| recipe.tail_sends().level(effect))
                    .is_some()
                {
                    checkpoint(pending)?;
                }
                if owner.wet && route.wet_allowed(layer) && !recipe.wet_sends().is_empty() {
                    checkpoint(pending)?;
                    pending = 0;
                    for effect in [
                        deadpan_core::TailEffect::Reverb,
                        deadpan_core::TailEffect::Delay,
                    ] {
                        if recipe.wet_sends().level(effect).is_some() {
                            checkpoint(0)?;
                        }
                    }
                }
            }
        }
        checkpoint(pending)?;
        Ok((count, slots))
    }

    pub(super) fn resident_bytes(
        route: &Route<'_>,
        owners: &BTreeMap<OwnerKey, Owner<'_>>,
        send: Option<deadpan_core::TailEffect>,
    ) -> Result<usize, StageAudioError> {
        // The Option discriminant and Program/Vec headers are resident even
        // when a dynamic route does not allocate any checkpoints.
        let header = std::mem::size_of::<Option<Self>>();
        if !Self::constant(route, owners) {
            return Ok(header);
        }
        let (checkpoints, stages) = Self::layout(route, owners, send)?;
        let transfer = authored_gain::PreparedTransfer::bytes(0).ok_or(StageAudioError::Range)?;
        let stage = authored_gain::PreparedTransfer::bytes(1)
            .and_then(|bytes| bytes.checked_sub(transfer))
            .ok_or(StageAudioError::Range)?;
        checkpoints
            .checked_mul(transfer)
            .and_then(|bytes| {
                stages
                    .checked_mul(stage)
                    .and_then(|slots| bytes.checked_add(slots))
            })
            .and_then(|bytes| bytes.checked_add(header))
            .ok_or(StageAudioError::Limit("contribution gain residency"))
    }

    pub(super) fn prepare(
        route: &Route<'_>,
        owners: &BTreeMap<OwnerKey, Owner<'_>>,
        send: Option<deadpan_core::TailEffect>,
        selected: bool,
        control: WorkControl<'_>,
    ) -> Result<Option<Self>, StageAudioError> {
        // add() establishes actual admitted support. An inactive route must
        // never evaluate an escalation, gain sum or amplitude speculatively.
        if route.event_gain.is_none() {
            return Ok(None);
        }
        control.spend_plan_work(route.layers.len())?;
        if !Self::constant(route, owners) {
            return Ok(None);
        }
        let (capacity, _) = Self::layout(route, owners, send)?;
        let mut checkpoints = Vec::with_capacity(capacity);
        let mut gain = authored_gain::beat_sound_gain(
            &[],
            AudioSample(0),
            route.event_gain.unwrap_or(0),
            control,
            false,
        )?;
        if let Some(level) = send.and_then(|effect| route.sends.level(effect)) {
            checkpoints.push(gain.prepare_send(level, control)?);
        }
        for (layer, (clock, _)) in route.layers.iter().enumerate().rev() {
            control.spend_plan_work(1)?;
            let owner = &owners[&OwnerKey::new(clock)];
            if !owner.skipped {
                gain.owner_slice(clock, AudioSample(0), 0..0, true, control)?;
            }
            if let Some(recipe) = clock.treatments() {
                for (stage, kind) in recipe.order()[..owner.until].iter().enumerate() {
                    if flushes(*kind, recipe) {
                        checkpoints.push(std::mem::take(&mut gain).prepare(control)?);
                    } else if *kind == AudioTreatmentStage::ClipGain {
                        let clip = recipe
                            .clip_gain()
                            .ok_or(PlanError::InvalidPlan("contribution ClipGain absent"))?;
                        authored_gain::PreparedClipGain::prepare(clip, control)?
                            .ok_or(PlanError::InvalidPlan(
                                "dynamic contribution gain checkpoint",
                            ))?
                            .apply(&mut gain, control)?;
                    } else {
                        gain.treatment(recipe, ExactRatio::ZERO, control, stage..stage + 1)?;
                    }
                }
                if let Some(level) = send.and_then(|effect| recipe.tail_sends().level(effect)) {
                    checkpoints.push(gain.prepare_send(level, control)?);
                }
                if owner.wet && route.wet_allowed(layer) && !recipe.wet_sends().is_empty() {
                    checkpoints.push(std::mem::take(&mut gain).prepare(control)?);
                    for effect in [
                        deadpan_core::TailEffect::Reverb,
                        deadpan_core::TailEffect::Delay,
                    ] {
                        if let Some(level) = recipe.wet_sends().level(effect) {
                            checkpoints.push(
                                authored_gain::GainSum::default().prepare_send(level, control)?,
                            );
                        }
                    }
                }
            }
        }
        if selected {
            checkpoints.push(gain.prepare(control)?);
        }
        Ok(Some(Self {
            checkpoints,
            next: 0,
        }))
    }
}

fn flushes(kind: AudioTreatmentStage, recipe: &deadpan_core::AudioTreatments) -> bool {
    kind == AudioTreatmentStage::Saturation || policy::is_equalizer(kind, Some(recipe))
}

#[cfg(test)]
#[path = "program/tests.rs"]
pub(super) mod tests;
