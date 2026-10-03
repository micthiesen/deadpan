//! Captured authoring branches remain separate from concrete presentation plays.

use deadpan_core::{
    InstancePath, MAX_DOCUMENT_NODES, NodeId, NodeKind, ProjectFrame, ProjectId, RevisionId,
    ScopedNodeTarget,
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
