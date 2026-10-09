use std::{
    fs::{self, File},
    io::{BufWriter, Write},
    path::PathBuf,
    process::Command,
};

use deadpan_core::{AssetId, ExactRatio, FrameRate, RevisionId, SourceTimeBase};
use deadpan_source::{
    ColorPrimaries, ColorTransfer, DecodeLimits, SourceDecoder,
    audio::{AudioChannelLayout, AudioDecodeLimits, AudioDecoder},
};
use deadpan_store::{AccessMode, ProjectStore};
use serde_json::json;

use super::{
    SOURCE_FRAMES, boundary,
    support::{Result, Run, json_file, sha256, text},
};

pub struct Fixture {
    pub movie: PathBuf,
    pub package: PathBuf,
    pub asset: AssetId,
    pub initial_revision: RevisionId,
    /// Direct Original decoder output at original sample PTS. No project audio
    /// reader, render plan or rounded frame conversion participates in this oracle.
    pub pcm: Vec<[f32; 2]>,
}

pub fn create(run: &Run) -> Result<Fixture> {
    let raw_video = run.root.join("original.rgb");
    let raw_audio = run.root.join("original.s16le");
    let movie = run.root.join("original.mp4");
    let package = run.root.join("fractional.deadpan");
    write_video(&raw_video)?;
    let mut audio = BufWriter::new(File::create(&raw_audio)?);
    for sample in 0..boundary(SOURCE_FRAMES) {
        for channel in 0..2 {
            audio.write_all(&authored_sample(sample, channel).to_le_bytes())?;
        }
    }
    audio.flush()?;
    drop(audio);
    let ffmpeg = std::env::var_os("DEADPAN_BRIDGE_FFMPEG")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/opt/homebrew/bin/ffmpeg"));
    if !ffmpeg.is_file() {
        return Err(format!(
            "real fractional fixture needs ffmpeg with libx264 at {} (or DEADPAN_BRIDGE_FFMPEG)",
            ffmpeg.display()
        )
        .into());
    }
    let ffmpeg_sha256 = sha256(&ffmpeg)?;
    // Strict input admission permits avc1/mp4a, not MOV PCM sample entries.
    // A timescale divisible by 30000 and 48000 retains both exact endpoints.
    run.command(
        "fixture-encode",
        Command::new(&ffmpeg).args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-y",
            "-f",
            "rawvideo",
            "-pixel_format",
            "rgb24",
            "-video_size",
            "320x180",
            "-framerate",
            "30000/1001",
            "-i",
            text(&raw_video)?,
            "-f",
            "s16le",
            "-ar",
            "48000",
            "-channel_layout",
            "stereo",
            "-i",
            text(&raw_audio)?,
            "-map",
            "0:v:0",
            "-map",
            "1:a:0",
            "-vf",
            // The authored RGB values below are nonlinear BT.709 R'G'B'.
            // Declare that before matrix conversion; otherwise scale carries
            // unspecified transfer/primaries into the encoder despite -color_*.
            "setparams=range=full:color_primaries=bt709:color_trc=bt709:colorspace=gbr,scale=in_range=pc:out_range=tv:out_color_matrix=bt709,format=yuv420p,setparams=range=limited:color_primaries=bt709:color_trc=bt709:colorspace=bt709",
            "-c:v",
            "libx264",
            "-crf",
            "0",
            "-preset",
            "ultrafast",
            "-bf",
            "0",
            "-g",
            "1",
            "-x264-params",
            "colorprim=bt709:transfer=bt709:colormatrix=bt709",
            "-color_range",
            "tv",
            "-colorspace",
            "bt709",
            "-color_trc",
            "bt709",
            "-color_primaries",
            "bt709",
            "-c:a",
            "aac",
            "-b:a",
            "256k",
            "-ar",
            "48000",
            "-ac",
            "2",
            "-video_track_timescale",
            "30000",
            "-movie_timescale",
            "240000",
            "-movflags",
            "+faststart+write_colr",
            "-threads",
            "1",
            "-fs",
            "16777216",
            text(&movie)?,
        ]),
    )?;
    assert_eq!(
        ffmpeg_sha256,
        sha256(&ffmpeg)?,
        "fixture encoder changed during execution"
    );
    let pcm = decode_original(run, &movie)?;
    let created = json_file(&run.cli(
        "create-original",
        &["project", "create-original", text(&package)?, text(&movie)?],
    )?)?;
    let asset = AssetId::new(
        created["created"]["asset_id"]
            .as_str()
            .ok_or("create-original asset_id")?,
    )?;
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    let document = store.snapshot()?;
    assert_eq!(
        document.presentation_basis().frame_rate,
        FrameRate::new(30_000, 1_001)?
    );
    assert_eq!(document.nodes().len(), 2);
    let receipt = store.registered_source(document.revision_id(), &asset)?;
    assert_eq!(receipt.snapshot().origin_seconds(), ExactRatio::ZERO);
    let qualified_video = receipt
        .snapshot()
        .video()
        .ok_or("fixture has no qualified video")?;
    assert_eq!(
        qualified_video.interpretation().color.transfer,
        ColorTransfer::Bt709
    );
    assert_eq!(
        qualified_video.interpretation().color.primaries,
        ColorPrimaries::Bt709
    );
    let video = qualified_video.index().index();
    assert_eq!(video.time_base(), SourceTimeBase::new(1, 30_000)?);
    assert_eq!(video.frames().len(), SOURCE_FRAMES as usize);
    assert_eq!(video.terminal_end(), 100_100);
    for (ordinal, frame) in video.frames().iter().enumerate() {
        assert_eq!(frame.pts, ordinal as i64 * 1001);
    }
    let audio = receipt
        .snapshot()
        .audio()
        .ok_or("fixture has no qualified audio")?;
    assert_eq!(audio.valid_samples(), 160_160);
    run.write_json(
        "source-qualification.json",
        &serde_json::to_value(receipt.snapshot())?,
    )?;
    run.write_json("fixture.json", &json!({
        "frames": 100, "rate": [30000, 1001], "samples": 160160,
        "sample_rate": 48000, "channels": ["left", "right"],
        "source_sha256": sha256(&movie)?, "ffmpeg_sha256": ffmpeg_sha256,
        "oracle": "direct Original AAC decode at original PTS; checked against authored chirps",
        "recipe": "binary frame ordinal in seven bars; stereo chirps with separate frequencies and burst envelopes",
    }))?;
    // Input recipes remain small; their hashes and deterministic implementation
    // suffice after qualification, without retaining 17 MB of raw RGB per run.
    fs::remove_file(raw_video)?;
    Ok(Fixture {
        movie,
        package,
        asset,
        initial_revision: document.revision_id().clone(),
        pcm,
    })
}

