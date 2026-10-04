//! Private selected-target tracking worker.
//!
//! The host launches this executable with an empty environment in a fresh
//! attempt workspace and sends one framed `Track` message on stdin. The worker
//! opens the Original's bytes below `input/` without following links, checks
//! their exact length and SHA-256, decodes them through the pinned
//! descriptor-only FFmpeg decoder, and runs Apple Vision's object tracker on
//! every `stride`-th picture of the requested PTS range, seeded with the
//! selected region. It writes one raw observation per analysed picture to
//! `output/observations.json` created exclusively. Stdout carries only framed
//! protocol messages; diagnostics go to stderr, which the host keeps as a
//! bounded tail. The worker applies no tracking policy: the host does.

use std::fs::File;
use std::io::{self, Read, Seek, Write};
use std::path::Path;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use deadpan_analysis::{NormalizedRect, RawObservation, RawTrack};
use deadpan_jobs::protocol::{
    AttemptId, Diagnostic, RequestId, Sha256, WorkspaceArtifact, WorkspaceRef,
};
use deadpan_jobs::tracking::{
    ExpectedStream, HostMessage, RuntimeReport, VERSION, WorkerMessage, read_host, write_worker,
};
use deadpan_source::{DecodeControl, DecodeLimits, DecodedRgbaFrame, SourceDecoder};
use rustix::fs::{Mode, OFlags};
use sha2::Digest;

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod vision;

const OUTPUT_FILE: &str = "observations.json";
/// After this many consecutive Vision failures the tracker is not asked again;
/// the remaining pictures are reported without a region.
const MAX_CONSECUTIVE_FAILURES: u32 = 30;

struct Job {
    request: RequestId,
    attempt: AttemptId,
}

impl Job {
    fn send(&self, message: &WorkerMessage) -> Result<(), String> {
        let mut output = io::stdout().lock();
        write_worker(&mut output, message)?;
        output.flush().map_err(|error| error.to_string())
    }
}

/// The validated `Track` request.
struct Request {
    source: WorkspaceArtifact,
    stream: ExpectedStream,
    start_pts: i64,
    end_pts: i64,
    pictures: u32,
    stride: u32,
    region: NormalizedRect,
    output_scope: WorkspaceRef,
    maximum_output_bytes: u64,
    deadline: Instant,
}

fn main() -> ExitCode {
    let mut input = io::stdin();
    let Ok(Some(HostMessage::Track {
        request,
        attempt,
        cancellation_token,
        source,
        stream,
        start_pts,
        end_pts,
        pictures,
        stride,
        region,
        output_scope,
        maximum_output_bytes,
        timeout_millis,
        ..
    })) = read_host(&mut input)
    else {
        return ExitCode::from(2);
    };
    let cancelled: &'static AtomicBool = Box::leak(Box::new(AtomicBool::new(false)));
    // Later host messages can only cancel this attempt. The host holds stdin
    // open for the whole job, so end of input or a read error means it is
    // gone, and tracking stops instead of running unowned.
    std::thread::spawn(move || {
        while let Ok(Some(message)) = read_host(&mut input) {
            if let HostMessage::Cancel {
                cancellation_token: token,
                ..
            } = message
                && token == cancellation_token
            {
                break;
            }
        }
        cancelled.store(true, Ordering::Release);
    });
    let job = Job { request, attempt };
    let parameters = Request {
        source,
        stream,
        start_pts,
        end_pts,
        pictures,
        stride,
        region,
        output_scope,
        maximum_output_bytes,
        deadline: Instant::now() + Duration::from_millis(timeout_millis),
    };
    match run(&job, &parameters, cancelled) {
        Ok(Some(completed)) => match job.send(&completed) {
            Ok(()) => ExitCode::SUCCESS,
            Err(_) => ExitCode::from(2),
        },
        Ok(None) => {
            let message = WorkerMessage::Cancelled {
                protocol: VERSION,
                request: job.request.clone(),
                attempt: job.attempt.clone(),
            };
            match job.send(&message) {
                Ok(()) => ExitCode::SUCCESS,
                Err(_) => ExitCode::from(2),
            }
        }
        Err(error) => {
            let diagnostic = Diagnostic::new(truncate(&error, 4_000))
                .unwrap_or_else(|_| Diagnostic::new("tracking failed").expect("static"));
            let _ = job.send(&WorkerMessage::Failed {
                protocol: VERSION,
                request: job.request.clone(),
                attempt: job.attempt.clone(),
                diagnostic,
            });
            ExitCode::from(1)
        }
    }
}

