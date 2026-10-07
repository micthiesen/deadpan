#![cfg(target_os = "macos")]
//! The real pinned Vision landmark request over the complete drawn fixture.
//! This qualifies extraction and worker containment, not real-person quality.

use std::fs::File;
use std::io::{Cursor, Read};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_analysis::NormalizedRect;
use deadpan_analysis::generated_geometry::{
    FaceObservationSet, LandmarkAvailability, LandmarkRegion,
};
use deadpan_analysis::generated_region::{RegionObservation, RegionSeeds};
use deadpan_jobs::artifact::{ArtifactLimits, ArtifactWorkspace};
use deadpan_jobs::landmarks::{
    BoundaryInputs, CONSTELLATION, ENGINE, ExpectedStream, HostMessage, InspectionObservations,
    LandmarkProtocol, REGION_ENGINE, REGION_REQUEST_REVISION, REGION_TRACKING_LEVEL,
    REQUEST_REVISION, RegionRuntimeReport, RuntimeReport, VERSION, WORKER_ARGUMENT, WorkerMessage,
};
use deadpan_jobs::process::{ProcessEvent, ProcessLimits, ProcessSpec, SupervisedProcess};
use deadpan_jobs::protocol::{
    AttemptId, CancellationToken, RequestId, Sha256, WorkspaceArtifact, WorkspaceRef,
};
use deadpan_source::{DecodeControl, DecodeLimits, SourceDecoder};
use sha2::Digest;

const PTS: [i64; 4] = [0, 42, 83, 125];
const OUTPUT_BYTES: u64 = 4 * 1024 * 1024;

struct Fixture {
    workspace: tempfile::TempDir,
    request: HostMessage,
}

fn artifact(path: &str, bytes: &[u8]) -> WorkspaceArtifact {
    let hash = sha2::Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    WorkspaceArtifact::new(
        WorkspaceRef::new(path).unwrap(),
        Sha256::new(hash).unwrap(),
        bytes.len() as u64,
    )
    .unwrap()
}

impl Fixture {
    fn new(with_boundaries: bool) -> Self {
        let workspace = tempfile::tempdir().unwrap();
        std::fs::create_dir(workspace.path().join("input")).unwrap();
        std::fs::create_dir(workspace.path().join("output")).unwrap();
        let source = include_bytes!("fixtures/two-drawn-faces.mkv");
        let path = workspace.path().join("input/source.mkv");
        std::fs::write(&path, source).unwrap();
        let boundaries = if with_boundaries {
            // Fixture-only PNG creation. Production supplies the actual retained
            // PNG bytes unchanged. This PNG is the decoder's exact first picture.
            let cancelled = AtomicBool::new(false);
            let control = || DecodeControl {
                timeout: Duration::from_secs(30),
                cancelled: &cancelled,
            };
            let mut decoder = SourceDecoder::open(
                File::open(&path).unwrap(),
                DecodeLimits::default(),
                control(),
            )
            .unwrap();
            assert_eq!(decoder.next_metadata(control()).unwrap().unwrap().pts, 0);
            let frame = decoder.copy_current_rgba(control()).unwrap();
            let mut rgb = Vec::with_capacity((frame.width * frame.height * 3) as usize);
            for row in frame
                .rgba
                .chunks(frame.row_stride_bytes)
                .take(frame.height as usize)
            {
                for pixel in row[..frame.width as usize * 4].chunks_exact(4) {
                    rgb.extend_from_slice(&pixel[..3]);
                }
            }
            let image = image::RgbImage::from_raw(frame.width, frame.height, rgb).unwrap();
            let mut png = Cursor::new(Vec::new());
            image::DynamicImage::ImageRgb8(image)
                .write_to(&mut png, image::ImageFormat::Png)
                .unwrap();
            let png = png.into_inner();
            std::fs::write(workspace.path().join("input/boundary.png"), &png).unwrap();
            let boundary = artifact("input/boundary.png", &png);
            Some(Box::new(BoundaryInputs {
                left: boundary.clone(),
                right: boundary,
            }))
        } else {
            None
        };
        let request = HostMessage::InspectLandmarks {
            protocol: VERSION,
            request: RequestId::new("landmark-fixture").unwrap(),
            attempt: AttemptId::new("attempt-1").unwrap(),
            cancellation_token: CancellationToken::new("cancel-1").unwrap(),
            source: artifact("input/source.mkv", source),
            stream: ExpectedStream {
                stream_index: 0,
                width: 480,
                height: 270,
                time_base_num: 1,
                time_base_den: 1000,
                rotation_quarter_turns: 0,
            },
            picture_pts: PTS.to_vec(),
            boundaries,
            region_seeds: None,
            output_scope: WorkspaceRef::new("output").unwrap(),
            maximum_output_bytes: OUTPUT_BYTES,
            timeout_millis: 120_000,
        };
        Self { workspace, request }
    }

