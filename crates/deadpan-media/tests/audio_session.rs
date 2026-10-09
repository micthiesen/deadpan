#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::io::Cursor;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use deadpan_core::{AssetId, SourceFrameId};
use deadpan_media::ConversionError;
use deadpan_media::audio_index::AudioIndexSnapshot;
use deadpan_media::audio_session::{
    AudioSession, AudioSessionError, AudioSessionLimits, SourceAudioSample,
};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_media::source_input::VerifiedSourceInput;
use deadpan_media::source_session::{SourceSession, SourceSessionLimits};
use sha2::{Digest, Sha256};

fn fixture(folder: &str, name: &str) -> Vec<u8> {
    std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../native/deadpan-source/tests")
            .join(folder)
            .join(name),
    )
    .unwrap()
}

fn identity(bytes: &[u8]) -> SourceContentIdentity {
    SourceContentIdentity::new(Sha256::digest(bytes).into(), bytes.len() as u64).unwrap()
}

fn open(bytes: &[u8], stream: u32) -> AudioSession {
    AudioSession::open_verified(
        &mut Cursor::new(bytes),
        identity(bytes),
        stream,
        AudioSessionLimits::default(),
        &AtomicBool::new(false),
    )
    .unwrap()
}

fn verified_input(bytes: &[u8]) -> VerifiedSourceInput {
    VerifiedSourceInput::copy_verified(
        &mut Cursor::new(bytes),
        identity(bytes),
        bytes.len() as u64,
        Duration::from_secs(10),
        &AtomicBool::new(false),
    )
    .unwrap()
}

#[test]
fn automatic_audio_selection_retains_exact_index_and_samples() {
    for (folder, name, stream, start) in [
        ("audio-fixtures", "pcm-stereo-48000.wav", 0, 0),
        ("fixtures", "cfr-bframes.mp4", 1, 0),
        ("fixtures", "offset-bframes.mp4", 1, 95072),
    ] {
        let bytes = fixture(folder, name);
        let automatic = AudioSession::open_first_input(
            verified_input(&bytes),
            AudioSessionLimits::default(),
            &AtomicBool::new(false),
        )
        .unwrap();
        let explicit = open(&bytes, stream);
        assert_eq!(automatic.index().stream().stream_index, stream);
        assert_eq!(automatic.index(), explicit.index());
        assert_eq!(
            automatic
                .read_samples(
                    SourceAudioSample(start),
                    100,
                    Duration::from_secs(2),
                    &AtomicBool::new(false)
                )
                .unwrap(),
            explicit
                .read_samples(
                    SourceAudioSample(start),
                    100,
                    Duration::from_secs(2),
                    &AtomicBool::new(false)
                )
                .unwrap()
        );
    }
}

#[test]
fn opus_sample_clock_retains_raw_ticks_and_exact_padding_without_cumulative_drift() {
    for (name, start, stream) in [
        ("stereo-20", 0, 0),
        ("mono-2.5", 0, 0),
        ("mono-60", 0, 0),
        ("stereo-120", 0, 0),
        ("mono-silk", 0, 0),
        ("mono-hybrid", 0, 0),
        ("mono-preskip", 0, 0),
        ("av", 0, 1),
        ("av-offset", 6048, 1),
    ] {
        let bytes = fixture("audio-fixtures", &format!("opus-{name}.webm"));
        let available = if name == "mono-preskip" { 7717 } else { 8197 };
        let session = open(&bytes, stream);
        let index = session.index();
        let clock = index.stream().matroska_opus.unwrap();
        assert_eq!(index.observations().len() as u64, clock.packet_count);
        assert_eq!(index.decoded_samples(), clock.decoded_sample_count);
        assert_eq!(index.valid_samples(), available as u64);
        assert_eq!(
            index.frames().first().unwrap().source_start,
            start - i64::from(clock.pre_skip)
        );
        assert_eq!(
            index
                .frames()
                .iter()
                .find(|f| f.valid_start < f.valid_end)
                .unwrap()
                .valid_start,
            start
        );
        assert_eq!(index.frames().last().unwrap().valid_end, start + available);
        assert_eq!(
            AudioIndexSnapshot::from_json(&index.to_json().unwrap()).unwrap(),
            *index
        );
        let samples = session
            .read_samples(
                SourceAudioSample(start),
                available as u32,
                Duration::from_secs(2),
                &AtomicBool::new(false),
            )
            .unwrap()
            .samples;
        let reference = fixture("audio-fixtures", &format!("opus-{name}.f32le"));
        assert_eq!(samples.len() * 4, reference.len());
        for (actual, expected) in samples.iter().zip(reference.chunks_exact(4)) {
            let expected = f32::from_le_bytes(expected.try_into().unwrap());
            assert!(
                (actual - expected).abs() < 0.0001,
                "{name}: {actual} != {expected}"
            );
        }
        assert!(
            session
                .read_samples(
                    SourceAudioSample(start - 1),
                    1,
                    Duration::from_secs(2),
                    &AtomicBool::new(false)
                )
                .is_err()
        );
        assert!(
            session
                .read_samples(
                    SourceAudioSample(start + available),
                    1,
                    Duration::from_secs(2),
                    &AtomicBool::new(false)
                )
                .is_err()
        );
        // Tampered receipts must not turn real discontinuities into continuous PCM.
        let mut value = serde_json::to_value(index).unwrap();
        value["observations"][1]["pts"] = serde_json::json!(index.observations()[1].pts + 2);
        assert!(
            AudioIndexSnapshot::from_json(&serde_json::to_vec(&value).unwrap()).is_err(),
            "{name}"
        );
        let mut value = serde_json::to_value(index).unwrap();
        value["stream"]["matroska_opus"]["codec_delay_ns"] =
            serde_json::json!(clock.codec_delay_ns + 1000);
        assert!(AudioIndexSnapshot::from_json(&serde_json::to_vec(&value).unwrap()).is_err());
    }
}

