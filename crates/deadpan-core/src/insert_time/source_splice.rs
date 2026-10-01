//! Source insertion at an explicit Sequence slot. Keeping the slot separate
//! from its project boundary preserves ownership at nested group edges.

use crate::{
    AnchorIndex, AudioTimingId, BeatNode, EditError, EditErrorCode, FrameDuration,
    MAX_DOCUMENT_DEPTH, MAX_DOCUMENT_NODES, NodeId, NodeKind, ProjectDocument, ProjectFrame,
    RevisionId, SplitIdentities,
};

/// Structural preflight for one strict interior of a named direct child.
/// The result grants no edit authority and contains no allocated identities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSpliceInterior {
    /// The target's child slot before Split. Insert at index+1 after Split.
    pub index: usize,
    pub boundary: ProjectFrame,
    /// IDs consumed by Split, excluding the new Source identity.
    pub required_ids: usize,
}

impl ProjectDocument {
    /// Keep the explicitly named ordinary Sequence owner. Only a Source,
    /// ordinary Hold or conservatively admitted transparent fragment can be
    /// split; this never descends a group or resolves a Repeat/Retime occurrence.
    pub fn source_splice_interior(
        &self,
        parent: &NodeId,
        target: &NodeId,
        at: FrameDuration,
    ) -> Result<SourceSpliceInterior, EditError> {
        self.splice_interior(parent, target, at, 1, false)
    }

    /// Preflight a copied-content insertion, including both Split and imported
    /// node counts. `required_ids` counts only the destination Split identities.
    pub fn slice_splice_interior(
        &self,
        parent: &NodeId,
        target: &NodeId,
        at: FrameDuration,
        slice: &crate::CapturedEditSlice,
    ) -> Result<SourceSpliceInterior, EditError> {
        slice.check_destination(self)?;
        if slice.duration() == FrameDuration::ZERO {
            return Err(super::invalid(
                "empty copied structure requires an explicit Sequence seam",
            ));
        }
        self.splice_interior(
            parent,
            target,
            at,
            slice.identity_requirements()?.nodes,
            true,
        )
    }

    fn splice_interior(
        &self,
        parent: &NodeId,
        target: &NodeId,
        at: FrameDuration,
        inserted_nodes: usize,
        nested_windows: bool,
    ) -> Result<SourceSpliceInterior, EditError> {
        let node = self.nodes().get(parent).ok_or_else(|| {
            EditError::new(
                EditErrorCode::SelectionUnavailable,
                "splice parent is missing",
            )
        })?;
        let NodeKind::Sequence { children } = &node.kind else {
            return Err(super::invalid("source splice parent is not a Sequence"));
        };
        let index = children
            .iter()
            .position(|child| child == target)
            .ok_or_else(|| {
                EditError::new(
                    EditErrorCode::SelectionUnavailable,
                    "source splice target is not a direct child of its named Sequence",
                )
            })?;
        let start = self.source_splice_boundary(parent, index)?;
        let duration = self.node_duration(target)?;
        if at == FrameDuration::ZERO || at >= duration {
            return Err(super::invalid(
                "source splice must be strictly inside its named child",
            ));
        }
        (if nested_windows { super::slice_physical(self, target) } else { super::physical(self, target) }).map_err(|error| EditError::new(
            error.code,
            "Slice placement here requires a Source, ordinary Hold or supported fragment; enter a group or choose a seam for other structures",
        ))?;
        let required_ids = super::split_node_count(self, target)?;
        if self
            .nodes()
            .len()
            .checked_add(required_ids)
            .and_then(|count| count.checked_add(inserted_nodes))
            .is_none_or(|count| count > MAX_DOCUMENT_NODES)
        {
            return Err(super::limit(
                "source splice exceeds the combined document node limit",
            ));
        }
        let boundary = ProjectFrame(
            start
                .0
                .checked_add(at.frames())
                .ok_or_else(super::overflow)?,
        );
        Ok(SourceSpliceInterior {
            index,
            boundary,
            required_ids,
        })
    }

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

pub(crate) struct InteriorInsertion<'a> {
    pub node: BeatNode,
    pub id: &'a NodeId,
    pub identities: &'a SplitIdentities,
    pub timing: &'a AudioTimingId,
}

pub(crate) fn apply_interior(
    document: &ProjectDocument,
    parent: &NodeId,
    target: &NodeId,
    at: FrameDuration,
    insertion: InteriorInsertion<'_>,
    context: crate::command::EditContext<'_>,
) -> Result<ProjectDocument, EditError> {
    let NodeKind::Source { source } = &insertion.node.kind else {
        return Err(super::invalid("source splice requires a Source leaf"));
    };
    if source.duration == FrameDuration::ZERO {
        return Err(EditError::new(
            EditErrorCode::InvalidDuration,
            "zero-length source splice changes no time or history",
        ));
    }
    if &insertion.timing.allocation != context.allocation {
        return Err(super::invalid(
            "source splice timing allocation must equal the new revision",
        ));
    }
    let resolved = document.source_splice_interior(parent, target, at)?;
    super::validate_identities(document, Some(insertion.id), insertion.identities)?;
    super::validate_split_budget(document, Some(target), insertion.identities)?;
    let placement_timing =
        AudioTimingId {
            allocation: insertion.timing.allocation.clone(),
            ordinal: insertion.timing.ordinal.checked_add(1).ok_or_else(|| {
                super::limit("interior source splice needs a second timing identity")
            })?,
        };
    let total = document.duration()?.frames();
    total
        .checked_add(source.duration.frames())
        .ok_or_else(super::overflow)?;
    let allocation = context.allocation;
    let mut working = document.clone();
    // Both retained copies must inherit the original sample lattice. Capture
    // before Split and pass the outer allowance relation through exactly once.
    working.audio_bindings = crate::audio_binding_lifecycle::capture_unbound_audio_bindings(
        document,
        insertion.timing.clone(),
    )?;
    working = crate::split::apply(&working, target, at, insertion.identities, context)?;
    super::composite::apply_at(
        &working,
        parent,
        resolved.index + 1,
        super::composite::Insertion {
            at: resolved.boundary,
            total,
            node: insertion.node,
            id: insertion.id,
            timing: &placement_timing,
            allocation,
        },
    )
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