    fn square() -> Self {
        let mut fixture = Self::new(false);
        let source = include_bytes!("fixtures/moving-square-cut.mkv");
        let path = fixture.workspace.path().join("input/source.mkv");
        std::fs::write(&path, source).unwrap();
        let cancelled = AtomicBool::new(false);
        let control = || DecodeControl {
            timeout: Duration::from_secs(30),
            cancelled: &cancelled,
        };
        let mut decoder = SourceDecoder::open(
            File::open(&path).unwrap(),
            DecodeLimits::default(),
            control(),
        )
        .unwrap();
        let mut points = Vec::new();
        let mut first = None;
        let mut last = None;
        while let Some(metadata) = decoder.next_metadata(control()).unwrap() {
            points.push(metadata.pts);
            let frame = decoder.copy_current_rgba(control()).unwrap();
            let mut rgb = Vec::with_capacity((frame.width * frame.height * 3) as usize);
            for row in frame
                .rgba
                .chunks(frame.row_stride_bytes)
                .take(frame.height as usize)
            {
                for pixel in row[..frame.width as usize * 4].chunks_exact(4) {
                    rgb.extend_from_slice(&pixel[..3]);
                }
            }
            let image = image::RgbImage::from_raw(frame.width, frame.height, rgb).unwrap();
            if first.is_none() {
                first = Some(image.clone());
            }
            last = Some(image);
        }
        let mut endpoints = Vec::new();
        for (name, image) in [("left", first.unwrap()), ("right", last.unwrap())] {
            let mut png = Cursor::new(Vec::new());
            image::DynamicImage::ImageRgb8(image)
                .write_to(&mut png, image::ImageFormat::Png)
                .unwrap();
            let bytes = png.into_inner();
            let reference = format!("input/{name}.png");
            std::fs::write(fixture.workspace.path().join(&reference), &bytes).unwrap();
            endpoints.push(artifact(&reference, &bytes));
        }
        let HostMessage::InspectLandmarks {
            source: artifact_source,
            stream,
            picture_pts,
            boundaries,
            region_seeds,
            ..
        } = &mut fixture.request
        else {
            unreachable!()
        };
        *artifact_source = artifact("input/source.mkv", source);
        stream.width = 320;
        stream.height = 180;
        *picture_pts = points;
        *boundaries = Some(Box::new(BoundaryInputs {
            left: endpoints.remove(0),
            right: endpoints.remove(0),
        }));
        let left =
            NormalizedRect::new(40.0 / 320.0, 40.0 / 180.0, 36.0 / 320.0, 36.0 / 180.0).unwrap();
        // The right authored box is deliberately independent of the actual
        // tracked result; worker evidence must retain both unchanged.
        *region_seeds = Some(Box::new(RegionSeeds { left, right: left }));
        fixture
    }

