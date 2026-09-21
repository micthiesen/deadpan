use std::fs;
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use blake3::hash;
use deadpan_media::protocol::{ConversionLimits, ConversionRequest, VideoContract};
use deadpan_media::{ConversionError, InputIdentity, canonicalize};
use serde_json::Value;

const FIXTURE_MANIFEST: &str = include_str!("fixtures/manifest.json");
const FIXTURE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

#[derive(Debug, Clone)]
struct Fixture {
    name: String,
    path: PathBuf,
    width: u32,
    height: u32,
    frames: u32,
    rate_num: u32,
    rate_den: u32,
    file_bytes: u64,
    file_sha256: String,
    rgb_sha256: String,
    has_audio: bool,
}

fn fixture(name: &str) -> Fixture {
    let manifest: Value = serde_json::from_str(FIXTURE_MANIFEST).expect("valid fixture manifest");
    let entry = manifest["fixtures"]
        .as_array()
        .expect("fixture array")
        .iter()
        .find(|entry| entry["name"] == name)
        .unwrap_or_else(|| panic!("fixture {name} is listed"));
    let string = |key: &str| {
        entry[key]
            .as_str()
            .unwrap_or_else(|| panic!("{name}.{key}"))
    };
    let number = |key: &str| {
        entry[key]
            .as_u64()
            .unwrap_or_else(|| panic!("{name}.{key}"))
    };
    let path = Path::new(FIXTURE_DIR).join(string("file"));
    let file_bytes = number("file_bytes");
    assert_eq!(
        fs::metadata(&path).unwrap().len(),
        file_bytes,
        "{name} length"
    );
    Fixture {
        name: name.to_owned(),
        path,
        width: number("width") as u32,
        height: number("height") as u32,
        frames: number("frames") as u32,
        rate_num: number("rate_num") as u32,
        rate_den: number("rate_den") as u32,
        file_bytes,
        file_sha256: string("file_sha256").to_owned(),
        rgb_sha256: string("rgb_sha256").to_owned(),
        has_audio: entry["has_audio"].as_bool().expect("audio flag"),
    }
}

fn read_fixture(fixture: &Fixture) -> Vec<u8> {
    fs::read(&fixture.path).unwrap_or_else(|error| panic!("read {}: {error}", fixture.name))
}

fn hex_identity(value: &str) -> InputIdentity {
    assert_eq!(value.len(), 64);
    let mut sha256 = [0; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        sha256[index] = (hex(pair[0]) << 4) | hex(pair[1]);
    }
    InputIdentity { sha256 }
}

fn hex(value: u8) -> u8 {
    match value {
        b'0'..=b'9' => value - b'0',
        b'a'..=b'f' => value - b'a' + 10,
        _ => panic!("invalid lowercase hexadecimal fixture hash"),
    }
}

fn request(fixture: &Fixture) -> ConversionRequest {
    let video = VideoContract {
        width: fixture.width,
        height: fixture.height,
        frames: fixture.frames,
        rate_num: fixture.rate_num,
        rate_den: fixture.rate_den,
    };
    ConversionRequest {
        protocol: 1,
        video,
        input_byte_length: fixture.file_bytes,
        limits: ConversionLimits {
            max_input_bytes: fixture.file_bytes + 1,
            max_output_bytes: 4 * 1024 * 1024,
            max_scratch_bytes: video.scratch_bytes().unwrap(),
            timeout_ms: 30_000,
        },
    }
}

fn run(
    fixture: &Fixture,
    request: &ConversionRequest,
) -> Result<deadpan_media::CanonicalMedia, ConversionError> {
    let input = read_fixture(fixture);
    let cancelled = AtomicBool::new(false);
    canonicalize(
        Path::new(env!("CARGO_BIN_EXE_deadpan-media-worker")),
        &mut Cursor::new(input),
        hex_identity(&fixture.file_sha256),
        request,
        &cancelled,
    )
}

fn expect_worker(result: Result<deadpan_media::CanonicalMedia, ConversionError>) -> String {
    match result {
        Err(ConversionError::Worker { code, .. }) => code,
        Ok(_) => panic!("expected worker failure, got a successful conversion"),
        Err(error) => panic!("expected worker failure, got {error}"),
    }
}

