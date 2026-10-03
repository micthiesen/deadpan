//! Identity-based sibling spans retain empty children at either endpoint.

use super::*;

/// Exact direct-child slots and their absolute Edit interval. `end` is exclusive;
/// a nonempty structural span can have a zero-duration interval.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SequenceChildrenPlan {
    pub first: usize,
    pub end: usize,
    pub range: FrameRange,
}

impl ProjectDocument {
    /// Resolve an inclusive, nonempty span of direct children in sibling order.
    /// The named parent and every ancestor must be ordinary Sequences. Equal
    /// endpoint identities select one whole child, including an empty Sequence.
    pub fn sequence_children(
        &self,
        parent: &NodeId,
        first: &NodeId,
        last: &NodeId,
    ) -> Result<SequenceChildrenPlan, EditError> {
        let parent_start = self.source_splice_boundary(parent, 0)?;
        let NodeKind::Sequence { children } = &self.nodes()[parent].kind else {
            unreachable!("boundary query admitted an ordinary Sequence")
        };
        let slot = |node: &NodeId| {
            children
                .iter()
                .position(|child| child == node)
                .ok_or_else(|| {
                    EditError::new(
                        EditErrorCode::SelectionUnavailable,
                        "sibling span endpoint is not a direct child of its named Sequence",
                    )
                })
        };
        let first = slot(first)?;
        let last = slot(last)?;
        if first > last {
            return Err(EditError::new(
                EditErrorCode::SelectionUnavailable,
                "sibling span endpoints are in reverse structural order",
            ));
        }
        // The validated child vector is bounded by MAX_DOCUMENT_NODES, so the
        // exclusive endpoint cannot overflow. Fold exact durations once, without
        // materializing occurrences or inferring membership from frame time.
        let end = last + 1;
        let durations = self.durations()?;
        let advance = |boundary: i64, child: &NodeId| {
            boundary
                .checked_add(durations[child].frames())
                .ok_or_else(overflow)
        };
        let start = children[..first].iter().try_fold(parent_start.0, advance)?;
        let stop = children[first..end].iter().try_fold(start, advance)?;
        Ok(SequenceChildrenPlan {
            first,
            end,
            range: FrameRange::new(ProjectFrame(start), ProjectFrame(stop))
                .map_err(DocumentError::from)?,
        })
    }
}

pub(super) fn validate(
    value: &SliceWire,
    durations: &BTreeMap<NodeId, FrameDuration>,
    first: &NodeId,
    last: &NodeId,
) -> Result<(), EditError> {
    if value.parts.first().is_none_or(|part| &part.root != first)
        || value.parts.last().is_none_or(|part| &part.root != last)
    {
        return Err(invalid(
            "children slice endpoints must match its first and last whole parts",
        ));
    }
    let mut roots = BTreeSet::new();
    for part in &value.parts {
        if !roots.insert(&part.root)
            || part.mapping.start() != ProjectFrame(0)
            || part.mapping.duration() != durations[&part.root]
        {
            return Err(invalid(
                "children slice requires distinct whole selected subtrees",
            ));
        }
    }
    if value.range.duration() == FrameDuration::ZERO
        && (value
            .nodes
            .values()
            .any(|node| !matches!(node.kind, NodeKind::Sequence { .. }))
            || value.audio_bindings != AudioBindingState::default())
    {
        return Err(invalid(
            "empty children slice requires a Sequence-only forest without audio bindings",
        ));
    }
    Ok(())
}
