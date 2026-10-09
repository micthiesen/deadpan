use deadpan_source::{
    DecodeControl, SourceDecodeError,
    audio::{
        AudioChannelLayout, AudioDecodeLimits, AudioDecodeMode, AudioDecoder, AudioSampleFormat,
    },
};
use std::{fs::File, io::Write, path::PathBuf, sync::atomic::AtomicBool, time::Duration};

static CANCELLED: AtomicBool = AtomicBool::new(false);
fn control() -> DecodeControl<'static> {
    DecodeControl {
        timeout: Duration::from_secs(10),
        cancelled: &CANCELLED,
    }
}
fn fixture(name: &str, extension: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/audio-fixtures")
        .join(format!("wide-{name}.{extension}"))
}
fn from_bytes(bytes: &[u8], limits: AudioDecodeLimits) -> Result<AudioDecoder, SourceDecodeError> {
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(bytes).unwrap();
    AudioDecoder::open_first(file, limits, control())
}
fn code(error: SourceDecodeError) -> String {
    match error {
        SourceDecodeError::Native { code, .. } => code,
        other => panic!("{other}"),
    }
}
const CASES: &[(&str, &str, u32, u32, u64)] = &[
    ("u8-mono", "pcm_u8", 8000, 1, 0),
    ("u8-stereo", "pcm_u8", 44100, 2, 0),
    ("u8-extensible", "pcm_u8", 48000, 1, 4),
    ("s16-fmt18", "pcm_s16le", 48000, 2, 0),
    ("s24-mono", "pcm_s24le", 44100, 1, 0),
    ("s24-stereo", "pcm_s24le", 96000, 2, 3),
    ("s24-surround", "pcm_s24le", 48000, 6, 0x3f),
    ("s24-fmt18", "pcm_s24le", 48000, 2, 0),
    ("s32-stereo", "pcm_s32le", 48000, 2, 0),
    ("s32-mono", "pcm_s32le", 192000, 1, 4),
    ("s32-384000", "pcm_s32le", 384000, 2, 3),
    ("f32-stereo", "pcm_f32le", 48000, 2, 0),
    ("f32-mono", "pcm_f32le", 44100, 1, 0),
    ("f32-surround", "pcm_f32le", 48000, 6, 0x3f),
];

#[test]
fn integer_and_float_wave_preserve_exact_clocks_layouts_and_scalar_pcm_in_both_modes() {
    for &(name, codec, rate, channels, mask) in CASES {
        let expected = std::fs::read(fixture(name, "f32le")).unwrap();
        for mode in [AudioDecodeMode::Manual, AudioDecodeMode::Ordinary] {
            let mut decoder = AudioDecoder::open_first_with_mode(
                File::open(fixture(name, "wav")).unwrap(),
                mode,
                AudioDecodeLimits::default(),
                control(),
            )
            .unwrap_or_else(|e| panic!("{name}: {e}"));
            let info = decoder.info();
            assert_eq!(info.codec, codec, "{name}");
            assert_eq!(decoder.evidence().decoder_name, codec);
            assert_eq!(decoder.evidence().container_format, "wav");
            assert_eq!(
                (info.time_base_num, info.time_base_den, info.sample_rate),
                (1, rate, rate)
            );
            assert_eq!(info.stream_duration, Some(8197));
            assert_eq!(
                (
                    info.initial_padding,
                    info.trailing_padding,
                    info.seek_preroll
                ),
                (0, 0, 0)
            );
            assert_eq!(
                info.channel_layout,
                if mask == 0 {
                    AudioChannelLayout::Unspecified { channels }
                } else {
                    AudioChannelLayout::Native { channels, mask }
                }
            );
            let format = match codec {
                "pcm_u8" => AudioSampleFormat::Unsigned8,
                "pcm_s16le" => AudioSampleFormat::Signed16,
                "pcm_s24le" | "pcm_s32le" => AudioSampleFormat::Signed32,
                "pcm_f32le" => AudioSampleFormat::Float32Interleaved,
                _ => unreachable!(),
            };
            assert_eq!(info.sample_format, format);
            let mut count = 0;
            let mut actual = Vec::new();
            while let Some(frame) = decoder.next_metadata(control()).unwrap() {
                assert_eq!(frame.pts, count, "{name}");
                assert_eq!(frame.reported_duration, Some(i64::from(frame.nb_samples)));
                assert_eq!(frame.sample_format, format);
                assert_eq!(frame.skip_samples, None);
                assert!(!frame.discard);
                count += i64::from(frame.nb_samples);
                for sample in decoder
                    .copy_current_interleaved_f32(control())
                    .unwrap()
                    .samples
                {
                    actual.extend_from_slice(&sample.to_le_bytes());
                }
            }
            assert_eq!(count, 8197, "{name}");
            // Includes signed zero, subnormal floats and excursions outside ±1.
            assert_eq!(actual, expected, "{name} {mode:?}");
            eprintln!(
                "{name} {mode:?}: {count} samples/channel, {rate} Hz, {channels} channels, exact scalar f32 match"
            );
        }
    }
}

