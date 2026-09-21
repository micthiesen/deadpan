use std::collections::{BTreeMap, BTreeSet};
use std::{error::Error, fmt};

use serde::{Deserialize, Serialize};

use crate::document::unique_map;
use crate::{
    AssetId, AssetRecord, BeatNode, DocumentError, DocumentErrorCode, FrameDuration, HoldRecipe,
    HoldVideo, MAX_DOCUMENT_NODES, NodeId, NodeKind, ProjectDocument, ProjectId, RevisionId,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Subtree {
    pub root: NodeId,
    #[serde(deserialize_with = "unique_map")]
    pub nodes: BTreeMap<NodeId, BeatNode>,
}

/// Structural node selectors are explicit. Range/text/occurrence resolution is
/// deliberately not inferred from absent UI context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
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
    },
    SetRepeat {
        node: NodeId,
        plays: u32,
        gap: Option<HoldRecipe>,
    },
    SetHoldDuration {
        node: NodeId,
        duration: FrameDuration,
    },
    SetHoldProvider {
        node: NodeId,
        video: HoldVideo,
    },
    Rename {
        node: NodeId,
        label: String,
    },
    AddAsset {
        id: AssetId,
        asset: AssetRecord,
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
    #[serde(deserialize_with = "unique_map")]
    pub nodes: BTreeMap<NodeId, ValueChange<BeatNode>>,
    #[serde(deserialize_with = "unique_map")]
    pub assets: BTreeMap<AssetId, ValueChange<AssetRecord>>,
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
        if self.nodes.len() > MAX_DOCUMENT_NODES || self.assets.len() > MAX_DOCUMENT_NODES {
            return Err(EditError::new(
                EditErrorCode::InvalidCommand,
                "patch exceeds document limits",
            ));
        }
        let mut result = document.clone();
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
        result.revision_id = self.to_revision.clone();
        result.validate()?;
        Ok(result)
    }

    pub fn inverse(&self) -> Self {
        Self {
            project_id: self.project_id.clone(),
            from_revision: self.to_revision.clone(),
            to_revision: self.from_revision.clone(),
            nodes: inverse_changes(&self.nodes),
            assets: inverse_changes(&self.assets),
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
    let mut result = document.clone();
    reduce(&mut result, &request.command)?;
    result.revision_id = request.new_revision.clone();
    let after_duration = result.duration()?.frames();
    let forward = DocumentPatch {
        project_id: document.project_id.clone(),
        from_revision: document.revision_id.clone(),
        to_revision: request.new_revision.clone(),
        nodes: diff(&document.nodes, &result.nodes),
        assets: diff(&document.assets, &result.assets),
    };
    Ok(EditTransaction {
        changed_ids: forward.nodes.keys().cloned().collect(),
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

fn reduce(document: &mut ProjectDocument, command: &Command) -> Result<(), EditError> {
    match command {
        Command::Insert {
            parent,
            index,
            subtree,
        } => {
            if !subtree.nodes.contains_key(&subtree.root) {
                return Err(EditError::new(
                    EditErrorCode::SelectionUnavailable,
                    "inserted subtree root is missing",
                ));
            }
            if subtree
                .nodes
                .len()
                .checked_add(document.nodes.len())
                .is_none_or(|len| len > MAX_DOCUMENT_NODES)
            {
                return Err(EditError::new(
                    EditErrorCode::InvalidCommand,
                    "insertion exceeds document node limit",
                ));
            }
            for id in subtree.nodes.keys() {
                unused(document, id)?;
            }
            insert_child(document, parent, *index, subtree.root.clone())?;
            document.nodes.extend(subtree.nodes.clone());
        }
        Command::Delete { node } => {
            detach(document, node)?;
            let mut pending = vec![node.clone()];
            while let Some(id) = pending.pop() {
                let removed = document.nodes.remove(&id).ok_or_else(|| missing(&id))?;
                pending.extend(removed.kind.children().iter().cloned());
            }
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
            let children = match &document.nodes.get(node).ok_or_else(|| missing(node))?.kind {
                NodeKind::Sequence { children } => children.clone(),
                _ => {
                    return Err(EditError::new(
                        EditErrorCode::WrongNodeKind,
                        "ungroup requires a Sequence",
                    ));
                }
            };
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
                    label: "Repeat".into(),
                    kind: NodeKind::Repeat {
                        child: node.clone(),
                        plays: *plays,
                        gap: gap.clone(),
                    },
                },
            );
        }
        Command::SetRepeat { node, plays, gap } => {
            let NodeKind::Repeat {
                plays: old_plays,
                gap: old_gap,
                ..
            } = &mut node_mut(document, node)?.kind
            else {
                return Err(EditError::new(
                    EditErrorCode::WrongNodeKind,
                    "set-repeat updates an existing Repeat; use wrap-repeat to insert a wrapper",
                ));
            };
            *old_plays = *plays;
            *old_gap = gap.clone();
        }
        Command::SetHoldDuration { node, duration } => {
            hold_mut(document, node)?.duration = *duration;
        }
        Command::SetHoldProvider { node, video } => {
            hold_mut(document, node)?.video = video.clone();
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
    }
    Ok(())
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

fn replace_child(
    document: &mut ProjectDocument,
    parent: &NodeId,
    old: &NodeId,
    new: NodeId,
) -> Result<(), EditError> {
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
        Command::Insert { .. } => "Insert beats",
        Command::Delete { .. } => "Delete beat",
        Command::Move { .. } => "Move beat",
        Command::Group { .. } => "Group beats",
        Command::Ungroup { .. } => "Ungroup beats",
        Command::WrapRepeat { .. } => "Wrap repeat",
        Command::SetRepeat { .. } => "Set repeat parameters",
        Command::SetHoldDuration { .. } => "Change hold duration",
        Command::SetHoldProvider { .. } => "Change hold provider",
        Command::Rename { .. } => "Rename beat",
        Command::AddAsset { .. } => "Register media asset",
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
    fn new(code: EditErrorCode, message: impl Into<String>) -> Self {
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
