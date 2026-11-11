//! Timing tables retained for independent sound clocks. Original audio
//! placement is derived from the current tree; these records only keep the
//! pre-edit structure each sound clock names.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io::{self, Write};

use serde::de::{self, DeserializeSeed, IgnoredAny, SeqAccess, Visitor};
use serde::ser::{SerializeSeq, SerializeStruct};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::value::RawValue;

use crate::{
    DocumentError, DocumentErrorCode, ExactRatio, FrozenAudioKind, FrozenAudioLayout, InstancePath,
    IterationId, MAX_DOCUMENT_JSON_BYTES, MAX_DOCUMENT_NODES, NodeId, NodeKind, ProjectDocument,
    RepeatInstance, RevisionId, TimeError,
};

pub(crate) mod admission;

pub const MAX_AUDIO_BINDING_ENTRIES: usize = 100_000;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioTimingId {
    pub allocation: RevisionId,
    pub ordinal: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AudioTimingRecord {
    pub id: AudioTimingId,
    pub layout: FrozenAudioLayout,
}

/// Closed serializable intent: independent sound clocks and the timing tables
/// they name. Source/recipe admission belongs to the current owned document
/// and the media host; these records only establish coordinates.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AudioBindingState {
    pub(crate) timings: BTreeMap<AudioTimingId, FrozenAudioLayout>,
    pub(crate) sound_clocks: crate::sound_clock::SoundClocks,
}

pub(crate) struct Work {
    pub(crate) used: usize,
    pub(crate) maximum: usize,
}
impl Work {
    fn new(maximum: usize) -> Result<Self, DocumentError> {
        if maximum == 0 || maximum > MAX_DOCUMENT_NODES {
            return Err(limit("invalid audio binding work limit"));
        }
        Ok(Self { used: 0, maximum })
    }
    fn spend(&mut self, count: usize) -> Result<(), DocumentError> {
        self.used = self
            .used
            .checked_add(count)
            .ok_or_else(|| limit("audio binding work overflow"))?;
        if self.used > self.maximum {
            return Err(limit("audio binding work exhausted"));
        }
        Ok(())
    }
    fn remaining(&self) -> Result<usize, DocumentError> {
        self.maximum
            .checked_sub(self.used)
            .filter(|count| *count > 0)
            .ok_or_else(|| limit("audio binding work exhausted"))
    }
}

impl AudioBindingState {
    /// Assemble independent sound journals over one checked timing table.
    /// Live owner/address admission still requires `validate_for`.
    pub fn new_with_sound_clocks(
        timings: Vec<AudioTimingRecord>,
        sound_clocks: BTreeMap<NodeId, BTreeMap<crate::SoundId, crate::SoundClockJournal>>,
    ) -> Result<Self, DocumentError> {
        let state = Self::assemble(timings, sound_clocks)?;
        state.to_json()?;
        Ok(state)
    }

    fn assemble(
        timings: Vec<AudioTimingRecord>,
        sound_clocks: BTreeMap<NodeId, BTreeMap<crate::SoundId, crate::SoundClockJournal>>,
    ) -> Result<Self, DocumentError> {
        if timings.len() > MAX_AUDIO_BINDING_ENTRIES {
            return Err(limit("audio timing record count"));
        }
        let mut unique = BTreeMap::new();
        for timing in timings {
            if unique.insert(timing.id, timing.layout).is_some() {
                return Err(invalid("duplicate audio timing identity"));
            }
        }
        Ok(Self {
            timings: unique,
            sound_clocks,
        })
    }
    pub fn is_empty(&self) -> bool {
        self.timings.is_empty() && self.sound_clocks.is_empty()
    }
    pub fn timings(&self) -> &BTreeMap<AudioTimingId, FrozenAudioLayout> {
        &self.timings
    }
    /// Chronological pre-edit clocks keyed by exact owner-local sound identity.
    pub fn sound_clocks(
        &self,
    ) -> &BTreeMap<NodeId, BTreeMap<crate::SoundId, crate::SoundClockJournal>> {
        &self.sound_clocks
    }

