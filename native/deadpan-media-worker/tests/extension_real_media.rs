//! An independent pixel/timing oracle for generated-only extension sampling.

use std::fs;
use std::io::{Cursor, Read, Seek, Write};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_core::{
    BridgeInterpolation, ExtensionDirection, ExtensionSamplingMap, FrameDuration, FrameRate,
};
use deadpan_media::protocol::{
    ConversionLimits, EXTENSION_PROTOCOL_VERSION, ExtensionConversionRequest, ExtensionOperation,
    VideoContract,
};
use deadpan_media::{
    CanonicalMedia, ConversionError, InputIdentity, canonicalize_extension, sample_extension,
};
use deadpan_source::{DecodeControl, DecodeLimits, SourceDecoder};
use rustix::process::{Pid, WaitId, WaitIdOptions, waitid};
use sha2::{Digest, Sha256};

const FRAME_BYTES: usize = 4 * 2 * 3;
const CONTEXT: u32 = 9;
const GENERATED: u32 = 8;

fn generated_frame(ordinal: u32) -> Vec<u8> {
    (0..8u32)
        .flat_map(|pixel| {
            [
                u8::try_from(ordinal * 2 + pixel).unwrap(),
                u8::try_from(ordinal * 17 + pixel * 3).unwrap(),
                u8::try_from(100 + ordinal + pixel).unwrap(),
            ]
        })
        .collect()
}

fn native_rgb(direction: ExtensionDirection) -> Vec<u8> {
    let context: Vec<u8> = (0..CONTEXT)
        .flat_map(|ordinal| {
            (0..8u32).flat_map(move |pixel| [250, ordinal as u8, (240 + pixel) as u8])
        })
        .collect();
    let generated: Vec<u8> = (0..GENERATED).flat_map(generated_frame).collect();
    match direction {
        ExtensionDirection::FromLeft => [context, generated].concat(),
        ExtensionDirection::FromRight => [generated, context].concat(),
    }
}

/// This oracle does not call ExtensionSamplingMap::native_position or any
/// production interpolation helper. Signed rational center coordinates and
/// explicit edge cases implement the documented equation independently.
fn expected_rgb(output: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    let denominator = i64::from(output) * 2;
    for ordinal in 0..output {
        let center = (2 * i64::from(ordinal) + 1) * i64::from(GENERATED) - i64::from(output);
        if center <= 0 {
            bytes.extend(generated_frame(0));
        } else if center >= i64::from(GENERATED - 1) * denominator {
            bytes.extend(generated_frame(GENERATED - 1));
        } else {
            let lower = u32::try_from(center / denominator).unwrap();
            let remainder = center % denominator;
            let left = generated_frame(lower);
            let right = generated_frame(lower + u32::from(remainder != 0));
            bytes.extend(left.iter().zip(right).map(|(&a, b)| {
                let twice_weighted =
                    2 * (i64::from(a) * (denominator - remainder) + i64::from(b) * remainder);
                u8::try_from((twice_weighted + denominator) / (2 * denominator)).unwrap()
            }));
        }
    }
    bytes
}

