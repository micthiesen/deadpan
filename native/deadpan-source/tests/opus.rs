use deadpan_source::{
    DecodeControl, DecodeLimits, SourceDecoder,
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
        .join(format!("opus-{name}.{extension}"))
}

#[test]
fn physical_opus_frames_preserve_skip_evidence_and_match_independent_libopus_pcm() {
    let mut failures = Vec::new();
    for (name, channels, packet_samples) in [
        ("stereo-20", 2, 960),
        ("mono-2.5", 1, 120),
        ("mono-60", 1, 2880),
        ("stereo-120", 2, 5760),
        ("av", 2, 960),
        ("av-offset", 2, 960),
        ("av-audio-first", 2, 960),
        ("av-multi", 2, 960),
        ("mono-silk", 1, 960),
        ("mono-hybrid", 1, 960),
        ("mono-preskip", 1, 120),
        ("mono-gain", 1, 960),
    ] {
        let mut manual = AudioDecoder::open_first(
            File::open(fixture(name, "webm")).unwrap(),
            AudioDecodeLimits::default(),
            control(),
        )
        .unwrap_or_else(|e| panic!("{name}: {e}"));
        let mut ordinary = AudioDecoder::open_first_with_mode(
            File::open(fixture(name, "webm")).unwrap(),
            AudioDecodeMode::Ordinary,
            AudioDecodeLimits::default(),
            control(),
        )
        .unwrap();
        assert_eq!(manual.info().codec, "opus");
        assert_eq!(manual.info().sample_rate, 48_000);
        assert_eq!(manual.info().channel_layout.channels(), channels);
        assert_eq!(manual.evidence().decoder_name, "libopus");
        assert_eq!(manual.info().time_base_num, 1);
        assert_eq!(manual.info().time_base_den, 1000);
        let clock = manual.info().matroska_opus.unwrap();
        assert_eq!(clock.pre_skip, manual.info().initial_padding);
        let mut trimmed = Vec::new();
        let mut observations = Vec::new();
        let mut leading = 0;
        while let Some(meta) = manual.next_metadata(control()).unwrap() {
            let frame = manual.copy_current_interleaved_f32(control()).unwrap();
            assert_eq!(meta.nb_samples, packet_samples, "{name}");
            let skip = meta.skip_samples;
            leading += skip.map_or(0, |s| s.leading);
            let begin = leading.min(meta.nb_samples);
            leading -= begin;
            let end = meta.nb_samples - skip.map_or(0, |s| s.trailing);
            assert!(begin <= end);
            trimmed.extend_from_slice(
                &frame.samples[(begin * channels) as usize..(end * channels) as usize],
            );
            observations.push(meta);
        }
        assert_eq!(
            trimmed.len(),
            if name == "mono-preskip" { 7717 } else { 8197 } * channels as usize,
            "{name}: {observations:?}"
        );
        let mut presented = Vec::new();
        while ordinary.next_metadata(control()).unwrap().is_some() {
            presented.extend(
                ordinary
                    .copy_current_interleaved_f32(control())
                    .unwrap()
                    .samples,
            );
        }
        assert_eq!(trimmed, presented, "manual vs ordinary: {name}");
        let reference: Vec<f32> = std::fs::read(fixture(name, "f32le"))
            .unwrap()
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
            .collect();
        assert_eq!(reference.len(), trimmed.len());
        let peak = reference
            .iter()
            .zip(&trimmed)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0_f32, f32::max);
        let rms = (reference
            .iter()
            .zip(&trimmed)
            .map(|(a, b)| f64::from(a - b).powi(2))
            .sum::<f64>()
            / reference.len() as f64)
            .sqrt();
        eprintln!(
            "{name}: clock={clock:?}, frames={}, first={:?}, last={:?}, libopus peak={peak} rms={rms}",
            observations.len(),
            observations[0],
            observations.last().unwrap()
        );
        if let Some(directory) = std::env::var_os("DEADPAN_OPUS_KEEP") {
            std::fs::write(
                PathBuf::from(directory).join(format!("{name}.f32le")),
                trimmed
                    .iter()
                    .flat_map(|sample| sample.to_le_bytes())
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        }
        if peak >= 0.0001 || rms >= 0.00001 {
            failures.push(format!("{name}: peak {peak}, rms {rms}"));
        }
        if name.starts_with("av") {
            let mut video = SourceDecoder::open(
                File::open(fixture(name, "webm")).unwrap(),
                DecodeLimits::default(),
                control(),
            )
            .unwrap();
            assert_eq!(
                video.info().audio_streams.len(),
                if name == "av-multi" { 2 } else { 1 }
            );
            assert_eq!(
                video.info().stream_index,
                u32::from(name == "av-audio-first")
            );
            assert_eq!(video.info().audio_streams[0].codec, "opus");
            let mut count = 0;
            while video.next_rgba(control()).unwrap().is_some() {
                count += 1;
            }
            assert_eq!(count, 12);
        }
        if name == "av-multi" {
            let selected = AudioDecoder::open(
                File::open(fixture(name, "webm")).unwrap(),
                2,
                AudioDecodeLimits::default(),
                control(),
            )
            .unwrap();
            assert_eq!(selected.info().stream_index, 2);
            assert_eq!(selected.info().matroska_opus, manual.info().matroska_opus);
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("; "));
}
