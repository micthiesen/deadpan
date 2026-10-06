//! Compact stored forms of edit transactions.
//!
//! A pause, Split or Repeat wrap at the project root changes one entry of the
//! root Sequence, but a node change records the whole node before and after,
//! and the transaction records the inverse patch as well. On a 10,000-beat
//! root that is four copies of a 10,000-entry child list in every history row.
//! Two exact encodings remove three of them:
//!
//! - A Sequence whose other fields are unchanged stores its new children as
//!   one splice of the old list (`after_children`) instead of `after`.
//! - A transaction whose inverse is exactly `forward.inverse()` omits it.
//!
//! Decoding reconstructs the identical values, so every reader sees the same
//! typed transaction, and the store still requires the stored text to decode
//! to the computed transaction before it commits. Rows in the complete form
//! remain readable.

use std::collections::BTreeMap;

use serde::de::{self, Deserializer};
use serde::ser::{SerializeMap, Serializer};
use serde::{Deserialize, Serialize};

use super::{DocumentPatch, EditTransaction, ValueChange};
use crate::{BeatNode, MAX_DOCUMENT_NODES, NodeId, NodeKind};

/// Only lists at least this long are worth encoding as a splice.
const MIN_SPLICED_CHILDREN: usize = 32;

/// `after.children = before.children[..at] ++ insert ++ before.children[at + remove..]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChildrenSplice {
    at: usize,
    remove: usize,
    insert: Vec<NodeId>,
}

#[derive(Serialize)]
struct SplicedChangeRef<'a> {
    before: &'a BeatNode,
    after_children: ChildrenSpliceRef<'a>,
}

#[derive(Serialize)]
struct ChildrenSpliceRef<'a> {
    at: usize,
    remove: usize,
    insert: &'a [NodeId],
}

/// Whether two nodes differ at most in their Sequence children. Destructuring
/// keeps this exhaustive: a new node field must be compared here.
fn same_but_children(before: &BeatNode, after: &BeatNode) -> bool {
    let BeatNode {
        label,
        kind: _,
        audio_treatments,
        audio_edges,
        audio_editorial_edges,
        framing,
        cutaways,
        captions,
    } = before;
    *label == after.label
        && *audio_treatments == after.audio_treatments
        && *audio_edges == after.audio_edges
        && *audio_editorial_edges == after.audio_editorial_edges
        && *framing == after.framing
        && *cutaways == after.cutaways
        && *captions == after.captions
}

/// The splice form of `change`, when it is exact and shorter.
fn splice(change: &ValueChange<BeatNode>) -> Option<(&BeatNode, ChildrenSpliceRef<'_>)> {
    let (Some(before), Some(after)) = (&change.before, &change.after) else {
        return None;
    };
    let (NodeKind::Sequence { children: old }, NodeKind::Sequence { children: new }) =
        (&before.kind, &after.kind)
    else {
        return None;
    };
    if new.len() < MIN_SPLICED_CHILDREN || !same_but_children(before, after) {
        return None;
    }
    let prefix = old
        .iter()
        .zip(new)
        .take_while(|(old, new)| old == new)
        .count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(old, new)| old == new)
        .count();
    let insert = &new[prefix..new.len() - suffix];
    // A splice is worthwhile only when it is much shorter than the list.
    (insert.len() * 2 < new.len()).then_some((
        before,
        ChildrenSpliceRef {
            at: prefix,
            remove: old.len() - prefix - suffix,
            insert,
        },
    ))
}

pub(super) fn serialize_nodes<S: Serializer>(
    nodes: &BTreeMap<NodeId, ValueChange<BeatNode>>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let mut map = serializer.serialize_map(Some(nodes.len()))?;
    for (id, change) in nodes {
        match splice(change) {
            Some((before, after_children)) => map.serialize_entry(
                id,
                &SplicedChangeRef {
                    before,
                    after_children,
                },
            )?,
            None => map.serialize_entry(id, change)?,
        }
    }
    map.end()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeChangeWire {
    before: Option<BeatNode>,
    /// `None` when the field is absent; `Some(None)` for an explicit null.
    #[serde(default, deserialize_with = "present")]
    after: Option<Option<BeatNode>>,
    #[serde(default)]
    after_children: Option<ChildrenSplice>,
}

fn present<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Option<BeatNode>>, D::Error> {
    Option::<BeatNode>::deserialize(deserializer).map(Some)
}

/// A node change decoded from either stored form.
pub(super) struct NodeChange(ValueChange<BeatNode>);

impl<'de> Deserialize<'de> for NodeChange {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = NodeChangeWire::deserialize(deserializer)?;
        let Some(splice) = wire.after_children else {
            return Ok(Self(ValueChange {
                before: wire.before,
                after: wire.after.flatten(),
            }));
        };
        let invalid = || de::Error::custom("invalid Sequence child splice");
        let before = wire.before.ok_or_else(invalid)?;
        let NodeKind::Sequence { children } = &before.kind else {
            return Err(invalid());
        };
        let end = splice.at.checked_add(splice.remove).ok_or_else(invalid)?;
        // The splice form has exactly one encoding: no `after` at all.
        if wire.after.is_some()
            || end > children.len()
            || children.len() - splice.remove + splice.insert.len() > MAX_DOCUMENT_NODES
        {
            return Err(invalid());
        }
        let mut new = Vec::with_capacity(children.len() - splice.remove + splice.insert.len());
        new.extend_from_slice(&children[..splice.at]);
        new.extend(splice.insert);
        new.extend_from_slice(&children[end..]);
        let mut after = before.clone();
        after.kind = NodeKind::Sequence { children: new };
        Ok(Self(ValueChange {
            before: Some(before),
            after: Some(after),
        }))
    }
}