#[test]
fn long_opus_clock_does_not_accumulate_rounding_and_cross_packet_preskip_remains_unavailable() {
    use deadpan_media::audio_index::{AudioFrameObservation, AudioSkipSamples};
    let source = fixture("audio-fixtures", "opus-mono-2.5.webm");
    let session = open(&source, 0);
    let mut stream = session.index().stream().clone();
    let count = 100_000_u64;
    let clock = stream.matroska_opus.as_mut().unwrap();
    clock.packet_count = count;
    clock.decoded_sample_count = count * 120;
    clock.pre_skip = 600;
    clock.codec_delay_ns = 12_500_000;
    stream.initial_padding = 600;
    let observations: Vec<_> = (0..count)
        .map(|i| AudioFrameObservation {
            pts: (i * 5).div_ceil(2) as i64 - 13,
            discard: false,
            decode_timestamp: None,
            reported_duration: Some(2),
            sample_count: 120,
            sample_format: "flt".into(),
            skip_samples: Some(AudioSkipSamples {
                leading: if i == 0 { 600 } else { 0 },
                trailing: if i + 1 == count { 83 } else { 0 },
                leading_reason: 0,
                trailing_reason: 0,
            }),
        })
        .collect();
    let index =
        AudioIndexSnapshot::new(identity(&source), stream.clone(), observations.clone()).unwrap();
    assert_eq!(index.valid_samples(), count * 120 - 600 - 83);
    assert_eq!(index.frames()[5].valid_start, 0);
    assert_eq!(
        index.frames().last().unwrap().valid_end,
        (count * 120 - 683) as i64
    );
    for f in &index.frames()[..5] {
        assert_eq!(f.valid_start, f.valid_end);
    }
    let mut drift = observations;
    for (i, frame) in drift.iter_mut().enumerate() {
        frame.pts += (i / 1000) as i64;
    }
    assert!(AudioIndexSnapshot::new(identity(&source), stream, drift).is_err());
}

#[test]
fn automatic_audio_selection_preserves_absence_cancellation_and_cache_limits() {
    let no_audio = fixture("fixtures", "rotated90.mp4");
    assert!(
        matches!(AudioSession::open_first_input(verified_input(&no_audio), AudioSessionLimits::default(), &AtomicBool::new(false)), Err(AudioSessionError::Native(deadpan_source::SourceDecodeError::Native { code, .. })) if code == "unsupported_streams")
    );
    let wav = fixture("audio-fixtures", "pcm-stereo-48000.wav");
    assert!(matches!(
        AudioSession::open_first_input(
            verified_input(&wav),
            AudioSessionLimits::default(),
            &AtomicBool::new(true)
        ),
        Err(AudioSessionError::Snapshot(ConversionError::Cancelled))
    ));
    assert!(matches!(
        AudioSession::open_first_input(
            verified_input(&wav),
            AudioSessionLimits {
                maximum_cache_bytes: 1,
                ..Default::default()
            },
            &AtomicBool::new(false)
        ),
        Err(AudioSessionError::Limits(_))
    ));
    let mp4 = fixture("fixtures", "cfr-bframes.mp4");
    assert!(
        matches!(AudioSession::open_input(verified_input(&mp4), 0, AudioSessionLimits::default(), &AtomicBool::new(false)), Err(AudioSessionError::Native(deadpan_source::SourceDecodeError::Native { code, .. })) if code == "unsupported_streams")
    );
}

