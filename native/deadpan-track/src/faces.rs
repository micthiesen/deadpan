//! Face-detection mode (`deadpan-track detect-faces`): decode exactly one
//! indexed picture and report Vision's face rectangles in the displayed
//! picture, ordered left to right, then top to bottom. Detection never edits
//! a project; the host treats every face as a proposal.

use std::io::{self, Write};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use deadpan_analysis::NormalizedRect;
use deadpan_jobs::faces::{
    DetectedFace, ExpectedStream, HostMessage, MAX_FACES, RuntimeReport, VERSION, WorkerMessage,
    read_host, write_worker,
};
use deadpan_jobs::protocol::{AttemptId, Diagnostic, RequestId, WorkspaceArtifact};
use deadpan_source::{DecodeLimits, SourceDecoder};

use super::{Interrupted, check_stream, decode_control, open_source, truncate, verify_source};

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

pub(super) fn main() -> ExitCode {
    let mut input = io::stdin();
    let Ok(Some(HostMessage::DetectFaces {
        request,
        attempt,
        cancellation_token,
        source,
        stream,
        pts,
        timeout_millis,
        ..
    })) = read_host(&mut input)
    else {
        return ExitCode::from(2);
    };
    let cancelled: &'static AtomicBool = Box::leak(Box::new(AtomicBool::new(false)));
    // As for tracking: later messages can only cancel, and end of input
    // means the host is gone.
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
    let deadline = Instant::now() + Duration::from_millis(timeout_millis);
    let message = match detect(&job, &source, &stream, pts, cancelled, deadline) {
        Ok(message) => message,
        Err(Interrupted::Cancelled) => cancelled_message(&job),
        Err(Interrupted::Failed(_)) if cancelled.load(Ordering::Acquire) => cancelled_message(&job),
        Err(Interrupted::Failed(error)) => {
            let diagnostic = Diagnostic::new(truncate(&error, 4_000))
                .unwrap_or_else(|_| Diagnostic::new("face detection failed").expect("static"));
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

fn detect(
    job: &Job,
    source: &WorkspaceArtifact,
    stream: &ExpectedStream,
    pts: i64,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<WorkerMessage, Interrupted> {
    let started = Instant::now();
    let control = || decode_control(cancelled, deadline);
    let file = open_source(source)?;
    verify_source(&file, source, cancelled, deadline)?;
    let mut decoder = SourceDecoder::open(file, DecodeLimits::default(), control()?)
        .map_err(|error| format!("open source: {error}"))?;
    check_stream(decoder.info(), stream)?;
    decoder
        .seek(pts, control()?)
        .map_err(|error| format!("seek to {pts}: {error}"))?;
    // Decode metadata up to the requested picture, which must exist exactly.
    loop {
        let Some(metadata) = decoder
            .next_metadata(control()?)
            .map_err(|error| format!("decode: {error}"))?
        else {
            return Err(format!("decoding ended before PTS {pts}").into());
        };
        if metadata.pts < pts {
            continue;
        }
        if metadata.pts != pts {
            return Err(format!(
                "the first decoded picture at or after PTS {pts} is at {}",
                metadata.pts
            )
            .into());
        }
        break;
    }
    let picture = decoder
        .copy_current_rgba(control()?)
        .map_err(|error| format!("convert picture at PTS {pts}: {error}"))?;
    control()?;
    let detected = Instant::now();
    let (faces, revision) = detect_in(&picture, stream.rotation_quarter_turns)?;
    let vision = detected.elapsed();
    control()?;
    let elapsed = started.elapsed();
    let millis = |duration: Duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX);
    Ok(WorkerMessage::Completed {
        protocol: VERSION,
        request: job.request.clone(),
        attempt: job.attempt.clone(),
        pts,
        faces,
        runtime: RuntimeReport {
            engine: face_engine().into(),
            request_revision: revision,
        },
        decode_millis: millis(elapsed.saturating_sub(vision)),
        vision_millis: millis(vision),
        elapsed_millis: millis(elapsed),
    })
}

/// Vision boxes (coded orientation, lower-left origin) as displayed-picture
/// rectangles with a top-left origin, clipped to the picture; boxes with no
/// usable part inside it are dropped. Ordered left to right, then top to
/// bottom; exact duplicates are reported once.
pub(super) fn displayed_faces(
    boxes: impl IntoIterator<Item = (f64, f64, f64, f64, f32)>,
    rotation: u8,
) -> Vec<DetectedFace> {
    let mut faces: Vec<DetectedFace> = boxes
        .into_iter()
        .filter_map(|(x, y, width, height, confidence)| {
            let coded = NormalizedRect::clipped(x, 1.0 - y - height, width, height)?;
            Some(DetectedFace {
                region: coded.displayed_from_coded(rotation),
                confidence: if confidence.is_finite() {
                    confidence.clamp(0.0, 1.0)
                } else {
                    0.0
                },
            })
        })
        .collect();
    faces.sort_by(DetectedFace::order);
    faces.dedup_by(|a, b| a.order(b).is_eq());
    faces.truncate(MAX_FACES);
    faces
}

#[cfg(target_os = "macos")]
fn face_engine() -> &'static str {
    super::vision::FACE_ENGINE
}

#[cfg(target_os = "macos")]
fn detect_in(
    picture: &deadpan_source::DecodedRgbaFrame,
    rotation: u8,
) -> Result<(Vec<DetectedFace>, u64), String> {
    let found = super::vision::detect_faces(
        &super::vision::Picture {
            width: picture.width,
            height: picture.height,
            row_stride_bytes: picture.row_stride_bytes,
            rgba: &picture.rgba,
        },
        MAX_FACES,
    )?;
    Ok((
        displayed_faces(
            found
                .faces
                .into_iter()
                .map(|(rect, confidence)| (rect.x, rect.y, rect.width, rect.height, confidence)),
            rotation,
        ),
        found.revision,
    ))
}

#[cfg(not(target_os = "macos"))]
fn face_engine() -> &'static str {
    "none"
}

/// No other platform has a qualified detection runtime.
#[cfg(not(target_os = "macos"))]
fn detect_in(
    _picture: &deadpan_source::DecodedRgbaFrame,
    _rotation: u8,
) -> Result<(Vec<DetectedFace>, u64), String> {
    Err("this platform has no qualified face-detection runtime".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vision_boxes_become_ordered_clipped_displayed_rectangles() {
        // Vision: lower-left origin. A face at the top-right, one at the
        // bottom-left, one partly outside the right edge, one wholly outside.
        let faces = displayed_faces(
            [
                (0.6, 0.6, 0.2, 0.3, 0.9),
                (0.1, 0.1, 0.2, 0.3, 0.8),
                (0.9, 0.4, 0.2, 0.2, f32::NAN),
                (1.2, 0.4, 0.2, 0.2, 0.7),
                (0.1, 0.1, 0.2, 0.3, 0.8),
            ],
            0,
        );
        let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
        assert_eq!(faces.len(), 3);
        assert!(near(faces[0].region.x(), 0.1) && near(faces[0].region.y(), 0.6));
        assert!(near(faces[1].region.x(), 0.6) && near(faces[1].region.y(), 0.1));
        assert!(near(faces[2].region.x(), 0.9) && near(faces[2].region.width(), 0.1));
        assert_eq!(faces[2].confidence, 0.0);
        assert!(deadpan_jobs::faces::validate_faces(&faces).is_ok());
        // A quarter turn clockwise: coded top-left becomes displayed top-right.
        let turned = displayed_faces([(0.0, 0.7, 0.2, 0.3, 0.9)], 1);
        assert!(near(turned[0].region.x(), 0.7) && near(turned[0].region.y(), 0.0));
    }
}
