//! Exercise the real AAC encoder and decoder at their declared sample origin.
//! Quiet onsets expose encoder startup artifacts that isolated clicks miss.

use std::{
    fs::{File, OpenOptions},
    os::unix::fs::OpenOptionsExt,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

use deadpan_encode::{
    BFramePolicy, EncodeContract, EncodeLimits, EncodeReport, EncoderMode, EncoderSession,
    NextInput,
};
use deadpan_source::{
    DecodeControl, DecodeLimits, Mp4PacketReader, Mp4TrackKind,
    audio::{AudioDecodeLimits, AudioDecodeMode, AudioDecoder},
};

const RATE: [u32; 2] = [30_000, 1_001];
const FRAMES: u64 = 100;
const SAMPLES: usize = 160_160;
const BLOCK: usize = 480;
static CANCELLED: AtomicBool = AtomicBool::new(false);

fn control() -> DecodeControl<'static> {
    DecodeControl {
        timeout: Duration::from_secs(10),
        cancelled: &CANCELLED,
    }
}

fn encode(
    pcm: &[[f32; 2]],
    frames: u64,
    rate: [u32; 2],
) -> (File, EncodeReport, tempfile::TempDir) {
    let directory = tempfile::tempdir().unwrap();
    let file = OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(directory.path().join("encoded.mp4"))
        .unwrap();
    let contract = EncodeContract::new(
        [64, 64],
        rate,
        frames,
        pcm.len() as u64,
        EncoderMode::Software,
        BFramePolicy::None,
    )
    .unwrap();
    let mut session = EncoderSession::open(
        file,
        contract,
        EncodeLimits::default(),
        &CANCELLED,
        Instant::now() + Duration::from_secs(60),
    )
    .unwrap();
    let mut picture = vec![128; 64 * 64 * 3 / 2];
    picture[..64 * 64].fill(64);
    let (mut left, mut right) = ([0.0; 1024], [0.0; 1024]);
    loop {
        match session.next_input().unwrap() {
            NextInput::Picture {
                ordinal,
                pts,
                duration,
            } => session
                .push_picture(ordinal, pts, duration, &picture)
                .unwrap(),
            NextInput::Audio {
                first_sample,
                samples,
            } => {
                let start = usize::try_from(first_sample).unwrap();
                let count = usize::try_from(samples).unwrap();
                for (offset, pair) in pcm[start..start + count].iter().enumerate() {
                    left[offset] = pair[0];
                    right[offset] = pair[1];
                }
                session
                    .push_audio(first_sample, &left[..count], &right[..count])
                    .unwrap();
            }
            NextInput::Finish => break,
        }
    }
    let result = session.finish().unwrap().into_parts();
    assert_eq!(result.1.audio_samples, pcm.len() as u64);
    assert_eq!(result.1.video_frames, frames);
    (result.0, result.1, directory)
}

/// Inspect the independent MP4 tables and decode both FFmpeg skip modes. Only
/// the declared edit maps samples into [0, count); no event search or alignment.
fn decode(file: &File, report: &EncodeReport, count: usize) -> Vec<[f32; 2]> {
    let mut packets = Mp4PacketReader::open(
        file.try_clone().unwrap(),
        DecodeLimits::default(),
        control(),
    )
    .unwrap();
    let movie = packets.inspection();
    assert!(movie.moov_before_mdat);
    let audio = movie
        .tracks
        .iter()
        .find(|track| track.kind == Mp4TrackKind::Audio)
        .unwrap();
    assert_eq!(audio.media_timescale, 48_000);
    assert_eq!(audio.edits.len(), 1);
    let edit = audio.edits[0];
    assert!(edit.media_time >= 1024);
    assert_eq!(edit.media_time % 1024, 0);
    assert_eq!((edit.media_rate_integer, edit.media_rate_fraction), (1, 0));
    assert_eq!(
        u128::from(edit.segment_duration) * 48_000,
        count as u128 * u128::from(movie.movie_timescale),
        "edit duration must retain the exact authored sample endpoint"
    );
    let priming = edit.media_time as u64;
    assert_eq!(audio.media_duration, Some(priming + count as u64));
    assert_eq!(audio.timing_duration, priming + count as u64);
    assert_eq!(u64::from(audio.sample_count), report.audio_packets);
    assert_eq!(
        report.audio_packets,
        priming / 1024 + (count as u64).div_ceil(1024)
    );
    let audio_index = audio.index;
    let mut next = -edit.media_time;
    while let Some(packet) = packets.next_packet(control()).unwrap() {
        if packet.track_index != audio_index {
            continue;
        }
        let presentation = packet.presentation.unwrap();
        assert_eq!(presentation.pts, next);
        assert_eq!(presentation.dts, next);
        assert_eq!(i64::from(packet.duration), (count as i64 - next).min(1024));
        next += i64::from(packet.duration);
    }
    assert_eq!(next, count as i64);

    let mut modes = Vec::new();
    for mode in [AudioDecodeMode::Manual, AudioDecodeMode::Ordinary] {
        let mut decoder = AudioDecoder::open_first_with_mode(
            file.try_clone().unwrap(),
            mode,
            AudioDecodeLimits::default(),
            control(),
        )
        .unwrap();
        assert_eq!(decoder.info().stream_start, Some(0));
        assert_eq!(decoder.info().stream_duration, Some(count as i64));
        assert_eq!(decoder.info().sample_rate, 48_000);
        assert_eq!(
            (decoder.info().time_base_num, decoder.info().time_base_den),
            (1, 48_000)
        );
        let mut pcm = Vec::with_capacity(count);
        let mut end = None;
        while let Some(metadata) = decoder.next_metadata(control()).unwrap() {
            if let Some(previous) = end {
                assert_eq!(metadata.pts, previous);
            } else if mode == AudioDecodeMode::Ordinary {
                assert_eq!(metadata.pts, 0);
            } else {
                assert_eq!(metadata.pts, -edit.media_time);
            }
            assert_eq!(metadata.nb_samples, 1024);
            assert_eq!(metadata.discard, metadata.pts < 0);
            if mode == AudioDecodeMode::Manual && metadata.pts == -edit.media_time {
                let skip = metadata.skip_samples.unwrap();
                assert_eq!(i64::from(skip.leading), edit.media_time);
                assert_eq!(skip.trailing, 0);
                assert_eq!((skip.leading_reason, skip.trailing_reason), (0, 0));
            } else {
                assert_eq!(metadata.skip_samples, None);
            }
            assert_eq!(
                metadata.reported_duration,
                Some((count as i64 - metadata.pts).min(1024))
            );
            end = Some(metadata.pts + i64::from(metadata.nb_samples));
            let decoded = decoder.copy_current_interleaved_f32(control()).unwrap();
            for (offset, pair) in decoded.samples.chunks_exact(2).enumerate() {
                let at = metadata.pts + offset as i64;
                if !(0..count as i64).contains(&at) {
                    continue;
                }
                assert_eq!(at as usize, pcm.len());
                assert!(pair.iter().all(|sample| sample.is_finite()));
                pcm.push([pair[0], pair[1]]);
            }
        }
        assert_eq!(pcm.len(), count);
        assert_eq!(end, Some(count.div_ceil(1024) as i64 * 1024));
        modes.push(pcm);
    }
    assert_eq!(modes[0], modes[1], "manual and ordinary AAC presentation");
    modes.pop().unwrap()
}

