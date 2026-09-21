#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::io::Cursor;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use deadpan_audio::{PreparationError, PreparedSource, ResampleRecipe};
use deadpan_core::{AudioSample, ExactRatio};
use deadpan_media::audio_index::{AudioChannelLayout, AudioIndexSnapshot};
use deadpan_media::audio_session::{
    AudioSession, AudioSessionError, AudioSessionLimits, SourceAudioSample,
};
use deadpan_media::source_index::SourceContentIdentity;
use sha2::{Digest, Sha256};

const READ_TIMEOUT: Duration = Duration::from_secs(2);

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../native/deadpan-source/tests/audio-fixtures")
            .join(name),
    )
    .unwrap()
}

fn open(bytes: &[u8], maximum_read_frames: u32) -> AudioSession {
    AudioSession::open_verified(
        &mut Cursor::new(bytes),
        SourceContentIdentity::new(Sha256::digest(bytes).into(), bytes.len() as u64).unwrap(),
        0,
        AudioSessionLimits {
            maximum_read_frames,
            ..AudioSessionLimits::default()
        },
        &AtomicBool::new(false),
    )
    .unwrap()
}

fn prepared(name: &str, maximum_read_frames: u32) -> PreparedSource {
    let mut bytes = fixture(name);
    let session = open(&bytes, maximum_read_frames);
    let expected = session.index().clone();
    // The retained private session must not depend on the caller's buffer.
    bytes.fill(0);
    // Plain PCM16 WAV carries no speaker mask. These explicit interpretations
    // come from the repository fixture recipes, never inferred channel counts.
    let layout = match name {
        "pcm-stereo-48000.wav" => AudioChannelLayout::Native {
            channels: 2,
            mask: 3,
        },
        "pcm-mono-44100.wav" => AudioChannelLayout::Native {
            channels: 1,
            mask: 4,
        },
        _ => panic!("fixture has no declared speaker interpretation"),
    };
    PreparedSource::with_layout(session, &expected, layout, &AtomicBool::new(false)).unwrap()
}

