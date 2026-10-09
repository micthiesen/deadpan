use deadpan_source::{
    DecodeControl,
    audio::{AudioDecodeLimits, AudioDecoder},
};
use std::{fs::File, io::Write, path::PathBuf, sync::atomic::AtomicBool, time::Duration};

fn tag(bytes: &[u8], name: &[u8; 4]) -> usize {
    let hits: Vec<_> = bytes
        .windows(4)
        .enumerate()
        .filter_map(|(i, b)| (b == name).then_some(i))
        .collect();
    assert_eq!(hits.len(), 1, "{name:?}");
    hits[0] + 4
}

#[test]
fn mp4_opus_refuses_malformed_headers_packets_and_inexact_edits_before_decode() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/audio-fixtures");
    let original = std::fs::read(root.join("opus-mp4-av-av1.mp4")).unwrap();
    let dops = tag(&original, b"dOps");
    let entry = tag(&original, b"Opus");
    let stco = original
        .windows(4)
        .enumerate()
        .filter_map(|(at, b)| (b == b"stco").then_some(at + 4))
        .next_back()
        .unwrap();
    let packet = u32::from_be_bytes(original[stco + 8..stco + 12].try_into().unwrap()) as usize;
    let cancelled = AtomicBool::new(false);
    let control = || DecodeControl {
        timeout: Duration::from_secs(30),
        cancelled: &cancelled,
    };
    for (at, value) in [
        (dops, 1),
        (dops + 1, 3),
        (dops + 10, 1),
        (entry + 8, 1),
        (entry + 20, 1),
        (entry + 16, 1),
        (packet, 0xfb),
        (dops + 3, 0),
    ] {
        let mut bytes = original.clone();
        bytes[at] = value;
        let mut file = tempfile::tempfile().unwrap();
        file.write_all(&bytes).unwrap();
        let error = AudioDecoder::open_first(
            file.try_clone().unwrap(),
            AudioDecodeLimits::default(),
            control(),
        )
        .err()
        .expect("malformed audio refused");
        assert!(
            matches!(error, deadpan_source::SourceDecodeError::Native { ref code, .. } if code == "invalid_input" || code == "resource_limit"),
            "{at}: {error}"
        );
        assert!(
            deadpan_source::SourceDecoder::open(
                file,
                deadpan_source::DecodeLimits::default(),
                control()
            )
            .is_err(),
            "ignored audio must still be admitted"
        );
    }
    let standalone = std::fs::read(root.join("opus-mp4-stereo-20.mp4")).unwrap();
    let movie = tag(&standalone, b"mvhd");
    let edit = tag(&standalone, b"elst");
    let timing = tag(&standalone, b"stts");
    for (at, value) in [
        (movie + 12, 48001_u32),
        (edit + 8, 8198),
        (edit + 12, 313),
        (timing + 12, 959),
    ] {
        let mut bytes = standalone.clone();
        bytes[at..at + 4].copy_from_slice(&value.to_be_bytes());
        let mut file = tempfile::tempfile().unwrap();
        file.write_all(&bytes).unwrap();
        assert!(
            AudioDecoder::open_first(file, AudioDecodeLimits::default(), control()).is_err(),
            "{at}"
        );
    }
    for limits in [
        AudioDecodeLimits {
            max_decoded_samples: 8639,
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
        let mut file = tempfile::tempfile().unwrap();
        file.write_all(&standalone).unwrap();
        assert!(AudioDecoder::open_first(file, limits, control()).is_err());
    }
    // dOps InputSampleRate describes the encoder input, not the decoder clock.
    for rate in [0_u32, 44100, 96000] {
        let mut bytes = standalone.clone();
        let at = tag(&bytes, b"dOps") + 4;
        bytes[at..at + 4].copy_from_slice(&rate.to_be_bytes());
        let mut file = tempfile::tempfile().unwrap();
        file.write_all(&bytes).unwrap();
        let decoder =
            AudioDecoder::open_first(file, AudioDecodeLimits::default(), control()).unwrap();
        assert_eq!(decoder.info().sample_rate, 48000);
    }
}

