//! Granular guarded changes to retained audio timing state.
//!
//! A transaction records only the timing tables, owner bindings and sound
//! journals it changes, each with its exact before-value. Storing the complete
//! binding state in both directions of every history entry made each edit's
//! history row proportional to every retained clock in the project.

use std::collections::{BTreeMap, BTreeSet};

use serde::de::{self, Deserializer, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use crate::document::unique_map;
use crate::sound_clock::SoundClocks;
use crate::{
    AudioBindingState, AudioTimingId, DocumentError, EditError, EditErrorCode, FrozenAudioLayout,
    MAX_AUDIO_BINDING_ENTRIES, NodeId, OwnedAudioBinding, ValueChange,
};

/// One timing table's guarded replacement. Tables are immutable once named,
/// so a transaction only adds or removes them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AudioTimingChange {
    pub id: AudioTimingId,
    pub before: Option<FrozenAudioLayout>,
    pub after: Option<FrozenAudioLayout>,
}

/// Changed keys of an [`AudioBindingState`]. Application checks every
/// before-value; the containing document then validates the complete result.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct AudioBindingPatch {
    /// Sorted by identity and unique.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub timings: Vec<AudioTimingChange>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub bindings: BTreeMap<NodeId, ValueChange<OwnedAudioBinding>>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub gap_bindings: BTreeMap<NodeId, ValueChange<OwnedAudioBinding>>,
    /// Sound journals are small and bounded as a whole relation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sound_clocks: Option<ValueChange<SoundClocks>>,
}

impl AudioBindingPatch {
    /// The changed keys from `before` to `after`, or `None` when equal.
    pub fn between(before: &AudioBindingState, after: &AudioBindingState) -> Option<Self> {
        let timings = before
            .timings
            .keys()
            .chain(after.timings.keys())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .filter_map(|id| {
                let old = before.timings.get(id);
                let new = after.timings.get(id);
                (old != new).then(|| AudioTimingChange {
                    id: id.clone(),
                    before: old.cloned(),
                    after: new.cloned(),
                })
            })
            .collect();
        let patch = Self {
            timings,
            bindings: diff(&before.bindings, &after.bindings),
            gap_bindings: diff(&before.gap_bindings, &after.gap_bindings),
            sound_clocks: (before.sound_clocks != after.sound_clocks).then(|| ValueChange {
                before: Some(before.sound_clocks.clone()),
                after: Some(after.sound_clocks.clone()),
            }),
        };
        (!patch.is_empty()).then_some(patch)
    }

