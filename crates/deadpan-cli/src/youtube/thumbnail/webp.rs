//! image's WebP adapter does not forward its allocation budget to metadata
//! reads, and coded frame dimensions are independent of the extended canvas.
//! Refuse truncated/oversized chunks and mismatched dimensions before entering
//! that decoder. Confirmation needs one still image, never animation frames.

use super::{MAX_DIMENSION, MAX_PIXELS, Unavailable};

pub(super) fn validate(bytes: &[u8]) -> Result<(), Unavailable> {
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WEBP" {
        return Err(Unavailable::InvalidImage);
    }
    let size = u32::from_le_bytes(bytes[4..8].try_into().expect("four-byte size"));
    if u64::from(size) + 8 != bytes.len() as u64 {
        return Err(Unavailable::InvalidImage);
    }
    let mut position = 12;
    let mut canvas = None;
    let mut coded = None;
    while position < bytes.len() {
        let header = bytes
            .get(position..position + 8)
            .ok_or(Unavailable::InvalidImage)?;
        let length = u32::from_le_bytes(header[4..8].try_into().expect("four-byte size"));
        let length = usize::try_from(length).map_err(|_| Unavailable::TooLarge)?;
        let start = position + 8;
        let end = start.checked_add(length).ok_or(Unavailable::TooLarge)?;
        let chunk = bytes.get(start..end).ok_or(Unavailable::InvalidImage)?;
        position = end.checked_add(length & 1).ok_or(Unavailable::TooLarge)?;
        if position > bytes.len() {
            return Err(Unavailable::InvalidImage);
        }
        let dimensions = match &header[..4] {
            b"VP8X" => {
                if start != 20 || chunk.len() != 10 {
                    return Err(Unavailable::InvalidImage);
                }
                if chunk[0] & 0x02 != 0 {
                    return Err(Unavailable::UnsupportedImage);
                }
                let width = u32::from_le_bytes([chunk[4], chunk[5], chunk[6], 0]) + 1;
                let height = u32::from_le_bytes([chunk[7], chunk[8], chunk[9], 0]) + 1;
                canvas = Some(checked_dimensions(width, height)?);
                None
            }
            b"VP8 " => {
                if chunk.len() < 10 || chunk[0] & 1 != 0 || chunk[3..6] != [0x9d, 0x01, 0x2a] {
                    return Err(Unavailable::InvalidImage);
                }
                let width = u32::from(u16::from_le_bytes([chunk[6], chunk[7]]) & 0x3fff);
                let height = u32::from(u16::from_le_bytes([chunk[8], chunk[9]]) & 0x3fff);
                Some(checked_dimensions(width, height)?)
            }
            b"VP8L" => {
                if chunk.len() < 5 || chunk[0] != 0x2f {
                    return Err(Unavailable::InvalidImage);
                }
                let bits =
                    u32::from_le_bytes(chunk[1..5].try_into().expect("four-byte dimensions"));
                Some(checked_dimensions(
                    (bits & 0x3fff) + 1,
                    ((bits >> 14) & 0x3fff) + 1,
                )?)
            }
            b"ANIM" | b"ANMF" => return Err(Unavailable::UnsupportedImage),
            _ => None,
        };
        if let Some(dimensions) = dimensions
            && coded.replace(dimensions).is_some()
        {
            return Err(Unavailable::InvalidImage);
        }
    }
    let coded = coded.ok_or(Unavailable::InvalidImage)?;
    if canvas.is_some_and(|canvas| canvas != coded) {
        return Err(Unavailable::InvalidImage);
    }
    Ok(())
}

fn checked_dimensions(width: u32, height: u32) -> Result<[u32; 2], Unavailable> {
    if width == 0
        || height == 0
        || width > MAX_DIMENSION
        || height > MAX_DIMENSION
        || u64::from(width) * u64::from(height) > MAX_PIXELS
    {
        Err(Unavailable::TooLarge)
    } else {
        Ok([width, height])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(chunks: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
        let mut bytes = b"RIFF\0\0\0\0WEBP".to_vec();
        for (name, body) in chunks {
            bytes.extend_from_slice(*name);
            bytes.extend_from_slice(&u32::try_from(body.len()).unwrap().to_le_bytes());
            bytes.extend_from_slice(body);
            if body.len() % 2 != 0 {
                bytes.push(0);
            }
        }
        let size = u32::try_from(bytes.len() - 8).unwrap();
        bytes[4..8].copy_from_slice(&size.to_le_bytes());
        bytes
    }

    fn lossless(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = vec![0x2f];
        bytes.extend_from_slice(&((width - 1) | ((height - 1) << 14)).to_le_bytes());
        bytes
    }

    #[test]
    fn checks_coded_dimensions_independently_of_the_extended_canvas() {
        let canvas = vec![0, 0, 0, 0, 119, 0, 0, 89, 0, 0];
        assert_eq!(
            validate(&image(&[
                (b"VP8X", canvas.clone()),
                (b"VP8L", lossless(120, 90))
            ])),
            Ok(())
        );
        assert_eq!(
            validate(&image(&[
                (b"VP8X", canvas.clone()),
                (b"VP8L", lossless(16384, 16384))
            ])),
            Err(Unavailable::TooLarge)
        );
        assert_eq!(
            validate(&image(&[(b"VP8X", canvas), (b"VP8L", lossless(240, 90))])),
            Err(Unavailable::InvalidImage)
        );
        let mut lossy = vec![0, 0, 0, 0x9d, 0x01, 0x2a];
        lossy.extend_from_slice(&16383u16.to_le_bytes());
        lossy.extend_from_slice(&16383u16.to_le_bytes());
        assert_eq!(
            validate(&image(&[(b"VP8 ", lossy)])),
            Err(Unavailable::TooLarge)
        );
    }

    #[test]
    fn forged_metadata_sizes_and_animations_are_rejected_before_decoder_allocation() {
        let mut bytes = image(&[(b"VP8L", lossless(120, 90)), (b"EXIF", vec![0; 4])]);
        let end = bytes.len();
        bytes[end - 8..end - 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(validate(&bytes), Err(Unavailable::InvalidImage));
        assert_eq!(
            super::super::Thumbnail::decode(&bytes),
            Err(Unavailable::InvalidImage)
        );
        assert_eq!(
            validate(&image(&[
                (b"VP8X", vec![2, 0, 0, 0, 119, 0, 0, 89, 0, 0]),
                (b"ANIM", vec![0; 6])
            ])),
            Err(Unavailable::UnsupportedImage)
        );
        let valid = image(&[(b"VP8L", lossless(120, 90))]);
        for end in 0..valid.len() {
            assert!(validate(&valid[..end]).is_err());
        }
        assert_eq!(
            validate(&image(&[
                (b"VP8L", lossless(120, 90)),
                (b"VP8L", lossless(120, 90))
            ])),
            Err(Unavailable::InvalidImage)
        );
    }
}
