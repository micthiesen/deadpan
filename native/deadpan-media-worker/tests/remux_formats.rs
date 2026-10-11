//! Qualified acquisition formats keep their encoded packets and decoded content.
use deadpan_media::{RemuxLimits, remux_av};
use deadpan_source::{
    DecodeControl, DecodeLimits, SourceDecoder,
    audio::{AudioDecodeLimits, AudioDecoder},
};
use std::{
    fs::File,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::Duration,
};

static CANCELLED: AtomicBool = AtomicBool::new(false);

#[test]
fn large_opus_offset_upgrades_movie_headers_without_rounding_or_losing_samples() {
    let scratch = tempfile::tempdir().unwrap();
    let audio = scratch.path().join("large-offset.mp4");
    single_stream(
        &source_fixture("audio-fixtures/opus-mp4-av-vp9-offset.mp4"),
        &audio,
        b"soun",
    );
    let mut bytes = std::fs::read(&audio).unwrap();
    // Express the existing 240 kHz movie in the audio's 48 kHz clock before
    // moving its start to 19,000 seconds. Input v0 durations still fit u32;
    // assembly with 60 kHz video needs a 240 kHz movie and v1 duration fields.
    fn move_audio(bytes: &mut [u8], start: usize, end: usize) {
        let mut at = start;
        while at < end {
            let size = u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
            let tag: [u8; 4] = bytes[at + 4..at + 8].try_into().unwrap();
            assert!(size >= 8 && size <= end - at);
            if matches!(&tag, b"moov" | b"trak" | b"edts") {
                move_audio(bytes, at + 8, at + size);
            } else if matches!(&tag, b"mvhd" | b"tkhd") {
                assert_eq!(bytes[at + 8], 0);
                let duration = at + if &tag == b"mvhd" { 24 } else { 28 };
                // The retained movie duration included the now-voided video
                // track. Author the audio-only endpoint in its exact clock.
                bytes[duration..duration + 4].copy_from_slice(&912_008_197_u32.to_be_bytes());
                if &tag == b"mvhd" {
                    assert_eq!(&bytes[at + 20..at + 24], 240_000_u32.to_be_bytes());
                    bytes[at + 20..at + 24].copy_from_slice(&48_000_u32.to_be_bytes());
                }
            } else if &tag == b"elst" {
                assert_eq!(size, 40);
                bytes[at + 16..at + 20].copy_from_slice(&912_000_000_u32.to_be_bytes());
                bytes[at + 28..at + 32].copy_from_slice(&8197_u32.to_be_bytes());
            }
            at += size;
        }
    }
    let length = bytes.len();
    move_audio(&mut bytes, 0, length);
    std::fs::write(&audio, bytes).unwrap();
    let video = scratch.path().join("video.mp4");
    video_only(&source_fixture("fixtures/av1-sdr-8-limited.mp4"), &video);
    let output = scratch.path().join("result.mp4");
    assemble(&video, &audio, &output).unwrap();
    let movie = deadpan_source::inspect_mp4(
        &File::open(&output).unwrap(),
        DecodeLimits::default(),
        control(),
    )
    .unwrap();
    assert_eq!(movie.movie_timescale, 240_000);
    assert!(movie.movie_duration.unwrap() > u64::from(u32::MAX));
    let mut decoder = AudioDecoder::open_first(
        File::open(output).unwrap(),
        AudioDecodeLimits::default(),
        control(),
    )
    .unwrap();
    let framing = decoder.info().mp4_opus.unwrap();
    assert_eq!(
        framing.first_sample + i64::from(framing.pre_skip),
        912_000_000
    );
    assert_eq!(framing.valid_samples, 8197);
    let mut physical_samples = 0;
    while let Some(frame) = decoder.next_metadata(control()).unwrap() {
        physical_samples += u64::from(frame.nb_samples);
    }
    assert!(physical_samples >= framing.valid_samples);
}

