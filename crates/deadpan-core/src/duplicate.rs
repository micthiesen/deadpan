//! Duplicate a beat, sibling span or range of an ordinary Sequence in place.
//!
//! Specification §5.2/§5.4: a copy has new authored IDs, shares immutable
//! source media, has no hidden live link to the original, and duplicates the
//! selection's owned attachments. Duplicate captures the current revision with
//! the edited-slice capture and inserts the copy immediately after the
//! selection with the same placement commands as paste, so marks, beat sounds,
//! overrides, retained audio clocks and root sound transforms follow the
//! established copy rules exactly.

use serde::Serialize;

use crate::{
    AudioTimingId, CapturedEditSlice, Command, EditError, EditErrorCode, FrameDuration, NodeId,
    NodeKind, ProjectDocument, SliceCaptureSelection, SliceIdentityRequirements,
    SlicePasteIdentities, SplitIdentities,
};

/// Fresh identities a host supplies for [`Command::Duplicate`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct DuplicateRequirements {
    /// The paste pools: `identities.authored.nodes`, `.marks` and `.aliases`.
    pub slice: SliceIdentityRequirements,
    /// Split identities for a range ending strictly inside a beat; zero at a seam.
    pub split_nodes: usize,
}

/// Where the copy goes: an explicit seam, or strictly inside the beat
/// containing a range's end.
enum Destination {
    Seam(usize),
    Interior { target: NodeId, at: FrameDuration },
}

/// The capture name is internal scratch, renamed into imported timing
/// ordinals by placement. The allocation is this transaction's fresh revision.
fn scratch(timing: &AudioTimingId) -> AudioTimingId {
    AudioTimingId {
        allocation: timing.allocation.clone(),
        ordinal: u32::MAX,
    }
}

fn plan(
    document: &ProjectDocument,
    parent: &NodeId,
    selection: &SliceCaptureSelection,
    timing: &AudioTimingId,
) -> Result<(CapturedEditSlice, Destination), EditError> {
    let slice = CapturedEditSlice::capture_selection(document, parent, selection, scratch(timing))?;
    let NodeKind::Sequence { children } = &document.nodes()[parent].kind else {
        unreachable!("capture admits only ordinary Sequence parents")
    };
    let index_of = |node: &NodeId| {
        children
            .iter()
            .position(|child| child == node)
            .ok_or_else(|| {
                EditError::new(
                    EditErrorCode::SelectionUnavailable,
                    "duplicate target is not a direct child of its Sequence",
                )
            })
    };
    let destination = match selection {
        SliceCaptureSelection::Child { node } => Destination::Seam(index_of(node)? + 1),
        SliceCaptureSelection::Children { last, .. } => Destination::Seam(index_of(last)? + 1),
        SliceCaptureSelection::Range { range } => {
            let durations = document.durations()?;
            let mut start = document.source_splice_boundary(parent, 0)?.0;
            let mut found = None;
            for (index, child) in children.iter().enumerate() {
                let end = start
                    .checked_add(durations[child].frames())
                    .ok_or_else(|| {
                        EditError::new(EditErrorCode::TimingOverflow, "duplicate end")
                    })?;
                if start < range.end().0 && range.end().0 <= end && start != end {
                    found = Some(if end == range.end().0 {
                        Destination::Seam(index + 1)
                    } else {
                        Destination::Interior {
                            target: child.clone(),
                            at: FrameDuration::new(range.end().0 - start)
                                .map_err(crate::DocumentError::from)?,
                        }
                    });
                }
                start = end;
            }
            found.ok_or_else(|| {
                EditError::new(
                    EditErrorCode::SelectionUnavailable,
                    "duplicate range ends outside its Sequence",
                )
            })?
        }
    };
    Ok((slice, destination))
}

impl ProjectDocument {
    /// Exact fresh identity needs of [`Command::Duplicate`]. Read-only; the
    /// timing names the transaction's allocation.
    pub fn duplicate_requirements(
        &self,
        parent: &NodeId,
        selection: &SliceCaptureSelection,
        timing: &AudioTimingId,
    ) -> Result<DuplicateRequirements, EditError> {
        let (slice, destination) = plan(self, parent, selection, timing)?;
        let split_nodes = match &destination {
            Destination::Seam(_) => 0,
            Destination::Interior { target, at } => {
                self.slice_splice_interior(parent, target, *at, &slice)?
                    .required_ids
            }
        };
        Ok(DuplicateRequirements {
            slice: slice.identity_requirements()?,
            split_nodes,
        })
    }
}

/// The equivalent placement command for this exact revision. Hosts persist the
/// Duplicate request itself; replay resolves it again deterministically.
pub(crate) fn resolve(
    document: &ProjectDocument,
    parent: &NodeId,
    selection: &SliceCaptureSelection,
    identities: &SlicePasteIdentities,
    split_identities: &SplitIdentities,
    timing: &AudioTimingId,
) -> Result<Command, EditError> {
    let (slice, destination) = plan(document, parent, selection, timing)?;
    Ok(match destination {
        Destination::Seam(index) => {
            if !split_identities.nodes.is_empty() {
                return Err(EditError::new(
                    EditErrorCode::InvalidCommand,
                    "a duplicate at a seam takes no Split identities",
                ));
            }
            Command::SpliceSlice {
                parent: parent.clone(),
                index,
                slice,
                identities: identities.clone(),
                timing: timing.clone(),
            }
        }
        Destination::Interior { target, at } => Command::SpliceSliceAt {
            parent: parent.clone(),
            target,
            at,
            slice,
            identities: identities.clone(),
            split_identities: split_identities.clone(),
            timing: timing.clone(),
        },
    })
}
