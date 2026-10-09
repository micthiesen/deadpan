//! Raw Layer III framing before FFmpeg allocation or parser resynchronization.
//! The walk admits every frame, with no skipped junk or inferred trims.

use super::{HEADER_BYTES, Reader, Result, invalid, limit, require};
use crate::audio::Mp3Framing;
use std::os::unix::fs::FileExt;

// Sparse reads avoid charging every compressed payload page as header work.
fn read<const N: usize>(r: &mut Reader<'_>, at: u64) -> Result<[u8; N]> {
    r.check()?;
    require(
        N <= 64 && at <= r.length && N as u64 <= r.length - at,
        "truncated MP3 field",
    )?;
    if r.read_bytes + N as u64 > HEADER_BYTES.min(r.limits.max_io_bytes_per_call) {
        return Err(limit("MP3 header read budget exceeded"));
    }
    let mut bytes = [0; N];
    r.file.read_exact_at(&mut bytes, at)?;
    r.read_bytes += N as u64;
    r.check()?;
    Ok(bytes)
}

fn synchsafe(bytes: [u8; 4]) -> Result<u32> {
    require(
        bytes.iter().all(|v| v & 0x80 == 0),
        "invalid ID3 synchsafe size",
    )?;
    Ok(bytes.into_iter().fold(0, |n, b| (n << 7) | u32::from(b)))
}

fn zeros(r: &mut Reader<'_>, mut at: u64, end: u64) -> Result<()> {
    while end - at >= 64 {
        require(read::<64>(r, at)? == [0; 64], "nonzero ID3 padding")?;
        at += 64;
    }
    while at < end {
        require(read::<1>(r, at)? == [0], "nonzero ID3 padding")?;
        at += 1;
    }
    Ok(())
}

fn id3(r: &mut Reader<'_>) -> Result<u64> {
    if read::<3>(r, 0)? != *b"ID3" {
        return Ok(0);
    }
    let header = read::<10>(r, 0)?;
    require(
        matches!(header[3], 3 | 4) && header[4] == 0 && header[5] == 0,
        "unqualified ID3 version, unsynchronization, extension or footer",
    )?;
    let length = u64::from(synchsafe(header[6..10].try_into().unwrap())?);
    require(
        length <= 1024 * 1024 && length <= r.length - 10,
        "ID3 tag exceeds its bound",
    )?;
    let end = 10 + length;
    let mut at = 10;
    let mut fields = 0;
    let mut picture = false;
    while at < end {
        if read::<1>(r, at)? == [0] {
            zeros(r, at, end)?;
            break;
        }
        fields += 1;
        require(
            fields <= 1024 && end - at >= 10,
            "ID3 frame table exceeds its bound",
        )?;
        let frame = read::<10>(r, at)?;
        require(
            frame[..4]
                .iter()
                .all(|v| v.is_ascii_uppercase() || v.is_ascii_digit())
                && frame[8..10] == [0, 0],
            "unqualified ID3 frame identifier or flags",
        )?;
        let size = if header[3] == 4 {
            synchsafe(frame[4..8].try_into().unwrap())?
        } else {
            u32::from_be_bytes(frame[4..8].try_into().unwrap())
        };
        at += 10;
        require(
            size > 0 && u64::from(size) <= end - at,
            "truncated ID3 frame",
        )?;
        let text = frame[0] == b'T' || frame[..4] == *b"COMM";
        if text {
            require(
                size <= 64 * 1024 && read::<1>(r, at)?[0] <= 3,
                "unqualified ID3 text field",
            )?;
        } else if frame[..4] == *b"APIC" {
            require(
                !picture && u64::from(size) <= r.limits.max_packet_bytes.min(1024 * 1024),
                "ID3 attached picture exceeds its count or byte bound",
            )?;
            picture = true;
            // Only MIME-labelled inline JPEG/PNG, never an external URL. The
            // audio path discards the attached picture without decoding it.
            require(
                size >= 16 && read::<1>(r, at)?[0] <= 3,
                "invalid ID3 picture header",
            )?;
            let mime = read::<11>(r, at + 1)?;
            require(
                mime.starts_with(b"image/png\0") || mime.starts_with(b"image/jpeg\0"),
                "unqualified ID3 picture MIME type",
            )?;
        } else {
            return Err(invalid("unqualified ID3 frame type"));
        }
        at += u64::from(size);
    }
    Ok(end)
}

#[derive(Clone, Copy)]
struct Header {
    rate: u32,
    channels: u32,
    samples: u32,
    bytes: u32,
}

fn header(r: &mut Reader<'_>, at: u64, end: u64) -> Result<Header> {
    require(end - at >= 4, "truncated MP3 header")?;
    let bits = u32::from_be_bytes(read(r, at)?);
    let version = (bits >> 19) & 3;
    let bitrate = (bits >> 12) & 15;
    let frequency = (bits >> 10) & 3;
    require(
        bits >> 21 == 0x7ff
            && version != 1
            && (bits >> 17) & 3 == 1
            && (1..15).contains(&bitrate)
            && frequency < 3
            && bits & 3 == 0,
        "unqualified MPEG Layer III header, free format or emphasis",
    )?;
    let low = version != 3;
    let rate = [44_100, 48_000, 32_000][frequency as usize]
        >> if version == 0 { 2 } else { u32::from(low) };
    let rates = if low {
        [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160]
    } else {
        [
            0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320,
        ]
    };
    let bytes = rates[bitrate as usize] * 144_000 / (rate << u32::from(low)) + ((bits >> 9) & 1);
    let channels = if (bits >> 6) & 3 == 3 { 1 } else { 2 };
    require(
        rate <= r.limits.max_sample_rate
            && channels <= r.limits.max_channels
            && u64::from(bytes) <= r.limits.max_packet_bytes
            && u64::from(bytes) <= end - at,
        "MP3 frame exceeds its rate, channel, packet or input bound",
    )?;
    Ok(Header {
        rate,
        channels,
        samples: if low { 576 } else { 1152 },
        bytes,
    })
}

