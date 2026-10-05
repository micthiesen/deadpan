//! Host side of selected-target tracking.
//!
//! The host resolves the Original's qualified picture index, chooses the exact
//! range of indexed pictures (stopping at the first stored shot boundary by
//! default), copies the verified retained bytes into a fresh attempt
//! workspace, and launches the trusted `deadpan-track` executable through the
//! shared process supervisor. It admits the worker's raw observations only
//! after clean teardown, a contained hashed snapshot, bounded parsing and an
//! exact check against its own index, then applies the pure tracking policy.
//! Tracking itself never edits a project; `track --save` and `track-correct`
//! commit the result as an ordinary reversible `SetTarget` edit through the
//! project's writer (or the open app's authenticated endpoint).

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use deadpan_analysis::{
    Keyframe, MAX_TRACK_STRIDE, NormalizedRect, RawObservation, RawTrack, TRACK_RULE, TrackError,
    TrackPolicy, TrackRange, TrackStop, TrackedPath, rect_from_target, retrack_target,
    tracking_end,
};
use deadpan_core::{AssetId, AttentionTarget, SourceTimeBase, TargetId};
use deadpan_jobs::artifact::{ArtifactLimits, ArtifactWorkspace, SnapshotInterruption};
use deadpan_jobs::process::{ProcessEvent, ProcessLimits, ProcessSpec, SupervisedProcess};
use deadpan_jobs::tracking::{
    self, ExpectedStream, HostMessage, RuntimeReport, TrackingProtocol, WorkerMessage,
};
use deadpan_jobs::{
    AttemptId, CancellationToken, RequestId, Sha256, WorkspaceArtifact, WorkspaceRef,
};
use deadpan_store::ShotAnalysisKey;
use deadpan_store::original_media::OriginalMediaLimits;
use sha2::Digest;

const SOURCE: &str = "input/source";
const OUTPUT_SCOPE: &str = "output";

/// Selected by the trusted application host, never by project data.
#[derive(Debug, Clone)]
pub struct TrackingRuntime {
    pub executable: PathBuf,
    pub environment: BTreeMap<OsString, OsString>,
}

