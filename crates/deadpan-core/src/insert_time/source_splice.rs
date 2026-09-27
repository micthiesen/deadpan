//! Source insertion at an explicit Sequence slot. Keeping the slot separate
//! from its project boundary preserves ownership at nested group edges.

use crate::{
    AnchorIndex, AudioTimingId, BeatNode, EditError, EditErrorCode, FrameDuration,
    MAX_DOCUMENT_DEPTH, NodeId, NodeKind, ProjectDocument, ProjectFrame, RevisionId,
};

impl ProjectDocument {
    /// Return the exact project boundary of an ordinary Sequence child slot.
    /// Empty groups and either group edge retain the explicitly named owner.
    /// Repeat and Retime ancestors require occurrence/clock resolution and are
    /// deliberately rejected. This query grants no edit authority.
    pub fn source_splice_boundary(
        &self,
        parent: &NodeId,
        index: usize,
    ) -> Result<ProjectFrame, EditError> {
        let durations = self.durations()?;
        let node = self.nodes().get(parent).ok_or_else(|| {
            EditError::new(
                EditErrorCode::SelectionUnavailable,
                "splice parent is missing",
            )
        })?;
        let NodeKind::Sequence { children } = &node.kind else {
            return Err(super::invalid("source splice parent is not a Sequence"));
        };
        let prefix = children
            .get(..index)
            .ok_or_else(|| super::invalid("source splice slot is outside its Sequence"))?;
        let mut boundary = 0i64;
        for child in prefix {
            boundary = boundary
                .checked_add(durations[child].frames())
                .ok_or_else(super::overflow)?;
        }
        let anchors = AnchorIndex::from_durations(self, durations)?;
        let mut child = parent;
        let mut depth = 0usize;
        while child != self.root() {
            depth += 1;
            if depth > MAX_DOCUMENT_DEPTH {
                return Err(super::limit("source splice ancestry depth"));
            }
            let (ancestor, offset) = anchors
                .parents
                .get(child)
                .ok_or_else(|| super::invalid("source splice parent is outside the project"))?;
            if !matches!(self.nodes()[ancestor].kind, NodeKind::Sequence { .. }) {
                return Err(super::invalid(
                    "source splice cannot change a Repeat or Retime ancestor",
                ));
            }
            boundary = boundary.checked_add(*offset).ok_or_else(super::overflow)?;
            child = ancestor;
        }
        Ok(ProjectFrame(boundary))
    }
}

pub(crate) fn apply(
    document: &ProjectDocument,
    parent: &NodeId,
    index: usize,
    id: &NodeId,
    node: BeatNode,
    timing: &AudioTimingId,
    allocation: &RevisionId,
) -> Result<ProjectDocument, EditError> {
    let NodeKind::Source { source } = &node.kind else {
        return Err(super::invalid("source splice requires a Source leaf"));
    };
    if source.duration == FrameDuration::ZERO {
        return Err(EditError::new(
            EditErrorCode::InvalidDuration,
            "zero-length source splice changes no time or history",
        ));
    }
    if &timing.allocation != allocation {
        return Err(super::invalid(
            "source splice timing allocation must equal the new revision",
        ));
    }
    if document.nodes().contains_key(id) {
        return Err(EditError::new(
            EditErrorCode::IdentityConflict,
            "source splice identity must be fresh",
        ));
    }
    let at = document.source_splice_boundary(parent, index)?;
    let total = document.duration()?.frames();
    total
        .checked_add(source.duration.frames())
        .ok_or_else(super::overflow)?;
    // Capture only pre-existing owners. The new Source starts on the normal
    // zero-origin project grid, including its fractional first-sample phase.
    // The shared insertion path shifts each old owner's entry independently,
    // reconciles lineage and marks, and returns one unpublished candidate.
    super::composite::apply_at(
        document,
        parent,
        index,
        super::composite::Insertion {
            at,
            total,
            node,
            id,
            timing,
            allocation,
        },
    )
}
