//! Historical edited copies and store-sealed native source views.

use deadpan_core::{
    AudioTimingId, CapturedEditSlice, FrameRange, MarkId, OccurrenceIdentities, ProjectFrame,
    SlicePasteIdentities,
};
use deadpan_store::slice_preview::{AdmittedSliceView, SliceViewIdentities};

use super::*;
use crate::project::slice::{
    CaptureRequest, CaptureUpdate, Captured, CopiedView, CopiedViewId, CopyId, MediaView, Paste,
    SourceViewUpdate,
};

pub(super) struct PreparedCopy {
    copied: Arc<Captured>,
    pub view: Arc<CopiedView>,
    pub slice: Arc<CapturedEditSlice>,
}

impl Service {
    pub(super) fn clear_copied_slice(&mut self) {
        self.macros = None;
        self.saved_macro = None;
        self.captured_slice = None;
        self.registers = None;
        self.captured_original = None;
        self.cut_slice = None;
        self.last_cut = None;
        self.copied_view = None;
        self.splice_source_view = None;
    }

    pub(super) fn capture_edit_slice_command(&mut self, request: CaptureRequest) {
        let result = self.save_edit_slice(&request);
        self.captured_slice = Some(CaptureUpdate {
            id: request.id,
            result,
        });
    }

    pub(super) fn check_copy_context(&self, id: &CopyId) -> Result<()> {
        let workspace = self.workspace.as_ref().ok_or("Open a project first")?;
        if id.session == 0 || id.request == 0 {
            return Err("Copy identities must be nonzero".into());
        }
        if workspace.session != id.session
            || workspace.document.project_id() != &id.project
            || self.pending_session_change.is_some()
        {
            return Err("Copied Edit belongs to a different project session".into());
        }
        Ok(())
    }

    pub(super) fn capture_edit_slice(&self, request: &CaptureRequest) -> Result<Arc<Captured>> {
        self.check_copy_context(&request.id)?;
        let document = self
            .store
            .as_ref()
            .ok_or("Open a project first")?
            .snapshot_at(&request.id.source_revision)
            .map_err(display)?;
        if document.project_id() != &request.id.project {
            return Err("Copied Edit belongs to another project".into());
        }
        let plan = RenderPlan::compile(&document).map_err(display)?;
        let owner = request.scope.resolve_document(&document, &plan)?;
        if owner.owner != &request.parent {
            return Err("Copy range is outside the captured Sequence scope".into());
        }
        let bounds = FrameRange::new(
            ProjectFrame(i64::try_from(owner.start).map_err(display)?),
            ProjectFrame(i64::try_from(owner.end).map_err(display)?),
        )
        .map_err(display)?;
        let slice = Arc::new(
            CapturedEditSlice::capture_selection(
                &document,
                &request.parent,
                &request.selection,
                AudioTimingId {
                    allocation: revision(),
                    ordinal: 0,
                },
            )
            .map_err(display)?,
        );
        self.check_copy_context(&request.id)?;
        let source_path = std::iter::once("Your edit".to_owned())
            .chain(
                request
                    .scope
                    .groups()
                    .iter()
                    .map(|id| document.nodes()[id].label.clone()),
            )
            .collect();
        let child_label = match &request.selection {
            deadpan_core::SliceCaptureSelection::Range { .. } => None,
            deadpan_core::SliceCaptureSelection::Child { node } => {
                Some(document.nodes()[node].label.clone())
            }
        };
        Ok(Arc::new(Captured {
            id: request.id.clone(),
            scope: request.scope.clone(),
            slice,
            bounds,
            source_path,
            child_label,
        }))
    }

    fn check_copied(&self, copied: &Captured) -> Result<()> {
        self.check_copy_context(copied.id())?;
        let slice = copied.slice();
        if slice.project_id() != &copied.id().project
            || slice.revision_id() != &copied.id().source_revision
        {
            return Err("Copied Edit identity differs from its capture".into());
        }
        if self.workspace.as_ref().is_none_or(|workspace| {
            workspace.document.presentation_basis() != slice.presentation_basis()
        }) {
            return Err("Copied Edit presentation basis differs from this project".into());
        }
        Ok(())
    }

    fn validate_copy(&self, copied: &Captured) -> Result<ProjectDocument> {
        self.check_copied(copied)?;
        let slice = copied.slice();
        let document = self
            .store
            .as_ref()
            .ok_or("Open a project first")?
            .capture_snapshot_at(slice.revision_id())
            .map_err(display)?;
        slice.validate_capture(&document).map_err(display)?;
        let plan = RenderPlan::compile(&document).map_err(display)?;
        let owner = copied.scope().resolve_document(&document, &plan)?;
        if owner.owner != slice.parent()
            || i64::try_from(owner.start).map_err(display)? != copied.bounds().start().0
            || i64::try_from(owner.end).map_err(display)? != copied.bounds().end().0
        {
            return Err("Copied Edit scope differs from its historical parent".into());
        }
        Ok(document)
    }

