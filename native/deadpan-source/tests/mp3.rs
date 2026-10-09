use deadpan_source::{
    DecodeControl,
    audio::{AudioDecodeLimits, AudioDecodeMode, AudioDecoder},
};
use std::{fs::File, path::PathBuf, sync::atomic::AtomicBool, time::Duration};

static CANCELLED: AtomicBool = AtomicBool::new(false);
fn control() -> DecodeControl<'static> {
    DecodeControl {
        timeout: Duration::from_secs(30),
        cancelled: &CANCELLED,
    }
}
fn fixture(name: &str, extension: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/audio-fixtures")
        .join(format!("mp3-{name}.{extension}"))
}

#[test]
fn all_mpeg_layer_three_rates_preserve_physical_frames_and_match_independent_pcm() {
    for (name, rate, channels, available) in [
        ("stereo-cbr", 48000, 2, 8197),
        ("mono-vbr", 44100, 1, 8197),
        ("stereo-32000", 32000, 2, 8197),
        ("mono-mpeg2", 24000, 1, 8197),
        ("stereo-22050", 22050, 2, 8197),
        ("mono-16000", 16000, 1, 8197),
        ("stereo-12000", 12000, 2, 8197),
        ("mono-11025", 11025, 1, 8197),
        ("mono-mpeg25", 8000, 1, 8197),
        ("stereo-untagged", 48000, 2, 10368),
        ("mono-short", 24000, 1, 97),
        ("stereo-crc", 48000, 2, 8197),
        ("stereo-id3v3", 48000, 2, 8197),
        ("stereo-id3v4", 48000, 2, 8197),
        ("stereo-spanning", 48000, 2, 4868),
        ("stereo-unknown", 48000, 2, 10368),
    ] {
        let open = |mode| {
            AudioDecoder::open_first_with_mode(
                File::open(fixture(name, "mp3")).unwrap(),
                mode,
                AudioDecodeLimits::default(),
                control(),
            )
            .unwrap_or_else(|e| panic!("{name}: {e}"))
        };
        let mut manual = open(AudioDecodeMode::Manual);
        let mut ordinary = open(AudioDecodeMode::Ordinary);
        let framing = manual.info().mp3.unwrap();
        assert_eq!(manual.info().codec, "mp3");
        assert_eq!(manual.evidence().decoder_name, "mp3float");
        assert_eq!(framing.sample_rate, rate);
        assert_eq!(framing.channels, channels);
        let mut physical = Vec::new();
        let mut frames = Vec::new();
        while let Some(frame) = manual.next_metadata(control()).unwrap() {
            assert_eq!(frame.nb_samples, framing.samples_per_frame, "{name}");
            assert_eq!(
                i128::from(frame.pts) * i128::from(framing.sample_rate),
                frames.len() as i128 * i128::from(framing.samples_per_frame) * 14_112_000,
                "{name}"
            );
            physical.extend(
                manual
                    .copy_current_interleaved_f32(control())
                    .unwrap()
                    .samples,
            );
            frames.push(frame);
        }
        assert_eq!(frames.len() as u64, framing.frame_count);
        let channels = framing.channels as usize;
        let presented = &physical[framing.leading_skip as usize * channels
            ..physical.len() - framing.trailing_skip as usize * channels];
        assert_eq!(presented.len() / channels, available, "{name}");
        let mut automatic = Vec::new();
        while ordinary.next_metadata(control()).unwrap().is_some() {
            automatic.extend(
                ordinary
                    .copy_current_interleaved_f32(control())
                    .unwrap()
                    .samples,
            );
        }
        assert_eq!(presented, automatic, "manual vs ordinary {name}");
        let reference: Vec<f32> = std::fs::read(fixture(name, "f32le"))
            .unwrap()
            .chunks_exact(4)
            .map(|v| f32::from_le_bytes(v.try_into().unwrap()))
            .collect();
        assert_eq!(reference.len(), presented.len());
        let peak = reference
            .iter()
            .zip(presented)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0_f32, f32::max);
        let rms = (reference
            .iter()
            .zip(presented)
            .map(|(a, b)| f64::from(a - b).powi(2))
            .sum::<f64>()
            / reference.len() as f64)
            .sqrt();
        eprintln!(
            "{name}: {framing:?} frames={} first={:?} last={:?} peak={peak} rms={rms}",
            frames.len(),
            frames[0],
            frames.last().unwrap()
        );
        assert!(
            peak < 0.00001 && rms < 0.000001,
            "{name}: peak={peak} rms={rms}"
        );
    }
}

fn admitted_bytes(
    bytes: &[u8],
    limits: AudioDecodeLimits,
) -> Result<AudioDecoder, deadpan_source::SourceDecodeError> {
    use std::io::Write;
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(bytes).unwrap();
    AudioDecoder::open_first(file, limits, control())
}

