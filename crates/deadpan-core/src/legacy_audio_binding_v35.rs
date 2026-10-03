//! Closed frozen-source support for bindings authored before core schema 36.
#[cfg(test)]
use crate::{AudioBindingState, DocumentError, DocumentErrorCode, ValueChange};
use crate::{FrozenAudioKind, FrozenAudioLayout};
#[cfg(test)]
use serde::{Deserialize, Deserializer, Serialize, de};
#[cfg(test)]
use serde_json::value::RawValue;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
#[cfg(test)]
pub(crate) struct LegacyAudioBindingState(AudioBindingState);
#[cfg(test)]
impl LegacyAudioBindingState {
    pub(crate) fn project(state: &AudioBindingState) -> Option<Self> {
        supports(state).then(|| Self(state.clone()))
    }
}
pub(crate) fn supports_layout(layout: &FrozenAudioLayout) -> bool {
    !layout.has_editorial_edges() && layout.nodes().values().all(|node| !matches!(&node.kind,
        FrozenAudioKind::Source { placement: Some(placement) } if placement.start == placement.end
    ))
}
#[cfg(test)]
pub(crate) fn supports(state: &AudioBindingState) -> bool {
    state.timings().values().all(supports_layout)
        && crate::legacy_audio_binding_v36::supports(state)
}
#[cfg(test)]
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
#[cfg(test)]
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
#[cfg(test)]
impl<'de> Deserialize<'de> for LegacyAudioBindingState {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = Box::<RawValue>::deserialize(deserializer)?;
        from_json(raw.get()).map(Self).map_err(de::Error::custom)
    }
}
