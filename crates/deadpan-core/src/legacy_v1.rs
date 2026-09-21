//! Strict schema-1 migration adapter. Normal document ingress never accepts old
//! schemas. Hosts must replay the complete history, not upgrade each snapshot
//! independently: the allocation revision is part of stable occurrence identity.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::document::unique_map;
use crate::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum Kind {
    Source {
        source: SourceNode,
    },
    Sequence {
        children: Vec<NodeId>,
    },
    Hold {
        recipe: HoldRecipe,
    },
    Repeat {
        child: NodeId,
        plays: u32,
        gap: Option<HoldRecipe>,
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
                Kind::Source { source } => NodeKind::Source { source },
                Kind::Sequence { children } => NodeKind::Sequence { children },
                Kind::Hold { recipe } => NodeKind::Hold { recipe },
                Kind::Repeat { child, plays, gap } => NodeKind::Repeat {
                    child,
                    iterations: IterationOrder::new(allocation.clone(), plays)?,
                    gap,
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
    fn project(node: &BeatNode) -> Self {
        Self {
            label: node.label.clone(),
            kind: match &node.kind {
                NodeKind::Source { source } => Kind::Source {
                    source: source.clone(),
                },
                NodeKind::Sequence { children } => Kind::Sequence {
                    children: children.clone(),
                },
                NodeKind::Hold { recipe } => Kind::Hold {
                    recipe: recipe.clone(),
                },
                NodeKind::Repeat {
                    child,
                    iterations,
                    gap,
                } => Kind::Repeat {
                    child: child.clone(),
                    plays: iterations.len(),
                    gap: gap.clone(),
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
        }
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
    assets: BTreeMap<AssetId, AssetRecord>,
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
            assets: self.assets,
        };
        document.validate()?;
        Ok(document)
    }
    /// Compare every schema-1 field. Only new iteration metadata is projected out.
    pub fn matches(&self, document: &ProjectDocument) -> bool {
        self == &Self {
            schema_version: 1,
            project_id: document.project_id.clone(),
            revision_id: document.revision_id.clone(),
            presentation_basis: document.presentation_basis.clone(),
            root: document.root.clone(),
            nodes: document
                .nodes
                .iter()
                .map(|(id, node)| (id.clone(), Beat::project(node)))
                .collect(),
            assets: document.assets.clone(),
        }
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
        gap: Option<HoldRecipe>,
    },
    SetRepeat {
        node: NodeId,
        plays: u32,
        gap: Option<HoldRecipe>,
    },
    SetHoldDuration {
        node: NodeId,
        duration: FrameDuration,
    },
    SetHoldProvider {
        node: NodeId,
        video: HoldVideo,
    },
    Rename {
        node: NodeId,
        label: String,
    },
    AddAsset {
        id: AssetId,
        asset: AssetRecord,
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
            gap,
        },
        OldCommand::SetRepeat { node, plays, gap } => Command::SetRepeat { node, plays, gap },
        OldCommand::SetHoldDuration { node, duration } => {
            Command::SetHoldDuration { node, duration }
        }
        OldCommand::SetHoldProvider { node, video } => Command::SetHoldProvider { node, video },
        OldCommand::Rename { node, label } => Command::Rename { node, label },
        OldCommand::AddAsset { id, asset } => Command::AddAsset { id, asset },
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
    assets: BTreeMap<AssetId, ValueChange<AssetRecord>>,
}

impl Patch {
    fn project(patch: &DocumentPatch) -> Self {
        Self {
            project_id: patch.project_id.clone(),
            from_revision: patch.from_revision.clone(),
            to_revision: patch.to_revision.clone(),
            nodes: patch
                .nodes
                .iter()
                .map(|(id, change)| {
                    (
                        id.clone(),
                        ValueChange {
                            before: change.before.as_ref().map(Beat::project),
                            after: change.after.as_ref().map(Beat::project),
                        },
                    )
                })
                .collect(),
            assets: patch.assets.clone(),
        }
    }
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
    Ok(old
        == Edit {
            forward: Patch::project(&edit.forward),
            inverse: Patch::project(&edit.inverse),
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
