//! Single-anchor face geometry and generated-only mouth evidence. Association
//! travels outward from the retained anchor; raw frames always stay chronological.

use super::*;
use crate::generated_extension::{CoverageError, ExtensionCoverage, MAX_GENERATED_FRAMES};

pub const RAW_EXTENSION_LANDMARK_SCHEMA_VERSION: u32 = 1;
pub const PROFILE: &str = "deadpan-extension-face-geometry-mouth-1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawWire")]
pub struct RawExtensionLandmarkBatch {
    pub schema_version: u32,
    pub coverage: ExtensionCoverage,
    pub anchor: FaceObservationSet,
    pub frames: Vec<FrameObservation>,
}

impl RawExtensionLandmarkBatch {
    pub fn validate(&self, expected_native_pts: &[i64]) -> Result<(), ExtensionGeometryError> {
        if self.schema_version != RAW_EXTENSION_LANDMARK_SCHEMA_VERSION {
            return Err(RawObservationError::SchemaVersion.into());
        }
        self.coverage.validate(expected_native_pts)?;
        if self.frames.len() != (self.coverage.end - self.coverage.start) as usize {
            return Err(RawObservationError::FrameCount.into());
        }
        self.anchor.validate()?;
        for (ordinal, frame) in self.coverage.ordinals().zip(&self.frames) {
            if frame.ordinal != ordinal {
                return Err(RawObservationError::FrameOrdinal.into());
            }
            if frame.pts != expected_native_pts[ordinal as usize] {
                return Err(RawObservationError::FramePts.into());
            }
            frame.observation.validate()?;
        }
        Ok(())
    }
}

/// Geometry measures displacement from one static quiet-pose anchor. There is
/// no interpolated return path and no opposite picture in this policy. A track
/// may have both measured prefix evidence and a later unavailable continuation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionGeometryReport {
    pub schema_version: u32,
    pub profile: String,
    pub coverage: ExtensionCoverage,
    pub raster: [u32; 2],
    pub presentation: [u32; 4],
    pub anchor_usable: bool,
    pub thresholds: GeometryThresholds,
    pub mouth_thresholds: MouthThresholds,
    /// Sum of confidently associated generated observations across anchor
    /// faces, not a claim that every face or generated picture was measurable.
    pub measured_face_frames: u32,
    pub geometry: GeometryAssessment,
    pub mouth: MouthAssessment,
}

