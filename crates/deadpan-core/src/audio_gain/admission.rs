//! Gain admission is independent of raw audio lineage and timing bindings.
//! Stream node maps with separate before/after budgets so ingress never retains
//! an unbounded collection before aggregate validation can run.

use std::{collections::BTreeMap, fmt, marker::PhantomData};

use serde::{Deserialize, Deserializer, de};

use crate::{
    BeatNode, Command, DocumentError, DocumentErrorCode, MAX_DOCUMENT_NODES, NodeId,
    OccurrenceEdit, ProjectDocument, ValueChange,
};

use super::{GainError, MAX_GAIN_LAYERS, MAX_GAIN_RECORDS, validate_audio_treatments_with_limit};

// An occurrence edit can retire the copied target after isolating it. The
// public result remains capped at MAX_GAIN_RECORDS; this permits that bounded
// intermediate copy without rejecting a valid replacement or deletion.
pub(crate) const MAX_ISOLATED_GAIN_RECORDS: usize = 2 * MAX_GAIN_RECORDS;

pub(crate) fn invalid(error: GainError) -> DocumentError {
    DocumentError::new(
        if error == GainError::Limit {
            DocumentErrorCode::LimitExceeded
        } else {
            DocumentErrorCode::InvalidTree
        },
        error.to_string(),
    )
}

pub(crate) fn validate_nodes<'a>(
    nodes: impl Iterator<Item = &'a BeatNode>,
) -> Result<usize, DocumentError> {
    validate_nodes_with_limit(nodes, MAX_GAIN_RECORDS)
}

pub(crate) fn validate_nodes_with_limit<'a>(
    nodes: impl Iterator<Item = &'a BeatNode>,
    limit: usize,
) -> Result<usize, DocumentError> {
    validate_audio_treatments_with_limit(nodes.map(|node| &node.audio_treatments), limit)
        .map_err(invalid)
}

pub(crate) fn validate_document(
    document: &ProjectDocument,
    limit: usize,
) -> Result<(), DocumentError> {
    if validate_nodes_with_limit(document.nodes().values(), limit)? == 0 {
        return Ok(());
    }
    // Structural validation already established a bounded, uniquely owned tree.
    let mut pending = vec![(document.root(), 0usize)];
    while let Some((id, layers)) = pending.pop() {
        let layers = layers + usize::from(!document.nodes()[id].audio_treatments.is_empty());
        if layers > MAX_GAIN_LAYERS {
            return Err(invalid(GainError::Limit));
        }
        pending.extend(document.children(id).map(|child| (child, layers)));
    }
    Ok(())
}

pub(crate) fn validate_command(command: &Command) -> Result<(), DocumentError> {
    match command {
        Command::SetAudioTreatments { treatments, .. } => treatments.validate().map_err(invalid),
        Command::Insert { subtree, .. }
        | Command::SetPlayOverride { subtree, .. }
        | Command::SetGapOverride { subtree, .. } => validate_subtree(subtree),
        Command::EditOccurrence { edit, .. } => match edit {
            OccurrenceEdit::SetAudioTreatments { treatments } => {
                treatments.validate().map_err(invalid)
            }
            OccurrenceEdit::Insert { subtree, .. }
            | OccurrenceEdit::SetPlayOverride { subtree, .. }
            | OccurrenceEdit::SetGapOverride { subtree, .. } => validate_subtree(subtree),
            _ => Ok(()),
        },
        _ => Ok(()),
    }
}

fn validate_subtree(subtree: &crate::Subtree) -> Result<(), DocumentError> {
    if subtree.nodes.len() > MAX_DOCUMENT_NODES {
        return Err(invalid(GainError::Limit));
    }
    validate_nodes(subtree.nodes.values()).map(|_| ())
}

pub(crate) trait NodeGainInventory {
    fn gains(&self) -> [Option<&super::AudioTreatments>; 2];
}

impl NodeGainInventory for BeatNode {
    fn gains(&self) -> [Option<&super::AudioTreatments>; 2] {
        [None, Some(&self.audio_treatments)]
    }
}

impl NodeGainInventory for ValueChange<BeatNode> {
    fn gains(&self) -> [Option<&super::AudioTreatments>; 2] {
        [
            self.before.as_ref().map(|node| &node.audio_treatments),
            self.after.as_ref().map(|node| &node.audio_treatments),
        ]
    }
}

pub(crate) fn node_map<'de, D, V>(decoder: D) -> Result<BTreeMap<NodeId, V>, D::Error>
where
    D: Deserializer<'de>,
    V: Deserialize<'de> + NodeGainInventory,
{
    struct Visitor<V>(PhantomData<V>);
    impl<'de, V: Deserialize<'de> + NodeGainInventory> de::Visitor<'de> for Visitor<V> {
        type Value = BTreeMap<NodeId, V>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("unique node identities with bounded audio treatment records")
        }
        fn visit_map<A: de::MapAccess<'de>>(self, mut access: A) -> Result<Self::Value, A::Error> {
            let mut nodes = BTreeMap::new();
            let mut counts = [0usize; 2];
            while let Some(id) = access.next_key::<NodeId>()? {
                if nodes.len() == MAX_DOCUMENT_NODES || nodes.contains_key(&id) {
                    return Err(de::Error::custom("excess or duplicate node identities"));
                }
                let node = access.next_value::<V>()?;
                for (count, gain) in counts.iter_mut().zip(node.gains()) {
                    if let Some(gain) = gain {
                        gain.validate().map_err(de::Error::custom)?;
                        *count = count.checked_add(gain.record_count()).ok_or_else(|| {
                            de::Error::custom("aggregate gain record count overflow")
                        })?;
                        if *count > MAX_GAIN_RECORDS {
                            return Err(de::Error::custom("aggregate gain record limit exceeded"));
                        }
                    }
                }
                nodes.insert(id, node);
            }
            Ok(nodes)
        }
    }
    decoder.deserialize_map(Visitor(PhantomData))
}
