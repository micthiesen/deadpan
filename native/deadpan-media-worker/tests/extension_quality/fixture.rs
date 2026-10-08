use std::fs;
use std::io::{Cursor, Read, Seek};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_core::{
    ExactRatio, ExtensionDirection, FrameDuration, FrameRate, NodeId, ProjectId, RevisionId,
};
use deadpan_jobs::artifact::ArtifactWorkspace;
use deadpan_jobs::{
    AttemptId, AxisLimits, CancellationToken, ConditioningMode, ContextArtifact, DimensionLimits,
    ExtensionCapability, ExtensionCapturePolicy, ExtensionGenerationPlan, FrameCountFormula,
    GenerationCaptureSpec, GenerationInputBinding, GenerationInputSupport, GenerationInputs,
    GenerationPictureIdentity, HoldConstraints, HoldTarget, HostMessage, MessageIdentity,
    MotionAmount, NativeDimensions, ProtocolVersion, ProviderPackId, ProviderPackVersion,
    ProviderSelection, RelativeGenerationPicture, RequestId, RequestVersion, RuntimeId,
    RuntimeVersion, Sha256, VideoSpec, WorkspaceArtifact, WorkspaceRef,
};
use deadpan_media::protocol::{
    ConversionLimits, EXTENSION_PROTOCOL_VERSION, ExtensionConversionRequest, ExtensionOperation,
    VideoContract,
};
use deadpan_media::{CanonicalExtension, InputIdentity, canonicalize_extension};
use deadpan_models::{
    BoundaryClock, BoundaryPicture, ConditioningLimits, ExtensionContext, ExtensionContextPicture,
    ExtensionContinuityEvidence, ExtensionOppositeSeam, ExtensionRegionCapture, RasterRect,
    RetainedExtensionConditioning, capture_extension_conditioning,
};
use image::ImageEncoder;
use rustix::process::{Pid, WaitId, WaitIdOptions, waitid};
use sha2::Digest;

pub const WIDTH: u32 = 192;
pub const HEIGHT: u32 = 108;
pub const CONTEXT: u32 = 9;
pub const GENERATED: u32 = 8;

pub struct Fixture {
    pub directory: tempfile::TempDir,
    pub request: HostMessage,
    pub conditioning: RetainedExtensionConditioning,
    pub media: CanonicalExtension,
    pub native: Vec<u8>,
}

pub fn plan(direction: ExtensionDirection, output: u32) -> ExtensionGenerationPlan {
    plan_with_dimensions(direction, output, WIDTH, HEIGHT)
}

pub fn plan_with_dimensions(
    direction: ExtensionDirection,
    output: u32,
    width: u32,
    height: u32,
) -> ExtensionGenerationPlan {
    ExtensionGenerationPlan::new(
        direction,
        FrameDuration::new(i64::from(output)).unwrap(),
        FrameRate::new(24, 1).unwrap(),
        &ExtensionCapability::new(
            FrameRate::new(24, 1).unwrap(),
            CONTEXT,
            FrameCountFormula::new(8, 0, GENERATED, GENERATED).unwrap(),
            DimensionLimits::new(
                AxisLimits::new(width, width, 1).unwrap(),
                AxisLimits::new(height, height, 1).unwrap(),
            ),
            FrameDuration::new(8).unwrap(),
        )
        .unwrap(),
        NativeDimensions::new(width, height).unwrap(),
    )
    .unwrap()
}

