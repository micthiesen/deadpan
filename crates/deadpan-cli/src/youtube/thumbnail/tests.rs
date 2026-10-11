use super::*;
use std::cell::Cell;

const ID: &str = "Z4C82eyhwgU";
const URL: &str = "https://i.ytimg.com/vi/Z4C82eyhwgU/maxresdefault.jpg";

fn picture(width: u32, height: u32, format: ImageFormat) -> Vec<u8> {
    let pixels = image::RgbImage::from_fn(width, height, |x, y| {
        image::Rgb([
            if x < width / 2 { 200 } else { 20 },
            if y < height / 2 { 180 } else { 30 },
            80,
        ])
    });
    let mut encoded = Cursor::new(Vec::new());
    DynamicImage::ImageRgb8(pixels)
        .write_to(&mut encoded, format)
        .unwrap();
    encoded.into_inner()
}

#[test]
fn decodes_the_three_remote_formats_to_owned_bounded_rgba_without_stretching() {
    for format in [ImageFormat::Png, ImageFormat::Jpeg, ImageFormat::WebP] {
        let bytes = picture(960, 540, format);
        let image = Thumbnail::decode(&bytes).unwrap();
        assert_eq!(image.dimensions(), [480, 270]);
        assert_eq!(image.rgba().len(), 480 * 270 * 4);
        assert!(image.rgba().chunks_exact(4).all(|pixel| pixel[3] == 255));
        let top_left = &image.rgba()[(10 * 480 + 10) * 4..][..4];
        let bottom_right = &image.rgba()[(260 * 480 + 470) * 4..][..4];
        for (actual, expected) in top_left.iter().zip([200i16, 180, 80, 255]) {
            assert!(
                (i16::from(*actual) - expected).abs() < 5,
                "{format:?}: {top_left:?}"
            );
        }
        for (actual, expected) in bottom_right.iter().zip([20i16, 30, 80, 255]) {
            assert!(
                (i16::from(*actual) - expected).abs() < 5,
                "{format:?}: {bottom_right:?}"
            );
        }
    }
    assert_eq!(
        Thumbnail::decode(&picture(300, 600, ImageFormat::Png))
            .unwrap()
            .dimensions(),
        [135, 270]
    );
    assert_eq!(
        Thumbnail::decode(&picture(120, 90, ImageFormat::Png))
            .unwrap()
            .dimensions(),
        [120, 90]
    );
}

#[test]
fn lossy_webp_decodes_its_coded_raster_and_preserves_quadrants() {
    let thumbnail =
        Thumbnail::decode(include_bytes!("../../../tests/fixtures/youtube-lossy.webp")).unwrap();
    assert_eq!(thumbnail.dimensions(), [120, 80]);
    for (x, y, expected) in [
        (10, 10, [200i16, 180, 80, 255]),
        (110, 70, [20, 30, 80, 255]),
    ] {
        let pixel = &thumbnail.rgba()[(y * 120 + x) * 4..][..4];
        for (actual, expected) in pixel.iter().zip(expected) {
            assert!((i16::from(*actual) - expected).abs() <= 5, "{pixel:?}");
        }
    }
}

#[test]
fn jpeg_orientation_is_applied_before_sizing() {
    let mut bytes = picture(120, 80, ImageFormat::Jpeg);
    // APP1 Exif, little-endian TIFF: one SHORT orientation tag, value 6
    // (90 degrees clockwise), and no next directory.
    let exif = [
        0xff, 0xe1, 0x00, 0x22, b'E', b'x', b'i', b'f', 0, 0, b'I', b'I', 42, 0, 8, 0, 0, 0, 1, 0,
        0x12, 0x01, 3, 0, 1, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0,
    ];
    bytes.splice(2..2, exif);
    let thumbnail = Thumbnail::decode(&bytes).unwrap();
    assert_eq!(thumbnail.dimensions(), [80, 120]);
    let rotated_top_left = &thumbnail.rgba()[(10 * 80 + 10) * 4..][..4];
    // The original bottom-left red/dark-green quadrant is now top-left.
    assert!(
        rotated_top_left[0] > 190 && rotated_top_left[1] < 40,
        "{rotated_top_left:?}"
    );
}

#[test]
fn refuses_encoded_dimension_and_area_limits_before_publishing_pixels() {
    assert_eq!(
        Thumbnail::decode(&vec![0; MAX_BYTES + 1]),
        Err(Unavailable::TooLarge)
    );
    assert_eq!(
        Thumbnail::decode(&picture(MAX_DIMENSION + 1, 1, ImageFormat::Png)),
        Err(Unavailable::TooLarge)
    );
    assert_eq!(
        Thumbnail::decode(&picture(4096, 1025, ImageFormat::Png)),
        Err(Unavailable::TooLarge)
    );
    assert_eq!(Thumbnail::decode(&[]), Err(Unavailable::InvalidImage));
    assert_eq!(
        Thumbnail::decode(b"<svg></svg>"),
        Err(Unavailable::UnsupportedImage)
    );
    assert_eq!(
        Thumbnail::decode(b"GIF89a"),
        Err(Unavailable::UnsupportedImage)
    );
    let bytes = picture(120, 90, ImageFormat::Png);
    assert_eq!(
        Thumbnail::decode(&bytes[..bytes.len() / 2]),
        Err(Unavailable::InvalidImage)
    );
}