impl ExtensionGeometryReport {
    pub fn validate_recomputed(
        &self,
        batch: &RawExtensionLandmarkBatch,
        expected_native_pts: &[i64],
        raster: [u32; 2],
        presentation: [u32; 4],
        anchor_usable: bool,
    ) -> Result<(), ExtensionGeometryError> {
        if *self
            != analyze(
                batch,
                expected_native_pts,
                raster,
                presentation,
                anchor_usable,
            )?
        {
            return Err(ExtensionGeometryError::Geometry(
                GeometryError::PolicyMismatch,
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ExtensionGeometryError {
    #[error(transparent)]
    Coverage(#[from] CoverageError),
    #[error(transparent)]
    Raw(#[from] RawObservationError),
    #[error(transparent)]
    Geometry(#[from] GeometryError),
}

pub fn analyze(
    batch: &RawExtensionLandmarkBatch,
    expected_native_pts: &[i64],
    raster: [u32; 2],
    presentation: [u32; 4],
    anchor_usable: bool,
) -> Result<ExtensionGeometryReport, ExtensionGeometryError> {
    batch.validate(expected_native_pts)?;
    validate_raster(raster)?;
    validate_presentation(raster, presentation)?;
    let thresholds = GeometryThresholds::policy();
    let mouth_thresholds = MouthThresholds::policy();
    let (geometry, measured_face_frames) =
        geometry(batch, raster, presentation, anchor_usable, thresholds);
    let mut mouth = analyze_mouth_frames(&batch.frames, raster, presentation, mouth_thresholds);
    mouth.basis = MouthBasis::GeneratedIntervalOnly;
    if let Some(rejection) = &mut mouth.rejection {
        // Coverage validates <=4096 canonical pictures before this addition.
        rejection.first_after_ordinal += batch.coverage.start;
    }
    Ok(ExtensionGeometryReport {
        schema_version: 1,
        profile: PROFILE.into(),
        coverage: batch.coverage,
        raster,
        presentation,
        anchor_usable,
        thresholds,
        mouth_thresholds,
        measured_face_frames,
        geometry,
        mouth,
    })
}

fn geometry(
    batch: &RawExtensionLandmarkBatch,
    raster: [u32; 2],
    presentation: [u32; 4],
    anchor_usable: bool,
    thresholds: GeometryThresholds,
) -> (GeometryAssessment, u32) {
    let mut report = GeometryAssessment {
        status: CheckStatus::Unavailable,
        measured_tracks: 0,
        unavailable_tracks: 0,
        feature_unavailable_tracks: 0,
        maximum_center_residual: None,
        maximum_log_size_residual: None,
        maximum_feature_residual: None,
        rejection: None,
        unavailable_reasons: Vec::new(),
    };
    if !anchor_usable {
        report
            .unavailable_reasons
            .push(GeometryUnavailableReason::AuthoredBlackBoundary);
        return (report, 0);
    }
    let ordinals = batch.coverage.tracking_ordinals();
    let mut sets = Vec::with_capacity(ordinals.len() + 1);
    sets.push(EligibleFaceSet::new(&batch.anchor, raster, presentation));
    sets.extend(ordinals.iter().map(|ordinal| {
        EligibleFaceSet::new(
            &batch.frames[(ordinal - batch.coverage.start) as usize].observation,
            raster,
            presentation,
        )
    }));
    if sets[0].faces.is_empty() {
        report.unavailable_reasons.push(
            sets[0]
                .missing_reason()
                .unwrap_or(GeometryUnavailableReason::NoFaceDetected),
        );
        return (report, 0);
    }
    let mut measured_frames = 0_u32;
    for anchor_index in 0..sets[0].faces.len() {
        let Some(track) = associate_track(&sets, anchor_index, thresholds.minimum_confidence)
        else {
            report.unavailable_tracks += 1;
            push_geometry_reason(
                &mut report.unavailable_reasons,
                GeometryUnavailableReason::AmbiguousAssociation,
            );
            continue;
        };
        if !track.complete {
            report.unavailable_tracks += 1;
            push_geometry_reason(
                &mut report.unavailable_reasons,
                track.reason.unwrap_or(GeometryUnavailableReason::LostTrack),
            );
        }
        let anchor = sets[0].faces[anchor_index];
        let anchor_feature = shape_features(anchor, raster, presentation, thresholds);
        let mut feature_missing = Vec::new();
        if let Err(reason) = anchor_feature {
            push_geometry_reason(&mut feature_missing, reason.geometry());
        }
        let mut measured = false;
        let mut run = 0_u32;
        let mut run_start = 0_u32;
        for (offset, &index) in track.indices.iter().enumerate().skip(1) {
            if index == usize::MAX {
                break;
            }
            let actual = sets[offset].faces[index];
            // associate_track checks confidence before extending each link and
            // at its terminal observation, including a one-picture interval.
            if f64::from(actual.face.confidence) < thresholds.minimum_confidence {
                break;
            }
            measured = true;
            measured_frames += 1;
            let ordinal = ordinals[offset - 1];
            let center = center_residual(actual.rect, anchor.rect, presentation);
            let size = log_size_residual(actual.rect, anchor.rect);
            update_max(&mut report.maximum_center_residual, center);
            update_max(&mut report.maximum_log_size_residual, size);
            let actual_feature = shape_features(actual, raster, presentation, thresholds);
            if let Err(reason) = actual_feature {
                push_geometry_reason(&mut feature_missing, reason.geometry());
            }
            let feature = match (anchor_feature, actual_feature) {
                (Ok(expected), Ok(value)) => Some(
                    (value.nose[0] - expected.nose[0])
                        .hypot(value.nose[1] - expected.nose[1])
                        .max((value.eye_scale / expected.eye_scale).ln().abs()),
                ),
                _ => None,
            };
            if let Some(feature) = feature {
                update_max(&mut report.maximum_feature_residual, feature);
            }
            if center > thresholds.center_residual
                || size > thresholds.log_size_residual
                || feature.is_some_and(|value| value > thresholds.feature_residual)
            {
                if run == 0 {
                    run_start = ordinal;
                }
                run += 1;
                if run >= thresholds.sustained_frames {
                    // Report the chronological start of the proven window,
                    // including when tracking reached it in reverse order.
                    let candidate = GeometryRejection {
                        first_ordinal: run_start.min(ordinal),
                        consecutive_frames: run,
                        center_residual: center,
                        log_size_residual: size,
                        feature_residual: feature,
                    };
                    if report
                        .rejection
                        .is_none_or(|previous| candidate.first_ordinal < previous.first_ordinal)
                    {
                        report.rejection = Some(candidate);
                    }
                }
            } else {
                run = 0;
            }
        }
        if measured {
            report.measured_tracks += 1;
            if !feature_missing.is_empty() {
                report.feature_unavailable_tracks += 1;
                for reason in feature_missing {
                    push_geometry_reason(&mut report.unavailable_reasons, reason);
                }
            }
        }
    }
    report.status = if report.rejection.is_some() {
        CheckStatus::Rejected
    } else if report.measured_tracks > 0 {
        CheckStatus::Measured
    } else {
        CheckStatus::Unavailable
    };
    (report, measured_frames)
}

// Extension-only bounded wire readers leave historical Bridge serde unchanged.
struct Bounded<T, const N: usize>(Vec<T>);

impl<'de, T: Deserialize<'de>, const N: usize> Deserialize<'de> for Bounded<T, N> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor<T, const N: usize>(std::marker::PhantomData<T>);
        impl<'de, T: Deserialize<'de>, const N: usize> serde::de::Visitor<'de> for Visitor<T, N> {
            type Value = Bounded<T, N>;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "at most {N} extension observations")
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut sequence: A,
            ) -> Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while values.len() < N {
                    let Some(value) = sequence.next_element()? else {
                        return Ok(Bounded(values));
                    };
                    values.push(value);
                }
                if sequence.next_element::<serde::de::IgnoredAny>()?.is_some() {
                    return Err(serde::de::Error::custom(
                        "extension observation count exceeds its bound",
                    ));
                }
                Ok(Bounded(values))
            }
        }
        deserializer.deserialize_seq(Visitor::<T, N>(std::marker::PhantomData))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawWire {
    schema_version: u32,
    coverage: ExtensionCoverage,
    anchor: FaceSetWire,
    frames: Bounded<FrameWire, MAX_GENERATED_FRAMES>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FrameWire {
    ordinal: u32,
    pts: i64,
    observation: FaceSetWire,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, tag = "status", rename_all = "snake_case")]
enum FaceSetWire {
    Detected {
        faces: Bounded<FaceWire, MAX_FACES_PER_PICTURE>,
    },
    Unavailable {
        reason: FaceSetUnavailableReason,
    },
}

impl From<FaceSetWire> for FaceObservationSet {
    fn from(value: FaceSetWire) -> Self {
        match value {
            FaceSetWire::Detected { faces } => Self::Detected {
                faces: faces
                    .0
                    .into_iter()
                    .map(|face| FaceLandmarks {
                        region: face.region,
                        confidence: face.confidence,
                        landmarks: face.landmarks.into(),
                    })
                    .collect(),
            },
            FaceSetWire::Unavailable { reason } => Self::Unavailable { reason },
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FaceWire {
    region: NormalizedRect,
    confidence: f32,
    landmarks: LandmarksWire,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, tag = "status", rename_all = "snake_case")]
enum LandmarksWire {
    Available {
        confidence: f32,
        left_eye: RegionWire,
        right_eye: RegionWire,
        nose: RegionWire,
        outer_lips: RegionWire,
        inner_lips: RegionWire,
    },
    Unavailable {
        reason: LandmarkUnavailableReason,
    },
}

impl From<LandmarksWire> for LandmarkAvailability {
    fn from(value: LandmarksWire) -> Self {
        match value {
            LandmarksWire::Available {
                confidence,
                left_eye,
                right_eye,
                nose,
                outer_lips,
                inner_lips,
            } => Self::Available {
                confidence,
                left_eye: left_eye.into(),
                right_eye: right_eye.into(),
                nose: nose.into(),
                outer_lips: outer_lips.into(),
                inner_lips: inner_lips.into(),
            },
            LandmarksWire::Unavailable { reason } => Self::Unavailable { reason },
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, tag = "status", rename_all = "snake_case")]
enum RegionWire {
    Detected {
        points: Bounded<[f64; 2], MAX_LANDMARK_POINTS_PER_REGION>,
    },
    Unavailable {
        reason: LandmarkUnavailableReason,
    },
}

impl From<RegionWire> for LandmarkRegion {
    fn from(value: RegionWire) -> Self {
        match value {
            RegionWire::Detected { points } => Self::Detected { points: points.0 },
            RegionWire::Unavailable { reason } => Self::Unavailable { reason },
        }
    }
}

impl TryFrom<RawWire> for RawExtensionLandmarkBatch {
    type Error = RawObservationError;
    fn try_from(value: RawWire) -> Result<Self, Self::Error> {
        if value.schema_version != RAW_EXTENSION_LANDMARK_SCHEMA_VERSION {
            return Err(RawObservationError::SchemaVersion);
        }
        let raw = Self {
            schema_version: value.schema_version,
            coverage: value.coverage,
            anchor: value.anchor.into(),
            frames: value
                .frames
                .0
                .into_iter()
                .map(|frame| FrameObservation {
                    ordinal: frame.ordinal,
                    pts: frame.pts,
                    observation: frame.observation.into(),
                })
                .collect(),
        };
        raw.anchor.validate()?;
        for frame in &raw.frames {
            frame.observation.validate()?;
        }
        Ok(raw)
    }
}

#[cfg(test)]
mod tests;
