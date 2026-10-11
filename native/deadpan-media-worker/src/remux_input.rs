//! Descriptor-region admission before native stream-copy assembly.
use crate::ffi;
use deadpan_media::protocol::RemuxRequest;
use deadpan_source::{
    DecodeControl, DecodeLimits,
    audio::{AudioDecodeLimits, inspect_matroska_opus_region},
};
use std::{
    fs::File,
    io,
    os::fd::AsFd,
    os::unix::fs::FileExt,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

pub(super) fn failure_code(error: &(dyn std::error::Error + 'static)) -> &'static str {
    if let Some(error) = error.downcast_ref::<io::Error>() {
        return if error.kind() == io::ErrorKind::TimedOut {
            "deadline_exceeded"
        } else {
            "io_error"
        };
    }
    if let Some(deadpan_source::SourceDecodeError::Native { code, .. }) = error.downcast_ref() {
        return match code.as_str() {
            "deadline_exceeded" => "deadline_exceeded",
            "resource_limit" => "resource_exhausted",
            "cancelled" => "cancelled",
            _ => "invalid_media",
        };
    }
    "invalid_media"
}

pub(super) fn inspect(
    request: &RemuxRequest,
) -> Result<ffi::RemuxRequest, Box<dyn std::error::Error>> {
    let started = Instant::now();
    let timeout = Duration::from_millis(request.timeout_ms);
    let input = File::from(io::stdin().as_fd().try_clone_to_owned()?);
    let metadata = input.metadata()?;
    if !metadata.is_file()
        || request.video_byte_length < 4
        || request.audio_byte_length < 4
        || Some(metadata.len())
            != request
                .video_byte_length
                .checked_add(request.audio_byte_length)
    {
        return Err("remux descriptor does not hold exactly both inputs".into());
    }
    let cancelled = AtomicBool::new(false);
    let control = || -> Result<DecodeControl<'_>, io::Error> {
        let remaining = timeout
            .checked_sub(started.elapsed())
            .filter(|time| !time.is_zero())
            .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "remux admission deadline"))?;
        Ok(DecodeControl {
            cancelled: &cancelled,
            timeout: remaining.min(Duration::from_secs(60)),
        })
    };
    let mut result = ffi::RemuxRequest {
        video_byte_length: request.video_byte_length,
        audio_byte_length: request.audio_byte_length,
        max_output_bytes: request.max_output_bytes,
        timeout_ms: request.timeout_ms,
        opus_present: 0,
        opus_pre_skip: 0,
        opus_codec_delay_ns: 0,
        opus_timestamp_scale_ns: 0,
        opus_first_block_timestamp: 0,
        opus_packet_count: 0,
        opus_decoded_sample_count: 0,
        vp9_profile_plus_one: 0,
        vp9_bit_depth: 0,
        vp9_chroma: 0,
        vp9_primaries: 0,
        vp9_transfer: 0,
        vp9_matrix: 0,
        vp9_full_range: 0,
        chroma_assumed_left: 0,
        nominal_frame_duration_ns: 0,
    };
    let mut magic = [0; 4];
    input.read_exact_at(&mut magic, request.video_byte_length)?;
    if magic == [0x1a, 0x45, 0xdf, 0xa3] {
        let clock = inspect_matroska_opus_region(
            &input,
            request.video_byte_length,
            request.audio_byte_length,
            AudioDecodeLimits::default(),
            control()?,
        )?;
        result.opus_present = 1;
        result.opus_pre_skip = clock.pre_skip;
        result.opus_codec_delay_ns = clock.codec_delay_ns;
        result.opus_timestamp_scale_ns = clock.timestamp_scale_ns;
        result.opus_first_block_timestamp = clock.first_block_timestamp;
        result.opus_packet_count = clock.packet_count;
        result.opus_decoded_sample_count = clock.decoded_sample_count;
    }
    input.read_exact_at(&mut magic, 0)?;
    if magic == [0x1a, 0x45, 0xdf, 0xa3] {
        let video = deadpan_source::inspect_matroska_video_region(
            &input,
            0,
            request.video_byte_length,
            DecodeLimits::default(),
            control()?,
        )?;
        result.nominal_frame_duration_ns = video.nominal_frame_duration_ns.unwrap_or(0);
        result.chroma_assumed_left = u32::from(video.chroma_assumed_left);
        if let Some(vp9) = video.vp9 {
            result.vp9_profile_plus_one = u32::from(vp9.profile) + 1;
            result.vp9_bit_depth = u32::from(vp9.bit_depth);
            result.vp9_chroma = u32::from(vp9.chroma_subsampling);
            result.vp9_primaries = u32::from(vp9.color.primaries);
            result.vp9_transfer = u32::from(vp9.color.transfer);
            result.vp9_matrix = u32::from(vp9.color.matrix);
            result.vp9_full_range = u32::from(vp9.color.full_range);
        }
    }
    result.timeout_ms = u64::try_from(
        timeout
            .checked_sub(started.elapsed())
            .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "remux admission deadline"))?
            .as_millis(),
    )?;
    if result.timeout_ms == 0 {
        return Err(io::Error::new(io::ErrorKind::TimedOut, "remux admission deadline").into());
    }
    Ok(result)
}
