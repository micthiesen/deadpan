//! Emitted-movie inspection with the qualified source decoders. Pictures and
//! audio stream in presentation order; timing anomalies are reported, not
//! repaired, and only the audio windows currently being compared are retained.

use std::{
    fs::File,
    ops::Range,
    path::Path,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

use deadpan_source::{
    ChromaLocation, ColorMatrix, ColorPrimaries, ColorRange, ColorTransfer, DecodeControl,
    DecodeLimits, Mp4Inspection, Mp4TrackInspection, Mp4TrackKind, SourceDecoder,
    audio::{AudioChannelLayout, AudioDecodeLimits, AudioDecodeMode, AudioDecoder},
    inspect_mp4,
};
use serde::Serialize;

use super::{VerifyError, metrics::I420};

const MAX_CALL: Duration = Duration::from_secs(60);
/// Retained decoded audio across simultaneously open windows (about 64 MiB).
pub const MAX_RETAINED_AUDIO_SAMPLES: usize = 8 * 1024 * 1024;

pub(super) fn control<'a>(
    cancelled: &'a AtomicBool,
    deadline: Instant,
) -> Result<DecodeControl<'a>, VerifyError> {
    super::check(cancelled, deadline)?;
    Ok(DecodeControl {
        cancelled,
        timeout: deadline
            .saturating_duration_since(Instant::now())
            .min(MAX_CALL),
    })
}

fn open(path: &Path) -> Result<File, VerifyError> {
    let file = File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(VerifyError::Movie("movie is not a regular file".into()));
    }
    Ok(file)
}

fn limits(raster: [u32; 2], frames: u64) -> DecodeLimits {
    DecodeLimits {
        // Room for extra pictures so they are reported rather than refused.
        max_frames: frames.saturating_mul(2).saturating_add(16).min(10_000_000),
        // Complete 16x16 macroblocks are stored before the visible crop.
        max_pixels: (u64::from(raster[0]).div_ceil(16) * 16)
            * (u64::from(raster[1]).div_ceil(16) * 16),
        ..DecodeLimits::default()
    }
}

pub(super) fn inspect(
    path: &Path,
    raster: [u32; 2],
    frames: u64,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<Mp4Inspection, VerifyError> {
    let file = open(path)?;
    inspect_mp4(&file, limits(raster, frames), control(cancelled, deadline)?)
        .map_err(|error| VerifyError::Movie(error.to_string()))
}

pub(super) fn track(movie: &Mp4Inspection, kind: Mp4TrackKind) -> Option<&Mp4TrackInspection> {
    movie.tracks.iter().find(|track| track.kind == kind)
}

/// The one edit of a track, converted to its media clock. The priming check
/// derived from it is partly self-referential (the decoder applies the same
/// edit); the content alignment is the real timing guarantee.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct EditSummary {
    pub edits: usize,
    pub empty_edits: usize,
    /// Media ticks skipped at the start (priming or reorder delay).
    pub media_time: Option<i64>,
    /// Presented duration in the track's media ticks, exact or None.
    pub presented_media_ticks: Option<i64>,
}

pub(super) fn edits(movie: &Mp4Inspection, track: &Mp4TrackInspection) -> EditSummary {
    let empty = track
        .edits
        .iter()
        .filter(|edit| edit.media_time == -1)
        .count();
    let first = track.edits.first();
    let presented = first.and_then(|edit| {
        let scaled = i128::from(edit.segment_duration) * i128::from(track.media_timescale);
        let timescale = i128::from(movie.movie_timescale);
        (timescale > 0 && scaled % timescale == 0)
            .then(|| i64::try_from(scaled / timescale).ok())
            .flatten()
    });
    EditSummary {
        edits: track.edits.len(),
        empty_edits: empty,
        media_time: first.map(|edit| edit.media_time),
        presented_media_ticks: presented,
    }
}

