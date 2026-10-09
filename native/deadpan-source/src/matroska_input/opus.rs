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

/// RFC 6716 section 3 packet framing. Read only TOC/length/padding headers;
/// validate every frame boundary before FFmpeg's parser can discard a packet.
pub(super) fn packet(reader: &mut super::Reader<'_>, span: super::Span) -> Result<u64> {
    let mut at = span.start;
    let mut end = span.end;
    fn byte(reader: &mut super::Reader<'_>, at: &mut u64, end: u64) -> Result<u8> {
        require(*at < end, "truncated Opus packet framing")?;
        let value = reader.byte(*at)?;
        reader.charge(1)?;
        *at += 1;
        Ok(value)
    }
    fn size(reader: &mut super::Reader<'_>, at: &mut u64, end: u64) -> Result<u64> {
        let first = u64::from(byte(reader, at, end)?);
        Ok(if first < 252 {
            first
        } else {
            first + 4 * u64::from(byte(reader, at, end)?)
        })
    }
    let toc = byte(reader, &mut at, end)?;
    let config = toc >> 3;
    let duration = if config < 12 {
        [480, 960, 1920, 2880][usize::from(config & 3)]
    } else if config < 16 {
        [480, 960][usize::from(config & 1)]
    } else {
        [120, 240, 480, 960][usize::from(config & 3)]
    };
    let (count, vbr) = match toc & 3 {
        0 => (1, false),
        1 => (2, false),
        2 => (2, true),
        _ => {
            let flags = byte(reader, &mut at, end)?;
            let count = u64::from(flags & 63);
            require(count > 0 && count <= 48, "invalid Opus frame count")?;
            if flags & 64 != 0 {
                loop {
                    let value = byte(reader, &mut at, end)?;
                    let padding = if value == 255 { 254 } else { u64::from(value) };
                    require(padding <= end - at, "Opus padding exceeds packet")?;
                    end -= padding;
                    if value != 255 {
                        break;
                    }
                }
            }
            (count, flags & 128 != 0)
        }
    };
    let samples = count * duration;
    require(samples <= 5760, "Opus packet exceeds 120 ms")?;
    if vbr {
        let mut used = 0;
        for _ in 1..count {
            let length = size(reader, &mut at, end)?;
            require(length <= 1275, "Opus frame exceeds 1275 bytes")?;
            used += length;
        }
        require(
            used <= end - at && end - at - used <= 1275,
            "Opus VBR lengths exceed packet or frame bound",
        )?;
    } else {
        require(
            (end - at).is_multiple_of(count) && (end - at) / count <= 1275,
            "invalid Opus CBR frame lengths",
        )?;
    }
    Ok(samples)
}
