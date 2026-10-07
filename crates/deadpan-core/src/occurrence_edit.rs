//! Copy only authored subtrees needed to make a concrete occurrence independent.
//! The caller supplies fresh identities; isolation and the edit share one patch.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    AssetId, AssetRecord, AudioSample, Command, DocumentError, DocumentErrorCode, EditError,
    EditErrorCode, FrameDuration, GeneratedArtifact, HoldAudio, HoldRecipe, HoldVideo,
    InstancePath, IterationId, MAX_DOCUMENT_MARKS, MAX_DOCUMENT_NODES, MarkId, NodeId, NodeKind,
    PlayOverride, PlayOverrides, ProjectDocument, RevisionId, SourceAudioMapping,
    SourceVideoMapping, Subtree, WrapAnchorPolicy,
};

/// A bounded pool supplied by the host. Unused identities do not enter the document.
/// Nodes are consumed in structural preorder, marks in their current ID order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OccurrenceIdentities {
    pub nodes: Vec<NodeId>,
    pub marks: Vec<MarkId>,
}

/// Operations on the selected node after its repeated ancestors are isolated.
/// Child indexes and play IDs refer to the selected node, not project time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum OccurrenceEdit {
    Split {
        at: FrameDuration,
        identities: crate::SplitIdentities,
    },
    Insert {
        index: usize,
        subtree: Subtree,
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
    WrapRetime {
        id: NodeId,
        duration: FrameDuration,
        pitch: crate::PitchPolicy,
    },
    SetRetime {
        duration: FrameDuration,
        pitch: crate::PitchPolicy,
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
    SetHoldDuration {
        duration: FrameDuration,
    },
    SetHoldAudio {
        audio: HoldAudio,
    },
    SetSourceVideoMapping {
        mapping: SourceVideoMapping,
    },
    SetSourceAudioMapping {
        mapping: SourceAudioMapping,
        offset: AudioSample,
    },
    SetHoldProvider {
        video: HoldVideo,
    },
    SetHoldPictureContext {
        context: Option<crate::CapturedFraming>,
    },
    AcceptGeneratedHold {
        artifact: GeneratedArtifact,
        #[serde(deserialize_with = "crate::document::unique_map")]
        assets: BTreeMap<AssetId, AssetRecord>,
    },
    RevertGeneratedHold,
    Rename {
        label: String,
    },
    SetAudioEdge {
        edge: crate::AudioBoundaryKind,
        policy: crate::AudioEdgePolicy,
    },
    SetFraming {
        framing: Option<crate::Framing>,
    },
    SetAudioTreatments {
        treatments: crate::AudioTreatments,
    },
    SetPlayOverride {
        iteration: IterationId,
        subtree: Subtree,
    },
    ClearPlayOverride {
        iteration: IterationId,
    },
    SetGapOverride {
        iteration: IterationId,
        subtree: Subtree,
    },
    IsolateGap {
        iteration: IterationId,
        id: NodeId,
        timing: crate::AudioTimingId,
    },
    ClearGapOverride {
        iteration: IterationId,
    },
}

impl OccurrenceEdit {
    fn command(&self, node: NodeId) -> Command {
        match self {
            Self::Split { at, identities } => Command::Split {
                node,
                at: *at,
                identities: identities.clone(),
            },
            Self::Insert { index, subtree } => Command::Insert {
                parent: node,
                index: *index,
                subtree: subtree.clone(),
            },
            Self::Delete => Command::Delete { node },
            Self::Group {
                start,
                end,
                id,
                label,
            } => Command::Group {
                parent: node,
                start: *start,
                end: *end,
                id: id.clone(),
                label: label.clone(),
            },
            Self::Ungroup => Command::Ungroup { node },
            Self::WrapRepeat {
                id,
                plays,
                gap,
                anchor_policy,
            } => Command::WrapRepeat {
                node,
                id: id.clone(),
                plays: *plays,
                gap: gap.clone(),
                anchor_policy: *anchor_policy,
            },
            Self::SetRepeat { plays, gap } => Command::SetRepeat {
                node,
                plays: *plays,
                gap: gap.clone(),
            },
            Self::WrapRetime {
                id,
                duration,
                pitch,
            } => Command::WrapRetime {
                node,
                id: id.clone(),
                duration: *duration,
                pitch: *pitch,
            },
            Self::SetRetime { duration, pitch } => Command::SetRetime {
                node,
                duration: *duration,
                pitch: *pitch,
            },
            Self::InsertPlays { index, count } => Command::InsertPlays {
                node,
                index: *index,
                count: *count,
            },
            Self::MovePlays {
                start,
                end,
                destination,
            } => Command::MovePlays {
                node,
                start: *start,
                end: *end,
                destination: *destination,
            },
            Self::SetHoldDuration { duration } => Command::SetHoldDuration {
                node,
                duration: *duration,
            },
            Self::SetHoldAudio { audio } => Command::SetHoldAudio {
                node,
                audio: audio.clone(),
            },
            Self::SetSourceVideoMapping { mapping } => Command::SetSourceVideoMapping {
                node,
                mapping: *mapping,
            },
            Self::SetSourceAudioMapping { mapping, offset } => Command::SetSourceAudioMapping {
                node,
                mapping: *mapping,
                offset: *offset,
            },
            Self::SetHoldProvider { video } => Command::SetHoldProvider {
                node,
                video: video.clone(),
            },
            Self::SetHoldPictureContext { context } => Command::SetHoldPictureContext {
                node,
                context: context.clone(),
            },
            Self::AcceptGeneratedHold { artifact, assets } => Command::AcceptGeneratedHold {
                node,
                artifact: artifact.clone(),
                assets: assets.clone(),
            },
            Self::RevertGeneratedHold => Command::RevertGeneratedHold { node },
            Self::Rename { label } => Command::Rename {
                node,
                label: label.clone(),
            },
            Self::SetFraming { framing } => Command::SetFraming {
                node,
                framing: framing.clone(),
            },
            Self::SetAudioTreatments { treatments } => Command::SetAudioTreatments {
                node,
                treatments: treatments.clone(),
            },
            Self::SetAudioEdge { edge, policy } => Command::SetAudioEdge {
                node,
                edge: *edge,
                policy: *policy,
            },
            Self::SetPlayOverride { iteration, subtree } => Command::SetPlayOverride {
                node,
                iteration: iteration.clone(),
                subtree: subtree.clone(),
            },
            Self::ClearPlayOverride { iteration } => Command::ClearPlayOverride {
                node,
                iteration: iteration.clone(),
            },
            Self::SetGapOverride { iteration, subtree } => Command::SetGapOverride {
                node,
                iteration: iteration.clone(),
                subtree: subtree.clone(),
            },
            Self::IsolateGap {
                iteration,
                id,
                timing,
            } => Command::IsolateGap {
                node,
                iteration: iteration.clone(),
                id: id.clone(),
                timing: timing.clone(),
            },
            Self::ClearGapOverride { iteration } => Command::ClearGapOverride {
                node,
                iteration: iteration.clone(),
            },
        }
    }
}

struct Identities<'a> {
    nodes: std::slice::Iter<'a, NodeId>,
    marks: std::slice::Iter<'a, MarkId>,
}
impl<'a> Identities<'a> {
    fn new(
        document: &ProjectDocument,
        supplied: &'a OccurrenceIdentities,
    ) -> Result<Self, EditError> {
        if supplied.nodes.len() > MAX_DOCUMENT_NODES || supplied.marks.len() > MAX_DOCUMENT_MARKS {
            return Err(invalid("occurrence identity pool exceeds document limits"));
        }
        let mut nodes = BTreeSet::new();
        let mut marks = BTreeSet::new();
        if supplied
            .nodes
            .iter()
            .any(|id| document.nodes().contains_key(id) || !nodes.insert(id))
            || supplied
                .marks
                .iter()
                .any(|id| document.marks().contains_key(id) || !marks.insert(id))
        {
            return Err(invalid("occurrence identities must be fresh and distinct"));
        }
        Ok(Self {
            nodes: supplied.nodes.iter(),
            marks: supplied.marks.iter(),
        })
    }
    fn node(&mut self) -> Result<NodeId, EditError> {
        self.nodes
            .next()
            .cloned()
            .ok_or_else(|| invalid("occurrence isolation needs more node identities"))
    }
    fn mark(&mut self) -> Result<MarkId, DocumentError> {
        self.marks.next().cloned().ok_or_else(|| {
            DocumentError::new(
                DocumentErrorCode::InvalidIdentity,
                "occurrence isolation needs more mark identities",
            )
        })
    }
}