fn declaration(reference: &str, bytes: &[u8]) -> WorkspaceArtifact {
    WorkspaceArtifact::new(
        WorkspaceRef::new(reference).unwrap(),
        Sha256::new(
            sha2::Sha256::digest(bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
        )
        .unwrap(),
        u64::try_from(bytes.len()).unwrap(),
    )
    .unwrap()
}

fn picture(position: i64) -> BoundaryPicture {
    BoundaryPicture::AuthoredBlack {
        clock: BoundaryClock::Definition {
            project_id: ProjectId::new("extension-quality-project").unwrap(),
            revision_id: RevisionId::new("extension-quality-revision").unwrap(),
            definition: NodeId::new("extension-quality-definition").unwrap(),
            position: ExactRatio::integer(position),
        },
    }
}

pub fn retained_inputs(
    directory: &Path,
    plan: &ExtensionGenerationPlan,
    opposite_present: bool,
) -> (HostMessage, RetainedExtensionConditioning) {
    let dimensions = plan.native_dimensions();
    let (width, height) = (dimensions.width(), dimensions.height());
    fs::create_dir(directory.join("inputs")).unwrap();
    fs::create_dir(directory.join("outputs")).unwrap();
    // AuthoredBlack is an actual black picture, not a convenient identity for
    // arbitrary pixels. Worker output can differ; retained inputs cannot.
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(
            &vec![0; (width * height * 3) as usize],
            width,
            height,
            image::ExtendedColorType::Rgb8,
        )
        .unwrap();
    fs::write(directory.join("inputs/black.png"), &png).unwrap();
    let black = declaration("inputs/black.png", &png);
    let context = (0..CONTEXT)
        .map(|ordinal| ExtensionContextPicture {
            picture: picture(20 + i64::from(ordinal)),
            frame: black.clone(),
            content: None,
        })
        .collect();
    let anchor = match plan.direction() {
        ExtensionDirection::FromLeft => 28,
        ExtensionDirection::FromRight => 20,
    };
    let opposite_position = match plan.direction() {
        ExtensionDirection::FromLeft => plan.project_frames().frames() + 1,
        ExtensionDirection::FromRight => -plan.project_frames().frames() - 1,
    };
    let opposite = if opposite_present {
        ExtensionOppositeSeam::PresentUnconditioned {
            picture: Box::new(picture(anchor + opposite_position)),
            frame: black,
            content: None,
        }
    } else {
        ExtensionOppositeSeam::Absent
    };
    let samples: Vec<_> = (0..CONTEXT)
        .map(|ordinal| RelativeGenerationPicture {
            position: ExactRatio::integer(20 + i64::from(ordinal) - anchor),
            picture: GenerationPictureIdentity::AuthoredBlack,
        })
        .collect();
    let terminal = samples.last().unwrap().clone();
    let support = vec![GenerationInputSupport {
        start: samples[0].position,
        end_exclusive: terminal.position,
        first: GenerationPictureIdentity::AuthoredBlack,
        last: GenerationPictureIdentity::AuthoredBlack,
    }];
    // Empty deadpan-context-signatures-1 stream: every physical interval is
    // authored black, so no decoded-media signatures or counts are invented.
    let signatures = b"DPSIG001\0\0\0\0";
    fs::write(directory.join("inputs/continuity.bin"), signatures).unwrap();
    let evidence = ExtensionContinuityEvidence::new(
        GenerationInputBinding {
            duration: plan.project_frames(),
            frame_rate: plan.project_frame_rate(),
            canvas: [width, height],
            inputs: GenerationInputs::Extension {
                capture: GenerationCaptureSpec::Extension {
                    direction: plan.direction(),
                    native_rate: plan.native_frame_rate(),
                    context_frames: CONTEXT,
                    policy: ExtensionCapturePolicy::TemporalContextV1,
                },
                samples,
                opposite: opposite_present.then(|| RelativeGenerationPicture {
                    position: ExactRatio::integer(opposite_position),
                    picture: GenerationPictureIdentity::AuthoredBlack,
                }),
                support,
                terminal,
            },
            region: None,
        },
        vec![],
        vec![None, None],
        declaration("inputs/continuity.bin", signatures),
    )
    .unwrap();
    let context = ExtensionContext::new(
        plan.clone(),
        context,
        RasterRect::new(0, 0, width, height).unwrap(),
        opposite,
        "host authored black encoded as RGB8 sRGB PNG",
        ExtensionRegionCapture::None,
        evidence,
    )
    .unwrap();
    let bytes = serde_json::to_vec(&context).unwrap();
    fs::write(directory.join("inputs/context.json"), &bytes).unwrap();
    let manifest = declaration("inputs/context.json", &bytes);
    let request = HostMessage::GenerateExtension {
        protocol: ProtocolVersion::V3,
        identity: MessageIdentity::new(
            RequestId::new("extension-quality-request").unwrap(),
            AttemptId::new("extension-quality-attempt").unwrap(),
        ),
        cancellation_token: CancellationToken::new("extension-quality-cancel").unwrap(),
        project_id: ProjectId::new("extension-quality-project").unwrap(),
        revision_id: RevisionId::new("extension-quality-revision").unwrap(),
        target: HoldTarget {
            hold_id: NodeId::new("extension-quality-hold").unwrap(),
            request_version: RequestVersion::new(1).unwrap(),
        },
        input: ContextArtifact {
            manifest: manifest.reference().clone(),
            sha256: manifest.sha256().clone(),
        },
        output_workspace: WorkspaceRef::new("outputs").unwrap(),
        constraints: HoldConstraints {
            video: VideoSpec::new(
                plan.project_frames(),
                plan.project_frame_rate(),
                width,
                height,
            )
            .unwrap(),
            conditioning: match plan.direction() {
                ExtensionDirection::FromLeft => ConditioningMode::ExtendFromLeft,
                ExtensionDirection::FromRight => ConditioningMode::ExtendFromRight,
            },
            motion: MotionAmount::Still,
            instructions: None,
            region_target: None,
        },
        provider: Box::new(ProviderSelection {
            pack_id: ProviderPackId::new("fixture").unwrap(),
            pack_version: ProviderPackVersion::new("1").unwrap(),
            runtime_id: RuntimeId::new("fixture").unwrap(),
            runtime_version: RuntimeVersion::new("1").unwrap(),
            seed: 1,
        }),
        plan: Box::new(plan.clone()),
    };
    let conditioning = capture_extension_conditioning(
        &ArtifactWorkspace::open(directory).unwrap(),
        &request,
        &manifest,
        &WorkspaceRef::new("inputs").unwrap(),
        ConditioningLimits::new(1024 * 1024, 1024 * 1024, 30_000).unwrap(),
        &AtomicBool::new(false),
    )
    .unwrap();
    (request, conditioning)
}

impl Fixture {
    pub fn new(
        direction: ExtensionDirection,
        output: u32,
        generated: [u8; GENERATED as usize],
        context: [u8; CONTEXT as usize],
        opposite: bool,
    ) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let plan = plan(direction, output);
        let (request, conditioning) = retained_inputs(directory.path(), &plan, opposite);
        let values = match direction {
            ExtensionDirection::FromLeft => [context.as_slice(), generated.as_slice()].concat(),
            ExtensionDirection::FromRight => [generated.as_slice(), context.as_slice()].concat(),
        };
        let raw: Vec<_> = values
            .into_iter()
            .flat_map(|value| std::iter::repeat_n(value, (WIDTH * HEIGHT * 3) as usize))
            .collect();
        let native = encode(directory.path(), &raw);
        let media = convert(&native, &plan);
        Self {
            directory,
            request,
            conditioning,
            media,
            native,
        }
    }
}

