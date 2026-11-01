//! Request-local admission for current-sound and actual Original metadata.
//! Query/capture/probe, caller copies and node storage share the request ledger.
//! Wider renderer queries and the other Graph containers remain separate work.
#[cfg(test)]
use super::intrinsic_preflight_policy_native_trace as native_trace;
#[cfg(test)]
use super::read_signal_policy_native_trace as read_signal_trace;
#[cfg(test)]
use super::stage_validation_policy_native_trace as validation_trace;
use super::*;
use deadpan_plan::AudioSoundContextAdmission;
use std::alloc::Layout;

#[path = "sound_context_admission/policy_mask_handoff.rs"]
pub(in crate::stages) mod policy_mask_handoff;

#[derive(Clone, Copy)]
pub(super) struct Issuer<'work>(&'work RefCell<ReadWork>);
impl PartialEq for Issuer<'_> {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.0, other.0)
    }
}
impl Eq for Issuer<'_> {}
pub(super) fn issuer(control: WorkControl<'_>) -> Issuer<'_> {
    Issuer(control.work)
}

pub(super) struct Lease<'work> {
    ledger: &'work RefCell<ReadWork>,
    frames: u64,
}
impl Lease<'_> {
    pub(super) fn frames(&self) -> u64 {
        self.frames
    }
}
/// All identity/sum/subtraction checks precede Graph publication.
pub(super) struct JointTransfer<'work> {
    ledger: &'work RefCell<ReadWork>,
    remaining: u64,
}
#[derive(Default)]
pub(super) struct TransferLeases<'value, 'work> {
    pub(super) original: [Option<&'value Lease<'work>>; 3],
    pub(super) sound: Option<&'value Lease<'work>>,
    pub(super) key: Option<&'value Lease<'work>>,
    pub(super) boundary: Option<&'value Lease<'work>>,
    pub(super) capacity: Option<&'value Lease<'work>>,
}
#[derive(Default)]
pub(super) struct TransferLeasesMut<'value, 'work> {
    pub(super) original: [Option<&'value mut Lease<'work>>; 3],
    pub(super) sound: Option<&'value mut Lease<'work>>,
    pub(super) key: Option<&'value mut Lease<'work>>,
    pub(super) boundary: Option<&'value mut Lease<'work>>,
    pub(super) capacity: Option<&'value mut Lease<'work>>,
}
pub(super) struct TileTransferLeases<'value, 'work> {
    pub(super) index: Option<&'value Lease<'work>>,
    pub(super) arc_shell: Option<&'value Lease<'work>>,
}
pub(super) struct TileTransferLeasesMut<'value, 'work> {
    pub(super) index: Option<&'value mut Lease<'work>>,
    pub(super) arc_shell: Option<&'value mut Lease<'work>>,
}
pub(super) struct TileTransfer<'work> {
    inner: JointTransfer<'work>,
}
pub(super) fn preflight_transfer<'work>(
    control: WorkControl<'work>,
    leases: TransferLeases<'_, 'work>,
) -> Result<JointTransfer<'work>, StageAudioError> {
    let [plan, core, buffer] = leases.original;
    preflight_slots(
        control,
        &[
            plan,
            core,
            buffer,
            leases.sound,
            leases.key,
            leases.boundary,
            leases.capacity,
        ],
    )
}
pub(super) fn preflight_tile_transfer<'work>(
    control: WorkControl<'work>,
    leases: TileTransferLeases<'_, 'work>,
) -> Result<TileTransfer<'work>, StageAudioError> {
    Ok(TileTransfer {
        inner: preflight_slots(control, &[leases.index, leases.arc_shell])?,
    })
}
fn preflight_slots<'work>(
    control: WorkControl<'work>,
    leases: &[Option<&Lease<'work>>],
) -> Result<JointTransfer<'work>, StageAudioError> {
    let mut total = 0_u64;
    for lease in leases.iter().flatten() {
        if !std::ptr::eq(lease.ledger, control.work) {
            return Err(PlanError::AudioMetadataIssuerMismatch.into());
        }
        total = total
            .checked_add(lease.frames)
            .ok_or(StageAudioError::Range)?;
    }
    let remaining = control
        .work
        .borrow()
        .sound_context_frames
        .checked_sub(total)
        .ok_or(StageAudioError::Range)?;
    Ok(JointTransfer {
        ledger: control.work,
        remaining,
    })
}
impl<'work> JointTransfer<'work> {
    /// The no-fail publication tail: no arithmetic, reservation or callbacks.
    pub(super) fn commit(self, leases: TransferLeasesMut<'_, 'work>) {
        let [plan, core, buffer] = leases.original;
        self.commit_slots(&mut [
            plan,
            core,
            buffer,
            leases.sound,
            leases.key,
            leases.boundary,
            leases.capacity,
        ]);
    }
    fn commit_slots(self, leases: &mut [Option<&mut Lease<'work>>]) {
        self.ledger.borrow_mut().sound_context_frames = self.remaining;
        for lease in leases.iter_mut().flatten() {
            lease.frames = 0;
        }
    }
}
impl<'work> TileTransfer<'work> {
    pub(super) fn commit(self, leases: TileTransferLeasesMut<'_, 'work>) {
        self.inner
            .commit_slots(&mut [leases.index, leases.arc_shell]);
    }
}

impl Drop for Lease<'_> {
    fn drop(&mut self) {
        if self.frames != 0 {
            #[cfg(test)]
            let read_signal_release = read_signal_trace::release_begin(self.ledger, self.frames);
            #[cfg(test)]
            let validation_release = validation_trace::release_begin(self.ledger, self.frames);
            self.ledger.borrow_mut().sound_context_frames -= self.frames;
            #[cfg(test)]
            native_trace::released(self.ledger, self.frames);
            #[cfg(test)]
            validation_trace::release_result(validation_release, self.ledger);
            #[cfg(test)]
            read_signal_trace::release_result(read_signal_release, self.ledger);
        }
    }
}
pub(super) struct Admission<'controller, 'work> {
    controller: &'controller mut StageAudio,
    control: WorkControl<'work>,
}
impl<'controller, 'work> Admission<'controller, 'work> {
    pub(super) fn new(
        controller: &'controller mut StageAudio,
        control: WorkControl<'work>,
    ) -> Self {
        Self {
            controller,
            control,
        }
    }
}
impl<'work> AudioSoundContextAdmission for Admission<'_, 'work> {
    type Error = StageAudioError;
    type Issuer = Issuer<'work>;
    type Lease = Lease<'work>;
    fn issuer(&self) -> Self::Issuer {
        issuer(self.control)
    }
    fn spend(&mut self, amount: usize) -> Result<(), Self::Error> {
        #[cfg(test)]
        let read_signal_attempt = read_signal_trace::spend_begin(self.control, amount);
        #[cfg(test)]
        read_signal_attempt.apply_cancellation(self.control);
        #[cfg(test)]
        let validation_attempt = validation_trace::spend_begin(self.control, amount);
        #[cfg(test)]
        validation_attempt.apply_cancellation(self.control);
        #[cfg(test)]
        let attempt = native_trace::spend_begin(self.control, amount);
        #[cfg(test)]
        attempt.apply_cancellation(self.control);
        #[cfg(test)]
        if attempt.injected_refusal() {
            let result = self.control.check().and(Err(StageAudioError::Limit(
                "test-only intrinsic policy spend refusal",
            )));
            native_trace::spend_result(attempt, self.control, &result);
            #[cfg(test)]
            validation_trace::spend_result(validation_attempt, self.control, &result);
            #[cfg(test)]
            read_signal_trace::spend_result(read_signal_attempt, self.control, &result);
            return result;
        }
        #[cfg(test)]
        if validation_attempt.injected_refusal() {
            let result = self.control.check().and(Err(StageAudioError::Limit(
                "test-only stage validation policy spend refusal",
            )));
            native_trace::spend_result(attempt, self.control, &result);
            validation_trace::spend_result(validation_attempt, self.control, &result);
            #[cfg(test)]
            read_signal_trace::spend_result(read_signal_attempt, self.control, &result);
            return result;
        }
        #[cfg(test)]
        if read_signal_attempt.injected_refusal() {
            let result = self.control.check().and(Err(StageAudioError::Limit(
                "test-only read signal policy spend refusal",
            )));
            native_trace::spend_result(attempt, self.control, &result);
            validation_trace::spend_result(validation_attempt, self.control, &result);
            read_signal_trace::spend_result(read_signal_attempt, self.control, &result);
            return result;
        }
        let result = self.control.spend_plan_work(amount);
        #[cfg(test)]
        native_trace::spend_result(attempt, self.control, &result);
        #[cfg(test)]
        validation_trace::spend_result(validation_attempt, self.control, &result);
        #[cfg(test)]
        read_signal_trace::spend_result(read_signal_attempt, self.control, &result);
        result
    }
    fn reserve(&mut self, layout: Layout) -> Result<Self::Lease, Self::Error> {
        #[cfg(test)]
        let read_signal_attempt = read_signal_trace::reserve_begin(self.control, layout);
        #[cfg(test)]
        read_signal_attempt.apply_cancellation(self.control);
        #[cfg(test)]
        let validation_attempt = validation_trace::reserve_begin(self.control, layout);
        #[cfg(test)]
        validation_attempt.apply_cancellation(self.control);
        #[cfg(test)]
        let attempt = native_trace::reserve_begin(self.control, layout);
        #[cfg(test)]
        attempt.apply_cancellation(self.control);
        let result = (|| {
            self.control.check()?;
            #[cfg(test)]
            if attempt.injected_refusal() {
                return Err(StageAudioError::Limit(
                    "test-only intrinsic policy reserve refusal",
                ));
            }
            #[cfg(test)]
            if validation_attempt.injected_refusal() {
                return Err(StageAudioError::Limit(
                    "test-only stage validation policy reserve refusal",
                ));
            }
            #[cfg(test)]
            if read_signal_attempt.injected_refusal() {
                return Err(StageAudioError::Limit(
                    "test-only read signal policy reserve refusal",
                ));
            }
            let frames = u64::try_from(layout.size())
                .map_err(|_| StageAudioError::Range)?
                .div_ceil(std::mem::size_of::<[f32; 2]>() as u64);
            let next = self
                .control
                .work
                .borrow()
                .sound_context_frames
                .checked_add(frames)
                .ok_or(StageAudioError::Range)?;
            #[cfg(test)]
            let room = native_trace::room_begin(attempt, self.controller, self.control, frames);
            #[cfg(test)]
            let validation_room = validation_trace::room_begin(
                validation_attempt,
                self.controller,
                self.control,
                frames,
            );
            #[cfg(test)]
            let read_signal_room = read_signal_trace::room_begin(
                read_signal_attempt,
                self.controller,
                self.control,
                frames,
            );
            let room_result = self.controller.make_room(frames, false, self.control);
            #[cfg(test)]
            native_trace::room_result(room, self.controller, self.control, &room_result);
            #[cfg(test)]
            validation_trace::room_result(
                validation_room,
                self.controller,
                self.control,
                &room_result,
            );
            #[cfg(test)]
            read_signal_trace::room_result(
                read_signal_room,
                self.controller,
                self.control,
                &room_result,
            );
            room_result?;
            self.control.work.borrow_mut().sound_context_frames = next;
            Ok(Lease {
                ledger: self.control.work,
                frames,
            })
        })();
        #[cfg(test)]
        native_trace::reserve_result(
            attempt,
            self.control,
            result.as_ref().map(|lease| lease.frames),
        );
        #[cfg(test)]
        validation_trace::reserve_result(
            validation_attempt,
            self.control,
            result.as_ref().map(|lease| lease.frames),
        );
        #[cfg(test)]
        read_signal_trace::reserve_result(
            read_signal_attempt,
            self.control,
            result.as_ref().map(|lease| lease.frames),
        );
        result
    }
    fn merge(&mut self, target: &mut Self::Lease, source: &mut Self::Lease) {
        // Both were checked against this stable ledger before this callback.
        // The ledger's admitted sum is bounded by the existing u32 frame cap.
        #[cfg(test)]
        let validation_merge =
            validation_trace::merge_begin(self.control, target.frames, source.frames);
        #[cfg(test)]
        let read_signal_merge =
            read_signal_trace::merge_begin(self.control, target.frames, source.frames);
        target.frames += source.frames;
        source.frames = 0;
        #[cfg(test)]
        native_trace::merged(self.control);
        #[cfg(test)]
        validation_trace::merge_result(
            validation_merge,
            self.control,
            target.frames,
            source.frames,
        );
        #[cfg(test)]
        read_signal_trace::merge_result(
            read_signal_merge,
            self.control,
            target.frames,
            source.frames,
        );
    }
}

#[cfg(test)]
pub(super) struct Unmetered;
#[cfg(test)]
impl AudioSoundContextAdmission for Unmetered {
    const METERED: bool = false;
    type Error = StageAudioError;
    type Issuer = ();
    type Lease = ();
    fn issuer(&self) {}
    fn spend(&mut self, _: usize) -> Result<(), Self::Error> {
        Ok(())
    }
    fn reserve(&mut self, _: Layout) -> Result<(), Self::Error> {
        Ok(())
    }
    fn merge(&mut self, _: &mut (), _: &mut ()) {}
}

#[cfg(test)]
#[path = "sound_context_admission_tests.rs"]
mod tests;
