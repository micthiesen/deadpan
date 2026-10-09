//! Exact MP4 clean-aperture admission and packed-picture extraction.
//!
//! `clap` describes pixel centers relative to the center of the sample-entry
//! raster. Preserve fractional rectangles for the shared renderer; never use
//! the demuxer's floating-point/truncated crop as the authority.

use crate::SourceDecodeError;
use deadpan_core::ExactRatio;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CleanAperture {
    pub coded: [u32; 2],
    /// Left, top, width, height before sample aspect and rotation.
    pub rect: Option<[u32; 4]>,
    pub bounds: [ExactRatio; 4],
}

fn invalid(message: &'static str) -> SourceDecodeError {
    SourceDecodeError::Native {
        code: "invalid_input".into(),
        message: message.into(),
    }
}

impl CleanAperture {
    pub fn from_words(coded: [u32; 2], words: [i32; 8]) -> Result<Self, SourceDecodeError> {
        let [wn, wd, hn, hd, xn, xd, yn, yd] = words.map(i128::from);
        if wn <= 0 || hn <= 0 || wd <= 0 || hd <= 0 || xd <= 0 || yd <= 0 {
            return Err(invalid(
                "clean aperture needs positive extents and denominators",
            ));
        }
        let axis = |extent: u32, n: i128, d: i128, offset: i128, od: i128| {
            // left = (coded_width - aperture_width) / 2 + horizontal_offset.
            // i32 declarations and bounded dimensions fit these i128 products.
            let left = (i128::from(extent) * d - n) * od + 2 * offset * d;
            let denominator = 2 * d * od;
            if left < 0 || left + 2 * n * od > i128::from(extent) * denominator {
                return Err(invalid("clean aperture lies outside the declared raster"));
            }
            Ok((
                ExactRatio::new(left, denominator)
                    .map_err(|_| invalid("clean aperture offset overflow"))?,
                ExactRatio::new(n, d).map_err(|_| invalid("clean aperture extent overflow"))?,
            ))
        };
        let (left, width) = axis(coded[0], wn, wd, xn, xd)?;
        let (top, height) = axis(coded[1], hn, hd, yn, yd)?;
        let bounds = [left, top, width, height];
        let rect = if bounds.iter().all(|v| v.denominator() == 1) {
            Some(bounds.map(|v| u32::try_from(v.numerator()).expect("bounded clean rectangle")))
        } else {
            None
        };
        Ok(Self {
            coded,
            rect,
            bounds,
        })
    }

    /// Compact rows in the already allocated full-raster buffer. Conversion
    /// happens before this crop, preserving chroma interpolation at odd edges.
    pub fn packed<T: Copy>(&self, pixels: &mut Vec<T>, channels: usize) {
        let Some(rect) = self.rect else { return };
        let [left, top, width, height] = rect.map(pixel_count);
        let stride = pixel_count(self.coded[0]) * channels;
        let output_stride = width * channels;
        for row in 0..height {
            let start = (top + row) * stride + left * channels;
            pixels.copy_within(start..start + output_stride, row * output_stride);
        }
        pixels.truncate(output_stride * height);
    }

    pub fn validate_420(&self) -> Result<(), SourceDecodeError> {
        let Some(rect) = self.rect else {
            return Err(SourceDecodeError::Native {
                code: "unsupported_transform".into(),
                message: "raw 4:2:0 reads cannot represent a fractional clean aperture; use RGBA and its retained aperture".into(),
            });
        };
        if self.coded.iter().chain(&rect).any(|v| !v.is_multiple_of(2)) {
            return Err(SourceDecodeError::Native {
                code: "unsupported_transform".into(),
                message: "raw 4:2:0 clean aperture must align to the chroma grid; use RGBA".into(),
            });
        }
        Ok(())
    }

    pub fn planar_420<T: Copy>(&self, pixels: &mut Vec<T>) {
        let [left, top, width, height] = self
            .rect
            .expect("validated planar aperture")
            .map(pixel_count);
        let mut source_base = 0;
        let mut output_base = 0;
        for scale in [1, 2, 2] {
            let stride = pixel_count(self.coded[0]) / scale;
            let output_stride = width / scale;
            for row in 0..height / scale {
                let start = source_base + (top / scale + row) * stride + left / scale;
                pixels.copy_within(
                    start..start + output_stride,
                    output_base + row * output_stride,
                );
            }
            source_base += stride * (pixel_count(self.coded[1]) / scale);
            output_base += output_stride * (height / scale);
        }
        pixels.truncate(output_base);
    }
}

fn pixel_count(value: u32) -> usize {
    usize::try_from(value).expect("admitted raster dimensions fit the host address space")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rational_declarations_resolve_exact_pixel_edges_without_rounding() {
        assert_eq!(
            CleanAperture::from_words([320, 180], [600, 2, 320, 2, 6, 2, -2, 2])
                .unwrap()
                .rect,
            Some([13, 9, 300, 160])
        );
        // An odd aperture in an even raster needs a half-pixel center offset.
        assert_eq!(
            CleanAperture::from_words([320, 180], [299, 1, 159, 1, -1, 2, 1, 2])
                .unwrap()
                .rect,
            Some([10, 11, 299, 159])
        );
        for words in [
            [300, 0, 160, 1, 0, 1, 0, 1],
            [0, 1, 160, 1, 0, 1, 0, 1],
            [321, 1, 180, 1, 0, 1, 0, 1],
            [300, 1, 160, 1, -11, 1, 0, 1],
            [300, 1, 160, 1, 11, 1, 0, 1],
            [i32::MAX, i32::MAX, 160, 1, i32::MIN, 1, 0, 1],
        ] {
            assert!(matches!(CleanAperture::from_words([320, 180], words),
                Err(SourceDecodeError::Native { code, .. }) if code == "invalid_input"));
        }
        for words in [[299, 1, 160, 1, 0, 1, 0, 1], [599, 2, 160, 1, 0, 1, 0, 1]] {
            let aperture = CleanAperture::from_words([320, 180], words).unwrap();
            assert!(aperture.rect.is_none());
            assert!(aperture.bounds[0].denominator() > 1);
            assert!(aperture.validate_420().is_err());
        }
    }
}
