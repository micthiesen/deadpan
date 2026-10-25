//! One prepared Original and one supervised exact-picture proposal attempt.
//! Faces and salient regions remain separate typed, deterministic collections.
//! No detection path saves targets or changes a project.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use deadpan_analysis::{NormalizedRect, target_region};
use deadpan_core::{AssetId, AttentionTarget, SourceSpan, SourceTimeBase, SourceTimestamp};
use deadpan_jobs::picture_proposals::{
    self, ExpectedStream, HostMessage, PictureProposalProtocol, WorkerMessage,
};
pub use deadpan_jobs::picture_proposals::{
    DetectedFace, DetectedSalientRegion, DetectorKind, DetectorRuntime, FaceProposals, MAX_FACES,
    MAX_SALIENT_REGIONS, PictureProposals, ProposalSelection, SALIENCY_REQUEST_REVISION,
    SalientRegionProposals, validate_faces, validate_salient_regions,
};
use deadpan_jobs::process::{ProcessEvent, ProcessLimits, ProcessSpec, SupervisedProcess};
use deadpan_jobs::{AttemptId, CancellationToken, RequestId, WorkspaceArtifact};

use crate::tracking::{TrackingError, TrackingRuntime};

#[derive(Debug, thiserror::Error)]
pub enum ProposalError {
    #[error("picture proposals are unavailable: {0}")]
    Unavailable(String),
    #[error("picture proposals request is invalid: {0}")]
    Request(String),
    #[error("picture proposals cancelled")]
    Cancelled,
    #[error("picture proposals deadline elapsed")]
    Deadline,
    #[error("picture proposals worker: {0}")]
    Worker(String),
    #[error("picture proposals protocol: {0}")]
    Protocol(String),
    #[error(transparent)]
    Supervisor(#[from] deadpan_jobs::process::SupervisorError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl From<TrackingError> for ProposalError {
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

fn protocol_error(error: impl std::fmt::Display) -> ProposalError {
    ProposalError::Protocol(error.to_string())
}

/// Which picture to analyse.
#[derive(Debug, Clone)]
pub struct PictureProposalRequest {
    pub asset: Option<AssetId>,
    pub selection: ProposalSelection,
    /// The picture displayed at this PTS (the last indexed picture at or
    /// before it) is analysed.
    pub at_pts: i64,
}

/// A verified copy of the Original and the exact picture to analyse,
/// prepared from a store that may be closed afterwards.
pub struct PreparedPictureProposals {
    selection: ProposalSelection,
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
    /// The analysed picture's exact upright aspect, which an accepted target
    /// records (`crate::tracking::exact_display_aspect`).
    pub display_aspect: Option<deadpan_core::ExactRatio>,
    /// The head document the picture was resolved against.
    pub head: deadpan_core::ProjectDocument,
    /// The Original's content identity.
    pub content: String,
}

impl PreparedPictureProposals {
    pub fn selection(&self) -> ProposalSelection {
        self.selection
    }
    pub fn stream(&self) -> &ExpectedStream {
        &self.stream
    }
}

/// Admitted, request-bound proposals from one clean worker completion.
#[derive(Debug, Clone)]
pub struct PictureProposalDetection {
    pub selection: ProposalSelection,
    pub pts: i64,
    pub proposals: PictureProposals,
    pub decode_elapsed: Duration,
    pub vision_elapsed: Duration,
    pub worker_elapsed: Duration,
}

/// Resolve the picture against the qualified index and the asset's measured
/// video span, and copy the verified Original into a new attempt workspace.
pub fn prepare_picture_proposals(
    store: &deadpan_store::ProjectStore,
    request: &PictureProposalRequest,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<PreparedPictureProposals, ProposalError> {
    let unavailable = |error: &dyn std::fmt::Display| ProposalError::Unavailable(error.to_string());
    let receipt = crate::shots::shot_receipt(store, request.asset.as_ref())
        .map_err(|error| unavailable(&error))?;
    let asset = match &request.asset {
        Some(asset) => asset.clone(),
        None => match store.single_source_state().map_err(|e| unavailable(&e))? {
            Some(deadpan_store::single_source::SingleSourceState::Ready { asset, .. }) => asset,
            _ => {
                return Err(ProposalError::Unavailable(
                    "project has no ready Original; choose a registered source asset".into(),
                ));
            }
        },
    };
    let document = store.snapshot().map_err(|e| unavailable(&e))?;
    let record = document
        .assets()
        .get(&asset)
        .ok_or_else(|| ProposalError::Unavailable("the asset is not registered".into()))?;
    let video_span = record
        .video
        .ok_or_else(|| ProposalError::Unavailable("the asset has no video span".into()))?;
    if record.source_qualification.as_ref() != Some(receipt.id()) {
        return Err(ProposalError::Unavailable(
            "the asset's qualification receipt differs from the project head".into(),
        ));
    }
    let time_base = video_span.start().time_base;
    let video = receipt
        .snapshot()
        .video()
        .ok_or_else(|| ProposalError::Unavailable("the source has no qualified picture".into()))?;
    let index = video.index().index();
    let frames = index.frames();
    let interpretation = video.interpretation();
    let after = frames.partition_point(|frame| frame.pts <= request.at_pts);
    let ordinal = after.checked_sub(1).ok_or_else(|| {
        ProposalError::Request("no picture is displayed before the first indexed picture".into())
    })?;
    let pts = frames[ordinal].pts;
    if pts < video_span.start().ticks || pts >= video_span.end().ticks {
        return Err(ProposalError::Request(
            "the picture lies outside the asset's measured video span".into(),
        ));
    }
    let stream = ExpectedStream {
        interpretation: document.source_interpretations().get(&asset).copied(),
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
        return Err(ProposalError::Unavailable(
            "the picture index and its interpretation disagree on the time base".into(),
        ));
    }
    let content = receipt.original().content().to_string();
    let (workspace, source) = crate::tracking::copy_verified_original(
        store,
        &receipt,
        video.index().content(),
        "deadpan-picture-proposals-",
        cancelled,
        deadline,
    )?;
    #[cfg(test)]
    tests::record_copy();
    Ok(PreparedPictureProposals {
        selection: request.selection,
        workspace,
        source,
        stream,
        asset,
        time_base,
        pts,
        ordinal,
        video_end: video_span.end().ticks,
        display_aspect: crate::tracking::exact_display_aspect(interpretation),
        head: document,
        content,
    })
}

/// One admission boundary for real and scripted proposal data. Scripted callers
/// must describe their own origin honestly; this validates identity and payload,
/// not whether a detector actually ran.
pub fn admit_picture_proposals(
    prepared: &PreparedPictureProposals,
    detection: PictureProposalDetection,
) -> Result<PictureProposalDetection, ProposalError> {
    admit_proposal_geometry(
        prepared,
        detection.selection,
        detection.pts,
        detection
            .proposals
            .faces
            .as_ref()
            .map(|found| found.faces.as_slice()),
        detection
            .proposals
            .salient_regions
            .as_ref()
            .map(|found| found.regions.as_slice()),
    )?;
    detection
        .proposals
        .validate(detection.selection)
        .map_err(ProposalError::Protocol)?;
    Ok(detection)
}

/// Shared geometry/identity admission for real and explicitly scripted results.
/// This never invents a detector runtime for a replay that did not run Vision.
pub fn admit_proposal_geometry(
    prepared: &PreparedPictureProposals,
    selection: ProposalSelection,
    pts: i64,
    faces: Option<&[DetectedFace]>,
    salient_regions: Option<&[DetectedSalientRegion]>,
) -> Result<(), ProposalError> {
    if pts != prepared.pts {
        return Err(ProposalError::Protocol(
            "proposals were reported for a different picture".into(),
        ));
    }
    if selection != prepared.selection {
        return Err(ProposalError::Protocol(
            "proposals were reported for a different detector selection".into(),
        ));
    }
    if faces.is_some() != selection.includes_faces()
        || salient_regions.is_some() != selection.includes_salient_regions()
    {
        return Err(ProposalError::Protocol(
            "proposal collections differ from selected detectors".into(),
        ));
    }
    if let Some(faces) = faces {
        validate_faces(faces).map_err(ProposalError::Protocol)?;
    }
    if let Some(regions) = salient_regions {
        validate_salient_regions(regions).map_err(ProposalError::Protocol)?;
    }
    Ok(())
}

/// Run one supervised detection attempt on the prepared picture.
pub fn detect_picture_proposals(
    runtime: &TrackingRuntime,
    prepared: &PreparedPictureProposals,
    attempt: &str,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<PictureProposalDetection, ProposalError> {
    crate::tracking::check(cancelled, deadline)?;
    if attempt.contains('/') || attempt.starts_with('.') {
        return Err(ProposalError::Request(
            "an attempt name is one plain path component".into(),
        ));
    }
    let request_id =
        RequestId::new(format!("picture-proposals-{attempt}")).map_err(protocol_error)?;
    let attempt_id = AttemptId::new(attempt).map_err(protocol_error)?;
    let token = CancellationToken::new(format!("cancel-{attempt}")).map_err(protocol_error)?;
    let admission = crate::bundle::qualify_native_worker_interruptible(
        crate::bundle::NativeWorker::Track,
        &runtime.executable,
        cancelled,
        deadline,
    );
    crate::tracking::check(cancelled, deadline)?;
    admission?;
    let remaining = deadline
        .saturating_duration_since(Instant::now())
        .min(Duration::from_secs(60 * 60));
    let timeout_millis = u64::try_from(remaining.as_millis()).unwrap_or(u64::MAX);
    if timeout_millis == 0 {
        return Err(ProposalError::Deadline);
    }
    let request = HostMessage::DetectPictureProposals {
        protocol: picture_proposals::VERSION,
        request: request_id,
        attempt: attempt_id,
        cancellation_token: token,
        source: prepared.source.clone(),
        stream: Box::new(prepared.stream),
        pts: prepared.pts,
        selection: prepared.selection,
        timeout_millis,
    };
    let mut process = SupervisedProcess::<PictureProposalProtocol>::spawn(
        ProcessSpec {
            executable: runtime.executable.clone(),
            arguments: vec![picture_proposals::WORKER_ARGUMENT.into()],
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
                return Err(ProposalError::Worker(format!(
                    "picture proposals stopped at its deadline; worker cleanup is unconfirmed: {cleanup}"
                )));
            }
            return Err(if was_cancelled {
                ProposalError::Cancelled
            } else {
                ProposalError::Deadline
            });
        }
        for event in process.poll(now)? {
            match event {
                ProcessEvent::Message(message) => match *message {
                    // The protocol adapter checked identity, picture, mode and payloads.
                    WorkerMessage::Completed {
                        pts,
                        selection,
                        proposals,
                        decode_millis,
                        vision_millis,
                        elapsed_millis,
                        ..
                    } => {
                        completion = Some((
                            pts,
                            selection,
                            proposals,
                            [decode_millis, vision_millis, elapsed_millis],
                        ));
                    }
                    WorkerMessage::Failed { diagnostic, .. } => {
                        failure.get_or_insert_with(|| {
                            ProposalError::Worker(diagnostic.as_str().to_owned())
                        });
                    }
                    WorkerMessage::Cancelled { .. } if was_cancelled => {}
                    WorkerMessage::Cancelled { .. } => {
                        failure.get_or_insert_with(|| {
                            ProposalError::Protocol("unrequested cancellation".into())
                        });
                    }
                },
                ProcessEvent::Fault(reason) => {
                    failure.get_or_insert(ProposalError::Worker(reason));
                }
                ProcessEvent::Exited {
                    status,
                    cancellation_escalated,
                } => {
                    if (!status.success() || cancellation_escalated)
                        && !was_cancelled
                        && failure.is_none()
                    {
                        failure = Some(ProposalError::Worker(format!(
                            "picture proposals worker exited {status}"
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
        return Err(ProposalError::Cancelled);
    }
    let (pts, selection, proposals, [decode, vision, elapsed]) =
        completion.ok_or_else(|| ProposalError::Protocol("no clean completion".into()))?;
    admit_picture_proposals(
        prepared,
        PictureProposalDetection {
            selection,
            pts,
            proposals,
            decode_elapsed: Duration::from_millis(decode),
            vision_elapsed: Duration::from_millis(vision),
            worker_elapsed: Duration::from_millis(elapsed),
        },
    )
}

/// Salient region number is one-based within the independently ordered region list.
pub fn choose_salient_region(
    regions: &[DetectedSalientRegion],
    number: usize,
) -> Result<&DetectedSalientRegion, String> {
    if regions.is_empty() {
        return Err("No salient regions were found in this picture.".into());
    }
    if number == 0 {
        return Err("Regions are numbered from 1, left to right.".into());
    }
    regions.get(number - 1).ok_or_else(|| format!(
        "region:{number} is out of range: this picture has {} salient region{} (region:1{}), numbered left to right.",
        regions.len(), if regions.len() == 1 { "" } else { "s" },
        if regions.len() == 1 { String::new() } else { format!("–region:{}", regions.len()) }))
}

/// A detected region as an ordinary, untracked attention target over
/// `[pts, end)` of `asset`: its rectangle is the target's initial region,
/// with no samples, corrections or provenance, and `display_aspect` is the
/// analysed picture's exact upright aspect. Saving it is the caller's
/// explicit, reversible `SetTarget` edit.
pub fn region_target(
    region: &NormalizedRect,
    label: String,
    asset: AssetId,
    time_base: SourceTimeBase,
    display_aspect: Option<deadpan_core::ExactRatio>,
    pts: i64,
    end: i64,
) -> Result<AttentionTarget, ProposalError> {
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
    .map_err(|error| ProposalError::Request(error.to_string()))?;
    Ok(AttentionTarget {
        label,
        asset,
        span,
        region: target_region(region).into(),
        samples: Vec::new(),
        corrections: Vec::new(),
        provenance: None,
        display_aspect,
    })
}

/// Prepare one input and return a read-only proposal report with per-kind numbering.
pub fn detect_project(
    runtime: &TrackingRuntime,
    project: &Path,
    request: &PictureProposalRequest,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<serde_json::Value, ProposalError> {
    let store = deadpan_store::ProjectStore::open(project, deadpan_store::AccessMode::ReadOnly)
        .map_err(|error| ProposalError::Unavailable(error.to_string()))?;
    let prepared = prepare_picture_proposals(&store, request, cancelled, deadline)?;
    drop(store);
    let attempt = uuid::Uuid::new_v4().simple().to_string();
    let started = Instant::now();
    let detection = detect_picture_proposals(runtime, &prepared, &attempt, cancelled, deadline)?;
    let millis = crate::tracking::millis;
    Ok(serde_json::json!({
        "protocol": picture_proposals::VERSION,
        "selection": detection.selection,
        "content": prepared.content,
        "asset": prepared.asset.as_str(),
        "revision": prepared.head.revision_id(),
        "stream": {"stream_index": prepared.stream.stream_index,
            "time_base": [prepared.stream.time_base_num, prepared.stream.time_base_den],
            "rotation_quarter_turns": prepared.stream.rotation_quarter_turns},
        "pts": detection.pts, "picture": prepared.ordinal,
        "elapsed_ms": millis(started.elapsed()),
        "worker_elapsed_ms": millis(detection.worker_elapsed),
        "decode_elapsed_ms": millis(detection.decode_elapsed),
        "vision_elapsed_ms": millis(detection.vision_elapsed),
        "faces": detection.proposals.faces.as_ref().map(|found| serde_json::json!({
            "runtime": found.runtime, "faces": found.faces.iter().enumerate()
                .map(|(i,face)| serde_json::json!({"face":i+1,"region":face.region,"confidence":face.confidence}))
                .collect::<Vec<_>>()})),
        "salient_regions": detection.proposals.salient_regions.as_ref().map(|found| serde_json::json!({
            "runtime": found.runtime, "regions": found.regions.iter().enumerate()
                .map(|(i,region)| serde_json::json!({"region_number":i+1,"region":region.region}))
                .collect::<Vec<_>>()})),
    }))
}

const USAGE: &str =
    "usage: detect-regions <project.deadpan> --at <pts> [--asset <id>] [--include-faces]";
pub fn run_detect_regions(arguments: &[&str]) -> Result<(), crate::CliError> {
    let usage = || crate::CliError::Usage(USAGE.into());
    let [path, rest @ ..] = arguments else {
        return Err(usage());
    };
    let found = crate::tracking::options(rest, &["--include-faces"], &usage)?;
    if found
        .keys()
        .any(|key| !["--at", "--asset", "--include-faces"].contains(key))
    {
        return Err(usage());
    }
    let request = PictureProposalRequest {
        asset: found
            .get("--asset")
            .map(|asset| AssetId::new(*asset))
            .transpose()
            .map_err(|error| crate::CliError::Usage(error.to_string()))?,
        at_pts: found
            .get("--at")
            .ok_or_else(usage)?
            .parse::<i64>()
            .map_err(|_| usage())?,
        selection: if found.contains_key("--include-faces") {
            ProposalSelection::Both
        } else {
            ProposalSelection::SalientRegions
        },
    };
    let cancelled = crate::tracking::cancellation()?;
    let deadline = Instant::now() + Duration::from_secs(10 * 60);
    let runtime = TrackingRuntime::beside_current_executable()?;
    let report = detect_project(&runtime, Path::new(path), &request, &cancelled, deadline)?;
    crate::write_json(&report)
}

#[cfg(test)]
mod tests;
