//! Store-admitted, immutable edited content for unsaved native previews.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use deadpan_core::{
    AssetId, AudioTimingId, CapturedEditSlice, Command, CommandRequest, EditError, EditErrorCode,
    NodeId, ProjectDocument, RevisionId, SlicePasteIdentities,
};

use crate::generated_media::GeneratedReadHandle;
use crate::original_media::{OriginalImportHandle, OriginalMediaRecord};
use crate::source_registration::SourceQualificationReceipt;
use crate::{ProjectStore, StoreError};

/// Host-issued scratch identities. Materialization never reserves durable history.
pub struct SliceViewIdentities {
    pub empty_revision: RevisionId,
    pub view_revision: RevisionId,
    pub root: NodeId,
    pub paste: SlicePasteIdentities,
}

#[derive(Clone)]
pub struct AdmittedSliceSource {
    pub receipt: Arc<SourceQualificationReceipt>,
    pub original: OriginalMediaRecord,
}

/// The document and its media evidence are issued together by the owning store.
/// This is a live capability, not a serializable source of media authority.
pub struct AdmittedSliceView {
    document: Arc<ProjectDocument>,
    placement_base: Option<Arc<ProjectDocument>>,
    capture_revision: RevisionId,
    sources: BTreeMap<AssetId, AdmittedSliceSource>,
    originals: OriginalImportHandle,
    generated: GeneratedReadHandle,
}

impl AdmittedSliceView {
    pub fn document(&self) -> &Arc<ProjectDocument> {
        &self.document
    }
    pub fn placement_base(&self) -> Option<&Arc<ProjectDocument>> {
        self.placement_base.as_ref()
    }
    pub fn capture_revision(&self) -> &RevisionId {
        &self.capture_revision
    }
    pub fn sources(&self) -> &BTreeMap<AssetId, AdmittedSliceSource> {
        &self.sources
    }
    pub fn originals(&self) -> &OriginalImportHandle {
        &self.originals
    }
    pub fn generated(&self) -> &GeneratedReadHandle {
        &self.generated
    }
    pub fn matches_originals(&self, handle: &OriginalImportHandle) -> bool {
        self.originals.same_session(handle)
    }
    /// A warm decoder does not extend the session that admitted it.
    pub fn check_live(&self, cancelled: &AtomicBool) -> Result<(), StoreError> {
        self.originals.check_live(cancelled)?;
        self.generated.check_live(cancelled)?;
        Ok(())
    }
}

impl ProjectStore {
    /// Prepare exactly one edited-slice placement without changing history.
    /// Commit still revalidates the retained CommandRequest in its transaction.
    pub fn preview_edit_slice(
        &self,
        request: &CommandRequest,
    ) -> Result<AdmittedSliceView, StoreError> {
        self.require_writer()?;
        let capture_revision = placement_source_revision(&request.command)?;
        let transaction = self.connection.unchecked_transaction()?;
        let plan = crate::prepare_command(&transaction, request)?;
        self.admit_slice_view(plan.next, Some(plan.current), capture_revision.clone())
    }

    /// Materialize only the copied ownership contribution in an empty neutral
    /// Sequence. All scratch identities come from the host; nothing is committed.
    pub fn view_edit_slice(
        &self,
        slice: &CapturedEditSlice,
        identities: SliceViewIdentities,
    ) -> Result<AdmittedSliceView, StoreError> {
        self.require_writer()?;
        let transaction = self.connection.unchecked_transaction()?;
        let current = crate::read_snapshot(&transaction)?;
        if current.project_id() != slice.project_id() {
            return Err(invalid("copied view belongs to another project"));
        }
        crate::ensure_unused_revision(&transaction, &identities.empty_revision)?;
        crate::ensure_unused_revision(&transaction, &identities.view_revision)?;
        let empty = ProjectDocument::new(
            slice.project_id().clone(),
            identities.empty_revision,
            slice.presentation_basis().clone(),
            identities.root.clone(),
        )?;
        let request = CommandRequest {
            project_id: empty.project_id().clone(),
            expected_revision: empty.revision_id().clone(),
            new_revision: identities.view_revision.clone(),
            command: Command::SpliceSlice {
                parent: identities.root,
                index: 0,
                slice: slice.clone(),
                identities: identities.paste,
                timing: AudioTimingId {
                    allocation: identities.view_revision,
                    ordinal: 0,
                },
            },
        };
        let captured = crate::slice_capture_revision(&transaction, &request)?
            .ok_or_else(|| invalid("copied view requires captured edit content"))?;
        let edit = deadpan_core::apply(&empty, &request)?;
        let document = edit.forward.apply(&empty)?;
        crate::check_document_size(&document.to_json()?)?;
        crate::check_document_size(&serde_json::to_string(&request)?)?;
        crate::check_document_size(&serde_json::to_string(&edit)?)?;
        crate::ensure_source_admission(None, &document, None, Some(&captured))?;
        crate::ensure_generated_admission_with(None, &document, None, Some(&captured))?;
        self.admit_slice_view(document, None, slice.revision_id().clone())
    }

    fn admit_slice_view(
        &self,
        document: ProjectDocument,
        placement_base: Option<ProjectDocument>,
        capture_revision: RevisionId,
    ) -> Result<AdmittedSliceView, StoreError> {
        let mut sources = BTreeMap::new();
        for (id, asset) in document.assets() {
            let Some(qualification) = &asset.source_qualification else {
                continue;
            };
            // Read the bounded receipt directly. registered_source would reread
            // the entire immutable document separately for every catalog entry.
            let receipt = self.source_qualification(qualification)?;
            if receipt.asset_record(asset.label.clone())? != *asset {
                return Err(StoreError::SourceRegistration(
                    "slice asset differs from its immutable qualification".into(),
                ));
            }
            let original = self
                .original_record(receipt.original().content())?
                .ok_or_else(|| {
                    StoreError::SourceRegistration("slice Original inventory is missing".into())
                })?;
            if original.object() != receipt.original()
                || original.sha256() != receipt.snapshot().content().sha256()
            {
                return Err(StoreError::SourceRegistration(
                    "slice Original inventory differs from its qualification".into(),
                ));
            }
            sources.insert(
                id.clone(),
                AdmittedSliceSource {
                    receipt: Arc::new(receipt),
                    original,
                },
            );
        }
        let view = AdmittedSliceView {
            document: Arc::new(document),
            placement_base: placement_base.map(Arc::new),
            capture_revision,
            sources,
            originals: self.original_import_handle()?,
            generated: self.generated_read_handle(),
        };
        view.check_live(&AtomicBool::new(false))?;
        Ok(view)
    }
}

fn placement_source_revision(command: &Command) -> Result<&RevisionId, StoreError> {
    match command {
        Command::SpliceSlice { slice, .. }
        | Command::SpliceSliceAt { slice, .. }
        | Command::ReplaceSlice { slice, .. } => Ok(slice.revision_id()),
        // prepare_command validates this current source revision, both scopes,
        // the complete identity pool and ordinary media authority. Unlike a
        // copied slice this branch grants no historical media exception.
        Command::MoveRange {
            source_revision, ..
        } => Ok(source_revision),
        _ => Err(invalid("edited preview requires an edited-slice placement")),
    }
}

fn invalid(message: &str) -> StoreError {
    EditError {
        code: EditErrorCode::InvalidCommand,
        message: message.into(),
        current_revision: None,
    }
    .into()
}
