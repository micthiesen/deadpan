//! Frozen schema-31 document and history adapter. The closed command grammar
//! retains root sound routes and concrete Hold allowances, but rejects later
//! Hold audio-policy setters, including occurrence edits.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::document::unique_map;
use crate::legacy_audio_binding_v35::{LegacyAudioBindingState, project_change};
use crate::legacy_audio_mapping_v35::AudioMapping;
use crate::legacy_framing_v37::LegacyFraming;
use crate::legacy_mark_v13::{LegacyMark, project_mark_changes, project_marks, upgrade_marks};
use crate::legacy_video_mapping_v34::LegacySourceNode;
use crate::legacy_video_mapping_v34::VideoMapping;
use crate::*;

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
        #[serde(default)]
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    framing: Option<LegacyFraming>,
    label: String,
    kind: LegacyNodeKind,
    #[serde(default, skip_serializing_if = "AudioEdgePolicies::is_automatic")]
    audio_edges: AudioEdgePolicies,
}

impl LegacyBeatNode {
    fn upgrade(self) -> BeatNode {
        BeatNode {
            audio_treatments: Default::default(),
            framing: self.framing.map(LegacyFraming::upgrade),
            label: self.label,
            audio_editorial_edges: Default::default(),
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
        if !node.audio_editorial_edges.is_empty() {
            return None;
        }
        if !node.audio_treatments.is_empty() {
            return None;
        }
        Some(Self {
            framing: node
                .framing
                .as_ref()
                .map(LegacyFraming::project)
                .transpose()?,
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
    #[serde(
        default,
        deserialize_with = "unique_map",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    sounds: BTreeMap<SoundId, SoundEvent>,
    #[serde(
        default,
        deserialize_with = "crate::legacy_sound_routes::routes",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    sound_routes: BTreeMap<SoundId, RootSoundRoute>,
    #[serde(
        default,
        deserialize_with = "unique_map",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    sound_allowances: BTreeMap<SoundId, SoundHoldAllowances>,
    #[serde(deserialize_with = "unique_map")]
    overrides: BTreeMap<NodeId, PlayOverrides>,
    #[serde(
        default,
        deserialize_with = "unique_map",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    gap_overrides: BTreeMap<NodeId, PlayOverrides>,
    #[serde(
        default,
        deserialize_with = "unique_map",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    audio_lineage: BTreeMap<NodeId, AudioLineageId>,
    #[serde(default, skip_serializing_if = "LegacyAudioBindingState::is_empty")]
    audio_bindings: LegacyAudioBindingState,
}

impl Document {
    pub fn from_json(json: &str) -> Result<Self, DocumentError> {
        crate::framing::preflight(json)?;
        crate::picture_context::preflight(json)?;
        let old: Self = parse(json)?;
        if old.schema_version != 31 {
            return Err(invalid("migration requires document schema 31"));
        }
        old.clone().upgrade()?;
        Ok(old)
    }

    pub fn revision_id(&self) -> &RevisionId {
        &self.revision_id
    }

    pub fn upgrade(self) -> Result<ProjectDocument, DocumentError> {
        let document = ProjectDocument {
            sounds: self.sounds,
            sound_routes: self.sound_routes,
            sound_allowances: self.sound_allowances,
            gap_overrides: self.gap_overrides,
            audio_lineage: self.audio_lineage,
            audio_bindings: self.audio_bindings.upgrade(),
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
        let Some(audio_bindings) = LegacyAudioBindingState::project(&document.audio_bindings)
        else {
            return false;
        };
        if document
            .sound_routes
            .values()
            .any(|route| !crate::legacy_sound_routes::admitted(route))
        {
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
            schema_version: 31,
            project_id: document.project_id.clone(),
            revision_id: document.revision_id.clone(),
            presentation_basis: document.presentation_basis.clone(),
            basis_state: document.basis_state.clone(),
            root: document.root.clone(),
            nodes,
            assets,
            marks,
            sounds: document.sounds.clone(),
            sound_routes: document.sound_routes.clone(),
            sound_allowances: document.sound_allowances.clone(),
            overrides: document.overrides.clone(),
            audio_lineage: document.audio_lineage.clone(),
            gap_overrides: document.gap_overrides.clone(),
            audio_bindings,
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
    #[serde(
        default,
        deserialize_with = "unique_map",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    gap_overrides: BTreeMap<NodeId, PlayOverrides>,
}

impl OldSubtree {
    fn upgrade(self) -> Subtree {
        Subtree {
            gap_overrides: self.gap_overrides,
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
#[serde(deny_unknown_fields)]
struct OldSplitIdentities {
    nodes: Vec<NodeId>,
}

impl OldSplitIdentities {
    fn upgrade(self) -> SplitIdentities {
        SplitIdentities { nodes: self.nodes }
    }
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum OldOccurrenceEdit {
    Split {
        at: FrameDuration,
        identities: OldSplitIdentities,
    },
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
        #[serde(default)]
        gap: Option<HoldRecipe>,
        #[serde(default)]
        anchor_policy: WrapAnchorPolicy,
    },
    SetRepeat {
        plays: u32,
        #[serde(default)]
        gap: Option<HoldRecipe>,
    },
    WrapRetime {
        id: NodeId,
        duration: FrameDuration,
        pitch: PitchPolicy,
    },
    SetRetime {
        duration: FrameDuration,
        pitch: PitchPolicy,
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
    SetHoldPictureContext {
        context: Option<CapturedFraming>,
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
    SetFraming {
        framing: Option<LegacyFraming>,
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
    SetGapOverride {
        iteration: IterationId,
        subtree: OldSubtree,
    },
    IsolateGap {
        iteration: IterationId,
        id: NodeId,
        timing: AudioTimingId,
    },
    ClearGapOverride {
        iteration: IterationId,
    },
}

impl OldOccurrenceEdit {
    fn upgrade(self) -> OccurrenceEdit {
        match self {
            Self::Split { at, identities } => OccurrenceEdit::Split {
                at,
                identities: identities.upgrade(),
            },
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
            Self::WrapRetime {
                id,
                duration,
                pitch,
            } => OccurrenceEdit::WrapRetime {
                id,
                duration,
                pitch,
            },
            Self::SetRetime { duration, pitch } => OccurrenceEdit::SetRetime { duration, pitch },
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
            Self::SetSourceVideoMapping { mapping } => OccurrenceEdit::SetSourceVideoMapping {
                mapping: mapping.upgrade(),
            },
            Self::SetSourceAudioMapping { mapping, offset } => {
                OccurrenceEdit::SetSourceAudioMapping {
                    mapping: mapping.upgrade(),
                    offset,
                }
            }
            Self::SetHoldDuration { duration } => OccurrenceEdit::SetHoldDuration { duration },
            Self::SetHoldProvider { video } => OccurrenceEdit::SetHoldProvider { video },
            Self::SetHoldPictureContext { context } => {
                OccurrenceEdit::SetHoldPictureContext { context }
            }
            Self::AcceptGeneratedHold { artifact, assets } => {
                OccurrenceEdit::AcceptGeneratedHold { artifact, assets }
            }
            Self::RevertGeneratedHold {} => OccurrenceEdit::RevertGeneratedHold,
            Self::Rename { label } => OccurrenceEdit::Rename { label },
            Self::SetFraming { framing } => OccurrenceEdit::SetFraming {
                framing: framing.map(LegacyFraming::upgrade),
            },
            Self::SetAudioEdge { edge, policy } => OccurrenceEdit::SetAudioEdge { edge, policy },
            Self::SetPlayOverride { iteration, subtree } => OccurrenceEdit::SetPlayOverride {
                iteration,
                subtree: subtree.upgrade(),
            },
            Self::ClearPlayOverride { iteration } => {
                OccurrenceEdit::ClearPlayOverride { iteration }
            }
            Self::SetGapOverride { iteration, subtree } => OccurrenceEdit::SetGapOverride {
                iteration,
                subtree: subtree.upgrade(),
            },
            Self::IsolateGap {
                iteration,
                id,
                timing,
            } => OccurrenceEdit::IsolateGap {
                iteration,
                id,
                timing,
            },
            Self::ClearGapOverride { iteration } => OccurrenceEdit::ClearGapOverride { iteration },
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
enum OldCommand {
    ReplaceSound {
        id: SoundId,
        event: SoundEvent,
    },
    SetSound {
        id: SoundId,
        event: SoundEvent,
    },
    DeleteSound {
        id: SoundId,
    },
    SetSoundAllowance {
        sound: SoundId,
        issuer: SoundHoldIssuer,
        allowed: bool,
    },
    InsertTime {
        at: ProjectFrame,
        hold: HoldRecipe,
        id: NodeId,
        identities: OldSplitIdentities,
        timing: AudioTimingId,
    },
    SpliceSource {
        parent: NodeId,
        index: usize,
        source: LegacySourceNode,
        id: NodeId,
        label: String,
        timing: AudioTimingId,
    },
    Split {
        node: NodeId,
        at: FrameDuration,
        identities: OldSplitIdentities,
    },
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
        #[serde(default)]
        gap: Option<HoldRecipe>,
        #[serde(default)]
        anchor_policy: WrapAnchorPolicy,
    },
    SetRepeat {
        node: NodeId,
        plays: u32,
        #[serde(default)]
        gap: Option<HoldRecipe>,
    },
    WrapRetime {
        node: NodeId,
        id: NodeId,
        duration: FrameDuration,
        pitch: PitchPolicy,
    },
    SetRetime {
        node: NodeId,
        duration: FrameDuration,
        pitch: PitchPolicy,
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
    SetHoldPictureContext {
        node: NodeId,
        context: Option<CapturedFraming>,
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
    SetFraming {
        node: NodeId,
        framing: Option<LegacyFraming>,
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
    SetGapOverride {
        node: NodeId,
        iteration: IterationId,
        subtree: OldSubtree,
    },
    IsolateGap {
        node: NodeId,
        iteration: IterationId,
        id: NodeId,
        timing: AudioTimingId,
    },
    ClearGapOverride {
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
        OldCommand::SetSound { id, event } => Command::SetSound { id, event },
        OldCommand::ReplaceSound { id, event } => Command::ReplaceSound { id, event },
        OldCommand::DeleteSound { id } => Command::DeleteSound { id },
        OldCommand::SetSoundAllowance {
            sound,
            issuer,
            allowed,
        } => Command::SetSoundAllowance {
            sound,
            issuer,
            allowed,
        },
        OldCommand::InsertTime {
            at,
            hold,
            id,
            identities,
            timing,
        } => Command::InsertTime {
            at,
            hold,
            id,
            identities: identities.upgrade(),
            timing,
        },
        OldCommand::SpliceSource {
            parent,
            index,
            source,
            id,
            label,
            timing,
        } => Command::SpliceSource {
            parent,
            index,
            source: source.upgrade(),
            id,
            label,
            timing,
        },
        OldCommand::Split {
            node,
            at,
            identities,
        } => Command::Split {
            node,
            at,
            identities: identities.upgrade(),
        },
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
        OldCommand::WrapRetime {
            node,
            id,
            duration,
            pitch,
        } => Command::WrapRetime {
            node,
            id,
            duration,
            pitch,
        },
        OldCommand::SetRetime {
            node,
            duration,
            pitch,
        } => Command::SetRetime {
            node,
            duration,
            pitch,
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
        OldCommand::SetSourceVideoMapping { node, mapping } => Command::SetSourceVideoMapping {
            node,
            mapping: mapping.upgrade(),
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
        OldCommand::SetHoldPictureContext { node, context } => {
            Command::SetHoldPictureContext { node, context }
        }
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
        OldCommand::SetFraming { node, framing } => Command::SetFraming {
            node,
            framing: framing.map(LegacyFraming::upgrade),
        },
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
        OldCommand::SetGapOverride {
            node,
            iteration,
            subtree,
        } => Command::SetGapOverride {
            node,
            iteration,
            subtree: subtree.upgrade(),
        },
        OldCommand::IsolateGap {
            node,
            iteration,
            id,
            timing,
        } => Command::IsolateGap {
            node,
            iteration,
            id,
            timing,
        },
        OldCommand::ClearGapOverride { node, iteration } => {
            Command::ClearGapOverride { node, iteration }
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
    #[serde(default, deserialize_with = "unique_map")]
    sounds: BTreeMap<SoundId, ValueChange<SoundEvent>>,
    #[serde(default, deserialize_with = "crate::legacy_sound_routes::changes")]
    sound_routes: BTreeMap<SoundId, ValueChange<RootSoundRoute>>,
    #[serde(default, deserialize_with = "unique_map")]
    sound_allowances: BTreeMap<SoundId, ValueChange<SoundHoldAllowances>>,
    #[serde(deserialize_with = "unique_map")]
    overrides: BTreeMap<NodeId, ValueChange<PlayOverrides>>,
    #[serde(default, deserialize_with = "unique_map")]
    gap_overrides: BTreeMap<NodeId, ValueChange<PlayOverrides>>,
    #[serde(default, deserialize_with = "unique_map")]
    audio_lineage: BTreeMap<NodeId, ValueChange<AudioLineageId>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    audio_bindings: Option<ValueChange<LegacyAudioBindingState>>,
}

impl Patch {
    fn project(patch: &DocumentPatch) -> Option<Self> {
        if patch
            .sound_routes
            .values()
            .flat_map(|change| change.before.iter().chain(change.after.iter()))
            .any(|route| !crate::legacy_sound_routes::admitted(route))
        {
            return None;
        }
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
            sounds: patch.sounds.clone(),
            sound_routes: patch.sound_routes.clone(),
            sound_allowances: patch.sound_allowances.clone(),
            overrides: patch.overrides.clone(),
            gap_overrides: patch.gap_overrides.clone(),
            audio_lineage: patch.audio_lineage.clone(),
            audio_bindings: project_change(&patch.audio_bindings)?,
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
    // Schema 31 used the same changed-ID summary as current old commands,
    // including owners whose binding or referenced timing record changed.
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

/// Schema 31 admits only its original root sound-ripple subset. Keep this
/// admission frozen independently of modern sound command validation.
pub fn validate_request_context(
    document: &ProjectDocument,
    request: &CommandRequest,
) -> Result<(), EditError> {
    let command = &request.command;
    if matches!(
        command,
        Command::ApplySourceTrim { .. }
            | Command::TrimSource { .. }
            | Command::RollSources { .. }
            | Command::SlipSource { .. }
            | Command::SpliceSlice { .. }
            | Command::SpliceSliceAt { .. }
            | Command::ReplaceSlice { .. }
            | Command::MoveRange { .. }
            | Command::SpliceSourceAt { .. }
            | Command::ReplaceSource { .. }
            | Command::DeleteRipple { .. }
            | Command::DeleteRange { .. }
    ) {
        return Err(EditError::new(
            EditErrorCode::InvalidCommand,
            "schema 31 does not admit interior source splicing",
        ));
    }
    if matches!(
        command,
        Command::SetHoldAudio { .. }
            | Command::SetAudioTreatments { .. }
            | Command::EditOccurrence {
                edit: OccurrenceEdit::SetHoldAudio { .. }
                    | OccurrenceEdit::SetAudioTreatments { .. },
                ..
            }
    ) {
        return Err(EditError::new(
            EditErrorCode::InvalidCommand,
            "schema 31 cannot change Hold audio policy or node audio treatments",
        ));
    }
    if document.sounds().is_empty()
        || preserves_sound_clocks(command)
        || matches!(
            command,
            Command::InsertTime { .. } | Command::SpliceSource { .. } | Command::Delete { .. }
        )
        || matches!(command, Command::Split { node, .. } if node != document.root())
    {
        return Ok(());
    }
    Err(EditError::new(
        EditErrorCode::InvalidCommand,
        "schema 31 cannot preserve authored sound intervals through this command",
    ))
}

fn preserves_sound_clocks(command: &Command) -> bool {
    match command {
        Command::Compound { .. } | Command::EditScoped { .. } => false,
        Command::SetSound { .. }
        | Command::ReplaceSound { .. }
        | Command::DeleteSound { .. }
        | Command::SetSoundAllowance { .. }
        | Command::SetSourceVideoMapping { .. }
        | Command::SetSourceAudioMapping { .. }
        | Command::SetHoldProvider { .. }
        | Command::SetHoldPictureContext { .. }
        | Command::AcceptGeneratedHold { .. }
        | Command::RevertGeneratedHold { .. }
        | Command::Rename { .. }
        | Command::SetAudioEdge { .. }
        | Command::SetFraming { .. }
        | Command::AddAsset { .. }
        | Command::SetCanvas { .. }
        | Command::AdoptPrimaryGeometry { .. }
        | Command::SetMark { .. }
        | Command::DeleteMark { .. } => true,
        Command::ImportSource {
            insertion, primary, ..
        } => insertion.is_none() && primary.is_none(),
        Command::EditOccurrence { edit, .. } => match edit {
            OccurrenceEdit::SetSourceVideoMapping { .. }
            | OccurrenceEdit::SetSourceAudioMapping { .. }
            | OccurrenceEdit::SetHoldProvider { .. }
            | OccurrenceEdit::SetHoldPictureContext { .. }
            | OccurrenceEdit::AcceptGeneratedHold { .. }
            | OccurrenceEdit::RevertGeneratedHold
            | OccurrenceEdit::Rename { .. }
            | OccurrenceEdit::SetAudioEdge { .. }
            | OccurrenceEdit::SetFraming { .. } => true,
            OccurrenceEdit::SetAudioTreatments { .. }
            | OccurrenceEdit::SetHoldAudio { .. }
            | OccurrenceEdit::Split { .. }
            | OccurrenceEdit::Insert { .. }
            | OccurrenceEdit::Delete
            | OccurrenceEdit::Group { .. }
            | OccurrenceEdit::Ungroup
            | OccurrenceEdit::WrapRepeat { .. }
            | OccurrenceEdit::SetRepeat { .. }
            | OccurrenceEdit::WrapRetime { .. }
            | OccurrenceEdit::SetRetime { .. }
            | OccurrenceEdit::InsertPlays { .. }
            | OccurrenceEdit::MovePlays { .. }
            | OccurrenceEdit::SetHoldDuration { .. }
            | OccurrenceEdit::SetPlayOverride { .. }
            | OccurrenceEdit::ClearPlayOverride { .. }
            | OccurrenceEdit::SetGapOverride { .. }
            | OccurrenceEdit::IsolateGap { .. }
            | OccurrenceEdit::ClearGapOverride { .. } => false,
        },
        Command::SetAudioTreatments { .. }
        | Command::SetHoldAudio { .. }
        | Command::InsertTime { .. }
        | Command::SpliceSource { .. }
        | Command::ApplySourceTrim { .. }
        | Command::TrimSource { .. }
        | Command::RollSources { .. }
        | Command::SlipSource { .. }
        | Command::SpliceSlice { .. }
        | Command::SpliceSliceAt { .. }
        | Command::ReplaceSlice { .. }
        | Command::MoveRange { .. }
        | Command::SpliceSourceAt { .. }
        | Command::ReplaceSource { .. }
        | Command::DeleteRipple { .. }
        | Command::DeleteRange { .. }
        | Command::Split { .. }
        | Command::Insert { .. }
        | Command::Delete { .. }
        | Command::Move { .. }
        | Command::Group { .. }
        | Command::Ungroup { .. }
        | Command::WrapRepeat { .. }
        | Command::SetRepeat { .. }
        | Command::RepeatSelection { .. }
        | Command::SetRepeatPlays { .. }
        | Command::WrapRetime { .. }
        | Command::SetRetime { .. }
        | Command::InsertPlays { .. }
        | Command::MovePlays { .. }
        | Command::SetHoldDuration { .. }
        | Command::SetPlayOverride { .. }
        | Command::ClearPlayOverride { .. }
        | Command::SetGapOverride { .. }
        | Command::IsolateGap { .. }
        | Command::ClearGapOverride { .. } => false,
    }
}
