//! Constant routes execute one owner/stage over the complete block. A unity
//! checkpoint removes its whole transfer pass; nonidentity arithmetic and
//! finite checks remain at the same per-sample processing boundaries.

use super::*;

impl Route<'_> {
    pub(super) fn transfer_block(
        &mut self,
        permitted: permissions::PermissionRow<'_>,
        control: WorkControl<'_>,
    ) -> Result<(), StageAudioError> {
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
        for (at, frame) in self.pcm.iter_mut().enumerate() {
            if permitted.contains(at) {
                frame.value = transfer.apply_wide(frame.value, control)?;
            }
        }
        Ok(())
    }

    pub(super) fn send_block(
        &mut self,
        output: &mut OutputSamples<'_>,
        event_input: bool,
        permitted: permissions::PermissionRow<'_>,
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
            for (at, frame) in self.pcm.iter().enumerate() {
                if permitted.contains(at) {
                    control.spend_plan_work(1)?;
                    let value = input(frame);
                    output.add(at, value);
                }
            }
        } else {
            for (at, frame) in self.pcm.iter().enumerate() {
                if permitted.contains(at) {
                    control.spend_plan_work(1)?;
                    let value = transfer.apply_wide(input(frame), control)?;
                    output.add(at, value);
                }
            }
        }
        Ok(())
    }
}

impl<'plan> Components<'plan> {
    pub(super) fn finish_constant_into(
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
        let owners = policy::ordered_owners(&self.owners);
        // A common send uses the first borrowed output as its workspace, as
        // the old single-output path did. Capture each sent frame before
        // overwriting any target; no extra output PCM allocation is needed.
        if self.send.is_some() {
            outputs[0].samples.clear();
        }
        for (id, route) in self.routes.iter_mut().enumerate() {
            if self
                .send
                .and_then(|effect| route.sends.level(effect))
                .is_some()
            {
                route.send_block(
                    &mut outputs[0].samples,
                    true,
                    self.permitted.row(id),
                    control,
                )?;
            }
        }
        for owner in owners {
            member_phases::run_owner(
                &mut self.routes,
                member_phases::OwnerPhase {
                    permitted: &self.permitted,
                    owner,
                    start: self.start,
                    frames: self.frames,
                    send: self.send,
                },
                member_phases::Numerics::Block {
                    output: &mut outputs[0].samples,
                },
                control,
                &mut equalize,
            )?;
        }
        for (id, route) in self.routes.iter_mut().enumerate() {
            if requested(outputs, route.voice, control)? {
                route.transfer_block(self.permitted.row(id), control)?;
            }
        }
        for at in 0..self.frames {
            control.check()?;
            let sent = if self.send.is_some() {
                outputs[0].samples.frame(at)
            } else {
                [0.0; 2]
            };
            for (index, output) in outputs.iter_mut().enumerate() {
                if index != 0 {
                    control.spend_plan_work(self.routes.len())?;
                }
                let mut sum = [0.0; 2];
                let mut permitted = false;
                for (id, route) in self.routes.iter().enumerate() {
                    if self.permitted.contains(id, at) && output.selection.selects(route.voice) {
                        permitted = true;
                        for (out, value) in sum.iter_mut().zip(route.pcm[at].value) {
                            *out += value;
                        }
                    }
                }
                if sum
                    .iter()
                    .chain(sent.iter())
                    .any(|value| !value.is_finite())
                {
                    return Err(PreparationError::InvalidSamples.into());
                }
                output
                    .samples
                    .set(at, if self.send.is_some() { sent } else { sum });
                if !permitted {
                    output.mark_suppressed(AudioSample(self.start.0 + at as i64));
                }
            }
        }
        Ok(())
    }
}
