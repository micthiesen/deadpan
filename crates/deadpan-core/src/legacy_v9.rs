//! Frozen schema-9 document and history adapter. Presentation policy and primary
//! adoption intent cannot enter pre-schema-10 history.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::document::unique_map;
use crate::legacy_mark::{LegacyMark, project_mark_changes, project_marks, upgrade_marks};
use crate::legacy_v8::{LegacyBeatNode, LegacySourceNode};
use crate::*;
use crate::{SourceAudioMapping as AudioMapping, SourceVideoMapping as VideoMapping};

trait TransposeOption<T> {
    fn transpose(self) -> Option<Option<T>>;
}

impl<T> TransposeOption<T> for Option<Option<T>> {
    fn transpose(self) -> Option<Option<T>> {
        match self {
            Some(Some(value)) => Some(Some(value)),
            Some(None) => None,
            None => Some(None),
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
    nodes: BTreeMap<NodeId, LegacyBeatNode>,
    #[serde(deserialize_with = "unique_map")]
    assets: BTreeMap<AssetId, AssetRecord>,
    #[serde(deserialize_with = "unique_map")]
    marks: BTreeMap<MarkId, LegacyMark>,
    #[serde(deserialize_with = "unique_map")]
    overrides: BTreeMap<NodeId, PlayOverrides>,
}

impl Document {
    pub fn from_json(json: &str) -> Result<Self, DocumentError> {
        let old: Self = parse(json)?;
        if old.schema_version != 9 {
            return Err(invalid("migration requires document schema 9"));
        }
        old.clone().upgrade()?;
        Ok(old)
    }

    pub fn revision_id(&self) -> &RevisionId {
        &self.revision_id
    }

    pub fn upgrade(self) -> Result<ProjectDocument, DocumentError> {
        let document = ProjectDocument {
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
            assets: self.assets,
            marks: upgrade_marks(self.marks),
            overrides: self.overrides,
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
        let Some(nodes) = document
            .nodes
            .iter()
            .map(|(id, node)| Some((id.clone(), LegacyBeatNode::project(node)?)))
            .collect::<Option<BTreeMap<_, _>>>()
        else {
            return false;
        };
        let assets = document.assets.clone();
        self == &Self {
            schema_version: 9,
            project_id: document.project_id.clone(),
            revision_id: document.revision_id.clone(),
            presentation_basis: document.presentation_basis.clone(),
            root: document.root.clone(),
            nodes,
            assets,
            marks,
            overrides: document.overrides.clone(),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OldSubtree {
    root: NodeId,
    #[serde(deserialize_with = "unique_map")]
    nodes: BTreeMap<NodeId, LegacyBeatNode>,
    #[serde(default, deserialize_with = "unique_map")]
    overrides: BTreeMap<NodeId, PlayOverrides>,
}

impl OldSubtree {
    fn upgrade(self) -> Subtree {
        Subtree {
            root: self.root,
            nodes: self
                .nodes
                .into_iter()
                .map(|(id, node)| (id, node.upgrade()))
                .collect(),
            overrides: self.overrides,
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum OldOccurrenceEdit {
    Insert {
        index: usize,
        subtree: OldSubtree,
    },
    Delete {},
    Group {
        start: usize,
        end: usize,
        id: NodeId,
        label: String,
    },
    Ungroup {},
    WrapRepeat {
        id: NodeId,
        plays: u32,
        gap: Option<HoldRecipe>,
        #[serde(default)]
        anchor_policy: WrapAnchorPolicy,
    },
    SetRepeat {
        plays: u32,
        gap: Option<HoldRecipe>,
    },
    InsertPlays {
        index: u32,
        count: u32,
    },
    MovePlays {
        start: u32,
        end: u32,
        destination: u32,
    },
    SetSourceVideoMapping {
        mapping: VideoMapping,
    },
    SetSourceAudioMapping {
        mapping: AudioMapping,
        offset: AudioSample,
    },
    SetHoldDuration {
        duration: FrameDuration,
    },
    SetHoldProvider {
        video: HoldVideo,
    },
    AcceptGeneratedHold {
        artifact: GeneratedArtifact,
        #[serde(deserialize_with = "unique_map")]
        assets: BTreeMap<AssetId, AssetRecord>,
    },
    RevertGeneratedHold {},
    Rename {
        label: String,
    },
    SetPlayOverride {
        iteration: IterationId,
        subtree: OldSubtree,
    },
    ClearPlayOverride {
        iteration: IterationId,
    },
}

impl OldOccurrenceEdit {
    fn upgrade(self) -> OccurrenceEdit {
        match self {
            Self::Insert { index, subtree } => OccurrenceEdit::Insert {
                index,
                subtree: subtree.upgrade(),
            },
            Self::Delete {} => OccurrenceEdit::Delete,
            Self::Group {
                start,
                end,
                id,
                label,
            } => OccurrenceEdit::Group {
                start,
                end,
                id,
                label,
            },
            Self::Ungroup {} => OccurrenceEdit::Ungroup,
            Self::WrapRepeat {
                id,
                plays,
                gap,
                anchor_policy,
            } => OccurrenceEdit::WrapRepeat {
                id,
                plays,
                gap,
                anchor_policy,
            },
            Self::SetRepeat { plays, gap } => OccurrenceEdit::SetRepeat { plays, gap },
            Self::InsertPlays { index, count } => OccurrenceEdit::InsertPlays { index, count },
            Self::MovePlays {
                start,
                end,
                destination,
            } => OccurrenceEdit::MovePlays {
                start,
                end,
                destination,
            },
            Self::SetSourceVideoMapping { mapping } => {
                OccurrenceEdit::SetSourceVideoMapping { mapping }
            }
            Self::SetSourceAudioMapping { mapping, offset } => {
                OccurrenceEdit::SetSourceAudioMapping { mapping, offset }
            }
            Self::SetHoldDuration { duration } => OccurrenceEdit::SetHoldDuration { duration },
            Self::SetHoldProvider { video } => OccurrenceEdit::SetHoldProvider { video },
            Self::AcceptGeneratedHold { artifact, assets } => {
                OccurrenceEdit::AcceptGeneratedHold { artifact, assets }
            }
            Self::RevertGeneratedHold {} => OccurrenceEdit::RevertGeneratedHold,
            Self::Rename { label } => OccurrenceEdit::Rename { label },
            Self::SetPlayOverride { iteration, subtree } => OccurrenceEdit::SetPlayOverride {
                iteration,
                subtree: subtree.upgrade(),
            },
            Self::ClearPlayOverride { iteration } => {
                OccurrenceEdit::ClearPlayOverride { iteration }
            }
        }
    }
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
        gap: Option<HoldRecipe>,
        #[serde(default)]
        anchor_policy: WrapAnchorPolicy,
    },
    SetRepeat {
        node: NodeId,
        plays: u32,
        gap: Option<HoldRecipe>,
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
    SetSourceVideoMapping {
        node: NodeId,
        mapping: VideoMapping,
    },
    SetSourceAudioMapping {
        node: NodeId,
        mapping: AudioMapping,
        offset: AudioSample,
    },
    SetHoldDuration {
        node: NodeId,
        duration: FrameDuration,
    },
    SetHoldProvider {
        node: NodeId,
        video: HoldVideo,
    },
    AcceptGeneratedHold {
        node: NodeId,
        artifact: GeneratedArtifact,
        #[serde(deserialize_with = "unique_map")]
        assets: BTreeMap<AssetId, AssetRecord>,
    },
    RevertGeneratedHold {
        node: NodeId,
    },
    Rename {
        node: NodeId,
        label: String,
    },
    AddAsset {
        id: AssetId,
        asset: AssetRecord,
    },
    ImportSource {
        id: AssetId,
        asset: AssetRecord,
        insertion: Option<Box<OldSourceInsertion>>,
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
    SetPlayOverride {
        node: NodeId,
        iteration: IterationId,
        subtree: OldSubtree,
    },
    ClearPlayOverride {
        node: NodeId,
        iteration: IterationId,
    },
    EditOccurrence {
        instance: InstancePath,
        edit: OldOccurrenceEdit,
        identities: OccurrenceIdentities,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OldSourceInsertion {
    parent: NodeId,
    index: usize,
    node: NodeId,
    label: String,
    source: LegacySourceNode,
}

impl OldSourceInsertion {
    fn upgrade(self) -> SourceInsertion {
        SourceInsertion {
            parent: self.parent,
            index: self.index,
            node: self.node,
            label: self.label,
            source: self.source.upgrade(),
        }
    }
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
            subtree: subtree.upgrade(),
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
            gap,
            anchor_policy,
        },
        OldCommand::SetRepeat { node, plays, gap } => Command::SetRepeat { node, plays, gap },
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
        OldCommand::SetSourceVideoMapping { node, mapping } => {
            Command::SetSourceVideoMapping { node, mapping }
        }
        OldCommand::SetSourceAudioMapping {
            node,
            mapping,
            offset,
        } => Command::SetSourceAudioMapping {
            node,
            mapping,
            offset,
        },
        OldCommand::SetHoldDuration { node, duration } => {
            Command::SetHoldDuration { node, duration }
        }
        OldCommand::SetHoldProvider { node, video } => Command::SetHoldProvider { node, video },
        OldCommand::AcceptGeneratedHold {
            node,
            artifact,
            assets,
        } => Command::AcceptGeneratedHold {
            node,
            artifact,
            assets,
        },
        OldCommand::RevertGeneratedHold { node } => Command::RevertGeneratedHold { node },
        OldCommand::Rename { node, label } => Command::Rename { node, label },
        OldCommand::AddAsset { id, asset } => Command::AddAsset { id, asset },
        OldCommand::ImportSource {
            id,
            asset,
            insertion,
        } => Command::ImportSource {
            id,
            asset,
            insertion: insertion.map(|insertion| Box::new(insertion.upgrade())),
            primary: None,
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
        OldCommand::SetPlayOverride {
            node,
            iteration,
            subtree,
        } => Command::SetPlayOverride {
            node,
            iteration,
            subtree: subtree.upgrade(),
        },
        OldCommand::ClearPlayOverride { node, iteration } => {
            Command::ClearPlayOverride { node, iteration }
        }
        OldCommand::EditOccurrence {
            instance,
            edit,
            identities,
        } => Command::EditOccurrence {
            instance,
            edit: edit.upgrade(),
            identities,
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
    nodes: BTreeMap<NodeId, ValueChange<LegacyBeatNode>>,
    #[serde(deserialize_with = "unique_map")]
    assets: BTreeMap<AssetId, ValueChange<AssetRecord>>,
    #[serde(deserialize_with = "unique_map")]
    marks: BTreeMap<MarkId, ValueChange<LegacyMark>>,
    #[serde(deserialize_with = "unique_map")]
    overrides: BTreeMap<NodeId, ValueChange<PlayOverrides>>,
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
                            before: change
                                .before
                                .as_ref()
                                .map(LegacyBeatNode::project)
                                .transpose()?,
                            after: change
                                .after
                                .as_ref()
                                .map(LegacyBeatNode::project)
                                .transpose()?,
                        },
                    ))
                })
                .collect::<Option<_>>()?,
            assets: patch.assets.clone(),
            marks: project_mark_changes(&patch.marks)?,
            overrides: patch.overrides.clone(),
        })
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
    let (Some(forward), Some(inverse)) =
        (Patch::project(&edit.forward), Patch::project(&edit.inverse))
    else {
        return Ok(false);
    };
    Ok(old
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
