//! Captured authored subject regions in the exact conditioning pictures.

use deadpan_analysis::{NormalizedRect, generated_region::RegionSeeds};
use deadpan_core::{
    AssetId, AttentionTarget, ExactRatio, SourcePoint, TARGET_UNITS, TargetId, TargetRegion,
    TargetSource, TrackState,
};
use deadpan_jobs::Sha256;
use serde::{Deserialize, Serialize};
use sha2::Digest;

use crate::{BoundaryPicture, BridgeBoundaries, ConditioningGeometry, RasterRect};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "selection", rename_all = "snake_case", deny_unknown_fields)]
pub enum RegionCapture {
    None,
    Selected {
        target: TargetId,
        label: String,
        target_sha256: Sha256,
        left: Box<CapturedRegionBoundary>,
        right: Box<CapturedRegionBoundary>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum CapturedRegionBoundary {
    Available {
        asset: AssetId,
        point: SourcePoint,
        source: TargetSource,
        region: TargetRegion,
        confidence: Option<u16>,
    },
    Unavailable {
        reason: RegionCaptureUnavailable,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegionCaptureUnavailable {
    NotOriginal,
    DifferentAsset,
    OutsideTargetSpan,
    LostTrack,
    InterpolatedTrack,
    LowConfidence,
    OutsideSource,
    MissingContent,
    TooSmall,
}

impl RegionCaptureUnavailable {
    fn description(self) -> &'static str {
        match self {
            Self::NotOriginal => "boundary is not an Original picture",
            Self::DifferentAsset => "target belongs to a different Original",
            Self::OutsideTargetSpan => "picture is outside the target's supported source range",
            Self::LostTrack => "authored target tracking was lost",
            Self::InterpolatedTrack => "authored target position is interpolated",
            Self::LowConfidence => "authored target confidence is below 70%",
            Self::OutsideSource => "target extends outside the Original",
            Self::MissingContent => "boundary has no fitted Original content",
            Self::TooSmall => "target is too small in the conditioning picture",
        }
    }
}

impl RegionCapture {
    pub fn new(
        target: TargetId,
        record: &AttentionTarget,
        boundaries: &BridgeBoundaries,
        geometry: &ConditioningGeometry,
        native: [u32; 2],
    ) -> Result<Self, String> {
        let capture = Self::Selected {
            target,
            label: record.label.clone(),
            target_sha256: Sha256::new(
                sha2::Sha256::digest(
                    serde_json::to_vec(record).map_err(|error| error.to_string())?,
                )
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
            )
            .map_err(|error| error.to_string())?,
            left: Box::new(capture_boundary(
                record,
                &boundaries.left,
                geometry.left_content,
                native,
            )),
            right: Box::new(capture_boundary(
                record,
                &boundaries.right,
                geometry.right_content,
                native,
            )),
        };
        capture.validate(boundaries, geometry, native)?;
        Ok(capture)
    }

    pub fn target_id(&self) -> Option<&TargetId> {
        match self {
            Self::None => None,
            Self::Selected { target, .. } => Some(target),
        }
    }

    pub fn unavailable_reason(&self) -> Option<String> {
        match self {
            Self::None => Some("no selected region target".into()),
            Self::Selected { left, right, .. } => {
                let reasons: Vec<_> = [("left", left.as_ref()), ("right", right.as_ref())]
                    .into_iter()
                    .filter_map(|(side, boundary)| {
                        if let CapturedRegionBoundary::Unavailable { reason } = boundary {
                            Some(format!("{side}: {}", reason.description()))
                        } else {
                            None
                        }
                    })
                    .collect();
                (!reasons.is_empty()).then(|| reasons.join("; "))
            }
        }
    }

    pub fn validate(
        &self,
        boundaries: &BridgeBoundaries,
        geometry: &ConditioningGeometry,
        native: [u32; 2],
    ) -> Result<(), String> {
        geometry
            .validate(native, boundaries)
            .map_err(str::to_owned)?;
        let Self::Selected {
            label, left, right, ..
        } = self
        else {
            return Ok(());
        };
        if label.trim().is_empty() || label.len() > 128 {
            return Err("invalid captured target label".into());
        }
        for (captured, picture, content) in [
            (left.as_ref(), &boundaries.left, geometry.left_content),
            (right.as_ref(), &boundaries.right, geometry.right_content),
        ] {
            if let CapturedRegionBoundary::Available {
                asset,
                point,
                source,
                region,
                confidence,
            } = captured
            {
                let BoundaryPicture::Original {
                    asset: expected_asset,
                    picture,
                    ..
                } = picture
                else {
                    return Err("region seed requires an Original boundary".into());
                };
                let expected_point = SourcePoint {
                    ticks: ExactRatio::integer(picture.pts.ticks),
                    time_base: picture.pts.time_base,
                };
                if asset != expected_asset || *point != expected_point {
                    return Err("region seed differs from its decoded Original picture".into());
                }
                match (source, confidence) {
                    (TargetSource::Initial | TargetSource::Manual, None) => {}
                    (TargetSource::Tracked(TrackState::Tracked), Some(700..=1000)) => {}
                    _ => {
                        return Err("region seed has unavailable authored tracking evidence".into());
                    }
                }
                map_region(
                    *region,
                    content.ok_or("region seed has no fitted content")?,
                    native,
                )?;
            }
        }
        Ok(())
    }

    pub fn seeds(
        &self,
        boundaries: &BridgeBoundaries,
        geometry: &ConditioningGeometry,
        native: [u32; 2],
    ) -> Result<Option<RegionSeeds>, String> {
        self.validate(boundaries, geometry, native)?;
        let Self::Selected { left, right, .. } = self else {
            return Ok(None);
        };
        let (
            CapturedRegionBoundary::Available { region: left, .. },
            CapturedRegionBoundary::Available { region: right, .. },
        ) = (left.as_ref(), right.as_ref())
        else {
            return Ok(None);
        };
        let seeds = RegionSeeds {
            left: map_region(
                *left,
                geometry.left_content.ok_or("missing left content")?,
                native,
            )?,
            right: map_region(
                *right,
                geometry.right_content.ok_or("missing right content")?,
                native,
            )?,
        };
        seeds.validate().map_err(|error| error.to_string())?;
        Ok(Some(seeds))
    }
}

fn capture_boundary(
    target: &AttentionTarget,
    boundary: &BoundaryPicture,
    content: Option<RasterRect>,
    native: [u32; 2],
) -> CapturedRegionBoundary {
    use RegionCaptureUnavailable as Reason;
    let unavailable = |reason| CapturedRegionBoundary::Unavailable { reason };
    let BoundaryPicture::Original { asset, picture, .. } = boundary else {
        return unavailable(Reason::NotOriginal);
    };
    if asset != &target.asset {
        return unavailable(Reason::DifferentAsset);
    }
    let point = SourcePoint {
        ticks: ExactRatio::integer(picture.pts.ticks),
        time_base: picture.pts.time_base,
    };
    let Some(value) = target.evaluated_region_at(point) else {
        return unavailable(Reason::OutsideTargetSpan);
    };
    match value.source {
        TargetSource::Tracked(TrackState::Lost) => return unavailable(Reason::LostTrack),
        TargetSource::Tracked(TrackState::Interpolated) => {
            return unavailable(Reason::InterpolatedTrack);
        }
        TargetSource::Tracked(TrackState::Tracked)
            if value.confidence.is_none_or(|confidence| confidence < 700) =>
        {
            return unavailable(Reason::LowConfidence);
        }
        _ => {}
    }
    if !inside(value.region) {
        return unavailable(Reason::OutsideSource);
    }
    let Some(content) = content else {
        return unavailable(Reason::MissingContent);
    };
    if map_region(value.region, content, native).is_err() {
        return unavailable(Reason::TooSmall);
    }
    CapturedRegionBoundary::Available {
        asset: asset.clone(),
        point,
        source: value.source,
        region: value.region,
        confidence: value.confidence,
    }
}

fn inside(region: TargetRegion) -> bool {
    region.validate().is_ok()
        && (0..2).all(|axis| {
            let center = u64::from(region.center[axis]) * 2;
            let size = u64::from(region.size[axis]);
            center >= size && center + size <= u64::from(TARGET_UNITS) * 2
        })
}

fn map_region(
    region: TargetRegion,
    content: RasterRect,
    native: [u32; 2],
) -> Result<NormalizedRect, String> {
    if !inside(region) {
        return Err("region seed extends outside its Original".into());
    }
    content.validate_within(native).map_err(str::to_owned)?;
    let units = f64::from(TARGET_UNITS);
    let size = region.size.map(|value| f64::from(value) / units);
    let center = region.center.map(|value| f64::from(value) / units);
    NormalizedRect::new(
        (f64::from(content.x) + (center[0] - size[0] / 2.0) * f64::from(content.width))
            / f64::from(native[0]),
        (f64::from(content.y) + (center[1] - size[1] / 2.0) * f64::from(content.height))
            / f64::from(native[1]),
        size[0] * f64::from(content.width) / f64::from(native[0]),
        size[1] * f64::from(content.height) / f64::from(native[1]),
    )
    .map_err(|error| error.to_string())
}

#[cfg(test)]
#[path = "conditioning_region_tests.rs"]
mod tests;
