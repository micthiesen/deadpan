#![cfg(target_os = "macos")]
//! Actual pinned Vision inspection of one-sided native coverage. Drawn faces
//! and a moving square prove extraction/order, not quality on real people.

use std::fs::File;
use std::io::{Cursor, Read};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_analysis::NormalizedRect;
use deadpan_analysis::generated_extension::ExtensionCoverage;
use deadpan_analysis::generated_geometry::FaceObservationSet;
use deadpan_analysis::generated_region::RegionObservation;
use deadpan_core::ExtensionDirection;
use deadpan_jobs::artifact::{ArtifactLimits, ArtifactWorkspace};
use deadpan_jobs::landmarks::{
    ExpectedStream, HostMessage, InspectionExtensionObservations, LandmarkProtocol, REGION_ENGINE,
    REGION_REQUEST_REVISION, REGION_TRACKING_LEVEL, VERSION, WORKER_ARGUMENT, WorkerMessage,
};
use deadpan_jobs::process::{ProcessEvent, ProcessLimits, ProcessSpec, SupervisedProcess};
use deadpan_jobs::{
    AttemptId, CancellationToken, RequestId, Sha256, WorkspaceArtifact, WorkspaceRef,
};
use deadpan_source::{DecodeControl, DecodeLimits, SourceDecoder};
use sha2::Digest;

const OUTPUT_BYTES: u64 = 4 * 1024 * 1024;
const DIRECTIONS: [ExtensionDirection; 2] =
    [ExtensionDirection::FromLeft, ExtensionDirection::FromRight];

