//! Request-local admission for current-sound metadata. No source/planning probes.
use super::*;
use deadpan_plan::AudioSoundContextAdmission;
use std::alloc::Layout;

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
    /// Called only after owning values and this same charge reach Graph.
    pub(super) fn commit(&mut self) {
        self.ledger.borrow_mut().sound_context_frames -= self.frames;
        self.frames = 0;
    }
}
impl Drop for Lease<'_> {
    fn drop(&mut self) {
        if self.frames != 0 {
            self.ledger.borrow_mut().sound_context_frames -= self.frames;
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
        self.control.spend_plan_work(amount)
    }
    fn reserve(&mut self, layout: Layout) -> Result<Self::Lease, Self::Error> {
        self.control.check()?;
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
        self.controller.make_room(frames, false, self.control)?;
        self.control.work.borrow_mut().sound_context_frames = next;
        Ok(Lease {
            ledger: self.control.work,
            frames,
        })
    }
    fn merge(&mut self, target: &mut Self::Lease, source: &mut Self::Lease) {
        // Both were checked against this stable ledger before this callback.
        // The ledger's admitted sum is bounded by the existing u32 frame cap.
        target.frames += source.frames;
        source.frames = 0;
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
