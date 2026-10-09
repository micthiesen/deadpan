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

use deadpan_core::{ColorPolicy, ContentLight, MasteringDisplay};
use deadpan_source::{
    ChromaLocation, ColorMatrix, ColorPrimaries, ColorRange, ColorTransfer, DecodeControl,
    DecodeLimits, Mp4Inspection, Mp4TrackInspection, Mp4TrackKind, SourceDecoder,
    audio::{AudioChannelLayout, AudioDecodeLimits, AudioDecodeMode, AudioDecoder},
    inspect_mp4,
};
use serde::Serialize;

use super::{VerifyError, metrics::I420};

/// HEVC profile identifier of Main10 in both hvcC and the pinned decoder.
const HEVC_MAIN10: u8 = 2;

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

fn limits(raster: [u32; 2], frames: u64, hdr: bool) -> DecodeLimits {
    DecodeLimits {
        progressive_only: true,
        // Room for extra pictures so they are reported rather than refused.
        max_frames: frames.saturating_mul(2).saturating_add(16).min(10_000_000),
        max_pixels: crate::encoded_render::verification::decode_pixel_budget(raster, hdr),
        ..DecodeLimits::default()
    }
}

#[cfg(test)]
#[test]
fn emitted_movie_cannot_be_repaired_by_source_deinterlacing() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/fixtures/fields-tff.mp4");
    let result = pictures(
        &path,
        PictureGrid {
            raster: [96, 64],
            frames: 24,
            rate: (50, 1),
        },
        ExpectedColor {
            policy: ColorPolicy::SdrRec709,
            mastering: None,
        },
        None,
        &AtomicBool::new(false),
        Instant::now() + Duration::from_secs(10),
        |_, _| panic!("interlaced output must fail before comparing filtered pixels"),
    );
    assert!(matches!(result, Err(VerifyError::Movie(message)) if message.contains("progressive")));
}

/// The committed output branch the movie must carry.
#[derive(Clone, Copy, Debug)]
pub(super) struct ExpectedColor {
    pub policy: ColorPolicy,
    /// PQ only: the mastering volume the contract retained, if any.
    pub mastering: Option<MasteringDisplay>,
}

impl ExpectedColor {
    pub const fn hdr(&self) -> Option<ColorTransfer> {
        match self.policy {
            ColorPolicy::SdrRec709 => None,
            ColorPolicy::HdrRec2020Pq => Some(ColorTransfer::Pq),
            ColorPolicy::HdrRec2020Hlg => Some(ColorTransfer::Hlg),
        }
    }
}

