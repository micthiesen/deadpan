//! Automatic SDR/HDR output branch for one committed revision.
//!
//! Specification 22.5: produce HDR only when every picture-bearing source is
//! HDR with the project's transfer; any SDR picture provider (SDR video,
//! stills, accepted or generated Hold footage) makes the whole revision SDR,
//! with each HDR source tone-mapped by the shared renderer. The pure rule lives
//! in core so the store can re-derive a render decision from stored receipts. Background, Blank
//! and captions are neutral: their working values are BT.2408 graphics white
//! (203 cd/m²) or black in either branch. Preview and export both call
//! [`decide_output_color`] for the same committed document, so they share the
//! branch. No source becomes HDR by tags alone: the basis records the
//! Original's qualified transfer, and an SDR source can never produce HDR.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    AssetId, ColorPolicy, ContentLight, HoldVideo, MasteringDisplay, NodeKind, ProjectDocument,
    SourceVideo,
};
use serde::Serialize;

/// Default tone-mapping source peak when a source declares no usable light
/// metadata: the BT.2100 HLG nominal display and the common PQ grading peak.
pub const DEFAULT_HDR_PEAK_NITS: u32 = 1_000;

/// Qualified transfer class of one registered picture asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetTransfer {
    Sdr,
    Pq,
    Hlg,
}

/// The receipt-qualified color facts the decision needs for one asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssetColor {
    pub transfer: AssetTransfer,
    pub mastering: Option<MasteringDisplay>,
    pub content_light: Option<ContentLight>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputColorReason {
    /// The basis is SDR and no picture source is HDR.
    SdrSources,
    /// Every picture source is HDR with the basis transfer.
    HdrSources,
    /// The basis is SDR but an HDR source is present (legacy mixed project).
    HdrSourceInSdrBasis,
    /// An SDR video or still image is among the picture sources.
    SdrSourceMixed,
    /// Accepted generated or other accepted footage is SDR-only.
    GeneratedPictures,
    /// Sources use a transfer that differs from the basis transfer.
    TransferMismatch,
    /// The basis declares HDR but no qualified HDR source exists.
    NoHdrSource,
}

/// One revision's automatic branch. `output` is the encoded color policy;
/// `tone_map_peak_nits` is the declared HDR source peak used both to tone-map
/// HDR sources into an SDR branch and to tone-map the HDR composite for the
/// SDR preview display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct OutputColorDecision {
    pub output: ColorPolicy,
    pub reason: OutputColorReason,
    pub hdr_sources: bool,
    pub tone_map_peak_nits: u32,
    /// A declared MaxCLL was ignored for the tone-map peak because it was
    /// below the mastering peak or the sanity floor (a likely stale or bogus
    /// value would otherwise crush highlights).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub ignored_content_light: bool,
    /// Retained only for PQ output, from the single consistent source volume.
    pub mastering: Option<MasteringDisplay>,
}

impl OutputColorDecision {
    pub const fn sdr() -> Self {
        Self {
            output: ColorPolicy::SdrRec709,
            reason: OutputColorReason::SdrSources,
            hdr_sources: false,
            tone_map_peak_nits: DEFAULT_HDR_PEAK_NITS,
            ignored_content_light: false,
            mastering: None,
        }
    }

    pub const fn is_hdr(&self) -> bool {
        !matches!(self.output, ColorPolicy::SdrRec709)
    }
}

/// Assets whose pictures the document can show, plus whether any SDR-only
/// provider (still, accepted or generated footage) is authored.
fn picture_providers(document: &ProjectDocument) -> (BTreeSet<AssetId>, bool) {
    let mut assets = BTreeSet::new();
    let mut sdr_only = false;
    let hold = |video: &HoldVideo, assets: &mut BTreeSet<AssetId>, sdr_only: &mut bool| match video
    {
        HoldVideo::Background => {}
        HoldVideo::Freeze { asset, .. }
        | HoldVideo::Reverse { asset, .. }
        | HoldVideo::Play { asset, .. } => {
            assets.insert(asset.clone());
        }
        HoldVideo::Accepted { .. } | HoldVideo::Generated { .. } => *sdr_only = true,
    };
    for node in document.nodes().values() {
        match &node.kind {
            NodeKind::Source { source } => match &source.video {
                SourceVideo::Stream { asset, .. } => {
                    assets.insert(asset.clone());
                }
                SourceVideo::Still { .. } => sdr_only = true,
                SourceVideo::Blank => {}
            },
            NodeKind::Hold { recipe } => hold(&recipe.video, &mut assets, &mut sdr_only),
            NodeKind::Repeat { gap: Some(gap), .. } => {
                hold(&gap.video, &mut assets, &mut sdr_only);
            }
            _ => {}
        }
    }
    (assets, sdr_only)
}

/// Lowest declared MaxCLL trusted as a tone-map peak without a mastering
/// volume. A smaller value is more likely stale or bogus than real HDR.
pub const MIN_TRUSTED_CONTENT_LIGHT_NITS: u32 = 400;