/// Stop reasons that are not failures.
enum Interrupted {
    Cancelled,
    Failed(String),
}

impl From<String> for Interrupted {
    fn from(error: String) -> Self {
        Self::Failed(error)
    }
}

fn run(
    job: &Job,
    request: &Request,
    cancelled: &AtomicBool,
) -> Result<Option<WorkerMessage>, String> {
    match track(job, request, cancelled) {
        Ok(message) => Ok(Some(message)),
        Err(Interrupted::Cancelled) => Ok(None),
        Err(Interrupted::Failed(error)) => {
            if cancelled.load(Ordering::Acquire) {
                Ok(None)
            } else {
                Err(error)
            }
        }
    }
}

fn track(
    job: &Job,
    request: &Request,
    cancelled: &AtomicBool,
) -> Result<WorkerMessage, Interrupted> {
    let started = Instant::now();
    let control = || decode_control(cancelled, request.deadline);
    let file = open_source(&request.source)?;
    verify_source(&file, &request.source, cancelled, request.deadline)?;
    let mut decoder = SourceDecoder::open(file, DecodeLimits::default(), control()?)
        .map_err(|error| format!("open source: {error}"))?;
    check_stream(decoder.info(), &request.stream)?;
    decoder
        .seek(request.start_pts, control()?)
        .map_err(|error| format!("seek to {}: {error}", request.start_pts))?;

    let rotation = request.stream.rotation_quarter_turns;
    let seed = request.region.coded_from_displayed(rotation);
    let mut tracker = Tracker::new(seed)?;
    let mut observations = Vec::with_capacity(request.pictures.div_ceil(request.stride) as usize);
    let mut decoded = 0_u32;
    let mut decoded_pts = Vec::with_capacity(request.pictures as usize);
    let mut failures = 0_u32;
    let mut vision_time = Duration::ZERO;
    let mut last_percent = None;
    while decoded < request.pictures {
        let Some(metadata) = decoder
            .next_metadata(control()?)
            .map_err(|error| format!("decode: {error}"))?
        else {
            return Err(format!(
                "decoding ended after {decoded} of {} range pictures",
                request.pictures
            )
            .into());
        };
        if decoded == 0 && metadata.pts < request.start_pts {
            continue;
        }
        if decoded == 0 && metadata.pts != request.start_pts {
            return Err(format!(
                "the first decoded range picture is at PTS {}, not {}",
                metadata.pts, request.start_pts
            )
            .into());
        }
        if metadata.pts >= request.end_pts {
            return Err(format!(
                "only {decoded} of {} range pictures precede PTS {}",
                request.pictures, request.end_pts
            )
            .into());
        }
        // Presentation order is strict; the host checks every PTS against its
        // index.
        if decoded_pts
            .last()
            .is_some_and(|&previous| previous >= metadata.pts)
        {
            return Err(format!(
                "decoded PTS {} does not follow {:?}",
                metadata.pts,
                decoded_pts.last()
            )
            .into());
        }
        decoded_pts.push(metadata.pts);
        let ordinal = decoded;
        decoded += 1;
        if !ordinal.is_multiple_of(request.stride) {
            continue;
        }
        let picture = decoder
            .copy_current_rgba(control()?)
            .map_err(|error| format!("convert picture at PTS {}: {error}", metadata.pts))?;
        if failures >= MAX_CONSECUTIVE_FAILURES {
            observations.push(RawObservation {
                pts: metadata.pts,
                region: None,
                confidence: 0.0,
            });
            continue;
        }
        let tracked = Instant::now();
        // Vision releases its tracker after a request marked as the last.
        let last = ordinal + request.stride >= request.pictures;
        let result = tracker.track(&picture, last);
        vision_time += tracked.elapsed();
        let (region, confidence) = match result {
            Ok(Some((rect, confidence))) => {
                failures = 0;
                // Vision boxes may extend past the picture; keep the part
                // inside it, in displayed orientation with a top-left origin.
                let coded = NormalizedRect::clipped(
                    rect.x,
                    1.0 - rect.y - rect.height,
                    rect.width,
                    rect.height,
                );
                let confidence = if confidence.is_finite() {
                    confidence.clamp(0.0, 1.0)
                } else {
                    0.0
                };
                (
                    coded.map(|coded| coded.displayed_from_coded(rotation)),
                    confidence,
                )
            }
            Ok(None) => {
                failures += 1;
                (None, 0.0)
            }
            Err(error) => {
                failures += 1;
                eprintln!("tracking failed at PTS {}: {error}", metadata.pts);
                (None, 0.0)
            }
        };
        observations.push(RawObservation {
            pts: metadata.pts,
            region,
            confidence,
        });
        let percent = (u64::from(decoded) * 100 / u64::from(request.pictures)) as u8;
        if last_percent != Some(percent) {
            last_percent = Some(percent);
            job.send(&WorkerMessage::Progress {
                protocol: VERSION,
                request: job.request.clone(),
                attempt: job.attempt.clone(),
                percent,
            })?;
        }
    }
    let revision = tracker.revision();
    control()?;
    let analysed = observations.len() as u32;
    let bytes = serde_json::to_vec(&RawTrack {
        decoded: decoded_pts,
        observations,
    })
    .map_err(|error| error.to_string())?;
    let artifact = write_output(
        &request.output_scope,
        OUTPUT_FILE,
        &bytes,
        request.maximum_output_bytes,
    )?;
    let elapsed = started.elapsed();
    let millis = |duration: Duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX);
    Ok(WorkerMessage::Completed {
        protocol: VERSION,
        request: job.request.clone(),
        attempt: job.attempt.clone(),
        observations: artifact,
        runtime: RuntimeReport {
            engine: tracker_engine().into(),
            request_revision: revision,
            tracking_level: TRACKING_LEVEL.into(),
        },
        decoded,
        analysed,
        decode_millis: millis(elapsed.saturating_sub(vision_time)),
        vision_millis: millis(vision_time),
        elapsed_millis: millis(elapsed),
    })
}

