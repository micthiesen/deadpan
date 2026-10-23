//! Keep standalone audio-context codecs' explicit edge objects unchanged.
use super::*;
use serde::ser::SerializeMap;

#[derive(Serialize)]
struct Node<'a> {
    duration: &'a FrameDuration,
    edges: &'a AudioEdgePolicies,
    #[serde(skip_serializing_if = "crate::AudioEditorialEdges::is_empty")]
    editorial_edges: &'a crate::AudioEditorialEdges,
    kind: &'a FrozenAudioKind,
}

struct Nodes<'a>(&'a BTreeMap<NodeId, FrozenAudioNode>);

impl Serialize for Nodes<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut output = serializer.serialize_map(Some(self.0.len()))?;
        for (id, node) in self.0 {
            let FrozenAudioNode {
                duration,
                edges,
                editorial_edges,
                kind,
            } = node;
            output.serialize_entry(
                id,
                &Node {
                    duration,
                    edges,
                    editorial_edges,
                    kind,
                },
            )?;
        }
        output.end()
    }
}

#[derive(Serialize)]
struct Layout<'a> {
    root: &'a NodeId,
    rate: &'a FrameRate,
    nodes: Nodes<'a>,
    overrides: &'a BTreeMap<NodeId, PlayOverrides>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    gap_overrides: &'a BTreeMap<NodeId, PlayOverrides>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    audio_lineage: &'a BTreeMap<NodeId, AudioLineageId>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    derived_clocks: &'a BTreeMap<NodeId, FrozenRetimeClock>,
}

impl FrozenAudioLayout {
    pub(crate) fn serialize_explicit_edges<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        if self.inner.complete.is_some() {
            return Err(serde::ser::Error::custom(
                "a provisional frozen audio layout cannot be serialized",
            ));
        }
        // Exhaustive patterns make future persisted fields explicit in both
        // codecs. Borrowed wrappers stream into the caller's bounded writer.
        let FrozenAudioLayoutData {
            root,
            rate,
            nodes,
            overrides,
            gap_overrides,
            audio_lineage,
            derived_clocks,
            index: _,
            wire_bytes: _,
            complete: _,
        } = self.inner.as_ref();
        Layout {
            root,
            rate,
            nodes: Nodes(nodes),
            overrides,
            gap_overrides,
            audio_lineage,
            derived_clocks,
        }
        .serialize(serializer)
    }
}