    fn run(
        &self,
        cancel: bool,
    ) -> Result<
        (
            InspectionObservations,
            RuntimeReport,
            Option<RegionRuntimeReport>,
        ),
        String,
    > {
        let pinned =
            ArtifactWorkspace::open(self.workspace.path()).map_err(|error| error.to_string())?;
        let HostMessage::InspectLandmarks { timeout_millis, .. } = &self.request else {
            unreachable!()
        };
        // Leave enough headroom for a worker-originated deadline diagnostic,
        // while still bounding a blocked descriptor open in the regression.
        let maximum_duration = Duration::from_millis(*timeout_millis).max(Duration::from_secs(2));
        let mut process = SupervisedProcess::<LandmarkProtocol>::spawn(
            ProcessSpec {
                executable: PathBuf::from(env!("CARGO_BIN_EXE_deadpan-track")),
                arguments: vec![WORKER_ARGUMENT.into()],
                environment: Default::default(),
                workspace: self.workspace.path().to_path_buf(),
                limits: ProcessLimits {
                    maximum_duration,
                    cancellation_grace: Duration::from_secs(2).min(maximum_duration),
                    exit_grace: Duration::from_secs(5).min(maximum_duration),
                },
            },
            self.request.clone(),
        )
        .map_err(|error| error.to_string())?;
        if cancel {
            process
                .request_cancel(Instant::now())
                .map_err(|error| error.to_string())?;
        }
        let deadline = Instant::now() + maximum_duration + Duration::from_secs(5);
        let mut completion = None;
        let mut failure = None;
        let mut last_percent = 0;
        while !process.is_finished() {
            if Instant::now() >= deadline {
                process
                    .finish_owned_work(Instant::now() + Duration::from_secs(5))
                    .map_err(|error| error.to_string())?;
                return Err("test worker exceeded deadline".into());
            }
            for event in process
                .poll(Instant::now())
                .map_err(|error| error.to_string())?
            {
                match event {
                    ProcessEvent::Message(message) => match *message {
                        WorkerMessage::Completed {
                            observations,
                            runtime,
                            region_runtime,
                            decoded,
                            analysed,
                            decode_millis,
                            vision_millis,
                            elapsed_millis,
                            ..
                        } => {
                            eprintln!(
                                "landmark worker: decoded {decoded}, analysed {analysed}, decode {decode_millis}ms, Vision {vision_millis}ms, total {elapsed_millis}ms"
                            );
                            completion = Some((observations, runtime, region_runtime));
                        }
                        WorkerMessage::Failed { diagnostic, .. } => {
                            failure = Some(diagnostic.as_str().to_owned());
                        }
                        WorkerMessage::Cancelled { .. } => {
                            failure = Some("cancelled".into());
                        }
                        WorkerMessage::Progress { percent, .. } => {
                            if percent < last_percent {
                                failure = Some("progress moved backward".into());
                            }
                            last_percent = percent;
                        }
                    },
                    ProcessEvent::Fault(reason) => {
                        failure.get_or_insert(reason);
                    }
                    ProcessEvent::Exited {
                        status,
                        cancellation_escalated,
                    } => {
                        if (!status.success() || cancellation_escalated) && failure.is_none() {
                            failure = Some(format!(
                                "worker exit {status}, escalated {cancellation_escalated}"
                            ));
                        }
                    }
                }
            }
            if !process.is_finished() {
                std::thread::park_timeout(Duration::from_millis(5));
            }
        }
        if let Some(failure) = failure {
            return Err(failure);
        }
        let (artifact, runtime, region_runtime) =
            completion.ok_or("no clean landmark completion")?;
        let mut snapshot = pinned
            .snapshot(
                &WorkspaceRef::new("output").unwrap(),
                &artifact,
                ArtifactLimits::new(OUTPUT_BYTES).unwrap(),
            )
            .map_err(|error| error.to_string())?;
        let mut bytes = Vec::new();
        snapshot
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        let batch: InspectionObservations =
            serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        let HostMessage::InspectLandmarks {
            picture_pts,
            region_seeds,
            ..
        } = &self.request
        else {
            unreachable!()
        };
        batch.validate(picture_pts, region_seeds.as_deref())?;
        Ok((batch, runtime, region_runtime))
    }
}

