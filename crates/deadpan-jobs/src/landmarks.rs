//! Bounded batch landmark inspection in the `deadpan-track` worker.
//!
//! The host supplies private, hashed media and every expected native PTS. The
//! worker decodes the complete sequence and optional retained RGB8 boundary
//! PNGs. Its JSON artifact contains observations, not a quality verdict. The
//! host snapshots that artifact after clean teardown and validates it against
//! its captured inputs before applying policy.

use std::io::{Read, Write};

use deadpan_analysis::NormalizedRect;
use deadpan_analysis::generated_extension::ExtensionCoverage;
use deadpan_analysis::generated_geometry::RawLandmarkBatch;
use deadpan_analysis::generated_geometry::extension::RawExtensionLandmarkBatch;
use deadpan_analysis::generated_region::extension::RawExtensionRegionBatch;
use deadpan_analysis::generated_region::{RawRegionBatch, RegionSeeds};
use serde::{Deserialize, Serialize};

use crate::process::{ResponseKind, SupervisorError, WorkerProtocol};
use crate::protocol::{
    AttemptId, CancellationToken, Diagnostic, RequestId, WorkspaceArtifact, WorkspaceRef,
    read_frame, write_frame,
};
pub use crate::tracking::ExpectedStream;

pub const VERSION: u32 = 3;
pub const OBSERVATIONS_SCHEMA_VERSION: u32 = 2;
pub const EXTENSION_OBSERVATIONS_SCHEMA_VERSION: u32 = 1;
pub const WORKER_ARGUMENT: &str = "inspect-landmarks";
pub const OUTPUT_FILE: &str = "landmarks.json";
pub const MAX_FRAMES: usize = 1_025;
pub const MAX_EXTENSION_NATIVE_FRAMES: usize = 4_096;
pub const MAX_DIMENSION: u32 = 4_096;
pub const MAX_PIXELS: u64 = 4_096 * 4_096;
pub const MAX_SOURCE_BYTES: u64 = 16 * 1024 * 1024 * 1024;
pub const MAX_PNG_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_OBSERVATION_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_TIMEOUT_MILLIS: u64 = 60 * 60 * 1_000;
pub const ENGINE: &str = "Apple Vision VNDetectFaceLandmarksRequest";
pub const REQUEST_REVISION: u64 = 3;
/// Number of points in the pinned Vision constellation, not its ObjC enum value.
pub const CONSTELLATION: u32 = 76;
pub const REGION_ENGINE: &str = "Apple Vision VNTrackObjectRequest";
pub const REGION_REQUEST_REVISION: u64 = 2;
/// Actual property reported by revision 2 on the qualified runtime. Apple's
/// revision 2 ignores the level distinction and reads back Fast even after
/// Accurate is assigned; the host records that measured value truthfully.
pub const REGION_TRACKING_LEVEL: &str = "fast";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InspectionObservations {
    pub schema_version: u32,
    pub landmarks: RawLandmarkBatch,
    pub region: Option<RawRegionBatch>,
}

impl InspectionObservations {
    pub fn validate(
        &self,
        expected_pts: &[i64],
        requested_seeds: Option<&RegionSeeds>,
    ) -> Result<(), String> {
        if self.schema_version != OBSERVATIONS_SCHEMA_VERSION {
            return Err("unsupported inspection observations schema".into());
        }
        self.landmarks
            .validate(expected_pts)
            .map_err(|error| error.to_string())?;
        match (&self.region, requested_seeds) {
            (Some(region), Some(seeds)) if &region.seeds == seeds => {
                if self.landmarks.boundaries.is_none() {
                    return Err("region observations require retained boundary observations".into());
                }
                region
                    .validate(expected_pts)
                    .map_err(|error| error.to_string())
            }
            (None, None) => Ok(()),
            _ => Err("region observations differ from the captured seeds".into()),
        }
    }
}

/// Extension observations retain one actual conditioning anchor and only the
/// generated interval. An opposite seam is never supplied to this worker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InspectionExtensionObservations {
    pub schema_version: u32,
    pub landmarks: RawExtensionLandmarkBatch,
    #[serde(deserialize_with = "required_option")]
    pub region: Option<RawExtensionRegionBatch>,
}

impl InspectionExtensionObservations {
    pub fn validate(
        &self,
        expected_pts: &[i64],
        coverage: &ExtensionCoverage,
        requested_seed: Option<&NormalizedRect>,
    ) -> Result<(), String> {
        coverage
            .validate(expected_pts)
            .map_err(|error| error.to_string())?;
        if self.schema_version != EXTENSION_OBSERVATIONS_SCHEMA_VERSION
            || &self.landmarks.coverage != coverage
        {
            return Err("extension observations differ from the captured coverage".into());
        }
        self.landmarks
            .validate(expected_pts)
            .map_err(|error| error.to_string())?;
        match (&self.region, requested_seed) {
            (Some(region), Some(seed)) if &region.seed == seed && &region.coverage == coverage => {
                region
                    .validate(expected_pts)
                    .map_err(|error| error.to_string())
            }
            (None, None) => Ok(()),
            _ => Err(
                "extension region observations differ from the captured seed or coverage".into(),
            ),
        }
    }
}

