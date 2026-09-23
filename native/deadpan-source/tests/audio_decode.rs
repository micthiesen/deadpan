use deadpan_source::{
    DecodeControl, SourceDecodeError,
    audio::{AudioChannelLayout, AudioDecodeLimits, AudioDecoder, AudioSampleFormat},
};
use std::{
    fs::File,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

static CANCELLED: AtomicBool = AtomicBool::new(false);
fn control() -> DecodeControl<'static> {
    DecodeControl {
        timeout: Duration::from_secs(10),
        cancelled: &CANCELLED,
    }
}
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join(if name.ends_with(".wav") {
            "audio-fixtures"
        } else {
            "fixtures"
        })
        .join(name)
}
fn open_with(name: &str, limits: AudioDecodeLimits) -> Result<AudioDecoder, SourceDecodeError> {
    AudioDecoder::open(
        File::open(fixture(name)).unwrap(),
        u32::from(name.ends_with(".mp4")),
        limits,
        control(),
    )
}
fn open(name: &str) -> AudioDecoder {
    open_with(name, AudioDecodeLimits::default()).unwrap()
}
fn assert_code(error: SourceDecodeError, expected: &str) {
    match error {
        SourceDecodeError::Native { code, message } => assert_eq!(code, expected, "{message}"),
        SourceDecodeError::InvalidConfiguration(message) => {
            assert_eq!(expected, "invalid_configuration", "{message}");
        }
        other => panic!("{other}"),
    }
}

#[test]
fn first_audio_selects_actual_stream_and_preserves_decoded_evidence() {
    for (name, selected) in [
        ("pcm-stereo-48000.wav", 0),
        ("cfr-bframes.mp4", 1),
        ("offset-bframes.mp4", 1),
    ] {
        let mut first = AudioDecoder::open_first(
            File::open(fixture(name)).unwrap(),
            AudioDecodeLimits::default(),
            control(),
        )
        .unwrap();
        let mut explicit = AudioDecoder::open(
            File::open(fixture(name)).unwrap(),
            selected,
            AudioDecodeLimits::default(),
            control(),
        )
        .unwrap();
        assert_eq!(first.info().stream_index, selected);
        assert_eq!(first.info(), explicit.info());
        let mut frames = 0;
        loop {
            let next = first.next_metadata(control()).unwrap();
            assert_eq!(next, explicit.next_metadata(control()).unwrap());
            if next.is_none() {
                break;
            }
            assert_eq!(
                first.copy_current_interleaved_f32(control()).unwrap(),
                explicit.copy_current_interleaved_f32(control()).unwrap()
            );
            frames += 1;
        }
        assert!(frames > 0);
    }
}

#[test]
fn first_audio_does_not_fall_back_to_video_or_relax_admission() {
    assert_code(
        AudioDecoder::open_first(
            File::open(fixture("rotated90.mp4")).unwrap(),
            AudioDecodeLimits::default(),
            control(),
        )
        .err()
        .unwrap(),
        "unsupported_streams",
    );
    assert_code(
        AudioDecoder::open(
            File::open(fixture("cfr-bframes.mp4")).unwrap(),
            0,
            AudioDecodeLimits::default(),
            control(),
        )
        .err()
        .unwrap(),
        "unsupported_streams",
    );
    assert_code(
        AudioDecoder::open(
            File::open(fixture("pcm-stereo-48000.wav")).unwrap(),
            1,
            AudioDecodeLimits::default(),
            control(),
        )
        .err()
        .unwrap(),
        "unsupported_streams",
    );
    assert_code(
        AudioDecoder::open_first(
            File::open(fixture("limited709.mkv")).unwrap(),
            AudioDecodeLimits::default(),
            control(),
        )
        .err()
        .unwrap(),
        "unsupported_container",
    );
    let cancelled = AtomicBool::new(true);
    assert_code(
        AudioDecoder::open_first(
            File::open(fixture("cfr-bframes.mp4")).unwrap(),
            AudioDecodeLimits::default(),
            DecodeControl {
                cancelled: &cancelled,
                ..control()
            },
        )
        .err()
        .unwrap(),
        "cancelled",
    );
    assert_code(
        AudioDecoder::open_first(
            File::open(fixture("cfr-bframes.mp4")).unwrap(),
            AudioDecodeLimits {
                max_io_bytes_per_call: 1,
                ..Default::default()
            },
            control(),
        )
        .err()
        .unwrap(),
        "resource_limit",
    );
    assert_code(
        AudioDecoder::open_first(
            File::open(fixture("cfr-bframes.mp4")).unwrap(),
            AudioDecodeLimits::default(),
            DecodeControl {
                timeout: Duration::ZERO,
                ..control()
            },
        )
        .err()
        .unwrap(),
        "invalid_configuration",
    );
}