fn assert_valid_output(fixture: &Fixture, media: &mut deadpan_media::CanonicalMedia) {
    let report = media.report().clone();
    assert_eq!(report.protocol, 2);
    assert_eq!(report.video.width, fixture.width);
    assert_eq!(report.video.height, fixture.height);
    assert_eq!(report.video.frames, fixture.frames);
    assert_eq!(report.video.rate_num, fixture.rate_num);
    assert_eq!(report.video.rate_den, fixture.rate_den);
    assert_eq!(report.input_rgb_sha256, fixture.rgb_sha256);
    assert_eq!(report.output_rgb_sha256, fixture.rgb_sha256);
    assert_eq!(report.output_time_base_num, 1);
    assert_eq!(report.output_time_base_den, 1000);
    assert_eq!(report.first_output_pts, 0);
    assert_eq!(
        report.last_output_pts,
        report.video.matroska_pts(fixture.frames - 1).unwrap()
    );
    assert_eq!(
        report.last_output_duration,
        i64::from((fixture.rate_den * 1000) / fixture.rate_num)
    );
    let output_span = report.output_span().unwrap();
    assert_eq!(output_span.start().ticks, report.first_output_pts);
    assert_eq!(
        output_span.end().ticks,
        report.last_output_pts + report.last_output_duration
    );
    assert_eq!(report.input_time_base_num, 1);
    assert_eq!(report.input_time_base_den, fixture.rate_num);
    assert_eq!(report.ffv1_version, 3);
    assert!(report.slice_crc);
    assert_eq!(report.discarded_audio_streams, u32::from(fixture.has_audio));

    let object_algorithm = media.object().content().algorithm();
    let object_digest = media.object().content().digest().to_owned();
    let object_length = media.object().byte_length();
    assert_eq!(object_algorithm, "blake3");
    assert_eq!(object_digest.len(), 64);
    assert!(
        object_digest
            .bytes()
            .all(|byte| { byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte) })
    );
    assert_eq!(object_length, report.output_bytes);

    let mut bytes = Vec::new();
    media.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes.len() as u64, report.output_bytes);
    assert_eq!(hash(&bytes).to_hex().as_str(), object_digest);
    assert_eq!(media.seek(SeekFrom::Start(0)).unwrap(), 0);
    let mut prefix = [0; 16];
    let count = media.read(&mut prefix).unwrap();
    assert_eq!(count, prefix.len());
    assert_eq!(media.seek(SeekFrom::End(0)).unwrap(), report.output_bytes);
}

#[test]
fn real_rgb_inputs_preserve_pixels_profile_timing_and_object_identity() {
    for name in ["rgb30_30000_1001", "rgb25_24", "rgb1_24"] {
        let fixture = fixture(name);
        let mut media = run(&fixture, &request(&fixture)).unwrap();
        assert_valid_output(&fixture, &mut media);
    }
}

#[test]
fn measured_final_span_preserves_matroska_default_duration() {
    let native = fixture("rgb25_24");
    let media = run(&native, &request(&native)).unwrap();
    let report = media.report();
    assert_eq!(report.last_output_pts, 1000);
    assert_eq!(report.last_output_duration, 41);
    assert_eq!(report.output_span().unwrap().end().ticks, 1041);
    // Rounding the next 24 fps frame boundary would instead produce 1042 ms.
    assert_ne!(report.output_span().unwrap().end().ticks, 1042);

    let fractional = fixture("rgb30_30000_1001");
    let media = run(&fractional, &request(&fractional)).unwrap();
    let report = media.report();
    assert_eq!(report.last_output_pts, 968);
    assert_eq!(report.last_output_duration, 33);
    assert_eq!(report.output_span().unwrap().end().ticks, 1001);
}

#[test]
fn real_audio_stream_is_discarded_while_video_pixels_remain_exact() {
    let fixture = fixture("rgb2_24_audio");
    let mut media = run(&fixture, &request(&fixture)).unwrap();
    assert_valid_output(&fixture, &mut media);
    assert_eq!(media.report().discarded_audio_streams, 1);
}