/// The per-call decoder budget, or why work must stop.
fn decode_control(
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<DecodeControl<'_>, Interrupted> {
    if cancelled.load(Ordering::Acquire) {
        return Err(Interrupted::Cancelled);
    }
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err(Interrupted::Failed("tracking deadline elapsed".into()));
    }
    Ok(DecodeControl {
        timeout: remaining.min(Duration::from_secs(60)),
        cancelled,
    })
}

fn check_stream(
    info: &deadpan_source::SourceStreamInfo,
    expected: &ExpectedStream,
) -> Result<(), String> {
    if info.stream_index != expected.stream_index
        || (info.width, info.height) != (expected.width, expected.height)
        || (info.time_base_num, info.time_base_den)
            != (expected.time_base_num, expected.time_base_den)
        || info.rotation_quarter_turns != expected.rotation_quarter_turns
    {
        return Err("the decoder selected a different picture stream or interpretation".into());
    }
    Ok(())
}

/// Open the source directly below `input/` without following links.
fn open_source(source: &WorkspaceArtifact) -> Result<File, String> {
    let reference = Path::new(source.reference().as_str());
    let (directory, name) = match (reference.parent(), reference.file_name()) {
        (Some(directory), Some(name)) if directory == Path::new("input") => (directory, name),
        _ => return Err("the source must be directly below input/".into()),
    };
    let parent = rustix::fs::open(
        directory,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|error| format!("open input: {error}"))?;
    let descriptor = rustix::fs::openat(
        &parent,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|error| format!("open source: {error}"))?;
    let file = File::from(descriptor);
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("the source is not a regular file".into());
    }
    Ok(file)
}

