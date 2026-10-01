//! The preview and fast-paste paths retain the same qualified store requests.

use std::ops::Range;
use std::sync::atomic::AtomicBool;

use deadpan_core::{
    AudioTimingId, CapturedEditSlice, Command, CommandRequest, EditTransaction, FrameDuration,
    FrameRange, MoveRangeDestination, ProjectFrame, SplitIdentities,
};
use deadpan_store::source_registration::{
    SourceMomentInsertionRequest, SourceMomentInteriorInsertionRequest,
    SourceMomentReplacementRequest,
};
use deadpan_store::{CommitOutcome, ProjectStore, StoreError};

use super::super::{AssetId, NodeId, PreparedSourceRegistration, Result, display, node, revision};
use crate::project::Workspace;
use crate::project::splice::{Destination, Movement};

pub(in crate::project::service) enum Request {
    Slot(SourceMomentInsertionRequest),
    Interior(SourceMomentInteriorInsertionRequest),
    Replace(SourceMomentReplacementRequest),
    Edited {
        request: Box<CommandRequest>,
        node: NodeId,
        duration: FrameDuration,
        removed: Option<FrameRange>,
    },
    Move {
        request: Box<CommandRequest>,
        inserted: FrameRange,
        movement: Movement,
    },
}

impl Request {
    pub(super) fn move_edited(
        workspace: &Workspace,
        parent: &NodeId,
        destination: &Destination,
        slice: &CapturedEditSlice,
    ) -> Result<(Self, ProjectFrame)> {
        if slice.duration() == FrameDuration::ZERO {
            return Err(
                "Move requires picture time. Use cut and paste to move an empty group.".into(),
            );
        }
        if slice.revision_id() != workspace.document.revision_id() {
            return Err(
                "Copied Edit is from an older revision; select and copy again to move".into(),
            );
        }
        let destination = match destination {
            Destination::Slot(index) => MoveRangeDestination::Seam {
                parent: parent.clone(),
                index: *index,
            },
            Destination::Interior { target, at } => MoveRangeDestination::Interior {
                parent: parent.clone(),
                target: target.clone(),
                at: *at,
            },
            Destination::Replace { .. } => {
                return Err(
                    "Move cannot replace a selection; choose an insertion destination".into(),
                );
            }
        };
        let plan = workspace
            .document
            .range_move(slice.parent(), slice.range(), &destination)
            .map_err(display)?;
        if plan.is_noop {
            return Err("Already at this position; no change".into());
        }
        let new_revision = revision();
        let command = Command::MoveRange {
            source_revision: slice.revision_id().clone(),
            source_parent: slice.parent().clone(),
            range: slice.range(),
            destination,
            identities: SplitIdentities {
                nodes: (0..plan.required_ids).map(|_| node()).collect(),
            },
            timing: AudioTimingId {
                allocation: new_revision.clone(),
                ordinal: 0,
            },
        };
        Ok((
            Self::Move {
                request: Box::new(CommandRequest {
                    project_id: workspace.document.project_id().clone(),
                    expected_revision: workspace.document.revision_id().clone(),
                    new_revision,
                    command,
                }),
                inserted: plan.inserted,
                movement: Movement {
                    source_parent: slice.parent().clone(),
                    source_before: slice.range(),
                    destination_before: plan.destination_before,
                    removal_after: plan.removal_join,
                },
            },
            plan.inserted.start(),
        ))
    }

    pub(in crate::project::service) fn edited(
        workspace: &Workspace,
        parent: &NodeId,
        destination: &Destination,
        slice: &CapturedEditSlice,
    ) -> Result<(Self, ProjectFrame)> {
        let (cursor, required_ids) = match destination {
            Destination::Slot(index) => (
                workspace
                    .document
                    .source_splice_boundary(parent, *index)
                    .map_err(display)?,
                0,
            ),
            Destination::Interior { target, at } => {
                let target = workspace
                    .document
                    .slice_splice_interior(parent, target, *at, slice)
                    .map_err(display)?;
                (target.boundary, target.required_ids)
            }
            Destination::Replace { range } => {
                let target = workspace
                    .document
                    .slice_replacement(parent, *range, slice)
                    .map_err(display)?;
                (target.range.start(), target.required_ids)
            }
        };
        let identities = super::super::edit_slice::paste_identities(slice)?;
        let node = identities
            .authored
            .nodes
            .first()
            .cloned()
            .ok_or("Copied slice has no imported root")?;
        let split_identities = SplitIdentities {
            nodes: (0..required_ids).map(|_| super::super::node()).collect(),
        };
        let new_revision = revision();
        let timing = AudioTimingId {
            allocation: new_revision.clone(),
            ordinal: 0,
        };
        let command = match destination {
            Destination::Slot(index) => Command::SpliceSlice {
                parent: parent.clone(),
                index: *index,
                slice: slice.clone(),
                identities,
                timing,
            },
            Destination::Interior { target, at } => Command::SpliceSliceAt {
                parent: parent.clone(),
                target: target.clone(),
                at: *at,
                slice: slice.clone(),
                identities,
                split_identities,
                timing,
            },
            Destination::Replace { range } => Command::ReplaceSlice {
                parent: parent.clone(),
                range: *range,
                slice: slice.clone(),
                identities,
                split_identities,
                timing,
            },
        };
        let removed = match destination {
            Destination::Replace { range } => Some(*range),
            _ => None,
        };
        Ok((
            Self::Edited {
                request: Box::new(CommandRequest {
                    project_id: workspace.document.project_id().clone(),
                    expected_revision: workspace.document.revision_id().clone(),
                    new_revision,
                    command,
                }),
                node,
                duration: slice.duration(),
                removed,
            },
            cursor,
        ))
    }

