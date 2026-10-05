//! The automatic SDR/HDR branch re-derived from stored receipts.
//!
//! Preview, export and durable render admission all use this one function,
//! so a stored decision can be checked against the immutable revision's
//! qualified sources rather than trusted from its own claims.

use deadpan_core::{OutputColorDecision, ProjectDocument, decide_output_color};
use deadpan_media::output_color::asset_color;
use rusqlite::Connection;

use crate::{ProjectStore, StoreError, source_registration::read_receipt};

pub(crate) fn committed_output_color(
    connection: &Connection,
    document: &ProjectDocument,
) -> Result<OutputColorDecision, StoreError> {
    let mut colors = std::collections::BTreeMap::new();
    for (asset, record) in document.assets() {
        let Some(qualification) = &record.source_qualification else {
            continue;
        };
        if record.video.is_none() || record.still_image {
            continue;
        }
        let receipt = read_receipt(connection, qualification)?.ok_or_else(|| {
            StoreError::SourceRegistration(
                "source qualification of a picture asset is missing".into(),
            )
        })?;
        if receipt.asset_record(record.label.clone())? != *record {
            return Err(StoreError::SourceRegistration(
                "asset metadata disagrees with its source qualification".into(),
            ));
        }
        if let Some(video) = receipt.snapshot().video() {
            colors.insert(asset.clone(), asset_color(&video.interpretation().color));
        }
    }
    Ok(decide_output_color(document, |asset| {
        colors.get(asset).copied()
    }))
}

impl ProjectStore {
    /// The committed revision's automatic branch from its stored receipts.
    /// Legacy picture assets without a qualified receipt count as SDR.
    pub fn output_color(
        &self,
        document: &ProjectDocument,
    ) -> Result<OutputColorDecision, StoreError> {
        committed_output_color(&self.connection, document)
    }
}