    /// Allocation names remain reserved even when no current Repeat uses them.
    pub fn allocation_ids(&self) -> BTreeSet<&RevisionId> {
        let mut ids = BTreeSet::new();
        for (identity, layout) in &self.timings {
            ids.insert(&identity.allocation);
            for node in layout.nodes().values() {
                if let FrozenAudioKind::Repeat { iterations, .. } = &node.kind {
                    for (allocation, _, _) in iterations.segments() {
                        ids.insert(allocation);
                    }
                }
            }
            for lineage in layout.audio_lineage().values() {
                ids.insert(&lineage.allocation);
            }
        }
        ids
    }

    pub fn validate(&self) -> Result<(), DocumentError> {
        self.validation_work().map(|_| ())
    }

    fn validation_work(&self) -> Result<Work, DocumentError> {
        let mut work = Work::new(MAX_AUDIO_BINDING_ENTRIES)?;
        if self.timings.len() > MAX_AUDIO_BINDING_ENTRIES {
            return Err(limit("audio binding record count"));
        }

        let mut nodes = 0usize;
        let mut runs = 0usize;
        for layout in self.timings.values() {
            work.spend(layout.nodes().len() + layout.audio_lineage().len())?;
            nodes = nodes
                .checked_add(layout.nodes().len())
                .ok_or_else(|| limit("audio timing node overflow"))?;
            for node in layout.nodes().values() {
                if let FrozenAudioKind::Repeat { iterations, .. } = &node.kind {
                    work.spend(iterations.segment_count())?;
                    runs = runs
                        .checked_add(iterations.segment_count())
                        .ok_or_else(|| limit("audio timing run overflow"))?;
                }
            }
            if nodes > MAX_AUDIO_BINDING_ENTRIES || runs > MAX_AUDIO_BINDING_ENTRIES {
                return Err(limit("aggregate audio timing complexity"));
            }
            // A layout is immutable and can only be constructed through
            // admission, which builds and checks its complete index. Rebuilding
            // that index here repeated the same pure check on every document
            // validation (most of a 10,000-node project's validation time).
            debug_assert!(layout.validate().is_ok());
            layout.require_follow_speed_clocks()?;
        }
        let mut used = BTreeSet::new();
        work.spend(crate::sound_clock::reference_count(&self.sound_clocks)?)?;
        crate::sound_clock::wire_size(&self.sound_clocks)?;
        for journals in self.sound_clocks.values() {
            for journal in journals.values() {
                for reference in journal.clocks() {
                    work.spend(reference.introduced_groups().len())?;
                    work.spend(reference.split_work_count())?;
                    work.spend(reference.isolation_work_count())?;
                    let timing = reference.timing();
                    if !self.timings.contains_key(timing) {
                        return Err(invalid("sound clock timing identity is missing"));
                    }
                    used.insert(timing);
                }
            }
        }
        if used.len() != self.timings.len() {
            return Err(invalid("unreferenced audio timing record"));
        }
        Ok(work)
    }