impl crate::audio_gain::NodeGainInventory for NodeChange {
    fn gains(&self) -> [Option<&crate::AudioTreatments>; 2] {
        self.0.gains()
    }
}

pub(super) fn deserialize_nodes<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<NodeId, ValueChange<BeatNode>>, D::Error> {
    Ok(crate::audio_gain::node_map::<D, NodeChange>(deserializer)?
        .into_iter()
        .map(|(id, change)| (id, change.0))
        .collect())
}

#[derive(Serialize)]
struct TransactionRef<'a> {
    forward: &'a DocumentPatch,
    #[serde(skip_serializing_if = "Option::is_none")]
    inverse: Option<&'a DocumentPatch>,
    changed_ids: &'a [NodeId],
    duration_delta: i64,
    description: &'a str,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TransactionWire {
    forward: DocumentPatch,
    #[serde(default)]
    inverse: Option<DocumentPatch>,
    changed_ids: Vec<NodeId>,
    duration_delta: i64,
    description: String,
}

/// `inverse == forward.inverse()`, compared field by field without building
/// the reversed patch (whose node changes can hold a 10,000-entry list).
fn is_reverse(inverse: &DocumentPatch, forward: &DocumentPatch) -> bool {
    fn swapped<K: Eq, V: Eq>(
        reversed: &BTreeMap<K, ValueChange<V>>,
        forward: &BTreeMap<K, ValueChange<V>>,
    ) -> bool {
        reversed.len() == forward.len()
            && reversed
                .iter()
                .zip(forward)
                .all(|((key, back), (other, change))| {
                    key == other && back.before == change.after && back.after == change.before
                })
    }
    // Destructuring keeps this exhaustive when the patch gains a field.
    let DocumentPatch {
        project_id,
        from_revision,
        to_revision,
        presentation,
        nodes,
        assets,
        marks,
        sounds,
        beat_sounds,
        sound_routes,
        sound_allowances,
        targets,
        overrides,
        gap_overrides,
        audio_lineage,
        audio_bindings,
    } = inverse;
    let same = *project_id == forward.project_id
        && *from_revision == forward.to_revision
        && *to_revision == forward.from_revision
        && match (presentation, &forward.presentation) {
            (None, None) => true,
            (Some(back), Some(change)) => {
                back.before == change.after && back.after == change.before
            }
            _ => false,
        }
        && swapped(nodes, &forward.nodes)
        && swapped(assets, &forward.assets)
        && swapped(marks, &forward.marks)
        && swapped(sounds, &forward.sounds)
        && swapped(beat_sounds, &forward.beat_sounds)
        && swapped(sound_routes, &forward.sound_routes)
        && swapped(sound_allowances, &forward.sound_allowances)
        && swapped(targets, &forward.targets)
        && swapped(overrides, &forward.overrides)
        && swapped(gap_overrides, &forward.gap_overrides)
        && swapped(audio_lineage, &forward.audio_lineage)
        && match (audio_bindings, &forward.audio_bindings) {
            (None, None) => true,
            (Some(back), Some(change)) => back.is_reverse_of(change),
            _ => false,
        };
    debug_assert_eq!(same, *inverse == forward.inverse());
    same
}

