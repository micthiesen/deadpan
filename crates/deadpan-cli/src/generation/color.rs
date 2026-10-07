//! Shared SDR code conversion for model inputs and advisory join measurements.

use std::sync::OnceLock;

use deadpan_render::{Primaries, SourceColor, Transfer, source_to_working, working_to_display};

/// Per-channel full-range RGB8 to sRGB, before spatial fitting. The accepted
/// primaries are already BT.709, so the renderer's source/working/display
/// matrices cancel and a one-dimensional lookup covers every channel value.
/// No HDR tone map or wide-gamut approximation is implied by this path.
pub(super) fn srgb_codes(color: SourceColor) -> Result<&'static [u8; 256], &'static str> {
    if color.primaries != Primaries::Rec709 {
        return Err("only BT.709 primaries have a model-input conversion");
    }
    static SRGB: OnceLock<[u8; 256]> = OnceLock::new();
    static REC709: OnceLock<[u8; 256]> = OnceLock::new();
    match color.transfer {
        Transfer::Srgb => Ok(SRGB.get_or_init(|| {
            std::array::from_fn(|code| u8::try_from(code).expect("256-entry lookup"))
        })),
        Transfer::Rec709 => Ok(REC709.get_or_init(|| {
            std::array::from_fn(|code| {
                let value = f64::from(u8::try_from(code).expect("256-entry lookup")) / 255.0;
                let display = working_to_display(source_to_working([value; 3], color));
                (display[0] * 255.0).round() as u8
            })
        })),
        Transfer::Linear | Transfer::Pq | Transfer::Hlg => {
            Err("this transfer has no qualified SDR model-input conversion")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rec709_codes_convert_at_the_curve_break_and_across_the_full_byte_range() {
        let lookup = srgb_codes(SourceColor {
            transfer: Transfer::Rec709,
            primaries: Primaries::Rec709,
        })
        .unwrap();
        // Fixed reference values include the BT.709 break between 20 and 21.
        for (input, output) in [
            (0, 0),
            (1, 3),
            (4, 11),
            (16, 31),
            (20, 36),
            (21, 37),
            (32, 48),
            (64, 79),
            (128, 140),
            (192, 198),
            (254, 254),
            (255, 255),
        ] {
            assert_eq!(lookup[input], output);
        }
        assert!(lookup.windows(2).all(|pair| pair[0] <= pair[1]));
        // The analytic reference has no working-space matrix round trip.
        for code in 0..=255 {
            let value = f64::from(code) / 255.0;
            let linear = if value < 0.081 {
                value / 4.5
            } else {
                ((value + 0.099) / 1.099).powf(1.0 / 0.45)
            };
            let srgb = if linear <= 0.0031308 {
                linear * 12.92
            } else {
                1.055 * linear.powf(1.0 / 2.4) - 0.055
            };
            assert_eq!(lookup[code as usize], (srgb * 255.0).round() as u8);
        }
    }

    #[test]
    fn srgb_is_unchanged_and_other_interpretations_refuse() {
        let mut color = SourceColor {
            transfer: Transfer::Srgb,
            primaries: Primaries::Rec709,
        };
        assert!(
            srgb_codes(color)
                .unwrap()
                .iter()
                .enumerate()
                .all(|(i, code)| i == usize::from(*code))
        );
        for transfer in [Transfer::Linear, Transfer::Pq, Transfer::Hlg] {
            color.transfer = transfer;
            assert!(srgb_codes(color).is_err());
        }
        color.transfer = Transfer::Srgb;
        for primaries in [Primaries::Rec2020, Primaries::DisplayP3D65] {
            color.primaries = primaries;
            assert!(srgb_codes(color).is_err());
        }
    }
}