/// Container color tags and the decoder's interpretation, which must match
/// the contract's progressive limited Rec.709 with left-sited chroma.
#[derive(Clone, Debug, Default, Serialize)]
pub struct ColorObservation {
    pub container: Option<[u16; 3]>,
    pub container_full_range: Option<bool>,
    pub decoded: Option<String>,
    pub chroma_location: Option<String>,
    pub problems: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PictureAnomaly {
    pub decoded_index: u64,
    pub pts: i64,
    pub problem: String,
}

pub(super) struct PictureStream {
    pub decoded: u64,
    pub anomalies: Vec<PictureAnomaly>,
    pub color: ColorObservation,
}

/// Calls `visit(ordinal, picture)` for each decoded picture whose PTS maps
/// exactly onto an output ordinal inside the committed range. Off-grid,
/// duplicate, out-of-order and out-of-range pictures and missing ordinals are
/// recorded as anomalies; a picture is compared at the ordinal its own PTS
/// claims, so a shifted stream shows up as content mismatch too.
/// The committed output picture grid.
#[derive(Clone, Copy)]
pub(super) struct PictureGrid {
    pub raster: [u32; 2],
    pub frames: u64,
    /// Output frame rate N/D.
    pub rate: (u32, u32),
}

pub(super) fn pictures(
    path: &Path,
    grid: PictureGrid,
    track: Option<&Mp4TrackInspection>,
    cancelled: &AtomicBool,
    deadline: Instant,
    mut visit: impl FnMut(u64, I420) -> Result<(), VerifyError>,
) -> Result<PictureStream, VerifyError> {
    let PictureGrid {
        raster,
        frames,
        rate,
    } = grid;
    let file = open(path)?;
    let mut decoder =
        SourceDecoder::open(file, limits(raster, frames), control(cancelled, deadline)?)
            .map_err(|error| VerifyError::Movie(error.to_string()))?;
    let info = decoder.info().clone();
    let mut color = ColorObservation::default();
    if let Some(described) = track.and_then(|track| track.color) {
        color.container = Some([described.primaries, described.transfer, described.matrix]);
        color.container_full_range = Some(described.full_range);
        if [described.primaries, described.transfer, described.matrix] != [1, 1, 1]
            || described.full_range
        {
            color
                .problems
                .push("container color tags are not limited-range Rec.709".into());
        }
    } else {
        color
            .problems
            .push("movie has no container color description".into());
    }
    color.decoded = Some(format!("{:?}", info.color));
    if info.color.range != ColorRange::Limited
        || info.color.matrix != ColorMatrix::Bt709
        || info.color.transfer != ColorTransfer::Bt709
        || info.color.primaries != ColorPrimaries::Bt709
    {
        color
            .problems
            .push("decoded color interpretation is not limited-range Rec.709".into());
    }
    if [info.width, info.height] != raster || info.rotation_quarter_turns != 0 {
        return Err(VerifyError::Timing(format!(
            "movie picture raster {}x{} rotation {} differs from the committed {}x{}",
            info.width, info.height, info.rotation_quarter_turns, raster[0], raster[1]
        )));
    }
    // ordinal = pts * tb_num * N / (tb_den * D), required to be an exact integer.
    let numerator = i128::from(info.time_base_num) * i128::from(rate.0);
    let denominator = i128::from(info.time_base_den) * i128::from(rate.1);
    let mut anomalies = Vec::new();
    let mut decoded = 0_u64;
    let mut next = 0_u64;
    while let Some(frame) = decoder
        .next_i420(control(cancelled, deadline)?)
        .map_err(|error| VerifyError::Movie(error.to_string()))?
    {
        let index = decoded;
        decoded += 1;
        let pts = frame.metadata.source.pts;
        let mut anomaly = |problem: String| {
            anomalies.push(PictureAnomaly {
                decoded_index: index,
                pts,
                problem,
            });
        };
        if frame.metadata.chroma_location != ChromaLocation::Left {
            let location = format!("{:?}", frame.metadata.chroma_location);
            if color.chroma_location.as_deref() != Some(location.as_str()) {
                color
                    .problems
                    .push(format!("decoded chroma location is {location}, not Left"));
            }
            color.chroma_location = Some(location);
        } else if color.chroma_location.is_none() {
            color.chroma_location = Some("Left".into());
        }
        let scaled = i128::from(pts) * numerator;
        if denominator == 0 || scaled % denominator != 0 {
            anomaly("picture PTS is not on the output frame grid".into());
            continue;
        }
        let Ok(ordinal) = u64::try_from(scaled / denominator) else {
            anomaly("picture PTS precedes the output start".into());
            continue;
        };
        if ordinal >= frames {
            anomaly(format!(
                "picture ordinal {ordinal} is beyond the committed {frames} frames"
            ));
            continue;
        }
        if ordinal < next {
            anomaly(format!(
                "picture ordinal {ordinal} repeats or is out of order"
            ));
            continue;
        }
        if ordinal > next {
            anomaly(format!("output ordinals {next}..{ordinal} are missing"));
        }
        next = ordinal + 1;
        let picture = I420::from_tight(frame.width, frame.height, &frame.i420)
            .ok_or_else(|| VerifyError::Movie("decoded picture is not tight I420".into()))?;
        visit(ordinal, picture)?;
    }
    if next < frames {
        anomalies.push(PictureAnomaly {
            decoded_index: decoded,
            pts: -1,
            problem: format!("output ordinals {next}..{frames} are missing at the end"),
        });
    }
    Ok(PictureStream {
        decoded,
        anomalies,
        color,
    })
}

/// Decoded samples for one requested range. `covered[i]` is false where the
/// file presents nothing at that output sample; such samples read as zero.
pub(super) struct RangeSamples {
    pub range: Range<i64>,
    pub samples: Vec<[f32; 2]>,
    pub covered: Vec<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AudioGap {
    /// Output sample where the next decoded frame should have started.
    pub expected: i64,
    pub actual: i64,
}

#[derive(Debug, Serialize)]
pub struct AudioStream {
    pub first_pts: Option<i64>,
    /// Physically decoded samples, including priming and padding.
    pub physical_samples: u64,
    /// End of decoded samples inside the declared presentation, in output samples.
    pub presented_end: i64,
    pub gaps: Vec<AudioGap>,
}

/// Decode AAC in Manual mode so the physical priming stays visible. Decoded
/// PTS already carries the MP4 edit; output sample `t` is PTS `t`. Samples are
/// placed by PTS and never moved. Priming samples before zero are retained in
/// a range that reaches below zero, only as alignment context so an early
/// shift is observable; they are not counted as presented. `visit` receives
/// each range (sorted by start) as soon as decoding passes its end.
pub(super) fn audio(
    path: &Path,
    track_index: u32,
    presented_samples: i64,
    ranges: &[Range<i64>],
    cancelled: &AtomicBool,
    deadline: Instant,
    mut visit: impl FnMut(usize, RangeSamples) -> Result<(), VerifyError>,
) -> Result<AudioStream, VerifyError> {
    let file = open(path)?;
    let limits = AudioDecodeLimits {
        // FFmpeg's AAC decoder allocates a 2048-sample internal frame.
        max_samples_per_frame: 2048,
        max_channels: 2,
        max_sample_rate: 48_000,
        ..AudioDecodeLimits::default()
    };
    let mut decoder = AudioDecoder::open_with_mode(
        file,
        track_index,
        AudioDecodeMode::Manual,
        limits,
        control(cancelled, deadline)?,
    )
    .map_err(|error| VerifyError::Movie(error.to_string()))?;
    let info = decoder.info().clone();
    let stereo = AudioChannelLayout::Native {
        channels: 2,
        mask: 3,
    };
    if info.sample_rate != 48_000
        || info.time_base_num != 1
        || info.time_base_den != 48_000
        || info.channel_layout != stereo
    {
        return Err(VerifyError::Movie(format!(
            "movie audio is not 48 kHz stereo on a 1/48000 clock: {info:?}"
        )));
    }
    let mut order: Vec<usize> = (0..ranges.len()).collect();
    order.sort_by_key(|&index| ranges[index].start);
    let mut pending = order.into_iter().peekable();
    let mut open: Vec<(usize, RangeSamples)> = Vec::new();
    let mut retained = 0_usize;
    let mut stream = AudioStream {
        first_pts: None,
        physical_samples: 0,
        presented_end: 0,
        gaps: Vec::new(),
    };
    let mut next: Option<i64> = None;
    let mut flush = |open: &mut Vec<(usize, RangeSamples)>,
                     retained: &mut usize,
                     through: i64|
     -> Result<(), VerifyError> {
        let mut index = 0;
        while index < open.len() {
            if open[index].1.range.end <= through {
                let (window, samples) = open.swap_remove(index);
                *retained -= samples.samples.len();
                visit(window, samples)?;
            } else {
                index += 1;
            }
        }
        Ok(())
    };
    loop {
        let metadata = decoder
            .next_metadata(control(cancelled, deadline)?)
            .map_err(|error| VerifyError::Movie(error.to_string()))?;
        let Some(metadata) = metadata else {
            break;
        };
        stream.first_pts.get_or_insert(metadata.pts);
        if let Some(expected) = next
            && expected != metadata.pts
        {
            stream.gaps.push(AudioGap {
                expected,
                actual: metadata.pts,
            });
        }
        let frame = decoder
            .copy_current_interleaved_f32(control(cancelled, deadline)?)
            .map_err(|error| VerifyError::Movie(error.to_string()))?;
        let start = metadata.pts;
        let count = i64::from(metadata.nb_samples);
        // Codec end padding beyond the declared presentation is not output.
        let end = (start + count).min(presented_samples.max(start));
        next = Some(start + count);
        stream.physical_samples += u64::from(metadata.nb_samples);
        if end > 0 {
            stream.presented_end = stream.presented_end.max(end);
        }
        while let Some(&index) = pending.peek() {
            if ranges[index].start >= end {
                break;
            }
            pending.next();
            let range = ranges[index].clone();
            let length = usize::try_from(range.end - range.start)
                .map_err(|_| VerifyError::Request("audio range".into()))?;
            retained += length;
            if retained > MAX_RETAINED_AUDIO_SAMPLES {
                return Err(VerifyError::Request(format!(
                    "overlapping audio windows would retain more than {MAX_RETAINED_AUDIO_SAMPLES} samples"
                )));
            }
            open.push((
                index,
                RangeSamples {
                    range,
                    samples: vec![[0.0; 2]; length],
                    covered: vec![false; length],
                },
            ));
        }
        for (_, buffer) in &mut open {
            let low = buffer.range.start.max(start);
            let high = buffer.range.end.min(end);
            for sample in low..high {
                let source = usize::try_from(sample - start)
                    .map_err(|_| VerifyError::Movie("audio offset overflow".into()))?
                    * 2;
                let target = usize::try_from(sample - buffer.range.start)
                    .map_err(|_| VerifyError::Movie("audio offset overflow".into()))?;
                buffer.samples[target] = [frame.samples[source], frame.samples[source + 1]];
                buffer.covered[target] = true;
            }
        }
        flush(&mut open, &mut retained, start + count)?;
    }
    // Ranges the file never reached remain uncovered and are still compared.
    for index in pending {
        let range = ranges[index].clone();
        let length = usize::try_from(range.end - range.start)
            .map_err(|_| VerifyError::Request("audio range".into()))?;
        open.push((
            index,
            RangeSamples {
                range,
                samples: vec![[0.0; 2]; length],
                covered: vec![false; length],
            },
        ));
    }
    flush(&mut open, &mut retained, i64::MAX)?;
    if stream.first_pts.is_none() {
        return Err(VerifyError::Movie("movie audio decoded no frames".into()));
    }
    Ok(stream)
}