#[test]
fn missing_vp9_chroma_is_recorded_through_assembly_and_reassembly() {
    let scratch = tempfile::tempdir().unwrap();
    let original = source_fixture("fixtures/vp9-pq.webm");
    let audio = source_fixture("audio-fixtures/opus-stereo-20.webm");
    for absent in [false, true] {
        let mut bytes = std::fs::read(&original).unwrap();
        for id in [0xb7, 0xb8] {
            let at = bytes
                .windows(3)
                .position(|b| b == [0x55, id, 0x81])
                .unwrap();
            if absent {
                bytes[at..at + 4].copy_from_slice(&[0xec, 0x82, 0, 0]);
            } else {
                bytes[at + 3] = 0;
            }
        }
        let input = scratch.path().join(format!("unknown-{absent}.webm"));
        std::fs::write(&input, bytes).unwrap();
        let output = scratch.path().join(format!("first-{absent}.mp4"));
        assemble(&input, &audio, &output).unwrap();
        let isolated = scratch.path().join(format!("video-{absent}.mp4"));
        video_only(&output, &isolated);
        let second = scratch.path().join(format!("second-{absent}.mp4"));
        assemble(&isolated, &audio, &second).unwrap();
        for path in [&input, &output, &second] {
            let mut actual = SourceDecoder::open(
                File::open(path).unwrap(),
                DecodeLimits::default(),
                control(),
            )
            .unwrap();
            let mut reference = SourceDecoder::open(
                File::open(&original).unwrap(),
                DecodeLimits::default(),
                control(),
            )
            .unwrap();
            assert!(actual.info().chroma_assumed_left);
            assert!(!reference.info().chroma_assumed_left);
            let mut count = 0;
            while let Some(expected) = reference.next_rgba16(control()).unwrap() {
                let frame = actual.next_rgba16(control()).unwrap().unwrap();
                assert_eq!(frame.metadata.pts, expected.metadata.pts);
                assert_eq!(frame.rgba, expected.rgba);
                count += 1;
            }
            assert_eq!(count, 12);
            assert!(actual.next_metadata(control()).unwrap().is_none());
        }
        let mut bytes = std::fs::read(&output).unwrap();
        let last = bytes.len() - 1;
        bytes[last] = 2;
        let invalid = scratch.path().join(format!("invalid-{absent}.mp4"));
        std::fs::write(&invalid, bytes).unwrap();
        assert!(
            SourceDecoder::open(
                File::open(invalid).unwrap(),
                DecodeLimits::default(),
                control()
            )
            .is_err()
        );
    }
}
fn control() -> DecodeControl<'static> {
    DecodeControl {
        cancelled: &CANCELLED,
        timeout: Duration::from_secs(30),
    }
}
fn source_fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../deadpan-source/tests")
        .join(name)
}
// Replace audio track boxes by equal-size free boxes. This isolates the exact
// encoded video fixture without asking another muxer to rewrite its metadata.
fn video_only(input: &Path, output: &Path) {
    single_stream(input, output, b"vide");
}
fn single_stream(input: &Path, output: &Path, keep: &[u8; 4]) {
    let mut bytes = std::fs::read(input).unwrap();
    fn isolate(bytes: &mut [u8], start: usize, end: usize, keep: &[u8; 4]) {
        let mut at = start;
        while at < end {
            let size = u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
            assert!(size >= 8 && size <= end - at);
            if &bytes[at + 4..at + 8] == b"moov" {
                isolate(bytes, at + 8, at + size, keep);
            }
            if &bytes[at + 4..at + 8] == b"trak" {
                let track = &bytes[at..at + size];
                let handler = track.windows(4).position(|b| b == b"hdlr").unwrap();
                if &track[handler + 12..handler + 16] != keep {
                    bytes[at + 4..at + 8].copy_from_slice(b"free");
                }
            }
            at += size;
        }
    }
    if input.extension().unwrap() == "mp4" {
        let length = bytes.len();
        isolate(&mut bytes, 0, length, keep);
    }
    std::fs::write(output, bytes).unwrap();
}