fn repair_stereo_tag_crc(bytes: &mut [u8]) {
    let mut crc = 0_u16;
    for b in &bytes[..190] {
        crc ^= u16::from(*b);
        for _ in 0..8 {
            crc = (crc >> 1) ^ if crc & 1 != 0 { 0xa001 } else { 0 };
        }
    }
    bytes[190..192].copy_from_slice(&crc.to_be_bytes());
}

#[test]
fn mp3_admission_rejects_truncated_forged_and_changing_frames_before_decode() {
    let source = std::fs::read(fixture("stereo-cbr", "mp3")).unwrap();
    for (name, kind) in [
        ("truncated", 0),
        ("wrong frame count", 1),
        ("wrong byte count", 2),
        ("wrong checksum", 3),
        ("changing rate", 4),
        ("changing channels", 5),
        ("free format", 6),
        ("wrong layer", 7),
        ("emphasis", 8),
        ("insufficient declared padding", 9),
        ("junk between frames", 10),
        ("appended data", 11),
    ] {
        let mut bad = source.clone();
        match kind {
            0 => {
                bad.pop();
            }
            1 => {
                bad[47] ^= 1;
                repair_stereo_tag_crc(&mut bad);
            }
            2 => {
                bad[51] ^= 1;
                repair_stereo_tag_crc(&mut bad);
            }
            3 => bad[190] ^= 1,
            4 => bad[1152 + 2] ^= 4,
            5 => bad[1152 + 3] |= 0xc0,
            6 => bad[1152 + 2] &= 15,
            7 => bad[1152 + 1] ^= 2,
            8 => bad[1152 + 3] |= 1,
            9 => {
                bad[178] &= 0xf0;
                bad[179] = 0;
                repair_stereo_tag_crc(&mut bad);
            }
            10 => bad.insert(1152, 0),
            11 => bad.extend_from_slice(b"trailing junk"),
            _ => unreachable!(),
        }
        let error = admitted_bytes(&bad, AudioDecodeLimits::default())
            .err()
            .unwrap_or_else(|| panic!("accepted {name}"));
        eprintln!("{name}: {error}");
    }
    for limits in [
        AudioDecodeLimits {
            max_packets: 8,
            ..Default::default()
        },
        AudioDecodeLimits {
            max_decoded_samples: 10_000,
            ..Default::default()
        },
        AudioDecodeLimits {
            max_packet_bytes: 575,
            ..Default::default()
        },
        AudioDecodeLimits {
            max_channels: 1,
            ..Default::default()
        },
        AudioDecodeLimits {
            max_sample_rate: 44_100,
            ..Default::default()
        },
    ] {
        assert!(admitted_bytes(&source, limits).is_err());
    }
    assert!(
        AudioDecoder::open(
            File::open(fixture("stereo-cbr", "mp3")).unwrap(),
            1,
            AudioDecodeLimits::default(),
            control()
        )
        .is_err()
    );
}

#[test]
fn mp3_id3_allocation_grammar_rejects_transformed_and_unbounded_metadata() {
    let source = std::fs::read(fixture("stereo-id3v4", "mp3")).unwrap();
    for kind in 0..7 {
        let mut bad = source.clone();
        match kind {
            0 => bad[3] = 2,
            1 => bad[5] = 0x80,
            2 => bad[6] = 0x80,
            3 => bad[10..14].copy_from_slice(b"CHAP"),
            4 => bad[19] = 8,
            5 => bad[14..18].copy_from_slice(&[0x7f; 4]),
            6 => {
                let at = bad.windows(10).position(|w| w == b"image/png\0").unwrap();
                bad[at..at + 10].copy_from_slice(b"-->http://");
            }
            _ => unreachable!(),
        }
        assert!(
            admitted_bytes(&bad, AudioDecodeLimits::default()).is_err(),
            "mutation {kind}"
        );
    }
}

#[test]
fn crc_protected_mp3_corruption_fails_instead_of_returning_concealed_pcm() {
    let mut bytes = std::fs::read(fixture("stereo-crc", "mp3")).unwrap();
    // First physical audio frame follows the 576-byte Info frame. This changes
    // its stored CRC, preserving the admitted framing and all compressed data.
    bytes[576 + 4] ^= 1;
    let mut decoder = admitted_bytes(&bytes, AudioDecodeLimits::default()).unwrap();
    assert!(decoder.next_metadata(control()).is_err());
    assert!(decoder.next_metadata(control()).is_err());
}

#[test]
fn long_mp3_admission_reads_sparse_headers_within_one_opening_allowance() {
    use std::io::Write;
    let bytes = std::fs::read(fixture("stereo-untagged", "mp3")).unwrap();
    let mut input = tempfile::tempfile().unwrap();
    for _ in 0..4000 {
        input.write_all(&bytes).unwrap();
    }
    assert!(input.metadata().unwrap().len() > 16 * 1024 * 1024);
    let decoder = AudioDecoder::open_first(
        input,
        AudioDecodeLimits {
            max_io_bytes_per_call: 512 * 1024,
            ..Default::default()
        },
        control(),
    )
    .unwrap();
    assert_eq!(decoder.info().mp3.unwrap().frame_count, 36_000);
}
