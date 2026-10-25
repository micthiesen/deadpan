//! Face-only compatibility facade over the shared exact-picture proposal host.
//! Public face numbering, reports, explicit acceptance and error codes remain.

use crate::picture_proposals::{self, PictureProposalRequest, ProposalError, ProposalSelection};
use crate::tracking::{TrackingError, TrackingRuntime};
use deadpan_core::{AssetId, AttentionTarget, SourceTimeBase};
pub use deadpan_jobs::picture_proposals::{DetectedFace, MAX_FACES, validate_faces};
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

/// Preserve the face report's existing runtime shape.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeReport {
    pub engine: String,
    pub request_revision: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum FaceError {
    #[error("face detection is unavailable: {0}")]
    Unavailable(String),
    #[error("face detection request is invalid: {0}")]
    Request(String),
    #[error("face detection cancelled")]
    Cancelled,
    #[error("face detection deadline elapsed")]
    Deadline,
    #[error("face detection worker: {0}")]
    Worker(String),
    #[error("face detection protocol: {0}")]
    Protocol(String),
    #[error(transparent)]
    Supervisor(#[from] deadpan_jobs::process::SupervisorError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl From<TrackingError> for FaceError {
    fn from(error: TrackingError) -> Self {
        match error {
            TrackingError::Unavailable(reason) => Self::Unavailable(reason),
            TrackingError::Request(reason) => Self::Request(reason),
            TrackingError::Cancelled => Self::Cancelled,
            TrackingError::Deadline => Self::Deadline,
            TrackingError::Worker(reason) => Self::Worker(reason),
            TrackingError::Supervisor(error) => Self::Supervisor(error),
            TrackingError::Io(error) => Self::Io(error),
            error => Self::Protocol(error.to_string()),
        }
    }
}

impl From<ProposalError> for FaceError {
    fn from(error: ProposalError) -> Self {
        match error {
            ProposalError::Unavailable(reason) => Self::Unavailable(reason),
            ProposalError::Request(reason) => Self::Request(reason),
            ProposalError::Cancelled => Self::Cancelled,
            ProposalError::Deadline => Self::Deadline,
            ProposalError::Worker(reason) => Self::Worker(reason),
            ProposalError::Protocol(reason) => Self::Protocol(reason),
            ProposalError::Supervisor(error) => Self::Supervisor(error),
            ProposalError::Io(error) => Self::Io(error),
        }
    }
}

/// Which picture to analyse.
#[derive(Debug, Clone)]
pub struct FaceRequest {
    pub asset: Option<AssetId>,
    /// The picture displayed at this PTS (the last indexed picture at or
    /// before it) is analysed.
    pub at_pts: i64,
}

pub type PreparedFaces = picture_proposals::PreparedPictureProposals;

/// Faces found in one picture and what the worker reported about it.
#[derive(Debug, Clone)]
pub struct FaceDetection {
    pub pts: i64,
    /// Ordered left to right, then top to bottom; face `n` is `faces[n - 1]`.
    pub faces: Vec<DetectedFace>,
    pub runtime: RuntimeReport,
    pub decode_elapsed: Duration,
    pub vision_elapsed: Duration,
    pub worker_elapsed: Duration,
}

pub fn prepare_faces(
    store: &deadpan_store::ProjectStore,
    request: &FaceRequest,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<PreparedFaces, FaceError> {
    picture_proposals::prepare_picture_proposals(
        store,
        &PictureProposalRequest {
            asset: request.asset.clone(),
            at_pts: request.at_pts,
            selection: ProposalSelection::Faces,
        },
        cancelled,
        deadline,
    )
    .map_err(Into::into)
}

pub fn admit_faces(
    prepared: &PreparedFaces,
    pts: i64,
    faces: Vec<DetectedFace>,
) -> Result<Vec<DetectedFace>, FaceError> {
    if prepared.selection() != ProposalSelection::Faces || pts != prepared.pts {
        return Err(FaceError::Protocol(
            "faces were reported for a different picture or detector selection".into(),
        ));
    }
    validate_faces(&faces).map_err(FaceError::Protocol)?;
    Ok(faces)
}

pub fn detect_faces(
    runtime: &TrackingRuntime,
    prepared: &PreparedFaces,
    attempt: &str,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<FaceDetection, FaceError> {
    if prepared.selection() != ProposalSelection::Faces {
        return Err(FaceError::Request(
            "face detection needs a face-only prepared request".into(),
        ));
    }
    let found = picture_proposals::detect_picture_proposals(
        runtime, prepared, attempt, cancelled, deadline,
    )?;
    let faces = found
        .proposals
        .faces
        .expect("admitted face-only collection");
    Ok(FaceDetection {
        pts: found.pts,
        faces: faces.faces,
        runtime: RuntimeReport {
            engine: faces.runtime.engine,
            request_revision: faces.runtime.request_revision,
        },
        decode_elapsed: found.decode_elapsed,
        vision_elapsed: found.vision_elapsed,
        worker_elapsed: found.worker_elapsed,
    })
}

/// Face `number` (1-based, left to right) of `faces`, or a refusal that says
/// how many there are.
pub fn choose_face(faces: &[DetectedFace], number: usize) -> Result<&DetectedFace, String> {
    if faces.is_empty() {
        return Err("No faces were found in this picture.".into());
    }
    if number == 0 {
        return Err("Faces are numbered from 1, left to right.".into());
    }
    faces.get(number - 1).ok_or_else(|| {
        format!(
            "face:{number} is out of range: this picture has {} face{} (face:1{}), numbered left to right.",
            faces.len(),
            if faces.len() == 1 { "" } else { "s" },
            if faces.len() == 1 {
                String::new()
            } else {
                format!("–face:{}", faces.len())
            }
        )
    })
}

/// A detected face as an ordinary, untracked attention target over
/// `[pts, end)` of `asset`: its rectangle is the target's initial region,
/// with no samples, corrections or provenance, and `display_aspect` is the
/// analysed picture's exact upright aspect. Saving it is the caller's
/// explicit, reversible `SetTarget` edit.
pub fn face_target(
    face: &DetectedFace,
    label: String,
    asset: AssetId,
    time_base: SourceTimeBase,
    display_aspect: Option<deadpan_core::ExactRatio>,
    pts: i64,
    end: i64,
) -> Result<AttentionTarget, FaceError> {
    crate::picture_proposals::region_target(
        &face.region,
        label,
        asset,
        time_base,
        display_aspect,
        pts,
        end,
    )
    .map_err(Into::into)
}

/// Prepare and run one detection read-only and describe it as JSON.
pub fn detect_project(
    runtime: &TrackingRuntime,
    project: &Path,
    request: &FaceRequest,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<serde_json::Value, FaceError> {
    // Read-only throughout: detection never writes the project.
    let store = deadpan_store::ProjectStore::open(project, deadpan_store::AccessMode::ReadOnly)
        .map_err(|error| FaceError::Unavailable(error.to_string()))?;
    let prepared = prepare_faces(&store, request, cancelled, deadline)?;
    drop(store);
    let attempt = uuid::Uuid::new_v4().simple().to_string();
    let started = Instant::now();
    let detection = detect_faces(runtime, &prepared, &attempt, cancelled, deadline)?;
    let millis = crate::tracking::millis;
    Ok(serde_json::json!({
        "protocol": deadpan_jobs::picture_proposals::VERSION,
        "content": prepared.content,
        "asset": prepared.asset.as_str(),
        "revision": prepared.head.revision_id(),
        "stream": {
            "stream_index": prepared.stream().stream_index,
            "time_base": [prepared.stream().time_base_num, prepared.stream().time_base_den],
            "rotation_quarter_turns": prepared.stream().rotation_quarter_turns,
        },
        "pts": detection.pts,
        "picture": prepared.ordinal,
        "runtime": detection.runtime,
        "elapsed_ms": millis(started.elapsed()),
        "worker_elapsed_ms": millis(detection.worker_elapsed),
        "decode_elapsed_ms": millis(detection.decode_elapsed),
        "vision_elapsed_ms": millis(detection.vision_elapsed),
        "faces": detection
            .faces
            .iter()
            .enumerate()
            .map(|(index, face)| serde_json::json!({
                "face": index + 1,
                "region": face.region,
                "confidence": face.confidence,
            }))
            .collect::<Vec<_>>(),
    }))
}

const USAGE: &str = "usage: detect-faces <project.deadpan> --at <pts> [--asset <id>]";

/// `detect-faces PROJECT --at PTS [--asset ID]`: list the faces Vision finds
/// in the picture displayed at `PTS`, numbered left to right. Never edits.
pub fn run_detect_faces(arguments: &[&str]) -> Result<(), crate::CliError> {
    let usage = || crate::CliError::Usage(USAGE.into());
    let [path, rest @ ..] = arguments else {
        return Err(usage());
    };
    let found = crate::tracking::options(rest, &[], &usage)?;
    if found.keys().any(|key| !["--at", "--asset"].contains(key)) {
        return Err(usage());
    }
    let request = FaceRequest {
        asset: found
            .get("--asset")
            .map(|asset| AssetId::new(*asset))
            .transpose()
            .map_err(|e| crate::CliError::Usage(e.to_string()))?,
        at_pts: found
            .get("--at")
            .ok_or_else(usage)?
            .parse::<i64>()
            .map_err(|_| usage())?,
    };
    let cancelled = crate::tracking::cancellation()?;
    let deadline = Instant::now() + Duration::from_secs(10 * 60);
    let runtime = TrackingRuntime::beside_current_executable()?;
    let report = detect_project(&runtime, Path::new(path), &request, &cancelled, deadline)?;
    crate::write_json(&report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadpan_analysis::NormalizedRect;

    fn face(x: f64) -> DetectedFace {
        DetectedFace {
            region: NormalizedRect::new(x, 0.2, 0.1, 0.2).unwrap(),
            confidence: 0.8,
        }
    }

    #[test]
    fn faces_are_chosen_by_one_based_number_with_clear_refusals() {
        let faces = [face(0.1), face(0.5)];
        assert_eq!(choose_face(&faces, 2).unwrap(), &faces[1]);
        assert_eq!(
            choose_face(&faces, 3).unwrap_err(),
            "face:3 is out of range: this picture has 2 faces (face:1–face:2), numbered left to right."
        );
        assert_eq!(
            choose_face(&faces[..1], 2).unwrap_err(),
            "face:2 is out of range: this picture has 1 face (face:1), numbered left to right."
        );
        assert_eq!(
            choose_face(&[], 1).unwrap_err(),
            "No faces were found in this picture."
        );
        assert!(choose_face(&faces, 0).is_err());
    }

    #[test]
    fn a_face_target_is_a_static_untracked_region_from_its_picture() {
        let time_base = SourceTimeBase::new(1, 1_000).unwrap();
        let target = face_target(
            &face(0.5),
            "Face 2".into(),
            AssetId::new("clip").unwrap(),
            time_base,
            Some(deadpan_core::ExactRatio::new(9, 16).unwrap()),
            40,
            1_000,
        )
        .unwrap();
        assert_eq!(
            target.display_aspect,
            Some(deadpan_core::ExactRatio::new(9, 16).unwrap())
        );
        assert_eq!(target.label, "Face 2");
        assert_eq!(target.span.start().ticks, 40);
        assert_eq!(target.span.end().ticks, 1_000);
        assert_eq!(target.region.center(), [550_000, 300_000]);
        assert_eq!(target.region.as_region().unwrap().size, [100_000, 200_000]);
        assert!(target.samples.is_empty() && target.corrections.is_empty());
        assert!(target.provenance.is_none());
        assert!(
            face_target(
                &face(0.5),
                "x".into(),
                AssetId::new("clip").unwrap(),
                time_base,
                None,
                40,
                40
            )
            .is_err()
        );
    }
}
