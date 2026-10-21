//! Whole-owner gain steps and explicit mute state use each staged Play target.

use super::*;

impl<F, R, S, P> Planner<'_, F, R, S, P>
where
    F: FnMut(SemanticAllocationRequest) -> Result<SemanticAllocation, EditError>,
    R: FnMut(&ProjectDocument, &RegisterValue) -> Result<SourceNode, EditError>,
    S: FnMut(&ProjectDocument) -> Result<Arc<SpeechTimeline>, EditError>,
    P: FnMut(&ProjectDocument, PauseSite) -> Result<PauseProvider, EditError>,
{
    pub(super) fn set_scoped_gain(
        &mut self,
        trace_index: usize,
        change: crate::AudioChange,
        scoped: SemanticScopedContext,
    ) -> Result<(), EditError> {
        if !matches!(
            change,
            crate::AudioChange::Step { .. } | crate::AudioChange::Mute { .. }
        ) {
            return Err(invalid(
                "single-play audio macros support only whole-owner gain steps and mute state",
            ));
        }
        framing::validate_scoped_context(&self.current, &self.context)?;
        let current = &self.current.nodes()[&scoped.target.node].audio_treatments;
        let treatments = change.apply(current).map_err(crate::audio_gain::invalid)?;
        if &treatments == current {
            return Err(invalid(
                if matches!(change, crate::AudioChange::Mute { .. }) {
                    "the beat already has that mute state; no edit was made"
                } else {
                    "the beat already has that gain and saturation; no edit was made"
                },
            ));
        }
        self.set_scoped_value_edit(
            trace_index,
            crate::ScopedNodeEdit::SetAudioTreatments { treatments },
            scoped,
            if matches!(change, crate::AudioChange::Mute { .. }) {
                "single-play mute"
            } else {
                "single-play gain"
            },
        )
    }
}
