use std::collections::{BTreeMap, BTreeSet};
use std::{error::Error, fmt};

use serde::{Deserialize, Serialize};

use crate::document::unique_map;
use crate::{
    AcceptedGeneration, AnchorLossPolicy, AssetId, AssetRecord, AudioSample, BeatNode,
    BoundaryAnchor, DocumentError, DocumentErrorCode, FrameDuration, FrameRateOrigin,
    GeneratedArtifact, GeometryOrigin, HoldFallback, HoldRecipe, HoldVideo, InstancePath,
    IterationId, IterationOrder, MAX_DOCUMENT_MARKS, MAX_DOCUMENT_NODES, Mark, MarkId, MarkState,
    NodeId, NodeKind, OccurrenceEdit, OccurrenceIdentities, PlayOverrides, PresentationChange,
    PrimarySource, PrimarySourceImport, ProjectDocument, ProjectId, RevisionId, SourceAudioMapping,
    SourceNode, SourceVideo, SourceVideoMapping, WrapAnchorPolicy,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Subtree {
    pub root: NodeId,
    #[serde(deserialize_with = "unique_map")]
    pub nodes: BTreeMap<NodeId, BeatNode>,
    #[serde(default, deserialize_with = "unique_map")]
    pub overrides: BTreeMap<NodeId, PlayOverrides>,
}

/// One source beat inserted atomically with its immutable asset registration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceInsertion {
    pub parent: NodeId,
    pub index: usize,
    pub node: NodeId,
    pub label: String,
    pub source: SourceNode,
}

/// Structural node selectors are explicit. Range/text/occurrence resolution is
/// deliberately not inferred from absent UI context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    /// Split an interior local output boundary without changing rendered time.
    Split {
        node: NodeId,
        at: FrameDuration,
        identities: crate::SplitIdentities,
    },
    Insert {
        parent: NodeId,
        index: usize,
        subtree: Subtree,
    },
    Delete {
        node: NodeId,
    },
    /// Destination index is measured after removal from the old parent.
    Move {
        node: NodeId,
        parent: NodeId,
        index: usize,
    },
    /// Groups the nonempty half-open child-index range [start, end).
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
    /// Moves [start,end); destination is measured after removal.
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
    /// Changes picture rate and endpoint behavior without changing beat or audio time.
    SetSourceVideoMapping {
        node: NodeId,
        mapping: SourceVideoMapping,
    },
    /// Changes audio alignment and extent without changing picture or beat time.
    SetSourceAudioMapping {
        node: NodeId,
        mapping: SourceAudioMapping,
        offset: AudioSample,
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
    /// Changes one boundary exception without changing any temporal coordinate.
    SetAudioEdge {
        node: NodeId,
        edge: crate::AudioBoundaryKind,
        policy: crate::AudioEdgePolicy,
    },
    AddAsset {
        id: AssetId,
        asset: AssetRecord,
    },
    /// Host-qualified source registration with optional one-beat insertion.
    ImportSource {
        id: AssetId,
        asset: AssetRecord,
        insertion: Option<Box<SourceInsertion>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
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
        subtree: Subtree,
    },
    ClearPlayOverride {
        node: NodeId,
        iteration: IterationId,
    },
    EditOccurrence {
        instance: InstancePath,
        edit: OccurrenceEdit,
        identities: OccurrenceIdentities,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandRequest {
    pub project_id: ProjectId,
    pub expected_revision: RevisionId,
    /// Hosts allocate unique durable revision IDs, including for undo and redo.
    pub new_revision: RevisionId,
    pub command: Command,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValueChange<T> {
    pub before: Option<T>,
    pub after: Option<T>,
}

/// Granular authored changes, with before-values guarding patch preconditions.
/// No media, undo stack, worker handle, or external resource is embedded here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentPatch {
    pub project_id: ProjectId,
    pub from_revision: RevisionId,
    pub to_revision: RevisionId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation: Option<PresentationChange>,
    #[serde(deserialize_with = "unique_map")]
    pub nodes: BTreeMap<NodeId, ValueChange<BeatNode>>,
    #[serde(deserialize_with = "unique_map")]
    pub assets: BTreeMap<AssetId, ValueChange<AssetRecord>>,
    #[serde(deserialize_with = "unique_map")]
    pub marks: BTreeMap<MarkId, ValueChange<Mark>>,
    #[serde(deserialize_with = "unique_map")]
    pub overrides: BTreeMap<NodeId, ValueChange<PlayOverrides>>,
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        deserialize_with = "unique_map"
    )]
    pub audio_lineage: BTreeMap<NodeId, ValueChange<crate::AudioLineageId>>,
}

impl DocumentPatch {
    /// Used when recording undo/redo as new revisions. Hosts must never reuse a
    /// committed revision ID: that would defeat optimistic concurrency checks.
    pub fn rebased(&self, from: RevisionId, to: RevisionId) -> Self {
        Self {
            from_revision: from,
            to_revision: to,
            ..self.clone()
        }
    }