#[test]
fn mp4_opus_preserves_physical_packets_and_exact_presentation_samples() {
    let cancelled = AtomicBool::new(false);
    let control = || DecodeControl {
        timeout: Duration::from_secs(30),
        cancelled: &cancelled,
    };
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/audio-fixtures");
    for name in [
        "stereo-20",
        "mono-2.5",
        "mono-60",
        "stereo-120",
        "mono-silk",
        "mono-hybrid",
        "mono-gain",
        "mono-preskip",
        "av-h264",
        "av-av1",
        "av-vp9-offset",
    ] {
        let available = if name == "mono-preskip" { 7717 } else { 8197 };
        let offset = if name == "av-vp9-offset" { 6000 } else { 0 };
        let mut decoder = AudioDecoder::open_first(
            File::open(root.join(format!("opus-mp4-{name}.mp4"))).unwrap(),
            AudioDecodeLimits::default(),
            control(),
        )
        .unwrap();
        let framing = decoder.info().mp4_opus.unwrap();
        assert_eq!(framing.valid_samples, available as u64);
        assert_eq!(framing.first_sample, offset - i64::from(framing.pre_skip));
        assert_eq!(decoder.evidence().decoder_name, "libopus");
        let channels = framing.channels as usize;
        let mut pcm = Vec::new();
        let mut frames = Vec::new();
        while let Some(meta) = decoder.next_metadata(control()).unwrap() {
            let samples = decoder
                .copy_current_interleaved_f32(control())
                .unwrap()
                .samples;
            let start = meta.pts.max(offset);
            let end = (meta.pts + i64::from(meta.nb_samples)).min(offset + available);
            if start < end {
                pcm.extend_from_slice(
                    &samples[((start - meta.pts) as usize * channels)
                        ..((end - meta.pts) as usize * channels)],
                );
            }
            frames.push(meta);
        }
        eprintln!(
            "{name}: {:?}, first {:?}, last {:?}",
            decoder.info(),
            frames.first(),
            frames.last()
        );
        let reference_name = if name.starts_with("av-") {
            "stereo-20"
        } else {
            name
        };
        let reference: Vec<f32> = std::fs::read(root.join(format!("opus-{reference_name}.f32le")))
            .unwrap()
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
            .collect();
        assert_eq!(pcm.len(), reference.len());
        let peak = pcm
            .iter()
            .zip(&reference)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0_f32, f32::max);
        assert!(peak < 0.0001, "{name}: {peak}");
    }
}

#[test]
fn opus_audio_tracks_preserve_copied_sdr_and_hdr_pictures() {
    let cancelled = AtomicBool::new(false);
    let control = || DecodeControl {
        timeout: Duration::from_secs(30),
        cancelled: &cancelled,
    };
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests");
    for (name, original) in [
        ("av-h264", "cfr-bframes.mp4"),
        ("av-av1", "av1-sdr-8-limited.mp4"),
        ("av-vp9-offset", "vp9-pq.mp4"),
    ] {
        let mut actual = deadpan_source::SourceDecoder::open(
            File::open(root.join(format!("audio-fixtures/opus-mp4-{name}.mp4"))).unwrap(),
            deadpan_source::DecodeLimits::default(),
            control(),
        )
        .unwrap();
        let mut expected = deadpan_source::SourceDecoder::open(
            File::open(root.join("fixtures").join(original)).unwrap(),
            deadpan_source::DecodeLimits::default(),
            control(),
        )
        .unwrap();
        assert_eq!(actual.info().audio_streams.len(), 1);
        assert_eq!(actual.info().audio_streams[0].codec, "opus");
        assert_eq!(actual.info().color, expected.info().color, "{name}");
        let mut pictures = 0;
        while let Some(frame) = expected.next_rgba16(control()).unwrap() {
            let actual = actual.next_rgba16(control()).unwrap().unwrap();
            assert_eq!(actual.metadata, frame.metadata, "{name}");
            assert!(
                actual.rgba == frame.rgba,
                "{name}: copied picture bytes differ at frame {pictures}"
            );
            pictures += 1;
        }
        assert!(pictures > 0);
        assert!(actual.next_metadata(control()).unwrap().is_none());
    }
}