/// Check the exact length and SHA-256, checking cancellation between chunks,
/// and leave the descriptor at offset zero for the decoder.
fn verify_source(
    mut file: &File,
    source: &WorkspaceArtifact,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<(), Interrupted> {
    let length = file.metadata().map_err(|e| e.to_string())?.len();
    if length != source.byte_length() {
        return Err(format!(
            "source has {length} bytes, expected {}",
            source.byte_length()
        )
        .into());
    }
    let mut hasher = sha2::Sha256::new();
    let mut buffer = vec![0_u8; 1 << 20];
    let mut total = 0_u64;
    loop {
        decode_control(cancelled, deadline)?;
        let read = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        total += read as u64;
        hasher.update(&buffer[..read]);
    }
    if total != length {
        return Err("the source changed while hashing".to_owned().into());
    }
    if hex(&hasher.finalize()) != source.sha256().as_str() {
        return Err("the source hash differs from its declaration"
            .to_owned()
            .into());
    }
    file.rewind().map_err(|e| e.to_string())?;
    Ok(())
}

/// Create `name` exclusively below the output scope and write `bytes`.
fn write_output(
    output_scope: &WorkspaceRef,
    name: &str,
    bytes: &[u8],
    maximum_bytes: u64,
) -> Result<WorkspaceArtifact, String> {
    if bytes.is_empty() || bytes.len() as u64 > maximum_bytes {
        return Err(format!("{name} exceeds its byte budget"));
    }
    let parent = rustix::fs::open(
        Path::new(output_scope.as_str()),
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|error| format!("open output scope: {error}"))?;
    let descriptor = rustix::fs::openat(
        &parent,
        name,
        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::from_raw_mode(0o600),
    )
    .map_err(|error| format!("create {name}: {error}"))?;
    let mut file = File::from(descriptor);
    file.write_all(bytes).map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    let reference = WorkspaceRef::new(format!("{}/{name}", output_scope.as_str()))
        .map_err(|error| error.to_string())?;
    let digest = Sha256::new(hex(&sha2::Sha256::digest(bytes))).map_err(|e| e.to_string())?;
    WorkspaceArtifact::new(reference, digest, bytes.len() as u64).map_err(|error| error.to_string())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn truncate(text: &str, limit: usize) -> String {
    let mut end = text.len().min(limit);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].replace('\0', " ")
}

#[cfg(target_os = "macos")]
use vision::TRACKING_LEVEL;

#[cfg(target_os = "macos")]
fn tracker_engine() -> &'static str {
    vision::ENGINE
}

/// The Vision tracker seeded in coded orientation.
#[cfg(target_os = "macos")]
struct Tracker {
    inner: vision::Tracker,
}

#[cfg(target_os = "macos")]
impl Tracker {
    fn new(seed: NormalizedRect) -> Result<Self, String> {
        // Vision's origin is the lower-left corner.
        Ok(Self {
            inner: vision::Tracker::new(vision::VisionRect {
                x: seed.x(),
                y: 1.0 - seed.y() - seed.height(),
                width: seed.width(),
                height: seed.height(),
            }),
        })
    }

    fn track(
        &mut self,
        picture: &DecodedRgbaFrame,
        last: bool,
    ) -> Result<Option<(vision::VisionRect, f32)>, String> {
        self.inner.track(
            &vision::Picture {
                width: picture.width,
                height: picture.height,
                row_stride_bytes: picture.row_stride_bytes,
                rgba: &picture.rgba,
            },
            last,
        )
    }

    fn revision(&self) -> u64 {
        self.inner.revision()
    }
}

#[cfg(not(target_os = "macos"))]
const TRACKING_LEVEL: &str = "none";

#[cfg(not(target_os = "macos"))]
fn tracker_engine() -> &'static str {
    "none"
}

/// No other platform has a qualified tracking runtime.
#[cfg(not(target_os = "macos"))]
struct Tracker;

#[cfg(not(target_os = "macos"))]
impl Tracker {
    fn new(_seed: NormalizedRect) -> Result<Self, String> {
        Err("this platform has no qualified tracking runtime".into())
    }

    fn track(
        &mut self,
        _picture: &DecodedRgbaFrame,
        _last: bool,
    ) -> Result<Option<(VisionRect, f32)>, String> {
        Err("this platform has no qualified tracking runtime".into())
    }

    fn revision(&self) -> u64 {
        0
    }
}

#[cfg(not(target_os = "macos"))]
struct VisionRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}