    pub fn apply(&self, document: &ProjectDocument) -> Result<ProjectDocument, EditError> {
        check_revision(
            document,
            &self.project_id,
            &self.from_revision,
            &self.to_revision,
        )?;
        if self.nodes.len() > MAX_DOCUMENT_NODES
            || self.assets.len() > MAX_DOCUMENT_NODES
            || self.marks.len() > MAX_DOCUMENT_MARKS
            || self.overrides.len() > MAX_DOCUMENT_NODES
            || self.audio_lineage.len() > MAX_DOCUMENT_NODES
        {
            return Err(EditError::new(
                EditErrorCode::InvalidCommand,
                "patch exceeds document limits",
            ));
        }
        let mut result = document.clone();
        if let Some(change) = &self.presentation {
            if document.presentation_state() != change.before {
                return Err(EditError::new(
                    EditErrorCode::PatchConflict,
                    "presentation patch before-value does not match the current document",
                ));
            }
            result.presentation_basis = change.after.basis.clone();
            result.basis_state = change.after.state.clone();
        }
        apply_changes(&mut result.nodes, &self.nodes)?;
        for change in self.assets.values() {
            if let (Some(before), Some(after)) = (&change.before, &change.after)
                && before != after
            {
                return Err(EditError::new(
                    EditErrorCode::ImmutableAsset,
                    "asset identity and stream metadata cannot be changed in place",
                ));
            }
        }
        apply_changes(&mut result.assets, &self.assets)?;
        apply_changes(&mut result.marks, &self.marks)?;
        apply_changes(&mut result.overrides, &self.overrides)?;
        apply_changes(&mut result.audio_lineage, &self.audio_lineage)?;
        result.revision_id = self.to_revision.clone();
        result.validate()?;
        Ok(result)
    }