fn write_video(path: &std::path::Path) -> Result {
    let mut file = BufWriter::new(File::create(path)?);
    for ordinal in 0..100_u8 {
        // Deliberately authored nonlinear BT.709 code values, not linear-light
        // RGB or sRGB photographs. Matrix conversion preserves this transfer.
        let mut frame = vec![48_u8; 320 * 180 * 3];
        for bit in 0..7 {
            let value = if ordinal & (1 << bit) != 0 { 224 } else { 32 };
            for y in 50..140 {
                for x in (10 + bit * 42)..(38 + bit * 42) {
                    frame[(y * 320 + x) * 3..(y * 320 + x) * 3 + 3].fill(value);
                }
            }
        }
        file.write_all(&frame)?;
    }
    file.flush()?;
    Ok(())
}

/// Smooth, channel-distinct and nonperiodic chirps make an exact sample shift
/// observable, without asking AAC to preserve discontinuous white noise.
fn authored_sample(sample: i64, channel: usize) -> i16 {
    if !(256..159_904).contains(&sample) {
        return 0;
    }
    let t = sample as f64 / 48_000.0;
    let frequency = if channel == 0 { 233.0 } else { 419.0 };
    let sweep = if channel == 0 { 37.0 } else { 53.0 };
    let signal = (std::f64::consts::TAU * (frequency * t + sweep * t * t)).sin();
    let burst = if (12_000..36_000).contains(&sample)
        || (79_000..108_000).contains(&sample)
        || (128_000..151_000).contains(&sample)
    {
        3_500.0
    } else {
        1_200.0
    };
    let edge = ((sample - 256).min(159_904 - sample) as f64 / 1024.0).min(1.0);
    (signal * burst * edge).round() as i16
}

