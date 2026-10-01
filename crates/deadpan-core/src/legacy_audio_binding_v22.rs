//! Core schema 22 permits authored Repeat-gap bindings, but frozen timing
//! layouts cannot contain sparse gap branches introduced in schema 23.

use serde::de;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::value::RawValue;

use crate::{AudioBindingState, DocumentError, DocumentErrorCode, ValueChange};

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
        if raw.get().len() > crate::MAX_DOCUMENT_JSON_BYTES {
            return Err(de::Error::custom(DocumentError::new(
                DocumentErrorCode::LimitExceeded,
                "legacy audio binding JSON byte limit",
            )));
        }
        crate::legacy_audio_binding_v21::validate_v22_layouts(raw.get())
            .map_err(de::Error::custom)?;
        let state =
            crate::legacy_audio_binding_v35::from_json(raw.get()).map_err(de::Error::custom)?;
        if !supports(&state) {
            return Err(de::Error::custom(
                "new recipe ownership in legacy audio binding",
            ));
        }
        Ok(Self(state))
    }
}

fn supports(state: &AudioBindingState) -> bool {
    crate::legacy_audio_binding_v35::supports(state)
        && state
            .timings()
            .values()
            .all(|layout| layout.gap_overrides().is_empty())
        && state.bindings().values().all(|binding| {
            binding
                .placements()
                .all(|placement| placement.reference.recipe == crate::AudioRecipeKind::Node)
        })
        && state.gap_bindings().values().all(|binding| {
            binding
                .placements()
                .all(|placement| placement.reference.recipe == crate::AudioRecipeKind::RepeatGap)
        })
}
