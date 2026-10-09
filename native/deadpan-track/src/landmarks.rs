//! Complete, bounded face and authored-region inspection of a private candidate.

use std::io::{self, Read, Write};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use deadpan_analysis::NormalizedRect;
use deadpan_analysis::generated_extension::ExtensionCoverage;
use deadpan_analysis::generated_geometry::{
    BoundaryObservations, FaceObservationSet, FrameObservation, RawLandmarkBatch,
};
use deadpan_analysis::generated_region::{
    RAW_REGION_SCHEMA_VERSION, RawRegionBatch, RawRegionFrame, RegionObservation,
    RegionObservationUnavailableReason, RegionSeeds,
};
use deadpan_jobs::landmarks::{
    BoundaryInputs, CONSTELLATION, ENGINE, ExpectedStream, HostMessage, InspectionObservations,
    MAX_DIMENSION, MAX_FRAMES, MAX_PIXELS, OBSERVATIONS_SCHEMA_VERSION, OUTPUT_FILE, REGION_ENGINE,
    REGION_REQUEST_REVISION, REGION_TRACKING_LEVEL, REQUEST_REVISION, RegionRuntimeReport,
    RuntimeReport, VERSION, WorkerMessage, read_host, write_worker,
};
use deadpan_jobs::protocol::{
    AttemptId, CancellationToken, Diagnostic, RequestId, WorkspaceArtifact, WorkspaceRef,
};
use deadpan_source::{DecodeLimits, SourceDecoder};

use super::{
    Interrupted, check_stream, decode_control, open_source, truncate, verify_source, write_output,
};

mod extension;

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

struct Request {
    source: WorkspaceArtifact,
    stream: ExpectedStream,
    picture_pts: Vec<i64>,
    boundaries: Option<Box<BoundaryInputs>>,
    region_seeds: Option<Box<RegionSeeds>>,
    extension: Option<ExtensionRequest>,
    output_scope: WorkspaceRef,
    maximum_output_bytes: u64,
    deadline: Instant,
}

struct ExtensionRequest {
    anchor: WorkspaceArtifact,
    coverage: ExtensionCoverage,
    region_seed: Option<NormalizedRect>,
}

impl Request {
    fn from_message(message: HostMessage) -> Option<(Job, CancellationToken, Self)> {
        match message {
            HostMessage::InspectLandmarks {
                request,
                attempt,
                cancellation_token,
                source,
                stream,
                picture_pts,
                boundaries,
                region_seeds,
                output_scope,
                maximum_output_bytes,
                timeout_millis,
                ..
            } => Some((
                Job { request, attempt },
                cancellation_token,
                Self {
                    source,
                    stream,
                    picture_pts,
                    boundaries,
                    region_seeds,
                    extension: None,
                    output_scope,
                    maximum_output_bytes,
                    deadline: Instant::now().checked_add(Duration::from_millis(timeout_millis))?,
                },
            )),
            HostMessage::InspectExtensionLandmarks {
                request,
                attempt,
                cancellation_token,
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
            } => Some((
                Job { request, attempt },
                cancellation_token,
                Self {
                    source,
                    stream,
                    picture_pts,
                    boundaries: None,
                    region_seeds: None,
                    extension: Some(ExtensionRequest {
                        anchor,
                        coverage,
                        region_seed,
                    }),
                    output_scope,
                    maximum_output_bytes,
                    deadline: Instant::now().checked_add(Duration::from_millis(timeout_millis))?,
                },
            )),
            HostMessage::Cancel { .. } => None,
        }
    }
}

