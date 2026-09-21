use std::fmt::Write as _;
use std::fs;
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use blake3::hash;
use deadpan_media::protocol::{BridgeConversionRequest, VideoContract};
use deadpan_media::{
    CanonicalMedia, ConversionError, InputIdentity, canonicalize_bridge, sample_bridge,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

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

fn bridge_request(
    fixture: &Fixture,
    output_frames: u32,
    project_rate_num: u32,
    project_rate_den: u32,
) -> BridgeConversionRequest {
    let native = VideoContract {
        width: fixture.width,
        height: fixture.height,
        frames: fixture.frames,
        rate_num: fixture.rate_num,
        rate_den: fixture.rate_den,
    };
    serde_json::from_value(json!({
        "protocol": 2,
        "operation": "sample_bridge",
        "native": native,
        "sampling": {
            "schema_version": 1,
            "project_rate": {
                "numerator": project_rate_num,
                "denominator": project_rate_den,
            },
            "native_rate": {
                "numerator": fixture.rate_num,
                "denominator": fixture.rate_den,
            },
            "native_frame_count": fixture.frames,
            "output_frame_count": output_frames,
            "interpolation": "encoded_srgb_rgb8_linear_half_up",
        },
        "input_byte_length": fixture.file_bytes,
        "limits": {
            "max_input_bytes": fixture.file_bytes + 1,
            "max_output_bytes": 4 * 1024 * 1024,
            "max_scratch_bytes": native.scratch_bytes().unwrap(),
            "timeout_ms": 30_000,
        },
    }))
    .expect("valid bridge request")
}

fn run_pair(
    fixture: &Fixture,
    request: &BridgeConversionRequest,
) -> deadpan_media::CanonicalBridge {
    let input = read_fixture(fixture);
    let cancelled = AtomicBool::new(false);
    canonicalize_bridge(
        Path::new(env!("CARGO_BIN_EXE_deadpan-media-worker")),
        &mut Cursor::new(input),
        hex_identity(&fixture.file_sha256),
        request,
        &cancelled,
    )
    .unwrap_or_else(|error| panic!("{} bridge conversion failed: {error}", fixture.name))
}

fn expected_frame(frame: u32) -> [u8; 24] {
    let mut output = [0; 24];
    for y in 0..2u32 {
        for x in 0..4u32 {
            let index = ((y * 4 + x) * 3) as usize;
            output[index] = ((17 * frame + 31 * x + 7 * y + 3) % 256) as u8;
            output[index + 1] = ((29 * frame + 5 * x + 47 * y + 11) % 256) as u8;
            output[index + 2] = ((43 * frame + 13 * x + 19 * y + 23) % 256) as u8;
        }
    }
    output
}

fn expected_sampled_rgb(native_frames: u32, output_frames: u32) -> Vec<u8> {
    let denominator = u64::from(output_frames) + 1;
    let mut output = Vec::with_capacity(output_frames as usize * 24);
    for index in 0..output_frames {
        let numerator = u64::from(index + 1) * u64::from(native_frames - 1);
        let lower = numerator / denominator;
        let remainder = numerator % denominator;
        let left = expected_frame(lower as u32);
        if remainder == 0 {
            output.extend_from_slice(&left);
            continue;
        }
        let right = expected_frame((lower + 1) as u32);
        let left_weight = denominator - remainder;
        output.extend((0..left.len()).map(|byte| {
            let weighted = u64::from(left[byte]) * left_weight + u64::from(right[byte]) * remainder;
            ((weighted + denominator / 2) / denominator) as u8
        }));
    }
    output
}

fn assert_object_bytes(media: &mut CanonicalMedia) {
    let report = media.report().clone();
    let digest = media.object().content().digest().to_owned();
    assert_eq!(media.object().content().algorithm(), "blake3");
    assert_eq!(media.object().byte_length(), report.output_bytes);
    let mut bytes = Vec::new();
    media.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes.len() as u64, report.output_bytes);
    assert_eq!(hash(&bytes).to_hex().as_str(), digest);
    assert_eq!(media.seek(SeekFrom::Start(0)).unwrap(), 0);
    assert_eq!(media.seek(SeekFrom::End(0)).unwrap(), report.output_bytes);
}

