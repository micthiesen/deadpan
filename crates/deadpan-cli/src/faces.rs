//! Host side of face proposals.
//!
//! The host resolves one indexed picture of the Original (the picture
//! displayed at a source PTS), copies the verified retained bytes into a fresh
//! attempt workspace and runs the trusted `deadpan-track` executable in its
//! `detect-faces` mode through the shared process supervisor. It admits the
//! reported faces only after clean teardown and a strict check: the exact
//! picture, at most [`MAX_FACES`] valid rectangles, confidences in `[0, 1]`
//! and a strict left-to-right, then top-to-bottom order. Faces are proposals:
//! detection never edits a project. [`face_target`] turns one into an
//! ordinary static attention target that a caller may save explicitly.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use deadpan_analysis::target_region;
use deadpan_core::{AssetId, AttentionTarget, SourceSpan, SourceTimeBase, SourceTimestamp};
use deadpan_jobs::faces::{self, ExpectedStream, FaceProtocol, HostMessage, WorkerMessage};
pub use deadpan_jobs::faces::{DetectedFace, MAX_FACES, RuntimeReport, validate_faces};
use deadpan_jobs::process::{ProcessEvent, ProcessLimits, ProcessSpec, SupervisedProcess};
use deadpan_jobs::{AttemptId, CancellationToken, RequestId, WorkspaceArtifact};

use crate::tracking::{TrackingError, TrackingRuntime};

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

fn protocol_error(error: impl std::fmt::Display) -> FaceError {
    FaceError::Protocol(error.to_string())
}

/// Which picture to analyse.
#[derive(Debug, Clone)]
pub struct FaceRequest {
    pub asset: Option<AssetId>,
    /// The picture displayed at this PTS (the last indexed picture at or
    /// before it) is analysed.
    pub at_pts: i64,
}

/// A verified copy of the Original and the exact picture to analyse,
/// prepared from a store that may be closed afterwards.
pub struct PreparedFaces {
    workspace: tempfile::TempDir,
    source: WorkspaceArtifact,
    stream: ExpectedStream,
    /// The analysed asset, its video's time base, and the picture's exact
    /// indexed PTS and ordinal.
    pub asset: AssetId,
    pub time_base: SourceTimeBase,
    pub pts: i64,
    pub ordinal: usize,
    /// The end of the asset's measured video span.
    pub video_end: i64,
    /// The head document the picture was resolved against.
    pub head: deadpan_core::ProjectDocument,
    /// The Original's content identity.
    pub content: String,
}

impl PreparedFaces {
    pub fn stream(&self) -> &ExpectedStream {
        &self.stream
    }
}

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