    pub fn inverse(&self) -> Self {
        Self {
            timings: self
                .timings
                .iter()
                .map(|change| AudioTimingChange {
                    id: change.id.clone(),
                    before: change.after.clone(),
                    after: change.before.clone(),
                })
                .collect(),
            bindings: invert(&self.bindings),
            gap_bindings: invert(&self.gap_bindings),
            sound_clocks: self.sound_clocks.as_ref().map(|change| ValueChange {
                before: change.after.clone(),
                after: change.before.clone(),
            }),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.timings.is_empty()
            && self.bindings.is_empty()
            && self.gap_bindings.is_empty()
            && self.sound_clocks.is_none()
    }

    /// Guarded application. Structural and live-owner validity belongs to the
    /// containing document's validation of the complete result.
    pub(crate) fn apply(&self, state: &AudioBindingState) -> Result<AudioBindingState, EditError> {
        if self.is_empty() {
            return Err(invalid(
                "an audio binding patch must change at least one entry",
            ));
        }
        let mut result = state.clone();
        for change in &self.timings {
            if result.timings.get(&change.id) != change.before.as_ref() {
                return Err(conflict());
            }
            match &change.after {
                Some(layout) => {
                    result.timings.insert(change.id.clone(), layout.clone());
                }
                None => {
                    result.timings.remove(&change.id);
                }
            }
        }
        apply_owner_changes(&mut result.bindings, &self.bindings)?;
        apply_owner_changes(&mut result.gap_bindings, &self.gap_bindings)?;
        if let Some(change) = &self.sound_clocks {
            if Some(&result.sound_clocks) != change.before.as_ref() {
                return Err(conflict());
            }
            result.sound_clocks = change
                .after
                .clone()
                .ok_or_else(|| invalid("sound clock replacement requires an after-value"))?;
        }
        if result.timings.len() > MAX_AUDIO_BINDING_ENTRIES {
            return Err(invalid("audio timing record count"));
        }
        Ok(result)
    }
}

impl AudioBindingPatch {
    /// Exactly `self.apply(state).map(|result| result == *expected)`, with the
    /// same guards and errors in the same order, without copying `state`.
    pub(crate) fn restores(
        &self,
        state: &AudioBindingState,
        expected: &AudioBindingState,
    ) -> Result<bool, EditError> {
        if !crate::command_work::local() {
            return self.apply(state).map(|result| result == *expected);
        }
        if self.is_empty() {
            return Err(invalid(
                "an audio binding patch must change at least one entry",
            ));
        }
        // The timing list is applied in order and may name an id twice.
        let mut timings: BTreeMap<&AudioTimingId, Option<&crate::FrozenAudioLayout>> =
            BTreeMap::new();
        for change in &self.timings {
            let current = timings
                .get(&change.id)
                .copied()
                .unwrap_or_else(|| state.timings.get(&change.id));
            if current != change.before.as_ref() {
                return Err(conflict());
            }
            timings.insert(&change.id, change.after.as_ref());
        }
        check_owner_changes(&state.bindings, &self.bindings)?;
        check_owner_changes(&state.gap_bindings, &self.gap_bindings)?;
        let sound_clocks = match &self.sound_clocks {
            Some(change) => {
                if Some(&state.sound_clocks) != change.before.as_ref() {
                    return Err(conflict());
                }
                change
                    .after
                    .as_ref()
                    .ok_or_else(|| invalid("sound clock replacement requires an after-value"))?
            }
            None => &state.sound_clocks,
        };
        let count = timings
            .iter()
            .fold(state.timings.len(), |count, (id, after)| {
                match (state.timings.contains_key(*id), after.is_some()) {
                    (false, true) => count + 1,
                    (true, false) => count - 1,
                    _ => count,
                }
            });
        if count > MAX_AUDIO_BINDING_ENTRIES {
            return Err(invalid("audio timing record count"));
        }
        let AudioBindingState {
            timings: expected_timings,
            bindings: expected_bindings,
            gap_bindings: expected_gaps,
            sound_clocks: expected_clocks,
        } = expected;
        let timings_equal = timings
            .iter()
            .all(|(id, after)| expected_timings.get(*id) == *after)
            && crate::command::unchanged_entries_equal(
                &state.timings,
                |id| timings.contains_key(id),
                expected_timings,
            );
        Ok(timings_equal
            && crate::command::patched_equals(&state.bindings, &self.bindings, expected_bindings)
            && crate::command::patched_equals(
                &state.gap_bindings,
                &self.gap_bindings,
                expected_gaps,
            )
            && sound_clocks == expected_clocks)
    }
}

fn check_owner_changes(
    values: &BTreeMap<NodeId, OwnedAudioBinding>,
    changes: &BTreeMap<NodeId, ValueChange<OwnedAudioBinding>>,
) -> Result<(), EditError> {
    for (owner, change) in changes {
        if values.get(owner) != change.before.as_ref() {
            return Err(conflict());
        }
    }
    Ok(())
}

fn diff(
    before: &BTreeMap<NodeId, OwnedAudioBinding>,
    after: &BTreeMap<NodeId, OwnedAudioBinding>,
) -> BTreeMap<NodeId, ValueChange<OwnedAudioBinding>> {
    crate::command::diff(before, after)
}

fn invert(
    changes: &BTreeMap<NodeId, ValueChange<OwnedAudioBinding>>,
) -> BTreeMap<NodeId, ValueChange<OwnedAudioBinding>> {
    changes
        .iter()
        .map(|(owner, change)| {
            (
                owner.clone(),
                ValueChange {
                    before: change.after.clone(),
                    after: change.before.clone(),
                },
            )
        })
        .collect()
}

fn apply_owner_changes(
    values: &mut BTreeMap<NodeId, OwnedAudioBinding>,
    changes: &BTreeMap<NodeId, ValueChange<OwnedAudioBinding>>,
) -> Result<(), EditError> {
    for (owner, change) in changes {
        if values.get(owner) != change.before.as_ref() {
            return Err(conflict());
        }
        match &change.after {
            Some(binding) => {
                values.insert(owner.clone(), binding.clone());
            }
            None => {
                values.remove(owner);
            }
        }
    }
    Ok(())
}

impl<'de> Deserialize<'de> for AudioBindingPatch {
    fn deserialize<D: Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            #[serde(default)]
            timings: Option<Box<RawValue>>,
            #[serde(default, deserialize_with = "unique_map")]
            bindings: BTreeMap<NodeId, ValueChange<OwnedAudioBinding>>,
            #[serde(default, deserialize_with = "unique_map")]
            gap_bindings: BTreeMap<NodeId, ValueChange<OwnedAudioBinding>>,
            #[serde(default)]
            sound_clocks: Option<Box<RawValue>>,
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct SoundWire {
            before: Option<Box<RawValue>>,
            after: Option<Box<RawValue>>,
        }
        let wire = Wire::deserialize(decoder)?;
        let timings = wire
            .timings
            .map(|raw| timing_changes(raw.get()))
            .transpose()
            .map_err(de::Error::custom)?
            .unwrap_or_default();
        for change in wire.bindings.values().chain(wire.gap_bindings.values()) {
            for binding in change.before.iter().chain(change.after.iter()) {
                crate::audio_binding::binding_wire_size(binding).map_err(de::Error::custom)?;
            }
        }
        let sound_clocks = wire
            .sound_clocks
            .map(|raw| {
                let wire: SoundWire =
                    serde_json::from_str(raw.get()).map_err(DocumentError::json)?;
                let read = |raw: Option<Box<RawValue>>| {
                    raw.map(|raw| crate::sound_clock::from_json(raw.get()))
                        .transpose()
                };
                Ok::<_, DocumentError>(ValueChange {
                    before: read(wire.before)?,
                    after: read(wire.after)?,
                })
            })
            .transpose()
            .map_err(de::Error::custom)?;
        let patch = Self {
            timings,
            bindings: wire.bindings,
            gap_bindings: wire.gap_bindings,
            sound_clocks,
        };
        if patch.is_empty() {
            return Err(de::Error::custom("empty audio binding patch"));
        }
        Ok(patch)
    }
}