    pub fn inverse(&self) -> Self {
        Self {
            project_id: self.project_id.clone(),
            from_revision: self.to_revision.clone(),
            to_revision: self.from_revision.clone(),
            presentation: self.presentation.as_ref().map(|change| PresentationChange {
                before: change.after.clone(),
                after: change.before.clone(),
            }),
            nodes: inverse_changes(&self.nodes),
            assets: inverse_changes(&self.assets),
            marks: inverse_changes(&self.marks),
            overrides: inverse_changes(&self.overrides),
            audio_lineage: inverse_changes(&self.audio_lineage),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditTransaction {
    pub forward: DocumentPatch,
    pub inverse: DocumentPatch,
    pub changed_ids: Vec<NodeId>,
    pub duration_delta: i64,
    pub description: String,
}

/// Calculate one atomic transaction without mutating the input. Calling this for
/// a dry-run follows exactly the same validation and reduction path as a commit.
pub fn apply(
    document: &ProjectDocument,
    request: &CommandRequest,
) -> Result<EditTransaction, EditError> {
    check_revision(
        document,
        &request.project_id,
        &request.expected_revision,
        &request.new_revision,
    )?;
    let before_duration = document.duration()?.frames();
    let mut result = match &request.command {
        Command::Split {
            node,
            at,
            identities,
        } => crate::split::apply(document, node, *at, identities, &request.new_revision)?,
        Command::EditOccurrence {
            instance,
            edit,
            identities,
        } => crate::occurrence_edit::apply(
            document,
            instance,
            edit,
            identities,
            &request.new_revision,
        )?,
        command => {
            let mut result = document.clone();
            reduce(&mut result, command, &request.new_revision)?;
            crate::audio_lineage::reconcile(document, &mut result, command)?;
            if !matches!(
                command,
                Command::SetMark { .. } | Command::DeleteMark { .. }
            ) {
                result.marks = crate::marks::transform_marks(document, &result, command)?;
            }
            result
        }
    };
    result.lock_timed_basis(document)?;
    result.revision_id = request.new_revision.clone();
    let after_duration = result.duration()?.frames();
    let forward = DocumentPatch {
        project_id: document.project_id.clone(),
        from_revision: document.revision_id.clone(),
        to_revision: request.new_revision.clone(),
        presentation: (document.presentation_state() != result.presentation_state()).then(|| {
            PresentationChange {
                before: document.presentation_state(),
                after: result.presentation_state(),
            }
        }),
        nodes: diff(&document.nodes, &result.nodes),
        assets: diff(&document.assets, &result.assets),
        marks: diff(&document.marks, &result.marks),
        overrides: diff(&document.overrides, &result.overrides),
        audio_lineage: diff(&document.audio_lineage, &result.audio_lineage),
    };
    Ok(EditTransaction {
        changed_ids: forward
            .nodes
            .keys()
            .chain(forward.overrides.keys())
            .chain(forward.audio_lineage.keys())
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        inverse: forward.inverse(),
        forward,
        // Both durations are nonnegative i64, so their difference always fits.
        duration_delta: after_duration - before_duration,
        description: description(&request.command).to_owned(),
    })
}

fn check_revision(
    document: &ProjectDocument,
    project: &ProjectId,
    expected: &RevisionId,
    next: &RevisionId,
) -> Result<(), EditError> {
    if project != &document.project_id {
        return Err(EditError::new(
            EditErrorCode::ProjectConflict,
            "command targets a different project",
        ));
    }
    if expected != &document.revision_id {
        return Err(EditError {
            code: EditErrorCode::RevisionConflict,
            message: format!(
                "expected revision {expected}; current revision is {}",
                document.revision_id
            ),
            current_revision: Some(document.revision_id.clone()),
        });
    }
    if expected == next {
        return Err(EditError::new(
            EditErrorCode::InvalidCommand,
            "new revision must differ from the current revision",
        ));
    }
    Ok(())
}

pub(crate) fn reduce(
    document: &mut ProjectDocument,
    command: &Command,
    allocation: &RevisionId,
) -> Result<(), EditError> {
    match command {
        Command::Split { .. } => {
            return Err(EditError::new(
                EditErrorCode::InvalidCommand,
                "splits require the retained-context entrypoint",
            ));
        }
        Command::EditOccurrence { .. } => {
            return Err(EditError::new(
                EditErrorCode::InvalidCommand,
                "occurrence edits require the isolation entrypoint",
            ));
        }
        Command::Insert {
            parent,
            index,
            subtree,
        } => {
            let prepared = prepare_subtree(document, subtree, allocation)?;
            insert_child(document, parent, *index, prepared.root.clone())?;
            install_subtree(document, prepared);
        }
        Command::Delete { node } => {
            detach(document, node)?;
            remove_subtree(document, node)?;
        }

        Command::Move {
            node,
            parent,
            index,
        } => {
            detach(document, node)?;
            insert_child(document, parent, *index, node.clone())?;
        }
        Command::Group {
            parent,
            start,
            end,
            id,
            label,
        } => {
            unused(document, id)?;
            let siblings = children_mut(document, parent)?;
            if start >= end || *end > siblings.len() {
                return Err(EditError::new(
                    EditErrorCode::SelectionUnavailable,
                    "group requires a nonempty child range inside the selected sequence",
                ));
            }
            let children = siblings[*start..*end].to_vec();
            siblings.splice(*start..*end, [id.clone()]);
            document
                .nodes
                .insert(id.clone(), BeatNode::sequence(label, children));
        }
        Command::Ungroup { node } => {
            let parent = sequence_parent(document, node)?;
            let beat = document.nodes.get(node).ok_or_else(|| missing(node))?;
            let children = match &beat.kind {
                NodeKind::Sequence { children } => children.clone(),
                _ => {
                    return Err(EditError::new(
                        EditErrorCode::WrongNodeKind,
                        "ungroup requires a Sequence",
                    ));
                }
            };
            if beat.audio_edges != crate::AudioEdgePolicies::default() {
                return Err(EditError::new(
                    EditErrorCode::InvalidCommand,
                    "reset this Sequence's explicit audio edge choices before ungrouping",
                ));
            }
            let siblings = children_mut(document, &parent)?;
            let position = siblings
                .iter()
                .position(|id| id == node)
                .ok_or_else(|| missing(node))?;
            siblings.splice(position..=position, children);
            document.nodes.remove(node);
        }
        Command::WrapRepeat {
            node,
            id,
            plays,
            gap,
            ..
        } => {
            unused(document, id)?;
            let parent = document.parent_of(node).ok_or_else(|| {
                EditError::new(
                    EditErrorCode::SelectionUnavailable,
                    "wrap-repeat requires an existing non-root node",
                )
            })?;
            replace_child(document, &parent, node, id.clone())?;
            document.nodes.insert(
                id.clone(),
                BeatNode {
                    audio_edges: Default::default(),
                    label: "Repeat".into(),
                    kind: NodeKind::Repeat {
                        child: node.clone(),
                        iterations: IterationOrder::new(allocation.clone(), *plays)?,
                        gap: gap.clone(),
                    },
                },
            );
        }
        Command::SetRepeat { node, plays, gap } => {
            let NodeKind::Repeat {
                iterations,
                gap: old_gap,
                ..
            } = &mut node_mut(document, node)?.kind
            else {
                return Err(EditError::new(
                    EditErrorCode::WrongNodeKind,
                    "set-repeat updates an existing Repeat; use wrap-repeat to insert a wrapper",
                ));
            };
            *iterations = iterations.resized(*plays, allocation.clone())?;
            *old_gap = gap.clone();
            let retained = iterations.clone();
            let retired: Vec<_> = document
                .overrides
                .get(node)
                .into_iter()
                .flat_map(|entries| entries.iter())
                .filter(|(identity, _)| retained.position(identity).is_none())
                .map(|(identity, root)| (identity.clone(), root.clone()))
                .collect();
            for (identity, root) in retired {
                remove_override(document, node, &identity);
                remove_subtree(document, &root)?;
            }
        }
        Command::InsertPlays { node, index, count } => {
            let iterations = iterations_mut(document, node)?;
            *iterations = iterations.inserted(*index, *count, allocation.clone())?;
        }
        Command::MovePlays {
            node,
            start,
            end,
            destination,
        } => {
            let iterations = iterations_mut(document, node)?;
            *iterations = iterations.moved(*start, *end, *destination)?;
        }
        Command::SetSourceVideoMapping { node, mapping } => {
            let NodeKind::Source { source } = &mut node_mut(document, node)?.kind else {
                return Err(EditError::new(
                    EditErrorCode::WrongNodeKind,
                    "video mapping requires a Source beat",
                ));
            };
            if !matches!(source.video, SourceVideo::Stream { .. }) {
                return Err(EditError::new(
                    EditErrorCode::SourceRangeInvalid,
                    "video mapping requires selected video",
                ));
            }
            source.video_mapping = *mapping;
        }
        Command::SetSourceAudioMapping {
            node,
            mapping,
            offset,
        } => {
            let NodeKind::Source { source } = &mut node_mut(document, node)?.kind else {
                return Err(EditError::new(
                    EditErrorCode::WrongNodeKind,
                    "audio mapping requires a Source beat",
                ));
            };
            if source.audio.is_none() {
                return Err(EditError::new(
                    EditErrorCode::SourceRangeInvalid,
                    "audio mapping requires selected audio",
                ));
            }
            source.audio_mapping = *mapping;
            source.audio_offset = *offset;
        }
        Command::SetHoldDuration { node, duration } => {
            let recipe = hold_mut(document, node)?;
            if let HoldVideo::Generated { accepted } = &recipe.video
                && *duration > accepted.artifact.sampling.output_frame_count()
            {
                recipe.video = fallback_video(&accepted.fallback);
            }
            recipe.duration = *duration;
        }
        Command::SetHoldProvider { node, video } => {
            if matches!(video, HoldVideo::Generated { .. }) {
                return Err(EditError::new(
                    EditErrorCode::InvalidCommand,
                    "generated media must be accepted with AcceptGeneratedHold",
                ));
            }
            hold_mut(document, node)?.video = video.clone();
        }
        Command::AcceptGeneratedHold {
            node,
            artifact,
            assets,
        } => accept_generated_hold(document, node, artifact, assets)?,
        Command::RevertGeneratedHold { node } => {
            let recipe = hold_mut(document, node)?;
            let HoldVideo::Generated { accepted } = &recipe.video else {
                return Err(EditError::new(
                    EditErrorCode::InvalidCommand,
                    "revert-generated requires a generated Hold provider",
                ));
            };
            recipe.video = fallback_video(&accepted.fallback);
        }
        Command::Rename { node, label } => {
            node_mut(document, node)?.label.clone_from(label);
        }
        Command::AddAsset { id, asset } => {
            if document.assets.contains_key(id) {
                return Err(EditError::new(
                    EditErrorCode::ImmutableAsset,
                    format!("asset {id} already exists; asset records are immutable"),
                ));
            }
            document.assets.insert(id.clone(), asset.clone());
        }
        Command::ImportSource {
            id,
            asset,
            insertion,
            primary,
        } => {
            if asset.source_qualification.is_none() {
                return Err(EditError::new(
                    EditErrorCode::InvalidCommand,
                    "source import requires a qualification receipt binding",
                ));
            }
            if let Some(existing) = document.assets.get(id) {
                if existing != asset {
                    return Err(EditError::new(
                        EditErrorCode::ImmutableAsset,
                        format!("asset {id} already has different immutable metadata"),
                    ));
                }
            } else {
                document.assets.insert(id.clone(), asset.clone());
            }
            if let Some(primary) = primary {
                if document.basis_state.primary.is_some()
                    || !insertion.as_ref().is_some_and(|insertion| matches!(&insertion.source.video, SourceVideo::Stream { asset, .. } if asset == id))
                {
                    return Err(EditError::new(EditErrorCode::InvalidCommand, "first-primary designation requires an inserted stream from the supplied qualified asset and no prior primary"));
                }
                match primary {
                    PrimarySourceImport::Adopt { basis } => {
                        if document.basis_state.rate_origin != FrameRateOrigin::Provisional {
                            return Err(EditError::new(
                                EditErrorCode::InvalidCommand,
                                "primary basis adoption requires a provisional project",
                            ));
                        }
                        crate::basis::validate_canvas(basis.width, basis.height)?;
                        document.presentation_basis = basis.clone();
                        document.basis_state.rate_origin = FrameRateOrigin::PrimarySource;
                        document.basis_state.geometry_origin = GeometryOrigin::PrimarySource;
                    }
                    PrimarySourceImport::KeepBasis => {
                        if document.basis_state.rate_origin == FrameRateOrigin::Provisional {
                            return Err(EditError::new(
                                EditErrorCode::InvalidCommand,
                                "a provisional first primary must adopt its measured basis",
                            ));
                        }
                    }
                }
                document.basis_state.primary = Some(PrimarySource {
                    asset: id.clone(),
                    qualification: asset.source_qualification.clone().ok_or_else(|| {
                        EditError::new(
                            EditErrorCode::InvalidCommand,
                            "primary source requires qualification",
                        )
                    })?,
                });
            }
            if let Some(insertion) = insertion {
                let video_matches = match &insertion.source.video {
                    SourceVideo::Stream { asset, .. } | SourceVideo::Still { asset } => asset == id,
                    SourceVideo::Blank => true,
                };
                if !video_matches
                    || insertion
                        .source
                        .audio
                        .as_ref()
                        .is_some_and(|audio| &audio.asset != id)
                {
                    return Err(EditError::new(
                        EditErrorCode::SourceRangeInvalid,
                        "imported source selections must reference the supplied asset",
                    ));
                }
                unused(document, &insertion.node)?;
                insert_child(
                    document,
                    &insertion.parent,
                    insertion.index,
                    insertion.node.clone(),
                )?;
                document.nodes.insert(
                    insertion.node.clone(),
                    BeatNode {
                        audio_edges: Default::default(),
                        label: insertion.label.clone(),
                        kind: NodeKind::Source {
                            source: insertion.source.clone(),
                        },
                    },
                );
            }
        }
        Command::SetAudioEdge { node, edge, policy } => {
            let beat = node_mut(document, node)?;
            if !edge.supports(&beat.kind) {
                return Err(EditError::new(
                    EditErrorCode::WrongNodeKind,
                    "selected audio boundary does not belong to this node kind",
                ));
            }
            beat.audio_edges.set(*edge, *policy);
        }
        Command::SetCanvas { width, height } => {
            crate::basis::validate_canvas(*width, *height)?;
            document.presentation_basis.width = *width;
            document.presentation_basis.height = *height;
            document.basis_state.geometry_origin = GeometryOrigin::Explicit;
            if document.basis_state.rate_origin == FrameRateOrigin::Provisional {
                document.basis_state.rate_origin = FrameRateOrigin::Explicit;
            }
        }
        Command::AdoptPrimaryGeometry { width, height } => {
            crate::basis::validate_canvas(*width, *height)?;
            if document.basis_state.primary.is_none() {
                return Err(EditError::new(
                    EditErrorCode::InvalidCommand,
                    "primary geometry adoption requires a recorded primary source",
                ));
            }
            document.presentation_basis.width = *width;
            document.presentation_basis.height = *height;
            document.basis_state.geometry_origin = GeometryOrigin::PrimarySource;
        }
        Command::SetPlayOverride {
            node,
            iteration,
            subtree,
        } => {
            require_play(document, node, iteration)?;
            // Check fresh identities before retiring the prior override. A
            // replacement cannot quietly resurrect that subtree's marks.
            let prepared = prepare_subtree(document, subtree, allocation)?;
            if let Some(old) = remove_override(document, node, iteration) {
                remove_subtree(document, &old)?;
            }
            let root = prepared.root.clone();
            install_subtree(document, prepared);
            document
                .overrides
                .entry(node.clone())
                .or_default()
                .insert(iteration.clone(), root);
        }
        Command::ClearPlayOverride { node, iteration } => {
            require_play(document, node, iteration)?;
            let root = remove_override(document, node, iteration).ok_or_else(|| {
                EditError::new(
                    EditErrorCode::SelectionUnavailable,
                    "selected play has no override",
                )
            })?;
            remove_subtree(document, &root)?;
        }

        Command::SetMark {
            id,
            owner,
            label,
            boundary,
            loss_policy,
        } => {
            document.marks.insert(
                id.clone(),
                Mark {
                    fragments: Vec::new(),
                    owner: owner.clone(),
                    label: label.clone(),
                    boundary: boundary.clone(),
                    loss_policy: *loss_policy,
                    state: MarkState::Bound,
                },
            );
        }
        Command::DeleteMark { id } => {
            if document.marks.remove(id).is_none() {
                return Err(EditError::new(
                    EditErrorCode::SelectionUnavailable,
                    format!("mark {id} does not exist"),
                ));
            }
        }
    }
    Ok(())
}

fn prepare_subtree(
    document: &ProjectDocument,
    subtree: &Subtree,
    allocation: &RevisionId,
) -> Result<Subtree, EditError> {
    if !subtree.nodes.contains_key(&subtree.root) {
        return Err(EditError::new(
            EditErrorCode::SelectionUnavailable,
            "inserted subtree root is missing",
        ));
    }
    if subtree.nodes.len() > MAX_DOCUMENT_NODES || subtree.overrides.len() > MAX_DOCUMENT_NODES {
        return Err(EditError::new(
            EditErrorCode::InvalidCommand,
            "inserted subtree exceeds node limit",
        ));
    }
    for id in subtree.nodes.keys() {
        unused(document, id)?;
    }
    let mut prepared = subtree.clone();
    for (id, entries) in &subtree.overrides {
        let Some(BeatNode {
            kind: NodeKind::Repeat { iterations, .. },
            ..
        }) = subtree.nodes.get(id)
        else {
            return Err(EditError::new(
                EditErrorCode::InvalidCommand,
                "inserted override owner is not an inserted Repeat",
            ));
        };
        let normalized = IterationOrder::new(allocation.clone(), iterations.len())?;
        let mut remapped = Vec::with_capacity(entries.len());
        for (identity, root) in entries.iter() {
            let position = iterations.position(identity).ok_or_else(|| {
                EditError::new(
                    EditErrorCode::InvalidCommand,
                    "inserted override names a retired play",
                )
            })?;
            remapped.push(crate::PlayOverride {
                iteration: normalized.at(position).ok_or_else(|| missing(id))?,
                root: root.clone(),
            });
        }
        prepared
            .overrides
            .insert(id.clone(), PlayOverrides::try_from(remapped)?);
    }
    for node in prepared.nodes.values_mut() {
        if let NodeKind::Repeat { iterations, .. } = &mut node.kind {
            *iterations = IterationOrder::new(allocation.clone(), iterations.len())?;
        }
    }
    Ok(prepared)
}
fn install_subtree(document: &mut ProjectDocument, subtree: Subtree) {
    document.nodes.extend(subtree.nodes);
    document.overrides.extend(subtree.overrides);
}
fn remove_subtree(document: &mut ProjectDocument, root: &NodeId) -> Result<(), EditError> {
    let mut pending = vec![root.clone()];
    while let Some(id) = pending.pop() {
        pending.extend(document.children(&id).cloned());
        document.nodes.remove(&id).ok_or_else(|| missing(&id))?;
        document.overrides.remove(&id);
    }
    Ok(())
}
fn require_play(
    document: &ProjectDocument,
    node: &NodeId,
    iteration: &IterationId,
) -> Result<(), EditError> {
    let Some(BeatNode {
        kind: NodeKind::Repeat { iterations, .. },
        ..
    }) = document.nodes.get(node)
    else {
        return Err(EditError::new(
            EditErrorCode::WrongNodeKind,
            "play override requires a Repeat",
        ));
    };
    if iterations.position(iteration).is_none() {
        return Err(EditError::new(
            EditErrorCode::SelectionUnavailable,
            "override names a missing or retired play",
        ));
    }
    Ok(())
}
fn remove_override(
    document: &mut ProjectDocument,
    node: &NodeId,
    iteration: &IterationId,
) -> Option<NodeId> {
    let entries = document.overrides.get_mut(node)?;
    let root = entries.remove(iteration);
    if entries.is_empty() {
        document.overrides.remove(node);
    }
    root
}

fn iterations_mut<'a>(
    document: &'a mut ProjectDocument,
    id: &NodeId,
) -> Result<&'a mut IterationOrder, EditError> {
    match &mut node_mut(document, id)?.kind {
        NodeKind::Repeat { iterations, .. } => Ok(iterations),
        _ => Err(EditError::new(
            EditErrorCode::WrongNodeKind,
            "play editing requires a Repeat",
        )),
    }
}

fn unused(document: &ProjectDocument, id: &NodeId) -> Result<(), EditError> {
    if document.nodes.contains_key(id) {
        return Err(EditError::new(
            EditErrorCode::IdentityConflict,
            format!("node {id} already exists"),
        ));
    }
    Ok(())
}

fn missing(id: &NodeId) -> EditError {
    EditError::new(
        EditErrorCode::SelectionUnavailable,
        format!("node {id} does not exist"),
    )
}

fn node_mut<'a>(
    document: &'a mut ProjectDocument,
    id: &NodeId,
) -> Result<&'a mut BeatNode, EditError> {
    document.nodes.get_mut(id).ok_or_else(|| missing(id))
}

