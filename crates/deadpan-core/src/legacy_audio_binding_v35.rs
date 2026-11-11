//! Closed frozen-source support for audio contexts before core schema 36.
use crate::{FrozenAudioKind, FrozenAudioLayout};

pub(crate) fn supports_layout(layout: &FrozenAudioLayout) -> bool {
    !layout.has_editorial_edges() && layout.nodes().values().all(|node| !matches!(&node.kind,
        FrozenAudioKind::Source { placement: Some(placement) } if placement.start == placement.end
    ))
}