struct FakeFetch {
    calls: Cell<usize>,
    bytes: Vec<u8>,
    fail: Option<Unavailable>,
    cancel: bool,
}

impl FakeFetch {
    fn good() -> Self {
        Self {
            calls: Cell::new(0),
            bytes: picture(120, 90, ImageFormat::Png),
            fail: None,
            cancel: false,
        }
    }
}

impl Fetch for FakeFetch {
    fn fetch(&self, url: &str, cancelled: &AtomicBool) -> Result<Vec<u8>, FetchError> {
        assert_eq!(url, URL);
        self.calls.set(self.calls.get() + 1);
        if self.cancel {
            cancelled.store(true, Ordering::Relaxed);
        }
        match self.fail {
            Some(reason) => Err(FetchError::Unavailable(reason)),
            None => Ok(self.bytes.clone()),
        }
    }
}

#[test]
fn absent_and_rejected_addresses_never_start_a_network_request() {
    let transport = FakeFetch::good();
    let cancelled = AtomicBool::new(false);
    assert_eq!(
        load_with(ID, None, &cancelled, &transport),
        Ok(ThumbnailState::Missing)
    );
    assert_eq!(
        load_with("wrong_video", Some(URL), &cancelled, &transport),
        Ok(ThumbnailState::Unavailable(Unavailable::Address))
    );
    assert_eq!(
        load_with(
            ID,
            Some("file:///private/etc/passwd"),
            &cancelled,
            &transport
        ),
        Ok(ThumbnailState::Unavailable(Unavailable::Address))
    );
    assert_eq!(transport.calls.get(), 0);
}

#[test]
fn optional_thumbnail_failures_are_safe_confirmation_states() {
    let cancelled = AtomicBool::new(false);
    let mut transport = FakeFetch::good();
    assert!(matches!(
        load_with(ID, Some(URL), &cancelled, &transport),
        Ok(ThumbnailState::Ready(_))
    ));
    transport.fail = Some(Unavailable::Timeout);
    assert_eq!(
        load_with(ID, Some(URL), &cancelled, &transport),
        Ok(ThumbnailState::Unavailable(Unavailable::Timeout))
    );
    transport.fail = None;
    transport.bytes = b"not an image".to_vec();
    assert_eq!(
        load_with(ID, Some(URL), &cancelled, &transport),
        Ok(ThumbnailState::Unavailable(Unavailable::UnsupportedImage))
    );
    assert_eq!(transport.calls.get(), 3);
}

#[test]
fn cancellation_before_or_during_fetch_never_reaches_confirmation() {
    let cancelled = AtomicBool::new(true);
    let mut transport = FakeFetch::good();
    assert_eq!(
        load_with(ID, Some(URL), &cancelled, &transport),
        Err(Cancelled)
    );
    assert_eq!(transport.calls.get(), 0);
    cancelled.store(false, Ordering::Relaxed);
    transport.cancel = true;
    assert_eq!(
        load_with(ID, Some(URL), &cancelled, &transport),
        Err(Cancelled)
    );
    cancelled.store(false, Ordering::Relaxed);
    transport.fail = Some(Unavailable::Network);
    assert_eq!(
        load_with(ID, Some(URL), &cancelled, &transport),
        Err(Cancelled)
    );
    assert_eq!(transport.calls.get(), 2);
}

#[test]
fn body_reader_bounds_undeclared_responses_and_observes_cancellation_between_reads() {
    let cancelled = AtomicBool::new(false);
    assert_eq!(
        read_body(Cursor::new(vec![23; MAX_BYTES]), &cancelled)
            .unwrap()
            .len(),
        MAX_BYTES
    );
    let mut oversized = Cursor::new(vec![23; MAX_BYTES + 8192]);
    assert!(matches!(
        read_body(&mut oversized, &cancelled),
        Err(FetchError::Unavailable(Unavailable::TooLarge))
    ));
    assert_eq!(oversized.position(), (MAX_BYTES + 1) as u64);

    struct CancellingReader<'a> {
        cancelled: &'a AtomicBool,
        reads: usize,
    }
    impl Read for CancellingReader<'_> {
        fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
            self.reads += 1;
            bytes[..3].copy_from_slice(b"abc");
            self.cancelled.store(true, Ordering::Relaxed);
            Ok(3)
        }
    }
    let mut reader = CancellingReader {
        cancelled: &cancelled,
        reads: 0,
    };
    assert!(matches!(
        read_body(&mut reader, &cancelled),
        Err(FetchError::Cancelled)
    ));
    assert_eq!(reader.reads, 1);
}

#[test]
fn interrupted_body_is_unavailable_and_ureq_timeout_keeps_its_reason() {
    struct BrokenReader;
    impl Read for BrokenReader {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "private response detail",
            ))
        }
    }
    assert!(matches!(
        read_body(BrokenReader, &AtomicBool::new(false)),
        Err(FetchError::Unavailable(Unavailable::Network))
    ));
    let timeout = ureq::Error::Timeout(ureq::Timeout::Global).into_io();
    assert!(matches!(
        network_error(timeout.into()),
        FetchError::Unavailable(Unavailable::Timeout)
    ));
}