/// Resolve the picture against the qualified index and the asset's measured
/// video span, and copy the verified Original into a new attempt workspace.
pub fn prepare_faces(
    store: &deadpan_store::ProjectStore,
    request: &FaceRequest,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<PreparedFaces, FaceError> {
    let unavailable = |error: &dyn std::fmt::Display| FaceError::Unavailable(error.to_string());
    let receipt = crate::shots::shot_receipt(store, request.asset.as_ref())
        .map_err(|error| unavailable(&error))?;
    let asset = match &request.asset {
        Some(asset) => asset.clone(),
        None => match store.single_source_state().map_err(|e| unavailable(&e))? {
            Some(deadpan_store::single_source::SingleSourceState::Ready { asset, .. }) => asset,
            _ => {
                return Err(FaceError::Unavailable(
                    "project has no ready Original; choose a registered source asset".into(),
                ));
            }
        },
    };
    let document = store.snapshot().map_err(|e| unavailable(&e))?;
    let record = document
        .assets()
        .get(&asset)
        .ok_or_else(|| FaceError::Unavailable("the asset is not registered".into()))?;
    let video_span = record
        .video
        .ok_or_else(|| FaceError::Unavailable("the asset has no video span".into()))?;
    if record.source_qualification.as_ref() != Some(receipt.id()) {
        return Err(FaceError::Unavailable(
            "the asset's qualification receipt differs from the project head".into(),
        ));
    }
    let time_base = video_span.start().time_base;
    let video = receipt
        .snapshot()
        .video()
        .ok_or_else(|| FaceError::Unavailable("the source has no qualified picture".into()))?;
    let index = video.index().index();
    let frames = index.frames();
    let interpretation = video.interpretation();
    let after = frames.partition_point(|frame| frame.pts <= request.at_pts);
    let ordinal = after.checked_sub(1).ok_or_else(|| {
        FaceError::Request("no picture is displayed before the first indexed picture".into())
    })?;
    let pts = frames[ordinal].pts;
    if pts < video_span.start().ticks || pts >= video_span.end().ticks {
        return Err(FaceError::Request(
            "the picture lies outside the asset's measured video span".into(),
        ));
    }
    let stream = ExpectedStream {
        stream_index: video.index().stream_index(),
        width: interpretation.width,
        height: interpretation.height,
        clean_aperture: interpretation.clean_aperture,
        time_base_num: interpretation.time_base_num,
        time_base_den: interpretation.time_base_den,
        rotation_quarter_turns: interpretation.rotation_quarter_turns,
    };
    let base = index.time_base();
    if u64::from(base.numerator()) * u64::from(stream.time_base_den)
        != u64::from(base.denominator()) * u64::from(stream.time_base_num)
        || base != time_base
    {
        return Err(FaceError::Unavailable(
            "the picture index and its interpretation disagree on the time base".into(),
        ));
    }
    let content = receipt.original().content().to_string();
    let (workspace, source) = crate::tracking::copy_verified_original(
        store,
        &receipt,
        video.index().content(),
        "deadpan-faces-",
        cancelled,
        deadline,
    )?;
    Ok(PreparedFaces {
        workspace,
        source,
        stream,
        asset,
        time_base,
        pts,
        ordinal,
        video_end: video_span.end().ticks,
        head: document,
        content,
    })
}

/// Admit faces reported for `pts`: the prepared picture exactly, and a
/// bounded, valid, strictly ordered list. Every path that supplies faces,
/// including test seams, goes through this check.
pub fn admit_faces(
    prepared: &PreparedFaces,
    pts: i64,
    faces: Vec<DetectedFace>,
) -> Result<Vec<DetectedFace>, FaceError> {
    if pts != prepared.pts {
        return Err(FaceError::Protocol(
            "faces were reported for a different picture".into(),
        ));
    }
    validate_faces(&faces).map_err(FaceError::Protocol)?;
    Ok(faces)
}

/// Run one supervised detection attempt on the prepared picture.
pub fn detect_faces(
    runtime: &TrackingRuntime,
    prepared: &PreparedFaces,
    attempt: &str,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<FaceDetection, FaceError> {
    crate::tracking::check(cancelled, deadline)?;
    if attempt.contains('/') || attempt.starts_with('.') {
        return Err(FaceError::Request(
            "an attempt name is one plain path component".into(),
        ));
    }
    let request_id = RequestId::new(format!("faces-{attempt}")).map_err(protocol_error)?;
    let attempt_id = AttemptId::new(attempt).map_err(protocol_error)?;
    let token = CancellationToken::new(format!("cancel-{attempt}")).map_err(protocol_error)?;
    let remaining = deadline
        .saturating_duration_since(Instant::now())
        .min(Duration::from_secs(60 * 60));
    let timeout_millis = u64::try_from(remaining.as_millis()).unwrap_or(u64::MAX);
    if timeout_millis == 0 {
        return Err(FaceError::Deadline);
    }
    let request = HostMessage::DetectFaces {
        protocol: faces::VERSION,
        request: request_id,
        attempt: attempt_id,
        cancellation_token: token,
        source: prepared.source.clone(),
        stream: Box::new(prepared.stream),
        pts: prepared.pts,
        timeout_millis,
    };
    let mut process = SupervisedProcess::<FaceProtocol>::spawn(
        ProcessSpec {
            executable: runtime.executable.clone(),
            arguments: vec![faces::WORKER_ARGUMENT.into()],
            environment: runtime.environment.clone(),
            workspace: prepared.workspace.path().to_path_buf(),
            limits: ProcessLimits {
                maximum_duration: remaining,
                cancellation_grace: Duration::from_secs(2).min(remaining),
                exit_grace: Duration::from_secs(5).min(remaining),
            },
        },
        request,
    )?;
    let mut completion = None;
    let mut failure = None;
    let mut was_cancelled = false;
    while !process.is_finished() {
        let now = Instant::now();
        if cancelled.load(Ordering::Acquire) && !was_cancelled {
            was_cancelled = true;
            process.request_cancel(now)?;
        }
        if now >= deadline {
            process.request_cancel(now)?;
            // Drain to a stopped receipt (bounded) before the workspace goes.
            if let Err(cleanup) = process.finish_owned_work(Instant::now() + Duration::from_secs(5))
            {
                return Err(FaceError::Worker(format!(
                    "face detection stopped at its deadline; worker cleanup is unconfirmed: {cleanup}"
                )));
            }
            return Err(if was_cancelled {
                FaceError::Cancelled
            } else {
                FaceError::Deadline
            });
        }
        for event in process.poll(now)? {
            match event {
                ProcessEvent::Message(message) => match *message {
                    // The protocol adapter has checked identity, picture and faces.
                    WorkerMessage::Completed {
                        pts,
                        faces,
                        runtime,
                        decode_millis,
                        vision_millis,
                        elapsed_millis,
                        ..
                    } => {
                        completion = Some((
                            pts,
                            faces,
                            runtime,
                            [decode_millis, vision_millis, elapsed_millis],
                        ));
                    }
                    WorkerMessage::Failed { diagnostic, .. } => {
                        failure.get_or_insert_with(|| {
                            FaceError::Worker(diagnostic.as_str().to_owned())
                        });
                    }
                    WorkerMessage::Cancelled { .. } if was_cancelled => {}
                    WorkerMessage::Cancelled { .. } => {
                        failure.get_or_insert_with(|| {
                            FaceError::Protocol("unrequested cancellation".into())
                        });
                    }
                },
                ProcessEvent::Fault(reason) => {
                    failure.get_or_insert(FaceError::Worker(reason));
                }
                ProcessEvent::Exited {
                    status,
                    cancellation_escalated,
                } => {
                    if (!status.success() || cancellation_escalated)
                        && !was_cancelled
                        && failure.is_none()
                    {
                        failure = Some(FaceError::Worker(format!(
                            "face detection worker exited {status}"
                        )));
                    }
                }
            }
        }
        if !process.is_finished() {
            std::thread::park_timeout(Duration::from_millis(5));
        }
    }
    if let Some(error) = failure {
        return Err(error);
    }
    if was_cancelled {
        return Err(FaceError::Cancelled);
    }
    let (pts, faces, runtime, [decode, vision, elapsed]) =
        completion.ok_or_else(|| FaceError::Protocol("no clean completion".into()))?;
    let faces = admit_faces(prepared, pts, faces)?;
    Ok(FaceDetection {
        pts,
        faces,
        runtime,
        decode_elapsed: Duration::from_millis(decode),
        vision_elapsed: Duration::from_millis(vision),
        worker_elapsed: Duration::from_millis(elapsed),
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
/// with no samples, corrections or provenance. Saving it is the caller's
/// explicit, reversible `SetTarget` edit.
pub fn face_target(
    face: &DetectedFace,
    label: String,
    asset: AssetId,
    time_base: SourceTimeBase,
    pts: i64,
    end: i64,
) -> Result<AttentionTarget, FaceError> {
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: pts,
            time_base,
        },
        SourceTimestamp {
            ticks: end,
            time_base,
        },
    )
    .map_err(|error| FaceError::Request(error.to_string()))?;
    Ok(AttentionTarget {
        label,
        asset,
        span,
        region: target_region(&face.region),
        samples: Vec::new(),
        corrections: Vec::new(),
        provenance: None,
    })
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
        "protocol": faces::VERSION,
        "content": prepared.content,
        "asset": prepared.asset.as_str(),
        "revision": prepared.head.revision_id(),
        "stream": {
            "stream_index": prepared.stream.stream_index,
            "time_base": [prepared.stream.time_base_num, prepared.stream.time_base_den],
            "rotation_quarter_turns": prepared.stream.rotation_quarter_turns,
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
            40,
            1_000,
        )
        .unwrap();
        assert_eq!(target.label, "Face 2");
        assert_eq!(target.span.start().ticks, 40);
        assert_eq!(target.span.end().ticks, 1_000);
        assert_eq!(target.region.center, [550_000, 300_000]);
        assert_eq!(target.region.size, [100_000, 200_000]);
        assert!(target.samples.is_empty() && target.corrections.is_empty());
        assert!(target.provenance.is_none());
        assert!(
            face_target(
                &face(0.5),
                "x".into(),
                AssetId::new("clip").unwrap(),
                time_base,
                40,
                40
            )
            .is_err()
        );
    }
}