impl TrackingRuntime {
    /// The worker installed beside the current executable.
    pub fn beside_current_executable() -> Result<Self, TrackingError> {
        let current = std::env::current_exe()?;
        let directory = current.parent().ok_or(TrackingError::Unavailable(
            "executable has no directory".into(),
        ))?;
        Ok(Self {
            executable: directory.join("deadpan-track"),
            environment: BTreeMap::new(),
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TrackingError {
    #[error("tracking is unavailable: {0}")]
    Unavailable(String),
    #[error("tracking request is invalid: {0}")]
    Request(String),
    #[error("tracking cancelled")]
    Cancelled,
    #[error("tracking deadline elapsed")]
    Deadline,
    #[error("tracking worker: {0}")]
    Worker(String),
    #[error("tracking protocol: {0}")]
    Protocol(String),
    #[error(transparent)]
    Track(#[from] TrackError),
    #[error(transparent)]
    Supervisor(#[from] deadpan_jobs::process::SupervisorError),
    #[error(transparent)]
    Artifact(#[from] deadpan_jobs::artifact::ArtifactError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

fn protocol_error(error: impl std::fmt::Display) -> TrackingError {
    TrackingError::Protocol(error.to_string())
}

/// What to track, in the source's own picture clock.
#[derive(Debug, Clone)]
pub struct TrackRequest {
    pub asset: Option<deadpan_core::AssetId>,
    /// The picture displayed at this PTS is the one the region was selected on.
    pub from_pts: i64,
    /// Exclusive PTS end of the requested range.
    pub to_pts: i64,
    pub region: NormalizedRect,
    pub stride: u32,
    /// Stop at the first stored shot boundary after the start (the default).
    pub stop_at_shots: bool,
}

/// A verified copy of the Original in a fresh attempt workspace and the exact
/// pictures to track, prepared from a store that may be closed afterwards.
pub struct PreparedTrack {
    workspace: tempfile::TempDir,
    source: WorkspaceArtifact,
    stream: ExpectedStream,
    /// Every indexed picture PTS in `[start_pts, end_pts)`.
    pictures: Vec<i64>,
    end_pts: i64,
    stop: TrackStop,
    region: NormalizedRect,
    stride: u32,
    /// The tracked asset, its video's (reduced) time base and the displayed
    /// picture's width over height.
    pub asset: AssetId,
    pub time_base: SourceTimeBase,
    pub display_aspect: f64,
    /// The head document the range was resolved against, captured before any
    /// tracking. Saving uses its revision as the expected one, so edits made
    /// while tracking refuse the save instead of being overwritten.
    pub head: deadpan_core::ProjectDocument,
    /// The Original's content identity.
    pub content: String,
    /// The shot analysis that bounded the range, if one was consulted.
    pub shots: Option<ShotAnalysisKey>,
}

impl PreparedTrack {
    pub fn start_pts(&self) -> i64 {
        self.pictures[0]
    }
    pub fn end_pts(&self) -> i64 {
        self.end_pts
    }
    pub fn pictures(&self) -> &[i64] {
        &self.pictures
    }
    pub fn stop(&self) -> TrackStop {
        self.stop
    }
    pub fn stream(&self) -> &ExpectedStream {
        &self.stream
    }
    /// The PTS of every picture the worker will analyse.
    pub fn analysed(&self) -> impl Iterator<Item = i64> + '_ {
        self.pictures.iter().copied().step_by(self.stride as usize)
    }
}

#[derive(Debug, Clone)]
pub struct TrackingResult {
    pub path: TrackedPath,
    pub runtime: RuntimeReport,
    pub decode_elapsed: Duration,
    pub vision_elapsed: Duration,
    pub worker_elapsed: Duration,
    pub analysed: u32,
}

pub(crate) fn check(cancelled: &AtomicBool, deadline: Instant) -> Result<(), TrackingError> {
    if cancelled.load(Ordering::Acquire) {
        return Err(TrackingError::Cancelled);
    }
    if Instant::now() >= deadline {
        return Err(TrackingError::Deadline);
    }
    Ok(())
}

/// Resolve the range against the qualified index and stored shots, and copy
/// the verified Original into a new attempt workspace.
pub fn prepare_tracking(
    store: &deadpan_store::ProjectStore,
    request: &TrackRequest,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<PreparedTrack, TrackingError> {
    if !(1..=MAX_TRACK_STRIDE).contains(&request.stride) {
        return Err(TrackingError::Request(format!(
            "stride must be 1 to {MAX_TRACK_STRIDE}"
        )));
    }
    let unavailable = |error: &dyn std::fmt::Display| TrackingError::Unavailable(error.to_string());
    let receipt = crate::shots::shot_receipt(store, request.asset.as_ref())
        .map_err(|error| unavailable(&error))?;
    let asset = match &request.asset {
        Some(asset) => asset.clone(),
        None => match store.single_source_state().map_err(|e| unavailable(&e))? {
            Some(deadpan_store::single_source::SingleSourceState::Ready { asset, .. }) => asset,
            _ => {
                return Err(TrackingError::Unavailable(
                    "project has no ready Original; choose a registered source asset".into(),
                ));
            }
        },
    };
    // The asset's measured video span bounds every range, as targets require.
    let document = store.snapshot().map_err(|e| unavailable(&e))?;
    let video_span = document
        .assets()
        .get(&asset)
        .and_then(|record| record.video)
        .ok_or_else(|| TrackingError::Unavailable("the asset has no video span".into()))?;
    let time_base = video_span.start().time_base;
    // The receipt read above must be the one this head binds to the asset.
    let record = &document.assets()[&asset];
    if record.source_qualification.as_ref() != Some(receipt.id()) {
        return Err(TrackingError::Unavailable(
            "the asset's qualification receipt differs from the project head".into(),
        ));
    }
    let to_pts = request.to_pts.min(video_span.end().ticks);
    let video = receipt
        .snapshot()
        .video()
        .ok_or_else(|| TrackingError::Unavailable("the source has no qualified picture".into()))?;
    let index = video.index().index();
    let frames = index.frames();
    let base = index.time_base();
    let interpretation = video.interpretation();
    let first = frames.partition_point(|frame| frame.pts <= request.from_pts);
    if first == 0 || request.from_pts >= to_pts || frames[first - 1].pts < video_span.start().ticks
    {
        return Err(TrackingError::Request(
            "the range must start at or after the first picture and end after it starts".into(),
        ));
    }
    let start = first - 1;
    let requested_end = frames.partition_point(|frame| frame.pts < to_pts);
    let content = receipt.original().content().to_string();
    let stream_index = video.index().stream_index();
    let stored = if request.stop_at_shots {
        let stored = crate::shots::stored_shots(store, &content, stream_index, frames.len())
            .ok_or_else(|| {
                TrackingError::Unavailable(
                    "no stored shot analysis; run detect-shots, or pass --through-shots".into(),
                )
            })?;
        Some(stored)
    } else {
        None
    };
    let boundaries = stored
        .as_ref()
        .map(|(_, analysis)| analysis.boundaries())
        .unwrap_or_default();
    let (end, stop) = tracking_end(
        frames.len(),
        start,
        requested_end,
        &boundaries,
        request.stop_at_shots,
    )?;
    let end_pts = match stop {
        TrackStop::RangeEnd => to_pts,
        TrackStop::ShotBoundary { .. } | TrackStop::PictureLimit => frames[end].pts,
    };
    let pictures: Vec<i64> = frames[start..end].iter().map(|frame| frame.pts).collect();
    let stream = ExpectedStream {
        stream_index,
        width: interpretation.width,
        height: interpretation.height,
        time_base_num: interpretation.time_base_num,
        time_base_den: interpretation.time_base_den,
        rotation_quarter_turns: interpretation.rotation_quarter_turns,
    };
    // The index stores its time base reduced; the decoder reports it raw.
    if u64::from(base.numerator()) * u64::from(stream.time_base_den)
        != u64::from(base.denominator()) * u64::from(stream.time_base_num)
        || base != time_base
    {
        return Err(TrackingError::Unavailable(
            "the picture index and its interpretation disagree on the time base".into(),
        ));
    }

    // Sample aspect ratio stretches the coded width; odd quarter turns swap
    // the displayed axes.
    let coded_aspect = (f64::from(interpretation.width)
        * f64::from(interpretation.sample_aspect_num))
        / (f64::from(interpretation.height) * f64::from(interpretation.sample_aspect_den));
    let display_aspect = if interpretation.rotation_quarter_turns % 2 == 1 {
        coded_aspect.recip()
    } else {
        coded_aspect
    };

    let (workspace, source) = copy_verified_original(
        store,
        &receipt,
        video.index().content(),
        "deadpan-track-",
        cancelled,
        deadline,
    )?;
    std::fs::create_dir(workspace.path().join(OUTPUT_SCOPE))?;
    Ok(PreparedTrack {
        workspace,
        source,
        stream,
        pictures,
        end_pts,
        stop,
        region: request.region,
        stride: request.stride,
        asset,
        time_base,
        display_aspect,
        head: document,
        content,
        shots: stored.map(|(key, _)| key),
    })
}

/// Copy the receipt's verified Original into `input/source` of a fresh
/// attempt workspace, hashing as it goes, and admit the copy only when its
/// length and SHA-256 equal the qualified index's content identity. The
/// store's snapshot exposes no descriptor to clone, so this is one full copy
/// per attempt.
pub(crate) fn copy_verified_original(
    store: &deadpan_store::ProjectStore,
    receipt: &deadpan_store::source_registration::SourceQualificationReceipt,
    identity: deadpan_media::source_index::SourceContentIdentity,
    prefix: &str,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<(tempfile::TempDir, WorkspaceArtifact), TrackingError> {
    let unavailable = |error: &dyn std::fmt::Display| TrackingError::Unavailable(error.to_string());
    let remaining = deadline
        .saturating_duration_since(Instant::now())
        .min(Duration::from_secs(3_600));
    check(cancelled, deadline)?;
    let mut original = store
        .snapshot_original(
            receipt.original().content(),
            OriginalMediaLimits::new(tracking::MAX_SOURCE_BYTES, remaining)
                .map_err(|error| unavailable(&error))?,
            cancelled,
        )
        .map_err(|error| unavailable(&error))?;
    if original.record().object() != receipt.original() {
        return Err(TrackingError::Protocol(
            "retained Original differs from its receipt".into(),
        ));
    }
    let workspace = tempfile::Builder::new().prefix(prefix).tempdir()?;
    std::fs::create_dir(workspace.path().join("input"))?;
    let mut file = std::fs::File::create_new(workspace.path().join(SOURCE))?;
    let mut hasher = sha2::Sha256::new();
    let mut buffer = vec![0_u8; 1 << 20];
    let mut copied = 0_u64;
    loop {
        check(cancelled, deadline)?;
        let read = original.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        copied += read as u64;
        if copied > identity.byte_length() {
            return Err(TrackingError::Protocol(
                "the Original is longer than its index".into(),
            ));
        }
        hasher.update(&buffer[..read]);
        file.write_all(&buffer[..read])?;
    }
    file.sync_all()?;
    drop(file);
    let digest: [u8; 32] = hasher.finalize().into();
    if copied != identity.byte_length() || digest != identity.sha256() {
        return Err(TrackingError::Protocol(
            "the retained Original differs from its qualified index".into(),
        ));
    }
    let source = WorkspaceArtifact::new(
        WorkspaceRef::new(SOURCE).map_err(protocol_error)?,
        Sha256::new(hex(&digest)).map_err(protocol_error)?,
        copied,
    )
    .map_err(protocol_error)?;
    Ok((workspace, source))
}

/// Observations of one worker run and what the worker reported about it.
struct WorkerRun {
    observations: Vec<RawObservation>,
    runtime: RuntimeReport,
    analysed: u32,
    decode: Duration,
    vision: Duration,
    elapsed: Duration,
}

/// Run the worker over `range` (every indexed picture PTS of a contiguous part
/// of the prepared range, ending before `end_pts`) seeded with `seed`, and
/// admit its observations after teardown, a hashed snapshot and an exact
/// check against the requested pictures.
#[allow(clippy::too_many_arguments)]
fn run_worker(
    runtime: &TrackingRuntime,
    prepared: &PreparedTrack,
    range: &[i64],
    end_pts: i64,
    seed: Keyframe,
    attempt: &str,
    cancelled: &AtomicBool,
    deadline: Instant,
    mut progress: impl FnMut(u8),
) -> Result<WorkerRun, TrackingError> {
    check(cancelled, deadline)?;
    if range.first() != Some(&seed.pts) {
        return Err(TrackingError::Request(
            "the seed must be the range's first picture".into(),
        ));
    }
    let pictures = u32::try_from(range.len())
        .map_err(|_| TrackingError::Request("too many pictures".into()))?;
    // Validate every identity before touching the workspace.
    let request_id = RequestId::new(format!("track-{attempt}")).map_err(protocol_error)?;
    let attempt_id = AttemptId::new(attempt).map_err(protocol_error)?;
    let token = CancellationToken::new(format!("cancel-{attempt}")).map_err(protocol_error)?;
    if attempt.contains('/') || attempt.starts_with('.') {
        return Err(TrackingError::Request(
            "an attempt name is one plain path component".into(),
        ));
    }
    let scope = format!("{OUTPUT_SCOPE}/{attempt}");
    let scope_ref = WorkspaceRef::new(scope.clone()).map_err(protocol_error)?;
    // Each run writes below its own output scope.
    std::fs::create_dir(prepared.workspace.path().join(&scope))?;
    let analysed = u64::from(pictures.div_ceil(prepared.stride));
    let maximum_output_bytes = (u64::from(pictures) * tracking::DECODED_PTS_BYTES
        + analysed * tracking::OBSERVATION_BYTES)
        .clamp(4_096, tracking::MAX_OBSERVATION_BYTES);
    let pinned = ArtifactWorkspace::open(prepared.workspace.path())?;
    let remaining = deadline.saturating_duration_since(Instant::now());
    let timeout_millis = u64::try_from(remaining.as_millis()).unwrap_or(u64::MAX);
    if timeout_millis == 0 {
        return Err(TrackingError::Deadline);
    }
    let request = HostMessage::Track {
        protocol: tracking::VERSION,
        request: request_id,
        attempt: attempt_id,
        cancellation_token: token,
        source: prepared.source.clone(),
        stream: prepared.stream,
        start_pts: seed.pts,
        end_pts,
        pictures,
        stride: prepared.stride,
        region: seed.region,
        output_scope: scope_ref.clone(),
        maximum_output_bytes,
        timeout_millis,
    };
    let mut process = SupervisedProcess::<TrackingProtocol>::spawn(
        ProcessSpec {
            executable: runtime.executable.clone(),
            arguments: Vec::new(),
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
    let mut last_percent = 0;
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
                return Err(TrackingError::Worker(format!(
                    "tracking stopped at its deadline; worker cleanup is unconfirmed: {cleanup}"
                )));
            }
            return Err(if was_cancelled {
                TrackingError::Cancelled
            } else {
                TrackingError::Deadline
            });
        }
        for event in process.poll(now)? {
            match event {
                ProcessEvent::Message(message) => match *message {
                    WorkerMessage::Progress { percent, .. } => {
                        if percent < last_percent {
                            failure.get_or_insert_with(|| {
                                TrackingError::Protocol("progress moved backward".into())
                            });
                            process.request_cancel(now)?;
                        } else {
                            last_percent = percent;
                            progress(percent);
                        }
                    }
                    // The protocol adapter has checked scope, budget and counts.
                    WorkerMessage::Completed {
                        observations,
                        runtime,
                        analysed,
                        decode_millis,
                        vision_millis,
                        elapsed_millis,
                        ..
                    } => {
                        completion = Some((
                            observations,
                            runtime,
                            analysed,
                            [decode_millis, vision_millis, elapsed_millis],
                        ))
                    }
                    WorkerMessage::Failed { diagnostic, .. } => {
                        failure.get_or_insert_with(|| {
                            TrackingError::Worker(diagnostic.as_str().to_owned())
                        });
                    }
                    WorkerMessage::Cancelled { .. } if was_cancelled => {}
                    WorkerMessage::Cancelled { .. } => {
                        failure.get_or_insert_with(|| {
                            TrackingError::Protocol("unrequested cancellation".into())
                        });
                    }
                },
                ProcessEvent::Fault(reason) => {
                    failure.get_or_insert(TrackingError::Worker(reason));
                }
                ProcessEvent::Exited {
                    status,
                    cancellation_escalated,
                } => {
                    if (!status.success() || cancellation_escalated)
                        && !was_cancelled
                        && failure.is_none()
                    {
                        failure = Some(TrackingError::Worker(format!(
                            "tracking worker exited {status}"
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
        return Err(TrackingError::Cancelled);
    }
    let (artifact, runtime_report, analysed, [decode, vision, elapsed]) =
        completion.ok_or_else(|| TrackingError::Protocol("no clean completed artifact".into()))?;
    let mut snapshot = pinned.snapshot_with_control(
        &scope_ref,
        &artifact,
        ArtifactLimits::new(maximum_output_bytes)?,
        || {
            if cancelled.load(Ordering::Acquire) {
                return Err(SnapshotInterruption::Cancelled);
            }
            if Instant::now() >= deadline {
                return Err(SnapshotInterruption::Deadline);
            }
            Ok(())
        },
    )?;
    let mut bytes = Vec::new();
    snapshot
        .by_ref()
        .take(maximum_output_bytes + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 != artifact.byte_length() {
        return Err(TrackingError::Protocol(
            "artifact snapshot length changed".into(),
        ));
    }
    let RawTrack {
        decoded,
        observations,
    } = serde_json::from_slice(&bytes).map_err(protocol_error)?;
    // Every decoded picture, analysed or not, is the indexed one.
    if decoded != range {
        return Err(TrackingError::Protocol(
            "decoded pictures do not match the indexed pictures requested".into(),
        ));
    }
    if !observations
        .iter()
        .map(|observation| observation.pts)
        .eq(range.iter().copied().step_by(prepared.stride as usize))
    {
        return Err(TrackingError::Protocol(
            "observations do not match the indexed pictures requested".into(),
        ));
    }
    Ok(WorkerRun {
        observations,
        runtime: runtime_report,
        analysed,
        decode: Duration::from_millis(decode),
        vision: Duration::from_millis(vision),
        elapsed: Duration::from_millis(elapsed),
    })
}

/// Run one supervised tracking attempt from the selected region and apply
/// the policy.
pub fn track(
    runtime: &TrackingRuntime,
    prepared: &PreparedTrack,
    policy: TrackPolicy,
    attempt: &str,
    cancelled: &AtomicBool,
    deadline: Instant,
    progress: impl FnMut(u8),
) -> Result<TrackingResult, TrackingError> {
    policy.validate()?;
    let seed = Keyframe {
        pts: prepared.start_pts(),
        region: prepared.region,
    };
    let run = run_worker(
        runtime,
        prepared,
        &prepared.pictures,
        prepared.end_pts,
        seed,
        attempt,
        cancelled,
        deadline,
        progress,
    )?;
    let path = TrackedPath::track(
        policy,
        prepared.display_aspect,
        &prepared.pictures,
        prepared.end_pts,
        prepared.stop,
        seed,
        &run.observations,
    )?;
    Ok(TrackingResult {
        path,
        runtime: run.runtime,
        decode_elapsed: run.decode,
        vision_elapsed: run.vision,
        worker_elapsed: run.elapsed,
        analysed: run.analysed,
    })
}

/// Correct `path` with a manual keyframe and re-track only the range it
/// governs (to the next keyframe or the path's end), seeded from the
/// correction. All or nothing: the path changes only when the worker run and
/// the policy both succeed, so a failed or cancelled correction is never
/// mistaken for a loss.
#[allow(clippy::too_many_arguments)]
pub fn correct(
    runtime: &TrackingRuntime,
    prepared: &PreparedTrack,
    path: &mut TrackedPath,
    keyframe: Keyframe,
    attempt: &str,
    cancelled: &AtomicBool,
    deadline: Instant,
    progress: impl FnMut(u8),
) -> Result<TrackRange, TrackingError> {
    if path.start_pts() != prepared.start_pts() || path.end_pts() != prepared.end_pts {
        return Err(TrackingError::Request(
            "the path was not tracked over this prepared range".into(),
        ));
    }
    let first = prepared
        .pictures
        .binary_search(&keyframe.pts)
        .map_err(|_| TrackingError::Request("a keyframe must be at an indexed picture".into()))?;
    let range = path.governed_range(keyframe.pts)?;
    let last = prepared
        .pictures
        .partition_point(|&pts| pts < range.end_pts);
    let pictures = &prepared.pictures[first..last];
    let run = run_worker(
        runtime,
        prepared,
        pictures,
        range.end_pts,
        keyframe,
        attempt,
        cancelled,
        deadline,
        progress,
    )?;
    Ok(path.correct_and_retrack(keyframe, pictures, &run.observations)?)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(crate) fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

/// Parse `x,y,w,h` in `[0, 1]` of the displayed picture.
pub fn parse_region(text: &str) -> Result<NormalizedRect, TrackingError> {
    let values: Vec<f64> = text
        .split(',')
        .map(|part| part.trim().parse::<f64>())
        .collect::<Result<_, _>>()
        .map_err(|_| TrackingError::Request("region must be four numbers x,y,w,h".into()))?;
    let [x, y, width, height] = values[..] else {
        return Err(TrackingError::Request(
            "region must be four numbers x,y,w,h".into(),
        ));
    };
    NormalizedRect::new(x, y, width, height)
        .map_err(|error| TrackingError::Request(error.to_string()))
}

/// A finished tracking run, its JSON description and what saving it needs.
pub struct TrackOutcome {
    pub report: serde_json::Value,
    pub path: TrackedPath,
    pub asset: AssetId,
    pub time_base: SourceTimeBase,
    /// Tracker provenance: engine, request revision and level.
    pub engine: String,
    /// The head the range was resolved against, before tracking.
    pub head: deadpan_core::ProjectDocument,
}

/// Tracker provenance for a run: engine, request revision and level.
pub fn engine_label(runtime: &RuntimeReport) -> String {
    format!(
        "{} {} {}",
        runtime.engine, runtime.request_revision, runtime.tracking_level
    )
}

/// Prepare and run one tracking attempt and describe it as JSON.
pub fn track_project(
    runtime: &TrackingRuntime,
    project: &Path,
    request: &TrackRequest,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<TrackOutcome, TrackingError> {
    // Read-only throughout: tracking itself never writes the project.
    let store = deadpan_store::ProjectStore::open(project, deadpan_store::AccessMode::ReadOnly)
        .map_err(|error| TrackingError::Unavailable(error.to_string()))?;
    let prepared = prepare_tracking(&store, request, cancelled, deadline)?;
    drop(store);
    let attempt = uuid::Uuid::new_v4().simple().to_string();
    let started = Instant::now();
    let result = track(
        runtime,
        &prepared,
        TrackPolicy::default(),
        &attempt,
        cancelled,
        deadline,
        |_| {},
    )?;
    let elapsed = started.elapsed();
    let analysed = u64::from(result.analysed).max(1);
    let report = serde_json::json!({
        "protocol": 1,
        "rule": TRACK_RULE,
        "content": prepared.content,
        "asset": prepared.asset.as_str(),
        "stream": {
            "stream_index": prepared.stream.stream_index,
            "time_base": [prepared.stream.time_base_num, prepared.stream.time_base_den],
            "rotation_quarter_turns": prepared.stream.rotation_quarter_turns,
            "display_aspect": prepared.display_aspect,
        },
        "shot_analysis": prepared.shots,
        "pictures": prepared.pictures.len(),
        "analysed": result.analysed,
        "runtime": result.runtime,
        "elapsed_ms": millis(elapsed),
        "worker_elapsed_ms": millis(result.worker_elapsed),
        "decode_elapsed_ms": millis(result.decode_elapsed),
        "vision_elapsed_ms": millis(result.vision_elapsed),
        "vision_ms_per_picture": result.vision_elapsed.as_secs_f64() * 1_000.0 / analysed as f64,
        "path": result.path,
    });
    Ok(TrackOutcome {
        report,
        engine: engine_label(&result.runtime),
        path: result.path,
        asset: prepared.asset,
        time_base: prepared.time_base,
        head: prepared.head,
    })
}

/// Samples a target may hold beside the project's other targets.
pub fn sample_budget(document: &deadpan_core::ProjectDocument, id: &TargetId) -> usize {
    let others: usize = document
        .targets()
        .iter()
        .filter(|(other, _)| *other != id)
        .map(|(_, target)| target.samples.len())
        .sum();
    deadpan_core::MAX_DOCUMENT_TARGET_SAMPLES
        .saturating_sub(others)
        .min(deadpan_core::MAX_TARGET_SAMPLES)
}

/// Commit `target` as `id` with one reversible `SetTarget` edit against the
/// current head, through the writer or the open app's endpoint.
pub fn save_target(
    project: &Path,
    expected: &deadpan_core::ProjectDocument,
    id: TargetId,
    target: AttentionTarget,
) -> Result<serde_json::Value, crate::CliError> {
    let request = deadpan_core::CommandRequest {
        project_id: expected.project_id().clone(),
        expected_revision: expected.revision_id().clone(),
        new_revision: deadpan_core::RevisionId::new(uuid::Uuid::new_v4().to_string())?,
        command: deadpan_core::Command::SetTarget { id, target },
    };
    Ok(crate::live_project::dispatch_short(
        project,
        Some(request.project_id.clone()),
        crate::live_project::ShortOperation::Edit {
            request: Box::new(request),
            dry_run: false,
        },
    )?)
}

fn refuse_existing(
    head: &deadpan_core::ProjectDocument,
    id: &TargetId,
    replace: bool,
) -> Result<(), TrackingError> {
    if head.targets().contains_key(id) && !replace {
        return Err(TrackingError::Request(format!(
            "target {} already exists; pass --replace to overwrite it and its corrections",
            id.as_str()
        )));
    }
    Ok(())
}

/// Save a tracked outcome as target `id`, expecting the head it was resolved
/// against: an edit made meanwhile refuses the save (stale revision) rather
/// than being overwritten, and an existing target is replaced only with
/// `replace`.
pub fn save_tracked(
    project: &Path,
    outcome: &TrackOutcome,
    id: &TargetId,
    label: &str,
    replace: bool,
) -> Result<serde_json::Value, crate::CliError> {
    refuse_existing(&outcome.head, id, replace)?;
    let (target, tolerance) = outcome
        .path
        .to_target(
            label.into(),
            outcome.asset.clone(),
            outcome.time_base,
            &outcome.engine,
            sample_budget(&outcome.head, id),
        )
        .map_err(TrackingError::from)?;
    let samples = target.samples.len();
    let receipt = save_target(project, &outcome.head, id.clone(), target)?;
    Ok(serde_json::json!({
        "target": id.as_str(),
        "samples": samples,
        "compaction_tolerance_millionths": tolerance,
        "replaced": outcome.head.targets().contains_key(id),
        "receipt": receipt,
    }))
}

/// The current head document, read without the writer.
fn head_document(project: &Path) -> Result<deadpan_core::ProjectDocument, crate::CliError> {
    Ok(
        deadpan_store::ProjectStore::open(project, deadpan_store::AccessMode::ReadOnly)?
            .snapshot()?,
    )
}

/// SIGINT/SIGTERM request cancellation; a second signal exits at once.
pub(crate) fn cancellation() -> Result<Arc<AtomicBool>, crate::CliError> {
    let cancelled = Arc::new(AtomicBool::new(false));
    for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        signal_hook::flag::register_conditional_shutdown(signal, 130, Arc::clone(&cancelled))?;
        signal_hook::flag::register(signal, Arc::clone(&cancelled))?;
    }
    Ok(cancelled)
}

/// Options as `--name value` pairs plus bare flags, each at most once.
pub(crate) fn options<'a>(
    arguments: &[&'a str],
    flags: &[&str],
    usage: &dyn Fn() -> crate::CliError,
) -> Result<BTreeMap<&'a str, &'a str>, crate::CliError> {
    let mut found = BTreeMap::new();
    let mut rest = arguments.iter();
    while let Some(&option) = rest.next() {
        let value = if flags.contains(&option) {
            ""
        } else {
            rest.next().copied().ok_or_else(usage)?
        };
        if !option.starts_with("--") || found.insert(option, value).is_some() {
            return Err(usage());
        }
    }
    Ok(found)
}

const TRACK_USAGE: &str = "usage: track <project.deadpan> --from <pts> --to <pts> --region <x,y,w,h> [--asset <id>] [--stride <n>] [--through-shots] [--save <target-id> [--label <text>] [--replace]]";

/// `track PROJECT --from PTS --to PTS --region x,y,w,h [--asset ID]
/// [--stride N] [--through-shots] [--save ID [--label TEXT]]`: track a
/// selected region through the source's pictures and print the path; with
/// `--save`, commit it as an attention target.
pub fn run_track(arguments: &[&str]) -> Result<(), crate::CliError> {
    let usage = || crate::CliError::Usage(TRACK_USAGE.into());
    let [path, rest @ ..] = arguments else {
        return Err(usage());
    };
    let found = options(rest, &["--through-shots", "--replace"], &usage)?;
    let allowed = [
        "--from",
        "--to",
        "--region",
        "--asset",
        "--stride",
        "--through-shots",
        "--save",
        "--label",
        "--replace",
    ];
    if found.keys().any(|key| !allowed.contains(key))
        || ((found.contains_key("--label") || found.contains_key("--replace"))
            && !found.contains_key("--save"))
    {
        return Err(usage());
    }
    let integer = |key: &str| -> Result<i64, crate::CliError> {
        found
            .get(key)
            .ok_or_else(usage)?
            .parse::<i64>()
            .map_err(|_| usage())
    };
    let request = TrackRequest {
        asset: found
            .get("--asset")
            .map(|asset| AssetId::new(*asset))
            .transpose()
            .map_err(|e| crate::CliError::Usage(e.to_string()))?,
        from_pts: integer("--from")?,
        to_pts: integer("--to")?,
        region: parse_region(found.get("--region").ok_or_else(usage)?)?,
        stride: found
            .get("--stride")
            .map_or(Ok(1), |value| value.parse::<u32>().map_err(|_| usage()))?,
        stop_at_shots: !found.contains_key("--through-shots"),
    };
    let save = found
        .get("--save")
        .map(|id| TargetId::new(*id))
        .transpose()
        .map_err(|e| crate::CliError::Usage(e.to_string()))?;
    let label = found.get("--label").copied().unwrap_or("Tracked target");
    let replace = found.contains_key("--replace");
    let project = Path::new(path);
    // Refuse an existing target before spending time tracking.
    if let Some(id) = &save {
        refuse_existing(&head_document(project)?, id, replace)?;
    }
    let cancelled = cancellation()?;
    let deadline = Instant::now() + Duration::from_secs(6 * 60 * 60);
    let runtime = TrackingRuntime::beside_current_executable()?;
    let mut outcome = track_project(&runtime, project, &request, &cancelled, deadline)?;
    if let Some(id) = save {
        outcome.report["saved"] = save_tracked(project, &outcome, &id, label, replace)?;
    }
    crate::write_json(&outcome.report)
}

/// Re-track a saved target from a correction and commit it. Only the
/// correction's range (to the next correction or the span end) changes.
#[allow(clippy::too_many_arguments)]
pub fn correct_target(
    runtime: &TrackingRuntime,
    project: &Path,
    id: &TargetId,
    at: i64,
    region: NormalizedRect,
    stride: u32,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<(AttentionTarget, serde_json::Value), crate::CliError> {
    let document = head_document(project)?;
    let target = document
        .targets()
        .get(id)
        .cloned()
        .ok_or_else(|| TrackingError::Unavailable(format!("no target {}", id.as_str())))?;
    let (_, end) = target.correction_range(at).ok_or_else(|| {
        TrackingError::Request("the correction lies outside the target's span".into())
    })?;
    let request = TrackRequest {
        asset: Some(target.asset.clone()),
        from_pts: at,
        to_pts: end.ticks,
        region,
        stride,
        // The target's span already ends where tracking stopped.
        stop_at_shots: false,
    };
    let store = deadpan_store::ProjectStore::open(project, deadpan_store::AccessMode::ReadOnly)?;
    let prepared = prepare_tracking(&store, &request, cancelled, deadline)?;
    drop(store);
    if prepared.start_pts() != at || prepared.end_pts() != end.ticks {
        return Err(TrackingError::Request(
            "a correction must be at a picture PTS, and its range must fit one attempt".into(),
        )
        .into());
    }
    let attempt = uuid::Uuid::new_v4().simple().to_string();
    let result = track(
        runtime,
        &prepared,
        TrackPolicy::default(),
        &attempt,
        cancelled,
        deadline,
        |_| {},
    )?;
    if prepared.head.revision_id() != document.revision_id() {
        return Err(TrackingError::Unavailable(
            "the project changed while preparing the correction; try again".into(),
        )
        .into());
    }
    let (corrected, saved) = save_correction(
        project,
        &document,
        id,
        &result.path,
        &engine_label(&result.runtime),
    )?;
    Ok((
        corrected,
        serde_json::json!({
            "protocol": 1,
            "target": id.as_str(),
            "corrected_range": [at, end.ticks],
            "pictures": prepared.pictures.len(),
            "analysed": result.analysed,
            "runtime": result.runtime,
            "compaction_tolerance_millionths": saved["compaction_tolerance_millionths"],
            "receipt": saved["receipt"],
        }),
    ))
}

/// Apply `segment`, tracked from a correction over exactly its range, to
/// target `id` of `head` and commit it expecting that head. Samples before
/// the correction are unchanged.
pub fn save_correction(
    project: &Path,
    head: &deadpan_core::ProjectDocument,
    id: &TargetId,
    segment: &TrackedPath,
    engine: &str,
) -> Result<(AttentionTarget, serde_json::Value), crate::CliError> {
    let target = head
        .targets()
        .get(id)
        .ok_or_else(|| TrackingError::Unavailable(format!("no target {}", id.as_str())))?;
    let (corrected, tolerance) = retrack_target(target, segment, engine, sample_budget(head, id))
        .map_err(TrackingError::from)?;
    let receipt = save_target(project, head, id.clone(), corrected.clone())?;
    Ok((
        corrected,
        serde_json::json!({
            "compaction_tolerance_millionths": tolerance,
            "receipt": receipt,
        }),
    ))
}

const CORRECT_USAGE: &str = "usage: track-correct <project.deadpan> --target <id> --at <pts> --region <x,y,w,h> [--stride <n>]";

/// `track-correct PROJECT --target ID --at PTS --region x,y,w,h [--stride N]`.
pub fn run_track_correct(arguments: &[&str]) -> Result<(), crate::CliError> {
    let usage = || crate::CliError::Usage(CORRECT_USAGE.into());
    let [path, rest @ ..] = arguments else {
        return Err(usage());
    };
    let found = options(rest, &[], &usage)?;
    if found
        .keys()
        .any(|key| !["--target", "--at", "--region", "--stride"].contains(key))
    {
        return Err(usage());
    }
    let id = TargetId::new(*found.get("--target").ok_or_else(usage)?)
        .map_err(|e| crate::CliError::Usage(e.to_string()))?;
    let at = found
        .get("--at")
        .ok_or_else(usage)?
        .parse::<i64>()
        .map_err(|_| usage())?;
    let region = parse_region(found.get("--region").ok_or_else(usage)?)?;
    let stride = found
        .get("--stride")
        .map_or(Ok(1), |value| value.parse::<u32>().map_err(|_| usage()))?;
    let cancelled = cancellation()?;
    let deadline = Instant::now() + Duration::from_secs(6 * 60 * 60);
    let runtime = TrackingRuntime::beside_current_executable()?;
    let (_, report) = correct_target(
        &runtime,
        Path::new(path),
        &id,
        at,
        region,
        stride,
        &cancelled,
        deadline,
    )?;
    crate::write_json(&report)
}

/// The region a saved target gives `at`, as a normalized rectangle.
pub fn target_rect_at(target: &AttentionTarget, at: i64) -> Option<NormalizedRect> {
    let (region, _) = target.region_at(deadpan_core::SourcePoint {
        ticks: deadpan_core::ExactRatio::integer(at),
        time_base: target.span.start().time_base,
    })?;
    rect_from_target(&region)
}