#[test]
fn pcm_exact_samples_keep_rate_channel_slots_and_non_aligned_endpoint() {
    for (name, rate, channels, count) in [
        ("pcm-stereo-48000.wav", 48_000, 2, 8197_u32),
        ("pcm-mono-44100.wav", 44_100, 1, 44_117),
    ] {
        let mut decoder = open(name);
        assert_eq!(decoder.info().sample_rate, rate);
        assert_eq!(
            decoder.info().channel_layout,
            AudioChannelLayout::Unspecified { channels }
        );
        assert_eq!(decoder.info().sample_format, AudioSampleFormat::Signed16);
        assert_eq!(
            (decoder.info().time_base_num, decoder.info().time_base_den),
            (1, rate)
        );
        let mut total = 0;
        let mut last_count = 0;
        let mut all = Vec::new();
        while let Some(metadata) = decoder.next_metadata(control()).unwrap() {
            assert_eq!(metadata.pts, i64::from(total));
            assert_eq!(
                metadata.reported_duration,
                Some(i64::from(metadata.nb_samples))
            );
            assert_eq!(metadata.sample_rate, rate);
            assert_eq!(metadata.sample_format, AudioSampleFormat::Signed16);
            let copied = decoder.copy_current_interleaved_f32(control()).unwrap();
            assert_eq!(copied.metadata, metadata);
            assert_eq!(
                copied.samples.len(),
                metadata.nb_samples as usize * channels as usize
            );
            total += metadata.nb_samples;
            last_count = metadata.nb_samples;
            all.extend(copied.samples);
        }
        assert_eq!(total, count);
        assert_ne!(last_count % 1024, 0);
        assert_eq!(decoder.next_metadata(control()).unwrap(), None);
        drop(decoder);
        // Independent byte oracle reads the repository-authored WAVE PCM payload.
        let bytes = std::fs::read(fixture(name)).unwrap();
        assert_eq!(&bytes[36..40], b"data");
        let expected: Vec<f32> = bytes[44..]
            .chunks_exact(2)
            .map(|pair| f32::from(i16::from_le_bytes([pair[0], pair[1]])) / 32768.0)
            .collect();
        assert_eq!(all, expected);
        if channels == 2 {
            assert_eq!(&all[..4], &[0.75, -1.0, -0.75, 32767.0 / 32768.0]);
        }
    }
}

#[test]
fn aac_manual_skip_preserves_priming_duration_tail_and_original_pts() {
    for (name, expected_first, leading, tail, physical) in [
        ("cfr-bframes.mp4", -1024, 1024, 320, 193_536_u64),
        ("offset-bframes.mp4", 95_072, 0, 320, 193_536),
        ("vfr.mp4", -1024, 1024, 640, 386_048),
    ] {
        let mut decoder = open(name);
        assert_eq!(decoder.info().codec, "aac");
        assert_eq!(
            decoder.info().sample_format,
            AudioSampleFormat::Float32Planar
        );
        assert_eq!(
            decoder.info().channel_layout,
            AudioChannelLayout::Native {
                channels: 2,
                mask: 3
            }
        );
        let first = decoder.next_metadata(control()).unwrap().unwrap();
        assert_eq!(first.pts, expected_first);
        assert_eq!(first.skip_samples.map_or(0, |skip| skip.leading), leading);
        assert_eq!(first.discard, leading != 0);
        assert_eq!(first.decode_timestamp, Some(expected_first));
        assert_eq!(decoder.info().initial_padding, 0);
        assert_eq!(decoder.info().trailing_padding, 0);
        assert_eq!(decoder.info().seek_preroll, 0);
        let retained = decoder.copy_current_interleaved_f32(control()).unwrap();
        let original = retained.clone();
        let mut samples = u64::from(first.nb_samples);
        let mut frames = 1;
        let mut last = first;
        while let Some(metadata) = decoder.next_metadata(control()).unwrap() {
            assert_eq!(metadata.pts, last.pts + i64::from(last.nb_samples));
            assert_eq!(metadata.sample_rate, 48_000);
            assert_eq!(metadata.nb_samples, 1024);
            let output = decoder.copy_current_interleaved_f32(control()).unwrap();
            assert!(output.samples.iter().all(|sample| sample.is_finite()));
            samples += u64::from(metadata.nb_samples);
            frames += 1;
            last = metadata;
        }
        assert_eq!(frames, if name == "vfr.mp4" { 377 } else { 189 });
        assert_eq!(samples, physical);
        assert_eq!(1024 - last.reported_duration.unwrap(), tail);
        assert_eq!(last.skip_samples, None);
        assert!(!last.discard);
        drop(decoder);
        assert_eq!(retained, original);
    }
}

