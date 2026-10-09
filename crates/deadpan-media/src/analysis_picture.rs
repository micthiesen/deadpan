//! Fractional-aperture rasters for RGB8 analysis and model conditioning.
//!
//! These consumers operate on encoded RGB, as they do for ordinary sources.
//! This resampling is deliberately separate from canonical picture rendering,
//! whose aperture is geometry and whose filtering follows color interpretation.

use deadpan_core::ExactRatio;
use deadpan_source::{DecodedRgbaFrame, SourceStreamInfo};
use image::RgbaImage;

const MAX_DIMENSION: u32 = 8192;
const MAX_PIXELS: u64 = 16_777_216;

/// Bilinear encoded-RGB sampling over exact pixel-edge bounds. The nearest
/// positive integral raster covers the whole aperture; normalized coordinates
/// therefore still refer to the complete clean image. Calls `check` per row.
pub fn aperture_rgba8(
    pixels: &[u8],
    backing: [u32; 2],
    stride: usize,
    bounds: [ExactRatio; 4],
    mut check: impl FnMut() -> Result<(), String>,
) -> Result<RgbaImage, String> {
    let [width, height] = backing;
    let [left, top, clean_width, clean_height] = bounds;
    if !(1..=MAX_DIMENSION).contains(&width)
        || !(1..=MAX_DIMENSION).contains(&height)
        || u64::from(width) * u64::from(height) > MAX_PIXELS
        || stride < width as usize * 4
        || stride.checked_mul(height as usize) != Some(pixels.len())
        || left.compare_integer(0).is_lt()
        || top.compare_integer(0).is_lt()
        || clean_width.compare_integer(0).is_le()
        || clean_height.compare_integer(0).is_le()
        || !left
            .checked_add(clean_width)
            .is_ok_and(|v| v.compare_integer(i64::from(width)).is_le())
        || !top
            .checked_add(clean_height)
            .is_ok_and(|v| v.compare_integer(i64::from(height)).is_le())
    {
        return Err("analysis clean aperture or raster is outside its bounds".into());
    }
    let float = |q: ExactRatio| q.numerator() as f64 / q.denominator() as f64;
    let [left, top, extent_x, extent_y] = bounds.map(float);
    let dimension = |extent: ExactRatio| -> Result<u32, String> {
        let rounded = extent
            .checked_add(ExactRatio::new(1, 2).expect("half"))
            .map_err(|_| "analysis aperture dimension overflowed")?
            .floor()
            .max(1);
        u32::try_from(rounded).map_err(|_| "analysis aperture dimension overflowed".into())
    };
    let output_width = dimension(clean_width)?;
    let output_height = dimension(clean_height)?;
    if u64::from(output_width) * u64::from(output_height) > MAX_PIXELS {
        return Err("analysis aperture exceeds its pixel budget".into());
    }
    check()?;
    let mut output = RgbaImage::new(output_width, output_height);
    for y in 0..output_height {
        check()?;
        let sy = (top + (f64::from(y) + 0.5) * extent_y / f64::from(output_height) - 0.5)
            .clamp(0.0, f64::from(height - 1));
        let y0 = sy.floor() as usize;
        let y1 = (y0 + 1).min(height as usize - 1);
        let fy = sy - y0 as f64;
        for x in 0..output_width {
            let sx = (left + (f64::from(x) + 0.5) * extent_x / f64::from(output_width) - 0.5)
                .clamp(0.0, f64::from(width - 1));
            let x0 = sx.floor() as usize;
            let x1 = (x0 + 1).min(width as usize - 1);
            let fx = sx - x0 as f64;
            let pixel = output.get_pixel_mut(x, y);
            for channel in 0..4 {
                let at = |x, y| f64::from(pixels[y * stride + x * 4 + channel]);
                let upper = at(x0, y0) * (1.0 - fx) + at(x1, y0) * fx;
                let lower = at(x0, y1) * (1.0 - fx) + at(x1, y1) * fx;
                pixel[channel] = (upper * (1.0 - fy) + lower * fy).round() as u8;
            }
        }
    }
    Ok(output)
}

/// Consume one decoded analysis picture, retaining its PTS and color codes.
/// Ordinary and already compacted integral apertures need no copy.
pub fn visible_picture(
    mut picture: DecodedRgbaFrame,
    info: &SourceStreamInfo,
    check: impl FnMut() -> Result<(), String>,
) -> Result<DecodedRgbaFrame, String> {
    let Some(bounds) = info.clean_aperture else {
        return Ok(picture);
    };
    if picture.sample_bits != 8 || [picture.width, picture.height] != [info.width, info.height] {
        return Err("analysis picture differs from its eight-bit backing raster".into());
    }
    let image = aperture_rgba8(
        &picture.rgba,
        [picture.width, picture.height],
        picture.row_stride_bytes,
        bounds,
        check,
    )?;
    picture.width = image.width();
    picture.height = image.height();
    picture.row_stride_bytes = picture.width as usize * 4;
    picture.rgba = image.into_raw();
    Ok(picture)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fractional_sampling_preserves_clean_coordinates_and_checks_cancellation() {
        // Affine ramp: the independent oracle is the continuous ramp equation.
        let mut pixels = Vec::new();
        for y in 0..8 {
            for x in 0..10 {
                pixels.extend_from_slice(&[x * 20, y * 20, 0, 255]);
            }
        }
        let q = |n, d| ExactRatio::new(n, d).unwrap();
        let bounds = [q(1, 2), q(5, 4), q(8, 1), q(11, 2)];
        let image = aperture_rgba8(&pixels, [10, 8], 40, bounds, || Ok(())).unwrap();
        assert_eq!(image.dimensions(), (8, 6));
        for (x, y, pixel) in image.enumerate_pixels() {
            assert_eq!(pixel[0], ((f64::from(x) + 0.5) * 20.0).round() as u8);
            assert_eq!(
                pixel[1],
                ((0.75 + (f64::from(y) + 0.5) * 5.5 / 6.0) * 20.0).round() as u8
            );
            assert_eq!(pixel[3], 255);
        }
        let mut calls = 0;
        assert_eq!(
            aperture_rgba8(&pixels, [10, 8], 40, bounds, || {
                calls += 1;
                if calls == 3 {
                    Err("cancelled".into())
                } else {
                    Ok(())
                }
            })
            .unwrap_err(),
            "cancelled"
        );
        assert!(
            aperture_rgba8(
                &pixels,
                [10, 8],
                40,
                [q(1, 1), q(0, 1), q(10, 1), q(8, 1)],
                || Ok(())
            )
            .is_err()
        );
    }
}