fn stereo_sample(index: usize) -> [f32; 2] {
    let left = match index % 2048 {
        0 => 24576,
        1 => -24576,
        512..=1023 => ((index * 97) % 16384) as i32 - 8192,
        _ => 0,
    };
    let right = match index % 257 {
        0 => -32768,
        1 => 32767,
        _ => (index % 97) as i32 * 3 - 48 * 3,
    };
    [left as f32 / 32768.0, right as f32 / 32768.0]
}

#[test]
fn pcm_ranges_preserve_exact_amplitudes_channel_order_original_rate_and_terminal_samples() {
    for (name, rate, count, channels) in [
        ("pcm-stereo-48000.wav", 48000, 8197, 2),
        ("pcm-mono-44100.wav", 44100, 44117, 1),
    ] {
        let mut bytes = fixture("audio-fixtures", name);
        let session = open(&bytes, 0);
        bytes.fill(0);
        let index = session.index();
        assert_eq!(index.decoded_samples(), count as u64);
        assert_eq!(index.valid_samples(), count as u64);
        assert_eq!(index.stream().sample_rate, rate);
        assert_eq!(index.frames().last().unwrap().valid_end, count as i64);
        assert_eq!(
            AudioIndexSnapshot::from_json(&index.to_json().unwrap()).unwrap(),
            *index
        );
        for (start, length) in [(0, 17), (4090, 31), (count - 5, 5), (0, count)] {
            let block = session
                .read_samples(
                    SourceAudioSample(start as i64),
                    length as u32,
                    Duration::from_secs(2),
                    &AtomicBool::new(false),
                )
                .unwrap();
            assert_eq!(block.sample_rate, rate);
            assert_eq!(block.samples.len(), length * channels);
            for (offset, frame) in block.samples.chunks_exact(channels).enumerate() {
                let sample = start + offset;
                if channels == 2 {
                    assert_eq!(frame, stereo_sample(sample));
                } else {
                    assert_eq!(
                        frame,
                        [(((sample * 73) % 65536) as i32 - 32768) as f32 / 32768.0]
                    );
                }
            }
        }
        for (start, length) in [(-1, 1), (count as i64, 1), (count as i64 - 1, 2)] {
            assert!(matches!(
                session.read_samples(
                    SourceAudioSample(start),
                    length,
                    Duration::from_secs(1),
                    &AtomicBool::new(false)
                ),
                Err(AudioSessionError::UnavailableRange)
            ));
        }
        let last = session
            .read_samples(
                SourceAudioSample(count as i64 - 1),
                1,
                Duration::from_secs(1),
                &AtomicBool::new(false),
            )
            .unwrap();
        drop(session);
        assert_eq!(last.samples.len(), channels);
    }
}

