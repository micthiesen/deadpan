//! Gate G adversarial regression for the closed container grammar, run before
//! any FFmpeg parsing. Seeds are the committed real fixtures. Every input must
//! end in admission or a typed `SourceDecodeError`, never a panic, within the
//! per-case time and allocation bounds. `cargo xtask chaos` extends the same
//! targets into a time-bounded campaign; see docs/ADVERSARIAL.md.

use super::*;
use deadpan_chaos::{Outcome, Target, Verdict, fuzz};
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

static CANCELLED: AtomicBool = AtomicBool::new(false);

fn control() -> DecodeControl<'static> {
    DecodeControl {
        timeout: Duration::from_secs(10),
        cancelled: &CANCELLED,
    }
}

fn fixture_dir(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join(name)
}

fn fixtures(extension: &[&str]) -> Vec<Vec<u8>> {
    let mut paths: Vec<PathBuf> = ["fixtures", "audio-fixtures"]
        .iter()
        .filter_map(|directory| std::fs::read_dir(fixture_dir(directory)).ok())
        .flat_map(|entries| entries.filter_map(std::result::Result::ok))
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| extension.contains(&value))
        })
        .collect();
    paths.sort();
    paths
        .into_iter()
        // Keep regression seeds small; large grain fixtures add little grammar.
        .filter_map(|path| std::fs::read(path).ok())
        .filter(|bytes| bytes.len() <= 128 * 1024)
        .collect()
}

fn snapshot(bytes: &[u8]) -> File {
    let mut file = tempfile::tempfile().expect("temporary snapshot");
    file.write_all(bytes).expect("write snapshot");
    file
}

fn classify(result: Result<u64>) -> Outcome {
    match result {
        Ok(_) => Ok(Verdict::Accepted),
        Err(SourceDecodeError::Native { code, message }) => {
            if code.is_empty() || message.is_empty() {
                return Err(format!("untyped rejection {code:?}: {message:?}"));
            }
            Ok(Verdict::Rejected(format!("{code}: {message}")))
        }
        Err(other) => Ok(Verdict::Rejected(other.to_string())),
    }
}

fn limits() -> InputLimits {
    DecodeLimits::default().into()
}

#[test]
fn video_container_admission_survives_mutation() {
    let seeds = fixtures(&["mp4", "mkv", "webm"]);
    assert!(seeds.len() >= 10, "fixture seeds missing");
    let report = fuzz(
        Target::bytes("source-container-video").iterations(1500),
        seeds,
        |input| {
            if input.is_empty() {
                return Ok(Verdict::Rejected("empty".into()));
            }
            let file = snapshot(input);
            let admitted = classify(validate(&file, Selection::Video, limits(), control()))?;
            // The public inspection path shares the MP4 grammar; it must agree
            // that an admitted MP4 is well formed and must not panic otherwise.
            if input.get(4..8) == Some(b"ftyp") {
                let inspected = inspect_mp4(&file, DecodeLimits::default(), control());
                if admitted == Verdict::Accepted && inspected.is_err() {
                    return Err(format!(
                        "admitted MP4 failed inspection: {}",
                        inspected
                            .err()
                            .map(|error| error.to_string())
                            .unwrap_or_default()
                    ));
                }
            }
            Ok(admitted)
        },
    );
    report.assert_clean();
}

#[test]
fn audio_container_admission_survives_mutation() {
    let seeds = fixtures(&["mp4", "wav", "webm", "mkv", "mp3"]);
    let report = fuzz(
        Target::bytes("source-container-audio").iterations(800),
        seeds,
        |input| {
            if input.is_empty() {
                return Ok(Verdict::Rejected("empty".into()));
            }
            let file = snapshot(input);
            let audio: InputLimits = crate::audio::AudioDecodeLimits::default().into();
            classify(validate(&file, Selection::FirstAudio, audio, control()))
        },
    );
    report.assert_clean();
}

/// Byte offset and length of the first box body with `tag`.
fn box_body(bytes: &[u8], tag: &[u8; 4]) -> Option<(usize, usize)> {
    let at = bytes.windows(4).position(|window| window == tag)?;
    let size = u32::from_be_bytes(bytes.get(at - 4..at)?.try_into().ok()?) as usize;
    let body = at + 4;
    (size >= 8 && at - 4 + size <= bytes.len()).then_some((body, size - 8))
}

