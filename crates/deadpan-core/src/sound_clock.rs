//! Retained structural clocks for independent beat-owned sound voices.
//!
//! Each reference names an immutable frozen scope and owner. The journal also
//! names the owner's current live scope. These records preserve clock history;
//! they do not establish media admission or permission to edit a sound owner.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

use crate::{AudioTimingId, DocumentError, DocumentErrorCode, MAX_DOCUMENT_DEPTH, NodeId, SoundId};

pub(crate) mod edit;

pub const MAX_SOUND_CLOCKS: usize = 1024;
pub const MAX_SOUND_CLOCK_BYTES: usize = 1024 * 1024;

/// External Repeat ancestors of a retained processing scope, outermost first.
/// Internal Repeats are paired by the complete subtree correspondence instead.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "RepeatMapWire")]
pub struct SoundClockRepeatMap {
    steps: Vec<SoundClockRepeatStep>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SoundClockRepeatStep {
    Shared {
        live_repeat: NodeId,
        historical_repeat: NodeId,
    },
    /// Only these stable plays copy the earlier definition clock. Later count
    /// growth is born on its own allocation, even after a shrink and regrow.
    Introduced {
        live_repeat: NodeId,
        plays: crate::IterationOrder,
    },
}

impl SoundClockRepeatStep {
    pub fn live_repeat(&self) -> &NodeId {
        match self {
            Self::Shared { live_repeat, .. } | Self::Introduced { live_repeat, .. } => live_repeat,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RepeatMapWire {
    #[serde(deserialize_with = "repeat_steps")]
    steps: Vec<SoundClockRepeatStep>,
}

fn repeat_steps<'de, D: Deserializer<'de>>(
    decoder: D,
) -> Result<Vec<SoundClockRepeatStep>, D::Error> {
    struct Steps;
    impl<'de> Visitor<'de> for Steps {
        type Value = Vec<SoundClockRepeatStep>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a bounded Repeat ancestry map")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut steps = Vec::new();
            while let Some(step) = seq.next_element()? {
                if steps.len() == MAX_DOCUMENT_DEPTH {
                    return Err(de::Error::custom("sound clock Repeat depth limit"));
                }
                steps.push(step);
            }
            Ok(steps)
        }
    }
    decoder.deserialize_seq(Steps)
}

impl TryFrom<RepeatMapWire> for SoundClockRepeatMap {
    type Error = DocumentError;
    fn try_from(wire: RepeatMapWire) -> Result<Self, Self::Error> {
        Self::new(wire.steps)
    }
}

impl SoundClockRepeatMap {
    pub fn new(steps: Vec<SoundClockRepeatStep>) -> Result<Self, DocumentError> {
        if steps.len() > MAX_DOCUMENT_DEPTH {
            return Err(limit("sound clock Repeat depth limit"));
        }
        let mut live = BTreeSet::new();
        let mut historical = BTreeSet::new();
        for step in &steps {
            if !live.insert(step.live_repeat()) {
                return Err(invalid("duplicate live sound clock Repeat"));
            }
            if let SoundClockRepeatStep::Shared {
                historical_repeat, ..
            } = step
                && !historical.insert(historical_repeat)
            {
                return Err(invalid("duplicate historical sound clock Repeat"));
            }
        }
        Ok(Self { steps })
    }
    pub fn steps(&self) -> &[SoundClockRepeatStep] {
        &self.steps
    }
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}

/// One historical owner's clock in a frozen processing scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SoundClockReference {
    timing: AudioTimingId,
    scope: NodeId,
    owner: NodeId,
    #[serde(default, skip_serializing_if = "SoundClockRepeatMap::is_empty")]
    repeats: SoundClockRepeatMap,
}

impl SoundClockReference {
    pub fn new(timing: AudioTimingId, scope: NodeId, owner: NodeId) -> Self {
        Self {
            timing,
            scope,
            owner,
            repeats: SoundClockRepeatMap::default(),
        }
    }

    pub fn timing(&self) -> &AudioTimingId {
        &self.timing
    }

    pub fn scope(&self) -> &NodeId {
        &self.scope
    }

    pub fn owner(&self) -> &NodeId {
        &self.owner
    }

    pub fn with_repeats(mut self, repeats: SoundClockRepeatMap) -> Self {
        self.repeats = repeats;
        self
    }

    pub fn repeats(&self) -> &SoundClockRepeatMap {
        &self.repeats
    }
}

/// Chronological pre-edit clocks. The current document supplies the final clock.
/// Equal frozen layouts remain distinct when their timing identities differ.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SoundClockJournal {
    scope: NodeId,
    clocks: Vec<SoundClockReference>,
}

impl SoundClockJournal {
    pub fn new(scope: NodeId, clocks: Vec<SoundClockReference>) -> Result<Self, DocumentError> {
        if clocks.is_empty() {
            return Err(invalid("sound clock journal requires at least one capture"));
        }
        if clocks.len() > MAX_SOUND_CLOCKS {
            return Err(limit("sound clock journal capture limit"));
        }
        if clocks
            .iter()
            .map(SoundClockReference::timing)
            .collect::<BTreeSet<_>>()
            .len()
            != clocks.len()
        {
            return Err(invalid("duplicate sound clock capture identity"));
        }
        Ok(Self { scope, clocks })
    }

    /// The current live processing scope containing this sound owner.
    pub fn scope(&self) -> &NodeId {
        &self.scope
    }