fn decode_original(run: &Run, movie: &std::path::Path) -> Result<Vec<[f32; 2]>> {
    let mut video = SourceDecoder::open(
        File::open(movie)?,
        DecodeLimits {
            max_input_bytes: 16 * 1024 * 1024,
            max_frames: 102,
            max_packets: 1024,
            max_pixels: 320 * 192,
            max_dimension: 320,
            ..DecodeLimits::default()
        },
        run.control()?,
    )?;
    assert_eq!(
        (video.info().time_base_num, video.info().time_base_den),
        (1, 30_000)
    );
    assert_eq!(video.info().color.transfer, ColorTransfer::Bt709);
    assert_eq!(video.info().color.primaries, ColorPrimaries::Bt709);
    let mut ordinal = 0_i64;
    while let Some(frame) = video.next_rgba(run.control()?)? {
        assert!(ordinal < SOURCE_FRAMES, "fixture emitted excess pictures");
        assert_eq!(frame.metadata.pts, ordinal * 1001);
        assert_eq!(frame.metadata.reported_duration, Some(1001));
        assert_eq!((frame.width, frame.height), (320, 180));
        let mut visible = 0_u8;
        for bit in 0..7 {
            let at = 90 * frame.row_stride_bytes + (24 + bit * 42) * 4;
            if frame.rgba[at] > 128 {
                visible |= 1 << bit;
            }
        }
        assert_eq!(i64::from(visible), ordinal, "authored pixel ordinal");
        ordinal += 1;
    }
    assert_eq!(ordinal, SOURCE_FRAMES);
    let mut audio = AudioDecoder::open_first(
        File::open(movie)?,
        AudioDecodeLimits {
            max_input_bytes: 16 * 1024 * 1024,
            max_decoded_samples: 164_000,
            max_frames: 200,
            ..AudioDecodeLimits::default()
        },
        run.control()?,
    )?;
    assert_eq!(audio.info().sample_rate, 48_000);
    assert_eq!(
        (audio.info().time_base_num, audio.info().time_base_den),
        (1, 48_000)
    );
    assert_eq!(
        audio.info().channel_layout,
        AudioChannelLayout::Native {
            channels: 2,
            mask: 3
        }
    );
    let mut pcm = Vec::with_capacity(160_160);
    let mut previous_end = None;
    let mut error_energy = 0_f64;
    let mut signal_energy = 0_f64;
    while let Some(metadata) = audio.next_metadata(run.control()?)? {
        if let Some(end) = previous_end {
            assert_eq!(metadata.pts, end, "Original audio PTS continuity");
        }
        previous_end = Some(metadata.pts + i64::from(metadata.nb_samples));
        let frame = audio.copy_current_interleaved_f32(run.control()?)?;
        assert_eq!(frame.samples.len(), metadata.nb_samples as usize * 2);
        for (offset, pair) in frame.samples.chunks_exact(2).enumerate() {
            let sample = metadata.pts + offset as i64;
            if !(0..160_160).contains(&sample) {
                continue;
            }
            assert_eq!(
                sample as usize,
                pcm.len(),
                "Original PCM is placed by PTS, never aligned by events"
            );
            assert!(pair.iter().all(|value| value.is_finite()));
            for (channel, actual) in pair.iter().enumerate() {
                let expected = f64::from(authored_sample(sample, channel)) / 32768.0;
                signal_energy += expected * expected;
                error_energy += (f64::from(*actual) - expected).powi(2);
            }
            pcm.push([pair[0], pair[1]]);
        }
    }
    assert_eq!(pcm.len(), 160_160);
    let snr = 10.0 * (signal_energy / error_energy).log10();
    assert!(
        snr > 20.0,
        "fixture AAC differs from authored waveform or PTS origin: {snr} dB"
    );
    run.write_json(
        "original-pcm.json",
        &json!({"samples": pcm.len(), "authored_snr_db": snr,
        "physical_end": previous_end, "placement": "original PTS, no offset correction"}),
    )?;
    Ok(pcm)
}
