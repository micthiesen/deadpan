//! Closed RFC 7845 family-zero header admission before native Opus allocation.

use super::{Result, error, limit, require};
use crate::{audio::MatroskaOpusClock, input::InputLimits};

pub(super) fn header(
    private: &[u8],
    channels: u64,
    rate: f64,
    delay: Option<u64>,
    preroll: u64,
    timestamp_scale_ns: u64,
    limits: InputLimits,
) -> Result<MatroskaOpusClock> {
    require(
        private.len() == 19 && &private[..8] == b"OpusHead" && private[8] == 1,
        "Opus requires an exact version-one family-zero OpusHead",
    )?;
    require(
        private[18] == 0 && (1..=2).contains(&private[9]),
        "only Opus mapping-family-zero mono or stereo is qualified",
    )?;
    require(
        channels == u64::from(private[9]),
        "Opus channel declarations disagree",
    )?;
    let input_rate = u32::from_le_bytes(private[12..16].try_into().expect("fixed header"));
    require(
        rate == 48_000.0 && input_rate == 48_000,
        "Opus currently requires explicit 48000 Hz container and input-rate declarations",
    )?;
    limit(
        channels <= u64::from(limits.max_channels) && limits.max_sample_rate >= 48_000,
        "Opus rate or channels exceed configured limits",
    )?;
    let pre_skip = u32::from(u16::from_le_bytes([private[10], private[11]]));
    let codec_delay_ns = delay.ok_or_else(|| error("invalid_input", "Opus requires CodecDelay"))?;
    // Nanoseconds cannot express every 48 kHz sample. Accept only either adjacent
    // nanosecond, not a rounded millisecond or an independent delay declaration.
    require(
        (i128::from(codec_delay_ns) * 48_000 - i128::from(pre_skip) * 1_000_000_000).abs() < 48_000,
        "Opus CodecDelay disagrees with exact pre-skip",
    )?;
    require(
        preroll == 80_000_000,
        "Opus requires the qualified 80 ms seek preroll",
    )?;
    require(
        (1..=1_000_000).contains(&timestamp_scale_ns),
        "Opus timestamp ticks must be at most one millisecond",
    )?;
    Ok(MatroskaOpusClock {
        pre_skip,
        codec_delay_ns,
        timestamp_scale_ns,
        first_block_timestamp: 0,
        packet_count: 0,
        decoded_sample_count: 0,
    })
}

/// Share the exact packet framing guard with MP4 without reading packet payloads.
pub(super) fn packet(reader: &mut super::Reader<'_>, span: super::Span) -> Result<u64> {
    crate::opus_packet::samples(span.start, span.end, |at| {
        reader.charge(1)?;
        reader.byte(at)
    })
}
