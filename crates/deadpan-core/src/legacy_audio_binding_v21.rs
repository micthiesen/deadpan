//! Closed owned-binding vocabulary through core schema 21.
//!
//! Stream the complete placement grammar before the shared bounded decoder.
//! Frozen layouts already have a separate closed schema; their existing gap
//! geometry is not authored gap-binding intent.

use std::{fmt, marker::PhantomData};

use serde::de::{self, IgnoredAny, MapAccess, SeqAccess, Visitor};
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

pub(crate) fn supports(state: &AudioBindingState) -> bool {
    state
        .timings()
        .values()
        .all(|layout| layout.gap_overrides().is_empty())
        && state.gap_bindings().is_empty()
        && state.bindings().values().all(|binding| {
            binding.placements().all(|placement| {
                placement.gap_after.is_none()
                    && placement.reference.recipe == crate::AudioRecipeKind::Node
                    && matches!(
                        placement.reference.root,
                        crate::AudioClockRoot::ProjectRootRoundEven
                            | crate::AudioClockRoot::PreserveInputPointCeil { .. }
                            | crate::AudioClockRoot::DefinitionPointCeil { .. }
                    )
            })
        })
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
        validate_v21(raw.get()).map_err(de::Error::custom)?;
        AudioBindingState::from_json(raw.get())
            .map(Self)
            .map_err(de::Error::custom)
    }
}

pub(crate) fn validate_v20(json: &str) -> Result<(), serde_json::Error> {
    serde_json::from_str::<OldState<OldBinding20>>(json).map(|_| ())
}

fn validate_v21(json: &str) -> Result<(), serde_json::Error> {
    serde_json::from_str::<OldState<OldBinding21>>(json).map(|_| ())
}

pub(crate) fn validate_v22_layouts(json: &str) -> Result<(), serde_json::Error> {
    serde_json::from_str::<State22>(json).map(|_| ())
}

