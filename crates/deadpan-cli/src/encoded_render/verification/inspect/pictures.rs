use deadpan_encode::{HdrTransfer, VideoFormat};
use deadpan_source::{
    ChromaLocation, ColorMatrix, ColorMetadata, ColorPrimaries, ColorRange, ColorTransfer,
    DecodeControl, ExportFrameMetadata, Mp4TrackInspection, ObservedSampleAspectRatio, PictureType,
    SourceDecoder,
};
use sha2::{Digest, Sha256};

use super::{
    Context, Result,
    light::{DecodedLight, LightMeter},
    require,
};
use crate::encoded_render::verification::{VerificationProgress, VerificationStage};

pub(super) struct PictureResult {
    pub fresh_frames: u64,
    pub maximum_b_run: u32,
    pub runtime_versions: [u32; 3],
    /// PQ only: content light recomputed from every continuously decoded picture.
    pub light: Option<DecodedLight>,
}

/// One decoded picture in the contract's exact sample layout. SDR keeps the
/// tight 8-bit I420 bytes; HDR keeps tight 10-bit Y, Cb, Cr samples.
struct Frame {
    metadata: ExportFrameMetadata,
    width: u32,
    height: u32,
    pixels: Pixels,
}

enum Pixels {
    I420(Vec<u8>),
    P10(Vec<u16>),
}

impl Pixels {
    fn bytes(&self) -> u64 {
        match self {
            Self::I420(bytes) => bytes.len() as u64,
            Self::P10(samples) => samples.len() as u64 * 2,
        }
    }
}

/// Exact decoded expectations per output format. SDR values are unchanged.
struct Expected {
    codec: &'static str,
    pixel_format: &'static str,
    profile: i32,
    transfer: Option<HdrTransfer>,
}

fn expected(format: VideoFormat) -> Expected {
    match format.hdr_transfer() {
        None => Expected {
            codec: "h264",
            pixel_format: "yuv420p",
            profile: 100,
            transfer: None,
        },
        Some(transfer) => Expected {
            codec: "hevc",
            pixel_format: "yuv420p10le",
            // FFmpeg AV_PROFILE_HEVC_MAIN_10.
            profile: 2,
            transfer: Some(transfer),
        },
    }
}

fn next(
    decoder: &mut SourceDecoder,
    expected: &Expected,
    control: DecodeControl<'_>,
) -> Result<Option<Frame>> {
    if expected.transfer.is_some() {
        Ok(decoder
            .next_yuv420p10(control)
            .map_err(|e| e.to_string())?
            .map(|frame| Frame {
                metadata: frame.metadata,
                width: frame.width,
                height: frame.height,
                pixels: Pixels::P10(frame.samples),
            }))
    } else {
        Ok(decoder
            .next_i420(control)
            .map_err(|e| e.to_string())?
            .map(|frame| Frame {
                metadata: frame.metadata,
                width: frame.width,
                height: frame.height,
                pixels: Pixels::I420(frame.i420),
            }))
    }
}

/// The decoder admits HDR frames only when every frame repeats the stream's
/// captured interpretation and static metadata, so the stream description is
/// the per-picture color observation compared with the contract here.
fn hdr_color(
    context: &Context<'_>,
    track: &Mp4TrackInspection,
    transfer: HdrTransfer,
    color: &ColorMetadata,
) -> Result<()> {
    let mastering = context.manifest.contract.picture.mastering_display;
    require(
        color.range == ColorRange::Limited
            && color.matrix == ColorMatrix::Bt2020NonConstant
            && color.primaries == ColorPrimaries::Bt2020
            && color.transfer
                == match transfer {
                    HdrTransfer::Pq => ColorTransfer::Pq,
                    HdrTransfer::Hlg => ColorTransfer::Hlg,
                }
            && color
                .mastering
                .map(|m| (m.primaries, m.white_point, m.max_luminance, m.min_luminance))
                == mastering
                    .map(|m| (m.primaries, m.white_point, m.max_luminance, m.min_luminance))
            && color.content_light.map(|l| (l.max_cll, l.max_fall))
                == track.content_light.map(|l| (l.max_cll, l.max_fall)),
        "decoded HDR color interpretation or static metadata differs from the contract",
    )
}

