//! Captured authoring branches remain separate from concrete presentation plays.

use std::collections::BTreeMap;

use deadpan_core::{
    InstancePath, IterationId, MAX_DOCUMENT_DEPTH, MAX_DOCUMENT_NODES, NodeId, NodeKind,
    ProjectDocument, ProjectFrame, ProjectId, RepeatEditBranch, RepeatEditStep, RevisionId,
    ScopedNodeEdit, ScopedNodeTarget,
};

use super::{SequenceScope, Workspace};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub session: u64,
    pub project: ProjectId,
    pub revision: RevisionId,
    pub scope: SequenceScope,
    /// The inspected Repeat or Retime, a direct child of the ordinary scope.
    pub root: NodeId,
    pub target: ScopedNodeTarget,
    /// None permits a dormant definition without inventing a rendered play.
    pub presentation: Option<InstancePath>,
    pub cursor: ProjectFrame,
    /// `:scope plays 2-3`: the same node in each further selected play,
    /// edited in the same transaction. Empty for All plays or one play.
    pub also: Vec<ScopedNodeTarget>,
}

impl Target {
    pub fn validate(&self, workspace: &Workspace) -> Result<(), String> {
        if self.session != workspace.session
            || &self.project != workspace.document.project_id()
            || &self.revision != workspace.document.revision_id()
        {
            return Err("Project changed since scoped editing began; reopen the inspector.".into());
        }
        if self.cursor.0 < 0 || self.cursor.0 > workspace.plan.duration().frames() {
            return Err("Captured scoped cursor is outside the project.".into());
        }
        if !self.scope.resolve(workspace)?.children.contains(&self.root) {
            return Err("Inspected root is not a direct child of the captured Sequence.".into());
        }
        let document = &workspace.document;
        if !matches!(
            document.nodes()[&self.root].kind,
            NodeKind::Repeat { .. } | NodeKind::Retime { .. }
        ) {
            return Err("Scoped editing requires a Repeat or Retime root.".into());
        }
        self.target
            .validate(document)
            .map_err(|error| error.to_string())?;
        let mut pending = vec![self.root.clone()];
        let mut visited = 0_usize;
        let mut found = false;
        while let Some(node) = pending.pop() {
            visited += 1;
            if visited > MAX_DOCUMENT_NODES {
                return Err("Scoped target exceeds the document node limit.".into());
            }
            if node == self.target.node {
                found = true;
                break;
            }
            pending.extend(document.children(&node).cloned());
        }
        if !found {
            return Err("Scoped target is outside the inspected root.".into());
        }
        for also in &self.also {
            also.validate(document).map_err(|error| error.to_string())?;
            if also.repeats.len() != self.target.repeats.len() || also == &self.target {
                return Err("A further selected play does not mirror the scoped owner.".into());
            }
        }
        if let Some(presentation) = &self.presentation
            && !self
                .target
                .matches_instance(document, presentation)
                .map_err(|error| error.to_string())?
        {
            return Err("Presentation does not match the captured scoped owner.".into());
        }
        Ok(())
    }

