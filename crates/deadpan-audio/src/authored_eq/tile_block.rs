//! Pending shallow Arc storage. Tile PCM retains its separate existing bound.
//! Variable suppression/dependency storage is outside this admission slice.

use super::*;
use deadpan_plan::AudioSoundContextAdmission;
use std::alloc::Layout;

// The owning value must physically drop before its allocation charge.
pub(super) struct PendingArc<'work, T> {
    pub(super) value: Option<Arc<T>>,
    lease: Option<sound_context_admission::Lease<'work>>,
}

impl<'work, T> PendingArc<'work, T> {
    pub(super) fn new(
        controller: &mut StageAudio,
        value: T,
        control: WorkControl<'work>,
    ) -> Result<Self, StageAudioError> {
        // Same pinned Rust Arc shape contract as Plan metadata admission.
        // This does not claim allocator-overhead or RSS instrumentation.
        let layout = Layout::new::<[usize; 2]>()
            .extend(Layout::new::<T>())
            .map(|(layout, _)| layout.pad_to_align())
            .map_err(|_| StageAudioError::Range)?;
        let lease = {
            let mut admission = sound_context_admission::Admission::new(controller, control);
            admission.reserve(layout)?
        };
        Ok(Self {
            value: Some(Arc::new(value)),
            lease: Some(lease),
        })
    }

    pub(super) fn frames(&self) -> u64 {
        self.lease
            .as_ref()
            .map_or(0, sound_context_admission::Lease::frames)
    }

    pub(super) fn lease(&self) -> Option<&sound_context_admission::Lease<'work>> {
        self.lease.as_ref()
    }

    pub(super) fn lease_mut(&mut self) -> Option<&mut sound_context_admission::Lease<'work>> {
        self.lease.as_mut()
    }
}
