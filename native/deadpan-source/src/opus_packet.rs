//! Shared bounded RFC 6716 packet framing for MP4 and Matroska admission.

use crate::SourceDecodeError;
type Result<T> = std::result::Result<T, SourceDecodeError>;
fn require(value: bool, message: &'static str) -> Result<()> {
    if value {
        Ok(())
    } else {
        Err(SourceDecodeError::Native {
            code: "invalid_input".into(),
            message: message.into(),
        })
    }
}
/// Inspect framing bytes only. The caller accounts for I/O, work and deadlines.
pub(crate) fn samples(
    start: u64,
    end: u64,
    mut read: impl FnMut(u64) -> Result<u8>,
) -> Result<u64> {
    let mut at = start;
    let mut end = end;
    fn byte(read: &mut impl FnMut(u64) -> Result<u8>, at: &mut u64, end: u64) -> Result<u8> {
        require(*at < end, "truncated Opus packet framing")?;
        let value = read(*at)?;
        *at += 1;
        Ok(value)
    }
    fn size(read: &mut impl FnMut(u64) -> Result<u8>, at: &mut u64, end: u64) -> Result<u64> {
        let first = u64::from(byte(read, at, end)?);
        Ok(if first < 252 {
            first
        } else {
            first + 4 * u64::from(byte(read, at, end)?)
        })
    }
    let toc = byte(&mut read, &mut at, end)?;
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
            let flags = byte(&mut read, &mut at, end)?;
            let count = u64::from(flags & 63);
            require(count > 0 && count <= 48, "invalid Opus frame count")?;
            if flags & 64 != 0 {
                loop {
                    let value = byte(&mut read, &mut at, end)?;
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
            let length = size(&mut read, &mut at, end)?;
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
