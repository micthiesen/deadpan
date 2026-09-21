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
