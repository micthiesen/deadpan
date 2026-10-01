//! Stored media admission for one complete accepted Trim intent.

use super::*;
use deadpan_core::{NodeKind, SourceTrimEditResolution, SourceTrimOwnerGeometry, SourceVideo};

#[derive(Debug, Serialize)]
pub struct SourceTrimEditPreview {
    pub resolution: SourceTrimEditResolution,
    pub edit: Option<EditTransaction>,
}

impl ProjectStore {
    /// Preview against one consistent entry revision. The descriptive result
    /// cannot replace the command or its stored qualification checks at commit.
    pub fn preview_source_trim_edit(
        &self,
        request: &CommandRequest,
    ) -> Result<SourceTrimEditPreview, StoreError> {
        let transaction = self.connection.unchecked_transaction()?;
        let current = crate::read_command_snapshot(&transaction, request)?;
        let Command::ApplySourceTrim {
            parent,
            node,
            right,
            intent,
            ..
        } = &request.command
        else {
            return Err(invalid("combined Trim preview requires ApplySourceTrim"));
        };
        let resolution = current.source_trim_edit(parent, node, right.as_ref(), *intent)?;
        let edit = if intent.is_zero() {
            crate::ensure_unused_revision(&transaction, &request.new_revision)?;
            validate_source_trim_edit(&transaction, &current, &current, request)?;
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
        Ok(SourceTrimEditPreview { resolution, edit })
    }
}

pub(crate) fn validate_source_trim_edit(
    connection: &Connection,
    current: &ProjectDocument,
    next: &ProjectDocument,
    request: &CommandRequest,
) -> Result<(), StoreError> {
    let Command::ApplySourceTrim {
        parent,
        node,
        right,
        intent,
        resources,
    } = &request.command
    else {
        return Ok(());
    };
    let resolution = current.source_trim_edit(parent, node, right.as_ref(), *intent)?;
    // Nonzero requests have already passed the core's complete resource and
    // candidate validation. The no-op preview needs equivalent empty-pool
    // admission even though it intentionally creates no transaction.
    resolution.result_identities(resources)?;
    if let Some(timing) = &resources.timing
        && timing.allocation != request.new_revision
    {
        return Err(invalid(
            "combined Trim timing must belong to its new revision",
        ));
    }
    if intent.is_zero() && next != current {
        return Err(invalid("zero combined Trim must preserve the document"));
    }
    if next.duration()? != resolution.geometry.project_duration_after {
        return Err(invalid(
            "combined Trim candidate has the wrong project duration",
        ));
    }
    validate_owner(connection, current, next, &resolution.geometry.target, true)?;
    if (intent.roll_frames != 0 || resolution.footprint.is_some())
        && let Some(side) = &resolution.geometry.right
    {
        // Even a fully consumed B supplied the accepted Roll/overlay geometry.
        // Its immutable measured context must still be admitted before removal.
        validate_owner(
            connection,
            current,
            next,
            side,
            resolution.right_after.is_some(),
        )?;
    }
    Ok(())
}

fn validate_owner(
    connection: &Connection,
    current: &ProjectDocument,
    next: &ProjectDocument,
    side: &SourceTrimOwnerGeometry,
    retained: bool,
) -> Result<(), StoreError> {
    let record = current
        .assets()
        .get(&side.asset)
        .ok_or_else(|| invalid("combined Trim asset is absent from its entry revision"))?;
    if next.assets().get(&side.asset) != Some(record) {
        return Err(invalid("combined Trim changes its admitted asset contract"));
    }
    let qualification = record
        .source_qualification
        .as_ref()
        .ok_or_else(|| invalid("combined Trim asset has no measured qualification"))?;
    if qualification != &side.qualification {
        return Err(invalid(
            "combined Trim geometry names a different qualification",
        ));
    }
    let receipt = read_receipt(connection, qualification)?
        .ok_or_else(|| invalid("combined Trim qualification is missing"))?;
    if receipt.asset_record(record.label.clone())? != *record {
        return Err(invalid(
            "combined Trim asset differs from its qualification",
        ));
    }
    check_original_binding(connection, &receipt)?;
    let Some(NodeKind::Source { source: before }) = current
        .nodes()
        .get(&side.physical_source)
        .map(|beat| &beat.kind)
    else {
        return Err(invalid("combined Trim entry lost its physical Source"));
    };
    if before != &side.before {
        return Err(invalid(
            "combined Trim entry differs from its resolved Source",
        ));
    }
    for source in [&side.before, &side.after] {
        let SourceVideo::Stream { asset, span } = &source.video else {
            return Err(invalid("combined Trim requires measured picture context"));
        };
        if asset != &side.asset || Some(*span) != record.video {
            return Err(invalid(
                "combined Trim picture is not the complete measured context",
            ));
        }
        if let Some(audio) = &source.audio
            && (audio.asset != side.asset || Some(audio.span) != record.audio)
        {
            return Err(invalid(
                "combined Trim audio is not the complete measured context",
            ));
        }
    }
    match next.nodes().get(&side.physical_source) {
        Some(beat) if retained => {
            if !matches!(&beat.kind, NodeKind::Source { source } if source == &side.after) {
                return Err(invalid(
                    "combined Trim candidate differs from its resolved Source",
                ));
            }
        }
        None if !retained => {}
        _ => {
            return Err(invalid(
                "combined Trim candidate has the wrong retained Source owner",
            ));
        }
    }
    Ok(())
}