    pub(in crate::project::service) fn capture(
        workspace: &Workspace,
        parent: &NodeId,
        destination: &Destination,
        asset: &AssetId,
        ordinals: Range<u64>,
        source_label: &str,
    ) -> Result<(Self, ProjectFrame)> {
        if ordinals.start >= ordinals.end {
            return Err("Select a nonempty Original slice".into());
        }
        let (cursor, required_ids) = match destination {
            Destination::Slot(index) => (
                workspace
                    .document
                    .source_splice_boundary(parent, *index)
                    .map_err(display)?,
                0,
            ),
            Destination::Interior { target, at } => {
                let target = workspace
                    .document
                    .source_splice_interior(parent, target, *at)
                    .map_err(display)?;
                (target.boundary, target.required_ids)
            }
            Destination::Replace { range } => {
                let target = workspace
                    .document
                    .source_replacement(parent, *range)
                    .map_err(display)?;
                (target.range.start(), target.required_ids)
            }
        };
        let expected_revision = workspace.document.revision_id().clone();
        let new_revision = revision();
        let node = node();
        let label = format!("{source_label} [{}..{})", ordinals.start, ordinals.end);
        let timing = AudioTimingId {
            allocation: new_revision.clone(),
            ordinal: 0,
        };
        let identities = SplitIdentities {
            nodes: (0..required_ids).map(|_| super::super::node()).collect(),
        };
        let request = match destination {
            Destination::Slot(index) => Self::Slot(SourceMomentInsertionRequest {
                expected_revision,
                new_revision,
                asset: asset.clone(),
                parent: parent.clone(),
                index: *index,
                node,
                label,
                timing,
                ordinals,
            }),
            Destination::Interior { target, at } => {
                Self::Interior(SourceMomentInteriorInsertionRequest {
                    expected_revision,
                    new_revision,
                    asset: asset.clone(),
                    parent: parent.clone(),
                    target: target.clone(),
                    at: *at,
                    node,
                    label,
                    identities,
                    timing,
                    ordinals,
                })
            }
            Destination::Replace { range } => Self::Replace(SourceMomentReplacementRequest {
                expected_revision,
                new_revision,
                asset: asset.clone(),
                parent: parent.clone(),
                range: *range,
                node,
                label,
                identities,
                timing,
                ordinals,
            }),
        };
        Ok((request, cursor))
    }

    pub(in crate::project::service) fn node(&self) -> Option<&NodeId> {
        match self {
            Self::Slot(request) => Some(&request.node),
            Self::Interior(request) => Some(&request.node),
            Self::Replace(request) => Some(&request.node),
            Self::Edited { node, .. } => Some(node),
            Self::Move { .. } => None,
        }
    }

    pub(in crate::project::service) fn asset(&self) -> Option<&AssetId> {
        match self {
            Self::Slot(request) => Some(&request.asset),
            Self::Interior(request) => Some(&request.asset),
            Self::Replace(request) => Some(&request.asset),
            Self::Edited { .. } | Self::Move { .. } => None,
        }
    }

    pub(super) fn edited_command(&self) -> Option<&CommandRequest> {
        match self {
            Self::Edited { request, .. } | Self::Move { request, .. } => Some(request),
            _ => None,
        }
    }

    pub(super) fn inserted_duration(&self) -> Option<FrameDuration> {
        match self {
            Self::Edited { duration, .. } => Some(*duration),
            Self::Move { inserted, .. } => Some(inserted.duration()),
            _ => None,
        }
    }

    pub(super) fn removed(&self) -> Option<FrameRange> {
        match self {
            Self::Replace(request) => Some(request.range),
            Self::Slot(_) | Self::Interior(_) | Self::Move { .. } => None,
            Self::Edited { removed, .. } => *removed,
        }
    }

    pub(super) fn movement(&self) -> Option<&Movement> {
        match self {
            Self::Move { movement, .. } => Some(movement),
            _ => None,
        }
    }

    pub(super) fn preview(
        &self,
        store: &ProjectStore,
        source: &PreparedSourceRegistration,
        cancelled: &AtomicBool,
    ) -> std::result::Result<EditTransaction, StoreError> {
        match self {
            Self::Slot(request) => store.preview_prepared_source_moment(request, source, cancelled),
            Self::Interior(request) => {
                store.preview_prepared_source_moment_interior(request, source, cancelled)
            }
            Self::Replace(request) => {
                store.preview_prepared_source_replacement(request, source, cancelled)
            }
            Self::Edited { request, .. } | Self::Move { request, .. } => store.preview(request),
        }
    }

    pub(in crate::project::service) fn commit(
        &self,
        store: &mut ProjectStore,
        source: Option<&PreparedSourceRegistration>,
        cancelled: &AtomicBool,
    ) -> std::result::Result<CommitOutcome, StoreError> {
        match self {
            Self::Slot(request) => store.commit_prepared_source_moment(
                request,
                require_source(source)?,
                None,
                cancelled,
            ),
            Self::Interior(request) => store.commit_prepared_source_moment_interior(
                request,
                require_source(source)?,
                None,
                cancelled,
            ),
            Self::Replace(request) => store.commit_prepared_source_replacement(
                request,
                require_source(source)?,
                None,
                cancelled,
            ),
            Self::Edited { request, .. } | Self::Move { request, .. } => store.commit(request),
        }
    }
}

fn require_source(
    source: Option<&PreparedSourceRegistration>,
) -> std::result::Result<&PreparedSourceRegistration, StoreError> {
    source.ok_or_else(|| {
        StoreError::SourceRegistration("Original paste has no prepared source".into())
    })
}
