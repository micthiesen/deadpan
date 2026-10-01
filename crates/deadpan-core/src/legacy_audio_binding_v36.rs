//! Frozen placement vocabulary before exact physical-local origin offsets.
//! The current bounded decoder runs first; this streaming pass rejects even an
//! explicit zero/null new field in lattice, phase or chronological placements.
use crate::{AudioBindingState, ExactRatio};
use serde::de::{IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::fmt;
use std::marker::PhantomData;

pub(crate) fn supports(state: &AudioBindingState) -> bool {
    state
        .timings()
        .values()
        .all(|layout| !layout.has_editorial_edges())
        && state
            .bindings()
            .values()
            .chain(state.gap_bindings().values())
            .all(|binding| {
                binding
                    .reanchors
                    .iter()
                    .all(|step| step.anchor.is_allocation_entry())
                    && binding
                        .placements()
                        .all(|placement| placement.reference_local_offset == ExactRatio::ZERO)
            })
}
pub(crate) fn validate(json: &str) -> Result<(), serde_json::Error> {
    serde_json::from_str::<State>(json).map(|_| ())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    #[serde(rename = "timings")]
    _timings: Sequence<Timing>,
    #[serde(rename = "bindings")]
    _bindings: Map<Binding>,
    #[serde(rename = "gap_bindings")]
    _gap_bindings: Option<Map<Binding>>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Timing {
    #[serde(rename = "id")]
    _id: IgnoredAny,
    #[serde(rename = "layout", deserialize_with = "pre_editorial_layout")]
    _layout: (),
}

fn pre_editorial_layout<'de, D: Deserializer<'de>>(decoder: D) -> Result<(), D::Error> {
    let raw = <&serde_json::value::RawValue>::deserialize(decoder)?;
    crate::FrozenAudioLayout::validate_pre_editorial_json(raw.get())
        .map_err(serde::de::Error::custom)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    #[serde(rename = "lattice")]
    _lattice: Placement,
    #[serde(rename = "resume")]
    _resume: Option<Resume>,
    #[serde(rename = "reanchors")]
    _reanchors: Option<Sequence<Step>>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Resume {
    #[serde(rename = "local_boundary")]
    _local_boundary: IgnoredAny,
    #[serde(rename = "phase")]
    _phase: Phase,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Phase {
    #[serde(rename = "constant")]
    _constant: IgnoredAny,
    #[serde(rename = "terms")]
    _terms: Sequence<Term>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Term {
    #[serde(rename = "placement")]
    _placement: Placement,
    #[serde(rename = "from_local")]
    _from_local: IgnoredAny,
    #[serde(rename = "to_local")]
    _to_local: IgnoredAny,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Step {
    #[serde(rename = "placement")]
    _placement: Placement,
    #[serde(rename = "window")]
    _window: Option<IgnoredAny>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Placement {
    #[serde(rename = "reference")]
    _reference: IgnoredAny,
    #[serde(rename = "arguments")]
    _arguments: IgnoredAny,
    #[serde(rename = "births")]
    _births: IgnoredAny,
    #[serde(rename = "gap_after")]
    _gap_after: Option<IgnoredAny>,
}
struct Map<T>(PhantomData<T>);
struct Sequence<T>(PhantomData<T>);
impl<'de, T: Deserialize<'de>> Deserialize<'de> for Map<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Entries<T>(PhantomData<T>);
        impl<'de, T: Deserialize<'de>> Visitor<'de> for Entries<T> {
            type Value = Map<T>;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("legacy audio binding map")
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
                formatter.write_str("legacy audio binding sequence")
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

#[cfg(test)]
mod endpoint_tests {
    use super::*;
    use crate::*;
    use serde_json::json;

    fn fixture() -> AudioBindingState {
        let id = NodeId::new("source").unwrap();
        let time_base = SourceTimeBase::new(1, 30).unwrap();
        let picture = SourceSpan::new(
            SourceTimestamp {
                ticks: 0,
                time_base,
            },
            SourceTimestamp {
                ticks: 30,
                time_base,
            },
        )
        .unwrap();
        let blank = ProjectDocument::new(
            ProjectId::new("endpoint-wire").unwrap(),
            RevisionId::new("old").unwrap(),
            PresentationBasis {
                width: 16,
                height: 16,
                frame_rate: FrameRate::new(30_000, 1001).unwrap(),
                color_policy: ColorPolicy::SdrRec709,
            },
            NodeId::new("root").unwrap(),
        )
        .unwrap();
        let node = BeatNode {
            label: "Source".into(),
            framing: None,
            audio_treatments: Default::default(),
            audio_editorial_edges: Default::default(),
            audio_edges: Default::default(),
            kind: NodeKind::Source {
                source: SourceNode {
                    duration: FrameDuration::new(3).unwrap(),
                    edit_window: None,
                    video: SourceVideo::Stream {
                        asset: AssetId::new("picture").unwrap(),
                        span: picture,
                    },
                    video_mapping: SourceVideoMapping::FitBeat,
                    audio: None,
                    audio_mapping: SourceAudioMapping::FitBeat,
                    audio_offset: AudioSample(0),
                    link: LinkRelation::Independent,
                },
            },
        };
        let mut wire = serde_json::to_value(blank).unwrap();
        wire["nodes"]["root"] =
            serde_json::to_value(BeatNode::sequence("Root", vec![id.clone()])).unwrap();
        wire["nodes"]["source"] = serde_json::to_value(node).unwrap();
        wire["assets"]["picture"] = serde_json::to_value(AssetRecord {
            label: "Picture without audio".into(),
            content_hash: "a".repeat(64),
            video: Some(picture),
            audio: None,
            still_image: false,
            frame_count: Some(FrameDuration::new(30).unwrap()),
            source_qualification: None,
        })
        .unwrap();
        let doc = ProjectDocument::from_json(&wire.to_string()).unwrap();
        let mut state = capture_unbound_audio_bindings(
            &doc,
            AudioTimingId {
                allocation: RevisionId::new("timing").unwrap(),
                ordinal: 0,
            },
        )
        .unwrap();
        let binding = state.bindings.get_mut(&id).unwrap();
        binding.reanchors.push(AudioReanchorStep::for_allocation(
            binding.lattice.clone(),
            None,
        ));
        state
    }
    #[test]
    fn old_binding_ingress_rejects_even_default_or_escaped_anchor_fields() {
        let state = fixture();
        let encoded = state.to_json().unwrap();
        assert!(super::validate(&encoded).is_ok());
        assert!(supports(&state));
        assert!(crate::legacy_audio_binding_v35::from_json(&encoded).is_ok());
        assert!(
            serde_json::from_str::<crate::legacy_audio_binding_v22::LegacyAudioBindingState>(
                &encoded
            )
            .is_ok()
        );
        assert!(
            serde_json::from_str::<crate::legacy_audio_binding_v21::LegacyAudioBindingState>(
                &encoded
            )
            .is_ok()
        );
        let mut pre_reanchor = state.clone();
        pre_reanchor
            .bindings
            .values_mut()
            .next()
            .unwrap()
            .reanchors
            .clear();
        assert!(
            serde_json::from_str::<crate::legacy_audio_binding_v20::LegacyAudioBindingState>(
                &pre_reanchor.to_json().unwrap()
            )
            .is_ok()
        );
        for anchor in [
            json!({"type":"allocation_entry"}),
            json!({"type":"source_endpoint","endpoint":"end"}),
            json!(null),
        ] {
            let mut wire = serde_json::to_value(&state).unwrap();
            wire["bindings"]["source"]["reanchors"][0]["anchor"] = anchor;
            for encoded in [
                wire.to_string(),
                wire.to_string().replace("\"anchor\":", "\"anc\\u0068or\":"),
            ] {
                assert!(super::validate(&encoded).is_err());
                assert!(crate::legacy_audio_binding_v35::from_json(&encoded).is_err());
                assert!(serde_json::from_str::<crate::legacy_audio_binding_v22::LegacyAudioBindingState>(&encoded).is_err());
                assert!(serde_json::from_str::<crate::legacy_audio_binding_v21::LegacyAudioBindingState>(&encoded).is_err());
                assert!(serde_json::from_str::<crate::legacy_audio_binding_v20::LegacyAudioBindingState>(&encoded).is_err());
            }
        }
    }
    #[test]
    fn old_projection_rejects_endpoint_on_both_patch_sides() {
        let before = fixture();
        let mut after = before.clone();
        after.bindings.values_mut().next().unwrap().reanchors[0].anchor =
            AudioReanchorAnchor::SourceEndpoint {
                endpoint: AudioSourceEndpoint::End,
            };
        after.validate().unwrap();
        assert!(!supports(&after));
        assert!(
            crate::legacy_audio_binding_v35::LegacyAudioBindingState::project(&after).is_none()
        );
        for (left, right) in [(&before, &after), (&after, &before)] {
            let change = Some(ValueChange {
                before: Some(left.clone()),
                after: Some(right.clone()),
            });
            assert!(crate::legacy_audio_binding_v35::project_change(&change).is_none());
            assert!(crate::legacy_audio_binding_v22::project_change(&change).is_none());
            assert!(crate::legacy_audio_binding_v21::project_change(&change).is_none());
            assert!(crate::legacy_audio_binding_v20::project_change(&change).is_none());
        }
    }
}