fn children_mut<'a>(
    document: &'a mut ProjectDocument,
    id: &NodeId,
) -> Result<&'a mut Vec<NodeId>, EditError> {
    let NodeKind::Sequence { children } = &mut node_mut(document, id)?.kind else {
        return Err(EditError::new(
            EditErrorCode::WrongNodeKind,
            format!("node {id} must be a Sequence"),
        ));
    };
    Ok(children)
}

fn hold_mut<'a>(
    document: &'a mut ProjectDocument,
    id: &NodeId,
) -> Result<&'a mut HoldRecipe, EditError> {
    let NodeKind::Hold { recipe } = &mut node_mut(document, id)?.kind else {
        return Err(EditError::new(
            EditErrorCode::WrongNodeKind,
            "hold setters require an existing Hold; insert a new hold with insert",
        ));
    };
    Ok(recipe)
}

fn fallback_video(fallback: &HoldFallback) -> HoldVideo {
    match fallback {
        HoldFallback::Background => HoldVideo::Background,
        HoldFallback::Freeze { asset, timestamp } => HoldVideo::Freeze {
            asset: asset.clone(),
            timestamp: *timestamp,
        },
    }
}

fn accept_generated_hold(
    document: &mut ProjectDocument,
    node: &NodeId,
    artifact: &GeneratedArtifact,
    assets: &BTreeMap<AssetId, AssetRecord>,
) -> Result<(), EditError> {
    let fallback = match &hold_mut(document, node)?.video {
        HoldVideo::Background => HoldFallback::Background,
        HoldVideo::Freeze { asset, timestamp } => HoldFallback::Freeze {
            asset: asset.clone(),
            timestamp: *timestamp,
        },
        HoldVideo::Generated { accepted } => accepted.fallback.clone(),
        HoldVideo::Accepted { .. } => {
            return Err(EditError::new(
                EditErrorCode::InvalidCommand,
                "legacy accepted Holds have no explicit fallback and cannot accept generated media",
            ));
        }
    };
    let referenced = BTreeSet::from([
        artifact.sampled_asset.clone(),
        artifact.native_asset.clone(),
    ]);
    if assets.keys().any(|id| !referenced.contains(id)) {
        return Err(EditError::new(
            EditErrorCode::InvalidCommand,
            "generated acceptance may register only artifact-referenced assets",
        ));
    }
    for id in &referenced {
        match (document.assets.get(id), assets.get(id)) {
            (Some(existing), Some(supplied)) if existing != supplied => {
                return Err(EditError::new(
                    EditErrorCode::ImmutableAsset,
                    format!("asset {id} already exists with different immutable metadata"),
                ));
            }
            (None, None) => {
                return Err(EditError::new(
                    EditErrorCode::InvalidCommand,
                    format!("generated artifact asset {id} is not registered or supplied"),
                ));
            }
            _ => {}
        }
    }
    for (id, asset) in assets {
        document
            .assets
            .entry(id.clone())
            .or_insert_with(|| asset.clone());
    }
    hold_mut(document, node)?.video = HoldVideo::Generated {
        accepted: Box::new(AcceptedGeneration {
            artifact: artifact.clone(),
            fallback,
        }),
    };
    Ok(())
}

