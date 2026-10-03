//! Closed owned-binding vocabulary for core schemas 16 through 20.
//!
//! Validate old binding fields before invoking the shared bounded timing/layout
//! decoder. Even an empty or null `reanchors` field is new authored vocabulary.

use serde::de;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::value::RawValue;

use crate::{AudioBindingState, DocumentError, DocumentErrorCode, ValueChange};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub(crate) struct LegacyAudioBindingState(AudioBindingState);

impl LegacyAudioBindingState {
    pub(crate) fn project(state: &AudioBindingState) -> Option<Self> {
        (crate::legacy_audio_binding_v21::supports(state)
            && state
                .bindings()
                .values()
                .all(|binding| binding.reanchors.is_empty()))
        .then(|| Self(state.clone()))
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
        // Walk only the old object vocabulary without materializing any trees
        // or collections. The existing decoder then enforces aggregate budgets,
        // unique map keys, frozen layout schemas and every unchanged value.
        crate::legacy_audio_binding_v21::validate_v20(raw.get()).map_err(de::Error::custom)?;
        crate::legacy_audio_binding_v35::from_json(raw.get())
            .map(Self)
            .map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use crate::*;
    use std::collections::BTreeMap;

    fn node(value: &str) -> NodeId {
        NodeId::new(value).unwrap()
    }

    fn bound_document() -> ProjectDocument {
        let empty = ProjectDocument::new(
            ProjectId::new("old-binding").unwrap(),
            RevisionId::new("initial").unwrap(),
            PresentationBasis {
                width: 16,
                height: 16,
                frame_rate: FrameRate::new(30_000, 1001).unwrap(),
                color_policy: ColorPolicy::SdrRec709,
            },
            node("root"),
        )
        .unwrap();
        let mut wire = serde_json::to_value(empty).unwrap();
        wire["nodes"] = serde_json::to_value(BTreeMap::from([
            (node("root"), BeatNode::sequence("Root", vec![node("hold")])),
            (
                node("hold"),
                BeatNode::hold(
                    "Hold",
                    HoldRecipe {
                        duration: FrameDuration::new(6).unwrap(),
                        video: HoldVideo::Background,
                        audio: HoldAudio::Silence,
                        picture_context: None,
                    },
                ),
            ),
        ]))
        .unwrap();
        let unbound = ProjectDocument::from_json(&wire.to_string()).unwrap();
        let state = capture_unbound_audio_bindings(
            &unbound,
            AudioTimingId {
                allocation: RevisionId::new("binding").unwrap(),
                ordinal: 0,
            },
        )
        .unwrap();
        wire["audio_bindings"] = serde_json::to_value(&state).unwrap();
        wire["audio_bindings"]["bindings"]["hold"]["resume"] = serde_json::to_value(AudioResume {
            local_boundary: ExactRatio::ONE,
            phase: AudioLocalPhase {
                constant: ExactRatio::new(1, 7).unwrap(),
                terms: vec![AudioPhaseTerm {
                    placement: state.bindings()[&node("hold")].lattice.clone(),
                    from_local: ExactRatio::ZERO,
                    to_local: ExactRatio::ONE,
                }],
            },
        })
        .unwrap();
        ProjectDocument::from_json(&wire.to_string()).unwrap()
    }

    #[test]
    fn projection_rejects_gap_intent_and_decoder_retains_duplicate_detection() {
        let document = bound_document();
        let state = document.audio_bindings();
        assert!(super::LegacyAudioBindingState::project(state).is_some());
        assert!(crate::legacy_audio_binding_v21::LegacyAudioBindingState::project(state).is_some());
        let mut gap_state = state.clone();
        gap_state.gap_bindings = gap_state.bindings.clone();
        assert!(super::LegacyAudioBindingState::project(&gap_state).is_none());
        assert!(
            crate::legacy_audio_binding_v21::LegacyAudioBindingState::project(&gap_state).is_none()
        );
        for position in ["lattice", "phase", "reanchor"] {
            let mut modern = state.clone();
            let binding = modern.bindings.values_mut().next().unwrap();
            let placement = match position {
                "lattice" => &mut binding.lattice,
                "phase" => &mut binding.resume.as_mut().unwrap().phase.terms[0].placement,
                _ => {
                    binding.reanchors.push(AudioReanchorStep {
                        anchor: Default::default(),
                        placement: binding.lattice.clone(),
                        window: None,
                    });
                    &mut binding.reanchors[0].placement
                }
            };
            placement.reference.recipe = AudioRecipeKind::RepeatGap;
            assert!(super::LegacyAudioBindingState::project(&modern).is_none());
            assert!(
                crate::legacy_audio_binding_v21::LegacyAudioBindingState::project(&modern)
                    .is_none()
            );
        }
        let encoded = state.to_json().unwrap();
        let binding = serde_json::to_string(&state.bindings()[&node("hold")]).unwrap();
        let duplicate = encoded.replace(
            "\"bindings\":{",
            &format!("\"bindings\":{{\"hold\":{binding},"),
        );
        assert!(serde_json::from_str::<super::LegacyAudioBindingState>(&duplicate).is_err());
        assert!(
            serde_json::from_str::<crate::legacy_audio_binding_v21::LegacyAudioBindingState>(
                &duplicate
            )
            .is_err()
        );
    }
}
