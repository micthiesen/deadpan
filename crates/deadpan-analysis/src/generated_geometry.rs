//! Conservative face-geometry and mouth-motion checks for generated bridges.
//!
//! This module consumes keyed observations from the native detector. It does
//! not assign identity or infer speech. A missing or ambiguous observation is
//! retained as unavailable evidence, never changed into a zero measurement.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use crate::NormalizedRect;

pub const RAW_LANDMARK_SCHEMA_VERSION: u32 = 1;
pub const PROFILE: &str = "deadpan-generated-face-geometry-mouth-1";
pub const MAX_NATIVE_FRAMES: usize = 1025;
pub const MAX_FACES_PER_PICTURE: usize = 8;
pub const MAX_LANDMARK_POINTS_PER_REGION: usize = 76;
pub const MAX_LANDMARK_POINTS_PER_FACE: usize = 76;

const MIN_FACE_CONFIDENCE: f64 = 0.70;
const ASSOCIATION_MAX_SCORE: f64 = 1.25;
const ASSOCIATION_UNIQUENESS_MARGIN: f64 = 0.15;
const ASSOCIATION_OVERLAP_AMBIGUITY: f64 = 0.25;
const GEOMETRY_CENTER_LIMIT: f64 = 0.10;
const GEOMETRY_LOG_SCALE_LIMIT: f64 = 0.40;
const GEOMETRY_FEATURE_LIMIT: f64 = 0.20;
const SUSTAINED_EXCESS_FRAMES: u32 = 2;
const MIN_LANDMARK_CONFIDENCE: f64 = 0.70;
const MOUTH_EYE_SEPARATION_MIN_FRACTION: f64 = 0.01;
const MAX_RASTER_PIXELS: u64 = 4096 * 4096;

/// One ordered native picture observation. The ordinal and exact PTS are both
/// checked against the host's canonical native-frame contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrameObservation {
    pub ordinal: u32,
    pub pts: i64,
    pub observation: FaceObservationSet,
}

/// Observations for the two actual conditioning pictures, in boundary order.
/// Their object identities are bound by the host report, not inferred here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoundaryObservations {
    pub left: FaceObservationSet,
    pub right: FaceObservationSet,
}

/// Bounded raw evidence from one sequential native inspection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawLandmarkBatch {
    pub schema_version: u32,
    /// `None` means the two retained conditioning pictures were not measured.
    /// The policy reports that geometry cannot be boundary-anchored.
    pub boundaries: Option<BoundaryObservations>,
    pub frames: Vec<FrameObservation>,
}

impl RawLandmarkBatch {
    /// Validate exact native-frame coverage without depending on a provider or
    /// on a media contract type. `expected_native_pts` is the host-verified
    /// ordered PTS sequence for the canonical master.
    pub fn validate(&self, expected_native_pts: &[i64]) -> Result<(), RawObservationError> {
        if self.schema_version != RAW_LANDMARK_SCHEMA_VERSION {
            return Err(RawObservationError::SchemaVersion);
        }
        if !(2..=MAX_NATIVE_FRAMES).contains(&expected_native_pts.len()) {
            return Err(RawObservationError::NativeFrameLimit);
        }
        if expected_native_pts
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
        {
            return Err(RawObservationError::ExpectedPtsOrder);
        }
        if self.frames.len() != expected_native_pts.len() {
            return Err(RawObservationError::FrameCount);
        }
        if let Some(boundaries) = &self.boundaries {
            boundaries.left.validate()?;
            boundaries.right.validate()?;
        }
        for (index, frame) in self.frames.iter().enumerate() {
            if usize::try_from(frame.ordinal).ok() != Some(index) {
                return Err(RawObservationError::FrameOrdinal);
            }
            if frame.pts != expected_native_pts[index] {
                return Err(RawObservationError::FramePts);
            }
            frame.observation.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawObservationError {
    SchemaVersion,
    NativeFrameLimit,
    ExpectedPtsOrder,
    FrameCount,
    FrameOrdinal,
    FramePts,
    FaceCount,
    FaceConfidence,
    FaceOrder,
    LandmarkConfidence,
    LandmarkPointCount,
    LandmarkPointBounds,
    LandmarkPointTotal,
}

impl std::fmt::Display for RawObservationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::SchemaVersion => "unsupported raw-landmark schema",
            Self::NativeFrameLimit => "native frame count is outside the observation bound",
            Self::ExpectedPtsOrder => "expected native PTS values are not strictly increasing",
            Self::FrameCount => "raw landmarks do not cover every native frame",
            Self::FrameOrdinal => "raw landmark frame ordinals are not contiguous",
            Self::FramePts => "raw landmark frame PTS differs from the canonical frame",
            Self::FaceCount => "face count exceeds the observation bound",
            Self::FaceConfidence => "face confidence is outside [0, 1]",
            Self::FaceOrder => "faces are not in strict canonical wire order",
            Self::LandmarkConfidence => "landmark confidence is outside [0, 1]",
            Self::LandmarkPointCount => "landmark feature point count is outside its bound",
            Self::LandmarkPointBounds => "landmark point is non-finite or outside the raster",
            Self::LandmarkPointTotal => "combined landmark point count exceeds its bound",
        })
    }
}

impl std::error::Error for RawObservationError {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum FaceObservationSet {
    Detected { faces: Vec<FaceLandmarks> },
    Unavailable { reason: FaceSetUnavailableReason },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FaceSetUnavailableReason {
    TooManyFaces,
    InvalidGeometry,
}

impl FaceObservationSet {
    pub fn validate(&self) -> Result<(), RawObservationError> {
        let Self::Detected { faces } = self else {
            return Ok(());
        };
        if faces.len() > MAX_FACES_PER_PICTURE {
            return Err(RawObservationError::FaceCount);
        }
        for face in faces {
            face.validate()?;
        }
        if faces
            .windows(2)
            .any(|pair| pair[0].order(&pair[1]) != Ordering::Less)
        {
            return Err(RawObservationError::FaceOrder);
        }
        Ok(())
    }

    pub fn is_unavailable(&self) -> bool {
        matches!(self, Self::Unavailable { .. })
    }
}

/// One detected face. Ordering is only a canonical wire representation; the
/// association policy never treats a list position as a face identity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FaceLandmarks {
    pub region: NormalizedRect,
    pub confidence: f32,
    pub landmarks: LandmarkAvailability,
}

impl FaceLandmarks {
    pub fn order(&self, other: &Self) -> Ordering {
        rect_order(&self.region, &other.region)
            .then_with(|| self.confidence.total_cmp(&other.confidence))
    }

