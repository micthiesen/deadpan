//! Complete, bounded landmark inspection of a private candidate and its PNGs.

use std::io::{self, Read, Write};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use deadpan_analysis::generated_geometry::{
    BoundaryObservations, FaceObservationSet, FrameObservation, RawLandmarkBatch,
};
use deadpan_jobs::landmarks::{
    BoundaryInputs, CONSTELLATION, ENGINE, ExpectedStream, HostMessage, MAX_DIMENSION, MAX_FRAMES,
    MAX_PIXELS, OUTPUT_FILE, REQUEST_REVISION, RuntimeReport, VERSION, WorkerMessage, read_host,
    write_worker,
};
use deadpan_jobs::protocol::{AttemptId, Diagnostic, RequestId, WorkspaceArtifact, WorkspaceRef};
use deadpan_source::{DecodeLimits, SourceDecoder};

use super::{
    Interrupted, check_stream, decode_control, open_source, truncate, verify_source, write_output,
};

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
    output_scope: WorkspaceRef,
    maximum_output_bytes: u64,
    deadline: Instant,
}

pub(super) fn main() -> ExitCode {
    let mut input = io::stdin();
    let Ok(Some(HostMessage::InspectLandmarks {
        request,
        attempt,
        cancellation_token,
        source,
        stream,
        picture_pts,
        boundaries,
        output_scope,
        maximum_output_bytes,
        timeout_millis,
        ..
    })) = read_host(&mut input)
    else {
        return ExitCode::from(2);
    };
    let cancelled: &'static AtomicBool = Box::leak(Box::new(AtomicBool::new(false)));
    let cancel_request = request.clone();
    let cancel_attempt = attempt.clone();
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
                HostMessage::InspectLandmarks { .. } => break,
            }
        }
        cancelled.store(true, Ordering::Release);
    });
    let job = Job { request, attempt };
    let Some(deadline) = Instant::now().checked_add(Duration::from_millis(timeout_millis)) else {
        return ExitCode::from(2);
    };
    let request = Request {
        source,
        stream,
        picture_pts,
        boundaries,
        output_scope,
        maximum_output_bytes,
        deadline,
    };
    let message = match inspect(&job, &request, cancelled) {
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
    let mut vision_time = Duration::ZERO;
    let total = request.picture_pts.len() + if request.boundaries.is_some() { 2 } else { 0 };
    let mut last_percent = None;
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
        let observation = detect_picture(
            picture.width,
            picture.height,
            picture.row_stride_bytes,
            &picture.rgba,
            request.stream.rotation_quarter_turns,
        )?;
        vision_time += vision_started.elapsed();
        control()?;
        frames.push(FrameObservation {
            ordinal: ordinal as u32,
            pts,
            observation,
        });
        progress(job, ordinal + 1, total, &mut last_percent)?;
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
    let boundaries = if let Some(inputs) = &request.boundaries {
        let left = inspect_png(&inputs.left, request, cancelled, &mut vision_time)?;
        progress(job, frames.len() + 1, total, &mut last_percent)?;
        let right = if inputs.left == inputs.right {
            left.clone()
        } else {
            inspect_png(&inputs.right, request, cancelled, &mut vision_time)?
        };
        control()?;
        progress(job, total, total, &mut last_percent)?;
        Some(BoundaryObservations { left, right })
    } else {
        None
    };
    let batch = RawLandmarkBatch {
        schema_version: 1,
        boundaries,
        frames,
    };
    batch
        .validate(&request.picture_pts)
        .map_err(|error| error.to_string())?;
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

fn inspect_png(
    artifact: &WorkspaceArtifact,
    request: &Request,
    cancelled: &AtomicBool,
    vision_time: &mut Duration,
) -> Result<FaceObservationSet, Interrupted> {
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
    let started = Instant::now();
    // Retained conditioning PNGs are already in displayed native orientation.
    let found = detect_picture(
        request.stream.width,
        request.stream.height,
        stride,
        &rgba,
        0,
    )?;
    *vision_time += started.elapsed();
    control()?;
    Ok(found)
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