pub(super) fn main() -> ExitCode {
    let mut input = io::stdin();
    let Ok(Some(message)) = read_host(&mut input) else {
        return ExitCode::from(2);
    };
    let Some((job, cancellation_token, request)) = Request::from_message(message) else {
        return ExitCode::from(2);
    };
    let cancelled: &'static AtomicBool = Box::leak(Box::new(AtomicBool::new(false)));
    let cancel_request = job.request.clone();
    let cancel_attempt = job.attempt.clone();
    // EOF or malformed control means the host is gone. Only the exact
    // captured cancellation identity can request cooperative cancellation.
    std::thread::spawn(move || {
        while let Ok(Some(message)) = read_host(&mut input) {
            match message {
                HostMessage::Cancel {
                    request,
                    attempt,
                    cancellation_token: token,
                    ..
                } if request == cancel_request
                    && attempt == cancel_attempt
                    && token == cancellation_token =>
                {
                    break;
                }
                HostMessage::Cancel { .. } => continue,
                HostMessage::InspectLandmarks { .. }
                | HostMessage::InspectExtensionLandmarks { .. } => break,
            }
        }
        cancelled.store(true, Ordering::Release);
    });
    let outcome = match &request.extension {
        Some(extension) => extension::inspect(&job, &request, extension, cancelled),
        None => inspect(&job, &request, cancelled),
    };
    let message = match outcome {
        Ok(message) => message,
        Err(Interrupted::Cancelled) => cancelled_message(&job),
        Err(Interrupted::Failed(_)) if cancelled.load(Ordering::Acquire) => cancelled_message(&job),
        Err(Interrupted::Failed(error)) => {
            let diagnostic = Diagnostic::new(truncate(&error, 4_000))
                .unwrap_or_else(|_| Diagnostic::new("landmark inspection failed").expect("static"));
            let _ = job.send(&WorkerMessage::Failed {
                protocol: VERSION,
                request: job.request.clone(),
                attempt: job.attempt.clone(),
                diagnostic,
            });
            return ExitCode::from(1);
        }
    };
    match job.send(&message) {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::from(2),
    }
}

fn cancelled_message(job: &Job) -> WorkerMessage {
    WorkerMessage::Cancelled {
        protocol: VERSION,
        request: job.request.clone(),
        attempt: job.attempt.clone(),
    }
}