#[test]
fn positive_opus_offset_survives_without_exposing_priming_or_moving_pictures() {
    let scratch = tempfile::tempdir().unwrap();
    let video = source_fixture("fixtures/vp9-pq.webm");
    let mp4 = scratch.path().join("offset.mp4");
    single_stream(
        &source_fixture("audio-fixtures/opus-mp4-av-vp9-offset.mp4"),
        &mp4,
        b"soun",
    );
    let webm = scratch.path().join("offset.webm");
    let mut bytes = std::fs::read(source_fixture("audio-fixtures/opus-stereo-20.webm")).unwrap();
    let clock = bytes
        .windows(3)
        .enumerate()
        .filter_map(|(i, b)| (b == [0xe7, 0x81, 0]).then_some(i))
        .collect::<Vec<_>>();
    assert_eq!(clock.len(), 1);
    // One retained cluster; relative packet clocks remain byte-identical.
    bytes[clock[0] + 2] = 125;
    std::fs::write(&webm, bytes).unwrap();
    for (index, input) in [mp4, webm].iter().enumerate() {
        let output = scratch.path().join(format!("offset-result-{index}.mp4"));
        assemble(&video, input, &output).unwrap();
        let mut audio = AudioDecoder::open_first(
            File::open(&output).unwrap(),
            AudioDecodeLimits::default(),
            control(),
        )
        .unwrap();
        let clock = audio.info().mp4_opus.unwrap();
        assert_eq!(clock.first_sample + i64::from(clock.pre_skip), 6000);
        assert_eq!(clock.valid_samples, 8197);
        let mut samples = Vec::new();
        while let Some(meta) = audio.next_metadata(control()).unwrap() {
            let pcm = audio
                .copy_current_interleaved_f32(control())
                .unwrap()
                .samples;
            let begin = meta.pts.max(6000);
            let end = (meta.pts + i64::from(meta.nb_samples)).min(14197);
            if begin < end {
                samples.extend_from_slice(
                    &pcm[(begin - meta.pts) as usize * 2..(end - meta.pts) as usize * 2],
                );
            }
        }
        let reference =
            std::fs::read(source_fixture("audio-fixtures/opus-stereo-20.f32le")).unwrap();
        assert_eq!(samples.len() * 4, reference.len());
        assert!(
            samples
                .iter()
                .zip(reference.chunks_exact(4))
                .all(|(a, b)| (a - f32::from_le_bytes(b.try_into().unwrap())).abs() < 0.0001)
        );
        let mut pictures = SourceDecoder::open(
            File::open(output).unwrap(),
            DecodeLimits::default(),
            control(),
        )
        .unwrap();
        assert_eq!(pictures.next_metadata(control()).unwrap().unwrap().pts, 0);
    }
}
fn assemble(video: &Path, audio: &Path, output: &Path) -> Result<(), String> {
    let output = File::options()
        .read(true)
        .write(true)
        .create_new(true)
        .open(output)
        .unwrap();
    remux_av(
        Path::new(env!("CARGO_BIN_EXE_deadpan-media-worker")),
        &mut File::open(video).unwrap(),
        &mut File::open(audio).unwrap(),
        &output,
        RemuxLimits {
            max_output_bytes: 8 * 1024 * 1024,
            timeout: Duration::from_secs(60),
        },
        &CANCELLED,
    )
    .map(|_| ())
    .map_err(|e| e.to_string())
}

#[test]
fn av1_empty_delimiters_are_counted_without_weakening_coded_byte_verification() {
    let scratch = tempfile::tempdir().unwrap();
    let video = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/remux/av1-temporal-delimiters.mp4");
    let audio = source_fixture("audio-fixtures/opus-stereo-20.webm");
    let output = scratch.path().join("delimiters.mp4");
    let report = remux_av(
        Path::new(env!("CARGO_BIN_EXE_deadpan-media-worker")),
        &mut File::open(&video).unwrap(),
        &mut File::open(&audio).unwrap(),
        &File::options()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&output)
            .unwrap(),
        RemuxLimits {
            max_output_bytes: 1024 * 1024,
            timeout: Duration::from_secs(30),
        },
        &CANCELLED,
    )
    .unwrap();
    assert_eq!(
        (report.video_packets, report.av1_temporal_delimiters),
        (12, 12)
    );
    let mut original = SourceDecoder::open(
        File::open(&video).unwrap(),
        DecodeLimits::default(),
        control(),
    )
    .unwrap();
    let mut actual = SourceDecoder::open(
        File::open(&output).unwrap(),
        DecodeLimits::default(),
        control(),
    )
    .unwrap();
    while let Some(frame) = original.next_rgba16(control()).unwrap() {
        let copied = actual.next_rgba16(control()).unwrap().unwrap();
        assert_eq!(copied.metadata, frame.metadata);
        assert!(copied.rgba == frame.rgba);
    }
    assert!(actual.next_metadata(control()).unwrap().is_none());
    let mut bytes = std::fs::read(video).unwrap();
    let first_sample = u32::from_be_bytes(bytes[..4].try_into().unwrap()) as usize + 8;
    assert_eq!(&bytes[first_sample..first_sample + 2], [0x12, 0]);
    // FFmpeg also strips padding. This path does not authorize that transform.
    bytes[first_sample] = 0x7a;
    let input = scratch.path().join("padding.mp4");
    std::fs::write(&input, bytes).unwrap();
    assert!(
        assemble(&input, &audio, &scratch.path().join("refused.mp4"))
            .unwrap_err()
            .contains("verification_failed")
    );
}

