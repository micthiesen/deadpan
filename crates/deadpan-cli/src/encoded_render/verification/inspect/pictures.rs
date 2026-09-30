use deadpan_source::{
    ChromaLocation, DecodedI420Frame, Mp4TrackInspection, ObservedSampleAspectRatio, PictureType,
    SourceDecoder,
};
use sha2::{Digest, Sha256};

use super::{Context, Result, require};
use crate::encoded_render::verification::{VerificationProgress, VerificationStage};

pub(super) struct PictureResult {
    pub fresh_frames: u64,
    pub maximum_b_run: u32,
    pub runtime_versions: [u32; 3],
}

pub(super) fn inspect(
    context: &Context<'_>,
    track: &Mp4TrackInspection,
    keys: &[bool],
    progress: &mut impl FnMut(VerificationProgress) -> Result<()>,
) -> Result<PictureResult> {
    let native = context.manifest.contract.native_contract()?;
    let mut linear = SourceDecoder::open(context.file()?, context.limits, context.control()?)
        .map_err(|e| e.to_string())?;
    let info = linear.info();
    require(
        info.stream_index == track.index
            && info.time_base_num == 1
            && info.time_base_den == track.media_timescale
            && info.stream_start == Some(0)
            && info.stream_duration == Some(context.manifest.contract.picture.terminal_pts)
            && [info.width, info.height] == native.raster()
            && info.rotation_quarter_turns == 0
            && info.codec == "h264"
            && info.pixel_format == "yuv420p"
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
    for ordinal in 0..native.video_frames() {
        let frame = linear
            .next_i420(context.control()?)
            .map_err(|e| e.to_string())?
            .ok_or("video decoder ended before captured frame count")?;
        let key = keys[usize::try_from(ordinal).map_err(|_| "picture ordinal overflow")?];
        validate(context, &frame, ordinal, key)?;
        if key && ordinal != 0 {
            require(
                count <= u64::from(native.policy().gop_frames) + 1,
                "decoded GOP exceeds qualified interval",
            )?;
            fresh_frames += compare_gop(
                context,
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
        run = if frame.metadata.picture_type == PictureType::B {
            run + 1
        } else {
            0
        };
        maximum_b_run = maximum_b_run.max(run);
        require(
            run <= native.policy().b_frames,
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
        linear
            .next_i420(context.control()?)
            .map_err(|e| e.to_string())?
            .is_none(),
        "decoded extra picture after captured endpoint",
    )?;
    require(
        count > 0 && count <= u64::from(native.policy().gop_frames) + 1,
        "terminal GOP exceeds qualified interval",
    )?;
    fresh_frames += compare_gop(
        context,
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
    frame: &DecodedI420Frame,
    ordinal: u64,
    key: bool,
) -> Result<()> {
    let native = context.manifest.contract.native_contract()?;
    let (pts, duration) = native.picture_timing(ordinal).map_err(|e| e.to_string())?;
    let m = &frame.metadata;
    require(
        [frame.width, frame.height] == native.raster()
            && u64::try_from(frame.i420.len()).ok() == Some(native.picture_bytes())
            && m.source.pts == pts
            && m.best_effort_pts == Some(pts)
            && m.source.reported_duration == Some(duration)
            && m.source.keyframe == key
            && m.decoder_profile == 100
            // This descriptor-only decoder does not run find_stream_info.
            // codecpar may retain FF_PROFILE_UNKNOWN; avcC and the actual
            // decoder must independently identify High profile.
            && matches!(m.codec_profile, -99 | 100)
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
            frame.i420.len()
        )
    })
}

fn append(digest: &mut Sha256, frame: &DecodedI420Frame) {
    digest.update(frame.metadata.source.pts.to_le_bytes());
    digest.update(
        frame
            .metadata
            .source
            .reported_duration
            .unwrap_or(0)
            .to_le_bytes(),
    );
    digest.update(&frame.i420);
}

fn compare_gop(
    context: &Context<'_>,
    fresh: &mut SourceDecoder,
    start: u64,
    count: u64,
    expected: [u8; 32],
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
        let frame = fresh
            .next_i420(context.control()?)
            .map_err(|e| e.to_string())?
            .ok_or("fresh GOP decoder ended early")?;
        validate(
            context,
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
        <[u8; 32]>::from(digest.finalize()) == expected,
        "fresh GOP pixels or timestamps differ from continuous decode",
    )?;
    if final_gop {
        require(
            fresh
                .next_i420(context.control()?)
                .map_err(|e| e.to_string())?
                .is_none(),
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
