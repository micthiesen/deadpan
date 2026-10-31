//! One existing request controls mask admission, including recursive transfers.
use super::*;
use point_suppression::ops::{MaskContext, MaskWorkPhase};

pub(crate) struct Context<'renderer, 'work> {
    renderer: &'renderer mut StageAudio,
    control: WorkControl<'work>,
}
impl MaskContext for Context<'_, '_> {
    fn check(&self) -> Result<(), StageAudioError> {
        self.control.check().map(|_| ())
    }

    fn spend(&self, phase: MaskWorkPhase, units: usize) -> Result<(), StageAudioError> {
        point_suppression::ops::spend_work(self.control, phase, units)
    }

    fn reserve(&mut self, frames: u64) -> Result<point_suppression::Charge, StageAudioError> {
        self.renderer.reserve_mask_storage(frames, self.control)
    }
}

impl StageAudio {
    pub(crate) fn masks_for_budget<'renderer, 'work>(
        &'renderer mut self,
        budget: &'work PreparationBudget<'_>,
    ) -> Context<'renderer, 'work> {
        self.masks(budget.control())
    }

    pub(super) fn masks<'renderer, 'work>(
        &'renderer mut self,
        control: WorkControl<'work>,
    ) -> Context<'renderer, 'work> {
        Context {
            renderer: self,
            control,
        }
    }
}

pub(super) struct TransferContext<'renderer, 'provider, 'work, P> {
    pub(super) renderer: &'renderer mut StageAudio,
    pub(super) provider: &'provider mut P,
    pub(super) control: WorkControl<'work>,
    pub(super) dependencies: Dependencies,
    pub(super) relative_depth: usize,
}
impl<'renderer, 'provider, 'work, P> TransferContext<'renderer, 'provider, 'work, P> {
    pub(super) fn new(
        renderer: &'renderer mut StageAudio,
        provider: &'provider mut P,
        control: WorkControl<'work>,
    ) -> Self {
        Self {
            renderer,
            provider,
            control,
            dependencies: Dependencies::new(),
            relative_depth: 0,
        }
    }
}
impl<P> MaskContext for TransferContext<'_, '_, '_, P> {
    fn check(&self) -> Result<(), StageAudioError> {
        self.control.check().map(|_| ())
    }

    fn spend(&self, phase: MaskWorkPhase, units: usize) -> Result<(), StageAudioError> {
        point_suppression::ops::spend_work(self.control, phase, units)
    }

    fn reserve(&mut self, frames: u64) -> Result<point_suppression::Charge, StageAudioError> {
        self.renderer.reserve_mask_storage(frames, self.control)
    }
}
