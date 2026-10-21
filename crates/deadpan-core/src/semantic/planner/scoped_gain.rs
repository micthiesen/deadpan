//! Whole-owner audio treatment intent uses each staged concrete Play target.

use super::*;

/// These changes use whole-owner treatment values and introduce no range,
/// independent effect-tail routing or modal proposal coordinates.
pub(super) fn supports(change: crate::AudioChange) -> bool {
    matches!(
        change,
        crate::AudioChange::Step { .. }
            | crate::AudioChange::Mute { .. }
            | crate::AudioChange::Trim { .. }
            | crate::AudioChange::Saturation { .. }
            | crate::AudioChange::Balance { .. }
            | crate::AudioChange::Equalizer { .. }
    )
}

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
        if !supports(change) {
            return Err(invalid(
                "single-play audio macros support only whole-owner gain, mute, saturation, balance and EQ",
            ));
        }
        framing::validate_scoped_context(&self.current, &self.context)?;
        let current = &self.current.nodes()[&scoped.target.node].audio_treatments;
        let treatments = change.apply(current).map_err(crate::audio_gain::invalid)?;
        if &treatments == current {
            return Err(invalid(match change {
                crate::AudioChange::Mute { .. } => {
                    "the beat already has that mute state; no edit was made"
                }
                crate::AudioChange::Step { .. } => {
                    "the beat already has that gain and saturation; no edit was made"
                }
                _ => "the beat already has those audio treatments; no edit was made",
            }));
        }
        self.set_scoped_value_edit(
            trace_index,
            crate::ScopedNodeEdit::SetAudioTreatments { treatments },
            scoped,
            match change {
                crate::AudioChange::Mute { .. } => "single-play mute",
                crate::AudioChange::Step { .. } | crate::AudioChange::Trim { .. } => {
                    "single-play gain"
                }
                _ => "single-play audio treatments",
            },
        )
    }
}