fn quiet_chirp() -> Vec<[f32; 2]> {
    (0..SAMPLES)
        .map(|sample| {
            std::array::from_fn(|channel| {
                if !(256..SAMPLES - 256).contains(&sample) {
                    return 0.0;
                }
                let t = sample as f64 / 48_000.0;
                let (frequency, sweep) = if channel == 0 {
                    (233.0, 37.0)
                } else {
                    (419.0, 53.0)
                };
                let signal = (std::f64::consts::TAU * (frequency * t + sweep * t * t)).sin();
                let burst = if (12_000..36_000).contains(&sample)
                    || (79_000..108_000).contains(&sample)
                    || (128_000..151_000).contains(&sample)
                {
                    3_500.0
                } else {
                    1_200.0
                };
                let edge = ((sample - 256).min(SAMPLES - 256 - sample) as f64 / 1024.0).min(1.0);
                (signal * burst * edge).round() as f32 / 32768.0
            })
        })
        .collect()
}

fn energy(pcm: &[[f32; 2]]) -> f64 {
    pcm.iter()
        .flatten()
        .map(|sample| f64::from(*sample).powi(2))
        .sum()
}

fn assert_opening(reference: &[[f32; 2]], actual: &[[f32; 2]], label: &str) {
    let signal = energy(&reference[..BLOCK]);
    assert!(
        10.0 * (signal / (BLOCK * 2) as f64).log10() >= -60.0,
        "fixture must exercise the production content-block gate"
    );
    let decoded = energy(&actual[..BLOCK]);
    let level_db = 10.0 * (decoded / signal).log10();
    let error: f64 = reference[..BLOCK]
        .iter()
        .flatten()
        .zip(actual[..BLOCK].iter().flatten())
        .map(|(a, b)| (f64::from(*a) - f64::from(*b)).powi(2))
        .sum();
    let snr_db = 10.0 * (signal / error).log10();
    eprintln!("{label}: opening level delta {level_db:.6} dB, SNR {snr_db:.6} dB");
    // These are the existing production export verifier's content-block gates.
    // AAC priming is excluded by declared timestamps, never a fitted offset.
    assert!(
        level_db.abs() <= 3.0,
        "{label}: opening level changed {level_db} dB"
    );
    assert!(snr_db >= 1.5, "{label}: opening SNR is {snr_db} dB");
}

#[test]
fn quiet_aac_transcode_keeps_the_first_ten_milliseconds_at_the_authored_origin() {
    let authored = quiet_chirp();
    let (first, first_report, _first_directory) = encode(&authored, FRAMES, RATE);
    let canonical = decode(&first, &first_report, SAMPLES);
    assert_opening(&authored, &canonical, "first encode");
    let (second, second_report, _second_directory) = encode(&canonical, FRAMES, RATE);
    let exported = decode(&second, &second_report, SAMPLES);
    assert_opening(&canonical, &exported, "AAC transcode");
    let error: f64 = canonical
        .iter()
        .flatten()
        .zip(exported.iter().flatten())
        .map(|(a, b)| (f64::from(*a) - f64::from(*b)).powi(2))
        .sum();
    assert!(
        10.0 * (energy(&canonical) / error).log10() > 20.0,
        "full waveform changed or shifted at its declared PTS"
    );
}

#[test]
fn sub_block_export_retains_signal_at_sample_zero_and_its_exact_terminal_sample() {
    let pcm: Vec<[f32; 2]> = (0..800)
        .map(|sample| {
            let angle = std::f64::consts::TAU * sample as f64 / 64.0;
            [
                (0.1 * angle.cos()) as f32,
                (-0.1 * (angle * 1.3).cos()) as f32,
            ]
        })
        .collect();
    let (file, report, _directory) = encode(&pcm, 1, [60, 1]);
    let actual = decode(&file, &report, pcm.len());
    assert_opening(&pcm, &actual, "sample-zero signal");
    for channel in 0..2 {
        assert!((actual[0][channel] - pcm[0][channel]).abs() < 0.04);
    }
    assert!(energy(&actual[736..]) > energy(&pcm[736..]) / 2.0);
}
