//! Frozen schema-10 document and history adapter. Authored audio edge policies
//! and edge commands cannot enter pre-schema-11 history.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::document::unique_map;
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
    basis_state: BasisState,
    root: NodeId,
    #[serde(deserialize_with = "unique_map")]
    nodes: BTreeMap<NodeId, LegacyBeatNode>,
    #[serde(deserialize_with = "unique_map")]
    assets: BTreeMap<AssetId, AssetRecord>,
    #[serde(deserialize_with = "unique_map")]
    marks: BTreeMap<MarkId, Mark>,
    #[serde(deserialize_with = "unique_map")]
    overrides: BTreeMap<NodeId, PlayOverrides>,
}

impl Document {
    pub fn from_json(json: &str) -> Result<Self, DocumentError> {
        let old: Self = parse(json)?;
        if old.schema_version != 10 {
            return Err(invalid("migration requires document schema 10"));
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
            basis_state: self.basis_state,
            root: self.root,
            nodes: self
                .nodes
                .into_iter()
                .map(|(id, node)| (id, node.upgrade()))
                .collect(),
            assets: self.assets,
            marks: self.marks,
            overrides: self.overrides,
        };
        document.validate()?;
        Ok(document)
    }

    pub fn matches(&self, document: &ProjectDocument) -> bool {
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
            schema_version: 10,
            project_id: document.project_id.clone(),
            revision_id: document.revision_id.clone(),
            presentation_basis: document.presentation_basis.clone(),
            basis_state: document.basis_state.clone(),
            root: document.root.clone(),
            nodes,
            assets,
            marks: document.marks.clone(),
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
    Delete,
    Group {
        start: usize,
        end: usize,
        id: NodeId,
        label: String,
    },
    Ungroup,
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
    RevertGeneratedHold,
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
            Self::Delete => OccurrenceEdit::Delete,
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
            Self::Ungroup => OccurrenceEdit::Ungroup,
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
            Self::RevertGeneratedHold => OccurrenceEdit::RevertGeneratedHold,
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
    marks: BTreeMap<MarkId, ValueChange<Mark>>,
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
            marks: patch.marks.clone(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn document(version: u32) -> Value {
        let mut wire = json!({
            "schema_version": version, "project_id": "legacy", "revision_id": "initial",
            "presentation_basis": {"width": 16, "height": 16, "frame_rate": {"numerator": 30, "denominator": 1}, "color_policy": "sdr_rec709"},
            "root": "root", "nodes": {"root": {"label": "Root", "kind": {"type": "sequence", "children": []}}},
            "assets": {}
        });
        if version >= 3 {
            wire["marks"] = json!({});
        }
        if version >= 4 {
            wire["overrides"] = json!({});
        }
        if version >= 10 {
            wire["basis_state"] =
                json!({"rate_origin":"explicit", "geometry_origin":"explicit", "primary":null});
        }
        wire
    }

    macro_rules! with_document {
        ($version:expr, $wire:expr, $old:ident, $body:expr) => {
            match $version {
                1 => {
                    let $old = legacy_v1::Document::from_json($wire)?;
                    $body
                }
                2 => {
                    let $old = legacy_v2::Document::from_json($wire)?;
                    $body
                }
                3 => {
                    let $old = legacy_v3::Document::from_json($wire)?;
                    $body
                }
                4 => {
                    let $old = legacy_v4::Document::from_json($wire)?;
                    $body
                }
                5 => {
                    let $old = legacy_v5::Document::from_json($wire)?;
                    $body
                }
                6 => {
                    let $old = legacy_v6::Document::from_json($wire)?;
                    $body
                }
                7 => {
                    let $old = legacy_v7::Document::from_json($wire)?;
                    $body
                }
                8 => {
                    let $old = legacy_v8::Document::from_json($wire)?;
                    $body
                }
                9 => {
                    let $old = legacy_v9::Document::from_json($wire)?;
                    $body
                }
                10 => {
                    let $old = Document::from_json($wire)?;
                    $body
                }
                _ => unreachable!(),
            }
        };
    }

    fn upgrade(version: u32, wire: &str) -> Result<ProjectDocument, DocumentError> {
        with_document!(version, wire, old, old.upgrade())
    }

    fn matches_document(
        version: u32,
        wire: &str,
        current: &ProjectDocument,
    ) -> Result<bool, DocumentError> {
        with_document!(version, wire, old, Ok(old.matches(current)))
    }

    type RequestAdapter = fn(&str) -> Result<CommandRequest, DocumentError>;
    const REQUESTS: [RequestAdapter; 10] = [
        legacy_v1::upgrade_request,
        legacy_v2::upgrade_request,
        legacy_v3::upgrade_request,
        legacy_v4::upgrade_request,
        legacy_v5::upgrade_request,
        legacy_v6::upgrade_request,
        legacy_v7::upgrade_request,
        legacy_v8::upgrade_request,
        legacy_v9::upgrade_request,
        upgrade_request,
    ];
    type EditAdapter = fn(&str, &EditTransaction) -> Result<bool, DocumentError>;
    const EDITS: [EditAdapter; 10] = [
        legacy_v1::matches_edit,
        legacy_v2::matches_edit,
        legacy_v3::matches_edit,
        legacy_v4::matches_edit,
        legacy_v5::matches_edit,
        legacy_v6::matches_edit,
        legacy_v7::matches_edit,
        legacy_v8::matches_edit,
        legacy_v9::matches_edit,
        matches_edit,
    ];

    #[test]
    fn legacy_documents_and_projections_cannot_hide_authored_edges() {
        for version in 1..=10 {
            let old = document(version);
            let current = upgrade(version, &old.to_string()).unwrap();
            assert!(
                current
                    .nodes()
                    .values()
                    .all(|node| node.audio_edges == AudioEdgePolicies::default())
            );
            assert!(matches_document(version, &old.to_string(), &current).unwrap());
            let mut changed = current.clone();
            changed
                .nodes
                .get_mut(&NodeId::new("root").unwrap())
                .unwrap()
                .audio_edges
                .node_start = AudioEdgePolicy::Hard;
            changed.validate().unwrap();
            assert!(!matches_document(version, &old.to_string(), &changed).unwrap());
            for edges in [
                Value::Null,
                serde_json::to_value(AudioEdgePolicies::default()).unwrap(),
            ] {
                let mut forged = old.clone();
                forged["nodes"]["root"]["audio_edges"] = edges;
                assert!(
                    upgrade(version, &forged.to_string()).is_err(),
                    "schema {version}"
                );
            }
        }
    }

    #[test]
    fn legacy_requests_reject_edges_in_every_subtree_and_new_commands() {
        for (index, adapter) in REQUESTS.into_iter().enumerate() {
            let version = u32::try_from(index + 1).unwrap();
            let mut subtree = json!({"root":"root", "nodes":document(version)["nodes"]});
            if version >= 4 {
                subtree["overrides"] = json!({});
            }
            let mut commands =
                vec![json!({"command":"insert", "parent":"root", "index":0, "subtree":subtree})];
            if version >= 4 {
                commands.push(json!({"command":"set_play_override", "node":"root", "iteration":{"allocation":"initial","ordinal":0}, "subtree":subtree}));
                commands.push(json!({"command":"edit_occurrence", "instance":{"node":"root","repeats":[]}, "edit":{"type":"insert","index":0,"subtree":subtree}, "identities":{"nodes":[],"marks":[]}}));
                commands.push(json!({"command":"edit_occurrence", "instance":{"node":"root","repeats":[]}, "edit":{"type":"set_play_override","iteration":{"allocation":"initial","ordinal":0},"subtree":subtree}, "identities":{"nodes":[],"marks":[]}}));
            }
            for command in commands {
                let request = json!({"project_id":"legacy","expected_revision":"initial","new_revision":"next","command":command});
                assert!(
                    adapter(&request.to_string()).is_ok(),
                    "schema {version}: {request}"
                );
                for edges in [
                    Value::Null,
                    serde_json::to_value(AudioEdgePolicies::default()).unwrap(),
                ] {
                    let mut forged = request.clone();
                    let command = &mut forged["command"];
                    let subtree = if command["command"] == "edit_occurrence" {
                        &mut command["edit"]["subtree"]
                    } else {
                        &mut command["subtree"]
                    };
                    subtree["nodes"]["root"]["audio_edges"] = edges;
                    assert!(
                        adapter(&forged.to_string()).is_err(),
                        "schema {version}: {forged}"
                    );
                }
            }
            for command in [
                json!({"command":"set_audio_edge","node":"root","edge":"node_start","policy":"hard"}),
                json!({"command":"edit_occurrence","instance":{"node":"root","repeats":[]},"edit":{"type":"set_audio_edge","edge":"node_start","policy":"hard"},"identities":{"nodes":[],"marks":[]}}),
            ] {
                let request = json!({"project_id":"legacy","expected_revision":"initial","new_revision":"next","command":command});
                assert!(adapter(&request.to_string()).is_err(), "schema {version}");
            }
        }
    }

    #[test]
    fn legacy_forward_and_inverse_patches_reject_edges_and_nondefault_projection() {
        for (index, adapter) in EDITS.into_iter().enumerate() {
            let version = u32::try_from(index + 1).unwrap();
            let current = upgrade(version, &document(version).to_string()).unwrap();
            let edit = apply(
                &current,
                &CommandRequest {
                    project_id: current.project_id().clone(),
                    expected_revision: current.revision_id().clone(),
                    new_revision: RevisionId::new("renamed").unwrap(),
                    command: Command::Rename {
                        node: NodeId::new("root").unwrap(),
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
                    old[direction]["nodes"]["root"][side]
                        .as_object_mut()
                        .unwrap()
                        .remove("audio_edges");
                }
            }
            assert!(
                adapter(&old.to_string(), &edit).unwrap(),
                "schema {version}"
            );
            for direction in ["forward", "inverse"] {
                for side in ["before", "after"] {
                    let mut forged = old.clone();
                    forged[direction]["nodes"]["root"][side]["audio_edges"] = Value::Null;
                    assert!(
                        adapter(&forged.to_string(), &edit).is_err(),
                        "schema {version}"
                    );
                    let mut changed = edit.clone();
                    let patch = if direction == "forward" {
                        &mut changed.forward
                    } else {
                        &mut changed.inverse
                    };
                    let node = patch.nodes.get_mut(&NodeId::new("root").unwrap()).unwrap();
                    let node = if side == "before" {
                        node.before.as_mut()
                    } else {
                        node.after.as_mut()
                    }
                    .unwrap();
                    node.audio_edges.node_end = AudioEdgePolicy::Hard;
                    assert!(
                        !adapter(&old.to_string(), &changed).unwrap(),
                        "schema {version}"
                    );
                }
            }
        }
    }
}
