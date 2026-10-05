//! Receipt-qualified color facts for the automatic SDR/HDR branch.

use deadpan_core::{AssetColor, AssetTransfer, ContentLight, MasteringDisplay};
use deadpan_source::{ColorMetadata, ColorTransfer};

/// Map a qualified stream interpretation onto the branch-decision facts.
pub fn asset_color(color: &ColorMetadata) -> AssetColor {
    AssetColor {
        transfer: match color.transfer {
            ColorTransfer::Pq => AssetTransfer::Pq,
            ColorTransfer::Hlg => AssetTransfer::Hlg,
            ColorTransfer::Bt709 | ColorTransfer::Srgb | ColorTransfer::Linear => {
                AssetTransfer::Sdr
            }
        },
        mastering: color.mastering.map(|value| MasteringDisplay {
            primaries: value.primaries,
            white_point: value.white_point,
            max_luminance: value.max_luminance,
            min_luminance: value.min_luminance,
        }),
        content_light: color.content_light.map(|value| ContentLight {
            max_cll: value.max_cll,
            max_fall: value.max_fall,
        }),
    }
}