fn sequence_parent(document: &ProjectDocument, id: &NodeId) -> Result<NodeId, EditError> {
    let parent = document.parent_of(id).ok_or_else(|| {
        EditError::new(
            EditErrorCode::SelectionUnavailable,
            "operation requires an existing non-root node",
        )
    })?;
    if !matches!(
        document.nodes.get(&parent).map(|node| &node.kind),
        Some(NodeKind::Sequence { .. })
    ) {
        return Err(EditError::new(
            EditErrorCode::WrongNodeKind,
            "operation requires a Sequence child; select its enclosing Repeat or Retime to remove that structure",
        ));
    }
    Ok(parent)
}

fn detach(document: &mut ProjectDocument, id: &NodeId) -> Result<(), EditError> {
    let parent = sequence_parent(document, id)?;
    let children = children_mut(document, &parent)?;
    let position = children
        .iter()
        .position(|child| child == id)
        .ok_or_else(|| missing(id))?;
    children.remove(position);
    Ok(())
}

fn insert_child(
    document: &mut ProjectDocument,
    parent: &NodeId,
    index: usize,
    node: NodeId,
) -> Result<(), EditError> {
    let children = children_mut(document, parent)?;
    if index > children.len() {
        return Err(EditError::new(
            EditErrorCode::SelectionUnavailable,
            "insertion index is outside the sequence",
        ));
    }
    children.insert(index, node);
    Ok(())
}

