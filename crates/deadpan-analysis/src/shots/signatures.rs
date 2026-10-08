//! Bounded binary storage for retained context signatures.

use super::{
    GRID_COLUMNS, GRID_ROWS, HISTOGRAM_BINS, MAX_CONTEXT_SHOT_SIGNATURES, PictureSignature,
    ShotError,
};

pub const CONTEXT_SIGNATURE_ENCODING: &str = "deadpan-context-signatures-1";
pub const CONTEXT_SIGNATURE_HEADER_BYTES: usize = 12;
pub const PICTURE_SIGNATURE_BYTES: usize = GRID_COLUMNS * GRID_ROWS * 3 + HISTOGRAM_BINS * 4;
pub const MAX_CONTEXT_SIGNATURE_BYTES: usize =
    CONTEXT_SIGNATURE_HEADER_BYTES + MAX_CONTEXT_SHOT_SIGNATURES * PICTURE_SIGNATURE_BYTES;

const MAGIC: &[u8; 8] = b"DPSIG001";
const CELL_BYTES: usize = GRID_COLUMNS * GRID_ROWS * 3;
const HISTOGRAM_SUM_MIN: u32 = 65_520;
const HISTOGRAM_SUM_MAX: u32 = 65_552;
const HISTOGRAM_BIN_MAX: u32 = 65_536;

/// Encode consecutive retained signatures in the bounded context format.
pub fn encode_context_signatures(signatures: &[PictureSignature]) -> Result<Vec<u8>, ShotError> {
    if signatures.len() > MAX_CONTEXT_SHOT_SIGNATURES {
        return Err(ShotError::Limit);
    }
    for signature in signatures {
        validate_signature(signature)?;
    }

    let capacity = CONTEXT_SIGNATURE_HEADER_BYTES + signatures.len() * PICTURE_SIGNATURE_BYTES;
    let mut bytes = Vec::with_capacity(capacity);
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&(signatures.len() as u32).to_le_bytes());
    for signature in signatures {
        for cell in &signature.cells {
            bytes.extend_from_slice(cell);
        }
        for count in signature.histogram {
            bytes.extend_from_slice(&count.to_le_bytes());
        }
    }
    Ok(bytes)
}

/// Decode retained signatures after validating the complete bounded wire shape.
pub fn decode_context_signatures(bytes: &[u8]) -> Result<Vec<PictureSignature>, ShotError> {
    if bytes.len() < CONTEXT_SIGNATURE_HEADER_BYTES {
        return Err(ShotError::Invalid("context signature header is truncated"));
    }
    if bytes.len() > MAX_CONTEXT_SIGNATURE_BYTES {
        return Err(ShotError::Limit);
    }
    if &bytes[..MAGIC.len()] != MAGIC {
        return Err(ShotError::Invalid("context signature magic is invalid"));
    }
    let count = u32::from_le_bytes(bytes[8..12].try_into().expect("fixed header")) as usize;
    if count > MAX_CONTEXT_SHOT_SIGNATURES {
        return Err(ShotError::Limit);
    }
    let expected = CONTEXT_SIGNATURE_HEADER_BYTES + count * PICTURE_SIGNATURE_BYTES;
    if bytes.len() != expected {
        return Err(ShotError::Invalid(
            "context signature bytes do not match the declared picture count",
        ));
    }

    let mut signatures = Vec::with_capacity(count);
    for encoded in bytes[CONTEXT_SIGNATURE_HEADER_BYTES..].chunks_exact(PICTURE_SIGNATURE_BYTES) {
        let mut cells = Vec::with_capacity(GRID_COLUMNS * GRID_ROWS);
        for cell in encoded[..CELL_BYTES].chunks_exact(3) {
            cells.push([cell[0], cell[1], cell[2]]);
        }
        let histogram = std::array::from_fn(|index| {
            let offset = CELL_BYTES + index * 4;
            u32::from_le_bytes(
                encoded[offset..offset + 4]
                    .try_into()
                    .expect("fixed histogram"),
            )
        });
        let signature = PictureSignature { cells, histogram };
        validate_signature(&signature)?;
        signatures.push(signature);
    }
    Ok(signatures)
}