fn inspect(
    job: &Job,
    request: &Request,
    cancelled: &AtomicBool,
) -> Result<WorkerMessage, Interrupted> {
    let started = Instant::now();
    let control = || decode_control(cancelled, request.deadline);
    control()?;
    let mut vision_time = Duration::ZERO;
    let total = request.picture_pts.len() + if request.boundaries.is_some() { 2 } else { 0 };
    let mut last_percent = None;
    // The seed image must be the first item in the tracking sequence. Retain
    // decoded pixels only when both endpoint descriptors refer to the same PNG.
    let left_png = request
        .boundaries
        .as_ref()
        .map(|inputs| decode_png(&inputs.left, request, cancelled))
        .transpose()?;
    let mut region_tracker = request
        .region_seeds
        .as_ref()
        .map(|seeds| RegionTracker::new(seeds.left))
        .transpose()?;
    let mut region_left = None;
    let left_faces = if let Some(png) = &left_png {
        let started = Instant::now();
        let faces = png.detect()?;
        control()?;
        if let Some(tracker) = &mut region_tracker {
            region_left = Some(tracker.track(png, false)?);
        }
        vision_time += started.elapsed();
        control()?;
        progress(job, 1, total, &mut last_percent)?;
        Some(faces)
    } else {
        None
    };
    let alias_png = request
        .boundaries
        .as_ref()
        .is_some_and(|inputs| inputs.left == inputs.right);
    let retained_left_png = if alias_png { left_png } else { None };
    let file = open_source(&request.source)?;
    verify_source(&file, &request.source, cancelled, request.deadline)?;
    let limits = DecodeLimits {
        max_input_bytes: request.source.byte_length(),
        // Include one final metadata call proving no omitted terminal picture.
        max_frames: MAX_FRAMES as u64 + 1,
        max_packets: (MAX_FRAMES as u64 + 1) * 64,
        max_pixels: MAX_PIXELS,
        max_dimension: MAX_DIMENSION,
        ..DecodeLimits::default()
    };
    let mut decoder = SourceDecoder::open(file, limits, control()?)
        .map_err(|error| format!("open landmark source: {error}"))?;
    check_stream(decoder.info(), &request.stream)?;
    let mut frames = Vec::with_capacity(request.picture_pts.len());
    let mut region_frames = Vec::with_capacity(if region_tracker.is_some() {
        request.picture_pts.len()
    } else {
        0
    });
    for (ordinal, &pts) in request.picture_pts.iter().enumerate() {
        let metadata = decoder
            .next_metadata(control()?)
            .map_err(|error| format!("decode landmark picture: {error}"))?
            .ok_or_else(|| format!("landmark decoding ended before picture {ordinal}"))?;
        if metadata.pts != pts {
            return Err(format!(
                "landmark picture {ordinal} is at PTS {}, expected {pts}",
                metadata.pts
            )
            .into());
        }
        let picture = decoder
            .copy_current_rgba(control()?)
            .map_err(|error| format!("convert landmark picture {ordinal}: {error}"))?;
        if picture.width != request.stream.width
            || picture.height != request.stream.height
            || picture.sample_bits != 8
            || picture.metadata.pts != pts
        {
            return Err("landmark picture differs from its captured raster or PTS"
                .to_owned()
                .into());
        }
        control()?;
        let vision_started = Instant::now();
        let picture =
            super::analysis_picture(picture, decoder.info(), cancelled, request.deadline)?;
        let observation = detect_picture(
            picture.width,
            picture.height,
            picture.row_stride_bytes,
            &picture.rgba,
            request.stream.rotation_quarter_turns,
        )?;
        control()?;
        if let Some(tracker) = &mut region_tracker {
            let observation = tracker.track_picture(
                picture.width,
                picture.height,
                picture.row_stride_bytes,
                &picture.rgba,
                false,
            )?;
            region_frames.push(RawRegionFrame {
                ordinal: ordinal as u32,
                pts,
                observation,
            });
        }
        vision_time += vision_started.elapsed();
        control()?;
        frames.push(FrameObservation {
            ordinal: ordinal as u32,
            pts,
            observation,
        });
        progress(
            job,
            ordinal + 1 + usize::from(left_faces.is_some()),
            total,
            &mut last_percent,
        )?;
    }
    if decoder
        .next_metadata(control()?)
        .map_err(|error| format!("verify landmark source end: {error}"))?
        .is_some()
    {
        return Err("landmark source has pictures beyond the requested sequence"
            .to_owned()
            .into());
    }
    drop(decoder);
    let mut region_right = None;
    let boundaries = if let (Some(inputs), Some(left)) = (&request.boundaries, left_faces) {
        let png = match retained_left_png {
            Some(png) => png,
            None => decode_png(&inputs.right, request, cancelled)?,
        };
        let started = Instant::now();
        let right = if alias_png {
            left.clone()
        } else {
            png.detect()?
        };
        control()?;
        // An aliased endpoint reuses its decode and face detection. Tracking
        // still consumes it at this final position, never reuses its old box.
        if let Some(tracker) = &mut region_tracker {
            region_right = Some(tracker.track(&png, true)?);
        }
        vision_time += started.elapsed();
        control()?;
        progress(job, total, total, &mut last_percent)?;
        Some(BoundaryObservations { left, right })
    } else {
        None
    };
    let landmarks = RawLandmarkBatch {
        schema_version: 1,
        boundaries,
        frames,
    };
    let region = match (&request.region_seeds, region_left, region_right) {
        (Some(seeds), Some(left), Some(right)) => Some(RawRegionBatch {
            schema_version: RAW_REGION_SCHEMA_VERSION,
            seeds: *seeds.as_ref(),
            left,
            frames: region_frames,
            right,
        }),
        (None, None, None) => None,
        _ => {
            return Err("region inspection lost its retained boundary observations"
                .to_owned()
                .into());
        }
    };
    let batch = InspectionObservations {
        schema_version: OBSERVATIONS_SCHEMA_VERSION,
        landmarks,
        region,
    };
    batch.validate(&request.picture_pts, request.region_seeds.as_deref())?;
    control()?;
    let mut bytes = BoundedBytes {
        bytes: Vec::new(),
        maximum: request.maximum_output_bytes,
    };
    serde_json::to_writer(&mut bytes, &batch).map_err(|error| error.to_string())?;
    control()?;
    let observations = write_output(
        &request.output_scope,
        OUTPUT_FILE,
        &bytes.bytes,
        request.maximum_output_bytes,
    )?;
    control()?;
    let elapsed = started.elapsed();
    let millis = |duration: Duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX);
    Ok(WorkerMessage::Completed {
        protocol: VERSION,
        request: job.request.clone(),
        attempt: job.attempt.clone(),
        observations,
        runtime: RuntimeReport {
            engine: ENGINE.into(),
            request_revision: REQUEST_REVISION,
            constellation: CONSTELLATION,
        },
        region_runtime: request.region_seeds.as_ref().map(|_| RegionRuntimeReport {
            engine: REGION_ENGINE.into(),
            request_revision: REGION_REQUEST_REVISION,
            tracking_level: REGION_TRACKING_LEVEL.into(),
        }),
        decoded: request.picture_pts.len() as u32,
        analysed: total as u32,
        decode_millis: millis(elapsed.saturating_sub(vision_time)),
        vision_millis: millis(vision_time),
        elapsed_millis: millis(elapsed),
    })
}