pub(crate) fn replace_child(
    document: &mut ProjectDocument,
    parent: &NodeId,
    old: &NodeId,
    new: NodeId,
) -> Result<(), EditError> {
    if let Some(entries) = document.overrides.get_mut(parent) {
        let identity = entries
            .iter()
            .find(|(_, root)| *root == old)
            .map(|(identity, _)| identity.clone());
        if let Some(identity) = identity {
            entries.insert(identity, new);
            return Ok(());
        }
    }
    let kind = &mut node_mut(document, parent)?.kind;
    let children = match kind {
        NodeKind::Sequence { children } => children.as_mut_slice(),
        NodeKind::Repeat { child, .. } | NodeKind::Retime { child, .. } => {
            std::slice::from_mut(child)
        }
        _ => {
            return Err(EditError::new(
                EditErrorCode::WrongNodeKind,
                "selected parent cannot own children",
            ));
        }
    };
    let child = children
        .iter_mut()
        .find(|id| *id == old)
        .ok_or_else(|| missing(old))?;
    *child = new;
    Ok(())
}

fn diff<K: Ord + Clone, V: Eq + Clone>(
    before: &BTreeMap<K, V>,
    after: &BTreeMap<K, V>,
) -> BTreeMap<K, ValueChange<V>> {
    before
        .keys()
        .chain(after.keys())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter_map(|key| {
            let old = before.get(key);
            let new = after.get(key);
            (old != new).then(|| {
                (
                    key.clone(),
                    ValueChange {
                        before: old.cloned(),
                        after: new.cloned(),
                    },
                )
            })
        })
        .collect()
}