    /// Publish source admission independently before attempting the destination.
    pub(super) fn prepare_copied_view(
        &mut self,
        copied: &Arc<Captured>,
        range: FrameRange,
    ) -> Result<Arc<CapturedEditSlice>> {
        let id = CopiedViewId {
            copy: copied.id().clone(),
            parent: copied.slice().parent().clone(),
            range,
        };
        let result = self.build_copied_view(copied, id.clone());
        self.splice_source_view = Some(SourceViewUpdate {
            id,
            result: result
                .as_ref()
                .map(|prepared| prepared.view.clone())
                .map_err(Clone::clone),
        });
        let prepared = result?;
        let slice = prepared.slice.clone();
        self.copied_view = Some(prepared);
        Ok(slice)
    }

    fn build_copied_view(&self, copied: &Arc<Captured>, id: CopiedViewId) -> Result<PreparedCopy> {
        self.check_copied(copied)?;
        if let Some(prepared) = &self.copied_view
            && prepared.view.id() == &id
            && Arc::ptr_eq(&prepared.copied, copied)
        {
            prepared
                .view
                .media()
                .admitted()
                .check_live(&AtomicBool::new(false))
                .map_err(display)?;
            return Ok(PreparedCopy {
                copied: copied.clone(),
                view: prepared.view.clone(),
                slice: prepared.slice.clone(),
            });
        }
        let document = self.validate_copy(copied)?;
        let slice = if id.range == copied.slice().range() {
            copied.slice().clone()
        } else {
            check_range(copied.bounds(), id.range)?;
            Arc::new(
                CapturedEditSlice::capture(
                    &document,
                    copied.slice().parent(),
                    id.range,
                    AudioTimingId {
                        allocation: revision(),
                        ordinal: 0,
                    },
                )
                .map_err(display)?,
            )
        };
        let admitted = self
            .store
            .as_ref()
            .ok_or("Open a project first")?
            .view_edit_slice(
                &slice,
                SliceViewIdentities {
                    empty_revision: revision(),
                    view_revision: revision(),
                    root: node(),
                    paste: paste_identities(&slice)?,
                },
            )
            .map_err(display)?;
        let media = self.slice_media_view(admitted)?;
        self.check_copy_context(copied.id())?;
        Ok(PreparedCopy {
            copied: copied.clone(),
            view: Arc::new(CopiedView { id, media }),
            slice,
        })
    }

    pub(super) fn slice_media_view(&self, admitted: AdmittedSliceView) -> Result<Arc<MediaView>> {
        let workspace = self.workspace.as_ref().ok_or("Open a project first")?;
        if admitted.document().project_id() != workspace.document.project_id()
            || !admitted.matches_originals(&workspace.originals)
        {
            return Err("Copied media belongs to a different project session".into());
        }
        media_view(workspace.session, admitted)
    }

    pub(super) fn paste_edited_slice(&mut self, paste: Paste) -> Result<()> {
        self.check_context(paste.expected_session, &paste.expected_revision)?;
        self.validate_copy(&paste.copied)?;
        let workspace = self.workspace.as_ref().ok_or("Open a project first")?;
        if paste.scope.resolve(workspace)?.owner != &paste.parent {
            return Err("Paste target is outside the captured Sequence scope".into());
        }
        let (request, cursor) = splice::Request::edited(
            workspace,
            &paste.parent,
            &paste.destination,
            paste.copied.slice(),
        )?;
        let outcome = request
            .commit(self.writer()?, None, &AtomicBool::new(false))
            .map_err(display)?;
        self.complete_slice_placement(
            &request,
            cursor,
            paste.scope,
            outcome.revision_id,
            "Edited slice",
        );
        Ok(())
    }
}

pub(in crate::project) fn media_view(
    session: u64,
    admitted: AdmittedSliceView,
) -> Result<Arc<MediaView>> {
    if session == 0 {
        return Err("Copied media requires a nonzero session".into());
    }
    admitted
        .check_live(&AtomicBool::new(false))
        .map_err(display)?;
    let document = admitted.document();
    let sources = admitted
        .sources()
        .iter()
        .map(|(asset, source)| {
            let metadata = document
                .assets()
                .get(asset)
                .ok_or("Copied asset is missing")?;
            registered_source(
                asset,
                metadata,
                document.presentation_basis().frame_rate,
                source.receipt.clone(),
                source.original.clone(),
            )
            .map(|source| (asset.clone(), source))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    Ok(Arc::new(MediaView {
        session,
        admitted: Arc::new(admitted),
        sources,
    }))
}

fn check_range(bounds: FrameRange, range: FrameRange) -> Result<()> {
    if range.start() >= range.end() || range.start() < bounds.start() || range.end() > bounds.end()
    {
        return Err("Select a nonempty Edit range inside the captured Sequence".into());
    }
    Ok(())
}

pub(super) fn paste_identities(slice: &CapturedEditSlice) -> Result<SlicePasteIdentities> {
    let required = slice.identity_requirements().map_err(display)?;
    Ok(SlicePasteIdentities {
        authored: OccurrenceIdentities {
            nodes: (0..required.nodes).map(|_| node()).collect(),
            marks: (0..required.marks)
                .map(|_| {
                    MarkId::new(uuid::Uuid::new_v4().to_string())
                        .expect("UUID is a valid mark identity")
                })
                .collect(),
        },
        aliases: (0..required.aliases).map(|_| node()).collect(),
    })
}
