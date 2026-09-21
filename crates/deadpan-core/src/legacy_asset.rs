//! Frozen schema-5 through schema-8 asset vocabulary. Generated hash strings
//! were already supported; source qualification bindings were not.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{AssetId, AssetRecord, FrameDuration, SourceSpan, ValueChange};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Asset {
    label: String,
    content_hash: String,
    video: Option<SourceSpan>,
    audio: Option<SourceSpan>,
    still_image: bool,
    frame_count: Option<FrameDuration>,
}

impl Asset {
    pub(crate) fn upgrade(self) -> AssetRecord {
        AssetRecord {
            label: self.label,
            content_hash: self.content_hash,
            video: self.video,
            audio: self.audio,
            still_image: self.still_image,
            frame_count: self.frame_count,
            source_qualification: None,
        }
    }

    fn project(asset: &AssetRecord) -> Option<Self> {
        asset.source_qualification.is_none().then(|| Self {
            label: asset.label.clone(),
            content_hash: asset.content_hash.clone(),
            video: asset.video,
            audio: asset.audio,
            still_image: asset.still_image,
            frame_count: asset.frame_count,
        })
    }
}

pub(crate) fn upgrade_assets(assets: BTreeMap<AssetId, Asset>) -> BTreeMap<AssetId, AssetRecord> {
    assets
        .into_iter()
        .map(|(id, asset)| (id, asset.upgrade()))
        .collect()
}

pub(crate) fn project_assets(
    assets: &BTreeMap<AssetId, AssetRecord>,
) -> Option<BTreeMap<AssetId, Asset>> {
    assets
        .iter()
        .map(|(id, asset)| Some((id.clone(), Asset::project(asset)?)))
        .collect()
}

pub(crate) fn project_changes(
    changes: &BTreeMap<AssetId, ValueChange<AssetRecord>>,
) -> Option<BTreeMap<AssetId, ValueChange<Asset>>> {
    changes
        .iter()
        .map(|(id, change)| {
            Some((
                id.clone(),
                ValueChange {
                    before: match &change.before {
                        Some(asset) => Some(Asset::project(asset)?),
                        None => None,
                    },
                    after: match &change.after {
                        Some(asset) => Some(Asset::project(asset)?),
                        None => None,
                    },
                },
            ))
        })
        .collect()
}
