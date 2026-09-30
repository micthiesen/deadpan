//! The preview and fast-paste paths retain the same qualified store requests.

use std::ops::Range;
use std::sync::atomic::AtomicBool;

use deadpan_core::{AudioTimingId, EditTransaction, FrameRange, ProjectFrame, SplitIdentities};
use deadpan_store::source_registration::{
    SourceMomentInsertionRequest, SourceMomentInteriorInsertionRequest,
    SourceMomentReplacementRequest,
};
use deadpan_store::{CommitOutcome, ProjectStore, StoreError};

use super::super::{AssetId, NodeId, PreparedSourceRegistration, Result, display, node, revision};
use crate::project::Workspace;
use crate::project::splice::Destination;

pub(in crate::project::service) enum Request {
    Slot(SourceMomentInsertionRequest),
    Interior(SourceMomentInteriorInsertionRequest),
    Replace(SourceMomentReplacementRequest),
}

impl Request {
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

    pub(in crate::project::service) fn node(&self) -> &NodeId {
        match self {
            Self::Slot(request) => &request.node,
            Self::Interior(request) => &request.node,
            Self::Replace(request) => &request.node,
        }
    }

    pub(in crate::project::service) fn asset(&self) -> &AssetId {
        match self {
            Self::Slot(request) => &request.asset,
            Self::Interior(request) => &request.asset,
            Self::Replace(request) => &request.asset,
        }
    }

    pub(super) fn removed(&self) -> Option<FrameRange> {
        match self {
            Self::Replace(request) => Some(request.range),
            Self::Slot(_) | Self::Interior(_) => None,
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
        }
    }

    pub(in crate::project::service) fn commit(
        &self,
        store: &mut ProjectStore,
        source: &PreparedSourceRegistration,
        cancelled: &AtomicBool,
    ) -> std::result::Result<CommitOutcome, StoreError> {
        match self {
            Self::Slot(request) => {
                store.commit_prepared_source_moment(request, source, None, cancelled)
            }
            Self::Interior(request) => {
                store.commit_prepared_source_moment_interior(request, source, None, cancelled)
            }
            Self::Replace(request) => {
                store.commit_prepared_source_replacement(request, source, None, cancelled)
            }
        }
    }
}