#[test]
fn identity_length_and_contract_budgets_fail_before_or_at_the_worker_boundary() {
    let fixture = fixture("rgb1_24");
    let mut identity = hex_identity(&fixture.file_sha256);
    identity.sha256[0] ^= 1;
    let input = read_fixture(&fixture);
    let cancelled = AtomicBool::new(false);
    let error = canonicalize(
        Path::new(env!("CARGO_BIN_EXE_deadpan-media-worker")),
        &mut Cursor::new(input.clone()),
        identity,
        &request(&fixture),
        &cancelled,
    )
    .err()
    .expect("identity mismatch must fail");
    assert!(matches!(error, ConversionError::InputIdentity));

    let mut short = request(&fixture);
    short.input_byte_length -= 1;
    let error = canonicalize(
        Path::new(env!("CARGO_BIN_EXE_deadpan-media-worker")),
        &mut Cursor::new(input),
        hex_identity(&fixture.file_sha256),
        &short,
        &cancelled,
    )
    .err()
    .expect("short input must fail");
    assert!(matches!(error, ConversionError::InputIdentity));

    let mut input_budget = request(&fixture);
    input_budget.limits.max_input_bytes = fixture.file_bytes - 1;
    assert!(matches!(
        run(&fixture, &input_budget),
        Err(ConversionError::Contract(_))
    ));

    let mut scratch_budget = request(&fixture);
    scratch_budget.limits.max_scratch_bytes -= 1;
    assert!(matches!(
        run(&fixture, &scratch_budget),
        Err(ConversionError::Contract(_))
    ));

    let mut output_budget = request(&fixture);
    output_budget.limits.max_output_bytes = 1;
    let error = run(&fixture, &output_budget)
        .err()
        .expect("output budget must fail");
    assert!(matches!(
        error,
        ConversionError::Worker { code, .. } if code == "output_too_large"
    ));
}

#[test]
fn wrong_dimensions_rate_and_frame_count_are_rejected_by_real_decoder() {
    let fixture = fixture("rgb25_24");
    let mut dimensions = request(&fixture);
    dimensions.video.width += 1;
    dimensions.limits.max_scratch_bytes = dimensions.video.scratch_bytes().unwrap();
    assert_eq!(expect_worker(run(&fixture, &dimensions)), "invalid_media");

    let mut rate = request(&fixture);
    rate.video.rate_num = 25;
    rate.limits.max_scratch_bytes = rate.video.scratch_bytes().unwrap();
    assert_eq!(expect_worker(run(&fixture, &rate)), "invalid_media");

    let mut count = request(&fixture);
    count.video.frames += 1;
    count.limits.max_scratch_bytes = count.video.scratch_bytes().unwrap();
    assert_eq!(expect_worker(run(&fixture, &count)), "invalid_media");
}

#[test]
fn missing_color_tags_and_corrupt_encoded_picture_are_rejected() {
    let no_tags = fixture("rgb1_24_no_tags");
    assert_eq!(
        expect_worker(run(&no_tags, &request(&no_tags))),
        "invalid_media"
    );

    let corrupt = fixture("rgb1_24_corrupt");
    let code = expect_worker(run(&corrupt, &request(&corrupt)));
    assert!(matches!(code.as_str(), "invalid_media" | "ffmpeg_failure"));
}

#[test]
fn cancellation_is_checked_before_spawning_the_worker() {
    let fixture = fixture("rgb1_24");
    let input = read_fixture(&fixture);
    let cancelled = AtomicBool::new(true);
    let error = canonicalize(
        Path::new(env!("CARGO_BIN_EXE_deadpan-media-worker")),
        &mut Cursor::new(input),
        hex_identity(&fixture.file_sha256),
        &request(&fixture),
        &cancelled,
    )
    .err()
    .expect("cancellation must fail");
    assert!(matches!(error, ConversionError::Cancelled));
    assert!(cancelled.load(Ordering::Acquire));
}
