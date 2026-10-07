//! Bounded decoding of retained model-conditioning PNGs. Callers own input
//! identity, cancellation and deadlines around this cooperative CPU operation.

use std::io::Cursor;

pub const MAX_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_DIMENSION: u32 = 4096;
const MAX_PIXELS: u64 = 4096 * 4096;

/// Require exact RGB8 pixels and dimensions. Never convert alpha, grayscale,
/// sixteen-bit samples or a different raster into apparent conditioning data.
pub fn decode_rgb8(bytes: &[u8], width: u32, height: u32) -> Result<image::RgbImage, String> {
    if bytes.is_empty() || bytes.len() > MAX_BYTES {
        return Err("conditioning PNG bytes exceed the inspection bound".into());
    }
    if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err("conditioning PNG raster exceeds the inspection bound".into());
    }
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(width);
    limits.max_image_height = Some(height);
    limits.max_alloc = Some(MAX_PIXELS * 8);
    reader.limits(limits);
    let image::DynamicImage::ImageRgb8(image) =
        reader.decode().map_err(|error| error.to_string())?
    else {
        return Err("conditioning PNG must contain exact RGB8 pixels".into());
    };
    if image.dimensions() != (width, height) {
        return Err("conditioning PNG differs from the native raster".into());
    }
    Ok(image)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(image: image::DynamicImage) -> Vec<u8> {
        let mut encoded = Cursor::new(Vec::new());
        image
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        encoded.into_inner()
    }

    #[test]
    fn retained_rgb8_is_exact_and_other_pixel_interpretations_fail() {
        let pixels = image::RgbImage::from_pixel(3, 2, image::Rgb([10, 20, 30]));
        let bytes = png(image::DynamicImage::ImageRgb8(pixels.clone()));
        assert_eq!(decode_rgb8(&bytes, 3, 2).unwrap(), pixels);
        for (width, height) in [(0, 2), (3, 0), (2, 2), (4, 2), (4097, 1), (1, 4097)] {
            assert!(decode_rgb8(&bytes, width, height).is_err());
        }
        for image in [
            image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
                3,
                2,
                image::Rgba([10, 20, 30, 255]),
            )),
            image::DynamicImage::ImageRgb16(image::ImageBuffer::from_pixel(
                3,
                2,
                image::Rgb([10_u16, 20, 30]),
            )),
            image::DynamicImage::ImageLuma8(image::GrayImage::from_pixel(3, 2, image::Luma([10]))),
        ] {
            assert!(decode_rgb8(&png(image), 3, 2).is_err());
        }
        assert!(decode_rgb8(b"", 3, 2).is_err());
        assert!(decode_rgb8(b"not a PNG", 3, 2).is_err());
    }
}