/// Tone-map source peak in cd/m² and whether a declared MaxCLL was ignored.
/// The mastering display peak wins when present: MaxCLL is per-programme
/// content metadata that edits and stale tags can invalidate, while the
/// mastering volume bounds the graded light. Without a mastering volume a
/// MaxCLL at or above the sanity floor is used; otherwise the default.
fn asset_peak(color: &AssetColor) -> (u32, bool) {
    let mastering = color
        .mastering
        .filter(MasteringDisplay::is_valid)
        .map(|value| value.peak_nits());
    let content = color
        .content_light
        .filter(ContentLight::is_valid)
        .map(|value| u32::from(value.max_cll))
        .filter(|value| *value > 0);
    let (peak, ignored) = match (color.transfer, content, mastering) {
        // HLG is display-referred to its nominal 1,000 cd/m² peak here.
        (AssetTransfer::Hlg, _, _) => (DEFAULT_HDR_PEAK_NITS, false),
        (_, content, Some(mastering)) => (mastering, content.is_some_and(|c| c < mastering)),
        (_, Some(content), None) if content >= MIN_TRUSTED_CONTENT_LIGHT_NITS => (content, false),
        (_, Some(_), None) => (DEFAULT_HDR_PEAK_NITS, true),
        _ => (DEFAULT_HDR_PEAK_NITS, false),
    };
    (peak.clamp(203, 10_000), ignored)
}

/// Decide the branch for `document`. `color` returns the qualified color of
/// a registered asset; registered video assets without a qualified receipt
/// (legacy) are SDR. Every registered video asset counts, used or not, so
/// the decision does not depend on which interval is rendered.
pub fn decide_output_color(
    document: &ProjectDocument,
    mut color: impl FnMut(&AssetId) -> Option<AssetColor>,
) -> OutputColorDecision {
    let (mut providers, sdr_only) = picture_providers(document);
    for (id, record) in document.assets() {
        if record.video.is_some() && !record.still_image && record.source_qualification.is_some() {
            providers.insert(id.clone());
        }
    }
    let colors: BTreeMap<AssetId, AssetColor> = providers
        .iter()
        .map(|asset| {
            (
                asset.clone(),
                color(asset).unwrap_or(AssetColor {
                    transfer: AssetTransfer::Sdr,
                    mastering: None,
                    content_light: None,
                }),
            )
        })
        .collect();
    let hdr: Vec<&AssetColor> = colors
        .values()
        .filter(|value| value.transfer != AssetTransfer::Sdr)
        .collect();
    let hdr_sources = !hdr.is_empty();
    let peaks: Vec<(u32, bool)> = hdr.iter().map(|value| asset_peak(value)).collect();
    let tone_map_peak_nits = peaks
        .iter()
        .map(|(peak, _)| *peak)
        .max()
        .unwrap_or(DEFAULT_HDR_PEAK_NITS);
    let ignored_content_light = peaks.iter().any(|(_, ignored)| *ignored);
    let sdr = |reason| OutputColorDecision {
        output: ColorPolicy::SdrRec709,
        reason,
        hdr_sources,
        tone_map_peak_nits,
        ignored_content_light,
        mastering: None,
    };
    let wanted = match document.presentation_basis().color_policy {
        ColorPolicy::SdrRec709 => {
            return sdr(if hdr_sources {
                OutputColorReason::HdrSourceInSdrBasis
            } else {
                OutputColorReason::SdrSources
            });
        }
        ColorPolicy::HdrRec2020Pq => AssetTransfer::Pq,
        ColorPolicy::HdrRec2020Hlg => AssetTransfer::Hlg,
    };
    if !hdr_sources {
        return sdr(OutputColorReason::NoHdrSource);
    }
    if sdr_only {
        return sdr(OutputColorReason::GeneratedPictures);
    }
    if colors
        .values()
        .any(|value| value.transfer == AssetTransfer::Sdr)
    {
        return sdr(OutputColorReason::SdrSourceMixed);
    }
    if hdr.iter().any(|value| value.transfer != wanted) {
        return sdr(OutputColorReason::TransferMismatch);
    }
    let mastering = if wanted == AssetTransfer::Pq {
        let volumes: BTreeSet<_> = hdr
            .iter()
            .map(|value| {
                value
                    .mastering
                    .map(|m| (m.primaries, m.white_point, m.max_luminance, m.min_luminance))
            })
            .collect();
        match (volumes.len(), hdr[0].mastering) {
            (1, Some(volume)) if volume.is_valid() => Some(volume),
            _ => None,
        }
    } else {
        None
    };
    OutputColorDecision {
        output: document.presentation_basis().color_policy,
        reason: OutputColorReason::HdrSources,
        hdr_sources,
        tone_map_peak_nits,
        ignored_content_light,
        mastering,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pq(mastering: Option<u32>, cll: Option<u16>) -> AssetColor {
        AssetColor {
            transfer: AssetTransfer::Pq,
            mastering: mastering.map(|peak| MasteringDisplay {
                primaries: [[35_400, 14_600], [8_500, 39_850], [6_550, 2_300]],
                white_point: [15_635, 16_450],
                max_luminance: peak * 10_000,
                min_luminance: 50,
            }),
            content_light: cll.map(|max_cll| ContentLight {
                max_cll,
                max_fall: max_cll.min(100),
            }),
        }
    }

    #[test]
    fn tone_map_peak_prefers_mastering_and_ignores_low_content_light() {
        assert_eq!(asset_peak(&pq(Some(1_000), Some(1_000))), (1_000, false));
        // A MaxCLL below the mastering peak cannot lower the tone-map peak.
        assert_eq!(asset_peak(&pq(Some(4_000), Some(250))), (4_000, true));
        assert_eq!(asset_peak(&pq(None, Some(2_000))), (2_000, false));
        // Below the floor without mastering: default, and the value is noted.
        assert_eq!(
            asset_peak(&pq(None, Some(120))),
            (DEFAULT_HDR_PEAK_NITS, true)
        );
        assert_eq!(asset_peak(&pq(None, None)), (DEFAULT_HDR_PEAK_NITS, false));
        let hlg = AssetColor {
            transfer: AssetTransfer::Hlg,
            ..pq(Some(4_000), Some(250))
        };
        assert_eq!(asset_peak(&hlg), (DEFAULT_HDR_PEAK_NITS, false));
    }
}