    pub fn validate_request(
        &self,
        workspace: &Workspace,
        session: u64,
        revision: &RevisionId,
        scope: &SequenceScope,
        cursor: ProjectFrame,
    ) -> Result<(), String> {
        if self.session != session
            || &self.revision != revision
            || &self.scope != scope
            || self.cursor != cursor
        {
            return Err("Scoped request differs from its captured editor context.".into());
        }
        self.validate(workspace)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Commit {
    pub before: Target,
    pub revision: RevisionId,
    pub target: ScopedNodeTarget,
    pub presentation: Option<InstancePath>,
}

/// The node at `target`'s structural position in another play of the Repeat
/// named by `target.repeats[step]`. Copies made by isolation keep structure,
/// so an override and the shared definition correspond child by child; a
/// play whose owned contents were changed structurally refuses instead.
pub fn retarget(
    document: &ProjectDocument,
    target: &ScopedNodeTarget,
    step: usize,
    iteration: &IterationId,
) -> Result<ScopedNodeTarget, String> {
    let selected = target.repeats.get(step).ok_or("Scoped step is missing.")?;
    let repeat = &selected.repeat;
    let RepeatEditBranch::Play { iteration: first } = &selected.branch else {
        return Err("Choose plays with :scope plays before editing several.".into());
    };
    let NodeKind::Repeat {
        child, iterations, ..
    } = &document.nodes()[repeat].kind
    else {
        return Err("Scoped step is not a Repeat.".into());
    };
    let number = iterations
        .position(iteration)
        .map_or_else(|| "?".to_owned(), |index| (index + 1).to_string());
    let parents: BTreeMap<&NodeId, &NodeId> = document
        .nodes()
        .keys()
        .flat_map(|owner| document.children(owner).map(move |child| (child, owner)))
        .collect();
    let mut path = vec![&target.node];
    loop {
        let node = *path.last().expect("path");
        let parent = parents
            .get(node)
            .copied()
            .ok_or("Scoped node is outside its Repeat.")?;
        if parent == repeat {
            break;
        }
        if path.len() > MAX_DOCUMENT_DEPTH {
            return Err("Scoped path exceeds the depth limit.".into());
        }
        path.push(parent);
    }
    path.reverse();
    let gap = document
        .gap_overrides()
        .get(repeat)
        .and_then(|entries| entries.get(first))
        == Some(path[0]);
    let root = if gap {
        document
            .gap_overrides()
            .get(repeat)
            .and_then(|entries| entries.get(iteration))
            .ok_or_else(|| format!("Play {number} has no gap of its own to edit."))?
    } else {
        document
            .overrides()
            .get(repeat)
            .and_then(|entries| entries.get(iteration))
            .unwrap_or(child)
    };
    let mut mapping: BTreeMap<&NodeId, NodeId> = BTreeMap::new();
    let mut current = root.clone();
    let differs = || {
        format!(
            "Play {number}'s contents differ in structure; change it with :scope play {number}."
        )
    };
    for (depth, old) in path.iter().enumerate() {
        if depth > 0 {
            let parent = path[depth - 1];
            let position = document
                .children(parent)
                .position(|child| child == *old)
                .ok_or("Scoped path is inconsistent.")?;
            current = document
                .children(&mapping[parent])
                .nth(position)
                .cloned()
                .ok_or_else(differs)?;
        }
        if std::mem::discriminant(&document.nodes()[*old].kind)
            != std::mem::discriminant(&document.nodes()[&current].kind)
        {
            return Err(differs());
        }
        mapping.insert(*old, current.clone());
    }
    let repeats = target
        .repeats
        .iter()
        .enumerate()
        .map(|(index, scope)| {
            Ok(match index.cmp(&step) {
                std::cmp::Ordering::Less => scope.clone(),
                std::cmp::Ordering::Equal => RepeatEditStep {
                    repeat: repeat.clone(),
                    branch: RepeatEditBranch::Play {
                        iteration: iteration.clone(),
                    },
                },
                std::cmp::Ordering::Greater => RepeatEditStep {
                    repeat: mapping
                        .get(&scope.repeat)
                        .cloned()
                        .ok_or("Scoped nested Repeat is outside the path.")?,
                    branch: scope.branch.clone(),
                },
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let result = ScopedNodeTarget {
        node: mapping[&target.node].clone(),
        repeats,
    };
    result
        .validate(document)
        .map_err(|error| error.to_string())?;
    Ok(result)
}

/// The value one further selected play receives. Each play keeps its own
/// recipe: a gain change transfers as the same trim step or ranged step;
/// other values (framing, pause audio, labels, edges) are set as chosen.
pub fn transfer(
    edit: &ScopedNodeEdit,
    entry: &deadpan_core::BeatNode,
    other: &deadpan_core::BeatNode,
    number: u32,
) -> Result<ScopedNodeEdit, String> {
    let ScopedNodeEdit::SetAudioTreatments { treatments } = edit else {
        return Ok(edit.clone());
    };
    let before = &entry.audio_treatments;
    if &other.audio_treatments == before {
        return Ok(edit.clone());
    }
    let refuse = || {
        format!(
            "Play {number} has its own gain recipe; this change cannot be carried over. Change it with :scope play {number}."
        )
    };
    let empty = deadpan_core::ClipGain::default();
    let old = before.clip_gain().unwrap_or(&empty);
    let new = treatments.clip_gain().unwrap_or(&empty);
    if before.saturation() != treatments.saturation()
        || old.muted() != new.muted()
        || old.mute_ranges() != new.mute_ranges()
    {
        return Err(refuse());
    }
    let mine = other.audio_treatments.clip_gain().unwrap_or(&empty);
    let mut result = mine.clone();
    let trim = new.trim().millidecibels() - old.trim().millidecibels();
    if trim != 0 {
        result = result
            .adjust_trim(trim)
            .map_err(|error| error.to_string())?;
    }
    if old.envelopes() != new.envelopes() {
        // Exactly one constant ranged step was added, changed or removed.
        let mut changed = Vec::new();
        for envelope in old.envelopes().iter().chain(new.envelopes()) {
            let range = envelope.range();
            if old.range_step(range) != new.range_step(range) && !changed.contains(&range) {
                changed.push(range);
            }
        }
        let [range] = changed.as_slice() else {
            return Err(refuse());
        };
        let step = |clip: &deadpan_core::ClipGain| {
            clip.range_step(*range)
                .map_or(0, deadpan_core::GainDb::millidecibels)
        };
        let unchanged = |clip: &deadpan_core::ClipGain| {
            clip.envelopes()
                .iter()
                .filter(|envelope| envelope.range() != *range)
                .cloned()
                .collect::<Vec<_>>()
        };
        if unchanged(old) != unchanged(new) {
            return Err(refuse());
        }
        result = result
            .adjust_range(*range, step(new) - step(old))
            .map_err(|error| error.to_string())?;
    }
    let mut order = other.audio_treatments.order().to_vec();
    if !order.contains(&deadpan_core::AudioTreatmentStage::ClipGain) {
        order.insert(0, deadpan_core::AudioTreatmentStage::ClipGain);
    }
    let treatments = deadpan_core::AudioTreatments::with_stages(
        order,
        Some(result),
        other.audio_treatments.saturation(),
    )
    .map_err(|error| error.to_string())?;
    Ok(ScopedNodeEdit::SetAudioTreatments { treatments })
}