pub(super) fn inspect(
    context: &Context<'_>,
    track: &Mp4TrackInspection,
    keys: &[bool],
    hevc_reorder_delay: Option<u32>,
    progress: &mut impl FnMut(VerificationProgress) -> Result<()>,
) -> Result<PictureResult> {
    let native = context.manifest.contract.native_contract()?;
    let expected = expected(native.video_format());
    let mut linear = SourceDecoder::open(context.file()?, context.limits, context.control()?)
        .map_err(|e| e.to_string())?;
    let info = linear.info();
    if let Some(transfer) = expected.transfer {
        hdr_color(context, track, transfer, &info.color)?;
    }
    require(
        !info.bwdif_fields
            && info.stream_index == track.index
            && info.time_base_num == 1
            && info.time_base_den == track.media_timescale
            && info.stream_start == Some(0)
            && info.stream_duration == Some(context.manifest.contract.picture.terminal_pts)
            && [info.width, info.height] == native.raster()
            && info.rotation_quarter_turns == 0
            && info.codec == expected.codec
            && info.pixel_format == expected.pixel_format
            && info.sample_aspect_num == info.sample_aspect_den,
        "decoded video stream differs from captured timing or interpretation",
    )?;
    let runtime = linear.runtime_info();
    let mut fresh =
        SourceDecoder::open_at_keyframe(context.file()?, context.limits, context.control()?, 0)
            .map_err(|e| e.to_string())?;
    let mut start = 0_u64;
    let mut count = 0_u64;
    let mut digest = Sha256::new();
    let mut run = 0_u32;
    let mut maximum_b_run = 0_u32;
    let mut fresh_frames = 0_u64;
    let mut meter = match expected.transfer {
        Some(HdrTransfer::Pq) => Some(LightMeter::new(native.raster()[0])?),
        _ => None,
    };
    for ordinal in 0..native.video_frames() {
        let frame = next(&mut linear, &expected, context.control()?)?
            .ok_or("video decoder ended before captured frame count")?;
        let key = keys[usize::try_from(ordinal).map_err(|_| "picture ordinal overflow")?];
        validate(context, &expected, &frame, ordinal, key)?;
        if let (Some(meter), Pixels::P10(samples)) = (&mut meter, &frame.pixels) {
            meter.add(frame.width, frame.height, samples)?;
        }
        if key && ordinal != 0 {
            require(
                count <= u64::from(native.policy().gop_frames) + 1,
                "decoded GOP exceeds qualified interval",
            )?;
            fresh_frames += compare_gop(
                context,
                &expected,
                &mut fresh,
                start,
                count,
                digest.finalize().into(),
                keys,
                false,
            )?;
            start = ordinal;
            count = 0;
            digest = Sha256::new();
        }
        if let Some(delay) = hevc_reorder_delay {
            // HEVC slice types do not identify reordered pictures: the
            // VideoToolbox stream codes inter pictures as B slices in a
            // pyramid. The packet clock's reorder delay is the policy bound.
            maximum_b_run = delay;
        } else {
            run = if frame.metadata.picture_type == PictureType::B {
                run + 1
            } else {
                0
            };
            maximum_b_run = maximum_b_run.max(run);
        }
        require(
            run <= native.policy().b_frames && maximum_b_run <= native.policy().b_frames,
            "actual consecutive B pictures exceed policy",
        )?;
        append(&mut digest, &frame);
        count += 1;
        check_work(context, &linear, &fresh)?;
        if crate::render_worker::worker::progress_due(ordinal + 1, native.video_frames()) {
            progress(VerificationProgress {
                stage: VerificationStage::Pictures,
                completed: ordinal + 1,
                total: native.video_frames(),
            })?;
        }
    }
    require(
        next(&mut linear, &expected, context.control()?)?.is_none(),
        "decoded extra picture after captured endpoint",
    )?;
    require(
        count > 0 && count <= u64::from(native.policy().gop_frames) + 1,
        "terminal GOP exceeds qualified interval",
    )?;
    fresh_frames += compare_gop(
        context,
        &expected,
        &mut fresh,
        start,
        count,
        digest.finalize().into(),
        keys,
        true,
    )?;
    require(
        native.policy().b_frames == 0
            || native.video_frames() <= u64::from(native.policy().gop_frames)
            || maximum_b_run > 0,
        "requested B-frame path produced no actual reordered pictures",
    )?;
    check_work(context, &linear, &fresh)?;
    Ok(PictureResult {
        fresh_frames,
        maximum_b_run,
        runtime_versions: [runtime.avcodec, runtime.avformat, runtime.avutil],
        light: meter.map(|meter| meter.light()),
    })
}

fn square(value: ObservedSampleAspectRatio, required: bool) -> bool {
    if value.numerator == 0 {
        !required
    } else {
        value.numerator > 0 && value.numerator == value.denominator
    }
}