#[test]
fn aac_index_separates_raw_decode_skip_duration_padding_and_unknown_offset_priming() {
    for (name, decoded, valid, start, end, leading, tail) in [
        (
            "cfr-bframes.mp4",
            193536,
            192192,
            0,
            192192,
            Some(1024),
            320,
        ),
        (
            "offset-bframes.mp4",
            193536,
            193216,
            95072,
            288288,
            None,
            320,
        ),
        ("vfr.mp4", 386048, 384384, 0, 384384, Some(1024), 640),
    ] {
        let bytes = fixture("fixtures", name);
        let session = open(&bytes, 1);
        let index = session.index();
        assert_eq!(index.decoded_samples(), decoded);
        assert_eq!(index.valid_samples(), valid);
        assert_eq!(
            index.observations()[0]
                .skip_samples
                .map(|skip| skip.leading),
            leading
        );
        let last = index.observations().last().unwrap();
        assert_eq!(
            i64::from(last.sample_count) - last.reported_duration.unwrap(),
            tail
        );
        assert_eq!(
            index
                .frames()
                .iter()
                .find(|frame| frame.valid_end > frame.valid_start)
                .unwrap()
                .valid_start,
            start
        );
        assert_eq!(index.frames().last().unwrap().valid_end, end);
        let block = session
            .read_samples(
                SourceAudioSample(start + 1000),
                100,
                Duration::from_secs(1),
                &AtomicBool::new(false),
            )
            .unwrap();
        let first = session
            .read_samples(
                SourceAudioSample(start + 1000),
                24,
                Duration::from_secs(1),
                &AtomicBool::new(false),
            )
            .unwrap();
        let second = session
            .read_samples(
                SourceAudioSample(start + 1024),
                76,
                Duration::from_secs(1),
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(block.samples, [first.samples, second.samples].concat());
        assert!(matches!(
            session.read_samples(
                SourceAudioSample(end - 1),
                2,
                Duration::from_secs(1),
                &AtomicBool::new(false)
            ),
            Err(AudioSessionError::UnavailableRange)
        ));
        assert!(matches!(
            session.read_samples(
                SourceAudioSample(start - 1),
                1,
                Duration::from_secs(1),
                &AtomicBool::new(false)
            ),
            Err(AudioSessionError::UnavailableRange)
        ));
        assert_eq!(
            AudioIndexSnapshot::from_json(&index.to_json().unwrap()).unwrap(),
            *index
        );
    }
}

#[test]
fn audio_and_video_share_one_verified_input_and_remain_independent() {
    let mut bytes = fixture("fixtures", "cfr-bframes.mp4");
    let input = VerifiedSourceInput::copy_verified(
        &mut Cursor::new(&bytes),
        identity(&bytes),
        bytes.len() as u64,
        Duration::from_secs(2),
        &AtomicBool::new(false),
    )
    .unwrap();
    bytes.fill(0);
    let audio = AudioSession::open_input(
        input.clone(),
        1,
        AudioSessionLimits::default(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let mut video = SourceSession::open_input(
        input,
        AssetId::new("source").unwrap(),
        SourceSessionLimits::default(),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(audio.index().content(), video.index().content());
    let before = audio
        .read_samples(
            SourceAudioSample(4090),
            100,
            Duration::from_secs(2),
            &AtomicBool::new(false),
        )
        .unwrap();
    video
        .frame(
            SourceFrameId(119),
            Duration::from_secs(2),
            &AtomicBool::new(false),
        )
        .unwrap();
    video
        .frame(
            SourceFrameId(0),
            Duration::from_secs(2),
            &AtomicBool::new(false),
        )
        .unwrap();
    drop(video);
    assert_eq!(
        before,
        audio
            .read_samples(
                SourceAudioSample(4090),
                100,
                Duration::from_secs(2),
                &AtomicBool::new(false)
            )
            .unwrap()
    );
}

#[test]
fn limits_identity_and_cancellation_never_publish_partial_audio() {
    let bytes = fixture("audio-fixtures", "pcm-stereo-48000.wav");
    let content = identity(&bytes);
    for limits in [
        AudioSessionLimits {
            maximum_cache_bytes: 1,
            ..Default::default()
        },
        AudioSessionLimits {
            maximum_index_frames: 1,
            ..Default::default()
        },
    ] {
        assert!(matches!(
            AudioSession::open_verified(
                &mut Cursor::new(&bytes),
                content,
                0,
                limits,
                &AtomicBool::new(false)
            ),
            Err(AudioSessionError::Limits(_))
        ));
    }
    assert!(matches!(
        AudioSession::open_verified(
            &mut Cursor::new(&bytes),
            SourceContentIdentity::new([0; 32], content.byte_length()).unwrap(),
            0,
            AudioSessionLimits::default(),
            &AtomicBool::new(false)
        ),
        Err(AudioSessionError::Snapshot(ConversionError::InputIdentity))
    ));
    struct NoRead;
    impl std::io::Read for NoRead {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            panic!("preflight must run before copy")
        }
    }
    assert!(matches!(
        AudioSession::open_verified(
            &mut NoRead,
            content,
            0,
            AudioSessionLimits::default(),
            &AtomicBool::new(true)
        ),
        Err(AudioSessionError::Snapshot(ConversionError::Cancelled))
    ));
    for limits in [
        AudioSessionLimits {
            maximum_read_frames: 0,
            ..Default::default()
        },
        AudioSessionLimits {
            opening_timeout: Duration::ZERO,
            ..Default::default()
        },
        AudioSessionLimits {
            maximum_cache_bytes: 0,
            ..Default::default()
        },
    ] {
        assert!(matches!(
            AudioSession::open_verified(&mut NoRead, content, 0, limits, &AtomicBool::new(false)),
            Err(AudioSessionError::Limits(_))
        ));
    }
    let session = open(&bytes, 0);
    assert!(matches!(
        session.read_samples(
            SourceAudioSample(0),
            1,
            Duration::from_secs(1),
            &AtomicBool::new(true)
        ),
        Err(AudioSessionError::Snapshot(ConversionError::Cancelled))
    ));
    assert!(
        session
            .read_samples(
                SourceAudioSample(0),
                1,
                Duration::from_secs(1),
                &AtomicBool::new(false)
            )
            .is_ok()
    );
    for (start, count) in [(0, 0), (0, 65537), (i64::MAX, 1)] {
        assert!(
            session
                .read_samples(
                    SourceAudioSample(start),
                    count,
                    Duration::from_secs(1),
                    &AtomicBool::new(false)
                )
                .is_err()
        );
    }
}

#[test]
fn mp3_catalog_pcm_preserves_exact_trim_and_refuses_forged_clock_or_skip_receipts() {
    let manifest: serde_json::Value =
        serde_json::from_slice(&fixture("audio-fixtures", "mp3-manifest.json")).unwrap();
    for row in manifest["files"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["name"].as_str().unwrap().ends_with(".mp3"))
    {
        let name = row["name"].as_str().unwrap();
        let bytes = fixture("audio-fixtures", name);
        let session = AudioSession::open_first_input(
            verified_input(&bytes),
            AudioSessionLimits::default(),
            &AtomicBool::new(false),
        )
        .unwrap_or_else(|e| panic!("{name}: {e}"));
        let index = session.index();
        let framing = index.stream().mp3.unwrap();
        let available = row["available_samples"].as_u64().unwrap();
        let start = i64::from(framing.leading_skip);
        assert_eq!(index.valid_samples(), available, "{name}");
        assert_eq!(
            index.decoded_samples(),
            framing.frame_count * u64::from(framing.samples_per_frame)
        );
        assert_eq!(
            AudioIndexSnapshot::from_json(&index.to_json().unwrap()).unwrap(),
            *index
        );
        let actual = session
            .read_samples(
                SourceAudioSample(start),
                available as u32,
                Duration::from_secs(2),
                &AtomicBool::new(false),
            )
            .unwrap()
            .samples;
        let reference = fixture("audio-fixtures", &name.replace(".mp3", ".f32le"));
        assert_eq!(actual.len() * 4, reference.len());
        for (a, b) in actual.iter().zip(reference.chunks_exact(4)) {
            assert!(
                (a - f32::from_le_bytes(b.try_into().unwrap())).abs() < 0.00001,
                "{name}"
            );
        }
        for boundary in [start - 1, start + available as i64] {
            assert!(
                session
                    .read_samples(
                        SourceAudioSample(boundary),
                        1,
                        Duration::from_secs(2),
                        &AtomicBool::new(false)
                    )
                    .is_err(),
                "{name}: {boundary}"
            );
        }
        // Parsed JSON has to re-establish the same clock and trim contract.
        for mutation in 0..10 {
            let mut wire = serde_json::to_value(index).unwrap();
            match mutation {
                0 => wire["stream"]["mp3"] = serde_json::Value::Null,
                1 => wire["stream"]["mp3"]["frame_count"] = (framing.frame_count + 1).into(),
                2 => wire["stream"]["mp3"]["leading_skip"] = (framing.leading_skip + 1).into(),
                3 => wire["stream"]["mp3"]["trailing_skip"] = (framing.trailing_skip + 1).into(),
                4 => wire["observations"][1]["pts"] = (index.observations()[1].pts + 1).into(),
                5 => wire["observations"][0]["discard"] = true.into(),
                6 => {
                    wire["observations"][0]["sample_count"] = (framing.samples_per_frame - 1).into()
                }
                7 => wire["observations"][0]["reported_duration"] = 1.into(),
                8 => wire["stream"]["stream_index"] = 1.into(),
                9 => wire["stream"]["stream_start"] = serde_json::Value::Null,
                _ => unreachable!(),
            }
            assert!(
                AudioIndexSnapshot::from_json(&serde_json::to_vec(&wire).unwrap()).is_err(),
                "{name} mutation {mutation}"
            );
        }
    }
}