    pub fn validate_for(&self, document: &ProjectDocument) -> Result<(), DocumentError> {
        if self.is_empty() {
            return Ok(());
        }
        let mut work = self.validation_work()?;
        if self
            .timings
            .values()
            .any(|layout| layout.rate() != document.presentation_basis().frame_rate)
        {
            return Err(invalid("audio timing rate differs from the project rate"));
        }
        if !self.sound_clocks.is_empty() {
            // Charge the live snapshot before building its indexes or body.
            work.spend(document.nodes().len() + document.audio_lineage().len())?;
            for node in document.nodes().values() {
                if let NodeKind::Repeat { iterations, .. } = &node.kind {
                    work.spend(iterations.segment_count())?;
                }
            }
            let live = FrozenAudioLayout::capture_structural(document)?;
            let mut checked = BTreeMap::new();
            for (owner, journals) in &self.sound_clocks {
                for (sound, journal) in journals {
                    if !document
                        .beat_sounds()
                        .get(owner)
                        .is_some_and(|events| events.contains_key(sound))
                    {
                        return Err(invalid("sound clock address has no beat sound"));
                    }
                    for reference in journal.clocks() {
                        let timing = reference.timing();
                        // Bound relation cloning in the cache key even when a
                        // shared correspondence avoids another tree traversal.
                        work.spend(reference.introduced_groups().len())?;
                        work.spend(reference.split_work_count())?;
                        work.spend(reference.isolation_work_count())?;
                        let key = (
                            timing.clone(),
                            reference.scope().clone(),
                            journal.scope().clone(),
                            reference.repeats().clone(),
                            reference.introduced_groups().clone(),
                            reference.introduced_splits().to_vec(),
                            reference.introduced_isolations().to_vec(),
                            (reference.introduced_groups().contains(journal.scope())
                                || self.timings[timing].sound_uses_output_owner_proof(
                                    &live,
                                    (reference.scope(), journal.scope()),
                                    (reference.owner(), owner),
                                ))
                            .then(|| (reference.owner().clone(), owner.clone())),
                        );
                        if !checked.contains_key(&key) {
                            let proof = self.timings[timing]
                                .sound_owner_clock_correspondence_with_relations(
                                    &live,
                                    (reference.scope(), journal.scope()),
                                    (reference.owner(), owner),
                                    reference.repeats(),
                                    reference.relations(),
                                    work.remaining()?,
                                )?;
                            work.spend(proof.work())?;
                            checked.insert(key.clone(), proof);
                        }
                        let proof = checked
                            .get(&key)
                            .expect("sound clock correspondence was inserted");
                        if proof.historical_node(owner) != Some(reference.owner()) {
                            return Err(invalid(
                                "sound clock reference names another historical owner",
                            ));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub fn from_json(json: &str) -> Result<Self, DocumentError> {
        if json.len() > MAX_DOCUMENT_JSON_BYTES {
            return Err(limit("audio binding JSON byte limit"));
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire<'a> {
            #[serde(borrow)]
            timings: &'a RawValue,
            #[serde(default, borrow, deserialize_with = "present_raw")]
            sound_clocks: Option<&'a RawValue>,
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Timing<'a> {
            id: AudioTimingId,
            #[serde(borrow)]
            layout: &'a RawValue,
        }
        let wire: Wire<'_> = serde_json::from_str(json).map_err(DocumentError::json)?;
        let raws: Vec<&RawValue> =
            bounded_json_sequence(wire.timings.get(), MAX_AUDIO_BINDING_ENTRIES)?;
        let mut timing_wires = Vec::with_capacity(raws.len());
        let mut budget = Work::new(MAX_AUDIO_BINDING_ENTRIES)?;
        for raw in raws {
            let value: Timing<'_> = serde_json::from_str(raw.get()).map_err(DocumentError::json)?;
            let (nodes, runs, lineages) =
                FrozenAudioLayout::preflight_binding_counts(value.layout.get())?;
            budget.spend(nodes + runs + lineages)?;
            timing_wires.push(value);
        }
        let sound_clocks = wire
            .sound_clocks
            .map(|raw| crate::sound_clock::from_json(raw.get()))
            .transpose()?
            .unwrap_or_default();
        budget.spend(crate::sound_clock::reference_count(&sound_clocks)?)?;
        // All aggregate collection counts are charged before any frozen tree
        // or sound clock body is materialized.
        let mut timings = Vec::with_capacity(timing_wires.len());
        for value in timing_wires {
            timings.push(AudioTimingRecord {
                id: value.id,
                layout: FrozenAudioLayout::from_json(value.layout.get())?,
            });
        }
        // The admitted input already met the byte bound; re-serializing every
        // retained table on each document read only repeats it. Omitted
        // defaults can add a few canonical bytes, and every containing document
        // is bounded again whenever it is serialized for storage.
        let state = Self::assemble(timings, sound_clocks)?;
        state.validate()?;
        Ok(state)
    }

    pub fn to_json(&self) -> Result<String, DocumentError> {
        self.validate()?;
        let mut output = BoundedJson::default();
        serde_json::to_writer(&mut output, self).map_err(|error| {
            if output.exceeded {
                limit("audio binding JSON byte limit")
            } else {
                DocumentError::json(error)
            }
        })?;
        String::from_utf8(output.bytes).map_err(|error| invalid(&error.to_string()))
    }
}

impl Serialize for AudioBindingState {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        struct Timings<'a>(&'a BTreeMap<AudioTimingId, FrozenAudioLayout>);
        impl Serialize for Timings<'_> {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                #[derive(Serialize)]
                struct Record<'a> {
                    id: &'a AudioTimingId,
                    layout: &'a FrozenAudioLayout,
                }
                let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
                for (id, layout) in self.0 {
                    sequence.serialize_element(&Record { id, layout })?;
                }
                sequence.end()
            }
        }
        let mut state = serializer.serialize_struct(
            "AudioBindingState",
            1 + usize::from(!self.sound_clocks.is_empty()),
        )?;
        state.serialize_field("timings", &Timings(&self.timings))?;
        if !self.sound_clocks.is_empty() {
            state.serialize_field("sound_clocks", &self.sound_clocks)?;
        }
        state.end()
    }
}

impl<'de> Deserialize<'de> for AudioBindingState {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = Box::<RawValue>::deserialize(deserializer)?;
        Self::from_json(raw.get()).map_err(de::Error::custom)
    }
}

