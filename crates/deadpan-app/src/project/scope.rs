//! Ephemeral navigation through ordinary authored Sequence groups.

use deadpan_core::{
    FrameDuration, MAX_DOCUMENT_DEPTH, MAX_DOCUMENT_NODES, NodeId, NodeKind, ProjectDocument,
    ProjectFrame,
};
use deadpan_plan::RenderPlan;

use super::Workspace;

/// A checked path through authored Sequence children. The empty path is the
/// project root; Repeat and Retime children are never navigation scopes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SequenceScope {
    groups: Vec<NodeId>,
}

/// The active Sequence and its absolute project-frame extent.
#[derive(Debug, Clone, Copy)]
pub struct SequenceScopeView<'a> {
    pub owner: &'a NodeId,
    pub children: &'a [NodeId],
    pub start: u64,
    pub end: u64,
}

impl SequenceScope {
    /// Follow a retained host identity, including empty groups at shared
    /// boundaries. Composite ancestors stop native descent at their parent.
    pub(super) fn for_target(
        workspace: &Workspace,
        target: &NodeId,
    ) -> Result<(Self, Option<NodeId>), String> {
        let document = &workspace.document;
        if !document.nodes().contains_key(target) {
            return Err("The marked host no longer exists".into());
        }
        let parents: std::collections::BTreeMap<_, _> = document
            .nodes()
            .keys()
            .flat_map(|owner| document.children(owner).map(move |child| (child, owner)))
            .collect();
        let mut path = Vec::new();
        let mut current = target;
        while current != document.root() {
            if path.len() >= MAX_DOCUMENT_DEPTH {
                return Err("The marked host exceeds the document depth limit".into());
            }
            path.push(current);
            current = parents
                .get(current)
                .copied()
                .ok_or("The marked host is outside the authored tree")?;
        }
        path.reverse();
        let mut scope = Self::default();
        for child in path {
            if child == target || !matches!(document.nodes()[child].kind, NodeKind::Sequence { .. })
            {
                return Ok((scope, Some(child.clone())));
            }
            scope = scope.descend(workspace, child)?;
        }
        Ok((scope, None))
    }

    /// Identity-only fixture for reducers that compare scopes without resolving
    /// a workspace. Production paths still come from checked descent.
    #[cfg(test)]
    pub(crate) fn test_path(groups: Vec<NodeId>) -> Self {
        Self { groups }
    }

