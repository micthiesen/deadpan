//! Constant routes execute one owner/stage over the complete block. A unity
//! checkpoint removes its whole transfer pass; nonidentity arithmetic and
//! finite checks remain at the same per-sample processing boundaries.

use super::*;

impl Route<'_> {
    fn transfer_block(&mut self, control: WorkControl<'_>) -> Result<(), StageAudioError> {
        let Some(program) = &mut self.program else {
            // No add() means no admitted samples and no prepared checkpoints.
            return Ok(());
        };
        let transfer = program.next_transfer(control)?;
        if transfer.is_identity() {
            // add(), earlier transfers and every shared effect validate their
            // results. Nothing can introduce a nonfinite sample between them.
            return Ok(());
        }
        for frame in &mut self.pcm {
            if frame.permitted {
                frame.value = transfer.apply_wide(frame.value, control)?;
            }
        }
        Ok(())
    }

    fn send_block(
        &mut self,
        output: &mut [[f64; 2]],
        event_input: bool,
        control: WorkControl<'_>,
    ) -> Result<(), StageAudioError> {
        let Some(program) = &mut self.program else {
            return Ok(());
        };
        let transfer = program.next_transfer(control)?;
        let input = |frame: &Sample| {
            if event_input {
                frame.send_input
            } else {
                frame.value
            }
        };
        if transfer.is_identity() {
            for (frame, result) in self.pcm.iter().zip(output) {
                if frame.permitted {
                    control.spend_plan_work(1)?;
                    let value = input(frame);
                    for ch in 0..2 {
                        result[ch] += value[ch];
                    }
                }
            }
        } else {
            for (frame, result) in self.pcm.iter().zip(output) {
                if frame.permitted {
                    control.spend_plan_work(1)?;
                    let value = transfer.apply_wide(input(frame), control)?;
                    for ch in 0..2 {
                        result[ch] += value[ch];
                    }
                }
            }
        }
        Ok(())
    }
}