#[test]
fn wide_pcm_packet_bounds_use_encoded_width_and_never_drop_partial_frames() {
    for &(name, _, _, channels, _) in CASES {
        let bytes = std::fs::read(fixture(name, "wav")).unwrap();
        let alignment = u32::from(u16::from_le_bytes(bytes[32..34].try_into().unwrap()));
        let limits = AudioDecodeLimits {
            max_samples_per_frame: 7,
            max_packet_bytes: alignment * 5 + alignment - 1,
            ..Default::default()
        };
        let mut decoder = from_bytes(&bytes, limits).unwrap();
        let mut count = 0;
        while let Some(frame) = decoder.next_metadata(control()).unwrap() {
            assert!(frame.nb_samples <= 5, "{name}: {frame:?}");
            assert_eq!(frame.pts, count);
            count += i64::from(frame.nb_samples);
            assert_eq!(
                decoder
                    .copy_current_interleaved_f32(control())
                    .unwrap()
                    .samples
                    .len(),
                frame.nb_samples as usize * channels as usize
            );
        }
        assert_eq!(count, 8197, "{name}");
        if alignment > 1 {
            assert!(
                from_bytes(
                    &bytes,
                    AudioDecodeLimits {
                        max_packet_bytes: alignment - 1,
                        ..Default::default()
                    }
                )
                .is_err()
            );
        }
        assert!(
            from_bytes(
                &bytes,
                AudioDecodeLimits {
                    max_decoded_samples: 8196,
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
}

#[test]
fn wide_wave_refuses_ambiguous_widths_forged_counts_and_unqualified_extensions() {
    let source = std::fs::read(fixture("f32-surround", "wav")).unwrap();
    for (at, value) in [
        (20, 1),
        (22, 5),
        (32, 12),
        (34, 24),
        (34, 64),
        (36, 23),
        (38, 24),
        (38, 0),
        (40, 0),
        (40, 3),
        (44, 2),
        (45, 1),
        (68, 0),
        (68, 6),
    ] {
        let mut bad = source.clone();
        bad[at] = value;
        assert!(
            from_bytes(&bad, Default::default()).is_err(),
            "offset {at}={value}"
        );
    }
    let plain = std::fs::read(fixture("f32-stereo", "wav")).unwrap();
    for (at, value) in [(36, 1), (20, 6), (34, 16), (28, 1), (32, 7)] {
        let mut bad = plain.clone();
        assert_ne!(bad[at], value);
        bad[at] = value;
        assert!(
            from_bytes(&bad, Default::default()).is_err(),
            "plain offset {at}"
        );
    }
    for (name, extra) in [
        ("duplicate fact", b"fact\x04\0\0\0\x05\x20\0\0".as_slice()),
        ("unqualified metadata", b"LIST\0\0\0\0".as_slice()),
    ] {
        let mut bad = source.clone();
        bad.extend_from_slice(extra);
        let size = u32::try_from(bad.len() - 8).unwrap();
        bad[4..8].copy_from_slice(&size.to_le_bytes());
        assert!(from_bytes(&bad, Default::default()).is_err(), "{name}");
    }
    // Packed mono24 has an odd-sized data chunk, requiring the RIFF pad byte.
    let mut bad = std::fs::read(fixture("s24-mono", "wav")).unwrap();
    bad.pop();
    let size = u32::try_from(bad.len() - 8).unwrap();
    bad[4..8].copy_from_slice(&size.to_le_bytes());
    assert!(from_bytes(&bad, Default::default()).is_err());
    // The tempting 24-valid-in-32 combination is explicitly refused, not
    // passed to FFmpeg's legacy float24 reinterpretation.
    let mut bad = std::fs::read(fixture("s32-mono", "wav")).unwrap();
    bad[38] = 24;
    assert!(from_bytes(&bad, Default::default()).is_err());
}

#[test]
fn float_wave_nan_and_infinities_fail_copy_and_poison_the_decoder() {
    let original = std::fs::read(fixture("f32-stereo", "wav")).unwrap();
    let start = original.windows(4).position(|v| v == b"data").unwrap() + 8;
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut bytes = original.clone();
        bytes[start..start + 4].copy_from_slice(&value.to_le_bytes());
        let mut decoder = from_bytes(&bytes, Default::default()).unwrap();
        assert!(decoder.next_metadata(control()).unwrap().is_some());
        assert_eq!(
            code(decoder.copy_current_interleaved_f32(control()).unwrap_err()),
            "invalid_samples"
        );
        assert_eq!(
            code(decoder.next_metadata(control()).unwrap_err()),
            "session_failed"
        );
    }
}