#[test]
fn cancelled_and_invalid_requests_preserve_current_frame() {
    let mut decoder = open("pcm-stereo-48000.wav");
    assert_code(
        decoder.copy_current_interleaved_f32(control()).unwrap_err(),
        "invalid_configuration",
    );
    let first = decoder.next_metadata(control()).unwrap().unwrap();
    let cancelled = AtomicBool::new(true);
    let cancelled_control = DecodeControl {
        cancelled: &cancelled,
        ..control()
    };
    assert_code(
        decoder.next_metadata(cancelled_control).unwrap_err(),
        "cancelled",
    );
    assert_code(
        decoder
            .copy_current_interleaved_f32(cancelled_control)
            .unwrap_err(),
        "cancelled",
    );
    let invalid = DecodeControl {
        timeout: Duration::ZERO,
        ..control()
    };
    assert_code(
        decoder.next_metadata(invalid).unwrap_err(),
        "invalid_configuration",
    );
    assert_eq!(
        decoder
            .copy_current_interleaved_f32(control())
            .unwrap()
            .metadata,
        first
    );
    cancelled.store(false, Ordering::Relaxed);
    assert!(decoder.next_metadata(control()).unwrap().is_some());
}

#[test]
fn input_and_decode_budgets_fail_and_poison_failed_contexts() {
    for limits in [
        AudioDecodeLimits {
            max_input_bytes: 1,
            ..AudioDecodeLimits::default()
        },
        AudioDecodeLimits {
            max_io_bytes_per_call: 1,
            ..AudioDecodeLimits::default()
        },
        AudioDecodeLimits {
            max_channels: 1,
            ..AudioDecodeLimits::default()
        },
        AudioDecodeLimits {
            max_sample_rate: 44_100,
            ..AudioDecodeLimits::default()
        },
        AudioDecodeLimits {
            max_packet_bytes: 1,
            ..AudioDecodeLimits::default()
        },
        AudioDecodeLimits {
            max_decoded_samples: 1,
            ..AudioDecodeLimits::default()
        },
    ] {
        assert!(open_with("pcm-stereo-48000.wav", limits).is_err());
    }
    for limits in [
        AudioDecodeLimits {
            max_frames: 1,
            ..AudioDecodeLimits::default()
        },
        AudioDecodeLimits {
            max_packets: 1,
            ..AudioDecodeLimits::default()
        },
    ] {
        let mut decoder = open_with("pcm-stereo-48000.wav", limits).unwrap();
        let failure = loop {
            match decoder.next_metadata(control()) {
                Ok(Some(_)) => continue,
                Ok(None) => panic!("budget did not fail"),
                Err(error) => break error,
            }
        };
        assert_code(failure, "resource_limit");
        assert_code(
            decoder.next_metadata(control()).unwrap_err(),
            "session_failed",
        );
    }
}

#[test]
fn unsupported_selection_and_invalid_sources_fail() {
    for (name, stream) in [("cfr-bframes.mp4", 0), ("cfr-bframes.mp4", 99)] {
        let error = AudioDecoder::open(
            File::open(fixture(name)).unwrap(),
            stream,
            AudioDecodeLimits::default(),
            control(),
        )
        .err()
        .expect("unsupported selection");
        assert_code(error, "unsupported_streams");
    }
    let empty = tempfile::tempfile().unwrap();
    assert_code(
        AudioDecoder::open(empty, 0, AudioDecodeLimits::default(), control())
            .err()
            .unwrap(),
        "invalid_input",
    );
    let limits = AudioDecodeLimits {
        max_channels: 33,
        ..AudioDecodeLimits::default()
    };
    assert_code(limits.validate().unwrap_err(), "invalid_configuration");
}