#[test]
fn vision_landmarks_cover_every_picture_and_optional_retained_pngs() {
    let fixture = Fixture::new(true);
    let (batch, runtime, region_runtime) = fixture.run(false).unwrap();
    assert!(batch.region.is_none());
    assert!(region_runtime.is_none());
    let batch = batch.landmarks;
    assert_eq!(runtime.engine, ENGINE);
    assert_eq!(runtime.request_revision, REQUEST_REVISION);
    assert_eq!(runtime.constellation, CONSTELLATION);
    for (ordinal, frame) in batch.frames.iter().enumerate() {
        let FaceObservationSet::Detected { faces } = &frame.observation else {
            panic!("frame {ordinal} unavailable: {:?}", frame.observation);
        };
        assert_eq!(faces.len(), if ordinal < 2 { 2 } else { 0 });
        let with_landmarks = faces
            .iter()
            .filter(|face| matches!(&face.landmarks, LandmarkAvailability::Available { .. }))
            .count();
        eprintln!(
            "picture {ordinal} PTS {}: {} faces, {with_landmarks} landmark observations",
            frame.pts,
            faces.len()
        );
        for (number, face) in faces.iter().enumerate() {
            let LandmarkAvailability::Available {
                confidence,
                left_eye,
                right_eye,
                nose,
                outer_lips,
                inner_lips,
            } = &face.landmarks
            else {
                panic!("picture {ordinal}, face {number} lost measured landmark availability");
            };
            let count = |region: &LandmarkRegion| match region {
                LandmarkRegion::Detected { points } => points.len(),
                LandmarkRegion::Unavailable { .. } => 0,
            };
            for (name, region, minimum) in [
                ("left eye", left_eye, 2),
                ("right eye", right_eye, 2),
                ("nose", nose, 1),
                ("outer lips", outer_lips, 2),
                ("inner lips", inner_lips, 2),
            ] {
                assert!(
                    count(region) >= minimum,
                    "picture {ordinal}, face {number}: missing measured {name} landmarks"
                );
            }
            eprintln!(
                "face {number}: confidence {}, landmarks {confidence}; point counts left eye {}, right eye {}, nose {}, outer lips {}, inner lips {}",
                face.confidence,
                count(left_eye),
                count(right_eye),
                count(nose),
                count(outer_lips),
                count(inner_lips)
            );
        }
        // The measured drawn fixture establishes positive bounded landmark
        // extraction. It does not establish reliability on real people.
        frame.observation.validate().unwrap();
    }
    let boundaries = batch.boundaries.unwrap();
    assert_eq!(boundaries.left, boundaries.right);
    assert!(
        matches!(boundaries.left, FaceObservationSet::Detected { ref faces } if faces.len() == 2)
    );
}

#[test]
fn mismatched_pts_extra_pictures_and_source_hash_fail_the_inspection() {
    for mode in 0..3 {
        let mut fixture = Fixture::new(false);
        if let HostMessage::InspectLandmarks {
            source,
            picture_pts,
            ..
        } = &mut fixture.request
        {
            match mode {
                0 => picture_pts[1] = 43,
                1 => {
                    picture_pts.pop();
                }
                _ => {
                    *source = WorkspaceArtifact::new(
                        source.reference().clone(),
                        Sha256::new("0".repeat(64)).unwrap(),
                        source.byte_length(),
                    )
                    .unwrap()
                }
            }
        }
        let error = fixture.run(false).unwrap_err();
        assert!(
            error.contains(match mode {
                0 => "expected 43",
                1 => "beyond the requested sequence",
                _ => "hash differs",
            }),
            "{error}"
        );
    }
}

#[test]
fn malformed_boundary_png_and_worker_deadline_do_not_produce_observations() {
    let mut fixture = Fixture::new(true);
    let bytes = b"not a PNG";
    std::fs::write(fixture.workspace.path().join("input/boundary.png"), bytes).unwrap();
    if let HostMessage::InspectLandmarks { boundaries, .. } = &mut fixture.request {
        let boundary = artifact("input/boundary.png", bytes);
        *boundaries = Some(Box::new(BoundaryInputs {
            left: boundary.clone(),
            right: boundary,
        }));
    }
    assert!(fixture.run(false).is_err());
    assert!(
        !fixture
            .workspace
            .path()
            .join("output/landmarks.json")
            .exists()
    );

    let mut fixture = Fixture::new(false);
    if let HostMessage::InspectLandmarks { timeout_millis, .. } = &mut fixture.request {
        *timeout_millis = 1;
    }
    let error = fixture.run(false).unwrap_err();
    assert!(error.contains("deadline"), "{error}");
    assert!(
        !fixture
            .workspace
            .path()
            .join("output/landmarks.json")
            .exists()
    );
}

