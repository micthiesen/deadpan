//! Descriptive Slip previews and stored media admission share command preparation.

use super::*;
use deadpan_core::{NodeKind, SourceSlipResolution, SourceVideo};

/// Exact handle limits and the single atomic edit, if the clamped delta changes
/// media. This report supplies no authority to bypass admission on commit.
#[derive(Debug, Serialize)]
pub struct SourceSlipPreview {
    pub resolution: SourceSlipResolution,
    pub edit: Option<EditTransaction>,
}

impl ProjectStore {
    /// Resolve against one captured SQLite snapshot, including stored receipt
    /// and ownership checks even when the requested Slip resolves to zero.
    /// Commit must submit the original revision-bound CommandRequest again.
    pub fn preview_source_slip(
        &self,
        request: &CommandRequest,
    ) -> Result<SourceSlipPreview, StoreError> {
        let transaction = self.connection.unchecked_transaction()?;
        let current = crate::read_command_snapshot(&transaction, request)?;
        let Command::SlipSource {
            parent,
            node,
            delta_frames,
        } = &request.command
        else {
            return Err(invalid("source slip preview requires a SlipSource command"));
        };
        let resolution = current.source_slip(parent, node, *delta_frames)?;
        let edit = if resolution.applied_delta_frames == 0 {
            crate::ensure_unused_revision(&transaction, &request.new_revision)?;
            validate_source_slip(&transaction, &current, &current, request)?;
            None
        } else {
            Some(
                crate::prepare_current_command_with_admission(
                    &transaction,
                    current,
                    request,
                    None,
                    None,
                    None,
                )?
                .edit,
            )
        };
        Ok(SourceSlipPreview { resolution, edit })
    }
}

/// Check persisted admission only. Original bytes remain the responsibility of
/// fresh playback/export snapshots; ordinary editing performs no media I/O.
pub(crate) fn validate_source_slip(
    connection: &Connection,
    current: &ProjectDocument,
    next: &ProjectDocument,
    request: &CommandRequest,
) -> Result<(), StoreError> {
    let Command::SlipSource {
        parent,
        node,
        delta_frames,
    } = &request.command
    else {
        return Ok(());
    };
    let resolution = current.source_slip(parent, node, *delta_frames)?;
    let record = current
        .assets()
        .get(&resolution.asset)
        .ok_or_else(|| invalid("source slip asset is absent from the selected revision"))?;
    if next.assets().get(&resolution.asset) != Some(record) {
        return Err(invalid("source slip changes its admitted asset contract"));
    }
    let id = record
        .source_qualification
        .as_ref()
        .ok_or_else(|| invalid("source slip asset has no measured source qualification"))?;
    if id != &resolution.qualification {
        return Err(invalid(
            "source slip resolution differs from its asset qualification",
        ));
    }
    let receipt = read_receipt(connection, id)?
        .ok_or_else(|| invalid("source slip qualification is missing"))?;
    if receipt.asset_record(record.label.clone())? != *record {
        return Err(invalid(
            "source slip asset metadata disagrees with its qualification",
        ));
    }
    check_original_binding(connection, &receipt)?;
    let Some(NodeKind::Source { source }) = next
        .nodes()
        .get(&resolution.physical_source)
        .map(|node| &node.kind)
    else {
        return Err(invalid("source slip candidate lost its physical Source"));
    };
    if source != &resolution.after {
        return Err(invalid(
            "source slip candidate differs from the resolved linked edit",
        ));
    }
    for source in [&resolution.before, source] {
        let SourceVideo::Stream { asset, span } = &source.video else {
            return Err(invalid("source slip requires measured video"));
        };
        if asset != &resolution.asset || Some(*span) != record.video {
            return Err(invalid(
                "source slip video differs from the complete measured context",
            ));
        }
        if let Some(audio) = &source.audio
            && (audio.asset != resolution.asset || Some(audio.span) != record.audio)
        {
            return Err(invalid(
                "source slip audio differs from the complete measured context",
            ));
        }
    }
    Ok(())
}
