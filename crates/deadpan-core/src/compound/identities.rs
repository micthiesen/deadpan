//! Reserve caller-supplied fresh pools across the complete transaction, including
//! temporary nodes that disappear within a leaf. Existing setters retain IDs.
use super::identity;
use crate::{Command, EditError, MarkId, NodeId, OccurrenceEdit, ProjectDocument};
use std::collections::BTreeSet;

pub(super) struct Ledger {
    nodes: BTreeSet<NodeId>,
    marks: BTreeSet<MarkId>,
    pending_nodes: BTreeSet<NodeId>,
    pending_marks: BTreeSet<MarkId>,
}
impl Ledger {
    pub(super) fn new(document: &ProjectDocument) -> Self {
        Self {
            nodes: document.nodes().keys().cloned().collect(),
            marks: document.marks().keys().cloned().collect(),
            pending_nodes: BTreeSet::new(),
            pending_marks: BTreeSet::new(),
        }
    }
    pub(super) fn reserve(
        &mut self,
        before: &ProjectDocument,
        command: &Command,
    ) -> Result<(), EditError> {
        self.pending_nodes.clear();
        self.pending_marks.clear();
        let mut nodes = Vec::new();
        let mut marks = Vec::new();
        allocations(command, &mut nodes, &mut marks);
        if let Command::SetMark { id, .. } = command
            && !before.marks().contains_key(id)
        {
            marks.push(id);
        }
        for id in nodes {
            if !self.nodes.insert(id.clone()) {
                return Err(identity(
                    "resolved transaction reuses an allocated node identity",
                ));
            }
            self.pending_nodes.insert(id.clone());
        }
        for id in marks {
            if !self.marks.insert(id.clone()) {
                return Err(identity(
                    "resolved transaction reuses an allocated mark identity",
                ));
            }
            self.pending_marks.insert(id.clone());
        }
        Ok(())
    }
    pub(super) fn observe(
        &mut self,
        before: &ProjectDocument,
        after: &ProjectDocument,
    ) -> Result<(), EditError> {
        for id in after
            .nodes()
            .keys()
            .filter(|id| !before.nodes().contains_key(*id))
        {
            if !self.pending_nodes.contains(id) && !self.nodes.insert(id.clone()) {
                return Err(identity("resolved transaction resurrects a node identity"));
            }
        }
        for id in after
            .marks()
            .keys()
            .filter(|id| !before.marks().contains_key(*id))
        {
            if !self.pending_marks.contains(id) && !self.marks.insert(id.clone()) {
                return Err(identity("resolved transaction resurrects a mark identity"));
            }
        }
        Ok(())
    }
}
fn allocations<'a>(command: &'a Command, nodes: &mut Vec<&'a NodeId>, marks: &mut Vec<&'a MarkId>) {
    match command {
        Command::RepeatSelection { identities, .. } => {
            nodes.push(&identities.repeat);
            nodes.extend(identities.group.iter().chain(&identities.split.nodes));
        }
        Command::ApplySourceTrim { resources, .. } => {
            nodes.extend(
                resources
                    .target_wrapper
                    .iter()
                    .chain(resources.right_wrapper.iter())
                    .chain(&resources.split.nodes)
                    .chain(&resources.fillers),
            );
        }
        Command::RollSources {
            left_wrapper,
            right_wrapper,
            ..
        } => nodes.extend(left_wrapper.iter().chain(right_wrapper)),
        Command::TrimSource { wrapper, .. } => nodes.extend(wrapper),
        Command::InsertTime { id, identities, .. }
        | Command::InsertAiTime { id, identities, .. }
        | Command::SpliceSourceAt { id, identities, .. }
        | Command::ReplaceSource { id, identities, .. } => {
            nodes.push(id);
            nodes.extend(&identities.nodes);
        }
        Command::ReplaceSourceChildren { id, .. } => nodes.push(id),
        Command::GroupSelection { identities, .. } => {
            nodes.push(&identities.group);
            nodes.extend(&identities.split.nodes);
        }
        Command::SpliceSource { id, .. }
        | Command::Group { id, .. }
        | Command::WrapRepeat { id, .. }
        | Command::WrapRetime { id, .. }
        | Command::IsolateGap { id, .. } => nodes.push(id),
        Command::SpliceSlice { identities, .. } => {
            nodes.extend(identities.authored.nodes.iter().chain(&identities.aliases));
            marks.extend(&identities.authored.marks);
        }
        Command::SpliceSliceAt {
            identities,
            split_identities,
            ..
        }
        | Command::ReplaceSlice {
            identities,
            split_identities,
            ..
        } => {
            nodes.extend(
                identities
                    .authored
                    .nodes
                    .iter()
                    .chain(&identities.aliases)
                    .chain(&split_identities.nodes),
            );
            marks.extend(&identities.authored.marks);
        }
        Command::ReplaceSliceChildren { identities, .. } => {
            nodes.extend(identities.authored.nodes.iter().chain(&identities.aliases));
            marks.extend(&identities.authored.marks);
        }
        Command::Split { identities, .. }
        | Command::DeleteRange { identities, .. }
        | Command::MoveRange { identities, .. } => nodes.extend(&identities.nodes),
        Command::Insert { subtree, .. }
        | Command::SetPlayOverride { subtree, .. }
        | Command::SetGapOverride { subtree, .. } => nodes.extend(subtree.nodes.keys()),
        Command::SetRepeatGaps { branches, .. } => {
            nodes.extend(branches.iter().map(|branch| &branch.id));
        }
        Command::ImportSource { insertion, .. } => nodes.extend(insertion.iter().map(|v| &v.node)),
        Command::EditOccurrence {
            identities, edit, ..
        } => {
            nodes.extend(&identities.nodes);
            marks.extend(&identities.marks);
            occurrence(edit, nodes);
        }
        Command::EditScopedMany { identities, .. } => {
            for pool in identities {
                nodes.extend(&pool.nodes);
                marks.extend(&pool.marks);
            }
        }
        Command::EditScoped { identities, .. }
        | Command::KeepFirstPlayAttachments { identities, .. }
        | Command::Explode { identities, .. } => {
            nodes.extend(&identities.nodes);
            marks.extend(&identities.marks);
        }
        Command::Duplicate {
            identities,
            split_identities,
            ..
        } => {
            nodes.extend(
                identities
                    .authored
                    .nodes
                    .iter()
                    .chain(&identities.aliases)
                    .chain(&split_identities.nodes),
            );
            marks.extend(&identities.authored.marks);
        }
        Command::Compound { .. }
        | Command::WithBoundaryReplacements { .. }
        | Command::SlipSource { .. }
        | Command::SetSound { .. }
        | Command::SetBeatSound { .. }
        | Command::DeleteBeatSound { .. }
        | Command::ReplaceSound { .. }
        | Command::DeleteSound { .. }
        | Command::SetTarget { .. }
        | Command::DeleteTarget { .. }
        | Command::SetSoundAllowance { .. }
        | Command::Delete { .. }
        | Command::DeleteRipple { .. }
        | Command::DeleteChildren { .. }
        | Command::Move { .. }
        | Command::Ungroup { .. }
        | Command::SetRepeat { .. }
        | Command::SetRepeatPlays { .. }
        | Command::SetRetime { .. }
        | Command::InsertPlays { .. }
        | Command::MovePlays { .. }
        | Command::SetHoldDuration { .. }
        | Command::SetHoldAudio { .. }
        | Command::SetSourceAudioMapping { .. }
        | Command::SetSourceVideoMapping { .. }
        | Command::SetHoldProvider { .. }
        | Command::SetHoldPictureContext { .. }
        | Command::AcceptGeneratedHold { .. }
        | Command::RevertGeneratedHold { .. }
        | Command::Rename { .. }
        | Command::SetAudioEdge { .. }
        | Command::SetEditorialEdges { .. }
        | Command::SetFraming { .. }
        | Command::SetRepeatEscalation { .. }
        | Command::SetCutaways { .. }
        | Command::SetCaptions { .. }
        | Command::SetAudioTreatments { .. }
        | Command::AddAsset { .. }
        | Command::SetCanvas { .. }
        | Command::AdoptPrimaryGeometry { .. }
        | Command::SetMark { .. }
        | Command::DeleteMark { .. }
        | Command::ClearPlayOverride { .. }
        | Command::ClearGapOverride { .. } => {}
    }
}
fn occurrence<'a>(edit: &'a OccurrenceEdit, nodes: &mut Vec<&'a NodeId>) {
    match edit {
        OccurrenceEdit::Split { identities, .. } => nodes.extend(&identities.nodes),
        OccurrenceEdit::Insert { subtree, .. }
        | OccurrenceEdit::SetPlayOverride { subtree, .. }
        | OccurrenceEdit::SetGapOverride { subtree, .. } => nodes.extend(subtree.nodes.keys()),
        OccurrenceEdit::Group { id, .. }
        | OccurrenceEdit::WrapRepeat { id, .. }
        | OccurrenceEdit::WrapRetime { id, .. }
        | OccurrenceEdit::IsolateGap { id, .. } => nodes.push(id),
        _ => {}
    }
}