fn permissions(
    routes: &[Route<'_>],
    owner: &Owner<'_>,
    at: usize,
    wet: bool,
) -> Result<Permissions, StageAudioError> {
    let mut permissions = Permissions::default();
    for &(id, layer) in &owner.members {
        let route = &routes[id];
        if route.pcm[at].permitted && (!wet || route.wet_allowed(layer)) {
            permissions.insert(route.voice)?;
        }
    }
    Ok(permissions)
}

impl<'plan> Components<'plan> {
    pub(super) fn finish_constant(
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
        // This is the same single output allocation as the scalar path. Send
        // mode accumulates its returns here; final dry validation uses a local
        // sum and never needs a second full-sized PCM buffer.
        let mut output = vec![[0.0; 2]; self.frames];
        for route in &mut self.routes {
            if self
                .send
                .and_then(|effect| route.sends.level(effect))
                .is_some()
            {
                route.send_block(&mut output, true, control)?;
            }
        }
        for owner in owners {
            control.check()?;
            let recipe = owner.clock.treatments();
            for stage in 0..owner.until {
                let kind = recipe
                    .and_then(|recipe| recipe.order().get(stage))
                    .copied()
                    .ok_or(PlanError::InvalidPlan("contribution stage absent"))?;
                let eq = kind == AudioTreatmentStage::Equalizer
                    && recipe
                        .and_then(|recipe| recipe.equalizer())
                        .is_some_and(|recipe| !recipe.is_unity());
                if kind != AudioTreatmentStage::Saturation && !eq {
                    continue;
                }
                for &(id, _) in &owner.members {
                    self.routes[id].transfer_block(control)?;
                }
                for at in 0..self.frames {
                    control.check()?;
                    let permissions = permissions(&self.routes, owner, at, false)?;
                    if permissions.is_empty() {
                        continue;
                    }
                    let mut sum = [0.0; 2];
                    let mut count = 0;
                    for &(id, _) in &owner.members {
                        let frame = &self.routes[id].pcm[at];
                        if frame.permitted {
                            count += 1;
                            for (out, value) in sum.iter_mut().zip(frame.value) {
                                *out += value;
                            }
                        }
                    }
                    if sum.iter().any(|value| !value.is_finite()) {
                        return Err(PreparationError::InvalidSamples.into());
                    }
                    let saturation = recipe.and_then(|recipe| recipe.saturation());
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
                                    AudioSample(self.start.0 + at as i64),
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
                }
            }
            if let Some(effect) = self.send {
                for &(id, layer) in &owner.members {
                    if self.routes[id].layers[layer]
                        .0
                        .treatments()
                        .and_then(|recipe| recipe.tail_sends().level(effect))
                        .is_some()
                    {
                        self.routes[id].send_block(&mut output, false, control)?;
                    }
                }
            }
            if owner.wet {
                for &(id, layer) in &owner.members {
                    let route = &mut self.routes[id];
                    if !route.wet_allowed(layer) || route.program.is_none() {
                        continue;
                    }
                    let sends = route.layers[layer]
                        .0
                        .treatments()
                        .map(|recipe| recipe.wet_sends())
                        .unwrap_or_default();
                    if sends.is_empty() {
                        continue;
                    }
                    route.transfer_block(control)?;
                    // Borrow both send checkpoints once. They are applied to
                    // freshly returned samples, whose finiteness still needs
                    // checking even when a send transfer happens to be unity.
                    let mut program =
                        self.routes[id]
                            .program
                            .take()
                            .ok_or(PlanError::InvalidPlan(
                                "constant contribution program absent",
                            ))?;
                    let count =
                        usize::from(sends.level(deadpan_core::TailEffect::Reverb).is_some())
                            + usize::from(sends.level(deadpan_core::TailEffect::Delay).is_some());
                    // Retrieving the slice replaces the per-sample checkpoint
                    // dispatches, not the actual transfer applications.
                    let transfers = program.next_transfers(count, control)?;
                    for at in 0..self.frames {
                        control.check()?;
                        if !self.routes[id].pcm[at].permitted {
                            continue;
                        }
                        let wet_permissions = permissions(&self.routes, owner, at, true)?;
                        let clock = owner.members.iter().find_map(|&(member, layer)| {
                            let route = &self.routes[member];
                            (route.pcm[at].permitted && route.wet_allowed(layer))
                                .then(|| route.layers[layer].0.clone())
                        });
                        let mut transfer = transfers.iter();
                        for effect in [
                            deadpan_core::TailEffect::Reverb,
                            deadpan_core::TailEffect::Delay,
                        ] {
                            if sends.level(effect).is_none() {
                                continue;
                            }
                            let wet = equalize(
                                clock.as_ref().ok_or(PlanError::InvalidPlan(
                                    "wet return has no admitted clock",
                                ))?,
                                Temporal::Wet(effect),
                                AudioSample(self.start.0 + at as i64),
                                wet_permissions,
                                Some(self.routes[id].voice),
                            )?;
                            let wet = transfer
                                .next()
                                .ok_or(PlanError::InvalidPlan("wet gain checkpoint absent"))?
                                .apply_wide(wet, control)?;
                            for (out, input) in self.routes[id].pcm[at].value.iter_mut().zip(wet) {
                                *out += input;
                            }
                            if self.routes[id].pcm[at].value.iter().any(|x| !x.is_finite()) {
                                return Err(PreparationError::InvalidSamples.into());
                            }
                        }
                    }
                    self.routes[id].program = Some(program);
                }
            }
        }
        for route in &mut self.routes {
            if output_voice.is_none_or(|voice| voice == route.voice)
                || output_aliases.contains(route.voice)
            {
                route.transfer_block(control)?;
            }
        }
        let mut suppressed = Vec::new();
        for (at, result) in output.iter_mut().enumerate() {
            control.check()?;
            let mut sum = [0.0; 2];
            let mut permitted = false;
            for route in &self.routes {
                if route.pcm[at].permitted
                    && (output_voice.is_none_or(|voice| voice == route.voice)
                        || output_aliases.contains(route.voice))
                {
                    permitted = true;
                    for (out, value) in sum.iter_mut().zip(route.pcm[at].value) {
                        *out += value;
                    }
                }
            }
            if sum
                .iter()
                .chain(result.iter())
                .any(|value| !value.is_finite())
            {
                return Err(PreparationError::InvalidSamples.into());
            }
            if self.send.is_none() {
                *result = sum;
            }
            if !permitted {
                let sample = AudioSample(self.start.0 + at as i64);
                suppressed.push(sample..AudioSample(sample.0 + 1));
            }
        }
        Ok(MixedBlock {
            samples: output,
            suppressed: merged_suppression(suppressed),
        })
    }
}
