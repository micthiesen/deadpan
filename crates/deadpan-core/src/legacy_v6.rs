//! Frozen schema-6 document and history adapter. Independent source video mappings
//! and their editing commands must never enter old history through current serde.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::document::unique_map;
use crate::legacy_asset::{Asset, project_assets, project_changes, upgrade_assets};
use crate::legacy_mark::{LegacyMark, project_mark_changes, project_marks, upgrade_marks};
use crate::legacy_source_mapping::AudioMapping;
use crate::*;

/// Schema-6 source wire retains its required audio mapping. Even a null
/// `video_mapping` is unknown vocabulary and cannot be migrated as legacy data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacySourceNode {
    duration: FrameDuration,
    video: SourceVideo,
    audio: Option<SourceAudio>,
    link: LinkRelation,
    audio_offset: AudioSample,
    audio_mapping: AudioMapping,
}

impl LegacySourceNode {
    pub(crate) fn upgrade(self) -> SourceNode {
        SourceNode {
            duration: self.duration,
            video: self.video,
            audio: self.audio,
            link: self.link,
            audio_offset: self.audio_offset,
            audio_mapping: self.audio_mapping.upgrade(),
            video_mapping: SourceVideoMapping::FitBeat,
        }
    }

    pub(crate) fn project(source: &SourceNode) -> Option<Self> {
        if source.video_mapping != SourceVideoMapping::FitBeat {
            return None;
        }
        Some(Self {
            duration: source.duration,
            video: source.video.clone(),
            audio: source.audio.clone(),
            link: source.link,
            audio_offset: source.audio_offset,
            audio_mapping: AudioMapping::project(source.audio_mapping)?,
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
        if old.schema_version != 6 {
            return Err(invalid("migration requires document schema 6"));
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
            assets: upgrade_assets(self.assets),
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
        let Some(assets) = project_assets(&document.assets) else {
            return false;
        };
        self == &Self {
            schema_version: 6,
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
    #[serde(deserialize_with = "unique_map")]
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
            Self::SetSourceAudioMapping { mapping, offset } => {
                OccurrenceEdit::SetSourceAudioMapping {
                    mapping: mapping.upgrade(),
                    offset,
                }
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
        OldCommand::SetSourceAudioMapping {
            node,
            mapping,
            offset,
        } => Command::SetSourceAudioMapping {
            node,
            mapping: mapping.upgrade(),
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
        if version == 6 {
            value["nodes"]["source"]["kind"]["source"]["audio_mapping"] =
                json!({"type":"duration", "frames":{"numerator":"60000","denominator":"1001"}});
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
            6 => Document::from_json(json)?.upgrade(),
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
            6 => Document::from_json(json).unwrap().matches(current),
            _ => unreachable!(),
        }
    }

    type RequestAdapter = fn(&str) -> Result<CommandRequest, DocumentError>;
    const REQUEST_ADAPTERS: [RequestAdapter; 6] = [
        legacy_v1::upgrade_request,
        legacy_v2::upgrade_request,
        legacy_v3::upgrade_request,
        legacy_v4::upgrade_request,
        legacy_v5::upgrade_request,
        upgrade_request,
    ];
    type EditAdapter = fn(&str, &EditTransaction) -> Result<bool, DocumentError>;
    const EDIT_ADAPTERS: [EditAdapter; 6] = [
        legacy_v1::matches_edit,
        legacy_v2::matches_edit,
        legacy_v3::matches_edit,
        legacy_v4::matches_edit,
        legacy_v5::matches_edit,
        matches_edit,
    ];

    #[test]
    fn every_legacy_source_preserves_audio_and_projects_only_fit_beat_video() {
        for version in 1..=6 {
            let old = document(version).to_string();
            let mut current = upgrade(version, &old).unwrap();
            assert!(matches_document(version, &old, &current));
            let NodeKind::Source { source } = &mut current
                .nodes
                .get_mut(&NodeId::new("source").unwrap())
                .unwrap()
                .kind
            else {
                panic!()
            };
            assert_eq!(source.video_mapping, SourceVideoMapping::FitBeat);
            assert_eq!(source.audio_offset, AudioSample(-137));
            assert_eq!(
                source.audio_mapping,
                if version == 6 {
                    SourceAudioMapping::Duration {
                        frames: ExactRatio::new(60000, 1001).unwrap(),
                    }
                } else {
                    SourceAudioMapping::FitBeat
                }
            );
            // Even equal duration is new authored intent and cannot project as old.
            source.video_mapping = SourceVideoMapping::Duration {
                frames: ExactRatio::integer(60),
                endpoints: EndpointPolicy::Reject,
            };
            assert!(!matches_document(version, &old, &current));
        }
    }

    #[test]
    fn every_legacy_document_and_subtree_rejects_video_mapping_even_null() {
        for version in 1..=6 {
            let source = document(version)["nodes"]["source"].clone();
            let mut subtree = json!({"root":"source", "nodes":{"source":source}});
            if version >= 4 {
                subtree["overrides"] = json!({});
            }
            let request = json!({"project_id":"legacy", "expected_revision":"initial", "new_revision":"insert", "command":{"command":"insert", "parent":"root", "index":0, "subtree":subtree}});
            let adapter = REQUEST_ADAPTERS[(version - 1) as usize];
            assert!(adapter(&request.to_string()).is_ok());
            for mapping in [
                Value::Null,
                json!({"type":"fit_beat"}),
                json!({"type":"duration", "frames":{"numerator":"60","denominator":"1"}, "endpoints":"reject"}),
            ] {
                let mut forged = document(version);
                forged["nodes"]["source"]["kind"]["source"]["video_mapping"] = mapping.clone();
                assert!(
                    upgrade(version, &forged.to_string()).is_err(),
                    "schema {version}"
                );
                let mut forged = request.clone();
                forged["command"]["subtree"]["nodes"]["source"]["kind"]["source"]["video_mapping"] =
                    mapping;
                assert!(adapter(&forged.to_string()).is_err(), "schema {version}");
            }
        }
    }

    #[test]
    fn every_legacy_command_rejects_direct_and_occurrence_video_mapping_edits() {
        for adapter in REQUEST_ADAPTERS {
            for command in [
                json!({"command":"set_source_video_mapping", "node":"source", "mapping":{"type":"fit_beat"}}),
                json!({"command":"edit_occurrence", "instance":{"node":"source","repeats":[]}, "edit":{"type":"set_source_video_mapping","mapping":{"type":"fit_beat"}}, "identities":{"nodes":[],"marks":[]}}),
            ] {
                let request = json!({"project_id":"legacy","expected_revision":"initial","new_revision":"mapped","command":command});
                assert!(adapter(&request.to_string()).is_err());
            }
        }
    }

    #[test]
    fn every_legacy_patch_rejects_video_mapping_and_cannot_hide_new_intent() {
        for (index, adapter) in EDIT_ADAPTERS.into_iter().enumerate() {
            let version = index as u32 + 1;
            let current = upgrade(version, &document(version).to_string()).unwrap();
            let edit = apply(
                &current,
                &CommandRequest {
                    project_id: current.project_id().clone(),
                    expected_revision: current.revision_id().clone(),
                    new_revision: RevisionId::new("renamed").unwrap(),
                    command: Command::Rename {
                        node: NodeId::new("source").unwrap(),
                        label: "Renamed".into(),
                    },
                },
            )
            .unwrap();
            let mut old = serde_json::to_value(&edit).unwrap();
            for direction in ["forward", "inverse"] {
                if version < 3 {
                    old[direction].as_object_mut().unwrap().remove("marks");
                }
                if version < 4 {
                    old[direction].as_object_mut().unwrap().remove("overrides");
                }
                for side in ["before", "after"] {
                    old[direction]["nodes"]["source"][side]
                        .as_object_mut()
                        .unwrap()
                        .remove("audio_edges");
                    let source = old[direction]["nodes"]["source"][side]["kind"]["source"]
                        .as_object_mut()
                        .unwrap();
                    source.remove("video_mapping");
                    if version < 6 {
                        source.remove("audio_mapping");
                    }
                }
            }
            assert!(adapter(&old.to_string(), &edit).unwrap());
            for direction in ["forward", "inverse"] {
                for side in ["before", "after"] {
                    let mut forged = old.clone();
                    forged[direction]["nodes"]["source"][side]["kind"]["source"]["video_mapping"] =
                        Value::Null;
                    assert!(adapter(&forged.to_string(), &edit).is_err());
                }
            }
            let mut forged = edit.clone();
            let node = forged
                .forward
                .nodes
                .get_mut(&NodeId::new("source").unwrap())
                .unwrap()
                .after
                .as_mut()
                .unwrap();
            let NodeKind::Source { source } = &mut node.kind else {
                panic!()
            };
            source.video_mapping = SourceVideoMapping::Duration {
                frames: ExactRatio::integer(60),
                endpoints: EndpointPolicy::Reject,
            };
            assert!(!adapter(&old.to_string(), &forged).unwrap());
        }
    }

    #[test]
    fn schema_six_requires_audio_mapping_and_preserves_audio_commands() {
        let mut missing = document(6);
        missing["nodes"]["source"]["kind"]["source"]
            .as_object_mut()
            .unwrap()
            .remove("audio_mapping");
        assert!(Document::from_json(&missing.to_string()).is_err());
        for command in [
            json!({"command":"set_source_audio_mapping", "node":"source", "mapping":{"type":"duration", "frames":{"numerator":"60000","denominator":"1001"}}, "offset":-137}),
            json!({"command":"edit_occurrence", "instance":{"node":"source","repeats":[]}, "edit":{"type":"set_source_audio_mapping","mapping":{"type":"fit_beat"}, "offset":2401}, "identities":{"nodes":[],"marks":[]}}),
        ] {
            let request = json!({"project_id":"legacy", "expected_revision":"initial", "new_revision":"mapped", "command":command}).to_string();
            assert_eq!(
                upgrade_request(&request).unwrap(),
                serde_json::from_str::<CommandRequest>(&request).unwrap()
            );
        }
    }
}