/// Raw values keep tagged enums and identifiers borrowed until collection
fn present_raw<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<&'de RawValue>, D::Error> {
    <&RawValue>::deserialize(deserializer).map(Some)
}

/// Pretty document serialization changes indentation, never the string bytes.
/// Charge all bytes inside strings, including escapes, without allocating a
struct BoundedSequence<T> {
    maximum: usize,
    marker: std::marker::PhantomData<T>,
}
impl<'de, T: Deserialize<'de>> DeserializeSeed<'de> for BoundedSequence<T> {
    type Value = Vec<T>;
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_seq(self)
    }
}
impl<'de, T: Deserialize<'de>> Visitor<'de> for BoundedSequence<T> {
    type Value = Vec<T>;
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a bounded sequence")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value, A::Error> {
        let mut values = Vec::new();
        loop {
            if values.len() == self.maximum {
                if sequence.next_element::<IgnoredAny>()?.is_some() {
                    return Err(de::Error::custom("audio binding sequence limit"));
                }
                break;
            }
            let Some(value) = sequence.next_element()? else {
                break;
            };
            values.push(value);
        }
        Ok(values)
    }
}
fn bounded_json_sequence<'a, T: Deserialize<'a>>(
    json: &'a str,
    maximum: usize,
) -> Result<Vec<T>, DocumentError> {
    let mut decoder = serde_json::Deserializer::from_str(json);
    let values = BoundedSequence {
        maximum,
        marker: std::marker::PhantomData,
    }
    .deserialize(&mut decoder)
    .map_err(DocumentError::json)?;
    decoder.end().map_err(DocumentError::json)?;
    Ok(values)
}
#[derive(Default)]
struct BoundedJson {
    bytes: Vec<u8>,
    exceeded: bool,
}
impl Write for BoundedJson {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self
            .bytes
            .len()
            .checked_add(bytes.len())
            .is_none_or(|size| size > MAX_DOCUMENT_JSON_BYTES)
        {
            self.exceeded = true;
            return Err(io::Error::other("audio binding JSON byte limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn invalid(message: &str) -> DocumentError {
    DocumentError::new(DocumentErrorCode::InvalidTree, message)
}
fn limit(message: &str) -> DocumentError {
    DocumentError::new(DocumentErrorCode::LimitExceeded, message)
}

/// Removal never leaves dangling sound clocks or unreferenced timing tables.
pub(crate) fn prune(document: &mut ProjectDocument) {
    document
        .audio_bindings
        .sound_clocks
        .retain(|owner, journals| {
            if !document.nodes.contains_key(owner) {
                return false;
            }
            journals.retain(|sound, _| {
                document
                    .beat_sounds
                    .get(owner)
                    .is_some_and(|events| events.contains_key(sound))
            });
            !journals.is_empty()
        });
    let state = &mut document.audio_bindings;
    let mut retained = BTreeSet::new();
    for journals in state.sound_clocks.values() {
        for journal in journals.values() {
            retained.extend(journal.clocks().iter().map(|reference| reference.timing()));
        }
    }
    if retained.len() == state.timings.len()
        && retained
            .iter()
            .all(|identity| state.timings.contains_key(*identity))
    {
        // As many distinct retained identities as tables, each naming one:
        // every table is retained.
        return;
    }
    let retained: BTreeSet<AudioTimingId> = retained.into_iter().cloned().collect();
    state
        .timings
        .retain(|identity, _| retained.contains(identity));
}
