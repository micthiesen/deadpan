//! One bounded exact-picture proposal request for faces, salient regions or both.
//! Detection never edits. Both shares one prepared input and one worker decode;
//! each successful detector reports its own actual runtime and ordered geometry.

use std::io::{Read, Write};

use deadpan_analysis::NormalizedRect;
use serde::{Deserialize, Serialize};

use crate::process::{ResponseKind, SupervisorError, WorkerProtocol};
use crate::protocol::{
    AttemptId, CancellationToken, Diagnostic, RequestId, WorkspaceArtifact, read_frame, write_frame,
};
pub use crate::tracking::ExpectedStream;

pub const VERSION: u32 = 2;
/// The worker's command-line argument selecting this protocol.
pub const WORKER_ARGUMENT: &str = "picture-proposals";
/// The most faces one picture may report. More are refused, not truncated.
pub const MAX_FACES: usize = 64;
pub const MAX_SALIENT_REGIONS: usize = 64;
/// The concrete face detector identified by this protocol.
pub const FACE_ENGINE: &str = "Apple Vision VNDetectFaceRectanglesRequest";
/// The concrete objectness-saliency detector identified by this protocol.
pub const SALIENCY_ENGINE: &str = "Apple Vision VNGenerateObjectnessBasedSaliencyImageRequest";
/// This protocol deliberately uses objectness saliency revision 2.
pub const SALIENCY_REQUEST_REVISION: u64 = 2;
const MAX_TIMEOUT_MILLIS: u64 = 60 * 60 * 1_000;

/// One detected face in the displayed picture (top-left origin), with
/// Vision's confidence in `[0, 1]`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DetectedFace {
    pub region: NormalizedRect,
    pub confidence: f32,
}

impl DetectedFace {
    /// Left edge first, then top edge, then size: a total order on valid
    /// regions, so equal inputs always number faces the same way.
    pub fn order(&self, other: &Self) -> std::cmp::Ordering {
        let key = |face: &Self| {
            [
                face.region.x(),
                face.region.y(),
                face.region.width(),
                face.region.height(),
            ]
        };
        let (a, b) = (key(self), key(other));
        a.iter()
            .zip(b.iter())
            .map(|(a, b)| a.total_cmp(b))
            .find(|ordering| ordering.is_ne())
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| self.confidence.total_cmp(&other.confidence))
    }
}

/// Validate a reported face list: bounded, finite confidences in `[0, 1]`
/// and strictly ordered left to right (then top to bottom).
pub fn validate_faces(faces: &[DetectedFace]) -> Result<(), String> {
    if faces.len() > MAX_FACES {
        return Err(format!("more than {MAX_FACES} faces reported"));
    }
    if faces
        .iter()
        .any(|face| !face.confidence.is_finite() || !(0.0..=1.0).contains(&face.confidence))
    {
        return Err("face confidence outside [0, 1]".into());
    }
    if faces
        .windows(2)
        .any(|pair| pair[0].order(&pair[1]) != std::cmp::Ordering::Less)
    {
        return Err("faces are not ordered left to right, then top to bottom".into());
    }
    Ok(())
}