pub(crate) fn apply_captured(
    document: &ProjectDocument,
    instance: &InstancePath,
    edit: &OccurrenceEdit,
    supplied: &OccurrenceIdentities,
    mut context: crate::command::EditContext<'_>,
    mut isolation: Option<&mut Vec<crate::ScopedIsolationStep>>,
) -> Result<ProjectDocument, EditError> {
    let allocation = context.allocation;
    instance.validate(document)?;
    let mut identities = Identities::new(document, supplied)?;
    let mut result = document.clone();
    let mut target = instance.clone();
    let parents: BTreeMap<_, _> = document
        .nodes()
        .keys()
        .flat_map(|id| document.children(id).map(move |child| (child, id)))
        .collect();
    // Ownership, rather than only the selected iteration, disambiguates a
    // play child from that same play's separately owned gap branch.
    let mut gap_ancestors = BTreeSet::new();
    let mut owned = &instance.node;
    while let Some(parent) = parents.get(owned) {
        if document
            .gap_overrides()
            .get(*parent)
            .is_some_and(|entries| entries.iter().any(|(_, root)| root == owned))
        {
            gap_ancestors.insert(*parent);
        }
        owned = parent;
    }
    for index in 0..target.repeats.len() {
        let step = target.repeats[index].clone();
        if gap_ancestors.contains(&instance.repeats[index].node) {
            // The gap already owns an independent subtree. Any outer shared
            // play was isolated earlier in this loop and copied this branch.
            continue;
        }
        if result
            .overrides
            .get(&step.node)
            .and_then(|entries| entries.get(&step.iteration))
            .is_some()
        {
            continue;
        }
        let NodeKind::Repeat { child, .. } = &result.nodes[&step.node].kind else {
            return Err(invalid("occurrence ancestor is not a Repeat"));
        };
        let child = child.clone();
        let nodes = subtree_order(&result, &child)?;
        if result
            .nodes
            .len()
            .checked_add(nodes.len())
            .is_none_or(|count| count > MAX_DOCUMENT_NODES)
        {
            return Err(invalid("occurrence isolation exceeds document node limit"));
        }
        let mut mapping = BTreeMap::new();
        for old in nodes {
            mapping.insert(old, identities.node()?);
        }
        // Isolation preserves every local duration and source mapping. Relocate
        // owned/occurrence marks before the actual operation changes any time.
        let marks =
            crate::marks::clone_occurrence_marks(&result, &step, &mapping, || identities.mark())?;
        clone_nodes(&mut result, &mapping, allocation)?;
        if let Some(allowances) = context.allowances.as_deref_mut() {
            allowances.isolate(&target.repeats[..=index], &mapping)?;
        }
        if let Some(steps) = isolation.as_deref_mut() {
            steps.push(crate::ScopedIsolationStep::from_execution(
                target.repeats[..=index]
                    .iter()
                    .map(|step| crate::RepeatEditStep {
                        repeat: step.node.clone(),
                        branch: crate::RepeatEditBranch::Play {
                            iteration: step.iteration.clone(),
                        },
                    })
                    .collect(),
                mapping.clone(),
            ));
        }
        result
            .overrides
            .entry(step.node)
            .or_default()
            .insert(step.iteration, mapping[&child].clone());
        result.marks = marks;
        remap_instance(&mut target, &mapping);
    }
    target.validate(&result)?;
    result.validate_isolated_context()?;
    let command = edit.command(target.node);
    if let Command::IsolateGap {
        node,
        iteration,
        id,
        timing,
    } = &command
    {
        return crate::gap_override::isolate(&result, node, iteration, id, timing, allocation);
    }
    if let Command::Split {
        node,
        at,
        identities,
    } = &command
    {
        return crate::split::apply(&result, node, *at, identities, context);
    }
    let isolated = result.clone();
    crate::command::reduce(&mut result, &command, allocation)?;
    if let Some(allowances) = context.allowances.as_deref_mut() {
        allowances.apply_hold_audio_command(&command);
    }
    crate::audio_lineage::reconcile(&isolated, &mut result, &command)?;
    result.marks = crate::marks::transform_marks(&isolated, &result, &command)?;
    Ok(result)
}