    /// Resolve this path and calculate its absolute span from only the Sequence
    /// prefixes along the path. Durations come from the immutable compiled plan.
    pub fn resolve<'a>(&self, workspace: &'a Workspace) -> Result<SequenceScopeView<'a>, String> {
        self.resolve_document(&workspace.document, &workspace.plan)
    }

    /// Historical capture resolves the same ordinary path without manufacturing
    /// a committed workspace or granting media access to its document.
    pub(super) fn resolve_document<'a>(
        &self,
        document: &'a ProjectDocument,
        plan: &RenderPlan,
    ) -> Result<SequenceScopeView<'a>, String> {
        self.document_path(document, plan)?
            .last()
            .copied()
            .ok_or_else(|| "The project root has no Sequence scope".into())
    }

    fn path<'a>(&self, workspace: &'a Workspace) -> Result<Vec<SequenceScopeView<'a>>, String> {
        self.document_path(&workspace.document, &workspace.plan)
    }

    fn document_path<'a>(
        &self,
        document: &'a ProjectDocument,
        plan: &RenderPlan,
    ) -> Result<Vec<SequenceScopeView<'a>>, String> {
        if self.groups.len() > MAX_DOCUMENT_DEPTH {
            return Err("Sequence navigation exceeds the document depth limit".into());
        }
        let mut owner = document.root();
        let mut start = 0_u64;
        let mut views = Vec::with_capacity(self.groups.len() + 1);
        let root_children = sequence_children(document.nodes().get(owner).map(|node| &node.kind))?;
        let root_end = duration_frames(plan, owner)?;
        views.push(SequenceScopeView {
            owner,
            children: root_children,
            start,
            end: root_end,
        });
        let mut prefix_work = 0_usize;
        for group in &self.groups {
            let children = sequence_children(document.nodes().get(owner).map(|node| &node.kind))?;
            let index = children
                .iter()
                .position(|child| child == group)
                .ok_or("The active Sequence path is no longer valid")?;
            for sibling in &children[..index] {
                prefix_work = prefix_work
                    .checked_add(1)
                    .filter(|work| *work <= MAX_DOCUMENT_NODES)
                    .ok_or("Sequence scope prefix exceeds the document work limit")?;
                start = start
                    .checked_add(duration_frames(plan, sibling)?)
                    .ok_or("Sequence scope position overflowed")?;
            }
            let nested = document
                .nodes()
                .get(group)
                .ok_or("The active Sequence no longer exists")?;
            if !matches!(nested.kind, NodeKind::Sequence { .. }) {
                return Err("Navigation can enter only an ordinary Sequence group".into());
            }
            owner = children
                .get(index)
                .ok_or("The active Sequence path is no longer valid")?;
            let children = sequence_children(document.nodes().get(owner).map(|node| &node.kind))?;
            let duration = duration_frames(plan, owner)?;
            let end = start
                .checked_add(duration)
                .ok_or("Sequence scope end overflowed")?;
            views.push(SequenceScopeView {
                owner,
                children,
                start,
                end,
            });
        }
        Ok(views)
    }

    /// Enter a direct child Sequence while keeping every ancestor live.
    pub fn descend(&self, workspace: &Workspace, node: &NodeId) -> Result<Self, String> {
        let view = self.resolve(workspace)?;
        if !view.children.iter().any(|child| child == node) {
            return Err("Only a direct child of the active Sequence can be entered".into());
        }
        if !matches!(
            workspace
                .document
                .nodes()
                .get(node)
                .map(|entry| &entry.kind),
            Some(NodeKind::Sequence { .. })
        ) {
            return Err("Navigation can enter only an ordinary Sequence group".into());
        }
        if self.groups.len() >= MAX_DOCUMENT_DEPTH {
            return Err("Sequence navigation exceeds the document depth limit".into());
        }
        let mut next = self.clone();
        next.groups.push(node.clone());
        Ok(next)
    }

    /// Return to the enclosing Sequence. The root scope has no parent.
    pub fn parent(&self) -> Option<Self> {
        let mut parent = self.clone();
        parent.groups.pop().map(|_| parent)
    }

    /// Keep the longest path that still follows direct Sequence-child edges.
    pub fn reconcile(&mut self, workspace: &Workspace) {
        self.groups.truncate(MAX_DOCUMENT_DEPTH);
        let document = &workspace.document;
        let mut owner = document.root();
        let mut valid = 0;
        for group in &self.groups {
            let Ok(children) =
                sequence_children(document.nodes().get(owner).map(|node| &node.kind))
            else {
                break;
            };
            if !children.iter().any(|child| child == group)
                || !matches!(
                    document.nodes().get(group).map(|node| &node.kind),
                    Some(NodeKind::Sequence { .. })
                )
            {
                break;
            }
            owner = group;
            valid += 1;
        }
        self.groups.truncate(valid);
    }

    pub fn groups(&self) -> &[NodeId] {
        &self.groups
    }

    /// Select the nearest Sequence scope that contains an absolute transport
    /// cursor. This is for explicit pause/natural completion only; command and
    /// context-preserving stops retain their captured scope.
    pub fn enclosing_cursor(&self, workspace: &Workspace, cursor: u64) -> Self {
        let mut requested = self.clone();
        requested.reconcile(workspace);
        let Ok(path) = requested.path(workspace) else {
            return Self::default();
        };
        let mut enclosing = Self::default();
        for (index, view) in path.iter().enumerate().skip(1) {
            if cursor < view.start || cursor >= view.end {
                break;
            }
            enclosing.groups.push(requested.groups[index - 1].clone());
        }
        enclosing
    }

    /// Reject a pause whose existing project-boundary seam resolves above the
    /// active navigation scope. Core InsertTime remains boundary-based.
    pub fn check_pause(&self, workspace: &Workspace, at: ProjectFrame) -> Result<(), String> {
        let view = self.resolve(workspace)?;
        let at = u64::try_from(at.0)
            .map_err(|_| "Pause boundary is outside the active Sequence scope")?;
        if at < view.start || at > view.end {
            return Err("Pause boundary is outside the active Sequence scope".into());
        }
        let target = workspace
            .document
            .insert_time_target(ProjectFrame(
                i64::try_from(at).map_err(|_| "Pause boundary exceeds the project frame range")?,
            ))
            .map_err(|error| error.to_string())?;
        if !sequence_descendant_or_self(&workspace.document, view.owner, &target.parent) {
            return Err(
                "This is a Sequence edge; leave this scope before inserting a pause".into(),
            );
        }
        Ok(())
    }
}

fn sequence_children(kind: Option<&NodeKind>) -> Result<&[NodeId], String> {
    match kind {
        Some(NodeKind::Sequence { children }) => Ok(children),
        _ => Err("The active navigation owner is not a Sequence".into()),
    }
}

fn duration_frames(plan: &RenderPlan, node: &NodeId) -> Result<u64, String> {
    let duration: FrameDuration = plan
        .node_duration(node)
        .ok_or("The active Sequence path references a node outside the render plan")?;
    u64::try_from(duration.frames()).map_err(|_| "Sequence duration is negative".into())
}

fn sequence_descendant_or_self(
    document: &deadpan_core::ProjectDocument,
    ancestor: &NodeId,
    target: &NodeId,
) -> bool {
    let mut pending = vec![(ancestor.clone(), 0_usize)];
    let mut visited = 0_usize;
    while let Some((owner, depth)) = pending.pop() {
        if &owner == target {
            return true;
        }
        if depth >= MAX_DOCUMENT_DEPTH {
            continue;
        }
        let Some(NodeKind::Sequence { children }) =
            document.nodes().get(&owner).map(|node| &node.kind)
        else {
            continue;
        };
        for child in children {
            visited += 1;
            if visited > MAX_DOCUMENT_NODES {
                return false;
            }
            if matches!(
                document.nodes().get(child).map(|node| &node.kind),
                Some(NodeKind::Sequence { .. })
            ) {
                pending.push((child.clone(), depth + 1));
            }
        }
    }
    false
}