pub(super) fn inspect(
    path: &Path,
    raster: [u32; 2],
    frames: u64,
    hdr: bool,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<Mp4Inspection, VerifyError> {
    let file = open(path)?;
    inspect_mp4(
        &file,
        limits(raster, frames, hdr),
        control(cancelled, deadline)?,
    )
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
/// the contract's branch: progressive limited Rec.709 H.264 for SDR, or
/// limited BT.2020 NCL PQ/HLG HEVC Main10 (`hvc1`) for HDR, always with
/// left-sited chroma. HDR-only observations are omitted for SDR movies.
#[derive(Clone, Debug, Default, Serialize)]
pub struct ColorObservation {
    pub container: Option<[u16; 3]>,
    pub container_full_range: Option<bool>,
    pub decoded: Option<String>,
    pub chroma_location: Option<String>,
    /// `hvc1` or `avc1`, reported for HDR movies.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_entry: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hevc_profile_idc: Option<u8>,
    /// Decoder profile of the first picture (HEVC Main10 is 2), HDR only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decoder_profile: Option<i32>,
    /// Container `mdcv`, HDR only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mastering: Option<MasteringDisplay>,
    /// Container `clli`, HDR only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_light: Option<ContentLight>,
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
    /// Decoded pictures were classified as the wrong dynamic range for the
    /// committed branch and therefore not compared.
    pub wrong_branch: bool,
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

/// Container (sample description, `colr`, `mdcv`, `clli`) checks for the
/// committed branch. The SDR checks are unchanged from the original harness.
fn container_color(
    track: Option<&Mp4TrackInspection>,
    expected: ExpectedColor,
    color: &mut ColorObservation,
) {
    if let Some(described) = track.and_then(|track| track.color) {
        color.container = Some([described.primaries, described.transfer, described.matrix]);
        color.container_full_range = Some(described.full_range);
    }
    let Some(transfer) = expected.hdr() else {
        match track.and_then(|track| track.color) {
            Some(described)
                if [described.primaries, described.transfer, described.matrix] == [1, 1, 1]
                    && !described.full_range => {}
            Some(_) => color
                .problems
                .push("container color tags are not limited-range Rec.709".into()),
            None => color
                .problems
                .push("movie has no container color description".into()),
        }
        return;
    };
    let transfer_code = if transfer == ColorTransfer::Pq {
        16
    } else {
        18
    };
    match track.and_then(|track| track.color) {
        Some(described)
            if [described.primaries, described.transfer, described.matrix]
                == [9, transfer_code, 9]
                && !described.full_range => {}
        Some(_) => color.problems.push(format!(
            "container color tags are not limited-range BT.2020 NCL with transfer {transfer_code}"
        )),
        None => color
            .problems
            .push("movie has no container color description".into()),
    }
    let Some(track) = track else {
        return;
    };
    color.sample_entry = match (track.hevc, track.avc) {
        (Some(_), None) => Some("hvc1"),
        (None, Some(_)) => Some("avc1"),
        _ => None,
    };
    match track.hevc {
        Some(hevc) => {
            color.hevc_profile_idc = Some(hevc.profile_idc);
            if hevc.profile_idc != HEVC_MAIN10
                || hevc.bit_depth_luma != 10
                || hevc.bit_depth_chroma != 10
                || hevc.chroma_format_idc != 1
            {
                color.problems.push(format!(
                    "hvcC is not ten-bit 4:2:0 Main10: profile {}, depth {}/{}, chroma format {}",
                    hevc.profile_idc,
                    hevc.bit_depth_luma,
                    hevc.bit_depth_chroma,
                    hevc.chroma_format_idc
                ));
            }
        }
        None => color
            .problems
            .push("HDR movie video is not an hvc1 HEVC track".into()),
    }
    color.mastering = track.mastering.map(|value| MasteringDisplay {
        primaries: value.primaries,
        white_point: value.white_point,
        max_luminance: value.max_luminance,
        min_luminance: value.min_luminance,
    });
    color.content_light = track.content_light.map(|value| ContentLight {
        max_cll: value.max_cll,
        max_fall: value.max_fall,
    });
    if transfer == ColorTransfer::Pq {
        if color.mastering != expected.mastering {
            color.problems.push(format!(
                "container mdcv {:?} differs from the committed mastering volume {:?}",
                color.mastering, expected.mastering
            ));
        }
        if color.content_light.is_none() {
            color
                .problems
                .push("PQ movie has no clli content light box".into());
        }
    } else if color.mastering.is_some() || color.content_light.is_some() {
        color
            .problems
            .push("HLG movie carries mdcv or clli static metadata".into());
    }
}

/// Decoded interpretation checks for the committed branch.
fn decoded_color(
    info: &deadpan_source::SourceStreamInfo,
    expected: ExpectedColor,
    color: &mut ColorObservation,
) {
    color.decoded = Some(format!("{:?}", info.color));
    match expected.hdr() {
        None => {
            if info.color.range != ColorRange::Limited
                || info.color.matrix != ColorMatrix::Bt709
                || info.color.transfer != ColorTransfer::Bt709
                || info.color.primaries != ColorPrimaries::Bt709
            {
                color
                    .problems
                    .push("decoded color interpretation is not limited-range Rec.709".into());
            }
        }
        Some(transfer) => {
            if info.color.range != ColorRange::Limited
                || info.color.matrix != ColorMatrix::Bt2020NonConstant
                || info.color.transfer != transfer
                || info.color.primaries != ColorPrimaries::Bt2020
            {
                color.problems.push(format!(
                    "decoded color interpretation is not limited-range BT.2020 NCL {transfer:?}"
                ));
            }
            if info.codec != "hevc" {
                color
                    .problems
                    .push(format!("decoded HDR codec is {}, not hevc", info.codec));
            }
        }
    }
}

pub(super) fn pictures(
    path: &Path,
    grid: PictureGrid,
    expected: ExpectedColor,
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
    let hdr = expected.hdr();
    let mut decoder = SourceDecoder::open(
        file,
        limits(raster, frames, hdr.is_some()),
        control(cancelled, deadline)?,
    )
    .map_err(|error| VerifyError::Movie(error.to_string()))?;
    let info = decoder.info().clone();
    if info.bwdif_fields {
        return Err(VerifyError::Movie(
            "encoded output must contain progressive pictures".into(),
        ));
    }
    let mut color = ColorObservation::default();
    container_color(track, expected, &mut color);
    decoded_color(&info, expected, &mut color);
    if info.color.transfer.is_hdr() != hdr.is_some() {
        // Eight-bit and ten-bit pictures cannot be compared code for code.
        color.problems.push(format!(
            "movie pictures are {} but the committed branch is {:?}; pictures were not compared",
            if info.color.transfer.is_hdr() {
                "HDR"
            } else {
                "SDR"
            },
            expected.policy
        ));
        return Ok(PictureStream {
            decoded: 0,
            wrong_branch: true,
            anomalies: Vec::new(),
            color,
        });
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
    loop {
        let (metadata, picture) = if hdr.is_some() {
            let Some(frame) = decoder
                .next_yuv420p10(control(cancelled, deadline)?)
                .map_err(|error| VerifyError::Movie(error.to_string()))?
            else {
                break;
            };
            let picture = I420::from_tight_p10(frame.width, frame.height, &frame.samples)
                .ok_or_else(|| {
                    VerifyError::Movie("decoded picture is not tight yuv420p10".into())
                })?;
            (frame.metadata, picture)
        } else {
            let Some(frame) = decoder
                .next_i420(control(cancelled, deadline)?)
                .map_err(|error| VerifyError::Movie(error.to_string()))?
            else {
                break;
            };
            let picture = I420::from_tight(frame.width, frame.height, &frame.i420)
                .ok_or_else(|| VerifyError::Movie("decoded picture is not tight I420".into()))?;
            (frame.metadata, picture)
        };
        let index = decoded;
        decoded += 1;
        if hdr.is_some() && color.decoder_profile.is_none() {
            color.decoder_profile = Some(metadata.decoder_profile);
            if metadata.decoder_profile != i32::from(HEVC_MAIN10) {
                color.problems.push(format!(
                    "decoder reports HEVC profile {}, not Main10",
                    metadata.decoder_profile
                ));
            }
        }
        let pts = metadata.source.pts;
        let mut anomaly = |problem: String| {
            anomalies.push(PictureAnomaly {
                decoded_index: index,
                pts,
                problem,
            });
        };
        if metadata.chroma_location != ChromaLocation::Left {
            let location = format!("{:?}", metadata.chroma_location);
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
        wrong_branch: false,
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
