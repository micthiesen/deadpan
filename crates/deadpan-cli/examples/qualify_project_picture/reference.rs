//! Example-only numerical oracle. It deliberately bypasses the production
//! working matrix, half-float reader, OETF and I420 converter. Source geometry
//! is shared with the preview reference; known Rec.709 source primaries make
//! the round trip through Rec.2020 unnecessary in this f64 reference.

use std::time::Instant;

use deadpan_render::{PictureGeometry, Primaries, Rgba8Frame, Transfer};
use serde_json::{Value, json};

use super::{Result, check_deadline};

fn linear(code: u8, transfer: Transfer) -> f64 {
    let value = f64::from(code) / 255.0;
    match transfer {
        Transfer::Srgb if value <= 0.04045 => value / 12.92,
        Transfer::Srgb => ((value + 0.055) / 1.055).powf(2.4),
        Transfer::Rec709 if value < 0.081 => value / 4.5,
        Transfer::Rec709 => ((value + 0.099) / 1.099).powf(1.0 / 0.45),
        Transfer::Linear => value,
        // HDR transfers are outside this SDR oracle; NaN fails every comparison.
        Transfer::Pq | Transfer::Hlg => f64::NAN,
    }
}

fn ycbcr(rgb: [f64; 3]) -> [f64; 3] {
    let [r, g, b] = rgb.map(|component| {
        let value = component.clamp(0.0, 1.0);
        if value < 0.018 {
            value * 4.5
        } else {
            1.099 * value.powf(0.45) - 0.099
        }
    });
    let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    [
        y,
        (b - y) / (2.0 * (1.0 - 0.0722)),
        (r - y) / (2.0 * (1.0 - 0.2126)),
    ]
}

fn code(value: f64, minimum: f64, maximum: f64) -> Result<u8> {
    let value = (value.clamp(minimum, maximum) + 0.5).floor();
    if !value.is_finite() || !(0.0..=255.0).contains(&value) {
        return Err("invalid independent reference code".into());
    }
    // Finite integral value was checked above.
    Ok(value as u8)
}

pub(super) fn i420(
    source: &Rgba8Frame,
    geometry: &PictureGeometry,
    raster: [u32; 2],
    deadline: Instant,
) -> Result<Vec<u8>> {
    let metadata = source.metadata();
    if metadata.color.primaries != Primaries::Rec709
        || raster.contains(&0)
        || raster.iter().any(|axis| !axis.is_multiple_of(2))
    {
        return Err("reference requires Rec.709 source primaries and an even raster".into());
    }
    let source_width = usize::try_from(metadata.width)?;
    let source_height = usize::try_from(metadata.height)?;
    let stride = usize::try_from(metadata.row_stride_bytes)?;
    let mut pixels = Vec::with_capacity(source_width * source_height);
    for row in source.bytes().chunks_exact(stride) {
        for rgba in row[..source_width * 4].chunks_exact(4) {
            let alpha = f64::from(rgba[3]) / 255.0;
            pixels.push(
                [rgba[0], rgba[1], rgba[2]]
                    .map(|value| linear(value, metadata.color.transfer) * alpha),
            );
        }
    }
    let [width, height] = raster;
    let row_width = usize::try_from(width)?;
    let count = row_width * usize::try_from(height)?;
    let mut output = vec![0; count * 3 / 2];
    let mut rows = [vec![[0.0; 3]; row_width], vec![[0.0; 3]; row_width]];
    for pair in 0..height / 2 {
        check_deadline(deadline)?;
        for (row_number, row) in rows.iter_mut().enumerate() {
            let y = pair * 2 + u32::try_from(row_number)?;
            for (x, value) in row.iter_mut().enumerate() {
                let mut rgb = [0.0; 3];
                if let Some([u, v]) =
                    geometry.source_uv([f64::from(u32::try_from(x)?) + 0.5, f64::from(y) + 0.5])
                {
                    let sx = u * f64::from(metadata.width) - 0.5;
                    let sy = v * f64::from(metadata.height) - 0.5;
                    let fx = sx - sx.floor();
                    let fy = sy - sy.floor();
                    for (dx, wx) in [(0.0, 1.0 - fx), (1.0, fx)] {
                        for (dy, wy) in [(0.0, 1.0 - fy), (1.0, fy)] {
                            // Geometry and metadata bound each clamped integral index.
                            let px = (sx.floor() + dx).clamp(0.0, f64::from(metadata.width - 1))
                                as usize;
                            let py = (sy.floor() + dy).clamp(0.0, f64::from(metadata.height - 1))
                                as usize;
                            for (destination, component) in
                                rgb.iter_mut().zip(pixels[py * source_width + px])
                            {
                                *destination += component * wx * wy;
                            }
                        }
                    }
                }
                *value = ycbcr(rgb);
                output[usize::try_from(y)? * row_width + x] =
                    code(16.0 + 219.0 * value[0], 16.0, 235.0)?;
            }
        }
        for x in 0..row_width / 2 {
            let column = x * 2;
            let mut chroma = [0.0; 2];
            for row in &rows {
                for (column, weight) in [
                    (column.saturating_sub(1), 0.125),
                    (column, 0.25),
                    ((column + 1).min(row_width - 1), 0.125),
                ] {
                    for (destination, value) in chroma.iter_mut().zip(&row[column][1..]) {
                        *destination += value * weight;
                    }
                }
            }
            let offset = usize::try_from(pair)? * (row_width / 2) + x;
            for (channel, value) in chroma.into_iter().enumerate() {
                output[count + channel * (count / 4) + offset] =
                    code(128.0 + 224.0 * value, 16.0, 240.0)?;
            }
        }
    }
    Ok(output)
}

pub(super) fn compare(actual: &[u8], expected: &[u8], raster: [u32; 2]) -> Result<(bool, Value)> {
    let y_length = usize::try_from(u64::from(raster[0]) * u64::from(raster[1]))?;
    if actual.len() != y_length * 3 / 2 || actual.len() != expected.len() {
        return Err("reference plane lengths disagree".into());
    }
    let mut planes = Vec::new();
    let mut passed = true;
    for (name, start, length) in [
        ("Y", 0, y_length),
        ("Cb", y_length, y_length / 4),
        ("Cr", y_length * 5 / 4, y_length / 4),
    ] {
        let mut max_error = 0u8;
        let mut mismatches = 0usize;
        let mut beyond_one = 0usize;
        let mut sum = 0u64;
        for (actual, expected) in actual[start..start + length]
            .iter()
            .zip(&expected[start..start + length])
        {
            let difference = actual.abs_diff(*expected);
            max_error = max_error.max(difference);
            mismatches += usize::from(difference != 0);
            beyond_one += usize::from(difference > 1);
            sum += u64::from(difference);
        }
        passed &= beyond_one == 0;
        planes.push(
            json!({"plane": name, "codes": length, "different_codes": mismatches,
            "codes_beyond_one": beyond_one, "maximum_absolute_error": max_error,
            "mean_absolute_error": sum as f64 / length as f64}),
        );
    }
    Ok((
        passed,
        json!({"tolerance_codes": 1, "reason": "f64 reference omits actual GPU half-float quantization", "planes": planes}),
    ))
}
