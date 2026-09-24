//! Frozen schema-13 document and history adapter. Multi-binding marks and
//! transparent partitions are retained; Split commands do not belong to this grammar.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::document::unique_map;
use crate::legacy_mark_v13::{LegacyMark, project_mark_changes, project_marks, upgrade_marks};
use crate::legacy_v8::LegacySourceNode;
use crate::*;
use crate::{SourceAudioMapping as AudioMapping, SourceVideoMapping as VideoMapping};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum LegacyRetimePurpose {
    #[default]
    Edit,
    Partition,
}

impl LegacyRetimePurpose {
    fn is_edit(&self) -> bool {
        matches!(self, Self::Edit)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum LegacyNodeKind {
    Source {
        source: LegacySourceNode,
    },
    Sequence {
        children: Vec<NodeId>,
    },
    Hold {
        recipe: HoldRecipe,
    },
    Repeat {
        child: NodeId,
        iterations: IterationOrder,
        gap: Option<HoldRecipe>,
    },
    Retime {
        child: NodeId,
        duration: FrameDuration,
        mapping: FrameRange,
        pitch: PitchPolicy,
        #[serde(default, skip_serializing_if = "LegacyRetimePurpose::is_edit")]
        purpose: LegacyRetimePurpose,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyBeatNode {
    label: String,
    kind: LegacyNodeKind,
    #[serde(default, skip_serializing_if = "AudioEdgePolicies::is_automatic")]
    audio_edges: AudioEdgePolicies,
}

impl LegacyBeatNode {
    fn upgrade(self) -> BeatNode {
        BeatNode {
            label: self.label,
            audio_edges: self.audio_edges,
            kind: match self.kind {
                LegacyNodeKind::Source { source } => NodeKind::Source {
                    source: source.upgrade(),
                },
                LegacyNodeKind::Sequence { children } => NodeKind::Sequence { children },
                LegacyNodeKind::Hold { recipe } => NodeKind::Hold { recipe },
                LegacyNodeKind::Repeat {
                    child,
                    iterations,
                    gap,
                } => NodeKind::Repeat {
                    child,
                    iterations,
                    gap,
                },
                LegacyNodeKind::Retime {
                    child,
                    duration,
                    mapping,
                    pitch,
                    purpose,
                } => NodeKind::Retime {
                    child,
                    duration,
                    mapping,
                    pitch,
                    purpose: match purpose {
                        LegacyRetimePurpose::Edit => RetimePurpose::Edit,
                        LegacyRetimePurpose::Partition => RetimePurpose::Partition,
                    },
                },
            },
        }
    }

    fn project(node: &BeatNode) -> Option<Self> {
        Some(Self {
            label: node.label.clone(),
            audio_edges: node.audio_edges,
            kind: match &node.kind {
                NodeKind::Source { source } => LegacyNodeKind::Source {
                    source: LegacySourceNode::project(source)?,
                },
                NodeKind::Sequence { children } => LegacyNodeKind::Sequence {
                    children: children.clone(),
                },
                NodeKind::Hold { recipe } => LegacyNodeKind::Hold {
                    recipe: recipe.clone(),
                },
                NodeKind::Repeat {
                    child,
                    iterations,
                    gap,
                } => LegacyNodeKind::Repeat {
                    child: child.clone(),
                    iterations: iterations.clone(),
                    gap: gap.clone(),
                },
                NodeKind::Retime {
                    child,
                    duration,
                    mapping,
                    pitch,
                    purpose,
                } => LegacyNodeKind::Retime {
                    child: child.clone(),
                    duration: *duration,
                    mapping: *mapping,
                    pitch: *pitch,
                    purpose: match purpose {
                        RetimePurpose::Edit => LegacyRetimePurpose::Edit,
                        RetimePurpose::Partition => LegacyRetimePurpose::Partition,
                    },
                },
            },
        })
    }
}

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
    basis_state: BasisState,
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
        if old.schema_version != 13 {
            return Err(invalid("migration requires document schema 13"));
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
            audio_bindings: crate::AudioBindingState::default(),
            schema_version: DOCUMENT_SCHEMA_VERSION,
            project_id: self.project_id,
            revision_id: self.revision_id,
            presentation_basis: self.presentation_basis,
            basis_state: self.basis_state,
            root: self.root,
            nodes: self
                .nodes
                .into_iter()
                .map(|(id, node)| (id, node.upgrade()))
                .collect(),
            assets: self.assets,
            marks: upgrade_marks(self.marks)?,
            overrides: self.overrides,
        };
        document.validate()?;
        Ok(document)
    }

    pub fn matches(&self, document: &ProjectDocument) -> bool {
        if !document.audio_bindings.is_empty() {
            return false;
        }
        let Some(marks) = project_marks(&document.marks) else {
            return false;
        };
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
            schema_version: 13,
            project_id: document.project_id.clone(),
            revision_id: document.revision_id.clone(),
            presentation_basis: document.presentation_basis.clone(),
            basis_state: document.basis_state.clone(),
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
    SetAudioEdge {
        edge: AudioBoundaryKind,
        policy: AudioEdgePolicy,
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
            Self::SetAudioEdge { edge, policy } => OccurrenceEdit::SetAudioEdge { edge, policy },
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
    SetAudioEdge {
        node: NodeId,
        edge: AudioBoundaryKind,
        policy: AudioEdgePolicy,
    },
    AddAsset {
        id: AssetId,
        asset: AssetRecord,
    },
    ImportSource {
        id: AssetId,
        asset: AssetRecord,
        insertion: Option<Box<OldSourceInsertion>>,
        #[serde(default)]
        primary: Option<PrimarySourceImport>,
    },
    SetCanvas {
        width: u32,
        height: u32,
    },
    AdoptPrimaryGeometry {
        width: u32,
        height: u32,
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
        OldCommand::SetAudioEdge { node, edge, policy } => {
            Command::SetAudioEdge { node, edge, policy }
        }
        OldCommand::AddAsset { id, asset } => Command::AddAsset { id, asset },
        OldCommand::ImportSource {
            id,
            asset,
            insertion,
            primary,
        } => Command::ImportSource {
            id,
            asset,
            insertion: insertion.map(|insertion| Box::new(insertion.upgrade())),
            primary,
        },
        OldCommand::SetCanvas { width, height } => Command::SetCanvas { width, height },
        OldCommand::AdoptPrimaryGeometry { width, height } => {
            Command::AdoptPrimaryGeometry { width, height }
        }
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
    #[serde(default)]
    presentation: Option<PresentationChange>,
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
        Some(Self {
            project_id: patch.project_id.clone(),
            from_revision: patch.from_revision.clone(),
            to_revision: patch.to_revision.clone(),
            presentation: patch.presentation.clone(),
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
    if edit.forward.audio_bindings.is_some() || edit.inverse.audio_bindings.is_some() {
        return Ok(false);
    }
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
        .chain(forward.overrides.keys())
        .cloned()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    Ok(old
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