pub fn convert(native: &[u8], plan: &ExtensionGenerationPlan) -> CanonicalExtension {
    let request = ExtensionConversionRequest {
        protocol: EXTENSION_PROTOCOL_VERSION,
        operation: ExtensionOperation::SampleExtension,
        native: VideoContract {
            width: WIDTH,
            height: HEIGHT,
            frames: CONTEXT + GENERATED,
            rate_num: 24,
            rate_den: 1,
        },
        sampling: plan.sampling_map().clone(),
        input_byte_length: native.len() as u64,
        limits: ConversionLimits {
            max_input_bytes: native.len() as u64,
            max_output_bytes: 4 * 1024 * 1024,
            max_scratch_bytes: u64::from(WIDTH * HEIGHT * 3 * (CONTEXT + GENERATED)),
            timeout_ms: 30_000,
        },
    };
    canonicalize_extension(
        Path::new(env!("CARGO_BIN_EXE_deadpan-media-worker")),
        &mut Cursor::new(native),
        InputIdentity {
            sha256: sha2::Sha256::digest(native).into(),
        },
        &request,
        &AtomicBool::new(false),
    )
    .unwrap()
}

/// The fixture encoder retains sole ownership of its unreaped process leader
/// until checked group teardown, including diagnostic overflow and timeout.
fn encode(directory: &Path, bytes: &[u8]) -> Vec<u8> {
    encode_rgb(directory, bytes, WIDTH, HEIGHT, CONTEXT + GENERATED)
}