/// Count every layout's collections before materializing any of them. Both
/// sides together are bounded by twice the retained-state budget.
fn timing_changes(json: &str) -> Result<Vec<AudioTimingChange>, DocumentError> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Change {
        id: AudioTimingId,
        before: Option<Box<RawValue>>,
        after: Option<Box<RawValue>>,
    }
    struct Bounded(Vec<Change>);
    impl<'de> Deserialize<'de> for Bounded {
        fn deserialize<D: Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
            struct Sequence;
            impl<'de> Visitor<'de> for Sequence {
                type Value = Vec<Change>;
                fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                    formatter.write_str("a bounded sequence of timing changes")
                }
                fn visit_seq<A: SeqAccess<'de>>(
                    self,
                    mut items: A,
                ) -> Result<Self::Value, A::Error> {
                    let mut values = Vec::new();
                    while let Some(value) = items.next_element()? {
                        if values.len() == MAX_AUDIO_BINDING_ENTRIES {
                            return Err(de::Error::custom("audio timing change limit"));
                        }
                        values.push(value);
                    }
                    Ok(values)
                }
            }
            decoder.deserialize_seq(Sequence).map(Bounded)
        }
    }
    let changes = serde_json::from_str::<Bounded>(json)
        .map_err(DocumentError::json)?
        .0;
    let mut budget = 0usize;
    for change in &changes {
        for raw in change.before.iter().chain(change.after.iter()) {
            let (nodes, runs, lineages) = FrozenAudioLayout::preflight_binding_counts(raw.get())?;
            budget = budget
                .checked_add(nodes + runs + lineages)
                .filter(|total| *total <= 2 * MAX_AUDIO_BINDING_ENTRIES)
                .ok_or_else(|| {
                    DocumentError::new(
                        crate::DocumentErrorCode::LimitExceeded,
                        "audio timing patch complexity",
                    )
                })?;
        }
    }
    let mut previous: Option<AudioTimingId> = None;
    let mut result = Vec::with_capacity(changes.len());
    for change in changes {
        if previous
            .as_ref()
            .is_some_and(|previous| *previous >= change.id)
        {
            return Err(DocumentError::new(
                crate::DocumentErrorCode::InvalidTree,
                "audio timing changes must be sorted and unique",
            ));
        }
        previous = Some(change.id.clone());
        let read = |raw: Option<Box<RawValue>>| {
            raw.map(|raw| FrozenAudioLayout::from_json(raw.get()))
                .transpose()
        };
        let before = read(change.before)?;
        let after = read(change.after)?;
        if before == after {
            return Err(DocumentError::new(
                crate::DocumentErrorCode::InvalidTree,
                "an audio timing change must change its table",
            ));
        }
        result.push(AudioTimingChange {
            id: change.id,
            before,
            after,
        });
    }
    Ok(result)
}

fn conflict() -> EditError {
    EditError::new(
        EditErrorCode::PatchConflict,
        "audio binding patch before-value does not match the current document",
    )
}

fn invalid(message: &str) -> EditError {
    EditError::new(EditErrorCode::InvalidCommand, message)
}
