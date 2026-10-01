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