    fn validate(&self) -> Result<(), RawObservationError> {
        if !self.confidence.is_finite() || !(0.0..=1.0).contains(&self.confidence) {
            return Err(RawObservationError::FaceConfidence);
        }
        let LandmarkAvailability::Available {
            confidence,
            left_eye,
            right_eye,
            nose,
            outer_lips,
            inner_lips,
        } = &self.landmarks
        else {
            return Ok(());
        };
        if !confidence.is_finite() || !(0.0..=1.0).contains(confidence) {
            return Err(RawObservationError::LandmarkConfidence);
        }
        let regions = [left_eye, right_eye, nose, outer_lips, inner_lips];
        let mut total = 0_usize;
        for region in regions {
            if let LandmarkRegion::Detected { points } = region {
                if points.is_empty() || points.len() > MAX_LANDMARK_POINTS_PER_REGION {
                    return Err(RawObservationError::LandmarkPointCount);
                }
                total = total
                    .checked_add(points.len())
                    .ok_or(RawObservationError::LandmarkPointTotal)?;
                if points.iter().any(|point| {
                    point
                        .iter()
                        .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
                }) {
                    return Err(RawObservationError::LandmarkPointBounds);
                }
            }
        }
        if total > MAX_LANDMARK_POINTS_PER_FACE {
            return Err(RawObservationError::LandmarkPointTotal);
        }
        Ok(())
    }
}

fn rect_order(left: &NormalizedRect, right: &NormalizedRect) -> Ordering {
    left.x()
        .total_cmp(&right.x())
        .then_with(|| left.y().total_cmp(&right.y()))
        .then_with(|| left.width().total_cmp(&right.width()))
        .then_with(|| left.height().total_cmp(&right.height()))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum LandmarkAvailability {
    Available {
        confidence: f32,
        left_eye: LandmarkRegion,
        right_eye: LandmarkRegion,
        nose: LandmarkRegion,
        outer_lips: LandmarkRegion,
        inner_lips: LandmarkRegion,
    },
    Unavailable {
        reason: LandmarkUnavailableReason,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LandmarkUnavailableReason {
    Missing,
    InvalidGeometry,
    LowConfidence,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum LandmarkRegion {
    Detected { points: Vec<[f64; 2]> },
    Unavailable { reason: LandmarkUnavailableReason },
}

/// Pure policy output. It is bound to input objects and the native contract by
/// the host before persistence. A rejected value is an engineering heuristic,
/// not an identity or speech judgment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneratedGeometryReport {
    pub schema_version: u32,
    pub profile: String,
    pub native_frames: u32,
    pub thresholds: GeometryThresholds,
    pub mouth_thresholds: MouthThresholds,
    pub geometry: GeometryAssessment,
    pub mouth: MouthAssessment,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeometryThresholds {
    /// Center residual as a fraction of the presentation-crop diagonal.
    pub center_residual: f64,
    /// Maximum absolute log width, height, or aspect residual from the path.
    pub log_size_residual: f64,
    /// Eye/nose feature residual in eye-distance units.
    pub feature_residual: f64,
    pub sustained_frames: u32,
    /// Minimum detector confidence required for a measured track.
    pub minimum_confidence: f64,
    pub minimum_landmark_confidence: f64,
    /// Minimum eye separation as a fraction of the shorter presentation edge.
    pub minimum_eye_separation_fraction: f64,
    pub association_max_score: f64,
    pub association_uniqueness_margin: f64,
    /// Intersections at or above this fraction of the smaller box are
    /// ambiguous and cannot form an identity link.
    pub association_overlap_ambiguity: f64,
}

impl GeometryThresholds {
    pub const fn policy() -> Self {
        Self {
            center_residual: GEOMETRY_CENTER_LIMIT,
            log_size_residual: GEOMETRY_LOG_SCALE_LIMIT,
            feature_residual: GEOMETRY_FEATURE_LIMIT,
            sustained_frames: SUSTAINED_EXCESS_FRAMES,
            minimum_confidence: MIN_FACE_CONFIDENCE,
            minimum_landmark_confidence: MIN_LANDMARK_CONFIDENCE,
            minimum_eye_separation_fraction: MOUTH_EYE_SEPARATION_MIN_FRACTION,
            association_max_score: ASSOCIATION_MAX_SCORE,
            association_uniqueness_margin: ASSOCIATION_UNIQUENESS_MARGIN,
            association_overlap_ambiguity: ASSOCIATION_OVERLAP_AMBIGUITY,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MouthThresholds {
    /// Aperture change divided by inter-eye distance.
    pub aperture_step: f64,
    /// Consecutive native-frame changes required for rejection.
    pub sustained_changes: u32,
    pub minimum_landmark_confidence: f64,
    /// Minimum eye separation as a fraction of the shorter presentation edge.
    pub minimum_eye_separation_fraction: f64,
}

impl MouthThresholds {
    pub const fn policy() -> Self {
        Self {
            aperture_step: 0.18,
            sustained_changes: SUSTAINED_EXCESS_FRAMES,
            minimum_landmark_confidence: MIN_LANDMARK_CONFIDENCE,
            minimum_eye_separation_fraction: MOUTH_EYE_SEPARATION_MIN_FRACTION,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeometryAssessment {
    pub status: CheckStatus,
    pub measured_tracks: u8,
    pub unavailable_tracks: u8,
    pub feature_unavailable_tracks: u8,
    pub maximum_center_residual: Option<f64>,
    pub maximum_log_size_residual: Option<f64>,
    pub maximum_feature_residual: Option<f64>,
    pub rejection: Option<GeometryRejection>,
    pub unavailable_reasons: Vec<GeometryUnavailableReason>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Measured,
    Rejected,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeometryRejection {
    pub first_ordinal: u32,
    pub consecutive_frames: u32,
    pub center_residual: f64,
    pub log_size_residual: f64,
    pub feature_residual: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeometryUnavailableReason {
    BoundaryObservationsMissing,
    AuthoredBlackBoundary,
    NoLeftBoundaryFace,
    NoRightBoundaryFace,
    NoFaceDetected,
    OutsidePresentation,
    TooManyFaces,
    InvalidGeometry,
    LowConfidence,
    AmbiguousAssociation,
    LostTrack,
    MissingEyeOrNoseLandmarks,
    UnsupportedLandmarks,
    InsufficientResolution,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MouthAssessment {
    pub basis: MouthBasis,
    pub status: CheckStatus,
    /// Continuous qualified segments, including faces that enter later.
    pub measured_tracks: u32,
    pub unavailable_tracks: u32,
    pub leading_unobserved_frames: u32,
    pub maximum_aperture_step: Option<f64>,
    pub rejection: Option<MouthRejection>,
    pub unavailable_reasons: Vec<MouthUnavailableReason>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MouthBasis {
    NativeFramesOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MouthRejection {
    pub first_after_ordinal: u32,
    pub consecutive_changes: u32,
    /// Aperture change divided by eye separation, roll-normalized.
    pub aperture_step: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MouthUnavailableReason {
    NoFaceDetected,
    OutsidePresentation,
    TooManyFaces,
    InvalidGeometry,
    LowConfidence,
    AmbiguousAssociation,
    LostTrack,
    MissingEyeLandmarks,
    UnsupportedLandmarks,
    InsufficientResolution,
    MissingInnerLipLandmarks,
    InsufficientFrames,
}

/// Evaluate native frames and both usable conditioning anchors. Mouth motion
/// is measured only across the native sequence, so it remains available when
/// a conditioning boundary is authored black. Coordinates are top-left
/// normalized on the full raster; the presentation crop and raster size are
/// required to exclude padding and preserve aspect-correct distances.
pub fn analyze(
    batch: &RawLandmarkBatch,
    expected_native_pts: &[i64],
    raster: [u32; 2],
    presentation: [u32; 4],
    boundary_usable: [bool; 2],
) -> Result<GeneratedGeometryReport, GeometryError> {
    batch.validate(expected_native_pts)?;
    validate_raster(raster)?;
    validate_presentation(raster, presentation)?;
    let native_frames = u32::try_from(batch.frames.len()).map_err(|_| GeometryError::Raster)?;
    let thresholds = GeometryThresholds::policy();
    let mouth_thresholds = MouthThresholds::policy();
    let geometry = analyze_geometry(batch, raster, presentation, boundary_usable, thresholds);
    let mouth = analyze_mouth(batch, raster, presentation, mouth_thresholds);
    Ok(GeneratedGeometryReport {
        schema_version: 1,
        profile: PROFILE.into(),
        native_frames,
        thresholds,
        mouth_thresholds,
        geometry,
        mouth,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeometryError {
    RawObservation(RawObservationError),
    Raster,
    Presentation,
    PolicyMismatch,
}

impl From<RawObservationError> for GeometryError {
    fn from(value: RawObservationError) -> Self {
        Self::RawObservation(value)
    }
}

impl std::fmt::Display for GeometryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RawObservation(error) => error.fmt(f),
            Self::Raster => f.write_str("landmark raster dimensions are outside the bound"),
            Self::Presentation => {
                f.write_str("presentation rectangle is outside the landmark raster")
            }
            Self::PolicyMismatch => {
                f.write_str("retained geometry result differs from recomputed policy")
            }
        }
    }
}

impl std::error::Error for GeometryError {}

fn validate_raster(raster: [u32; 2]) -> Result<(), GeometryError> {
    let pixels = u64::from(raster[0]) * u64::from(raster[1]);
    if raster[0] == 0 || raster[1] == 0 || pixels > MAX_RASTER_PIXELS {
        return Err(GeometryError::Raster);
    }
    Ok(())
}

fn validate_presentation(raster: [u32; 2], rect: [u32; 4]) -> Result<(), GeometryError> {
    let [x, y, width, height] = rect;
    if width == 0
        || height == 0
        || x.checked_add(width).is_none_or(|right| right > raster[0])
        || y.checked_add(height)
            .is_none_or(|bottom| bottom > raster[1])
    {
        return Err(GeometryError::Presentation);
    }
    Ok(())
}

impl GeneratedGeometryReport {
    /// Recompute evidence at stored-report admission and require exact policy
    /// output equality. The host separately binds the input object identities.
    pub fn validate_recomputed(
        &self,
        batch: &RawLandmarkBatch,
        expected_native_pts: &[i64],
        raster: [u32; 2],
        presentation: [u32; 4],
        boundary_usable: [bool; 2],
    ) -> Result<(), GeometryError> {
        if *self
            != analyze(
                batch,
                expected_native_pts,
                raster,
                presentation,
                boundary_usable,
            )?
        {
            return Err(GeometryError::PolicyMismatch);
        }
        Ok(())
    }
}

fn analyze_geometry(
    batch: &RawLandmarkBatch,
    raster: [u32; 2],
    presentation: [u32; 4],
    boundary_usable: [bool; 2],
    thresholds: GeometryThresholds,
) -> GeometryAssessment {
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
    let Some(boundaries) = &batch.boundaries else {
        push_geometry_reason(
            &mut report.unavailable_reasons,
            GeometryUnavailableReason::BoundaryObservationsMissing,
        );
        return report;
    };
    if boundary_usable != [true, true] {
        push_geometry_reason(
            &mut report.unavailable_reasons,
            GeometryUnavailableReason::AuthoredBlackBoundary,
        );
        return report;
    }

    let mut sets = Vec::with_capacity(batch.frames.len() + 2);
    sets.push(EligibleFaceSet::new(&boundaries.left, raster, presentation));
    sets.extend(
        batch
            .frames
            .iter()
            .map(|frame| EligibleFaceSet::new(&frame.observation, raster, presentation)),
    );
    sets.push(EligibleFaceSet::new(
        &boundaries.right,
        raster,
        presentation,
    ));

    if sets[0].faces.is_empty() {
        let reason = sets[0]
            .missing_reason()
            .filter(|reason| *reason != GeometryUnavailableReason::NoFaceDetected)
            .unwrap_or(GeometryUnavailableReason::NoLeftBoundaryFace);
        push_geometry_reason(&mut report.unavailable_reasons, reason);
        return report;
    }
    if sets.last().is_some_and(|set| set.faces.is_empty()) {
        let last = sets.last().expect("the two boundary sets are present");
        let reason = last
            .missing_reason()
            .filter(|reason| *reason != GeometryUnavailableReason::NoFaceDetected)
            .unwrap_or(GeometryUnavailableReason::NoRightBoundaryFace);
        push_geometry_reason(&mut report.unavailable_reasons, reason);
        return report;
    }

    let mut first_rejection = None;
    for left_index in 0..sets[0].faces.len() {
        let Some(track) = associate_track(&sets, left_index, thresholds.minimum_confidence) else {
            report.unavailable_tracks = report.unavailable_tracks.saturating_add(1);
            push_geometry_reason(
                &mut report.unavailable_reasons,
                GeometryUnavailableReason::AmbiguousAssociation,
            );
            continue;
        };
        if !track.complete {
            report.unavailable_tracks = report.unavailable_tracks.saturating_add(1);
            push_geometry_reason(
                &mut report.unavailable_reasons,
                track.reason.unwrap_or(GeometryUnavailableReason::LostTrack),
            );
            continue;
        }
        let metrics = track_geometry_metrics(&track, &sets, raster, presentation, thresholds);
        report.measured_tracks = report.measured_tracks.saturating_add(1);
        update_max(&mut report.maximum_center_residual, metrics.max_center);
        update_max(&mut report.maximum_log_size_residual, metrics.max_log_size);
        if let Some(feature) = metrics.max_feature {
            update_max(&mut report.maximum_feature_residual, feature);
        }
        if !metrics.feature_unavailable.is_empty() {
            report.feature_unavailable_tracks = report.feature_unavailable_tracks.saturating_add(1);
            for reason in metrics.feature_unavailable {
                push_geometry_reason(&mut report.unavailable_reasons, reason);
            }
        }
        if let Some(rejection) = metrics.rejection
            && first_rejection.is_none_or(|current: GeometryRejection| {
                rejection.first_ordinal < current.first_ordinal
            })
        {
            first_rejection = Some(rejection);
        }
    }
    report.rejection = first_rejection;
    report.status = if first_rejection.is_some() {
        CheckStatus::Rejected
    } else if report.measured_tracks > 0 {
        CheckStatus::Measured
    } else {
        CheckStatus::Unavailable
    };
    report
}

#[derive(Clone, Copy)]
struct EligibleFace<'a> {
    face: &'a FaceLandmarks,
    rect: LocalRect,
}

struct EligibleFaceSet<'a> {
    faces: Vec<EligibleFace<'a>>,
    outside_presentation: bool,
    unavailable: Option<FaceSetUnavailableReason>,
}

impl<'a> EligibleFaceSet<'a> {
    fn new(observation: &'a FaceObservationSet, raster: [u32; 2], presentation: [u32; 4]) -> Self {
        let mut result = Self {
            faces: Vec::new(),
            outside_presentation: false,
            unavailable: None,
        };
        match observation {
            FaceObservationSet::Unavailable { reason } => result.unavailable = Some(*reason),
            FaceObservationSet::Detected { faces } => {
                for face in faces {
                    if let Some(rect) = local_rect(&face.region, raster, presentation) {
                        result.faces.push(EligibleFace { face, rect });
                    } else {
                        result.outside_presentation = true;
                    }
                }
            }
        }
        result
    }

    fn missing_reason(&self) -> Option<GeometryUnavailableReason> {
        if self.outside_presentation {
            return Some(GeometryUnavailableReason::OutsidePresentation);
        }
        if let Some(reason) = self.unavailable {
            return Some(geometry_set_reason(reason));
        }
        Some(GeometryUnavailableReason::NoFaceDetected)
    }
}

#[derive(Debug, Clone, Copy)]
struct LocalRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl LocalRect {
    fn center(self) -> (f64, f64) {
        (self.x + self.width * 0.5, self.y + self.height * 0.5)
    }
}

fn local_rect(
    rect: &NormalizedRect,
    raster: [u32; 2],
    presentation: [u32; 4],
) -> Option<LocalRect> {
    let [crop_x, crop_y, crop_width, crop_height] = presentation;
    let x = rect.x() * f64::from(raster[0]);
    let y = rect.y() * f64::from(raster[1]);
    let width = rect.width() * f64::from(raster[0]);
    let height = rect.height() * f64::from(raster[1]);
    let (left, top) = (f64::from(crop_x), f64::from(crop_y));
    let (right, bottom) = (left + f64::from(crop_width), top + f64::from(crop_height));
    if x < left || y < top || x + width > right || y + height > bottom {
        return None;
    }
    Some(LocalRect {
        x: (x - left) / f64::from(crop_width),
        y: (y - top) / f64::from(crop_height),
        width: width / f64::from(crop_width),
        height: height / f64::from(crop_height),
    })
}

fn local_point(point: [f64; 2], raster: [u32; 2], presentation: [u32; 4]) -> Option<[f64; 2]> {
    let [crop_x, crop_y, crop_width, crop_height] = presentation;
    let x = point[0] * f64::from(raster[0]);
    let y = point[1] * f64::from(raster[1]);
    let left = f64::from(crop_x);
    let top = f64::from(crop_y);
    if x < left || y < top || x > left + f64::from(crop_width) || y > top + f64::from(crop_height) {
        return None;
    }
    Some([
        (x - left) / f64::from(crop_width),
        (y - top) / f64::from(crop_height),
    ])
}

fn geometry_set_reason(reason: FaceSetUnavailableReason) -> GeometryUnavailableReason {
    match reason {
        FaceSetUnavailableReason::TooManyFaces => GeometryUnavailableReason::TooManyFaces,
        FaceSetUnavailableReason::InvalidGeometry => GeometryUnavailableReason::InvalidGeometry,
    }
}

#[derive(Clone)]
struct Track {
    indices: Vec<usize>,
    complete: bool,
    reason: Option<GeometryUnavailableReason>,
}

fn associate_track(
    sets: &[EligibleFaceSet<'_>],
    first_index: usize,
    minimum_confidence: f64,
) -> Option<Track> {
    if sets.len() < 2 || first_index >= sets[0].faces.len() {
        return None;
    }
    let mut track = Track {
        indices: vec![usize::MAX; sets.len()],
        complete: true,
        reason: None,
    };
    track.indices[0] = first_index;
    for pair_index in 0..sets.len() - 1 {
        let current = &sets[pair_index];
        let next = &sets[pair_index + 1];
        let current_index = track.indices[pair_index];
        if current_index == usize::MAX || current_index >= current.faces.len() {
            track.complete = false;
            track.reason = Some(GeometryUnavailableReason::LostTrack);
            break;
        }
        if f64::from(current.faces[current_index].face.confidence) < minimum_confidence {
            track.complete = false;
            track.reason = Some(GeometryUnavailableReason::LowConfidence);
            break;
        }
        match unique_mutual_match(current_index, current, next, minimum_confidence) {
            MatchResult::Unique(next_index) => track.indices[pair_index + 1] = next_index,
            MatchResult::Ambiguous => {
                track.complete = false;
                track.reason = Some(GeometryUnavailableReason::AmbiguousAssociation);
                break;
            }
            MatchResult::Missing(reason) => {
                track.complete = false;
                track.reason = Some(reason);
                break;
            }
            MatchResult::LowConfidence => {
                track.complete = false;
                track.reason = Some(GeometryUnavailableReason::LowConfidence);
                break;
            }
        }
    }
    if track.complete {
        let last = sets.len() - 1;
        let index = track.indices[last];
        if index == usize::MAX || index >= sets[last].faces.len() {
            track.complete = false;
            track.reason = Some(GeometryUnavailableReason::LostTrack);
        } else if f64::from(sets[last].faces[index].face.confidence) < minimum_confidence {
            track.complete = false;
            track.reason = Some(GeometryUnavailableReason::LowConfidence);
        }
    }
    Some(track)
}

enum MatchResult {
    Unique(usize),
    Ambiguous,
    Missing(GeometryUnavailableReason),
    LowConfidence,
}

fn unique_mutual_match(
    source_index: usize,
    source_set: &EligibleFaceSet<'_>,
    target_set: &EligibleFaceSet<'_>,
    minimum_confidence: f64,
) -> MatchResult {
    if source_set.faces.is_empty() || target_set.faces.is_empty() {
        return MatchResult::Missing(
            target_set
                .missing_reason()
                .unwrap_or(GeometryUnavailableReason::LostTrack),
        );
    }
    let Some(source) = source_set.faces.get(source_index) else {
        return MatchResult::Ambiguous;
    };
    let Some((target_index, _)) = unique_best(source, &target_set.faces) else {
        return MatchResult::Ambiguous;
    };
    if overlaps_competing_face(source_index, &source_set.faces)
        || overlaps_competing_face(target_index, &target_set.faces)
    {
        return MatchResult::Ambiguous;
    }
    if f64::from(target_set.faces[target_index].face.confidence) < minimum_confidence {
        return MatchResult::LowConfidence;
    }
    let Some((reverse_index, _)) = unique_best(&target_set.faces[target_index], &source_set.faces)
    else {
        return MatchResult::Ambiguous;
    };
    if reverse_index != source_index {
        return MatchResult::Ambiguous;
    }
    MatchResult::Unique(target_index)
}

fn overlaps_competing_face(index: usize, faces: &[EligibleFace<'_>]) -> bool {
    let Some(face) = faces.get(index) else {
        return true;
    };
    faces.iter().enumerate().any(|(other_index, other)| {
        if other_index == index {
            return false;
        }
        overlap_of_smaller(face.rect, other.rect) >= ASSOCIATION_OVERLAP_AMBIGUITY
    })
}

fn overlap_of_smaller(left: LocalRect, right: LocalRect) -> f64 {
    let width = (left.x + left.width).min(right.x + right.width) - left.x.max(right.x);
    let height = (left.y + left.height).min(right.y + right.height) - left.y.max(right.y);
    if width <= 0.0 || height <= 0.0 {
        return 0.0;
    }
    let intersection = width * height;
    intersection / (left.width * left.height).min(right.width * right.height)
}

fn unique_best(query: &EligibleFace<'_>, candidates: &[EligibleFace<'_>]) -> Option<(usize, f64)> {
    let mut best_index = None;
    let (mut best, mut second) = (f64::INFINITY, f64::INFINITY);
    for (index, candidate) in candidates.iter().enumerate() {
        let score = association_score(query.rect, candidate.rect);
        if score < best {
            second = best;
            best = score;
            best_index = Some(index);
        } else if score < second {
            second = score;
        }
    }
    let index = best_index?;
    if best > ASSOCIATION_MAX_SCORE || second - best < ASSOCIATION_UNIQUENESS_MARGIN {
        None
    } else {
        Some((index, best))
    }
}

fn association_score(left: LocalRect, right: LocalRect) -> f64 {
    let (left_x, left_y) = left.center();
    let (right_x, right_y) = right.center();
    let mean_width = (left.width + right.width) * 0.5;
    let mean_height = (left.height + right.height) * 0.5;
    let dx = (left_x - right_x) / mean_width.max(1e-9);
    let dy = (left_y - right_y) / mean_height.max(1e-9);
    let width_scale = (left.width / right.width).ln().abs();
    let height_scale = (left.height / right.height).ln().abs();
    dx.hypot(dy) + 0.35 * width_scale.hypot(height_scale)
}

#[derive(Clone, Copy)]
struct ShapeFeatures {
    nose: [f64; 2],
    eye_scale: f64,
}

#[derive(Clone)]
struct GeometryMetrics {
    max_center: f64,
    max_log_size: f64,
    max_feature: Option<f64>,
    feature_unavailable: Vec<GeometryUnavailableReason>,
    rejection: Option<GeometryRejection>,
}

fn track_geometry_metrics(
    track: &Track,
    sets: &[EligibleFaceSet<'_>],
    raster: [u32; 2],
    presentation: [u32; 4],
    thresholds: GeometryThresholds,
) -> GeometryMetrics {
    let native_count = sets.len() - 2;
    let left = sets[0].faces[track.indices[0]];
    let right = sets[sets.len() - 1].faces[*track.indices.last().unwrap()];
    let left_feature = shape_features(left, raster, presentation, thresholds);
    let right_feature = shape_features(right, raster, presentation, thresholds);
    let mut max_center: f64 = 0.0;
    let mut max_log_size: f64 = 0.0;
    let mut max_feature: Option<f64> = None;
    let mut first_rejection: Option<GeometryRejection> = None;
    let mut excess_run = 0_u32;
    let mut run_start = 0_u32;
    let mut feature_unavailable = Vec::new();
    for feature in [left_feature, right_feature] {
        if let Err(reason) = feature {
            push_geometry_reason(&mut feature_unavailable, reason.geometry());
        }
    }

    for ordinal in 0..native_count {
        let actual = sets[ordinal + 1].faces[track.indices[ordinal + 1]];
        let fraction = ordinal as f64 / (native_count - 1) as f64;
        let expected = interpolate_rect(left.rect, right.rect, fraction);
        let center = center_residual(actual.rect, expected, presentation);
        let log_size = log_size_residual(actual.rect, expected);
        max_center = max_center.max(center);
        max_log_size = max_log_size.max(log_size);

        let actual_feature = shape_features(actual, raster, presentation, thresholds);
        if let Err(reason) = actual_feature {
            push_geometry_reason(&mut feature_unavailable, reason.geometry());
        }
        let feature = match (left_feature, right_feature, actual_feature) {
            (Ok(a), Ok(b), Ok(value)) => {
                let expected_nose = [
                    lerp(a.nose[0], b.nose[0], fraction),
                    lerp(a.nose[1], b.nose[1], fraction),
                ];
                let expected_eye_scale = lerp(a.eye_scale.ln(), b.eye_scale.ln(), fraction).exp();
                let nose_error =
                    (value.nose[0] - expected_nose[0]).hypot(value.nose[1] - expected_nose[1]);
                let eye_error = (value.eye_scale / expected_eye_scale).ln().abs();
                let residual = nose_error.max(eye_error);
                update_max(&mut max_feature, residual);
                Some(residual)
            }
            _ => None,
        };

        let feature_excess = feature.is_some_and(|value| value > thresholds.feature_residual);
        let excess = center > thresholds.center_residual
            || log_size > thresholds.log_size_residual
            || feature_excess;
        if excess {
            if excess_run == 0 {
                run_start = ordinal as u32;
            }
            excess_run = excess_run.saturating_add(1);
            if excess_run >= thresholds.sustained_frames && first_rejection.is_none() {
                first_rejection = Some(GeometryRejection {
                    first_ordinal: run_start,
                    consecutive_frames: excess_run,
                    center_residual: center,
                    log_size_residual: log_size,
                    feature_residual: feature,
                });
            }
        } else {
            excess_run = 0;
        }
    }
    GeometryMetrics {
        max_center,
        max_log_size,
        max_feature,
        feature_unavailable,
        rejection: first_rejection,
    }
}

fn interpolate_rect(left: LocalRect, right: LocalRect, fraction: f64) -> LocalRect {
    LocalRect {
        x: lerp(left.x, right.x, fraction),
        y: lerp(left.y, right.y, fraction),
        width: lerp(left.width, right.width, fraction),
        height: lerp(left.height, right.height, fraction),
    }
}

fn center_residual(actual: LocalRect, expected: LocalRect, presentation: [u32; 4]) -> f64 {
    let (ax, ay) = actual.center();
    let (ex, ey) = expected.center();
    let width = f64::from(presentation[2]);
    let height = f64::from(presentation[3]);
    ((ax - ex) * width).hypot((ay - ey) * height) / width.hypot(height)
}

fn log_size_residual(actual: LocalRect, expected: LocalRect) -> f64 {
    let width = (actual.width / expected.width).ln().abs();
    let height = (actual.height / expected.height).ln().abs();
    let aspect = ((actual.width / actual.height) / (expected.width / expected.height))
        .ln()
        .abs();
    width.max(height).max(aspect)
}

fn shape_features(
    face: EligibleFace<'_>,
    raster: [u32; 2],
    presentation: [u32; 4],
    thresholds: GeometryThresholds,
) -> Result<ShapeFeatures, FeatureUnavailable> {
    let LandmarkAvailability::Available {
        confidence,
        left_eye,
        right_eye,
        nose,
        ..
    } = &face.face.landmarks
    else {
        let LandmarkAvailability::Unavailable { reason } = face.face.landmarks else {
            unreachable!()
        };
        return Err(FeatureUnavailable::from_raw(reason));
    };
    if f64::from(*confidence) < thresholds.minimum_landmark_confidence {
        return Err(FeatureUnavailable::LowConfidence);
    }
    let left = feature_centroid(left_eye, raster, presentation)?;
    let right = feature_centroid(right_eye, raster, presentation)?;
    let nose = feature_centroid(nose, raster, presentation)?;
    // Convert normalized presentation coordinates into aspect-correct pixels.
    let [_, _, crop_width, crop_height] = presentation;
    let to_pixel = |point: [f64; 2]| {
        [
            point[0] * f64::from(crop_width),
            point[1] * f64::from(crop_height),
        ]
    };
    let (left, right, nose) = (to_pixel(left), to_pixel(right), to_pixel(nose));
    let axis_vector = [right[0] - left[0], right[1] - left[1]];
    let eye_distance = axis_vector[0].hypot(axis_vector[1]);
    if eye_distance
        < f64::from(crop_width.min(crop_height)) * thresholds.minimum_eye_separation_fraction
    {
        return Err(FeatureUnavailable::InsufficientResolution);
    }
    let axis = [axis_vector[0] / eye_distance, axis_vector[1] / eye_distance];
    let normal = [-axis[1], axis[0]];
    let midpoint = [(left[0] + right[0]) * 0.5, (left[1] + right[1]) * 0.5];
    let nose_delta = [nose[0] - midpoint[0], nose[1] - midpoint[1]];
    let nose_in_eye_units = [
        (nose_delta[0] * axis[0] + nose_delta[1] * axis[1]) / eye_distance,
        (nose_delta[0] * normal[0] + nose_delta[1] * normal[1]) / eye_distance,
    ];
    let face_area_sqrt =
        (face.rect.width * f64::from(crop_width) * face.rect.height * f64::from(crop_height))
            .sqrt();
    if face_area_sqrt <= 0.0 || !eye_distance.is_finite() {
        return Err(FeatureUnavailable::InvalidGeometry);
    }
    Ok(ShapeFeatures {
        nose: nose_in_eye_units,
        eye_scale: eye_distance / face_area_sqrt,
    })
}

fn feature_centroid(
    region: &LandmarkRegion,
    raster: [u32; 2],
    presentation: [u32; 4],
) -> Result<[f64; 2], FeatureUnavailable> {
    let LandmarkRegion::Detected { points } = region else {
        let LandmarkRegion::Unavailable { reason } = region else {
            unreachable!()
        };
        return Err(FeatureUnavailable::from_raw(*reason));
    };
    let mut sum = [0.0, 0.0];
    for point in points {
        let local = local_point(*point, raster, presentation)
            .ok_or(FeatureUnavailable::OutsidePresentation)?;
        sum[0] += local[0];
        sum[1] += local[1];
    }
    let count = points.len() as f64;
    Ok([sum[0] / count, sum[1] / count])
}

#[derive(Clone, Copy)]
enum FeatureUnavailable {
    Missing,
    InvalidGeometry,
    LowConfidence,
    Unsupported,
    OutsidePresentation,
    InsufficientResolution,
}

impl FeatureUnavailable {
    fn from_raw(reason: LandmarkUnavailableReason) -> Self {
        match reason {
            LandmarkUnavailableReason::Missing => Self::Missing,
            LandmarkUnavailableReason::InvalidGeometry => Self::InvalidGeometry,
            LandmarkUnavailableReason::LowConfidence => Self::LowConfidence,
            LandmarkUnavailableReason::Unsupported => Self::Unsupported,
        }
    }
    fn geometry(self) -> GeometryUnavailableReason {
        match self {
            Self::Missing => GeometryUnavailableReason::MissingEyeOrNoseLandmarks,
            Self::InvalidGeometry => GeometryUnavailableReason::InvalidGeometry,
            Self::LowConfidence => GeometryUnavailableReason::LowConfidence,
            Self::Unsupported => GeometryUnavailableReason::UnsupportedLandmarks,
            Self::OutsidePresentation => GeometryUnavailableReason::OutsidePresentation,
            Self::InsufficientResolution => GeometryUnavailableReason::InsufficientResolution,
        }
    }
    fn mouth(self, lips: bool) -> MouthUnavailableReason {
        match self {
            Self::Missing if lips => MouthUnavailableReason::MissingInnerLipLandmarks,
            Self::Missing => MouthUnavailableReason::MissingEyeLandmarks,
            Self::InvalidGeometry => MouthUnavailableReason::InvalidGeometry,
            Self::LowConfidence => MouthUnavailableReason::LowConfidence,
            Self::Unsupported => MouthUnavailableReason::UnsupportedLandmarks,
            Self::OutsidePresentation => MouthUnavailableReason::OutsidePresentation,
            Self::InsufficientResolution => MouthUnavailableReason::InsufficientResolution,
        }
    }
}

fn analyze_mouth(
    batch: &RawLandmarkBatch,
    raster: [u32; 2],
    presentation: [u32; 4],
    thresholds: MouthThresholds,
) -> MouthAssessment {
    let mut report = MouthAssessment {
        basis: MouthBasis::NativeFramesOnly,
        status: CheckStatus::Unavailable,
        measured_tracks: 0,
        unavailable_tracks: 0,
        leading_unobserved_frames: 0,
        maximum_aperture_step: None,
        rejection: None,
        unavailable_reasons: Vec::new(),
    };
    let sets: Vec<EligibleFaceSet<'_>> = batch
        .frames
        .iter()
        .map(|frame| EligibleFaceSet::new(&frame.observation, raster, presentation))
        .collect();
    let first = sets.iter().position(|set| !set.faces.is_empty());
    let Some(first) = first else {
        report.leading_unobserved_frames = batch.frames.len() as u32;
        let reason = sets
            .iter()
            .find_map(|set| {
                set.missing_reason()
                    .filter(|reason| *reason != GeometryUnavailableReason::NoFaceDetected)
            })
            .map(mouth_geometry_reason)
            .unwrap_or(MouthUnavailableReason::NoFaceDetected);
        push_mouth_reason(&mut report.unavailable_reasons, reason);
        return report;
    };
    report.leading_unobserved_frames = first as u32;
    if first > 0 {
        let reason = sets[..first]
            .iter()
            .find_map(|set| {
                set.missing_reason()
                    .filter(|reason| *reason != GeometryUnavailableReason::NoFaceDetected)
            })
            .map(mouth_geometry_reason)
            .unwrap_or(MouthUnavailableReason::NoFaceDetected);
        push_mouth_reason(&mut report.unavailable_reasons, reason);
    }

    let mut visited: Vec<Vec<bool>> = sets
        .iter()
        .map(|set| vec![false; set.faces.len()])
        .collect();
    let mut first_rejection = None;
    for start in first..sets.len() {
        for face_index in 0..sets[start].faces.len() {
            if visited[start][face_index] {
                continue;
            }
            let (apertures, failure) = mouth_segment(
                start,
                face_index,
                &sets,
                &mut visited,
                raster,
                presentation,
                thresholds,
            );
            let enough_frames = apertures.len() > thresholds.sustained_changes as usize;
            if failure.is_some() || !enough_frames {
                // At most one segment per bounded native face observation.
                report.unavailable_tracks += 1;
            }
            if let Some(reason) = failure {
                push_mouth_reason(&mut report.unavailable_reasons, reason);
            }
            if !enough_frames {
                push_mouth_reason(
                    &mut report.unavailable_reasons,
                    MouthUnavailableReason::InsufficientFrames,
                );
                continue;
            }
            // A later occlusion or missing landmark cannot erase earlier
            // qualified motion. Never compare across that interruption.
            let metrics = mouth_track_metrics(start as u32, &apertures, thresholds);
            report.measured_tracks += 1;
            update_max(&mut report.maximum_aperture_step, metrics.maximum_step);
            if let Some(rejection) = metrics.rejection
                && first_rejection.is_none_or(|current: MouthRejection| {
                    rejection.first_after_ordinal < current.first_after_ordinal
                })
            {
                first_rejection = Some(rejection);
            }
        }
    }
    report.rejection = first_rejection;
    report.status = if first_rejection.is_some() {
        CheckStatus::Rejected
    } else if report.measured_tracks > 0 {
        CheckStatus::Measured
    } else {
        CheckStatus::Unavailable
    };
    report
}

fn mouth_segment(
    start: usize,
    mut face_index: usize,
    sets: &[EligibleFaceSet<'_>],
    visited: &mut [Vec<bool>],
    raster: [u32; 2],
    presentation: [u32; 4],
    thresholds: MouthThresholds,
) -> (Vec<f64>, Option<MouthUnavailableReason>) {
    let mut apertures = Vec::new();
    for ordinal in start..sets.len() {
        if visited[ordinal][face_index] {
            return (
                apertures,
                Some(MouthUnavailableReason::AmbiguousAssociation),
            );
        }
        visited[ordinal][face_index] = true;
        let face = sets[ordinal].faces[face_index];
        if f64::from(face.face.confidence) < MIN_FACE_CONFIDENCE {
            return (apertures, Some(MouthUnavailableReason::LowConfidence));
        }
        match mouth_aperture(face, raster, presentation, thresholds) {
            Ok(aperture) => apertures.push(aperture),
            Err(reason) => return (apertures, Some(reason)),
        }
        let Some(next) = sets.get(ordinal + 1) else {
            break;
        };
        match unique_mutual_match(face_index, &sets[ordinal], next, MIN_FACE_CONFIDENCE) {
            MatchResult::Unique(index) => face_index = index,
            MatchResult::Ambiguous => {
                return (
                    apertures,
                    Some(MouthUnavailableReason::AmbiguousAssociation),
                );
            }
            MatchResult::Missing(reason) => {
                return (apertures, Some(mouth_geometry_reason(reason)));
            }
            MatchResult::LowConfidence => {
                return (apertures, Some(MouthUnavailableReason::LowConfidence));
            }
        }
    }
    (apertures, None)
}

struct MouthMetrics {
    maximum_step: f64,
    rejection: Option<MouthRejection>,
}

fn mouth_track_metrics(
    start_ordinal: u32,
    apertures: &[f64],
    thresholds: MouthThresholds,
) -> MouthMetrics {
    let mut maximum_step: f64 = 0.0;
    let mut run = 0_u32;
    let mut run_start = 0_u32;
    let mut rejection = None;
    for index in 1..apertures.len() {
        let step = (apertures[index] - apertures[index - 1]).abs();
        maximum_step = maximum_step.max(step);
        if step > thresholds.aperture_step {
            if run == 0 {
                run_start = (index - 1) as u32;
            }
            run = run.saturating_add(1);
            if run >= thresholds.sustained_changes && rejection.is_none() {
                rejection = Some(MouthRejection {
                    first_after_ordinal: start_ordinal + run_start + 1,
                    consecutive_changes: run,
                    aperture_step: step,
                });
            }
        } else {
            run = 0;
        }
    }
    MouthMetrics {
        maximum_step,
        rejection,
    }
}

fn mouth_aperture(
    face: EligibleFace<'_>,
    raster: [u32; 2],
    presentation: [u32; 4],
    thresholds: MouthThresholds,
) -> Result<f64, MouthUnavailableReason> {
    let LandmarkAvailability::Available {
        confidence,
        left_eye,
        right_eye,
        inner_lips,
        ..
    } = &face.face.landmarks
    else {
        let LandmarkAvailability::Unavailable { reason } = face.face.landmarks else {
            unreachable!()
        };
        return Err(FeatureUnavailable::from_raw(reason).mouth(false));
    };
    if f64::from(*confidence) < thresholds.minimum_landmark_confidence {
        return Err(MouthUnavailableReason::LowConfidence);
    }
    let left =
        feature_centroid(left_eye, raster, presentation).map_err(|reason| reason.mouth(false))?;
    let right =
        feature_centroid(right_eye, raster, presentation).map_err(|reason| reason.mouth(false))?;
    let [_, _, crop_width, crop_height] = presentation;
    let scale = [f64::from(crop_width), f64::from(crop_height)];
    let left = [left[0] * scale[0], left[1] * scale[1]];
    let right = [right[0] * scale[0], right[1] * scale[1]];
    let vector = [right[0] - left[0], right[1] - left[1]];
    let eye_distance = vector[0].hypot(vector[1]);
    let minimum_eye_distance =
        f64::from(crop_width.min(crop_height)) * thresholds.minimum_eye_separation_fraction;
    if eye_distance < minimum_eye_distance {
        return Err(MouthUnavailableReason::InsufficientResolution);
    }
    let axis = [vector[0] / eye_distance, vector[1] / eye_distance];
    let normal = [-axis[1], axis[0]];
    let LandmarkRegion::Detected { points } = inner_lips else {
        let LandmarkRegion::Unavailable { reason } = inner_lips else {
            unreachable!()
        };
        return Err(FeatureUnavailable::from_raw(*reason).mouth(true));
    };
    if points.len() < 2 {
        return Err(MouthUnavailableReason::MissingInnerLipLandmarks);
    }
    let mut low = f64::INFINITY;
    let mut high = f64::NEG_INFINITY;
    for point in points {
        let local = local_point(*point, raster, presentation)
            .ok_or(MouthUnavailableReason::OutsidePresentation)?;
        let pixel = [local[0] * scale[0], local[1] * scale[1]];
        let projection = pixel[0] * normal[0] + pixel[1] * normal[1];
        low = low.min(projection);
        high = high.max(projection);
    }
    let aperture = (high - low) / eye_distance;
    if aperture.is_finite() {
        Ok(aperture)
    } else {
        Err(MouthUnavailableReason::InvalidGeometry)
    }
}

fn mouth_geometry_reason(reason: GeometryUnavailableReason) -> MouthUnavailableReason {
    match reason {
        GeometryUnavailableReason::AmbiguousAssociation => {
            MouthUnavailableReason::AmbiguousAssociation
        }
        GeometryUnavailableReason::LowConfidence => MouthUnavailableReason::LowConfidence,
        GeometryUnavailableReason::TooManyFaces => MouthUnavailableReason::TooManyFaces,
        GeometryUnavailableReason::InvalidGeometry => MouthUnavailableReason::InvalidGeometry,
        GeometryUnavailableReason::OutsidePresentation => {
            MouthUnavailableReason::OutsidePresentation
        }
        GeometryUnavailableReason::NoFaceDetected
        | GeometryUnavailableReason::NoLeftBoundaryFace
        | GeometryUnavailableReason::NoRightBoundaryFace => MouthUnavailableReason::NoFaceDetected,
        _ => MouthUnavailableReason::LostTrack,
    }
}

fn push_geometry_reason(
    reasons: &mut Vec<GeometryUnavailableReason>,
    reason: GeometryUnavailableReason,
) {
    if !reasons.contains(&reason) {
        reasons.push(reason);
    }
}

fn push_mouth_reason(reasons: &mut Vec<MouthUnavailableReason>, reason: MouthUnavailableReason) {
    if !reasons.contains(&reason) {
        reasons.push(reason);
    }
}

fn update_max(current: &mut Option<f64>, value: f64) {
    *current = Some(current.map_or(value, |old| old.max(value)));
}

fn lerp(a: f64, b: f64, fraction: f64) -> f64 {
    a + (b - a) * fraction
}

#[cfg(test)]
#[path = "generated_geometry/tests.rs"]
mod tests;
