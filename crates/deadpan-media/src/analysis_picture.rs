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
    check: impl FnMut() -> Result<(), String>,
) -> Result<RgbaImage, String> {
    let output = [dimension(bounds[2])?, dimension(bounds[3])?];
    resample_rgba8(pixels, backing, stride, bounds, output, check)
}

/// The nearest positive integral raster size for an exact extent, half up.
fn dimension(extent: ExactRatio) -> Result<u32, String> {
    let rounded = extent
        .checked_add(ExactRatio::new(1, 2).expect("half"))
        .map_err(|_| "analysis aperture dimension overflowed")?
        .floor()
        .max(1);
    u32::try_from(rounded).map_err(|_| "analysis aperture dimension overflowed".into())
}

/// Bilinear encoded-RGB sampling of exact pixel-edge `bounds` onto an
/// `output` raster that covers them completely, so normalized coordinates in
/// the output still refer to the whole bounded image. Calls `check` per row.
fn resample_rgba8(
    pixels: &[u8],
    backing: [u32; 2],
    stride: usize,
    bounds: [ExactRatio; 4],
    output: [u32; 2],
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
    let [output_width, output_height] = output;
    if !(1..=MAX_DIMENSION).contains(&output_width)
        || !(1..=MAX_DIMENSION).contains(&output_height)
        || u64::from(output_width) * u64::from(output_height) > MAX_PIXELS
    {
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

/// The square-pixel raster size for exact clean extents and a sample aspect
/// ratio: the narrower axis is enlarged by the ratio, or, where that would
/// exceed the analysis bounds, the wider axis is reduced instead. Normalized
/// coordinates are unchanged; only the pixels' shape is.
pub fn square_pixel_raster(
    extents: [ExactRatio; 2],
    sample_aspect: [u32; 2],
) -> Result<[u32; 2], String> {
    let [numerator, denominator] = sample_aspect;
    if numerator == 0 || denominator == 0 {
        return Err("analysis sample aspect ratio must be positive".into());
    }
    let sar = ExactRatio::new(i128::from(numerator), i128::from(denominator))
        .map_err(|_| "analysis sample aspect ratio is invalid")?;
    let scaled = |extent: ExactRatio, by: ExactRatio| -> Result<u32, String> {
        dimension(
            extent
                .checked_mul(by)
                .map_err(|_| "analysis raster dimension overflowed")?,
        )
    };
    let fits = |[width, height]: [u32; 2]| {
        (1..=MAX_DIMENSION).contains(&width)
            && (1..=MAX_DIMENSION).contains(&height)
            && u64::from(width) * u64::from(height) <= MAX_PIXELS
    };
    let [width, height] = extents;
    let unity = ExactRatio::ONE;
    let inverse = ExactRatio::ONE
        .checked_div(sar)
        .map_err(|_| "analysis sample aspect ratio is invalid")?;
    let (enlarged, reduced) = match sar.compare(unity) {
        std::cmp::Ordering::Equal => {
            let same = [dimension(width)?, dimension(height)?];
            return Ok(same);
        }
        // Wide pixels: more columns, or fewer rows.
        std::cmp::Ordering::Greater => (
            [scaled(width, sar)?, dimension(height)?],
            [dimension(width)?, scaled(height, inverse)?],
        ),
        // Tall pixels: more rows, or fewer columns.
        std::cmp::Ordering::Less => (
            [dimension(width)?, scaled(height, inverse)?],
            [scaled(width, sar)?, dimension(height)?],
        ),
    };
    if fits(enlarged) {
        Ok(enlarged)
    } else if fits(reduced) {
        Ok(reduced)
    } else {
        Err("analysis picture exceeds its pixel budget".into())
    }
}

/// Like [`visible_picture`], and also resampled to square pixels when the
/// stream declares a non-square sample aspect ratio, for analysers such as
/// Apple Vision that assume square pixels. Normalized coordinates still refer
/// to the whole clean picture; rotation is left to the caller.
pub fn square_pixel_picture(
    mut picture: DecodedRgbaFrame,
    info: &SourceStreamInfo,
    check: impl FnMut() -> Result<(), String>,
) -> Result<DecodedRgbaFrame, String> {
    let square = info.sample_aspect_num == info.sample_aspect_den;
    if square || info.sample_aspect_num == 0 || info.sample_aspect_den == 0 {
        return visible_picture(picture, info, check);
    }
    if picture.sample_bits != 8 || [picture.width, picture.height] != [info.width, info.height] {
        return Err("analysis picture differs from its eight-bit backing raster".into());
    }
    let bounds = info.clean_aperture.unwrap_or([
        ExactRatio::integer(0),
        ExactRatio::integer(0),
        ExactRatio::integer(i64::from(info.width)),
        ExactRatio::integer(i64::from(info.height)),
    ]);
    let output = square_pixel_raster(
        [bounds[2], bounds[3]],
        [info.sample_aspect_num, info.sample_aspect_den],
    )?;
    let image = resample_rgba8(
        &picture.rgba,
        [picture.width, picture.height],
        picture.row_stride_bytes,
        bounds,
        output,
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

    #[test]
    fn square_pixel_rasters_keep_coordinates_and_respect_bounds() {
        let q = |n| ExactRatio::integer(n);
        // 4:3 wide pixels: 240 columns become 320; 8:9 tall DV pixels add rows.
        assert_eq!(
            square_pixel_raster([q(240), q(180)], [4, 3]),
            Ok([320, 180])
        );
        assert_eq!(
            square_pixel_raster([q(720), q(480)], [8, 9]),
            Ok([720, 540])
        );
        assert_eq!(
            square_pixel_raster([q(320), q(180)], [1, 1]),
            Ok([320, 180])
        );
        // Enlarging past the dimension bound reduces the other axis instead.
        assert_eq!(
            square_pixel_raster([q(5000), q(1000)], [2, 1]),
            Ok([5000, 500])
        );
        assert!(square_pixel_raster([q(320), q(180)], [0, 1]).is_err());

        // A horizontal ramp sampled on 4:3 pixels: every output column holds
        // the ramp at its own normalized position (bilinear, edges clamped).
        let mut pixels = Vec::new();
        for _ in 0..2 {
            for x in 0..6_u8 {
                pixels.extend_from_slice(&[x * 40, 0, 0, 255]);
            }
        }
        let image = resample_rgba8(
            &pixels,
            [6, 2],
            24,
            [q(0), q(0), q(6), q(2)],
            [8, 2],
            || Ok(()),
        )
        .unwrap();
        for (x, _, pixel) in image.enumerate_pixels() {
            let source = ((f64::from(x) + 0.5) * 6.0 / 8.0 - 0.5).clamp(0.0, 5.0);
            assert_eq!(pixel[0], (source * 40.0).round() as u8);
        }
    }
}