pub(super) fn encode_rgb(
    directory: &Path,
    bytes: &[u8],
    width: u32,
    height: u32,
    frames: u32,
) -> Vec<u8> {
    assert_eq!(
        bytes.len(),
        (u64::from(width) * u64::from(height) * u64::from(frames) * 3) as usize
    );
    let raw = directory.join("native.rgb");
    let movie = directory.join("native.mp4");
    fs::write(&raw, bytes).unwrap();
    let ffmpeg = std::env::var_os("DEADPAN_BRIDGE_FFMPEG")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/opt/homebrew/bin/ffmpeg"));
    assert!(
        ffmpeg.is_file(),
        "fixture requires ffmpeg: {}",
        ffmpeg.display()
    );
    let errors = tempfile::tempfile().unwrap();
    let mut child = deadpan_native_process::spawn(
        Command::new(ffmpeg)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-nostdin",
                "-y",
                "-f",
                "rawvideo",
                "-pix_fmt",
                "rgb24",
                "-video_size",
            ])
            .arg(format!("{width}x{height}"))
            .args(["-framerate", "24", "-i"])
            .arg(&raw)
            .arg("-frames:v")
            .arg(frames.to_string())
            .args([
                "-vf",
                "setparams=range=full:color_primaries=bt709:color_trc=iec61966-2-1:colorspace=gbr",
                "-c:v",
                "libx264rgb",
                "-crf",
                "0",
                "-preset",
                "ultrafast",
                "-pix_fmt",
                "rgb24",
                "-movflags",
                "+write_colr",
                "-video_track_timescale",
                "24",
                "-color_range",
                "pc",
                "-colorspace",
                "rgb",
                "-color_trc",
                "iec61966-2-1",
                "-color_primaries",
                "bt709",
                "-threads",
                "1",
                "-fs",
                "4194304",
            ])
            .arg(&movie)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::from(errors.try_clone().unwrap()))
            .process_group(0),
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    let outcome = (|| -> std::io::Result<()> {
        loop {
            if errors.metadata()?.len() > 64 * 1024 {
                return Err(std::io::Error::other("fixture exceeded diagnostic bound"));
            }
            if waitid(
                WaitId::Pid(Pid::from_child(&child)),
                WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
            )?
            .is_some_and(|status| status.exited() || status.killed() || status.dumped())
            {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "fixture deadline",
                ));
            }
            std::thread::park_timeout(Duration::from_millis(5));
        }
    })();
    #[cfg(target_os = "macos")]
    let cleanup = deadpan_native_process::terminate_owned_group(
        &child,
        Instant::now() + Duration::from_secs(5),
    );
    #[cfg(target_os = "linux")]
    let cleanup = deadpan_native_process::signal_owned_group(&child);
    if cleanup.is_err() {
        deadpan_native_process::terminate_owned_leader(
            &child,
            Instant::now() + Duration::from_secs(5),
        )
        .expect("checked fixture leader cleanup");
    }
    let status = child.wait().unwrap();
    cleanup.expect("fixture group cleanup");
    outcome.expect("bounded fixture encoder");
    let mut diagnostic = String::new();
    let mut errors = errors;
    errors.rewind().unwrap();
    errors
        .take(64 * 1024)
        .read_to_string(&mut diagnostic)
        .unwrap();
    assert!(status.success(), "fixture encoder: {diagnostic}");
    let bytes = fs::read(movie).unwrap();
    assert!(bytes.len() <= 4 * 1024 * 1024);
    bytes
}