/// Generate a bounded 17-frame lossless fixture. Keep the unreaped process
/// leader until checked group teardown on success, timeout and every error.
fn fixture(directory: &Path, direction: ExtensionDirection) -> Vec<u8> {
    let raw = directory.join("native.rgb");
    let movie = directory.join("native.mp4");
    fs::write(&raw, native_rgb(direction)).unwrap();
    let ffmpeg = std::env::var_os("DEADPAN_BRIDGE_FFMPEG")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/opt/homebrew/bin/ffmpeg"));
    assert!(
        ffmpeg.is_file(),
        "real extension fixture requires ffmpeg: {}",
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
                "4x2",
                "-framerate",
                "24",
                "-i",
            ])
            .arg(&raw)
            .args([
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=880:sample_rate=8000:duration=0.75",
                "-map",
                "0:v:0",
                "-map",
                "1:a:0",
                "-frames:v",
                "17",
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
                "-c:a",
                "aac",
                "-threads",
                "1",
                "-fs",
                "1048576",
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
                return Err(std::io::Error::other(
                    "fixture encoder exceeded diagnostic bound",
                ));
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
                    "fixture encoder deadline",
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
    cleanup.expect("fixture encoder group cleanup");
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
    assert!(bytes.len() <= 1024 * 1024);
    bytes
}

fn request(
    input: &[u8],
    direction: ExtensionDirection,
    output: u32,
    rate_num: u32,
    rate_den: u32,
) -> ExtensionConversionRequest {
    ExtensionConversionRequest {
        protocol: EXTENSION_PROTOCOL_VERSION,
        operation: ExtensionOperation::SampleExtension,
        native: VideoContract {
            width: 4,
            height: 2,
            frames: CONTEXT + GENERATED,
            rate_num: 24,
            rate_den: 1,
        },
        sampling: ExtensionSamplingMap::new(
            direction,
            FrameRate::new(rate_num, rate_den).unwrap(),
            FrameRate::new(24, 1).unwrap(),
            FrameDuration::new(CONTEXT.into()).unwrap(),
            FrameDuration::new(GENERATED.into()).unwrap(),
            FrameDuration::new(output.into()).unwrap(),
            BridgeInterpolation::EncodedSrgbRgb8LinearHalfUp,
        )
        .unwrap(),
        input_byte_length: input.len() as u64,
        limits: ConversionLimits {
            max_input_bytes: input.len() as u64,
            max_output_bytes: 1024 * 1024,
            max_scratch_bytes: (FRAME_BYTES as u64) * u64::from(CONTEXT + GENERATED),
            timeout_ms: 30_000,
        },
    }
}

fn identity(input: &[u8]) -> InputIdentity {
    InputIdentity {
        sha256: Sha256::digest(input).into(),
    }
}

/// Decode the published private bytes through the independent source adapter,
/// checking every pixel, alpha, ordinal, PTS, duration and absence of audio.
fn verify(media: &mut CanonicalMedia, expected: &[u8], video: VideoContract) {
    let mut encoded = Vec::new();
    media.read_to_end(&mut encoded).unwrap();
    assert_eq!(encoded.len() as u64, media.object().byte_length());
    assert_eq!(
        blake3::hash(&encoded).to_hex().as_str(),
        media.object().content().digest()
    );
    assert_eq!(media.report().video, video);
    assert_eq!(
        media.report().output_rgb_sha256,
        Sha256::digest(expected)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    );
    assert_eq!(media.report().ffv1_version, 3);
    assert!(media.report().slice_crc);
    assert_eq!(media.report().discarded_audio_streams, 1);
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(&encoded).unwrap();
    file.rewind().unwrap();
    let cancelled = AtomicBool::new(false);
    let control = DecodeControl {
        timeout: Duration::from_secs(30),
        cancelled: &cancelled,
    };
    let mut decoder = SourceDecoder::open(file, DecodeLimits::default(), control).unwrap();
    assert!(decoder.info().audio_streams.is_empty());
    assert_eq!(decoder.info().time_base_num, 1);
    assert_eq!(decoder.info().time_base_den, 1000);
    let mut decoded = Vec::new();
    for ordinal in 0..video.frames {
        let frame = decoder
            .next_rgba(control)
            .unwrap()
            .expect("exact output frame count");
        assert_eq!((frame.width, frame.height, frame.sample_bits), (4, 2, 8));
        let pts = (u64::from(ordinal) * u64::from(video.rate_den) * 1000
            + u64::from(video.rate_num) / 2)
            / u64::from(video.rate_num);
        assert_eq!(frame.metadata.pts, pts as i64);
        assert_eq!(
            frame.metadata.reported_duration,
            Some(i64::from(video.rate_den * 1000 / video.rate_num))
        );
        for row in 0..2 {
            for pixel in frame.rgba[row * frame.row_stride_bytes..row * frame.row_stride_bytes + 16]
                .chunks_exact(4)
            {
                decoded.extend_from_slice(&pixel[..3]);
                assert_eq!(pixel[3], 255);
            }
        }
    }
    assert!(decoder.next_rgba(control).unwrap().is_none());
    assert_eq!(decoded, expected);
}

#[test]
fn extension_real_media_excludes_context_and_verifies_both_directions_at_every_sample() {
    for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
        let directory = tempfile::tempdir().unwrap();
        let input = fixture(directory.path(), direction);
        for (output, rate_num, rate_den) in [(12, 30_000, 1001), (1, 24, 1), (8, 24, 1), (3, 24, 1)]
        {
            let request = request(&input, direction, output, rate_num, rate_den);
            let extension = canonicalize_extension(
                Path::new(env!("CARGO_BIN_EXE_deadpan-media-worker")),
                &mut Cursor::new(&input),
                identity(&input),
                &request,
                &AtomicBool::new(false),
            )
            .unwrap();
            assert_eq!(extension.source_identity(), identity(&input));
            assert_eq!(extension.sampling(), &request.sampling);
            assert_eq!(
                extension.native().report().output_rgb_sha256,
                extension.sampled().report().input_rgb_sha256
            );
            let (mut native, mut sampled, map) = extension.into_parts();
            assert_eq!(map, request.sampling);
            verify(&mut native, &native_rgb(direction), request.native);
            let expected = expected_rgb(output);
            if output == 1 {
                assert_eq!(expected[2], 104, "103.5 rounds half up");
            }
            if output == 12 {
                assert_eq!(&expected[..FRAME_BYTES], generated_frame(0));
                assert_eq!(&expected[11 * FRAME_BYTES..], generated_frame(7));
            }
            verify(&mut sampled, &expected, request.output_video().unwrap());
        }
    }
}

#[test]
fn extension_sampling_refuses_false_native_contracts_and_private_input_identity() {
    let directory = tempfile::tempdir().unwrap();
    let input = fixture(directory.path(), ExtensionDirection::FromLeft);
    let mut request = request(&input, ExtensionDirection::FromLeft, 12, 24, 1);
    let executable = Path::new(env!("CARGO_BIN_EXE_deadpan-media-worker"));
    let cancelled = AtomicBool::new(false);
    assert!(matches!(
        sample_extension(
            executable,
            &mut Cursor::new(&input),
            InputIdentity { sha256: [0; 32] },
            &request,
            &cancelled
        ),
        Err(ConversionError::InputIdentity)
    ));
    request.native.width = 3;
    assert!(matches!(
        sample_extension(
            executable,
            &mut Cursor::new(&input),
            identity(&input),
            &request,
            &cancelled
        ),
        Err(ConversionError::Worker { .. })
    ));
    request.native.width = 4;
    request.limits.max_scratch_bytes -= 1;
    assert!(matches!(
        sample_extension(
            executable,
            &mut Cursor::new(&input),
            identity(&input),
            &request,
            &cancelled
        ),
        Err(ConversionError::Contract(_))
    ));
    request.limits.max_scratch_bytes += 1;
    assert!(matches!(
        sample_extension(
            executable,
            &mut Cursor::new(&input),
            identity(&input),
            &request,
            &AtomicBool::new(true)
        ),
        Err(ConversionError::Cancelled)
    ));
    let mut sampled = sample_extension(
        executable,
        &mut Cursor::new(&input),
        identity(&input),
        &request,
        &cancelled,
    )
    .unwrap();
    verify(
        &mut sampled,
        &expected_rgb(12),
        request.output_video().unwrap(),
    );
}
