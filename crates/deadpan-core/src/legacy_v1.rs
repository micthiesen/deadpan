//! Strict schema-1 migration adapter. Normal document ingress never accepts old
//! schemas. Hosts must replay the complete history, not upgrade each snapshot
//! independently: the allocation revision is part of stable occurrence identity.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::document::unique_map;
use crate::legacy_v4::{LegacyAssetRecord, LegacyHoldRecipe, LegacyHoldVideo};
use crate::legacy_v5::LegacySourceNode;
use crate::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum Kind {
    Source {
        source: LegacySourceNode,
    },
    Sequence {
        children: Vec<NodeId>,
    },
    Hold {
        recipe: LegacyHoldRecipe,
    },
    Repeat {
        child: NodeId,
        plays: u32,
        gap: Option<LegacyHoldRecipe>,
    },
    Retime {
        child: NodeId,
        duration: FrameDuration,
        mapping: FrameRange,
        pitch: PitchPolicy,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Beat {
    label: String,
    kind: Kind,
}

impl Beat {
    fn upgrade(self, allocation: &RevisionId) -> Result<BeatNode, DocumentError> {
        Ok(BeatNode {
            label: self.label,
            kind: match self.kind {
                Kind::Source { source } => NodeKind::Source {
                    source: source.upgrade(),
                },
                Kind::Sequence { children } => NodeKind::Sequence { children },
                Kind::Hold { recipe } => NodeKind::Hold {
                    recipe: recipe.upgrade(),
                },
                Kind::Repeat { child, plays, gap } => NodeKind::Repeat {
                    child,
                    iterations: IterationOrder::new(allocation.clone(), plays)?,
                    gap: gap.map(LegacyHoldRecipe::upgrade),
                },
                Kind::Retime {
                    child,
                    duration,
                    mapping,
                    pitch,
                } => NodeKind::Retime {
                    child,
                    duration,
                    mapping,
                    pitch,
                },
            },
        })
    }
    fn project(node: &BeatNode) -> Option<Self> {
        Some(Self {
            label: node.label.clone(),
            kind: match &node.kind {
                NodeKind::Source { source } => Kind::Source {
                    source: LegacySourceNode::project(source)?,
                },
                NodeKind::Sequence { children } => Kind::Sequence {
                    children: children.clone(),
                },
                NodeKind::Hold { recipe } => Kind::Hold {
                    recipe: LegacyHoldRecipe::project(recipe)?,
                },
                NodeKind::Repeat {
                    child,
                    iterations,
                    gap,
                } => Kind::Repeat {
                    child: child.clone(),
                    plays: iterations.len(),
                    gap: match gap {
                        Some(recipe) => Some(LegacyHoldRecipe::project(recipe)?),
                        None => None,
                    },
                },
                NodeKind::Retime {
                    child,
                    duration,
                    mapping,
                    pitch,
                } => Kind::Retime {
                    child: child.clone(),
                    duration: *duration,
                    mapping: *mapping,
                    pitch: *pitch,
                },
            },
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    schema_version: u32,
    project_id: ProjectId,
    revision_id: RevisionId,
    presentation_basis: PresentationBasis,
    root: NodeId,
    #[serde(deserialize_with = "unique_map")]
    nodes: BTreeMap<NodeId, Beat>,
    #[serde(deserialize_with = "unique_map")]
    assets: BTreeMap<AssetId, LegacyAssetRecord>,
}

impl Document {
    pub fn from_json(json: &str) -> Result<Self, DocumentError> {
        let value: Self = parse(json)?;
        if value.schema_version != 1 {
            return Err(invalid("migration requires document schema 1"));
        }
        // Validate the complete tree and timing contract before comparison.
        value.clone().upgrade()?;
        Ok(value)
    }
    pub fn revision_id(&self) -> &RevisionId {
        &self.revision_id
    }
    /// Use only for the initial revision. Subsequent states come from replay.
    pub fn upgrade(self) -> Result<ProjectDocument, DocumentError> {
        let nodes = self
            .nodes
            .into_iter()
            .map(|(id, node)| Ok((id, node.upgrade(&self.revision_id)?)))
            .collect::<Result<_, DocumentError>>()?;
        let document = ProjectDocument {
            schema_version: DOCUMENT_SCHEMA_VERSION,
            project_id: self.project_id,
            revision_id: self.revision_id,
            presentation_basis: self.presentation_basis,
            root: self.root,
            nodes,
            assets: self
                .assets
                .into_iter()
                .map(|(id, asset)| (id, asset.upgrade()))
                .collect(),
            marks: BTreeMap::new(),
            overrides: BTreeMap::new(),
        };
        document.validate()?;
        Ok(document)
    }
    /// Compare every schema-1 field. Only new iteration metadata is projected out.
    pub fn matches(&self, document: &ProjectDocument) -> bool {
        let projected = document
            .nodes
            .iter()
            .map(|(id, node)| Some((id.clone(), Beat::project(node)?)))
            .collect::<Option<BTreeMap<_, _>>>()
            .zip(
                document
                    .assets
                    .iter()
                    .map(|(id, asset)| Some((id.clone(), LegacyAssetRecord::project(asset)?)))
                    .collect::<Option<BTreeMap<_, _>>>(),
            );
        document.marks.is_empty()
            && document.overrides.is_empty()
            && projected.is_some_and(|(nodes, assets)| {
                self == &Self {
                    schema_version: 1,
                    project_id: document.project_id.clone(),
                    revision_id: document.revision_id.clone(),
                    presentation_basis: document.presentation_basis.clone(),
                    root: document.root.clone(),
                    nodes,
                    assets,
                }
            })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OldSubtree {
    root: NodeId,
    #[serde(deserialize_with = "unique_map")]
    nodes: BTreeMap<NodeId, Beat>,
}

// Freeze the old command vocabulary. Unknown/new commands cannot enter a v1
// history merely because today's reducer happens to understand them.
#[derive(Deserialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
enum OldCommand {
    Insert {
        parent: NodeId,
        index: usize,
        subtree: OldSubtree,
    },
    Delete {
        node: NodeId,
    },
    Move {
        node: NodeId,
        parent: NodeId,
        index: usize,
    },
    Group {
        parent: NodeId,
        start: usize,
        end: usize,
        id: NodeId,
        label: String,
    },
    Ungroup {
        node: NodeId,
    },
    WrapRepeat {
        node: NodeId,
        id: NodeId,
        plays: u32,
        gap: Option<LegacyHoldRecipe>,
    },
    SetRepeat {
        node: NodeId,
        plays: u32,
        gap: Option<LegacyHoldRecipe>,
    },
    SetHoldDuration {
        node: NodeId,
        duration: FrameDuration,
    },
    SetHoldProvider {
        node: NodeId,
        video: LegacyHoldVideo,
    },
    Rename {
        node: NodeId,
        label: String,
    },
    AddAsset {
        id: AssetId,
        asset: LegacyAssetRecord,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OldRequest {
    project_id: ProjectId,
    expected_revision: RevisionId,
    new_revision: RevisionId,
    command: OldCommand,
}

pub fn upgrade_request(json: &str) -> Result<CommandRequest, DocumentError> {
    let old: OldRequest = parse(json)?;
    let command = match old.command {
        OldCommand::Insert {
            parent,
            index,
            subtree,
        } => Command::Insert {
            parent,
            index,
            subtree: Subtree {
                root: subtree.root,
                overrides: BTreeMap::new(),
                nodes: subtree
                    .nodes
                    .into_iter()
                    .map(|(id, node)| Ok((id, node.upgrade(&old.new_revision)?)))
                    .collect::<Result<_, DocumentError>>()?,
            },
        },
        OldCommand::Delete { node } => Command::Delete { node },
        OldCommand::Move {
            node,
            parent,
            index,
        } => Command::Move {
            node,
            parent,
            index,
        },
        OldCommand::Group {
            parent,
            start,
            end,
            id,
            label,
        } => Command::Group {
            parent,
            start,
            end,
            id,
            label,
        },
        OldCommand::Ungroup { node } => Command::Ungroup { node },
        OldCommand::WrapRepeat {
            node,
            id,
            plays,
            gap,
        } => Command::WrapRepeat {
            node,
            id,
            plays,
            gap: gap.map(LegacyHoldRecipe::upgrade),
            anchor_policy: WrapAnchorPolicy::First,
        },
        OldCommand::SetRepeat { node, plays, gap } => Command::SetRepeat {
            node,
            plays,
            gap: gap.map(LegacyHoldRecipe::upgrade),
        },
        OldCommand::SetHoldDuration { node, duration } => {
            Command::SetHoldDuration { node, duration }
        }
        OldCommand::SetHoldProvider { node, video } => Command::SetHoldProvider {
            node,
            video: video.upgrade(),
        },
        OldCommand::Rename { node, label } => Command::Rename { node, label },
        OldCommand::AddAsset { id, asset } => Command::AddAsset {
            id,
            asset: asset.upgrade(),
        },
    };
    Ok(CommandRequest {
        project_id: old.project_id,
        expected_revision: old.expected_revision,
        new_revision: old.new_revision,
        command,
    })
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Patch {
    project_id: ProjectId,
    from_revision: RevisionId,
    to_revision: RevisionId,
    #[serde(deserialize_with = "unique_map")]
    nodes: BTreeMap<NodeId, ValueChange<Beat>>,
    #[serde(deserialize_with = "unique_map")]
    assets: BTreeMap<AssetId, ValueChange<LegacyAssetRecord>>,
}

impl Patch {
    fn project(patch: &DocumentPatch) -> Option<Self> {
        Some(Self {
            project_id: patch.project_id.clone(),
            from_revision: patch.from_revision.clone(),
            to_revision: patch.to_revision.clone(),
            nodes: patch
                .nodes
                .iter()
                .map(|(id, change)| {
                    Some((
                        id.clone(),
                        ValueChange {
                            before: match &change.before {
                                Some(node) => Some(Beat::project(node)?),
                                None => None,
                            },
                            after: match &change.after {
                                Some(node) => Some(Beat::project(node)?),
                                None => None,
                            },
                        },
                    ))
                })
                .collect::<Option<_>>()?,
            assets: patch
                .assets
                .iter()
                .map(|(id, change)| Some((id.clone(), project_asset_change(change)?)))
                .collect::<Option<_>>()?,
        })
    }
}

fn project_asset_change(
    change: &ValueChange<AssetRecord>,
) -> Option<ValueChange<LegacyAssetRecord>> {
    Some(ValueChange {
        before: match &change.before {
            Some(asset) => Some(LegacyAssetRecord::project(asset)?),
            None => None,
        },
        after: match &change.after {
            Some(asset) => Some(LegacyAssetRecord::project(asset)?),
            None => None,
        },
    })
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Edit {
    forward: Patch,
    inverse: Patch,
    changed_ids: Vec<NodeId>,
    duration_delta: i64,
    description: String,
}

pub fn matches_edit(json: &str, edit: &EditTransaction) -> Result<bool, DocumentError> {
    let old: Edit = parse(json)?;
    let (Some(forward), Some(inverse)) =
        (Patch::project(&edit.forward), Patch::project(&edit.inverse))
    else {
        return Ok(false);
    };
    Ok(edit.forward.marks.is_empty()
        && edit.inverse.marks.is_empty()
        && edit.forward.overrides.is_empty()
        && edit.inverse.overrides.is_empty()
        && old
            == Edit {
                forward,
                inverse,
                changed_ids: edit.changed_ids.clone(),
                duration_delta: edit.duration_delta,
                description: edit.description.clone(),
            })
}

fn parse<T: DeserializeOwned>(json: &str) -> Result<T, DocumentError> {
    if json.len() > MAX_DOCUMENT_JSON_BYTES {
        return Err(invalid("legacy JSON exceeds 64 MiB"));
    }
    serde_json::from_str(json).map_err(DocumentError::json)
}
fn invalid(message: &str) -> DocumentError {
    DocumentError::new(DocumentErrorCode::InvalidJson, message)
}