pub(crate) fn subtree_order(
    document: &ProjectDocument,
    root: &NodeId,
) -> Result<Vec<NodeId>, EditError> {
    let mut output = Vec::new();
    let mut pending = vec![root.clone()];
    while let Some(node) = pending.pop() {
        if output.len() >= MAX_DOCUMENT_NODES {
            return Err(invalid("occurrence subtree exceeds document node limit"));
        }
        pending.extend(document.children(&node).rev().cloned());
        output.push(node);
    }
    Ok(output)
}

pub(crate) fn clone_nodes(
    document: &mut ProjectDocument,
    mapping: &BTreeMap<NodeId, NodeId>,
    allocation: &RevisionId,
) -> Result<(), EditError> {
    let mut sound_clocks = Vec::new();
    for (owner, copy) in mapping {
        if let Some(events) = document.audio_bindings.sound_clocks.get(owner) {
            let mut cloned = BTreeMap::new();
            for (sound, journal) in events {
                let scope = mapping.get(journal.scope()).ok_or_else(|| {
                    invalid("sound clock isolation requires its complete processing scope")
                })?;
                let references = journal
                    .clocks()
                    .iter()
                    .map(|reference| {
                        let repeats = crate::SoundClockRepeatMap::new(
                            reference
                                .repeats()
                                .steps()
                                .iter()
                                .map(|step| {
                                    let live_repeat = mapping
                                        .get(step.live_repeat())
                                        .unwrap_or(step.live_repeat())
                                        .clone();
                                    match step {
                                        crate::SoundClockRepeatStep::Shared {
                                            historical_repeat,
                                            ..
                                        } => crate::SoundClockRepeatStep::Shared {
                                            live_repeat,
                                            historical_repeat: historical_repeat.clone(),
                                        },
                                        crate::SoundClockRepeatStep::Introduced {
                                            plays, ..
                                        } => crate::SoundClockRepeatStep::Introduced {
                                            live_repeat,
                                            plays: plays.clone(),
                                        },
                                    }
                                })
                                .collect(),
                        )?;
                        Ok(reference.clone().with_repeats(repeats))
                    })
                    .collect::<Result<Vec<_>, EditError>>()?;
                cloned.insert(
                    sound.clone(),
                    crate::SoundClockJournal::new(scope.clone(), references)?,
                );
            }
            sound_clocks.push((copy.clone(), cloned));
        }
    }
    // Isolation and Split can duplicate existing contexts repeatedly. Check the
    // combined set before copying any recipe, not after all ancestors expand.
    crate::audio_gain::validate_nodes_with_limit(
        document
            .nodes
            .values()
            .chain(mapping.keys().map(|id| &document.nodes[id])),
        crate::audio_gain::MAX_ISOLATED_GAIN_RECORDS,
    )?;
    crate::picture_context::validate_nodes_with_limit(
        document
            .nodes
            .values()
            .chain(mapping.keys().map(|id| &document.nodes[id])),
        crate::picture_context::MAX_ISOLATED_FRAMING_RECORDS,
    )?;
    for (old, new) in mapping {
        let mut node = document.nodes[old].clone();
        match &mut node.kind {
            NodeKind::Sequence { children } => {
                for child in children {
                    *child = mapping[child].clone();
                }
            }
            NodeKind::Repeat { child, .. } | NodeKind::Retime { child, .. } => {
                *child = mapping[child].clone()
            }
            NodeKind::Source { .. } | NodeKind::Hold { .. } => {}
        }
        // These are existing, scoped play identities, not imported definitions.
        // Fresh authored IDs separate copies; retaining the compact order keeps
        // every pre-isolation occurrence addressable during this transaction.
        document.nodes.insert(new.clone(), node);
        if let Some(events) = document.beat_sounds.get(old).cloned() {
            document.beat_sounds.insert(new.clone(), events);
        }
        if let Some(entries) = document.overrides.get(old) {
            let copied: Vec<_> = entries
                .iter()
                .map(|(iteration, root)| PlayOverride {
                    iteration: iteration.clone(),
                    root: mapping[root].clone(),
                })
                .collect();
            document
                .overrides
                .insert(new.clone(), PlayOverrides::try_from(copied)?);
        }
        if let Some(entries) = document.gap_overrides.get(old) {
            let copied: Vec<_> = entries
                .iter()
                .map(|(iteration, root)| PlayOverride {
                    iteration: iteration.clone(),
                    root: mapping[root].clone(),
                })
                .collect();
            document
                .gap_overrides
                .insert(new.clone(), PlayOverrides::try_from(copied)?);
        }
    }
    document.audio_bindings.sound_clocks.extend(sound_clocks);
    crate::audio_lineage::inherit(document, mapping, allocation);
    crate::audio_binding_lifecycle::inherit(document, mapping);
    Ok(())
}

