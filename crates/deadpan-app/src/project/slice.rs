//! Immutable edited copies and service-issued media views for native placement.

use std::collections::BTreeMap;
use std::sync::Arc;

use deadpan_core::{CapturedEditSlice, FrameRange, NodeId, ProjectId, RevisionId};
use deadpan_store::slice_preview::AdmittedSliceView;

use super::{RegisteredSource, SequenceScope, splice};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CopyId {
    pub session: u64,
    pub project: ProjectId,
    pub source_revision: RevisionId,
    pub request: u64,
}

#[derive(Clone, Debug)]
pub struct CaptureRequest {
    pub id: CopyId,
    pub scope: SequenceScope,
    pub parent: NodeId,
    pub range: FrameRange,
}

/// Accepted content outlives later edits and Undo within its original session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Captured {
    pub(super) id: CopyId,
    pub(super) scope: SequenceScope,
    pub(super) slice: Arc<CapturedEditSlice>,
    /// Complete historical parent extent, bounding local In/Out refinement.
    pub(super) bounds: FrameRange,
}

impl Captured {
    pub fn id(&self) -> &CopyId {
        &self.id
    }

    pub fn scope(&self) -> &SequenceScope {
        &self.scope
    }

    pub fn slice(&self) -> &Arc<CapturedEditSlice> {
        &self.slice
    }

    pub fn bounds(&self) -> FrameRange {
        self.bounds
    }
}

#[derive(Clone, Debug)]
pub struct CaptureUpdate {
    pub id: CopyId,
    pub result: Result<Arc<Captured>, String>,
}

pub struct Paste {
    pub expected_session: u64,
    pub expected_revision: RevisionId,
    pub copied: Arc<Captured>,
    pub scope: SequenceScope,
    pub parent: NodeId,
    pub destination: splice::Destination,
}

/// The service derives this catalog from the exact opaque store admission.
/// It is deliberately separate from a committed Workspace and its history.
pub struct MediaView {
    pub(super) session: u64,
    pub(super) admitted: Arc<AdmittedSliceView>,
    pub(super) sources: BTreeMap<deadpan_core::AssetId, Arc<RegisteredSource>>,
}

impl MediaView {
    pub fn session(&self) -> u64 {
        self.session
    }

    pub fn admitted(&self) -> &Arc<AdmittedSliceView> {
        &self.admitted
    }

    pub fn sources(&self) -> &BTreeMap<deadpan_core::AssetId, Arc<RegisteredSource>> {
        &self.sources
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CopiedViewId {
    pub copy: CopyId,
    pub parent: NodeId,
    /// Global frames in the immutable historical Edit clock.
    pub range: FrameRange,
}

pub struct CopiedView {
    pub(super) id: CopiedViewId,
    pub(super) media: Arc<MediaView>,
}

impl CopiedView {
    pub fn id(&self) -> &CopiedViewId {
        &self.id
    }

    pub fn media(&self) -> &Arc<MediaView> {
        &self.media
    }
}

#[derive(Clone)]
pub struct SourceViewUpdate {
    pub id: CopiedViewId,
    pub result: Result<Arc<CopiedView>, String>,
}

#[cfg(test)]
pub(crate) fn test_media_view(
    session: u64,
    admitted: AdmittedSliceView,
) -> Result<Arc<MediaView>, String> {
    super::service::edit_slice::media_view(session, admitted)
}

#[cfg(test)]
pub(crate) fn test_copied_view(
    id: CopiedViewId,
    media: Arc<MediaView>,
) -> Result<Arc<CopiedView>, String> {
    if id.copy.session != media.session()
        || id.copy.project != *media.admitted().document().project_id()
        || id.copy.source_revision != *media.admitted().capture_revision()
        || id.range.duration()
            != media
                .admitted()
                .document()
                .duration()
                .map_err(|error| error.to_string())?
        || media.admitted().placement_base().is_some()
    {
        return Err("Copied test view differs from its admitted source".into());
    }
    Ok(Arc::new(CopiedView { id, media }))
}
