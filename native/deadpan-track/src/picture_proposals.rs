//! Picture-proposal mode: verify and decode one exact indexed picture once,
//! then report the complete requested face/saliency batch. No project edits.

use std::io::{self, Write};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use deadpan_analysis::NormalizedRect;
use deadpan_jobs::picture_proposals::{
    DetectedFace, DetectedSalientRegion, DetectorKind, DetectorRuntime, ExpectedStream,
    FACE_ENGINE, FaceProposals, HostMessage, MAX_FACES, MAX_SALIENT_REGIONS, PictureProposals,
    ProposalSelection, SALIENCY_ENGINE, SalientRegionProposals, VERSION, WorkerMessage, read_host,
    write_worker,
};
use deadpan_jobs::protocol::{
    AttemptId, CancellationToken, Diagnostic, RequestId, WorkspaceArtifact,
};
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
    let Ok(Some(HostMessage::DetectPictureProposals {
        request,
        attempt,
        cancellation_token,
        source,
        stream,
        pts,
        selection,
        timeout_millis,
        ..
    })) = read_host(&mut input)
    else {
        return ExitCode::from(2);
    };
    let cancelled: &'static AtomicBool = Box::leak(Box::new(AtomicBool::new(false)));
    let cancelled_request = request.clone();
    let cancelled_attempt = attempt.clone();
    // As for tracking: later messages can only cancel, and end of input
    // means the host is gone.
    std::thread::spawn(move || {
        while let Ok(Some(message)) = read_host(&mut input) {
            if matches_cancel(
                &message,
                &cancelled_request,
                &cancelled_attempt,
                &cancellation_token,
            ) {
                break;
            }
        }
        cancelled.store(true, Ordering::Release);
    });
    let job = Job { request, attempt };
    let deadline = Instant::now() + Duration::from_millis(timeout_millis);
    let message = match detect(&job, &source, &stream, pts, selection, cancelled, deadline) {
        Ok(message) => message,
        Err(Interrupted::Cancelled) => cancelled_message(&job),
        Err(Interrupted::Failed(_)) if cancelled.load(Ordering::Acquire) => cancelled_message(&job),
        Err(Interrupted::Failed(error)) => {
            let diagnostic = Diagnostic::new(truncate(&error, 4_000))
                .unwrap_or_else(|_| Diagnostic::new("picture proposals failed").expect("static"));
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

fn matches_cancel(
    message: &HostMessage,
    request: &RequestId,
    attempt: &AttemptId,
    token: &CancellationToken,
) -> bool {
    matches!(message, HostMessage::Cancel { request: found_request, attempt: found_attempt,
        cancellation_token, .. } if found_request == request && found_attempt == attempt && cancellation_token == token)
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
    selection: ProposalSelection,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<WorkerMessage, Interrupted> {
    let started = Instant::now();
    let control = || decode_control(cancelled, deadline);
    let file = open_source(source)?;
    verify_source(&file, source, cancelled, deadline)?;
    let mut decoder = SourceDecoder::open_interpreted(
        file,
        DecodeLimits::default(),
        control()?,
        stream.interpretation.map(Into::into).unwrap_or_default(),
    )
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
    let picture = super::vision_picture(picture, decoder.info(), cancelled, deadline)?;
    control()?;
    let detected = Instant::now();
    let proposals = detect_in(&picture, stream.rotation_quarter_turns, selection)?;
    let vision = detected.elapsed();
    control()?;
    let elapsed = started.elapsed();
    let millis = |duration: Duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX);
    Ok(WorkerMessage::Completed {
        protocol: VERSION,
        request: job.request.clone(),
        attempt: job.attempt.clone(),
        pts,
        selection,
        proposals,
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

/// Apply the same coded-to-upright clipping policy as faces, but never
/// repair invalid numeric geometry or infer a relevance score.
pub(super) fn displayed_salient_regions(
    boxes: impl IntoIterator<Item = (f64, f64, f64, f64)>,
    rotation: u8,
) -> Result<Vec<DetectedSalientRegion>, String> {
    if rotation > 3 {
        return Err("salient-region rotation is invalid".into());
    }
    let mut regions = Vec::new();
    for (index, (x, y, width, height)) in boxes.into_iter().enumerate() {
        if index >= MAX_SALIENT_REGIONS {
            return Err(format!(
                "Vision found more than {MAX_SALIENT_REGIONS} salient regions"
            ));
        }
        if [x, y, width, height].iter().any(|value| !value.is_finite())
            || width <= 0.0
            || height <= 0.0
        {
            return Err("Vision returned invalid salient-region geometry".into());
        }
        if let Some(coded) = NormalizedRect::clipped(x, 1.0 - y - height, width, height) {
            regions.push(DetectedSalientRegion {
                region: coded.displayed_from_coded(rotation),
            });
        }
    }
    let order = |a: &DetectedSalientRegion, b: &DetectedSalientRegion| {
        a.region
            .x()
            .total_cmp(&b.region.x())
            .then(a.region.y().total_cmp(&b.region.y()))
            .then(a.region.width().total_cmp(&b.region.width()))
            .then(a.region.height().total_cmp(&b.region.height()))
    };
    regions.sort_by(order);
    regions.dedup_by(|a, b| a.region == b.region);
    deadpan_jobs::picture_proposals::validate_salient_regions(&regions)?;
    Ok(regions)
}

#[cfg(target_os = "macos")]
fn detect_in(
    picture: &deadpan_source::DecodedRgbaFrame,
    rotation: u8,
    selection: ProposalSelection,
) -> Result<PictureProposals, String> {
    let found = super::vision::detect_picture_proposals(
        &super::vision::Picture {
            width: picture.width,
            height: picture.height,
            row_stride_bytes: picture.row_stride_bytes,
            rgba: &picture.rgba,
        },
        selection,
        MAX_FACES,
        MAX_SALIENT_REGIONS,
    )?;
    let faces = found.faces.map(|found| FaceProposals {
        faces: displayed_faces(
            found
                .faces
                .into_iter()
                .map(|(rect, confidence)| (rect.x, rect.y, rect.width, rect.height, confidence)),
            rotation,
        ),
        runtime: DetectorRuntime {
            detector: DetectorKind::Faces,
            engine: FACE_ENGINE.into(),
            request_revision: found.revision,
        },
    });
    let salient_regions = found
        .salient_regions
        .map(|found| {
            Ok::<_, String>(SalientRegionProposals {
                regions: displayed_salient_regions(
                    found
                        .regions
                        .into_iter()
                        .map(|rect| (rect.x, rect.y, rect.width, rect.height)),
                    rotation,
                )?,
                runtime: DetectorRuntime {
                    detector: DetectorKind::SalientRegions,
                    engine: SALIENCY_ENGINE.into(),
                    request_revision: found.revision,
                },
            })
        })
        .transpose()?;
    let proposals = PictureProposals {
        faces,
        salient_regions,
    };
    proposals.validate(selection)?;
    Ok(proposals)
}

/// No other platform has a qualified detection runtime.
#[cfg(not(target_os = "macos"))]
fn detect_in(
    _picture: &deadpan_source::DecodedRgbaFrame,
    _rotation: u8,
    _selection: ProposalSelection,
) -> Result<PictureProposals, String> {
    Err("this platform has no qualified picture-proposal runtime".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_requires_request_attempt_and_token() {
        let request = RequestId::new("request-1").unwrap();
        let attempt = AttemptId::new("attempt-1").unwrap();
        let token = CancellationToken::new("token-1").unwrap();
        for (r, a, t, expected) in [
            ("request-1", "attempt-1", "token-1", true),
            ("request-2", "attempt-1", "token-1", false),
            ("request-1", "attempt-2", "token-1", false),
            ("request-1", "attempt-1", "token-2", false),
        ] {
            let message = HostMessage::Cancel {
                protocol: VERSION,
                request: RequestId::new(r).unwrap(),
                attempt: AttemptId::new(a).unwrap(),
                cancellation_token: CancellationToken::new(t).unwrap(),
            };
            assert_eq!(
                matches_cancel(&message, &request, &attempt, &token),
                expected
            );
        }
    }

    #[test]
    fn salient_geometry_is_bounded_sorted_deduplicated_and_upright() {
        let regions = displayed_salient_regions(
            [
                (0.6, 0.6, 0.2, 0.3),
                (0.1, 0.1, 0.2, 0.3),
                (0.1, 0.1, 0.2, 0.3),
                (1.2, 0.2, 0.1, 0.1),
            ],
            0,
        )
        .unwrap();
        assert_eq!(regions.len(), 2);
        assert!((regions[0].region.x() - 0.1).abs() < 1e-9);
        assert!((regions[0].region.y() - 0.6).abs() < 1e-9);
        for (turns, expected) in [
            (0, [0.1, 0.2]),
            (1, [0.5, 0.1]),
            (2, [0.7, 0.5]),
            (3, [0.2, 0.7]),
        ] {
            let found = displayed_salient_regions([(0.1, 0.5, 0.2, 0.3)], turns).unwrap()[0].region;
            assert!((found.x() - expected[0]).abs() < 1e-9);
            assert!((found.y() - expected[1]).abs() < 1e-9);
        }
        let boxes = (0..MAX_SALIENT_REGIONS).map(|i| (i as f64 / 100.0, 0.1, 0.005, 0.02));
        assert_eq!(
            displayed_salient_regions(boxes, 0).unwrap().len(),
            MAX_SALIENT_REGIONS
        );
        assert!(
            displayed_salient_regions(
                std::iter::repeat_n((0.1, 0.1, 0.2, 0.2), MAX_SALIENT_REGIONS + 1),
                0
            )
            .is_err()
        );
        assert!(displayed_salient_regions([(f64::NAN, 0.1, 0.2, 0.2)], 0).is_err());
        assert!(displayed_salient_regions([(0.1, 0.1, -0.2, 0.2)], 0).is_err());
        assert!(displayed_salient_regions([], 4).is_err());
    }

    #[test]
    fn salient_native_overflow_clips_before_rotation_without_repairing_invalid_geometry() {
        // VNRectangleObservation explicitly permits coordinates outside the
        // image. Retain its visible intersection; wholly outside is empty.
        for (native, expected) in [
            ((-0.1, 0.2, 0.3, 0.4), [0.0, 0.4, 0.2, 0.4]),
            ((0.8, 0.2, 0.3, 0.4), [0.8, 0.4, 0.2, 0.4]),
            ((0.2, -0.1, 0.4, 0.3), [0.2, 0.8, 0.4, 0.2]),
            ((0.2, 0.8, 0.4, 0.3), [0.2, 0.0, 0.4, 0.2]),
        ] {
            let found = displayed_salient_regions([native], 0).unwrap()[0].region;
            for (actual, expected) in [found.x(), found.y(), found.width(), found.height()]
                .into_iter()
                .zip(expected)
            {
                assert!((actual - expected).abs() < 1e-9, "{native:?}");
            }
        }
        for (turns, expected) in [
            (0, [0.0, 0.0]),
            (1, [0.75, 0.0]),
            (2, [0.75, 0.75]),
            (3, [0.0, 0.75]),
        ] {
            let found =
                displayed_salient_regions([(-0.25, 0.75, 0.5, 0.5)], turns).unwrap()[0].region;
            assert_eq!([found.x(), found.y()], expected);
            assert_eq!([found.width(), found.height()], [0.25, 0.25]);
            let edge = displayed_salient_regions([(0.0, 0.0, 1.0, 1.0)], turns).unwrap()[0].region;
            assert_eq!(
                [edge.x(), edge.y(), edge.width(), edge.height()],
                [0.0, 0.0, 1.0, 1.0]
            );
            for outside in [
                (-0.5, 0.1, 0.2, 0.2),
                (1.1, 0.1, 0.2, 0.2),
                (0.1, -0.5, 0.2, 0.2),
                (0.1, 1.1, 0.2, 0.2),
            ] {
                assert!(
                    displayed_salient_regions([outside], turns)
                        .unwrap()
                        .is_empty()
                );
            }
        }
        for invalid in [
            (f64::NAN, 0.1, 0.2, 0.2),
            (0.1, f64::INFINITY, 0.2, 0.2),
            (0.1, 0.1, f64::NEG_INFINITY, 0.2),
            (0.1, 0.1, 0.2, f64::NAN),
            (0.1, 0.1, 0.0, 0.2),
            (0.1, 0.1, 0.2, -0.2),
        ] {
            assert!(displayed_salient_regions([invalid], 0).is_err());
        }
    }

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
        assert!(deadpan_jobs::picture_proposals::validate_faces(&faces).is_ok());
        // A quarter turn clockwise: coded top-left becomes displayed top-right.
        let turned = displayed_faces([(0.0, 0.7, 0.2, 0.3, 0.9)], 1);
        assert!(near(turned[0].region.x(), 0.7) && near(turned[0].region.y(), 0.0));
    }
}