fn validate(
    context: &Context<'_>,
    expected: &Expected,
    frame: &Frame,
    ordinal: u64,
    key: bool,
) -> Result<()> {
    let native = context.manifest.contract.native_contract()?;
    let (pts, duration) = native.picture_timing(ordinal).map_err(|e| e.to_string())?;
    let m = &frame.metadata;
    require(
        [frame.width, frame.height] == native.raster()
            && frame.pixels.bytes() == native.picture_bytes()
            && m.source.pts == pts
            && m.best_effort_pts == Some(pts)
            && m.source.reported_duration == Some(duration)
            && m.source.keyframe == key
            && m.decoder_profile == expected.profile
            // This descriptor-only decoder does not run find_stream_info.
            // codecpar may retain FF_PROFILE_UNKNOWN; avcC/hvcC and the actual
            // decoder must independently identify High or Main10 profile.
            && (m.codec_profile == -99 || m.codec_profile == expected.profile)
            // Rejects the measured OS-software PQ HEVC stream, which declares
            // top-left chroma siting for left-sited input samples.
            && m.chroma_location == ChromaLocation::Left
            && m.decode_error_flags == 0
            && !m.corrupt
            && !m.interlaced
            && !m.top_field_first
            && square(m.stream_sample_aspect_ratio, true)
            && square(m.codec_sample_aspect_ratio, false)
            && square(m.frame_sample_aspect_ratio, false)
            && matches!(
                m.picture_type,
                PictureType::I | PictureType::P | PictureType::B
            )
            && (!key || m.picture_type == PictureType::I),
        "decoded picture differs from exact output contract",
    )
    .map_err(|error| {
        format!(
            "{error}; ordinal={ordinal}, raster={}x{}, bytes={}, observed={m:?}",
            frame.width,
            frame.height,
            frame.pixels.bytes()
        )
    })
}

fn append(digest: &mut Sha256, frame: &Frame) {
    digest.update(frame.metadata.source.pts.to_le_bytes());
    digest.update(
        frame
            .metadata
            .source
            .reported_duration
            .unwrap_or(0)
            .to_le_bytes(),
    );
    match &frame.pixels {
        Pixels::I420(bytes) => digest.update(bytes),
        Pixels::P10(samples) => {
            let mut buffer = [0_u8; 8192];
            for chunk in samples.chunks(buffer.len() / 2) {
                for (bytes, sample) in buffer.chunks_exact_mut(2).zip(chunk) {
                    bytes.copy_from_slice(&sample.to_le_bytes());
                }
                digest.update(&buffer[..chunk.len() * 2]);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn compare_gop(
    context: &Context<'_>,
    expected: &Expected,
    fresh: &mut SourceDecoder,
    start: u64,
    count: u64,
    gop_digest: [u8; 32],
    keys: &[bool],
    final_gop: bool,
) -> Result<u64> {
    let native = context.manifest.contract.native_contract()?;
    if start != 0 {
        let (pts, _) = native.picture_timing(start).map_err(|e| e.to_string())?;
        fresh
            .restart_at_keyframe(pts, context.control()?)
            .map_err(|e| e.to_string())?;
    }
    let mut digest = Sha256::new();
    for ordinal in start..start + count {
        let frame =
            next(fresh, expected, context.control()?)?.ok_or("fresh GOP decoder ended early")?;
        validate(
            context,
            expected,
            &frame,
            ordinal,
            keys[usize::try_from(ordinal).map_err(|_| "picture ordinal overflow")?],
        )?;
        append(&mut digest, &frame);
        // Counts are cumulative across fresh codec restarts, not reset limits.
        let work = fresh.work();
        require(
            work.frames <= native.video_frames() && work.packets <= context.limits.max_packets * 4,
            "fresh GOP verification exceeded its whole-file decode budget",
        )?;
    }
    require(
        <[u8; 32]>::from(digest.finalize()) == gop_digest,
        "fresh GOP pixels or timestamps differ from continuous decode",
    )?;
    if final_gop {
        require(
            next(fresh, expected, context.control()?)?.is_none(),
            "fresh terminal GOP has extra pictures",
        )?;
    }
    Ok(count)
}

fn check_work(context: &Context<'_>, linear: &SourceDecoder, fresh: &SourceDecoder) -> Result<()> {
    let frames = context.manifest.contract.picture.frame_count;
    require(
        linear.work().frames <= frames
            && fresh.work().frames <= frames
            && linear.work().packets + fresh.work().packets <= context.limits.max_packets * 6,
        "video verification exceeded its whole-file work budget",
    )
}