fn required_option<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    Option::<T>::deserialize(deserializer)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoundaryInputs {
    pub left: WorkspaceArtifact,
    pub right: WorkspaceArtifact,
}

/// The exact detector configuration, with no implicit runtime fallback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeReport {
    pub engine: String,
    pub request_revision: u64,
    pub constellation: u32,
}

impl RuntimeReport {
    pub fn validate(&self) -> Result<(), String> {
        if self.engine != ENGINE
            || self.request_revision != REQUEST_REVISION
            || self.constellation != CONSTELLATION
        {
            return Err("landmark detector differs from its pinned configuration".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegionRuntimeReport {
    pub engine: String,
    pub request_revision: u64,
    pub tracking_level: String,
}

impl RegionRuntimeReport {
    pub fn validate(&self) -> Result<(), String> {
        if self.engine != REGION_ENGINE
            || self.request_revision != REGION_REQUEST_REVISION
            || self.tracking_level != REGION_TRACKING_LEVEL
        {
            return Err("region tracker differs from its pinned configuration".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum HostMessage {
    InspectLandmarks {
        protocol: u32,
        request: RequestId,
        attempt: AttemptId,
        cancellation_token: CancellationToken,
        source: WorkspaceArtifact,
        stream: ExpectedStream,
        /// Every picture of the complete source, in presentation order.
        picture_pts: Vec<i64>,
        /// Unmodified retained PNG bytes, each with the native raster size.
        boundaries: Option<Box<BoundaryInputs>>,
        /// Explicit authored subject boxes in the retained boundary PNGs.
        region_seeds: Option<Box<RegionSeeds>>,
        output_scope: WorkspaceRef,
        maximum_output_bytes: u64,
        timeout_millis: u64,
    },
    InspectExtensionLandmarks {
        protocol: u32,
        request: RequestId,
        attempt: AttemptId,
        cancellation_token: CancellationToken,
        source: WorkspaceArtifact,
        stream: ExpectedStream,
        /// Complete canonical source index, including unanalysed context.
        picture_pts: Vec<i64>,
        anchor: WorkspaceArtifact,
        coverage: ExtensionCoverage,
        #[serde(deserialize_with = "required_option")]
        region_seed: Option<NormalizedRect>,
        output_scope: WorkspaceRef,
        maximum_output_bytes: u64,
        timeout_millis: u64,
    },
    Cancel {
        protocol: u32,
        request: RequestId,
        attempt: AttemptId,
        cancellation_token: CancellationToken,
    },
}

fn validate_input(artifact: &WorkspaceArtifact, maximum: u64) -> Result<(), String> {
    let reference = artifact.reference().as_str();
    if !reference.starts_with("input/") || reference[6..].contains('/') {
        return Err("landmark inputs must be directly below input/".into());
    }
    if artifact.byte_length() == 0 || artifact.byte_length() > maximum {
        return Err("landmark input exceeds its byte bound".into());
    }
    Ok(())
}

impl HostMessage {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::InspectLandmarks {
                protocol,
                source,
                stream,
                picture_pts,
                boundaries,
                region_seeds,
                output_scope,
                maximum_output_bytes,
                timeout_millis,
                ..
            } => {
                version(*protocol)?;
                validate_input(source, MAX_SOURCE_BYTES)?;
                stream.validate()?;
                if stream.width > MAX_DIMENSION
                    || stream.height > MAX_DIMENSION
                    || u64::from(stream.width) * u64::from(stream.height) > MAX_PIXELS
                {
                    return Err("landmark raster exceeds its pixel bound".into());
                }
                if !(2..=MAX_FRAMES).contains(&picture_pts.len())
                    || picture_pts.windows(2).any(|pair| pair[0] >= pair[1])
                {
                    return Err("landmark pictures must be bounded and strictly ordered".into());
                }
                if let Some(boundaries) = boundaries {
                    validate_input(&boundaries.left, MAX_PNG_BYTES)?;
                    validate_input(&boundaries.right, MAX_PNG_BYTES)?;
                    if boundaries.left.reference() == source.reference()
                        || boundaries.right.reference() == source.reference()
                    {
                        return Err("landmark PNG inputs alias the video source".into());
                    }
                    if boundaries.left.reference() == boundaries.right.reference()
                        && boundaries.left != boundaries.right
                    {
                        return Err("aliased landmark PNG inputs disagree".into());
                    }
                }
                if let Some(seeds) = region_seeds {
                    seeds.validate().map_err(|error| error.to_string())?;
                    if boundaries.is_none() || stream.rotation_quarter_turns != 0 {
                        return Err("region tracking requires retained boundaries and an unrotated canonical raster".into());
                    }
                }
                // Restrict the output to one sibling directory. A nested path
                // would require walking every component without following links.
                if output_scope.as_str() == "input" || output_scope.as_str().contains('/') {
                    return Err("landmark output scope must be one directory beside input".into());
                }
                if !(1..=MAX_OBSERVATION_BYTES).contains(maximum_output_bytes) {
                    return Err("landmark output exceeds its byte bound".into());
                }
                if !(1..=MAX_TIMEOUT_MILLIS).contains(timeout_millis) {
                    return Err("landmark timeout exceeds its bound".into());
                }
                Ok(())
            }
            Self::InspectExtensionLandmarks {
                protocol,
                source,
                stream,
                picture_pts,
                anchor,
                coverage,
                region_seed,
                output_scope,
                maximum_output_bytes,
                timeout_millis,
                ..
            } => {
                version(*protocol)?;
                validate_input(source, MAX_SOURCE_BYTES)?;
                validate_input(anchor, MAX_PNG_BYTES)?;
                stream.validate()?;
                if stream.width > MAX_DIMENSION
                    || stream.height > MAX_DIMENSION
                    || u64::from(stream.width) * u64::from(stream.height) > MAX_PIXELS
                    || stream.rotation_quarter_turns != 0
                    || stream.time_base_num != 1
                    || stream.time_base_den != 1000
                {
                    return Err(
                        "extension landmark source requires a bounded canonical raster and clock"
                            .into(),
                    );
                }
                coverage
                    .validate(picture_pts)
                    .map_err(|error| error.to_string())?;
                if picture_pts.len() > MAX_EXTENSION_NATIVE_FRAMES
                    || anchor.reference() == source.reference()
                {
                    return Err("extension input count or anchor alias is invalid".into());
                }
                if let Some(seed) = region_seed {
                    NormalizedRect::new(seed.x(), seed.y(), seed.width(), seed.height())
                        .map_err(|error| error.to_string())?;
                }
                if output_scope.as_str() == "input" || output_scope.as_str().contains('/') {
                    return Err("landmark output scope must be one directory beside input".into());
                }
                if !(1..=MAX_OBSERVATION_BYTES).contains(maximum_output_bytes)
                    || !(1..=MAX_TIMEOUT_MILLIS).contains(timeout_millis)
                {
                    return Err("extension output or deadline exceeds its bound".into());
                }
                Ok(())
            }
            Self::Cancel { protocol, .. } => version(*protocol),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkerMessage {
    Progress {
        protocol: u32,
        request: RequestId,
        attempt: AttemptId,
        percent: u8,
    },
    Completed {
        protocol: u32,
        request: RequestId,
        attempt: AttemptId,
        observations: WorkspaceArtifact,
        runtime: RuntimeReport,
        region_runtime: Option<RegionRuntimeReport>,
        /// Native pictures decoded, excluding boundary PNGs.
        decoded: u32,
        /// Bridge: native pictures plus optional two PNGs. Extension: generated
        /// pictures plus its one retained anchor, excluding native context.
        analysed: u32,
        decode_millis: u64,
        vision_millis: u64,
        elapsed_millis: u64,
    },
    Failed {
        protocol: u32,
        request: RequestId,
        attempt: AttemptId,
        diagnostic: Diagnostic,
    },
    Cancelled {
        protocol: u32,
        request: RequestId,
        attempt: AttemptId,
    },
}

impl WorkerMessage {
    fn identity(&self) -> (&RequestId, &AttemptId) {
        match self {
            Self::Progress {
                request, attempt, ..
            }
            | Self::Completed {
                request, attempt, ..
            }
            | Self::Failed {
                request, attempt, ..
            }
            | Self::Cancelled {
                request, attempt, ..
            } => (request, attempt),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        let protocol = match self {
            Self::Progress {
                protocol, percent, ..
            } => {
                if *percent > 100 {
                    return Err("landmark progress exceeds 100 percent".into());
                }
                *protocol
            }
            Self::Completed {
                protocol,
                observations,
                runtime,
                region_runtime,
                decoded,
                analysed,
                decode_millis,
                vision_millis,
                elapsed_millis,
                ..
            } => {
                runtime.validate()?;
                if let Some(runtime) = region_runtime {
                    runtime.validate()?;
                }
                if !(2..=MAX_EXTENSION_NATIVE_FRAMES as u32).contains(decoded)
                    || !(2..=MAX_FRAMES as u32 + 2).contains(analysed)
                    || observations.byte_length() == 0
                    || observations.byte_length() > MAX_OBSERVATION_BYTES
                    || *decode_millis > *elapsed_millis
                    || *vision_millis > *elapsed_millis
                {
                    return Err("landmark completion counts, size or timing are invalid".into());
                }
                *protocol
            }
            Self::Failed { protocol, .. } | Self::Cancelled { protocol, .. } => *protocol,
        };
        version(protocol)
    }
}

fn version(value: u32) -> Result<(), String> {
    if value == VERSION {
        Ok(())
    } else {
        Err("unsupported landmark protocol".into())
    }
}

/// Validates control messages; the host separately validates the contained JSON.
pub struct LandmarkProtocol {
    request: RequestId,
    attempt: AttemptId,
    token: CancellationToken,
    output_reference: String,
    maximum_output_bytes: u64,
    decoded: u32,
    analysed: u32,
    region_requested: bool,
}

impl WorkerProtocol for LandmarkProtocol {
    const WORKER_CLASS: deadpan_diagnostics::WorkerClass = deadpan_diagnostics::WorkerClass::Model;
    type Request = HostMessage;
    type Response = WorkerMessage;

    fn from_request(message: &HostMessage) -> Result<Self, SupervisorError> {
        message.validate().map_err(SupervisorError::Request)?;
        if let HostMessage::InspectExtensionLandmarks {
            request,
            attempt,
            cancellation_token,
            output_scope,
            maximum_output_bytes,
            picture_pts,
            coverage,
            region_seed,
            ..
        } = message
        {
            return Ok(Self {
                request: request.clone(),
                attempt: attempt.clone(),
                token: cancellation_token.clone(),
                output_reference: format!("{}/{OUTPUT_FILE}", output_scope.as_str()),
                maximum_output_bytes: *maximum_output_bytes,
                decoded: picture_pts.len() as u32,
                analysed: coverage.end - coverage.start + 1,
                region_requested: region_seed.is_some(),
            });
        }
        let HostMessage::InspectLandmarks {
            request,
            attempt,
            cancellation_token,
            output_scope,
            maximum_output_bytes,
            picture_pts,
            boundaries,
            region_seeds,
            ..
        } = message
        else {
            return Err(SupervisorError::Request(
                "initial landmark message must inspect".into(),
            ));
        };
        let decoded = picture_pts.len() as u32;
        Ok(Self {
            request: request.clone(),
            attempt: attempt.clone(),
            token: cancellation_token.clone(),
            output_reference: format!("{}/{OUTPUT_FILE}", output_scope.as_str()),
            maximum_output_bytes: *maximum_output_bytes,
            decoded,
            analysed: decoded + if boundaries.is_some() { 2 } else { 0 },
            region_requested: region_seeds.is_some(),
        })
    }

    fn cancellation(&self) -> HostMessage {
        HostMessage::Cancel {
            protocol: VERSION,
            request: self.request.clone(),
            attempt: self.attempt.clone(),
            cancellation_token: self.token.clone(),
        }
    }

    fn write_request(writer: &mut impl Write, request: &HostMessage) -> Result<(), String> {
        request.validate()?;
        write_frame(writer, request).map_err(|error| error.to_string())
    }

    fn read_response(reader: &mut impl Read) -> Result<Option<WorkerMessage>, String> {
        let value: Option<WorkerMessage> = read_frame(reader).map_err(|error| error.to_string())?;
        if let Some(value) = &value {
            value.validate()?;
        }
        Ok(value)
    }

    fn classify(&self, response: &WorkerMessage) -> Result<ResponseKind, String> {
        response.validate()?;
        if response.identity() != (&self.request, &self.attempt) {
            return Err("landmark response belongs to another attempt".into());
        }
        match response {
            WorkerMessage::Progress { .. } => Ok(ResponseKind::Progress),
            WorkerMessage::Completed {
                observations,
                decoded,
                analysed,
                region_runtime,
                ..
            } => {
                if observations.reference().as_str() != self.output_reference
                    || observations.byte_length() > self.maximum_output_bytes
                    || *decoded != self.decoded
                    || *analysed != self.analysed
                    || region_runtime.is_some() != self.region_requested
                {
                    return Err("landmark completion differs from the captured request".into());
                }
                Ok(ResponseKind::Completed)
            }
            WorkerMessage::Failed { .. } => Ok(ResponseKind::Failed),
            WorkerMessage::Cancelled { .. } => Ok(ResponseKind::Terminal),
        }
    }
}

pub fn read_host(reader: &mut impl Read) -> Result<Option<HostMessage>, String> {
    let value: Option<HostMessage> = read_frame(reader).map_err(|error| error.to_string())?;
    if let Some(value) = &value {
        value.validate()?;
    }
    Ok(value)
}

pub fn write_worker(writer: &mut impl Write, value: &WorkerMessage) -> Result<(), String> {
    value.validate()?;
    write_frame(writer, value).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests;