#[test]
fn retained_cadence_rejects_malformed_extensions_and_survives_another_remux() {
    let scratch = tempfile::tempdir().unwrap();
    let audio = source_fixture("audio-fixtures/opus-mp4-stereo-20.mp4");
    let output = scratch.path().join("first.mp4");
    assemble(&source_fixture("fixtures/av1-vfr.webm"), &audio, &output).unwrap();
    let bytes = std::fs::read(&output).unwrap();
    // The bounded UUID is the final movie child, outside all media chunks.
    let at = bytes.len() - 32;
    assert_eq!(&bytes[at + 4..at + 8], b"uuid");
    assert_eq!(
        u64::from_be_bytes(bytes[at + 24..].try_into().unwrap()),
        33_366_666
    );
    for (index, (field, value)) in [(at + 8, 0_u8), (at + 24, 0xff), (at + 3, 31)]
        .into_iter()
        .enumerate()
    {
        let mut damaged = bytes.clone();
        damaged[field] = value;
        let input = scratch.path().join(format!("damaged-{index}.mp4"));
        std::fs::write(&input, damaged).unwrap();
        assert!(
            SourceDecoder::open(
                File::open(input).unwrap(),
                DecodeLimits::default(),
                control()
            )
            .is_err()
        );
    }
    let video = scratch.path().join("video-only.mp4");
    video_only(&output, &video);
    let second = scratch.path().join("second.mp4");
    assemble(&video, &audio, &second).unwrap();
    let mut a = SourceDecoder::open(
        File::open(output).unwrap(),
        DecodeLimits::default(),
        control(),
    )
    .unwrap();
    let mut b = SourceDecoder::open(
        File::open(second).unwrap(),
        DecodeLimits::default(),
        control(),
    )
    .unwrap();
    assert_eq!(
        a.info().nominal_frame_duration_ns,
        b.info().nominal_frame_duration_ns
    );
    while let Some(frame) = a.next_rgba16(control()).unwrap() {
        let other = b.next_rgba16(control()).unwrap().unwrap();
        assert_eq!(frame.metadata, other.metadata);
        assert!(frame.rgba == other.rgba);
    }
    assert!(b.next_metadata(control()).unwrap().is_none());
}

#[test]
fn zero_preskip_and_invalid_opus_headers_are_explicit() {
    let scratch = tempfile::tempdir().unwrap();
    let video = source_fixture("fixtures/vp9-pq.webm");
    let original = std::fs::read(source_fixture("audio-fixtures/opus-stereo-20.webm")).unwrap();
    let head = original.windows(8).position(|b| b == b"OpusHead").unwrap();
    let delay = original
        .windows(3)
        .position(|b| b == [0x56, 0xaa, 0x83])
        .unwrap()
        + 3;
    let mut zero = original.clone();
    zero[head + 10..head + 12].fill(0);
    zero[delay..delay + 3].fill(0);
    let input = scratch.path().join("zero.webm");
    std::fs::write(&input, zero).unwrap();
    let output = scratch.path().join("zero.mp4");
    assemble(&video, &input, &output).unwrap();
    let audio = AudioDecoder::open_first(
        File::open(output).unwrap(),
        AudioDecodeLimits::default(),
        control(),
    )
    .unwrap();
    let clock = audio.info().mp4_opus.unwrap();
    assert_eq!(
        (clock.pre_skip, clock.first_sample, clock.valid_samples),
        (0, 0, 8509)
    );
    for (index, field) in [head + 18, head + 9, delay].into_iter().enumerate() {
        let mut bytes = original.clone();
        bytes[field] = 7;
        let input = scratch.path().join(format!("bad-{index}.webm"));
        std::fs::write(&input, bytes).unwrap();
        let output = scratch.path().join(format!("bad-{index}.mp4"));
        assert!(
            assemble(&video, &input, &output)
                .unwrap_err()
                .contains("invalid_media")
        );
        assert_eq!(std::fs::metadata(output).unwrap().len(), 0);
    }
}