fn validate_signature(signature: &PictureSignature) -> Result<(), ShotError> {
    if signature.cells.len() != GRID_COLUMNS * GRID_ROWS {
        return Err(ShotError::Invalid(
            "context signature cell count is invalid",
        ));
    }
    if signature
        .histogram
        .iter()
        .any(|count| *count > HISTOGRAM_BIN_MAX)
    {
        return Err(ShotError::Invalid(
            "context signature histogram bin exceeds its limit",
        ));
    }
    let sum: u32 = signature.histogram.iter().sum();
    if !(HISTOGRAM_SUM_MIN..=HISTOGRAM_SUM_MAX).contains(&sum) {
        return Err(ShotError::Invalid(
            "context signature histogram total is invalid",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shots::{PictureSignature, qualify_context};

    fn picture(color: impl Fn(u32, u32) -> [u8; 3]) -> PictureSignature {
        let rgba: Vec<u8> = (0..GRID_ROWS as u32)
            .flat_map(|y| (0..GRID_COLUMNS as u32).map(move |x| (x, y)))
            .flat_map(|(x, y)| {
                let [r, g, b] = color(x, y);
                [r, g, b, 255]
            })
            .collect();
        PictureSignature::from_rgba(
            &rgba,
            GRID_COLUMNS as u32,
            GRID_ROWS as u32,
            GRID_COLUMNS * 4,
        )
        .unwrap()
    }

    fn warm(x: u32, _: u32) -> [u8; 3] {
        [200, 120 + ((x * 3 % 192) as u8) / 4, 40]
    }

    fn cool(_: u32, y: u32) -> [u8; 3] {
        [10, 20, 90 + (y * 4) as u8]
    }

    #[test]
    fn rgba_round_trip_preserves_signatures_and_derived_measurements() {
        let signatures = vec![
            picture(warm),
            picture(cool),
            picture(|x, y| [x as u8 * 7, y as u8 * 11, 80]),
        ];
        let decoded =
            decode_context_signatures(&encode_context_signatures(&signatures).unwrap()).unwrap();
        assert_eq!(decoded, signatures);
        for (before, after) in signatures.iter().zip(&decoded) {
            assert_eq!(before.luma(), after.luma());
            assert_eq!(
                before.span(&signatures[0], &signatures[1]),
                after.span(&decoded[0], &decoded[1])
            );
        }
        assert_eq!(
            signatures[0].change(&signatures[1], None),
            decoded[0].change(&decoded[1], None)
        );
    }

    #[test]
    fn recomputed_context_qualification_survives_round_trip() {
        let signatures: Vec<_> = (0..24)
            .map(|index| {
                if index < 12 {
                    picture(warm)
                } else {
                    picture(cool)
                }
            })
            .collect();
        let restored =
            decode_context_signatures(&encode_context_signatures(&signatures).unwrap()).unwrap();
        let before = qualify_context(&signatures, 0, signatures.len(), 8..=16).unwrap();
        let after = qualify_context(&restored, 0, restored.len(), 8..=16).unwrap();
        assert_eq!(after, before);
        assert!(before.transition.is_some());
    }

    #[test]
    fn empty_context_has_a_valid_header() {
        let bytes = encode_context_signatures(&[]).unwrap();
        assert_eq!(bytes.len(), CONTEXT_SIGNATURE_HEADER_BYTES);
        assert_eq!(
            decode_context_signatures(&bytes).unwrap(),
            Vec::<PictureSignature>::new()
        );
    }

    #[test]
    fn malformed_wire_shapes_are_rejected() {
        let valid = encode_context_signatures(&[picture(warm)]).unwrap();
        assert!(decode_context_signatures(&valid[..11]).is_err());
        assert!(decode_context_signatures(&valid[..valid.len() - 1]).is_err());
        let mut trailing = valid.clone();
        trailing.push(0);
        assert!(decode_context_signatures(&trailing).is_err());

        let mut wrong_magic = valid.clone();
        wrong_magic[0] ^= 1;
        assert!(decode_context_signatures(&wrong_magic).is_err());
        let mut wire = MAGIC.to_vec();
        wire.extend_from_slice(&((MAX_CONTEXT_SHOT_SIGNATURES as u32) + 1).to_le_bytes());
        assert_eq!(decode_context_signatures(&wire), Err(ShotError::Limit));
    }

    #[test]
    fn invalid_histograms_are_rejected_on_encode_and_decode() {
        let valid = picture(warm);
        let mut bad_bin = valid.clone();
        bad_bin.histogram[0] = HISTOGRAM_BIN_MAX + 1;
        assert!(matches!(
            encode_context_signatures(&[bad_bin]),
            Err(ShotError::Invalid(_))
        ));

        let mut bad_sum = valid.clone();
        bad_sum.histogram = [0; HISTOGRAM_BINS];
        assert!(matches!(
            encode_context_signatures(&[bad_sum]),
            Err(ShotError::Invalid(_))
        ));

        let mut encoded_bin = encode_context_signatures(std::slice::from_ref(&valid)).unwrap();
        encoded_bin[CONTEXT_SIGNATURE_HEADER_BYTES + CELL_BYTES..][..4]
            .copy_from_slice(&(HISTOGRAM_BIN_MAX + 1).to_le_bytes());
        assert!(matches!(
            decode_context_signatures(&encoded_bin),
            Err(ShotError::Invalid(_))
        ));
        let mut encoded_sum = encode_context_signatures(&[valid]).unwrap();
        encoded_sum[CONTEXT_SIGNATURE_HEADER_BYTES + CELL_BYTES..].fill(0);
        assert!(matches!(
            decode_context_signatures(&encoded_sum),
            Err(ShotError::Invalid(_))
        ));
    }

    #[test]
    fn encoder_rejects_invalid_shape_and_oversized_context() {
        let valid = picture(warm);
        let malformed = PictureSignature {
            cells: Vec::new(),
            histogram: valid.histogram,
        };
        assert!(matches!(
            encode_context_signatures(&[malformed]),
            Err(ShotError::Invalid(_))
        ));
        assert_eq!(
            encode_context_signatures(&vec![valid; MAX_CONTEXT_SHOT_SIGNATURES + 1]),
            Err(ShotError::Limit)
        );
    }
}