fn progress(
    job: &Job,
    completed: usize,
    total: usize,
    last: &mut Option<u8>,
) -> Result<(), String> {
    let percent = (completed * 100 / total) as u8;
    if *last != Some(percent) {
        job.send(&WorkerMessage::Progress {
            protocol: VERSION,
            request: job.request.clone(),
            attempt: job.attempt.clone(),
            percent,
        })?;
        *last = Some(percent);
    }
    Ok(())
}

fn decode_png(
    artifact: &WorkspaceArtifact,
    request: &Request,
    cancelled: &AtomicBool,
) -> Result<PngPicture, Interrupted> {
    let control = || decode_control(cancelled, request.deadline);
    control()?;
    let mut file = open_source(artifact)?;
    verify_source(&file, artifact, cancelled, request.deadline)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(
            usize::try_from(artifact.byte_length()).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
    let mut chunk = [0; 64 * 1024];
    loop {
        control()?;
        let count = file.read(&mut chunk).map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        if (bytes.len() as u64)
            .checked_add(count as u64)
            .is_none_or(|length| length > artifact.byte_length())
        {
            return Err("landmark PNG grew during reading".to_owned().into());
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
    if bytes.len() as u64 != artifact.byte_length() {
        return Err("landmark PNG length changed during reading"
            .to_owned()
            .into());
    }
    control()?;
    let image = deadpan_media::conditioning_png::decode_rgb8(
        &bytes,
        request.stream.width,
        request.stream.height,
    )?;
    control()?;
    let capacity = image
        .as_raw()
        .len()
        .checked_div(3)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| "landmark PNG RGBA size overflow".to_owned())?;
    let mut rgba = Vec::new();
    rgba.try_reserve_exact(capacity)
        .map_err(|error| error.to_string())?;
    for rgb in image.as_raw().chunks_exact(3) {
        rgba.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
    }
    let stride = usize::try_from(request.stream.width)
        .map_err(|error| error.to_string())?
        .checked_mul(4)
        .ok_or_else(|| "landmark PNG stride overflow".to_owned())?;
    control()?;
    Ok(PngPicture {
        width: request.stream.width,
        height: request.stream.height,
        stride,
        rgba,
    })
}

struct PngPicture {
    width: u32,
    height: u32,
    stride: usize,
    rgba: Vec<u8>,
}

impl PngPicture {
    fn detect(&self) -> Result<FaceObservationSet, String> {
        // Retained conditioning PNGs are already in displayed orientation.
        detect_picture(self.width, self.height, self.stride, &self.rgba, 0)
    }
}

struct RegionTracker {
    #[cfg(target_os = "macos")]
    inner: Option<super::vision::Tracker>,
}

impl RegionTracker {
    #[cfg(target_os = "macos")]
    fn new(seed: NormalizedRect) -> Result<Self, String> {
        let inner = super::vision::Tracker::new_pinned(super::vision::VisionRect {
            x: seed.x(),
            y: 1.0 - seed.y() - seed.height(),
            width: seed.width(),
            height: seed.height(),
        })?;
        Ok(Self { inner: Some(inner) })
    }

    #[cfg(not(target_os = "macos"))]
    fn new(_seed: NormalizedRect) -> Result<Self, String> {
        Err("this platform has no qualified region tracking runtime".into())
    }

    fn track(&mut self, png: &PngPicture, last: bool) -> Result<RegionObservation, String> {
        self.track_picture(png.width, png.height, png.stride, &png.rgba, last)
    }

    #[cfg(target_os = "macos")]
    fn track_picture(
        &mut self,
        width: u32,
        height: u32,
        row_stride_bytes: usize,
        rgba: &[u8],
        last: bool,
    ) -> Result<RegionObservation, String> {
        let Some(tracker) = &mut self.inner else {
            return Ok(RegionObservation::Unavailable {
                reason: RegionObservationUnavailableReason::LostTrack,
            });
        };
        let observation = match tracker.track(
            &super::vision::Picture {
                width,
                height,
                row_stride_bytes,
                rgba,
            },
            last,
        )? {
            None => RegionObservation::Unavailable {
                reason: RegionObservationUnavailableReason::Missing,
            },
            Some((rect, confidence)) => {
                measured_region(rect.x, rect.y, rect.width, rect.height, confidence)
            }
        };
        if matches!(observation, RegionObservation::Unavailable { .. }) {
            self.inner = None;
        }
        Ok(observation)
    }

    #[cfg(not(target_os = "macos"))]
    fn track_picture(
        &mut self,
        _width: u32,
        _height: u32,
        _row_stride_bytes: usize,
        _rgba: &[u8],
        _last: bool,
    ) -> Result<RegionObservation, String> {
        Err("this platform has no qualified region tracking runtime".into())
    }
}

#[cfg(any(target_os = "macos", test))]
fn measured_region(x: f64, y: f64, width: f64, height: f64, confidence: f32) -> RegionObservation {
    match NormalizedRect::new(x, 1.0 - y - height, width, height) {
        Ok(region) if confidence.is_finite() && (0.0..=1.0).contains(&confidence) => {
            RegionObservation::Tracked { region, confidence }
        }
        _ => RegionObservation::Unavailable {
            reason: RegionObservationUnavailableReason::InvalidGeometry,
        },
    }
}

#[cfg(target_os = "macos")]
fn detect_picture(
    width: u32,
    height: u32,
    row_stride_bytes: usize,
    rgba: &[u8],
    rotation: u8,
) -> Result<FaceObservationSet, String> {
    super::vision::detect_landmarks(
        &super::vision::Picture {
            width,
            height,
            row_stride_bytes,
            rgba,
        },
        rotation,
    )
}

#[cfg(not(target_os = "macos"))]
fn detect_picture(
    _width: u32,
    _height: u32,
    _row_stride_bytes: usize,
    _rgba: &[u8],
    _rotation: u8,
) -> Result<FaceObservationSet, String> {
    Err("this platform has no qualified landmark runtime".into())
}

struct BoundedBytes {
    bytes: Vec<u8>,
    maximum: u64,
}

impl Write for BoundedBytes {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() as u64 > self.maximum.saturating_sub(self.bytes.len() as u64) {
            return Err(io::Error::other(
                "landmark observations exceed their byte budget",
            ));
        }
        self.bytes
            .try_reserve(bytes.len())
            .map_err(io::Error::other)?;
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