fn stereo_sample(index: usize) -> [f32; 2] {
    // Independent source fixture recipe, preserved by native PCM conversion.
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
fn verified_stereo_unity_preserves_actual_fixture_amplitudes_and_terminal_samples() {
    let source = prepared("pcm-stereo-48000.wav", 256);
    assert_eq!(source.index().stream().sample_rate, 48_000);
    assert_eq!(source.index().valid_samples(), 8197);
    let recipe = ResampleRecipe::new(
        0..8197,
        ExactRatio::ZERO,
        AudioSample(0),
        ExactRatio::ONE,
        AudioSample(0)..AudioSample(8197),
    )
    .unwrap();
    for (start, frames) in [(0, 256), (4090, 31), (8192, 5), (8196, 1)] {
        let block = source
            .prepare(
                recipe.clone(),
                AudioSample(start),
                frames,
                READ_TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(block.start, AudioSample(start));
        assert_eq!(block.samples.len(), frames as usize);
        for (offset, frame) in block.samples.iter().enumerate() {
            assert_eq!(*frame, stereo_sample(start as usize + offset));
        }
    }
}

#[test]
fn mono_44100_to_mix_rate_keeps_fractional_origin_across_partitions_and_random_seeks() {
    let source = prepared("pcm-mono-44100.wav", 65_536);
    assert_eq!(source.index().stream().sample_rate, 44_100);
    let recipe = ResampleRecipe::new(
        300..44_000,
        ExactRatio::new(100_003, 7).unwrap(),
        AudioSample(209),
        ExactRatio::new(147, 160).unwrap(),
        AudioSample(0)..AudioSample(2000),
    )
    .unwrap();
    let render = |start, frames| {
        source
            .prepare(
                recipe.clone(),
                AudioSample(start),
                frames,
                READ_TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap()
            .samples
    };
    let expected = render(189, 256);
    assert!(expected.iter().any(|sample| sample[0].abs() > 0.1));
    for sample in &expected {
        assert_eq!(sample[0], sample[1]);
        assert!(sample[0].is_finite());
    }
    let mut partitioned = Vec::new();
    for count in [1, 7, 31, 100, 117] {
        partitioned.extend(render(189 + partitioned.len() as i64, count));
    }
    assert_eq!(partitioned, expected);
    for (offset, count) in [(199, 57), (0, 1), (91, 31), (255, 1), (4, 17)] {
        assert_eq!(
            render(189 + offset as i64, count),
            expected[offset..offset + count as usize]
        );
    }
}

#[test]
fn filter_context_reads_only_the_authored_trim_and_unavailable_selected_samples_fail() {
    // A fractional-phase kernel needs much more context than seven frames.
    // Success under this physical read bound proves the halo is clipped to
    // the authored seven-frame trim, including at the actual file endpoint.
    let source = prepared("pcm-stereo-48000.wav", 7);
    let recipe = ResampleRecipe::new(
        8190..8197,
        ExactRatio::new(16_381, 2).unwrap(),
        AudioSample(0),
        ExactRatio::new(147, 160).unwrap(),
        AudioSample(0)..AudioSample(8),
    )
    .unwrap();
    let block = source
        .prepare(
            recipe,
            AudioSample(0),
            8,
            READ_TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(block.samples.len(), 8);
    assert!(
        block
            .samples
            .iter()
            .flatten()
            .all(|value| value.is_finite())
    );

    let source = prepared("pcm-stereo-48000.wav", 65_536);
    let invalid_coverage = ResampleRecipe::new(
        8190..8198,
        ExactRatio::new(16_381, 2).unwrap(),
        AudioSample(0),
        ExactRatio::new(147, 160).unwrap(),
        AudioSample(0)..AudioSample(8),
    )
    .unwrap();
    assert!(matches!(
        source.prepare(
            invalid_coverage,
            AudioSample(0),
            8,
            READ_TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(PreparationError::Session(
            AudioSessionError::UnavailableRange
        ))
    ));
}

#[test]
fn private_media_read_limits_and_preparation_limits_are_not_silently_relaxed() {
    let source = prepared("pcm-stereo-48000.wav", 7);
    let recipe = ResampleRecipe::new(
        0..8197,
        ExactRatio::new(4097, 2).unwrap(),
        AudioSample(0),
        ExactRatio::ONE,
        AudioSample(0)..AudioSample(1000),
    )
    .unwrap();
    assert!(matches!(
        source.prepare(
            recipe.clone(),
            AudioSample(0),
            1,
            READ_TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(PreparationError::Session(AudioSessionError::Limits(_)))
    ));
    for timeout in [Duration::ZERO, Duration::from_secs(61)] {
        assert!(matches!(
            source.prepare(
                recipe.clone(),
                AudioSample(0),
                1,
                timeout,
                &AtomicBool::new(false)
            ),
            Err(PreparationError::InvalidRecipe(_))
        ));
    }
    assert!(matches!(
        source.prepare(
            recipe,
            AudioSample(0),
            257,
            READ_TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(PreparationError::InvalidRecipe(_))
    ));
}

#[test]
fn admission_rechecks_identity_stream_and_raw_observations_before_exposing_pcm() {
    let bytes = fixture("pcm-stereo-48000.wav");
    let baseline = open(&bytes, 65_536).index().clone();
    let changed_identity = AudioIndexSnapshot::new(
        SourceContentIdentity::new([17; 32], bytes.len() as u64).unwrap(),
        baseline.stream().clone(),
        baseline.observations().to_vec(),
    )
    .unwrap();
    let mut changed_stream = baseline.stream().clone();
    changed_stream.initial_padding += 1;
    let changed_stream = AudioIndexSnapshot::new(
        baseline.content(),
        changed_stream,
        baseline.observations().to_vec(),
    )
    .unwrap();
    let mut changed_observations = baseline.observations().to_vec();
    changed_observations[0].decode_timestamp = Some(123);
    let changed_observations = AudioIndexSnapshot::new(
        baseline.content(),
        baseline.stream().clone(),
        changed_observations,
    )
    .unwrap();
    // Metadata drift can leave all derived coordinates and counts unchanged.
    assert_eq!(changed_observations.frames(), baseline.frames());
    assert_eq!(
        changed_observations.decoded_samples(),
        baseline.decoded_samples()
    );
    let mut changed_endpoint = baseline.observations().to_vec();
    *changed_endpoint
        .last_mut()
        .unwrap()
        .reported_duration
        .as_mut()
        .unwrap() -= 1;
    let changed_endpoint = AudioIndexSnapshot::new(
        baseline.content(),
        baseline.stream().clone(),
        changed_endpoint,
    )
    .unwrap();
    for expected in [
        changed_identity,
        changed_stream,
        changed_observations,
        changed_endpoint,
    ] {
        assert!(matches!(
            PreparedSource::new(open(&bytes, 65_536), &expected, &AtomicBool::new(false)),
            Err(PreparationError::IndexMismatch)
        ));
    }
}

#[test]
fn cancellation_prevents_admission_and_block_preparation_without_poisoning_the_source() {
    let bytes = fixture("pcm-stereo-48000.wav");
    let session = open(&bytes, 65_536);
    let expected = session.index().clone();
    let cancelled = AtomicBool::new(true);
    assert!(matches!(
        PreparedSource::new(session, &expected, &cancelled),
        Err(PreparationError::Cancelled)
    ));
    let source = PreparedSource::with_layout(
        open(&bytes, 65_536),
        &expected,
        AudioChannelLayout::Native {
            channels: 2,
            mask: 3,
        },
        &AtomicBool::new(false),
    )
    .unwrap();
    let recipe = ResampleRecipe::new(
        0..8197,
        ExactRatio::ZERO,
        AudioSample(0),
        ExactRatio::ONE,
        AudioSample(0)..AudioSample(8197),
    )
    .unwrap();
    assert!(matches!(
        source.prepare(recipe.clone(), AudioSample(0), 1, READ_TIMEOUT, &cancelled),
        Err(PreparationError::Cancelled)
    ));
    cancelled.store(false, Ordering::Relaxed);
    assert_eq!(
        source
            .prepare(recipe, AudioSample(0), 1, READ_TIMEOUT, &cancelled)
            .unwrap()
            .samples,
        [stereo_sample(0)]
    );
}

#[test]
fn unspecified_original_layouts_require_explicit_consistent_host_interpretation() {
    for name in ["pcm-stereo-48000.wav", "pcm-mono-44100.wav"] {
        let bytes = fixture(name);
        let session = open(&bytes, 65_536);
        let expected = session.index().clone();
        assert!(matches!(
            expected.stream().channel_layout,
            AudioChannelLayout::Unspecified { .. }
        ));
        assert!(matches!(
            PreparedSource::new(session, &expected, &AtomicBool::new(false)),
            Err(PreparationError::UnsupportedLayout)
        ));
        let mismatched_count = AudioChannelLayout::Native {
            channels: 3,
            mask: 7,
        };
        assert!(matches!(
            PreparedSource::with_layout(
                open(&bytes, 65_536),
                &expected,
                mismatched_count,
                &AtomicBool::new(false)
            ),
            Err(PreparationError::UnsupportedLayout)
        ));
        let source = prepared(name, 65_536);
        assert_eq!(source.index(), &expected);
        let declared_mask = match name {
            "pcm-stereo-48000.wav" => 3,
            "pcm-mono-44100.wav" => 4,
            _ => unreachable!(),
        };
        assert_eq!(
            source.matrix_layout(),
            AudioChannelLayout::Native {
                channels: expected.stream().channel_layout.channels(),
                mask: declared_mask,
            }
        );
    }
}

#[test]
fn actual_native_aac_layout_cannot_be_reinterpreted_as_different_speakers() {
    let bytes = std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../native/deadpan-source/tests/fixtures/cfr-bframes.mp4"),
    )
    .unwrap();
    let open = || {
        AudioSession::open_verified(
            &mut Cursor::new(&bytes),
            SourceContentIdentity::new(Sha256::digest(&bytes).into(), bytes.len() as u64).unwrap(),
            1,
            AudioSessionLimits::default(),
            &AtomicBool::new(false),
        )
        .unwrap()
    };
    let session = open();
    let expected = session.index().clone();
    assert_eq!(
        expected.stream().channel_layout,
        AudioChannelLayout::Native {
            channels: 2,
            mask: 3,
        }
    );
    let source = PreparedSource::new(session, &expected, &AtomicBool::new(false)).unwrap();
    assert_eq!(source.matrix_layout(), expected.stream().channel_layout);
    let original = open()
        .read_samples(
            SourceAudioSample(0),
            256,
            READ_TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    let rendered = source
        .prepare(
            ResampleRecipe::new(
                0..192_192,
                ExactRatio::ZERO,
                AudioSample(0),
                ExactRatio::ONE,
                AudioSample(0)..AudioSample(256),
            )
            .unwrap(),
            AudioSample(0),
            256,
            READ_TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert!(original.samples.iter().any(|sample| sample.abs() > 0.01));
    assert_eq!(
        rendered.samples.into_iter().flatten().collect::<Vec<_>>(),
        original.samples
    );
    assert!(matches!(
        PreparedSource::with_layout(
            open(),
            &expected,
            AudioChannelLayout::Native {
                channels: 2,
                mask: 0x30,
            },
            &AtomicBool::new(false)
        ),
        Err(PreparationError::UnsupportedLayout)
    ));
}