impl Serialize for EditTransaction {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        TransactionRef {
            forward: &self.forward,
            inverse: (!is_reverse(&self.inverse, &self.forward)).then_some(&self.inverse),
            changed_ids: &self.changed_ids,
            duration_delta: self.duration_delta,
            description: &self.description,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for EditTransaction {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = TransactionWire::deserialize(deserializer)?;
        Ok(Self {
            inverse: wire.inverse.unwrap_or_else(|| wire.forward.inverse()),
            forward: wire.forward,
            changed_ids: wire.changed_ids,
            duration_delta: wire.duration_delta,
            description: wire.description,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ColorPolicy, FrameDuration, FrameRate, HoldAudio, HoldRecipe, HoldVideo, PresentationBasis,
        ProjectDocument, ProjectId, RevisionId,
    };

    fn id(value: &str) -> NodeId {
        NodeId::new(value).unwrap()
    }

    fn hold() -> BeatNode {
        BeatNode::hold(
            "Pause",
            HoldRecipe {
                picture_context: None,
                duration: FrameDuration::new(3).unwrap(),
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
            },
        )
    }

    /// A root of `count` Holds and the patch inserting one more at `slot`.
    fn insertion(count: usize, slot: usize) -> (ProjectDocument, EditTransaction) {
        let mut document = ProjectDocument::new(
            ProjectId::new("wire").unwrap(),
            RevisionId::new("r0").unwrap(),
            PresentationBasis {
                width: 16,
                height: 16,
                frame_rate: FrameRate::new(24, 1).unwrap(),
                color_policy: ColorPolicy::SdrRec709,
            },
            id("root"),
        )
        .unwrap();
        let children: Vec<NodeId> = (0..count)
            .map(|index| id(&format!("h{index:05}")))
            .collect();
        for child in &children {
            document.nodes.insert(child.clone(), hold());
        }
        document
            .nodes
            .insert(id("root"), BeatNode::sequence("Root", children));
        let edit = crate::apply(
            &document,
            &crate::CommandRequest {
                project_id: document.project_id().clone(),
                expected_revision: document.revision_id().clone(),
                new_revision: RevisionId::new("r1").unwrap(),
                command: crate::Command::Insert {
                    parent: id("root"),
                    index: slot,
                    subtree: crate::Subtree {
                        root: id("new"),
                        nodes: BTreeMap::from([(id("new"), hold())]),
                        overrides: BTreeMap::new(),
                        gap_overrides: BTreeMap::new(),
                    },
                },
            },
        )
        .unwrap();
        (document, edit)
    }

    /// The complete form the derive used to write.
    fn complete(edit: &EditTransaction) -> serde_json::Value {
        let mut value = serde_json::to_value(edit).unwrap();
        for (name, patch) in [("forward", &edit.forward), ("inverse", &edit.inverse)] {
            let nodes: serde_json::Map<_, _> = patch
                .nodes
                .iter()
                .map(|(id, change)| {
                    (
                        id.as_str().to_owned(),
                        serde_json::to_value(change).unwrap(),
                    )
                })
                .collect();
            value[name] = serde_json::to_value(patch).unwrap();
            value[name]["nodes"] = serde_json::Value::Object(nodes);
        }
        value
    }

    #[test]
    fn compact_and_complete_forms_decode_to_the_same_transaction() {
        for (count, slot) in [(3, 1), (40, 0), (40, 17), (40, 40), (500, 250)] {
            let (_, edit) = insertion(count, slot);
            let compact = serde_json::to_string(&edit).unwrap();
            assert_eq!(
                serde_json::from_str::<EditTransaction>(&compact).unwrap(),
                edit
            );
            let complete = complete(&edit).to_string();
            assert_eq!(
                serde_json::from_str::<EditTransaction>(&complete).unwrap(),
                edit
            );
            let spliced = count + 1 >= MIN_SPLICED_CHILDREN;
            assert_eq!(compact.contains("after_children"), spliced, "{count}");
            assert!(!compact.contains("\"inverse\""));
            assert!(compact.len() < complete.len(), "{count}");
            if count >= 500 {
                // About one of the four child lists remains.
                assert!(compact.len() * 3 < complete.len(), "{count}");
            }
            // The patch alone, as revision storage writes it.
            let forward = serde_json::to_string(&edit.forward).unwrap();
            assert_eq!(
                serde_json::from_str::<DocumentPatch>(&forward).unwrap(),
                edit.forward
            );
        }
    }

    #[test]
    fn an_inverse_other_than_the_reverse_is_kept() {
        let (_, mut edit) = insertion(40, 3);
        edit.inverse.to_revision = RevisionId::new("elsewhere").unwrap();
        let json = serde_json::to_string(&edit).unwrap();
        assert!(json.contains("\"inverse\""));
        assert_eq!(
            serde_json::from_str::<EditTransaction>(&json).unwrap(),
            edit
        );
    }

    #[test]
    fn malformed_splices_are_refused() {
        let (_, edit) = insertion(40, 3);
        let value = serde_json::to_value(&edit).unwrap();
        let change = &value["forward"]["nodes"]["root"];
        for (field, bad) in [
            ("at", serde_json::json!(41)),
            ("remove", serde_json::json!(41)),
            ("at", serde_json::json!(usize::MAX)),
        ] {
            let mut broken = change.clone();
            broken["after_children"][field] = bad;
            let mut whole = value.clone();
            whole["forward"]["nodes"]["root"] = broken;
            assert!(serde_json::from_value::<EditTransaction>(whole).is_err());
        }
        let mut both = value.clone();
        both["forward"]["nodes"]["root"]["after"] =
            serde_json::to_value(edit.forward.nodes[&id("root")].after.clone()).unwrap();
        assert!(serde_json::from_value::<EditTransaction>(both).is_err());
        // A splice has one encoding: an explicit null `after` is refused too.
        let mut null = value.clone();
        null["forward"]["nodes"]["root"]["after"] = serde_json::Value::Null;
        assert!(serde_json::from_value::<EditTransaction>(null).is_err());
        let mut leaf = value.clone();
        leaf["forward"]["nodes"]["root"]["before"] = serde_json::to_value(hold()).unwrap();
        assert!(serde_json::from_value::<EditTransaction>(leaf).is_err());
    }
}
