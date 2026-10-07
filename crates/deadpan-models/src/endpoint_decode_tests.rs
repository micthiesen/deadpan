use super::*;

fn encoded(image: image::DynamicImage) -> Vec<u8> {
    let mut bytes = Cursor::new(Vec::new());
    image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    bytes.into_inner()
}

#[test]
fn endpoint_png_decoder_requires_exact_rgb8_and_raster() {
    let cancelled = AtomicBool::new(false);
    let control = Control {
        deadline: Instant::now() + Duration::from_secs(10),
        cancelled: &cancelled,
    };
    let pixels = image::RgbImage::from_pixel(4, 2, image::Rgb([10, 20, 30]));
    let bytes = encoded(image::DynamicImage::ImageRgb8(pixels.clone()));
    assert_eq!(decode_png(&bytes, 4, 2, &control).unwrap(), pixels);
    assert!(decode_png(&bytes, 2, 2, &control).is_err());
    assert!(decode_png(&bytes, 8, 2, &control).is_err());
    assert!(decode_png(b"invalid PNG", 4, 2, &control).is_err());
    for image in [
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            4,
            2,
            image::Rgba([10, 20, 30, 255]),
        )),
        image::DynamicImage::ImageRgb16(image::ImageBuffer::from_pixel(
            4,
            2,
            image::Rgb([10u16, 20, 30]),
        )),
        image::DynamicImage::ImageLuma8(image::GrayImage::from_pixel(4, 2, image::Luma([10]))),
    ] {
        assert!(decode_png(&encoded(image), 4, 2, &control).is_err());
    }
    assert!(decode_png(&bytes, 0, 2, &control).is_err());
    assert!(decode_png(&bytes, 4097, 4096, &control).is_err());
}

#[test]
fn endpoint_decode_obeys_cancellation_and_shared_deadline() {
    let cancelled = AtomicBool::new(true);
    let control = Control {
        deadline: Instant::now() + Duration::from_secs(10),
        cancelled: &cancelled,
    };
    assert!(matches!(
        decode_png(b"", 4, 2, &control),
        Err(QualificationError::Cancelled)
    ));
    cancelled.store(false, Ordering::Release);
    let control = Control {
        deadline: Instant::now(),
        cancelled: &cancelled,
    };
    assert!(matches!(
        decode_png(b"", 4, 2, &control),
        Err(QualificationError::Deadline)
    ));
}
