//! Frozen schema-3 document/history wire adapter. Only full chronological replay
//! may use this module. Normal document ingress accepts the current schema only.
//! Beat, asset, and mark wires are unchanged by schema 4. Document, subtree,
//! command, and patch wires explicitly exclude every play-override field.

use crate::document::unique_map;
use crate::legacy_mark::{LegacyMark, project_mark_changes, project_marks, upgrade_marks};
use crate::legacy_v4::{LegacyAssetRecord, LegacyBeatNode, LegacyHoldRecipe, LegacyHoldVideo};
use crate::*;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    schema_version: u32,
    project_id: ProjectId,
    revision_id: RevisionId,
    presentation_basis: PresentationBasis,
    root: NodeId,
    #[serde(deserialize_with = "unique_map")]
    nodes: BTreeMap<NodeId, LegacyBeatNode>,
    #[serde(deserialize_with = "unique_map")]
    assets: BTreeMap<AssetId, LegacyAssetRecord>,
    #[serde(deserialize_with = "unique_map")]
    marks: BTreeMap<MarkId, LegacyMark>,
}
impl Document {
    pub fn from_json(json: &str) -> Result<Self, DocumentError> {
        let old: Self = parse(json)?;
        if old.schema_version != 3 {
            return Err(invalid("migration requires document schema 3"));
        }
        old.clone().upgrade()?;
        Ok(old)
    }
    pub fn revision_id(&self) -> &RevisionId {
        &self.revision_id
    }
    pub fn upgrade(self) -> Result<ProjectDocument, DocumentError> {
        let document = ProjectDocument {
            audio_lineage: BTreeMap::new(),
            schema_version: DOCUMENT_SCHEMA_VERSION,
            project_id: self.project_id,
            revision_id: self.revision_id,
            presentation_basis: self.presentation_basis,
            basis_state: BasisState::explicit(),
            root: self.root,
            nodes: self
                .nodes
                .into_iter()
                .map(|(id, node)| (id, node.upgrade()))
                .collect(),
            assets: self
                .assets
                .into_iter()
                .map(|(id, asset)| (id, asset.upgrade()))
                .collect(),
            marks: upgrade_marks(self.marks),
            overrides: BTreeMap::new(),
        };
        document.validate()?;
        Ok(document)
    }
    pub fn matches(&self, document: &ProjectDocument) -> bool {
        let Some(marks) = project_marks(&document.marks) else {
            return false;
        };
        if document.basis_state != BasisState::explicit() {
            return false;
        }
        let projected = document
            .nodes
            .iter()
            .map(|(id, node)| Some((id.clone(), LegacyBeatNode::project(node)?)))
            .collect::<Option<BTreeMap<_, _>>>()
            .zip(
                document
                    .assets
                    .iter()
                    .map(|(id, asset)| Some((id.clone(), LegacyAssetRecord::project(asset)?)))
                    .collect::<Option<BTreeMap<_, _>>>(),
            );
        document.overrides.is_empty()
            && projected.is_some_and(|(nodes, assets)| {
                self == &Self {
                    schema_version: 3,
                    project_id: document.project_id.clone(),
                    revision_id: document.revision_id.clone(),
                    presentation_basis: document.presentation_basis.clone(),
                    root: document.root.clone(),
                    nodes,
                    assets,
                    marks,
                }
            })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OldSubtree {
    root: NodeId,
    #[serde(deserialize_with = "unique_map")]
    nodes: BTreeMap<NodeId, LegacyBeatNode>,
}

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
        #[serde(default)]
        anchor_policy: WrapAnchorPolicy,
    },
    SetRepeat {
        node: NodeId,
        plays: u32,
        gap: Option<LegacyHoldRecipe>,
    },
    InsertPlays {
        node: NodeId,
        index: u32,
        count: u32,
    },
    MovePlays {
        node: NodeId,
        start: u32,
        end: u32,
        destination: u32,
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
    SetMark {
        id: MarkId,
        owner: NodeId,
        label: String,
        boundary: BoundaryAnchor,
        loss_policy: AnchorLossPolicy,
    },
    DeleteMark {
        id: MarkId,
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
                    .map(|(id, node)| (id, node.upgrade()))
                    .collect(),
                overrides: BTreeMap::new(),
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
            anchor_policy,
        } => Command::WrapRepeat {
            node,
            id,
            plays,
            gap: gap.map(LegacyHoldRecipe::upgrade),
            anchor_policy,
        },
        OldCommand::SetRepeat { node, plays, gap } => Command::SetRepeat {
            node,
            plays,
            gap: gap.map(LegacyHoldRecipe::upgrade),
        },
        OldCommand::InsertPlays { node, index, count } => {
            Command::InsertPlays { node, index, count }
        }
        OldCommand::MovePlays {
            node,
            start,
            end,
            destination,
        } => Command::MovePlays {
            node,
            start,
            end,
            destination,
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
        OldCommand::SetMark {
            id,
            owner,
            label,
            boundary,
            loss_policy,
        } => Command::SetMark {
            id,
            owner,
            label,
            boundary,
            loss_policy,
        },
        OldCommand::DeleteMark { id } => Command::DeleteMark { id },
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
    nodes: BTreeMap<NodeId, ValueChange<LegacyBeatNode>>,
    #[serde(deserialize_with = "unique_map")]
    assets: BTreeMap<AssetId, ValueChange<LegacyAssetRecord>>,
    #[serde(deserialize_with = "unique_map")]
    marks: BTreeMap<MarkId, ValueChange<LegacyMark>>,
}
impl Patch {
    fn project(patch: &DocumentPatch) -> Option<Self> {
        if patch.presentation.is_some() {
            return None;
        }
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
                                Some(node) => Some(LegacyBeatNode::project(node)?),
                                None => None,
                            },
                            after: match &change.after {
                                Some(node) => Some(LegacyBeatNode::project(node)?),
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
            marks: project_mark_changes(&patch.marks)?,
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
    // New lineage-only changes do not alter the legacy changed-node summary.
    // Compare the complete summary derived from this frozen patch vocabulary.
    let changed_ids = forward
        .nodes
        .keys()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    Ok(edit.forward.overrides.is_empty()
        && edit.inverse.overrides.is_empty()
        && old
            == Edit {
                forward,
                inverse,
                changed_ids,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_document_projection_rejects_even_empty_override_entries() {
        let mut current = ProjectDocument::new(
            ProjectId::new("project").unwrap(),
            RevisionId::new("initial").unwrap(),
            PresentationBasis {
                width: 1,
                height: 1,
                frame_rate: FrameRate::new(30, 1).unwrap(),
                color_policy: ColorPolicy::SdrRec709,
            },
            NodeId::new("root").unwrap(),
        )
        .unwrap();
        let mut wire = serde_json::to_value(&current).unwrap();
        wire["nodes"]["root"]
            .as_object_mut()
            .unwrap()
            .remove("audio_edges");
        wire.as_object_mut().unwrap().remove("basis_state");
        wire.as_object_mut().unwrap().remove("overrides");
        wire["schema_version"] = serde_json::json!(3);
        let v3 = Document::from_json(&wire.to_string()).unwrap();
        wire.as_object_mut().unwrap().remove("marks");
        wire["schema_version"] = serde_json::json!(2);
        let v2 = legacy_v2::Document::from_json(&wire.to_string()).unwrap();
        wire["schema_version"] = serde_json::json!(1);
        let v1 = legacy_v1::Document::from_json(&wire.to_string()).unwrap();
        assert!(v1.matches(&current) && v2.matches(&current) && v3.matches(&current));
        // Model faulty replay without allowing projection to hide a new field.
        current
            .overrides
            .insert(current.root.clone(), PlayOverrides::default());
        assert!(!v1.matches(&current) && !v2.matches(&current) && !v3.matches(&current));
    }
}