    /// Chronological historical clocks, oldest first.
    pub fn clocks(&self) -> &[SoundClockReference] {
        &self.clocks
    }

    pub fn with_appended(&self, reference: SoundClockReference) -> Result<Self, DocumentError> {
        if self.clocks.len() == MAX_SOUND_CLOCKS {
            return Err(limit("sound clock journal capture limit"));
        }
        if self
            .clocks
            .iter()
            .any(|previous| previous.timing == reference.timing)
        {
            return Err(invalid("duplicate sound clock capture identity"));
        }
        let mut clocks = self.clocks.clone();
        clocks.push(reference);
        Ok(Self {
            scope: self.scope.clone(),
            clocks,
        })
    }
}

struct BoundedReferences(Vec<SoundClockReference>);

impl<'de> Deserialize<'de> for BoundedReferences {
    fn deserialize<D: Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        struct References;
        impl<'de> Visitor<'de> for References {
            type Value = BoundedReferences;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a bounded sequence of sound clock references")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut clocks = Vec::new();
                while let Some(reference) = seq.next_element::<SoundClockReference>()? {
                    if clocks.len() == MAX_SOUND_CLOCKS {
                        return Err(de::Error::custom("sound clock journal capture limit"));
                    }
                    clocks.push(reference);
                }
                Ok(BoundedReferences(clocks))
            }
        }
        decoder.deserialize_seq(References)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct JournalWire {
    scope: NodeId,
    clocks: BoundedReferences,
}

impl<'de> Deserialize<'de> for SoundClockJournal {
    fn deserialize<D: Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let wire = JournalWire::deserialize(decoder)?;
        Self::new(wire.scope, wire.clocks.0).map_err(de::Error::custom)
    }
}

pub(crate) type SoundClocks = BTreeMap<NodeId, BTreeMap<SoundId, SoundClockJournal>>;

/// Decode only small identity journals before any retained layouts are built.
pub(crate) fn from_json(json: &str) -> Result<SoundClocks, DocumentError> {
    if json.len() > MAX_SOUND_CLOCK_BYTES {
        return Err(limit("sound clock journal byte limit"));
    }
    struct Events(BTreeMap<SoundId, SoundClockJournal>);
    impl<'de> Deserialize<'de> for Events {
        fn deserialize<D: Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
            let events = bounded_map(decoder)?;
            if events.is_empty() {
                return Err(de::Error::custom("sound clock owner map is empty"));
            }
            Ok(Self(events))
        }
    }
    let mut decoder = serde_json::Deserializer::from_str(json);
    let owners: BTreeMap<NodeId, Events> =
        bounded_map(&mut decoder).map_err(DocumentError::json)?;
    decoder.end().map_err(DocumentError::json)?;
    let clocks = owners
        .into_iter()
        .map(|(owner, events)| (owner, events.0))
        .collect();
    reference_count(&clocks)?;
    Ok(clocks)
}

fn bounded_map<'de, D, K, V>(decoder: D) -> Result<BTreeMap<K, V>, D::Error>
where
    D: Deserializer<'de>,
    K: Deserialize<'de> + Ord,
    V: Deserialize<'de>,
{
    struct Map<K, V>(std::marker::PhantomData<(K, V)>);
    impl<'de, K: Deserialize<'de> + Ord, V: Deserialize<'de>> Visitor<'de> for Map<K, V> {
        type Value = BTreeMap<K, V>;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a bounded unique sound clock map")
        }

        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
            let mut values = BTreeMap::new();
            while let Some(key) = map.next_key::<K>()? {
                if values.len() == crate::MAX_DOCUMENT_SOUNDS {
                    return Err(de::Error::custom("sound clock address limit"));
                }
                if values.contains_key(&key) {
                    return Err(de::Error::custom("duplicate sound clock map key"));
                }
                values.insert(key, map.next_value()?);
            }
            Ok(values)
        }
    }
    decoder.deserialize_map(Map(std::marker::PhantomData))
}

pub(crate) fn reference_count(clocks: &SoundClocks) -> Result<usize, DocumentError> {
    let mut events = 0usize;
    let mut references = 0usize;
    for journals in clocks.values() {
        if journals.is_empty() {
            return Err(invalid("sound clock owner map is empty"));
        }
        events = events
            .checked_add(journals.len())
            .ok_or_else(|| limit("sound clock count"))?;
        if events > crate::MAX_DOCUMENT_SOUNDS {
            return Err(limit("sound clock address limit"));
        }
        for journal in journals.values() {
            references = references
                .checked_add(journal.clocks().len())
                .ok_or_else(|| limit("sound clock reference count"))?;
        }
    }
    Ok(references)
}

pub(crate) fn wire_size(clocks: &SoundClocks) -> Result<(), DocumentError> {
    struct Count(usize);
    impl std::io::Write for Count {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self
                .0
                .checked_add(bytes.len())
                .filter(|count| *count <= MAX_SOUND_CLOCK_BYTES)
                .ok_or_else(|| std::io::Error::other("sound clock journal byte limit"))?;
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(&mut Count(0), clocks)
        .map_err(|_| limit("sound clock journal byte limit"))
}

fn invalid(message: &str) -> DocumentError {
    DocumentError::new(DocumentErrorCode::InvalidTree, message)
}
fn limit(message: &str) -> DocumentError {
    DocumentError::new(DocumentErrorCode::LimitExceeded, message)
}

#[cfg(test)]
mod tests;