struct Tag {
    frames: u32,
    bytes: u32,
    leading: u32,
    trailing: u32,
}

fn crc_byte(mut crc: u16, byte: u8) -> u16 {
    crc ^= u16::from(byte);
    for _ in 0..8 {
        crc = (crc >> 1) ^ if crc & 1 != 0 { 0xa001 } else { 0 };
    }
    crc
}

fn xing(r: &mut Reader<'_>, start: u64, h: Header) -> Result<Option<Tag>> {
    let end = start + u64::from(h.bytes);
    let side = match (h.samples, h.channels) {
        (1152, 2) => 32,
        (1152, 1) | (576, 2) => 17,
        _ => 9,
    };
    let at = start + 4 + side;
    if start + 40 <= end {
        require(
            read::<4>(r, start + 36)? != *b"VBRI",
            "VBRI MP3 framing is not yet qualified",
        )?;
    }
    if at + 4 > end {
        return Ok(None);
    }
    if !matches!(&read::<4>(r, at)?, b"Xing" | b"Info") {
        return Ok(None);
    }
    require(at + 16 <= end, "truncated Xing header")?;
    let flags = u32::from_be_bytes(read(r, at + 4)?);
    require(
        flags & !15 == 0 && flags & 3 == 3,
        "Xing must declare frame and byte counts",
    )?;
    let frames = u32::from_be_bytes(read(r, at + 8)?);
    let bytes = u32::from_be_bytes(read(r, at + 12)?);
    let encoder =
        at + 16 + if flags & 4 != 0 { 100 } else { 0 } + if flags & 8 != 0 { 4 } else { 0 };
    require(encoder + 36 <= end, "truncated Xing encoder record")?;
    let data = read::<36>(r, encoder)?;
    let known = matches!(&data[..4], b"LAME" | b"Lavf" | b"Lavc");
    let (leading, trailing) = if known {
        // FFmpeg's qualified synthesis delay is 528 + 1 samples. It reports
        // these exact skips; this is never waveform alignment or guessed trim.
        let leading = (u32::from(data[21]) << 4) | (u32::from(data[22]) >> 4);
        let padding = (u32::from(data[22] & 15) << 8) | u32::from(data[23]);
        require(
            padding >= 529,
            "MP3 declared padding cannot cover decoder delay",
        )?;
        let mut crc = 0_u16;
        for at in start..encoder + 34 {
            crc = crc_byte(crc, read::<1>(r, at)?[0]);
        }
        let declared = u16::from_be_bytes(data[34..36].try_into().unwrap());
        // FFmpeg's mp3enc.c hashes a fixed 190-byte buffer, with the checksum
        // slots and buffer padding still zero. For mono/LSF this differs from
        // LAME's variable tag endpoint. Admit that exact producer variant too.
        let mut ffmpeg_crc = None;
        if matches!(&data[..4], b"Lavf" | b"Lavc") {
            let mut value = 0;
            for at in start..start + 190 {
                let byte = if at >= end || (encoder + 34..encoder + 36).contains(&at) {
                    0
                } else {
                    read::<1>(r, at)?[0]
                };
                value = crc_byte(value, byte);
            }
            ffmpeg_crc = Some(value);
        }
        require(
            crc == declared || ffmpeg_crc == Some(declared),
            "MP3 encoder tag checksum mismatch",
        )?;
        (leading + 529, padding - 529)
    } else {
        // Unknown encoders supply no qualified trim. FFmpeg makes the same
        // distinction; their entire measured physical decode remains audible.
        (0, 0)
    };
    Ok(Some(Tag {
        frames,
        bytes,
        leading,
        trailing,
    }))
}

pub(super) fn admit(r: &mut Reader<'_>) -> Result<Mp3Framing> {
    let start = id3(r)?;
    let mut end = r.length;
    if end >= 128 && read::<3>(r, end - 128)? == *b"TAG" {
        end -= 128;
    }
    require(start < end, "MP3 has no compressed frames")?;
    let first = header(r, start, end)?;
    let tag = xing(r, start, first)?;
    let mut at = start;
    if tag.is_some() {
        at += u64::from(first.bytes);
    }
    let mut frames = 0_u64;
    while at < end {
        let h = header(r, at, end)?;
        require(
            (h.rate, h.channels, h.samples) == (first.rate, first.channels, first.samples),
            "MP3 rate or channel interpretation changes between frames",
        )?;
        frames += 1;
        if frames > r.limits.max_packets
            || frames * u64::from(first.samples) > r.limits.max_decoded_samples
        {
            return Err(limit("MP3 frame or sample inventory exceeds its bound"));
        }
        at += u64::from(h.bytes);
    }
    require(
        frames >= 2,
        "MP3 requires at least two complete audio frames",
    )?;
    let (leading_skip, trailing_skip) = if let Some(tag) = tag {
        require(
            u64::from(tag.frames) == frames && u64::from(tag.bytes) == end - start,
            "Xing counts disagree with the complete MP3 frame inventory",
        )?;
        require(
            u64::from(tag.leading) + u64::from(tag.trailing) < frames * u64::from(first.samples),
            "MP3 trimming leaves no physical audio",
        )?;
        (tag.leading, tag.trailing)
    } else {
        (0, 0)
    };
    Ok(Mp3Framing {
        sample_rate: first.rate,
        channels: first.channels,
        samples_per_frame: first.samples,
        frame_count: frames,
        leading_skip,
        trailing_skip,
    })
}
