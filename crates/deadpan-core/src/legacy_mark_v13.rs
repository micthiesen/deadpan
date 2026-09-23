//! Frozen schema-13 logical mark and physical binding wire vocabulary.
//! Projections pass through these closed types so later fields cannot disappear.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{
    AssetId, DocumentError, ExactRatio, Mark, MarkId, NodeId, ProjectFrame, RevisionId,
    SourceTimestamp, ValueChange,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyMark {
    owner: NodeId,
    label: String,
    boundary: Boundary,
    loss_policy: LossPolicy,
    state: State,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    fragments: Vec<Fragment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Fragment {
    owner: NodeId,
    coordinate: Coordinate,
    state: State,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Boundary {
    coordinate: Coordinate,
    bias: Bias,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Bias {
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum LossPolicy {
    DeleteOwned,
    KeepUnresolved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum State {
    // A struct variant keeps serde's unknown-field check active for Bound too.
    Bound {},
    Unresolved { reason: LossReason },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum LossReason {
    OwnerMissing,
    HostMissing,
    ContentMissing,
    OutsideHost,
    OccurrenceMissing,
    GapMissing,
    OutsideMapping,
    SourceUnavailable,
    OutOfRange,
    WrapAmbiguous,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "space", rename_all = "snake_case", deny_unknown_fields)]
enum Coordinate {
    Source {
        asset: AssetId,
        moment: Moment,
    },
    Local {
        node: NodeId,
        position: ExactRatio,
    },
    Occurrence {
        instance: Instance,
        position: ExactRatio,
    },
    Sequence {
        frame: ProjectFrame,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum Moment {
    Timestamp {
        stream: Stream,
        timestamp: SourceTimestamp,
    },
    AudioSample {
        sample: i64,
        sample_rate: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Stream {
    Video,
    Audio,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Instance {
    node: NodeId,
    repeats: Vec<Repeat>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Repeat {
    node: NodeId,
    iteration: Iteration,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Iteration {
    allocation: RevisionId,
    ordinal: u32,
}

// These maps are bounded by the outer document/history parser and subsequent
// document validation. One conversion retains exact rational and integer values;
// no live Mark is ever deserialized as the legacy admission grammar.
fn convert<T: Serialize, U: DeserializeOwned>(value: T) -> Result<U, DocumentError> {
    let wire = serde_json::to_value(value).map_err(DocumentError::json)?;
    serde_json::from_value(wire).map_err(DocumentError::json)
}

pub(crate) fn upgrade_marks(
    marks: BTreeMap<MarkId, LegacyMark>,
) -> Result<BTreeMap<MarkId, Mark>, DocumentError> {
    convert(marks)
}

pub(crate) fn project_marks(
    marks: &BTreeMap<MarkId, Mark>,
) -> Option<BTreeMap<MarkId, LegacyMark>> {
    convert(marks).ok()
}

pub(crate) fn project_mark_changes(
    changes: &BTreeMap<MarkId, ValueChange<Mark>>,
) -> Option<BTreeMap<MarkId, ValueChange<LegacyMark>>> {
    convert(changes).ok()
}