/// Check one schema-22 frozen layout before the current bounded decoder.
/// This rejects an explicit `gap_overrides` field, including `{}` or `null`.
pub(crate) fn validate_v22_layout(json: &str) -> Result<(), serde_json::Error> {
    serde_json::from_str::<OldFrozenLayout>(json).map(|_| ())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct State22 {
    #[serde(rename = "timings")]
    _timings: Sequence<OldTiming>,
    #[serde(rename = "bindings")]
    _bindings: IgnoredAny,
    #[serde(rename = "gap_bindings")]
    _gap_bindings: Option<IgnoredAny>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OldState<B> {
    #[serde(rename = "timings")]
    _timings: Sequence<OldTiming>,
    #[serde(rename = "bindings")]
    _bindings: Map<B>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OldTiming {
    #[serde(rename = "id")]
    _id: IgnoredAny,
    #[serde(rename = "layout")]
    _layout: OldFrozenLayout,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OldFrozenLayout {
    #[serde(rename = "root")]
    _root: IgnoredAny,
    #[serde(rename = "rate")]
    _rate: IgnoredAny,
    #[serde(rename = "nodes")]
    _nodes: IgnoredAny,
    #[serde(rename = "overrides")]
    _overrides: IgnoredAny,
    #[serde(rename = "audio_lineage")]
    _audio_lineage: Option<IgnoredAny>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OldBinding20 {
    #[serde(rename = "lattice")]
    _lattice: OldPlacement,
    #[serde(rename = "resume")]
    _resume: Option<OldResume>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OldBinding21 {
    #[serde(rename = "lattice")]
    _lattice: OldPlacement,
    #[serde(rename = "resume")]
    _resume: Option<OldResume>,
    #[serde(rename = "reanchors")]
    _reanchors: Option<Sequence<OldStep>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OldResume {
    #[serde(rename = "local_boundary")]
    _local_boundary: IgnoredAny,
    #[serde(rename = "phase")]
    _phase: OldPhase,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OldPhase {
    #[serde(rename = "constant")]
    _constant: IgnoredAny,
    #[serde(rename = "terms")]
    _terms: Sequence<OldTerm>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OldTerm {
    #[serde(rename = "placement")]
    _placement: OldPlacement,
    #[serde(rename = "from_local")]
    _from_local: IgnoredAny,
    #[serde(rename = "to_local")]
    _to_local: IgnoredAny,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OldStep {
    #[serde(rename = "placement")]
    _placement: OldPlacement,
    #[serde(rename = "window")]
    _window: Option<IgnoredAny>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OldPlacement {
    #[serde(rename = "reference")]
    _reference: OldReference,
    #[serde(rename = "arguments")]
    _arguments: Sequence<OldArgument>,
    #[serde(rename = "births")]
    _births: Sequence<OldBirth>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OldReference {
    #[serde(rename = "timing")]
    _timing: crate::AudioTimingId,
    #[serde(rename = "root")]
    _root: OldChoice<0>,
    #[serde(rename = "physical")]
    _physical: IgnoredAny,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OldArgument {
    #[serde(rename = "reference_repeat")]
    _reference_repeat: IgnoredAny,
    #[serde(rename = "value")]
    _value: OldChoice<1>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OldBirth {
    #[serde(rename = "repeat")]
    _repeat: IgnoredAny,
    #[serde(rename = "survivors")]
    _survivors: OldChoice<2>,
    #[serde(rename = "definition_root")]
    _definition_root: IgnoredAny,
}

// An internally tagged derived enum first buffers its entire map. Stream these
// closed tags instead, so hostile unknown values cannot allocate a JSON tree
// before the shared collection preflight. The shared decoder checks each tag's
// exact required fields, value types and duplicate fields afterwards.
struct OldChoice<const KIND: u8>;

impl<'de, const KIND: u8> Deserialize<'de> for OldChoice<KIND> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Choice<const KIND: u8>;
        impl<'de, const KIND: u8> Visitor<'de> for Choice<KIND> {
            type Value = OldChoice<KIND>;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("closed legacy audio binding variant")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let (tags, fields): (&[&str], &[&str]) = match KIND {
                    0 => (
                        &[
                            "project_root_round_even",
                            "preserve_input_point_ceil",
                            "definition_point_ceil",
                        ],
                        &["stage", "root"],
                    ),
                    1 => (&["live", "captured"], &["repeat", "iteration"]),
                    _ => (
                        &["captured_repeat", "run"],
                        &["repeat", "allocation", "first", "count"],
                    ),
                };
                while let Some(field) = map.next_key::<String>()? {
                    if field == "type" {
                        let tag = map.next_value::<String>()?;
                        if !tags.contains(&tag.as_str()) {
                            return Err(de::Error::custom("new variant in legacy audio binding"));
                        }
                    } else if fields.contains(&field.as_str()) {
                        map.next_value::<IgnoredAny>()?;
                    } else {
                        return Err(de::Error::custom(
                            "new field in legacy audio binding variant",
                        ));
                    }
                }
                Ok(OldChoice)
            }
        }
        deserializer.deserialize_map(Choice)
    }
}

// These visitors retain no entries. The modern decoder independently rejects
// duplicate map keys and charges all aggregate collection work before allocation.
struct Map<T>(PhantomData<T>);
struct Sequence<T>(PhantomData<T>);

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Map<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Entries<T>(PhantomData<T>);
        impl<'de, T: Deserialize<'de>> Visitor<'de> for Entries<T> {
            type Value = Map<T>;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("legacy owned audio bindings")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                while map.next_entry::<IgnoredAny, T>()?.is_some() {}
                Ok(Map(PhantomData))
            }
        }
        deserializer.deserialize_map(Entries(PhantomData))
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Sequence<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Entries<T>(PhantomData<T>);
        impl<'de, T: Deserialize<'de>> Visitor<'de> for Entries<T> {
            type Value = Sequence<T>;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("legacy owned audio binding sequence")
            }
            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut sequence: A,
            ) -> Result<Self::Value, A::Error> {
                while sequence.next_element::<T>()?.is_some() {}
                Ok(Sequence(PhantomData))
            }
        }
        deserializer.deserialize_seq(Entries(PhantomData))
    }
}