#[test]
fn copied_video_retains_every_picture_color_geometry_and_clock() {
    let scratch = tempfile::tempdir().unwrap();
    let audio = source_fixture("audio-fixtures/opus-mp4-stereo-20.mp4");
    let mut failures = Vec::new();
    for name in [
        "av1-sdr-8-limited.mp4",
        "av1-sdr-8-limited.webm",
        "av1-sdr-10-full.mp4",
        "av1-vfr.webm",
        "av1-anamorphic.webm",
        "av1-pq.mp4",
        "av1-hlg.webm",
        "av1-grain.mp4",
        "av1-superres.webm",
        "av1-multigop.mp4",
        "vp9-sdr-8-limited.mp4",
        "vp9-sdr-8-limited.webm",
        "vp9-sdr-10-full.webm",
        "vp9-altref.webm",
        "vp9-vfr.webm",
        "vp9-pq.mp4",
        "vp9-pq.webm",
        "vp9-pq-topleft.webm",
        "vp9-hlg-anamorphic.webm",
        "vp9-pq-vfr.webm",
        "vp9-pq-static.mp4",
        "vp9-pq-static.webm",
    ] {
        let input = scratch.path().join(name);
        video_only(&source_fixture(&format!("fixtures/{name}")), &input);
        let output = scratch.path().join(format!("{name}.mp4"));
        if let Err(error) = assemble(&input, &audio, &output) {
            failures.push(format!("{name}: {error}"));
            continue;
        }
        let mut original = SourceDecoder::open(
            File::open(source_fixture(&format!("fixtures/{name}"))).unwrap(),
            DecodeLimits::default(),
            control(),
        )
        .unwrap();
        let mut actual = match SourceDecoder::open(
            File::open(output).unwrap(),
            DecodeLimits::default(),
            control(),
        ) {
            Ok(value) => value,
            Err(error) => {
                failures.push(format!("{name}: assembled decode: {error}"));
                continue;
            }
        };
        assert_eq!(actual.info().color, original.info().color, "{name}");
        assert_eq!(
            actual.info().nominal_frame_duration_ns,
            original.info().nominal_frame_duration_ns,
            "{name}"
        );
        assert_eq!(
            (
                actual.info().sample_aspect_num,
                actual.info().sample_aspect_den
            ),
            (
                original.info().sample_aspect_num,
                original.info().sample_aspect_den
            ),
            "{name}"
        );
        let old_clock = (original.info().time_base_num, original.info().time_base_den);
        let new_clock = (actual.info().time_base_num, actual.info().time_base_den);
        while let Some(expected) = original.next_rgba16(control()).unwrap() {
            let frame = actual.next_rgba16(control()).unwrap().unwrap();
            assert_eq!(
                i128::from(frame.metadata.pts) * i128::from(new_clock.0) * i128::from(old_clock.1),
                i128::from(expected.metadata.pts)
                    * i128::from(old_clock.0)
                    * i128::from(new_clock.1),
                "{name}"
            );
            assert!(frame.rgba == expected.rgba, "{name}: picture bytes differ");
        }
        assert!(actual.next_metadata(control()).unwrap().is_none(), "{name}");
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn opus_sample_clock_priming_gain_and_padding_survive_container_assembly() {
    let scratch = tempfile::tempdir().unwrap();
    let video = scratch.path().join("video.mp4");
    video_only(&source_fixture("fixtures/av1-sdr-8-limited.mp4"), &video);
    let mut failures = Vec::new();
    for (prefix, extension) in [("opus", "webm"), ("opus-mp4", "mp4")] {
        for name in [
            "stereo-20",
            "mono-2.5",
            "mono-60",
            "stereo-120",
            "mono-silk",
            "mono-hybrid",
            "mono-gain",
            "mono-preskip",
        ] {
            let input = source_fixture(&format!("audio-fixtures/{prefix}-{name}.{extension}"));
            let output = scratch.path().join(format!("{prefix}-{name}.mp4"));
            if let Err(error) = assemble(&video, &input, &output) {
                failures.push(format!("{prefix}-{name}: {error}"));
                continue;
            }
            let mut actual = AudioDecoder::open_first(
                File::open(output).unwrap(),
                AudioDecodeLimits::default(),
                control(),
            )
            .unwrap();
            let framing = actual.info().mp4_opus.unwrap();
            assert_eq!(
                framing.valid_samples,
                if name == "mono-preskip" { 7717 } else { 8197 }
            );
            assert_eq!(framing.first_sample, -i64::from(framing.pre_skip));
            let channels = framing.channels as usize;
            let mut pcm = Vec::new();
            while let Some(meta) = actual.next_metadata(control()).unwrap() {
                let samples = actual
                    .copy_current_interleaved_f32(control())
                    .unwrap()
                    .samples;
                let begin = meta.pts.max(0);
                let end = (meta.pts + i64::from(meta.nb_samples)).min(framing.valid_samples as i64);
                if begin < end {
                    pcm.extend_from_slice(
                        &samples[(begin - meta.pts) as usize * channels
                            ..(end - meta.pts) as usize * channels],
                    );
                }
            }
            let expected =
                std::fs::read(source_fixture(&format!("audio-fixtures/opus-{name}.f32le")))
                    .unwrap();
            assert_eq!(pcm.len() * 4, expected.len(), "{prefix}-{name}");
            let peak = pcm
                .iter()
                .zip(expected.chunks_exact(4))
                .map(|(a, b)| (a - f32::from_le_bytes(b.try_into().unwrap())).abs())
                .fold(0_f32, f32::max);
            assert!(peak < 0.0001, "{prefix}-{name}: {peak}");
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