#[test]
fn retained_descriptor_uses_pread_and_survives_unlinked_path() {
    use std::io::{Seek, SeekFrom, Write};
    let mut snapshot = tempfile::NamedTempFile::new().unwrap();
    snapshot
        .write_all(&std::fs::read(fixture("pcm-stereo-48000.wav")).unwrap())
        .unwrap();
    let mut file = snapshot.reopen().unwrap();
    file.seek(SeekFrom::Start(123)).unwrap();
    let mut cursor = file.try_clone().unwrap();
    let mut decoder = AudioDecoder::open(file, 0, AudioDecodeLimits::default(), control()).unwrap();
    snapshot.close().unwrap();
    assert_eq!(cursor.stream_position().unwrap(), 123);
    let metadata = decoder.next_metadata(control()).unwrap().unwrap();
    let copy = decoder.copy_current_interleaved_f32(control()).unwrap();
    assert_eq!(copy.metadata, metadata);
    assert_eq!(&copy.samples[..4], &[0.75, -1.0, -0.75, 32767.0 / 32768.0]);
    assert_eq!(cursor.stream_position().unwrap(), 123);
    drop(decoder);
    assert_eq!(&copy.samples[..2], &[0.75, -1.0]);
}

#[test]
fn unsupported_codec_garbage_and_truncated_packet_are_rejected() {
    use std::io::Write;
    let original = std::fs::read(fixture("pcm-stereo-48000.wav")).unwrap();
    // WAVE_FORMAT_MULAW selects an audio codec outside the explicit allowlist.
    let mut unsupported = original.clone();
    unsupported[20..22].copy_from_slice(&7_u16.to_le_bytes());
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(&unsupported).unwrap();
    let error = AudioDecoder::open(file, 0, AudioDecodeLimits::default(), control())
        .err()
        .unwrap();
    assert_code(error, "unsupported_codec");

    let mut file = tempfile::tempfile().unwrap();
    file.write_all(b"not a supported media container").unwrap();
    assert!(AudioDecoder::open(file, 0, AudioDecodeLimits::default(), control()).is_err());

    // A half PCM channel value at the declared terminal packet is corruption,
    // not a shorter but acceptable sample endpoint.
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(&original[..original.len() - 1]).unwrap();
    assert!(AudioDecoder::open(file, 0, AudioDecodeLimits::default(), control()).is_err());
}

#[test]
fn wav_packet_sizes_are_bounded_before_demux_allocation() {
    let mut decoder = open_with(
        "pcm-stereo-48000.wav",
        AudioDecodeLimits {
            max_packet_bytes: 64,
            max_samples_per_frame: 8,
            ..AudioDecodeLimits::default()
        },
    )
    .unwrap();
    let mut total = 0;
    while let Some(frame) = decoder.next_metadata(control()).unwrap() {
        assert!(frame.nb_samples <= 8);
        let copied = decoder.copy_current_interleaved_f32(control()).unwrap();
        assert_eq!(copied.samples.len(), frame.nb_samples as usize * 2);
        total += frame.nb_samples;
    }
    assert_eq!(total, 8197);
    let error = AudioDecoder::open(
        File::open(fixture("full709.mkv")).unwrap(),
        0,
        AudioDecodeLimits::default(),
        control(),
    )
    .err()
    .unwrap();
    assert_code(error, "unsupported_container");
}

#[test]
fn allocation_deadline_does_not_advance_retained_audio() {
    let mut decoder = open("pcm-stereo-48000.wav");
    let first = decoder.next_metadata(control()).unwrap().unwrap();
    assert_code(
        decoder
            .copy_current_interleaved_f32(DecodeControl {
                timeout: Duration::from_nanos(1),
                ..control()
            })
            .unwrap_err(),
        "deadline_exceeded",
    );
    assert_eq!(
        decoder
            .copy_current_interleaved_f32(control())
            .unwrap()
            .metadata,
        first
    );
}
