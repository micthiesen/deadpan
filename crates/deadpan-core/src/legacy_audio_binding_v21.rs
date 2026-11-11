//! Closed frozen-layout grammar of core schema 22.
//!
//! Frozen layouts have a separate closed schema; their existing gap geometry
//! is not authored gap-binding intent.

use serde::Deserialize;
use serde::de::IgnoredAny;

/// Check one schema-22 frozen layout before the current bounded decoder.
/// This rejects an explicit `gap_overrides` field, including `{}` or `null`.
pub(crate) fn validate_v22_layout(json: &str) -> Result<(), serde_json::Error> {
    serde_json::from_str::<OldFrozenLayout>(json).map(|_| ())
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