fn artifact(path: &str, bytes: &[u8]) -> WorkspaceArtifact {
    WorkspaceArtifact::new(
        WorkspaceRef::new(path).unwrap(),
        Sha256::new(
            sha2::Sha256::digest(bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
        )
        .unwrap(),
        bytes.len() as u64,
    )
    .unwrap()
}

struct Fixture {
    workspace: tempfile::TempDir,
    request: HostMessage,
}

impl Fixture {
    fn new(direction: ExtensionDirection, square: bool) -> Self {
        let workspace = tempfile::tempdir().unwrap();
        std::fs::create_dir(workspace.path().join("input")).unwrap();
        std::fs::create_dir(workspace.path().join("output")).unwrap();
        let source: &[u8] = if square {
            include_bytes!("fixtures/moving-square-cut.mkv")
        } else {
            include_bytes!("fixtures/two-drawn-faces.mkv")
        };
        let path = workspace.path().join("input/source.mkv");
        std::fs::write(&path, source).unwrap();
        let (width, height) = if square { (320, 180) } else { (480, 270) };
        let (start, end, anchor_ordinal) = match (square, direction) {
            // The retained left anchor begins Shot B. Worker-native context
            // ends in the occluded Shot A; using it would track the wrong box.
            (true, ExtensionDirection::FromLeft) => (30, 48, 30),
            // Starting with frame11 and travelling 11→0 keeps the square
            // adjacent. Chronological tracking would begin with a55px jump.
            (true, ExtensionDirection::FromRight) => (0, 12, 11),
            (false, ExtensionDirection::FromLeft) => (2, 4, 0),
            (false, ExtensionDirection::FromRight) => (0, 2, 0),
        };
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
        let mut pts = Vec::new();
        let mut png = None;
        while let Some(metadata) = decoder.next_metadata(control()).unwrap() {
            let ordinal = pts.len();
            pts.push(metadata.pts);
            if ordinal != anchor_ordinal {
                continue;
            }
            let frame = decoder.copy_current_rgba(control()).unwrap();
            let mut rgb = Vec::with_capacity((width * height * 3) as usize);
            for row in frame
                .rgba
                .chunks(frame.row_stride_bytes)
                .take(height as usize)
            {
                for pixel in row[..width as usize * 4].chunks_exact(4) {
                    rgb.extend_from_slice(&pixel[..3]);
                }
            }
            let image = image::RgbImage::from_raw(width, height, rgb).unwrap();
            let mut bytes = Cursor::new(Vec::new());
            image::DynamicImage::ImageRgb8(image)
                .write_to(&mut bytes, image::ImageFormat::Png)
                .unwrap();
            png = Some(bytes.into_inner());
        }
        let png = png.unwrap();
        std::fs::write(workspace.path().join("input/anchor.png"), &png).unwrap();
        let region_seed = square.then(|| {
            let (x, y) = square_origin(anchor_ordinal as u32);
            NormalizedRect::new(x / 320.0, y / 180.0, 36.0 / 320.0, 36.0 / 180.0).unwrap()
        });
        Self {
            workspace,
            request: HostMessage::InspectExtensionLandmarks {
                protocol: VERSION,
                request: RequestId::new("extension-landmarks").unwrap(),
                attempt: AttemptId::new("attempt-1").unwrap(),
                cancellation_token: CancellationToken::new("cancel-1").unwrap(),
                source: artifact("input/source.mkv", source),
                stream: ExpectedStream {
                    stream_index: 0,
                    width,
                    height,
                    clean_aperture: None,
                    time_base_num: 1,
                    time_base_den: 1000,
                    rotation_quarter_turns: 0,
                },
                picture_pts: pts,
                anchor: artifact("input/anchor.png", &png),
                coverage: ExtensionCoverage {
                    direction,
                    start,
                    end,
                },
                region_seed,
                output_scope: WorkspaceRef::new("output").unwrap(),
                maximum_output_bytes: OUTPUT_BYTES,
                timeout_millis: 120_000,
            },
        }
    }

    fn run(&self, cancel: bool) -> Result<InspectionExtensionObservations, String> {
        let pinned =
            ArtifactWorkspace::open(self.workspace.path()).map_err(|error| error.to_string())?;
        let HostMessage::InspectExtensionLandmarks {
            timeout_millis,
            coverage,
            picture_pts,
            region_seed,
            ..
        } = &self.request
        else {
            unreachable!()
        };
        let maximum_duration = Duration::from_millis(*timeout_millis).max(Duration::from_secs(2));
        let mut process = SupervisedProcess::<LandmarkProtocol>::spawn(
            ProcessSpec {
                executable: PathBuf::from(env!("CARGO_BIN_EXE_deadpan-track")),
                arguments: vec![WORKER_ARGUMENT.into()],
                environment: Default::default(),
                workspace: self.workspace.path().to_owned(),
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
                return Err("extension test worker exceeded deadline".into());
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
                            ..
                        } => {
                            runtime.validate()?;
                            assert_eq!(decoded as usize, picture_pts.len());
                            assert_eq!(analysed, coverage.end - coverage.start + 1);
                            assert_eq!(region_runtime.is_some(), region_seed.is_some());
                            if let Some(runtime) = region_runtime {
                                assert_eq!(runtime.engine, REGION_ENGINE);
                                assert_eq!(runtime.request_revision, REGION_REQUEST_REVISION);
                                assert_eq!(runtime.tracking_level, REGION_TRACKING_LEVEL);
                            }
                            completion = Some(observations);
                        }
                        WorkerMessage::Failed { diagnostic, .. } => {
                            failure = Some(diagnostic.as_str().to_owned());
                        }
                        WorkerMessage::Cancelled { .. } => {
                            failure = Some("cancelled".into());
                        }
                        WorkerMessage::Progress { percent, .. } => {
                            assert!(percent >= last_percent);
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
        if let Some(error) = failure {
            return Err(error);
        }
        let declaration = completion.ok_or("no clean extension inspection completion")?;
        let mut snapshot = pinned
            .snapshot(
                &WorkspaceRef::new("output").unwrap(),
                &declaration,
                ArtifactLimits::new(OUTPUT_BYTES).unwrap(),
            )
            .map_err(|error| error.to_string())?;
        let mut bytes = Vec::new();
        snapshot
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        let batch: InspectionExtensionObservations =
            serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        batch.validate(picture_pts, coverage, region_seed.as_ref())?;
        Ok(batch)
    }
}

fn square_origin(ordinal: u32) -> (f64, f64) {
    if ordinal < 30 {
        // Python's fixture generator uses ties-to-even integer rounding.
        (
            40.0 + 5.0 * f64::from(ordinal),
            (40.0 + 1.5 * f64::from(ordinal)).round_ties_even(),
        )
    } else {
        (
            230.0 - 4.0 * f64::from(ordinal - 30),
            110.0 - f64::from(ordinal - 30),
        )
    }
}

#[test]
fn extension_vision_tracks_outward_from_the_actual_anchor_and_returns_canonical_order() {
    for direction in DIRECTIONS {
        let fixture = Fixture::new(direction, true);
        let batch = fixture.run(false).unwrap();
        let region = batch.region.unwrap();
        assert!(matches!(region.anchor, RegionObservation::Tracked { .. }));
        assert_eq!(region.frames.len(), batch.landmarks.frames.len());
        for (ordinal, frame) in region.coverage.ordinals().zip(&region.frames) {
            assert_eq!(frame.ordinal, ordinal);
            let RegionObservation::Tracked { region, confidence } = frame.observation else {
                panic!(
                    "{direction:?} picture {ordinal} unavailable: {:?}",
                    frame.observation
                );
            };
            let (x, y) = region.center();
            let (left, top) = square_origin(ordinal);
            let error = (x * 320.0 - left - 18.0).hypot(y * 180.0 - top - 18.0);
            eprintln!(
                "{direction:?} picture {ordinal}: confidence {confidence}, center error {error:.2}px"
            );
            assert!(error < 8.0, "{direction:?} picture {ordinal}: {error}px");
        }
    }
}

#[test]
fn extension_face_observations_exclude_context_and_retain_one_actual_anchor() {
    for direction in DIRECTIONS {
        let batch = Fixture::new(direction, false).run(false).unwrap();
        assert!(batch.region.is_none());
        assert!(
            matches!(batch.landmarks.anchor, FaceObservationSet::Detected { ref faces } if faces.len() == 2)
        );
        for frame in &batch.landmarks.frames {
            let FaceObservationSet::Detected { faces } = &frame.observation else {
                panic!("missing face observation");
            };
            assert_eq!(
                faces.len(),
                if direction == ExtensionDirection::FromRight {
                    2
                } else {
                    0
                }
            );
        }
        assert_eq!(batch.landmarks.frames.len(), 2);
    }
}

#[test]
fn extension_worker_proves_unanalysed_context_pts_terminal_and_anchor_bytes() {
    for mode in 0..3 {
        let mut fixture = Fixture::new(ExtensionDirection::FromLeft, false);
        if let HostMessage::InspectExtensionLandmarks {
            picture_pts,
            coverage,
            anchor,
            ..
        } = &mut fixture.request
        {
            match mode {
                0 => picture_pts[1] = 43, // context, outside generated2..4
                1 => {
                    picture_pts.pop();
                    coverage.end -= 1;
                }
                _ => {
                    *anchor = WorkspaceArtifact::new(
                        anchor.reference().clone(),
                        Sha256::new("0".repeat(64)).unwrap(),
                        anchor.byte_length(),
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
        assert!(
            !fixture
                .workspace
                .path()
                .join("output/landmarks.json")
                .exists()
        );
    }
}

#[test]
fn extension_worker_cancellation_and_deadline_never_publish_a_completion() {
    let fixture = Fixture::new(ExtensionDirection::FromRight, true);
    assert!(fixture.run(true).is_err());
    let mut fixture = Fixture::new(ExtensionDirection::FromRight, true);
    if let HostMessage::InspectExtensionLandmarks { timeout_millis, .. } = &mut fixture.request {
        *timeout_millis = 1;
    }
    assert!(fixture.run(false).unwrap_err().contains("deadline"));
    assert!(
        !fixture
            .workspace
            .path()
            .join("output/landmarks.json")
            .exists()
    );
}
