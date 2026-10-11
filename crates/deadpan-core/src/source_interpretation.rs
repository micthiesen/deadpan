//! Authored interpretation of immutable source samples, independent of the
//! source's measured metadata and clocks. Hosts check choices against the
//! qualified pixel format before admitting a revision.

use serde::{Deserialize, Serialize};

use crate::{AssetId, DocumentError, DocumentErrorCode, ProjectDocument};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceColorRange {
    Limited,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceChromaLocation {
    Left,
    Center,
    TopLeft,
    Top,
    BottomLeft,
    Bottom,
}

/// Absence of either choice preserves the source's qualified interpretation.
/// An entirely empty value is invalid: remove the map entry to restore both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceInterpretation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<SourceColorRange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chroma_location: Option<SourceChromaLocation>,
}

pub(crate) fn validate_asset(
    document: &ProjectDocument,
    asset: &AssetId,
) -> Result<(), DocumentError> {
    let record = document.assets().get(asset).ok_or_else(|| {
        DocumentError::new(
            DocumentErrorCode::MissingAsset,
            "interpreted source is absent",
        )
    })?;
    if record.video.is_none() || record.still_image || record.source_qualification.is_none() {
        return Err(DocumentError::new(
            DocumentErrorCode::InvalidAsset,
            "source interpretation requires a qualified original video",
        ));
    }
    Ok(())
}

pub(crate) fn validate(document: &ProjectDocument) -> Result<(), DocumentError> {
    for (asset, interpretation) in &document.source_interpretations {
        validate_asset(document, asset)?;
        if interpretation.range.is_none() && interpretation.chroma_location.is_none() {
            return Err(DocumentError::new(
                DocumentErrorCode::InvalidAsset,
                "empty source interpretation must be removed",
            ));
        }
    }
    Ok(())
}
