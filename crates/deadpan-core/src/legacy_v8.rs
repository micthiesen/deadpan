//! Frozen schema-8 document and history adapter. Source qualification bindings
//! and atomic source imports cannot enter pre-schema-9 history.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::document::unique_map;
use crate::legacy_asset::{Asset, project_assets, project_changes, upgrade_assets};
use crate::legacy_mark::{LegacyMark, project_mark_changes, project_marks, upgrade_marks};
use crate::*;
use crate::{SourceAudioMapping as AudioMapping, SourceVideoMapping as VideoMapping};

/// Schema-8 source wire preserves independent exact placements.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacySourceNode {
    duration: FrameDuration,
    video: SourceVideo,
    audio: Option<SourceAudio>,
    link: LinkRelation,
    audio_offset: AudioSample,
    audio_mapping: AudioMapping,
    video_mapping: VideoMapping,
}

impl LegacySourceNode {
    pub(crate) fn upgrade(self) -> SourceNode {
        SourceNode {
            duration: self.duration,
            video: self.video,
            audio: self.audio,
            link: self.link,
            audio_offset: self.audio_offset,
            audio_mapping: self.audio_mapping,
            video_mapping: self.video_mapping,
        }
    }

    pub(crate) fn project(source: &SourceNode) -> Option<Self> {
        Some(Self {
            duration: source.duration,
            video: source.video.clone(),
            audio: source.audio.clone(),
            link: source.link,
            audio_offset: source.audio_offset,
            audio_mapping: source.audio_mapping,
            video_mapping: source.video_mapping,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum LegacyNodeKind {
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
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyBeatNode {
    pub(crate) label: String,
    pub(crate) kind: LegacyNodeKind,
}

impl LegacyBeatNode {
    pub(crate) fn upgrade(self) -> BeatNode {
        BeatNode {
            framing: None,
            label: self.label,
            audio_edges: AudioEdgePolicies::default(),
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
                } => NodeKind::Retime {
                    child,
                    duration,
                    mapping,
                    pitch,
                    purpose: RetimePurpose::Edit,
                },
            },
        }
    }

    pub(crate) fn project(value: &BeatNode) -> Option<Self> {
        if value.framing.is_some() {
            return None;
        }
        if value.audio_edges != AudioEdgePolicies::default() {
            return None;
        }
        Some(Self {
            label: value.label.clone(),
            kind: match &value.kind {
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
                    purpose: RetimePurpose::Partition,
                    ..
                } => return None,
                NodeKind::Retime {
                    child,
                    duration,
                    mapping,
                    pitch,
                    purpose: RetimePurpose::Edit,
                } => LegacyNodeKind::Retime {
                    child: child.clone(),
                    duration: *duration,
                    mapping: *mapping,
                    pitch: *pitch,
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
    root: NodeId,
    #[serde(deserialize_with = "unique_map")]
    nodes: BTreeMap<NodeId, LegacyBeatNode>,
    #[serde(deserialize_with = "unique_map")]
    assets: BTreeMap<AssetId, Asset>,
    #[serde(deserialize_with = "unique_map")]
    marks: BTreeMap<MarkId, LegacyMark>,
    #[serde(deserialize_with = "unique_map")]
    overrides: BTreeMap<NodeId, PlayOverrides>,
}

impl Document {
    pub fn from_json(json: &str) -> Result<Self, DocumentError> {
        let old: Self = parse(json)?;
        if old.schema_version != 8 {
            return Err(invalid("migration requires document schema 8"));
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
            basis_state: BasisState::explicit(),
            root: self.root,
            nodes: self
                .nodes
                .into_iter()
                .map(|(id, node)| (id, node.upgrade()))
                .collect(),
            assets: upgrade_assets(self.assets),
            marks: upgrade_marks(self.marks),
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
        let Some(assets) = project_assets(&document.assets) else {
            return false;
        };
        self == &Self {
            schema_version: 8,
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
        assets: BTreeMap<AssetId, Asset>,
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
            Self::AcceptGeneratedHold { artifact, assets } => OccurrenceEdit::AcceptGeneratedHold {
                artifact,
                assets: upgrade_assets(assets),
            },
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
        assets: BTreeMap<AssetId, Asset>,
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
        asset: Asset,
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
            assets: upgrade_assets(assets),
        },
        OldCommand::RevertGeneratedHold { node } => Command::RevertGeneratedHold { node },
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
    assets: BTreeMap<AssetId, ValueChange<Asset>>,
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
            assets: project_changes(&patch.assets)?,
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
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn document(version: u32) -> Value {
        let span = json!({
            "start": {"ticks": -24000, "time_base": {"numerator": 1, "denominator": 48000}},
            "end": {"ticks": 72000, "time_base": {"numerator": 1, "denominator": 48000}}
        });
        let mut value = json!({
            "schema_version": version, "project_id": "legacy", "revision_id": "initial",
            "presentation_basis": {"width": 16, "height": 16, "frame_rate": {"numerator": 30000, "denominator": 1001}, "color_policy": "sdr_rec709"},
            "root": "root", "nodes": {
                "root": {"label": "Root", "kind": {"type": "sequence", "children": ["source"]}},
                "source": {"label": "Original A/V", "kind": {"type": "source", "source": {
                    "duration": 60, "video": {"type": "stream", "asset": "asset", "span": span},
                    "audio": {"asset": "asset", "span": span},
                    "link": "independent", "audio_offset": -137
                }}}
            }, "assets": {"asset": {"label": "A/V", "content_hash": "a".repeat(64), "video": span, "audio": span, "still_image": false, "frame_count": 60}}
        });
        if version >= 3 {
            value["marks"] = json!({});
        }
        if version >= 4 {
            value["overrides"] = json!({});
        }
        if version >= 6 {
            value["nodes"]["source"]["kind"]["source"]["audio_mapping"] =
                json!({"type":"duration", "frames":{"numerator":"60000","denominator":"1001"}});
        }
        if version >= 7 {
            value["nodes"]["source"]["kind"]["source"]["video_mapping"] = json!({"type":"duration", "frames":{"numerator":"55", "denominator":"1"}, "endpoints":"hold_adjacent"});
        }
        if version == 8 {
            value["nodes"]["source"]["kind"]["source"]["audio_mapping"] = json!({"type":"placement", "start":{"numerator":"-1","denominator":"3"}, "frames":{"numerator":"60000","denominator":"1001"}});
            value["nodes"]["source"]["kind"]["source"]["video_mapping"] = json!({"type":"placement", "start":{"numerator":"1","denominator":"2"}, "frames":{"numerator":"59","denominator":"1"}, "endpoints":"hold_adjacent"});
        }
        value
    }

    fn upgrade(version: u32, json: &str) -> Result<ProjectDocument, DocumentError> {
        match version {
            1 => legacy_v1::Document::from_json(json)?.upgrade(),
            2 => legacy_v2::Document::from_json(json)?.upgrade(),
            3 => legacy_v3::Document::from_json(json)?.upgrade(),
            4 => legacy_v4::Document::from_json(json)?.upgrade(),
            5 => legacy_v5::Document::from_json(json)?.upgrade(),
            6 => legacy_v6::Document::from_json(json)?.upgrade(),
            7 => legacy_v7::Document::from_json(json)?.upgrade(),
            8 => Document::from_json(json)?.upgrade(),
            _ => unreachable!(),
        }
    }

    fn matches_document(version: u32, json: &str, current: &ProjectDocument) -> bool {
        match version {
            1 => legacy_v1::Document::from_json(json)
                .unwrap()
                .matches(current),
            2 => legacy_v2::Document::from_json(json)
                .unwrap()
                .matches(current),
            3 => legacy_v3::Document::from_json(json)
                .unwrap()
                .matches(current),
            4 => legacy_v4::Document::from_json(json)
                .unwrap()
                .matches(current),
            5 => legacy_v5::Document::from_json(json)
                .unwrap()
                .matches(current),
            6 => legacy_v6::Document::from_json(json)
                .unwrap()
                .matches(current),
            7 => legacy_v7::Document::from_json(json)
                .unwrap()
                .matches(current),
            8 => Document::from_json(json).unwrap().matches(current),
            _ => unreachable!(),
        }
    }

    type RequestAdapter = fn(&str) -> Result<CommandRequest, DocumentError>;
    const REQUEST_ADAPTERS: [RequestAdapter; 8] = [
        legacy_v1::upgrade_request,
        legacy_v2::upgrade_request,
        legacy_v3::upgrade_request,
        legacy_v4::upgrade_request,
        legacy_v5::upgrade_request,
        legacy_v6::upgrade_request,
        legacy_v7::upgrade_request,
        upgrade_request,
    ];
    type EditAdapter = fn(&str, &EditTransaction) -> Result<bool, DocumentError>;
    const EDIT_ADAPTERS: [EditAdapter; 8] = [
        legacy_v1::matches_edit,
        legacy_v2::matches_edit,
        legacy_v3::matches_edit,
        legacy_v4::matches_edit,
        legacy_v5::matches_edit,
        legacy_v6::matches_edit,
        legacy_v7::matches_edit,
        matches_edit,
    ];

    fn request(command: Value) -> String {
        json!({"project_id":"legacy", "expected_revision":"initial", "new_revision":"changed", "command":command}).to_string()
    }

    #[test]
    fn every_pre_nine_document_and_asset_command_rejects_even_null_qualification() {
        for version in 1..=8 {
            let old = document(version);
            let current = upgrade(version, &old.to_string()).unwrap();
            assert_eq!(current.schema_version(), DOCUMENT_SCHEMA_VERSION);
            assert!(matches_document(version, &old.to_string(), &current));
            assert!(
                current.assets()[&AssetId::new("asset").unwrap()]
                    .source_qualification
                    .is_none()
            );
            let adapter = REQUEST_ADAPTERS[(version - 1) as usize];
            let asset = old["assets"]["asset"].clone();
            let accepted = request(json!({"command":"add_asset", "id":"asset", "asset":asset}));
            assert!(adapter(&accepted).is_ok());
            assert!(adapter(&request(json!({"command":"import_source", "id":"asset", "asset":asset, "insertion":null}))).is_err());
            for qualification in [Value::Null, json!("c".repeat(64))] {
                let mut forged = old.clone();
                forged["assets"]["asset"]["source_qualification"] = qualification.clone();
                assert!(
                    upgrade(version, &forged.to_string()).is_err(),
                    "schema {version}"
                );
                let mut forged_asset = asset.clone();
                forged_asset["source_qualification"] = qualification;
                assert!(
                    adapter(&request(
                        json!({"command":"add_asset", "id":"asset", "asset":forged_asset})
                    ))
                    .is_err(),
                    "schema {version}"
                );
            }
            let mut qualified = serde_json::to_value(&current).unwrap();
            qualified["assets"]["asset"]["source_qualification"] = json!("c".repeat(64));
            let qualified = ProjectDocument::from_json(&qualified.to_string()).unwrap();
            assert!(!matches_document(version, &old.to_string(), &qualified));
        }
    }

    #[test]
    fn pre_nine_patches_reject_qualification_and_cannot_hide_current_bindings() {
        for version in 1..=8 {
            let original = upgrade(version, &document(version).to_string()).unwrap();
            let edit = apply(
                &original,
                &CommandRequest {
                    project_id: original.project_id().clone(),
                    expected_revision: original.revision_id().clone(),
                    new_revision: RevisionId::new("more-media").unwrap(),
                    command: Command::AddAsset {
                        id: AssetId::new("more").unwrap(),
                        asset: original.assets()[&AssetId::new("asset").unwrap()].clone(),
                    },
                },
            )
            .unwrap();
            let mut wire = serde_json::to_value(&edit).unwrap();
            for direction in ["forward", "inverse"] {
                if version < 3 {
                    wire[direction].as_object_mut().unwrap().remove("marks");
                }
                if version < 4 {
                    wire[direction].as_object_mut().unwrap().remove("overrides");
                }
            }
            let adapter = EDIT_ADAPTERS[(version - 1) as usize];
            assert!(
                adapter(&wire.to_string(), &edit).unwrap(),
                "schema {version}"
            );
            for (direction, side) in [("forward", "after"), ("inverse", "before")] {
                for qualification in [Value::Null, json!("c".repeat(64))] {
                    let mut forged = wire.clone();
                    forged[direction]["assets"]["more"][side]["source_qualification"] =
                        qualification;
                    assert!(
                        adapter(&forged.to_string(), &edit).is_err(),
                        "schema {version}"
                    );
                }
            }
            let mut qualified = edit.clone();
            qualified
                .forward
                .assets
                .get_mut(&AssetId::new("more").unwrap())
                .unwrap()
                .after
                .as_mut()
                .unwrap()
                .source_qualification = Some(SourceQualificationId::new("c".repeat(64)).unwrap());
            assert!(!adapter(&wire.to_string(), &qualified).unwrap());
        }
    }

    fn generated_artifact() -> GeneratedArtifact {
        let object = |digit: &str| {
            GeneratedObjectRef::new(GeneratedContentId::new(digit.repeat(64)).unwrap(), 100)
                .unwrap()
        };
        GeneratedArtifact {
            sampled_asset: AssetId::new("sampled").unwrap(),
            sampled_object: object("a"),
            native_asset: AssetId::new("native").unwrap(),
            native_object: object("b"),
            provenance: object("c"),
            sampling: BridgeSamplingMap::new(
                FrameRate::new(30, 1).unwrap(),
                FrameRate::new(25, 1).unwrap(),
                FrameDuration::new(25).unwrap(),
                FrameDuration::new(30).unwrap(),
                BridgeInterpolation::EncodedSrgbRgb8LinearHalfUp,
            )
            .unwrap(),
        }
    }

    #[test]
    fn generated_acceptance_asset_maps_use_frozen_vocabulary_in_commands_and_occurrences() {
        let artifact = generated_artifact();
        for version in 1..=8 {
            let adapter = REQUEST_ADAPTERS[(version - 1) as usize];
            for occurrence in [false, true] {
                let operation = |asset: Value| {
                    let mut body = json!({"artifact":artifact, "assets":{"asset":asset}});
                    if occurrence {
                        body["type"] = json!("accept_generated_hold");
                        json!({"command":"edit_occurrence", "instance":{"node":"hold", "repeats":[]}, "edit":body, "identities":{"nodes":[], "marks":[]}})
                    } else {
                        body["command"] = json!("accept_generated_hold");
                        body["node"] = json!("hold");
                        body
                    }
                };
                let asset = document(version)["assets"]["asset"].clone();
                assert_eq!(
                    adapter(&request(operation(asset.clone()))).is_ok(),
                    version >= 5
                );
                for qualification in [Value::Null, json!("c".repeat(64))] {
                    let mut forged = asset.clone();
                    forged["source_qualification"] = qualification;
                    assert!(
                        adapter(&request(operation(forged))).is_err(),
                        "schema {version}, occurrence {occurrence}"
                    );
                }
            }
        }
    }

    #[test]
    fn schema_eight_retains_placements_offsets_generated_hashes_and_no_receipt() {
        let mut old = document(8);
        old["assets"]["asset"]["content_hash"] = json!(format!("blake3:{}", "a".repeat(64)));
        let current = Document::from_json(&old.to_string())
            .unwrap()
            .upgrade()
            .unwrap();
        assert!(
            Document::from_json(&old.to_string())
                .unwrap()
                .matches(&current)
        );
        assert!(
            current.assets()[&AssetId::new("asset").unwrap()]
                .source_qualification
                .is_none()
        );
        let NodeKind::Source { source } = &current.nodes()[&NodeId::new("source").unwrap()].kind
        else {
            panic!()
        };
        assert_eq!(source.audio_offset, AudioSample(-137));
        assert_eq!(
            source.audio_mapping.start_frames(),
            ExactRatio::new(-1, 3).unwrap()
        );
        assert_eq!(
            source.video_mapping.start_frames(),
            ExactRatio::new(1, 2).unwrap()
        );
        for (mapping, offset) in [
            (
                serde_json::to_value(source.audio_mapping).unwrap(),
                Some(-137),
            ),
            (serde_json::to_value(source.video_mapping).unwrap(), None),
        ] {
            let mut command = json!({"command":if offset.is_some() {"set_source_audio_mapping"} else {"set_source_video_mapping"}, "node":"source", "mapping":mapping});
            if let Some(offset) = offset {
                command["offset"] = json!(offset);
            }
            let wire = request(command);
            assert_eq!(
                upgrade_request(&wire).unwrap(),
                serde_json::from_str::<CommandRequest>(&wire).unwrap()
            );
        }
    }
}