pub(crate) fn remap_instance(instance: &mut InstancePath, mapping: &BTreeMap<NodeId, NodeId>) {
    if let Some(node) = mapping.get(&instance.node) {
        instance.node = node.clone();
    }
    for step in &mut instance.repeats {
        if let Some(node) = mapping.get(&step.node) {
            step.node = node.clone();
        }
    }
}

fn invalid(message: &str) -> EditError {
    EditError::new(EditErrorCode::InvalidCommand, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Anchor, AnchorLossPolicy, BeatNode, BoundaryAnchor, ColorPolicy, CommandRequest,
        ExactRatio, FrameRate, HoldAudio, InsertionBias, IterationOrder, Mark, MarkState,
        PresentationBasis, ProjectId, RepeatInstance,
    };

    #[test]
    fn isolation_rejects_mark_and_node_growth_at_document_limits() {
        let root = NodeId::new("root").unwrap();
        let repeat = NodeId::new("repeat").unwrap();
        let hold = NodeId::new("hold").unwrap();
        let mut document = ProjectDocument::new(
            ProjectId::new("project").unwrap(),
            RevisionId::new("initial").unwrap(),
            PresentationBasis {
                width: 1920,
                height: 1080,
                frame_rate: FrameRate::new(30, 1).unwrap(),
                color_policy: ColorPolicy::SdrRec709,
            },
            root.clone(),
        )
        .unwrap();
        document
            .nodes
            .insert(root, BeatNode::sequence("root", vec![repeat.clone()]));
        let iterations = IterationOrder::new(RevisionId::new("plays").unwrap(), 2).unwrap();
        document.nodes.insert(
            repeat.clone(),
            BeatNode {
                audio_treatments: Default::default(),
                framing: None,
                audio_editorial_edges: Default::default(),
                audio_edges: Default::default(),
                label: "repeat".into(),
                kind: NodeKind::Repeat {
                    child: hold.clone(),
                    iterations: iterations.clone(),
                    gap: None,
                    escalation: None,
                },
                cutaways: Vec::new(),
                captions: Vec::new(),
            },
        );
        document.nodes.insert(
            hold.clone(),
            BeatNode::hold(
                "hold",
                HoldRecipe {
                    picture_context: None,
                    duration: FrameDuration::new(1).unwrap(),
                    video: HoldVideo::Background,
                    audio: HoldAudio::Silence,
                },
            ),
        );
        let request = CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: RevisionId::new("next").unwrap(),
            command: Command::EditOccurrence {
                instance: InstancePath {
                    node: hold.clone(),
                    repeats: vec![RepeatInstance {
                        node: repeat.clone(),
                        iteration: iterations.at(0).unwrap(),
                    }],
                },
                edit: OccurrenceEdit::Rename {
                    label: "selected".into(),
                },
                identities: OccurrenceIdentities {
                    nodes: vec![NodeId::new("copy").unwrap()],
                    marks: vec![MarkId::new("copy").unwrap()],
                },
            },
        };
        let owned = Mark {
            fragments: Vec::new(),
            owner: hold.clone(),
            label: "owned".into(),
            boundary: BoundaryAnchor {
                coordinate: Anchor::Local {
                    node: hold.clone(),
                    position: ExactRatio::ZERO,
                },
                bias: InsertionBias::Right,
            },
            loss_policy: AnchorLossPolicy::KeepUnresolved,
            state: MarkState::Bound,
        };
        document.marks = (0..MAX_DOCUMENT_MARKS)
            .map(|index| (MarkId::new(format!("mark-{index}")).unwrap(), owned.clone()))
            .collect();
        let error = crate::apply(&document, &request).unwrap_err();
        assert!(
            error.message.contains("exceeds mark binding limit"),
            "{error}"
        );
        assert_eq!(document.marks.len(), MAX_DOCUMENT_MARKS);
        assert!(document.overrides.is_empty());

        document.marks.clear();
        let group = NodeId::new("group").unwrap();
        let mut children = vec![hold];
        for index in 0..MAX_DOCUMENT_NODES - 4 {
            let id = NodeId::new(format!("empty-{index}")).unwrap();
            document
                .nodes
                .insert(id.clone(), BeatNode::sequence("empty", vec![]));
            children.push(id);
        }
        document
            .nodes
            .insert(group.clone(), BeatNode::sequence("group", children));
        let NodeKind::Repeat { child, .. } = &mut document.nodes.get_mut(&repeat).unwrap().kind
        else {
            panic!()
        };
        *child = group;
        assert_eq!(document.nodes.len(), MAX_DOCUMENT_NODES);
        let error = crate::apply(&document, &request).unwrap_err();
        assert!(
            error.message.contains("exceeds document node limit"),
            "{error}"
        );
        assert_eq!(document.nodes.len(), MAX_DOCUMENT_NODES);
        assert!(document.overrides.is_empty());
    }
}