/// Which concrete detectors are requested for this one indexed picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalSelection {
    Faces,
    SalientRegions,
    Both,
}
impl ProposalSelection {
    pub const fn includes_faces(self) -> bool {
        matches!(self, Self::Faces | Self::Both)
    }
    pub const fn includes_salient_regions(self) -> bool {
        matches!(self, Self::SalientRegions | Self::Both)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectorKind {
    Faces,
    SalientRegions,
}

/// Actual performed request provenance, separately for each selected detector.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DetectorRuntime {
    pub detector: DetectorKind,
    pub engine: String,
    pub request_revision: u64,
}
impl DetectorRuntime {
    pub fn validate(&self, expected: DetectorKind) -> Result<(), String> {
        if self.detector != expected {
            return Err("runtime belongs to a different detector".into());
        }
        let engine = match expected {
            DetectorKind::Faces => FACE_ENGINE,
            DetectorKind::SalientRegions => SALIENCY_ENGINE,
        };
        if self.engine != engine {
            return Err("runtime label does not match detector".into());
        }
        if self.request_revision == 0
            || (expected == DetectorKind::SalientRegions
                && self.request_revision != SALIENCY_REQUEST_REVISION)
        {
            return Err("unsupported detector request revision".into());
        }
        Ok(())
    }
}

/// One objectness-saliency rectangle. Vision supplies no saliency confidence.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DetectedSalientRegion {
    pub region: NormalizedRect,
}
impl DetectedSalientRegion {
    pub fn order(&self, other: &Self) -> std::cmp::Ordering {
        let key = |region: &Self| {
            [
                region.region.x(),
                region.region.y(),
                region.region.width(),
                region.region.height(),
            ]
        };
        key(self)
            .iter()
            .zip(key(other).iter())
            .map(|(a, b)| a.total_cmp(b))
            .find(|order| order.is_ne())
            .unwrap_or(std::cmp::Ordering::Equal)
    }
}
pub fn validate_salient_regions(regions: &[DetectedSalientRegion]) -> Result<(), String> {
    if regions.len() > MAX_SALIENT_REGIONS {
        return Err(format!(
            "more than {MAX_SALIENT_REGIONS} salient regions reported"
        ));
    }
    if regions
        .windows(2)
        .any(|pair| pair[0].order(&pair[1]) != std::cmp::Ordering::Less)
    {
        return Err("salient regions are not ordered left to right, then top to bottom".into());
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FaceProposals {
    #[serde(deserialize_with = "bounded_faces")]
    pub faces: Vec<DetectedFace>,
    pub runtime: DetectorRuntime,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SalientRegionProposals {
    #[serde(deserialize_with = "bounded_salient_regions")]
    pub regions: Vec<DetectedSalientRegion>,
    pub runtime: DetectorRuntime,
}

/// Missing collections mean unrequested; a successful empty detector is Some(empty).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PictureProposals {
    pub faces: Option<FaceProposals>,
    pub salient_regions: Option<SalientRegionProposals>,
}
impl PictureProposals {
    pub fn validate(&self, selection: ProposalSelection) -> Result<(), String> {
        if self.faces.is_some() != selection.includes_faces()
            || self.salient_regions.is_some() != selection.includes_salient_regions()
        {
            return Err("proposal collections differ from selected detectors".into());
        }
        if let Some(faces) = &self.faces {
            faces.runtime.validate(DetectorKind::Faces)?;
            validate_faces(&faces.faces)?;
        }
        if let Some(regions) = &self.salient_regions {
            regions.runtime.validate(DetectorKind::SalientRegions)?;
            validate_salient_regions(&regions.regions)?;
        }
        Ok(())
    }
}

fn bounded_faces<'de, D: serde::Deserializer<'de>>(
    decoder: D,
) -> Result<Vec<DetectedFace>, D::Error> {
    bounded_items::<D, DetectedFace, MAX_FACES>(decoder)
}
fn bounded_salient_regions<'de, D: serde::Deserializer<'de>>(
    decoder: D,
) -> Result<Vec<DetectedSalientRegion>, D::Error> {
    bounded_items::<D, DetectedSalientRegion, MAX_SALIENT_REGIONS>(decoder)
}
fn bounded_items<'de, D, T, const N: usize>(decoder: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Items<T, const N: usize>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>, const N: usize> serde::de::Visitor<'de> for Items<T, N> {
        type Value = Vec<T>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(formatter, "at most {N} proposals")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Self::Value, A::Error> {
            if seq.size_hint().is_some_and(|size| size > N) {
                return Err(serde::de::Error::custom("proposal count exceeds its bound"));
            }
            let mut result = Vec::new();
            loop {
                if result.len() == N {
                    if seq.next_element::<serde::de::IgnoredAny>()?.is_some() {
                        return Err(serde::de::Error::custom("proposal count exceeds its bound"));
                    }
                    return Ok(result);
                }
                let Some(item) = seq.next_element()? else {
                    return Ok(result);
                };
                result.push(item);
            }
        }
    }
    decoder.deserialize_seq(Items::<T, N>(std::marker::PhantomData))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum HostMessage {
    DetectPictureProposals {
        protocol: u32,
        request: RequestId,
        attempt: AttemptId,
        cancellation_token: CancellationToken,
        /// The Original's verified bytes, directly below `input/`.
        source: WorkspaceArtifact,
        stream: Box<ExpectedStream>,
        selection: ProposalSelection,
        /// The exact PTS of the indexed picture to analyse.
        pts: i64,
        timeout_millis: u64,
    },
    Cancel {
        protocol: u32,
        request: RequestId,
        attempt: AttemptId,
        cancellation_token: CancellationToken,
    },
}

impl HostMessage {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::DetectPictureProposals {
                protocol,
                source,
                stream,
                timeout_millis,
                ..
            } => {
                version(*protocol)?;
                stream.validate()?;
                let reference = source.reference().as_str();
                if !reference.starts_with("input/") || reference[6..].contains('/') {
                    return Err("detection source must be directly below input/".into());
                }
                if source.byte_length() == 0
                    || source.byte_length() > crate::tracking::MAX_SOURCE_BYTES
                {
                    return Err("detection source size outside its bound".into());
                }
                if !(1..=MAX_TIMEOUT_MILLIS).contains(timeout_millis) {
                    return Err("detection timeout outside its bound".into());
                }
                Ok(())
            }
            Self::Cancel { protocol, .. } => version(*protocol),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkerMessage {
    Completed {
        protocol: u32,
        request: RequestId,
        attempt: AttemptId,
        /// The analysed picture's PTS, which must be the requested one.
        pts: i64,
        selection: ProposalSelection,
        proposals: PictureProposals,
        /// Time spent verifying, decoding and seeking, and in Vision.
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
            Self::Completed {
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
            Self::Completed {
                protocol,
                selection,
                proposals,
                ..
            } => {
                proposals.validate(*selection)?;
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
        Err("unsupported picture-proposal protocol".into())
    }
}

/// Host-side adapter binding every response to the captured attempt.
pub struct PictureProposalProtocol {
    request: RequestId,
    attempt: AttemptId,
    token: CancellationToken,
    pts: i64,
    selection: ProposalSelection,
}

impl WorkerProtocol for PictureProposalProtocol {
    const WORKER_CLASS: deadpan_diagnostics::WorkerClass = deadpan_diagnostics::WorkerClass::Model;

    type Request = HostMessage;
    type Response = WorkerMessage;

    fn from_request(request: &HostMessage) -> Result<Self, SupervisorError> {
        request.validate().map_err(SupervisorError::Request)?;
        match request {
            HostMessage::DetectPictureProposals {
                request,
                attempt,
                cancellation_token,
                pts,
                selection,
                ..
            } => Ok(Self {
                request: request.clone(),
                attempt: attempt.clone(),
                token: cancellation_token.clone(),
                pts: *pts,
                selection: *selection,
            }),
            HostMessage::Cancel { .. } => Err(SupervisorError::Request(
                "initial detection message must detect".into(),
            )),
        }
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
            return Err("detection response belongs to another attempt".into());
        }
        match response {
            WorkerMessage::Completed { pts, selection, .. } => {
                if *selection != self.selection {
                    return Err("worker ran a different detector selection".into());
                }
                if *pts != self.pts {
                    return Err("worker analysed a different picture".into());
                }
                Ok(ResponseKind::Completed)
            }
            WorkerMessage::Failed { .. } => Ok(ResponseKind::Failed),
            WorkerMessage::Cancelled { .. } => Ok(ResponseKind::Terminal),
        }
    }
}

/// Worker side: read the next host message, validated.
pub fn read_host(reader: &mut impl Read) -> Result<Option<HostMessage>, String> {
    let value: Option<HostMessage> = read_frame(reader).map_err(|error| error.to_string())?;
    if let Some(value) = &value {
        value.validate()?;
    }
    Ok(value)
}

/// Worker side: write one validated message.
pub fn write_worker(writer: &mut impl Write, value: &WorkerMessage) -> Result<(), String> {
    value.validate()?;
    write_frame(writer, value).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests;