#[test]
fn cancellation_stops_the_owned_worker_without_a_completion() {
    let fixture = Fixture::new(false);
    assert!(fixture.run(true).is_err());
}

#[test]
fn source_links_and_fifos_fail_without_waiting_for_a_writer() {
    for fifo in [false, true] {
        let mut fixture = Fixture::new(false);
        let path = fixture.workspace.path().join("input/source.mkv");
        let saved = fixture.workspace.path().join("retained.mkv");
        std::fs::rename(&path, &saved).unwrap();
        if fifo {
            let mut command = std::process::Command::new("mkfifo");
            command.arg(&path);
            assert!(
                deadpan_native_process::spawn(&mut command)
                    .unwrap()
                    .wait()
                    .unwrap()
                    .success()
            );
        } else {
            std::os::unix::fs::symlink(&saved, &path).unwrap();
        }
        if let HostMessage::InspectLandmarks { timeout_millis, .. } = &mut fixture.request {
            *timeout_millis = 1_000;
        }
        let error = fixture.run(false).unwrap_err();
        assert!(
            error.contains(if fifo {
                "not a regular file"
            } else {
                "open source"
            }),
            "{error}"
        );
    }
}

#[test]
fn pinned_region_tracker_follows_the_square_with_complete_native_and_endpoint_evidence() {
    let fixture = Fixture::square();
    let (batch, _, runtime) = fixture.run(false).unwrap();
    let runtime = runtime.expect("requested region runtime");
    assert_eq!(runtime.engine, REGION_ENGINE);
    assert_eq!(runtime.request_revision, REGION_REQUEST_REVISION);
    assert_eq!(runtime.tracking_level, REGION_TRACKING_LEVEL);
    let region = batch.region.expect("requested region observations");
    assert!(matches!(region.left, RegionObservation::Tracked { .. }));
    assert_eq!(region.frames.len(), batch.landmarks.frames.len());
    // The existing fixture's first twelve pictures show a clear moving
    // square before occlusion and a later hard cut. Measure positive tracking
    // there; retain every later observation, including losses, without repair.
    for (ordinal, frame) in region.frames.iter().take(12).enumerate() {
        let RegionObservation::Tracked { region, confidence } = frame.observation else {
            panic!(
                "unoccluded picture {ordinal} unavailable: {:?}",
                frame.observation
            );
        };
        let (x, y) = region.center();
        let expected_x = (40.0 + 5.0 * ordinal as f64).round() + 18.0;
        let expected_y = (40.0 + 1.5 * ordinal as f64).round() + 18.0;
        let error = (x * 320.0 - expected_x).hypot(y * 180.0 - expected_y);
        eprintln!("region picture {ordinal}: confidence {confidence}, center error {error:.2}px");
        assert!(error < 8.0, "picture {ordinal}: {error}px");
    }
    // Detection confidence is measured evidence, not promised by the fixture.
    // Its subject moves far from the stationary authored seeds before losing
    // confidence. The host policy must preserve that measured rejection.
    let expected_pts: Vec<_> = region.frames.iter().map(|frame| frame.pts).collect();
    let assessment = deadpan_analysis::generated_region::analyze(
        &region,
        &expected_pts,
        [320, 180],
        [0, 0, 320, 180],
    )
    .unwrap();
    assert!(assessment.measured_frames >= 2, "{assessment:?}");
    assert!(assessment.rejection.is_some(), "{assessment:?}");
    assert!(assessment.unavailable_frames > 0, "{assessment:?}");
    eprintln!("real region policy: {assessment:?}");
    eprintln!(
        "region left {:?}, right {:?}, {} native observations",
        region.left,
        region.right,
        region.frames.len(),
    );
    // A lost request cannot silently reacquire after the cut or at the right
    // endpoint. This assertion depends on observed loss, never invents one.
    if let Some(first_lost) = region
        .frames
        .iter()
        .position(|frame| matches!(frame.observation, RegionObservation::Unavailable { .. }))
    {
        assert!(
            region.frames[first_lost..]
                .iter()
                .all(|frame| matches!(frame.observation, RegionObservation::Unavailable { .. }))
        );
        assert!(matches!(
            region.right,
            RegionObservation::Unavailable { .. }
        ));
    }
}