fn assert_report_clock(report: &deadpan_media::protocol::ConversionReport) {
    assert_eq!(report.output_time_base_num, 1);
    assert_eq!(report.output_time_base_den, 1000);
    assert_eq!(report.first_output_pts, 0);
    assert_eq!(
        report.last_output_pts,
        report.video.matroska_pts(report.video.frames - 1).unwrap()
    );
}

#[test]
fn canonicalize_bridge_retains_native_and_independently_checks_up_and_down_sampling() {
    let fixture = fixture("rgb25_24");
    for (output_frames, rate_num, rate_den, expected_hash) in [
        (
            30,
            30_000,
            1001,
            "3f44a221f04474dd1804beb59556259e03f4fd2b518b3b4c3a9efa431efa4ee5",
        ),
        (
            20,
            24,
            1,
            "b11c3d9d319ce8bc764f311b5e5395343b0624076e191428777f1d8ba21ebda8",
        ),
    ] {
        let request = bridge_request(&fixture, output_frames, rate_num, rate_den);
        let native_scratch = request.native.scratch_bytes().unwrap();
        assert_eq!(request.limits.max_scratch_bytes, native_scratch);
        let output_scratch = request.output_video().unwrap().scratch_bytes().unwrap();
        assert_eq!(
            output_scratch > native_scratch,
            output_frames > fixture.frames
        );

        let expected = expected_sampled_rgb(fixture.frames, output_frames);
        assert_eq!(sha256_hex(&expected), expected_hash);
        let bridge = run_pair(&fixture, &request);
        assert_eq!(bridge.source_identity(), hex_identity(&fixture.file_sha256));
        assert_eq!(
            bridge.sampling().output_frame_count().frames(),
            i64::from(output_frames)
        );
        assert_eq!(
            bridge.sampling().native_frame_count().frames(),
            i64::from(fixture.frames)
        );

        let native_report = bridge.native().report();
        assert_eq!(native_report.video.frames, fixture.frames);
        assert_eq!(native_report.video.rate_num, fixture.rate_num);
        assert_eq!(native_report.video.rate_den, fixture.rate_den);
        assert_eq!(native_report.input_rgb_sha256, fixture.rgb_sha256);
        assert_eq!(native_report.output_rgb_sha256, fixture.rgb_sha256);
        assert_report_clock(native_report);

        let sampled_report = bridge.sampled().report();
        assert_eq!(sampled_report.video.frames, output_frames);
        assert_eq!(sampled_report.video.rate_num, rate_num);
        assert_eq!(sampled_report.video.rate_den, rate_den);
        assert_eq!(sampled_report.input_rgb_sha256, fixture.rgb_sha256);
        assert_eq!(
            sampled_report.input_rgb_sha256,
            native_report.output_rgb_sha256
        );
        assert_ne!(
            sampled_report.output_rgb_sha256,
            sampled_report.input_rgb_sha256
        );
        assert_eq!(sampled_report.output_rgb_sha256, expected_hash);
        assert_report_clock(sampled_report);

        assert_eq!(expected.len(), output_frames as usize * 24);
        let (mut native, mut sampled, _) = bridge.into_parts();
        assert_object_bytes(&mut native);
        assert_object_bytes(&mut sampled);
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    let digest = digest.finalize();
    let mut output = String::with_capacity(64);
    for byte in digest {
        write!(&mut output, "{byte:02x}").unwrap();
    }
    output
}

#[test]
fn canonicalize_bridge_checks_halfway_rounding_and_audio_discard() {
    let fixture = fixture("rgb2_24_audio");
    let request = bridge_request(&fixture, 1, 24, 1);
    let expected = expected_sampled_rgb(fixture.frames, 1);
    assert_eq!(
        expected,
        vec![
            12, 26, 45, 43, 31, 58, 74, 36, 71, 105, 41, 84, 19, 73, 64, 50, 78, 77, 81, 83, 90,
            112, 88, 103,
        ]
    );
    assert_eq!(
        sha256_hex(&expected),
        "3763dae393f25801a1836e00b578d8e04ea34696ff5bd21289d31bf261c16028"
    );
    let bridge = run_pair(&fixture, &request);
    assert_eq!(bridge.native().report().discarded_audio_streams, 1);
    assert_eq!(bridge.sampled().report().discarded_audio_streams, 1);
    assert_eq!(
        bridge.sampled().report().output_rgb_sha256,
        "3763dae393f25801a1836e00b578d8e04ea34696ff5bd21289d31bf261c16028"
    );
    assert_ne!(
        bridge.sampled().report().input_rgb_sha256,
        bridge.sampled().report().output_rgb_sha256
    );
    let (mut native, mut sampled, _) = bridge.into_parts();
    assert_object_bytes(&mut native);
    assert_object_bytes(&mut sampled);
}

#[test]
fn bridge_contract_rejects_bad_native_map_rate_and_scratch_before_worker() {
    let fixture = fixture("rgb25_24");
    let input = read_fixture(&fixture);
    let cancelled = AtomicBool::new(false);
    let executable = Path::new(env!("CARGO_BIN_EXE_deadpan-media-worker"));

    let mut bad_native = bridge_request(&fixture, 30, 30_000, 1001);
    bad_native.native.rate_num = 25;
    assert!(matches!(
        sample_bridge(
            executable,
            &mut Cursor::new(input.clone()),
            hex_identity(&fixture.file_sha256),
            &bad_native,
            &cancelled,
        ),
        Err(ConversionError::Contract(_))
    ));

    let bad_map: BridgeConversionRequest = serde_json::from_value(json!({
        "protocol": 2,
        "operation": "sample_bridge",
        "native": {
            "width": fixture.width,
            "height": fixture.height,
            "frames": fixture.frames,
            "rate_num": fixture.rate_num,
            "rate_den": fixture.rate_den,
        },
        "sampling": {
            "schema_version": 1,
            "project_rate": {"numerator": 30_000, "denominator": 1001},
            "native_rate": {"numerator": fixture.rate_num, "denominator": fixture.rate_den},
            "native_frame_count": fixture.frames - 1,
            "output_frame_count": 30,
            "interpolation": "encoded_srgb_rgb8_linear_half_up",
        },
        "input_byte_length": fixture.file_bytes,
        "limits": {
            "max_input_bytes": fixture.file_bytes + 1,
            "max_output_bytes": 4 * 1024 * 1024,
            "max_scratch_bytes": 600,
            "timeout_ms": 30_000,
        },
    }))
    .expect("map shape is valid before cross-field validation");
    assert!(matches!(
        sample_bridge(
            executable,
            &mut Cursor::new(input.clone()),
            hex_identity(&fixture.file_sha256),
            &bad_map,
            &cancelled,
        ),
        Err(ConversionError::Contract(_))
    ));

    let mut bad_rate = bridge_request(&fixture, 30, 30_000, 1001);
    bad_rate.native.rate_den = 0;
    assert!(matches!(
        sample_bridge(
            executable,
            &mut Cursor::new(input.clone()),
            hex_identity(&fixture.file_sha256),
            &bad_rate,
            &cancelled,
        ),
        Err(ConversionError::Contract(_))
    ));

    let mut small_scratch = bridge_request(&fixture, 30, 30_000, 1001);
    small_scratch.limits.max_scratch_bytes -= 1;
    assert!(matches!(
        canonicalize_bridge(
            executable,
            &mut Cursor::new(input),
            hex_identity(&fixture.file_sha256),
            &small_scratch,
            &cancelled,
        ),
        Err(ConversionError::Contract(_))
    ));
}
