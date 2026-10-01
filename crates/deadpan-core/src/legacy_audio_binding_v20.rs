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
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn upgrade(self) -> AudioBindingState {
        self.0
    }

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
    use serde_json::{Value, json};
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

    fn forbidden_fields(mut wire: Value, path: &str, reject: impl Fn(&str) -> bool) {
        let binding = wire.pointer(path).unwrap().clone();
        for value in [
            Value::Null,
            json!([]),
            json!([{"placement":binding["lattice"]}]),
        ] {
            wire.pointer_mut(path).unwrap()["reanchors"] = value;
            let encoded = wire.to_string();
            assert!(reject(&encoded), "admitted reanchors at {path}");
            assert!(
                reject(&encoded.replace("\"reanchors\":", "\"reanc\\u0068ors\":")),
                "admitted escaped reanchors at {path}"
            );
        }
    }

    fn forbidden_gap_fields(wire: &Value, path: &str, reject: impl Fn(&str) -> bool) {
        let check = |parent: &str, field: &str, value: Value| {
            let mut changed = wire.clone();
            changed.pointer_mut(parent).unwrap()[field] = value;
            let encoded = changed.to_string();
            assert!(reject(&encoded), "admitted {field} at {parent}");
            let escaped = format!("\\u{:04x}{}", u32::from(field.as_bytes()[0]), &field[1..]);
            assert!(
                reject(&encoded.replace(&format!("\"{field}\":"), &format!("\"{escaped}\":"))),
                "admitted escaped {field} at {parent}"
            );
        };
        for value in [Value::Null, json!({})] {
            check(path, "gap_bindings", value);
        }
        for (owner, binding) in wire.pointer(path).unwrap()["bindings"].as_object().unwrap() {
            let base = format!("{path}/bindings/{owner}");
            let mut placements = vec![format!("{base}/lattice")];
            if let Some(terms) = binding
                .pointer("/resume/phase/terms")
                .and_then(Value::as_array)
            {
                placements.extend(
                    (0..terms.len()).map(|i| format!("{base}/resume/phase/terms/{i}/placement")),
                );
            }
            if let Some(steps) = binding.get("reanchors").and_then(Value::as_array) {
                placements
                    .extend((0..steps.len()).map(|i| format!("{base}/reanchors/{i}/placement")));
            }
            for placement in placements {
                for value in [Value::Null, json!({})] {
                    check(&placement, "gap_after", value);
                }
                for value in [Value::Null, json!("node"), json!("repeat_gap")] {
                    check(&format!("{placement}/reference"), "recipe", value);
                }
                check(
                    &format!("{placement}/reference"),
                    "root",
                    json!({"type":"gap_definition_point_ceil", "repeat":owner}),
                );
            }
        }
    }

    macro_rules! strict_gap_schema {
        ($name:ident, $version:literal, $legacy:ident) => {
            #[test]
            fn $name() {
                let mut document = bound_document();
                if $version == 21 {
                    for binding in document.audio_bindings.bindings.values_mut() {
                        binding.reanchors.push(AudioReanchorStep {
                            anchor: Default::default(),
                            placement: binding.lattice.clone(),
                            window: None,
                        });
                    }
                }
                let mut wire = serde_json::to_value(&document).unwrap();
                wire["schema_version"] = json!($version);
                let old = $legacy::Document::from_json(&wire.to_string()).unwrap();
                assert!(old.matches(&document));
                assert_eq!(old.upgrade().unwrap(), document);
                forbidden_gap_fields(&wire, "/audio_bindings", |json| {
                    $legacy::Document::from_json(json).is_err()
                });
                let edit = apply(
                    &document,
                    &CommandRequest {
                        project_id: document.project_id().clone(),
                        expected_revision: document.revision_id().clone(),
                        new_revision: RevisionId::new("split-gap-boundary").unwrap(),
                        command: Command::Split {
                            node: node("hold"),
                            at: FrameDuration::new(2).unwrap(),
                            identities: SplitIdentities {
                                nodes: vec![node("left"), node("right"), node("copy")],
                            },
                        },
                    },
                )
                .unwrap();
                let wire = serde_json::to_value(&edit).unwrap();
                assert!($legacy::matches_edit(&wire.to_string(), &edit).unwrap());
                for direction in ["forward", "inverse"] {
                    for side in ["before", "after"] {
                        forbidden_gap_fields(
                            &wire,
                            &format!("/{direction}/audio_bindings/{side}"),
                            |json| $legacy::matches_edit(json, &edit).is_err(),
                        );
                    }
                }
            }
        };
    }

    strict_gap_schema!(schema16_rejects_nested_gap_vocabulary, 16, legacy_v16);
    strict_gap_schema!(schema17_rejects_nested_gap_vocabulary, 17, legacy_v17);
    strict_gap_schema!(schema18_rejects_nested_gap_vocabulary, 18, legacy_v18);
    strict_gap_schema!(schema19_rejects_nested_gap_vocabulary, 19, legacy_v19);
    strict_gap_schema!(schema20_rejects_nested_gap_vocabulary, 20, legacy_v20);
    strict_gap_schema!(schema21_rejects_nested_gap_vocabulary, 21, legacy_v21);

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

    macro_rules! strict_schema {
        ($name:ident, $version:literal, $legacy:ident) => {
            #[test]
            fn $name() {
                let document = bound_document();
                let mut wire = serde_json::to_value(&document).unwrap();
                wire["schema_version"] = json!($version);
                let old = $legacy::Document::from_json(&wire.to_string()).unwrap();
                assert!(old.matches(&document));
                assert_eq!(old.clone().upgrade().unwrap(), document);
                forbidden_fields(wire, "/audio_bindings/bindings/hold", |json| {
                    $legacy::Document::from_json(json).is_err()
                });
                let edit = apply(
                    &document,
                    &CommandRequest {
                        project_id: document.project_id().clone(),
                        expected_revision: document.revision_id().clone(),
                        new_revision: RevisionId::new("split").unwrap(),
                        command: Command::Split {
                            node: node("hold"),
                            at: FrameDuration::new(2).unwrap(),
                            identities: SplitIdentities {
                                nodes: vec![node("left"), node("right"), node("copy")],
                            },
                        },
                    },
                )
                .unwrap();
                let wire = serde_json::to_value(&edit).unwrap();
                assert!($legacy::matches_edit(&wire.to_string(), &edit).unwrap());
                for direction in ["forward", "inverse"] {
                    for side in ["before", "after"] {
                        let bindings = wire[direction]["audio_bindings"][side]["bindings"]
                            .as_object()
                            .unwrap();
                        for owner in bindings.keys() {
                            let path =
                                format!("/{direction}/audio_bindings/{side}/bindings/{owner}");
                            forbidden_fields(wire.clone(), &path, |json| {
                                $legacy::matches_edit(json, &edit).is_err()
                            });
                        }
                    }
                }
                let mut modern = document.clone();
                let binding = modern
                    .audio_bindings
                    .bindings
                    .get_mut(&node("hold"))
                    .unwrap();
                binding.reanchors.push(AudioReanchorStep {
                    anchor: Default::default(),
                    placement: binding.lattice.clone(),
                    window: None,
                });
                modern.validate().unwrap();
                assert!(!old.matches(&modern));
                let mut edit = edit;
                edit.forward.audio_bindings.as_mut().unwrap().before = Some(modern.audio_bindings);
                assert!(!$legacy::matches_edit(&wire.to_string(), &edit).unwrap());
            }
        };
    }

    strict_schema!(schema16_rejects_new_binding_vocabulary, 16, legacy_v16);
    strict_schema!(schema17_rejects_new_binding_vocabulary, 17, legacy_v17);
    strict_schema!(schema18_rejects_new_binding_vocabulary, 18, legacy_v18);
    strict_schema!(schema19_rejects_new_binding_vocabulary, 19, legacy_v19);
    strict_schema!(schema20_rejects_new_binding_vocabulary, 20, legacy_v20);
}