/// Mutates only the codec configuration record in place, so the container
/// remains structurally valid and the avcC/hvcC/vpcC parsers see the damage.
#[test]
fn codec_configuration_records_survive_in_place_mutation() {
    let hosts: Vec<(Vec<u8>, usize, usize)> = fixtures(&["mp4"])
        .into_iter()
        .filter_map(|bytes| {
            let (start, length) = box_body(&bytes, b"hvcC")
                .or_else(|| box_body(&bytes, b"avcC"))
                .or_else(|| box_body(&bytes, b"vpcC"))?;
            Some((bytes, start, length))
        })
        .collect();
    assert!(hosts.len() >= 4, "codec configuration seeds missing");
    let seeds: Vec<Vec<u8>> = hosts
        .iter()
        .enumerate()
        .map(|(index, (bytes, start, length))| {
            let mut seed = vec![u8::try_from(index).expect("few hosts")];
            seed.extend_from_slice(&bytes[*start..*start + *length]);
            seed
        })
        .collect();
    let report = fuzz(
        Target::bytes("source-codec-config").iterations(1500),
        seeds,
        |input| {
            let Some((selector, record)) = input.split_first() else {
                return Ok(Verdict::Rejected("empty".into()));
            };
            let (host, start, length) = &hosts[usize::from(*selector) % hosts.len()];
            let mut bytes = host.clone();
            let mut record = record.to_vec();
            record.resize(*length, 0);
            bytes[*start..*start + *length].copy_from_slice(&record);
            classify(validate(
                &snapshot(&bytes),
                Selection::Video,
                limits(),
                control(),
            ))
        },
    );
    report.assert_clean();
}

/// Every SPS unit inside each fixture's `hvcC` arrays.
fn hevc_sps_units() -> Vec<Vec<u8>> {
    let mut units = Vec::new();
    for bytes in fixtures(&["mp4"]) {
        let Some((start, length)) = box_body(&bytes, b"hvcC") else {
            continue;
        };
        let record = &bytes[start..start + length];
        let Some(&arrays) = record.get(22) else {
            continue;
        };
        let mut cursor = 23;
        for _ in 0..arrays {
            let Some(header) = record.get(cursor..cursor + 3) else {
                break;
            };
            let kind = header[0] & 0x3f;
            let count = u16::from_be_bytes([header[1], header[2]]);
            cursor += 3;
            for _ in 0..count {
                let Some(size) = record.get(cursor..cursor + 2) else {
                    break;
                };
                let size = usize::from(u16::from_be_bytes([size[0], size[1]]));
                if let Some(unit) = record.get(cursor + 2..cursor + 2 + size)
                    && kind == 33
                {
                    units.push(unit.to_vec());
                }
                cursor += 2 + size;
            }
        }
    }
    units
}

#[test]
fn hevc_sps_parser_survives_mutation() {
    let seeds = hevc_sps_units();
    assert!(!seeds.is_empty(), "HEVC SPS seeds missing");
    let report = fuzz(
        Target::bytes("source-hevc-sps")
            .iterations(3000)
            .max_input_bytes(4096),
        seeds,
        |input| match hevc_sps_geometry(input) {
            Ok(geometry) => {
                if geometry.cropped[0] > geometry.coded[0]
                    || geometry.cropped[1] > geometry.coded[1]
                {
                    return Err("cropped HEVC picture exceeds coded size".into());
                }
                Ok(Verdict::Accepted)
            }
            Err(error) => classify(Err(error)),
        },
    );
    report.assert_clean();
}

/// EBML CodecPrivate payloads from the Matroska fixtures.
fn ffv1_configurations() -> Vec<Vec<u8>> {
    fixtures(&["mkv"])
        .into_iter()
        .filter_map(|bytes| {
            let at = bytes.windows(2).position(|window| window == [0x63, 0xa2])? + 2;
            let marker = *bytes.get(at)?;
            let width = marker.leading_zeros() as usize + 1;
            let mut size = u64::from(marker & (0xff >> width));
            for byte in bytes.get(at + 1..at + width)? {
                size = (size << 8) | u64::from(*byte);
            }
            let start = at + width;
            bytes
                .get(start..start + usize::try_from(size).ok()?)
                .map(<[u8]>::to_vec)
        })
        .collect()
}

#[test]
fn ffv1_configuration_parser_survives_mutation() {
    let seeds = ffv1_configurations();
    assert!(!seeds.is_empty(), "FFV1 configuration seeds missing");
    let report = fuzz(
        Target::bytes("source-ffv1-config")
            .iterations(3000)
            .max_input_bytes(70 * 1024),
        seeds,
        |input| match crate::video_codec::validate_ffv1(input, 8192 * 8192, 8192) {
            Ok(()) => Ok(Verdict::Accepted),
            Err(error) => classify(Err(error)),
        },
    );
    report.assert_clean();
}

#[global_allocator]
static ALLOCATOR: deadpan_chaos::CountingAllocator = deadpan_chaos::CountingAllocator;
