//! Closed frozen-source support for bindings authored before core schema 36.
use crate::{
    AudioBindingState, DocumentError, DocumentErrorCode, FrozenAudioKind, FrozenAudioLayout,
    ValueChange,
};
use serde::{Deserialize, Deserializer, Serialize, de};
use serde_json::value::RawValue;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub(crate) struct LegacyAudioBindingState(AudioBindingState);
impl LegacyAudioBindingState {
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub(crate) fn upgrade(self) -> AudioBindingState {
        self.0
    }
    pub(crate) fn project(state: &AudioBindingState) -> Option<Self> {
        supports(state).then(|| Self(state.clone()))
    }
}
pub(crate) fn supports_layout(layout: &FrozenAudioLayout) -> bool {
    layout.nodes().values().all(|node| !matches!(&node.kind,
        FrozenAudioKind::Source { placement: Some(placement) } if placement.start == placement.end
    ))
}
pub(crate) fn supports(state: &AudioBindingState) -> bool {
    state.timings().values().all(supports_layout)
        && crate::legacy_audio_binding_v36::supports(state)
}
pub(crate) fn from_json(json: &str) -> Result<AudioBindingState, DocumentError> {
    // Keep the bounded streaming decoder and duplicate-field validation.
    let state = AudioBindingState::from_json(json)?;
    crate::legacy_audio_binding_v36::validate(json).map_err(DocumentError::json)?;
    if !supports(&state) {
        return Err(DocumentError::new(
            DocumentErrorCode::InvalidTree,
            "legacy audio timing cannot contain dormant source support",
        ));
    }
    Ok(state)
}
pub(crate) fn project_change(
    change: &Option<ValueChange<AudioBindingState>>,
) -> Option<Option<ValueChange<LegacyAudioBindingState>>> {
    let Some(change) = change else {
        return Some(None);
    };
    let project = |state: &Option<AudioBindingState>| match state {
        Some(state) => LegacyAudioBindingState::project(state).map(Some),
        None => Some(None),
    };
    Some(Some(ValueChange {
        before: project(&change.before)?,
        after: project(&change.after)?,
    }))
}
impl<'de> Deserialize<'de> for LegacyAudioBindingState {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = Box::<RawValue>::deserialize(deserializer)?;
        from_json(raw.get()).map(Self).map_err(de::Error::custom)
    }
}