fn inverse_changes<K: Clone + Ord, V: Clone>(
    changes: &BTreeMap<K, ValueChange<V>>,
) -> BTreeMap<K, ValueChange<V>> {
    changes
        .iter()
        .map(|(key, change)| {
            (
                key.clone(),
                ValueChange {
                    before: change.after.clone(),
                    after: change.before.clone(),
                },
            )
        })
        .collect()
}

fn apply_changes<K: Ord + Clone, V: Eq + Clone>(
    values: &mut BTreeMap<K, V>,
    changes: &BTreeMap<K, ValueChange<V>>,
) -> Result<(), EditError> {
    for (key, change) in changes {
        if values.get(key) != change.before.as_ref() {
            return Err(EditError::new(
                EditErrorCode::PatchConflict,
                "patch before-value does not match the current document",
            ));
        }
        match &change.after {
            Some(value) => {
                values.insert(key.clone(), value.clone());
            }
            None => {
                values.remove(key);
            }
        }
    }
    Ok(())
}

fn description(command: &Command) -> &'static str {
    match command {
        Command::Split { .. } => "Split beat",
        Command::Insert { .. } => "Insert beats",
        Command::Delete { .. } => "Delete beat",
        Command::Move { .. } => "Move beat",
        Command::Group { .. } => "Group beats",
        Command::Ungroup { .. } => "Ungroup beats",
        Command::WrapRepeat { .. } => "Wrap repeat",
        Command::SetRepeat { .. } => "Set repeat parameters",
        Command::InsertPlays { .. } => "Insert repeat plays",
        Command::MovePlays { .. } => "Move repeat plays",
        Command::SetHoldDuration { .. } => "Change hold duration",
        Command::SetSourceAudioMapping { .. } => "Change source audio mapping",
        Command::SetSourceVideoMapping { .. } => "Change source video mapping",
        Command::SetHoldProvider { .. } => "Change hold provider",
        Command::AcceptGeneratedHold { .. } => "Accept generated hold",
        Command::RevertGeneratedHold { .. } => "Revert generated hold",
        Command::Rename { .. } => "Rename beat",
        Command::SetAudioEdge { .. } => "Change audio edge policy",
        Command::AddAsset { .. } => "Register media asset",
        Command::ImportSource { .. } => "Import source media",
        Command::SetCanvas { .. } => "Change canvas geometry",
        Command::AdoptPrimaryGeometry { .. } => "Adopt primary source geometry",
        Command::SetMark { .. } => "Set mark",
        Command::DeleteMark { .. } => "Delete mark",
        Command::SetPlayOverride { .. } => "Set play override",
        Command::ClearPlayOverride { .. } => "Clear play override",
        Command::EditOccurrence { .. } => "Edit selected occurrence",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum EditErrorCode {
    ProjectConflict,
    RevisionConflict,
    PatchConflict,
    SelectionUnavailable,
    WrongNodeKind,
    IdentityConflict,
    ImmutableAsset,
    InvalidCommand,
    InvalidDocument,
    InvalidDuration,
    SourceRangeInvalid,
    TimingOverflow,
    LimitExceeded,
}

impl EditErrorCode {
    /// Stable wire spelling, shared by the domain and every host protocol.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ProjectConflict => "ProjectConflict",
            Self::RevisionConflict => "RevisionConflict",
            Self::PatchConflict => "PatchConflict",
            Self::SelectionUnavailable => "SelectionUnavailable",
            Self::WrongNodeKind => "WrongNodeKind",
            Self::IdentityConflict => "IdentityConflict",
            Self::ImmutableAsset => "ImmutableAsset",
            Self::InvalidCommand => "InvalidCommand",
            Self::InvalidDocument => "InvalidDocument",
            Self::InvalidDuration => "InvalidDuration",
            Self::SourceRangeInvalid => "SourceRangeInvalid",
            Self::TimingOverflow => "TimingOverflow",
            Self::LimitExceeded => "LimitExceeded",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EditError {
    pub code: EditErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_revision: Option<RevisionId>,
}

impl EditError {
    pub(crate) fn new(code: EditErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            current_revision: None,
        }
    }
}
impl From<DocumentError> for EditError {
    fn from(value: DocumentError) -> Self {
        let code = match value.code {
            DocumentErrorCode::InvalidDuration => EditErrorCode::InvalidDuration,
            DocumentErrorCode::SourceRangeInvalid => EditErrorCode::SourceRangeInvalid,
            DocumentErrorCode::TimingOverflow => EditErrorCode::TimingOverflow,
            DocumentErrorCode::LimitExceeded => EditErrorCode::LimitExceeded,
            DocumentErrorCode::MissingNode => EditErrorCode::SelectionUnavailable,
            DocumentErrorCode::UnsupportedSchema | DocumentErrorCode::InvalidJson => {
                EditErrorCode::InvalidDocument
            }
            DocumentErrorCode::InvalidIdentity
            | DocumentErrorCode::InvalidPresentation
            | DocumentErrorCode::InvalidAsset
            | DocumentErrorCode::InvalidRoot
            | DocumentErrorCode::InvalidTree
            | DocumentErrorCode::MissingAsset => EditErrorCode::InvalidCommand,
            DocumentErrorCode::InvalidAnchor => EditErrorCode::InvalidCommand,
        };
        Self::new(code, value.to_string())
    }
}
impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl Error for EditError {}
