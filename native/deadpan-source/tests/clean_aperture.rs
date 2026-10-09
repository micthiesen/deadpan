use deadpan_source::{DecodeControl, DecodeLimits, SourceDecodeError, SourceDecoder, inspect_mp4};
use std::{fs::File, io::Write, path::PathBuf, sync::atomic::AtomicBool, time::Duration};

static CANCELLED: AtomicBool = AtomicBool::new(false);
fn control() -> DecodeControl<'static> {
    DecodeControl {
        timeout: Duration::from_secs(10),
        cancelled: &CANCELLED,
    }
}
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}
fn open(name: &str, threads: u32) -> SourceDecoder {
    SourceDecoder::open(
        File::open(fixture(name)).unwrap(),
        DecodeLimits {
            threads,
            ..DecodeLimits::default()
        },
        control(),
    )
    .unwrap()
}

// Independent expected bytes: copy original rows in source order, with no
// production crop, metadata parser, resampler, plan or geometry helper.
fn region<T: Copy>(full: &[T], coded: [usize; 2], rect: [usize; 4], channels: usize) -> Vec<T> {
    let [left, top, width, height] = rect;
    assert_eq!(full.len(), coded[0] * coded[1] * channels);
    (top..top + height)
        .flat_map(|y| {
            let start = (y * coded[0] + left) * channels;
            full[start..start + width * channels].iter().copied()
        })
        .collect()
}

#[test]
fn sdr_aperture_preserves_exact_pixels_pts_audio_and_reverse_threaded_seeks() {
    let mut original = open("cfr-bframes.mp4", 1);
    let mut cropped = open("aperture.mp4", 1);
    let mut info = original.info().clone();
    info.width = 300;
    info.height = 160;
    assert_eq!(cropped.info(), &info);
    let inspection = inspect_mp4(
        &File::open(fixture("aperture.mp4")).unwrap(),
        DecodeLimits::default(),
        control(),
    )
    .unwrap();
    assert_eq!(inspection.tracks[0].clean_aperture, Some([13, 9, 300, 160]));
    // Odd chroma origins require RGBA. A raw-plane refusal consumes no picture.
    assert!(
        matches!(cropped.next_i420(control()), Err(SourceDecodeError::Native { code, .. }) if code == "unsupported_transform")
    );
    let mut pictures = Vec::new();
    while let Some(full) = original.next_rgba(control()).unwrap() {
        let picture = cropped.next_rgba(control()).unwrap().unwrap();
        assert_eq!(picture.metadata, full.metadata);
        assert_eq!(
            (picture.width, picture.height, picture.row_stride_bytes),
            (300, 160, 1200)
        );
        assert_eq!(
            picture.rgba,
            region(&full.rgba, [320, 180], [13, 9, 300, 160], 4)
        );
        let full16 = original.copy_current_rgba16(control()).unwrap();
        let cropped16 = cropped.copy_current_rgba16(control()).unwrap();
        assert_eq!(cropped16.metadata, full16.metadata);
        assert_eq!(cropped16.row_stride_bytes, 2400);
        assert_eq!(
            cropped16.rgba,
            region(&full16.rgba, [320, 180], [13, 9, 300, 160], 8)
        );
        pictures.push(picture);
    }
    assert_eq!(pictures.len(), 120);
    assert!(cropped.next_rgba(control()).unwrap().is_none());
    let mut threaded = open("aperture.mp4", 8);
    for target in [119, 0, 61, 30, 17, 118, 1, 60, 119, 0] {
        threaded
            .seek(pictures[target].metadata.pts, control())
            .unwrap();
        loop {
            let current = threaded.next_metadata(control()).unwrap().unwrap();
            assert!(current.pts <= pictures[target].metadata.pts);
            if current.pts == pictures[target].metadata.pts {
                assert_eq!(
                    threaded.copy_current_rgba(control()).unwrap(),
                    pictures[target]
                );
                break;
            }
        }
    }
}

fn expected_planes<T: Copy>(full: &[T]) -> Vec<T> {
    let mut expected = region(&full[..64 * 36], [64, 36], [6, 2, 48, 28], 1);
    for plane in full[64 * 36..].chunks_exact(32 * 18) {
        expected.extend(region(plane, [32, 18], [3, 1, 24, 14], 1));
    }
    expected
}

#[test]
fn hdr_aperture_preserves_sixteen_bit_rgba_raw_planes_and_fresh_gop_decode() {
    let mut original = open("hevc-pq.mp4", 1);
    let mut cropped = open("aperture-hdr.mp4", 8);
    assert_eq!((cropped.info().width, cropped.info().height), (48, 28));
    assert_eq!(cropped.info().color, original.info().color);
    let mut count = 0;
    while let Some(full) = original.next_yuv420p10(control()).unwrap() {
        let picture = cropped.next_yuv420p10(control()).unwrap().unwrap();
        assert_eq!(picture.metadata, full.metadata);
        assert_eq!(picture.samples, expected_planes(&full.samples));
        let full_rgba = original.copy_current_rgba16(control()).unwrap();
        let rgba = cropped.copy_current_rgba16(control()).unwrap();
        assert_eq!(
            rgba.rgba,
            region(&full_rgba.rgba, [64, 36], [6, 2, 48, 28], 8)
        );
        if count == 4 {
            let mut fresh = SourceDecoder::open_at_keyframe(
                File::open(fixture("aperture-hdr.mp4")).unwrap(),
                DecodeLimits::default(),
                control(),
                picture.metadata.source.pts,
            )
            .unwrap();
            assert_eq!(fresh.next_yuv420p10(control()).unwrap().unwrap(), picture);
            fresh
                .restart_at_keyframe(picture.metadata.source.pts, control())
                .unwrap();
            assert_eq!(fresh.next_yuv420p10(control()).unwrap().unwrap(), picture);
        }
        count += 1;
    }
    assert_eq!(count, 8);
    assert!(cropped.next_yuv420p10(control()).unwrap().is_none());
}

#[test]
fn malformed_apertures_fail_before_decode() {
    let source = std::fs::read(fixture("aperture.mp4")).unwrap();
    let tag = source.windows(4).position(|b| b == b"clap").unwrap();
    for (field, value, code) in [
        (1, 0, "invalid_input"),
        (0, -1, "invalid_input"),
        (5, -1, "invalid_input"),
        (6, 12, "invalid_input"),
        (4, -11, "invalid_input"),
    ] {
        let mut bytes = source.clone();
        bytes[tag + 4 + field * 4..tag + 8 + field * 4].copy_from_slice(&i32::to_be_bytes(value));
        let mut file = tempfile::tempfile().unwrap();
        file.write_all(&bytes).unwrap();
        let error = SourceDecoder::open(file, DecodeLimits::default(), control())
            .err()
            .unwrap();
        assert!(
            matches!(error, SourceDecodeError::Native { code: actual, .. } if actual == code),
            "field {field}: expected {code}"
        );
    }
}

#[test]
fn fractional_apertures_retain_exact_geometry_and_unfiltered_backing_pixels() {
    use deadpan_core::ExactRatio as Q;
    for (source, cropped, rect, frames, hdr) in [
        (
            "cfr-bframes.mp4",
            "aperture-fractional.mp4",
            [(53, 4), (37, 4), (599, 2), (319, 2)],
            120,
            false,
        ),
        (
            "hevc-pq.mp4",
            "aperture-fractional-hdr.mp4",
            [(25, 4), (9, 4), (95, 2), (55, 2)],
            8,
            true,
        ),
    ] {
        let bounds = rect.map(|(n, d)| Q::new(n, d).unwrap());
        let mut full = open(source, 1);
        let mut clean = open(cropped, 8);
        let mut expected = full.info().clone();
        expected.clean_aperture = Some(bounds);
        assert_eq!(clean.info(), &expected);
        assert_eq!(clean.info().visible_bounds().unwrap(), bounds);
        let inspection = inspect_mp4(
            &File::open(fixture(cropped)).unwrap(),
            DecodeLimits::default(),
            control(),
        )
        .unwrap();
        assert_eq!(inspection.tracks[0].clean_aperture, None);
        assert_eq!(inspection.tracks[0].clean_aperture_bounds, Some(bounds));
        assert!(clean.next_i420(control()).is_err());
        assert!(clean.next_yuv420p10(control()).is_err());
        let mut last = None;
        for _ in 0..frames {
            let expected = if hdr {
                full.next_rgba16(control())
            } else {
                full.next_rgba(control())
            }
            .unwrap()
            .unwrap();
            let actual = if hdr {
                clean.next_rgba16(control())
            } else {
                clean.next_rgba(control())
            }
            .unwrap()
            .unwrap();
            assert_eq!(actual, expected);
            last = Some(actual);
        }
        assert!(clean.next_metadata(control()).unwrap().is_none());
        let last = last.unwrap();
        clean.seek(last.metadata.pts, control()).unwrap();
        loop {
            let metadata = clean.next_metadata(control()).unwrap().unwrap();
            assert!(metadata.pts <= last.metadata.pts);
            if metadata.pts == last.metadata.pts {
                break;
            }
        }
        let actual = if hdr {
            clean.copy_current_rgba16(control())
        } else {
            clean.copy_current_rgba(control())
        }
        .unwrap();
        assert_eq!(actual, last);
        for invalid in [
            [Q::integer(-1), Q::ZERO, Q::ONE, Q::ONE],
            [Q::ZERO, Q::ZERO, Q::integer(8193), Q::ONE],
            [Q::new(i128::MAX, 1).unwrap(), Q::ZERO, Q::ONE, Q::ONE],
        ] {
            expected.clean_aperture = Some(invalid);
            assert!(expected.visible_bounds().is_err());
        }
    }
}

#[test]
fn aperture_never_reduces_the_decoder_allocation_admission_bound() {
    let error = SourceDecoder::open(
        File::open(fixture("aperture.mp4")).unwrap(),
        DecodeLimits {
            max_pixels: 300 * 160,
            ..DecodeLimits::default()
        },
        control(),
    )
    .err()
    .unwrap();
    assert!(matches!(error, SourceDecodeError::Native { code, .. } if code == "resource_limit"));
}

#[test]
fn chroma_aligned_sdr_raw_planes_match_the_original() {
    let mut bytes = std::fs::read(fixture("aperture.mp4")).unwrap();
    let tag = bytes.windows(4).position(|b| b == b"clap").unwrap();
    // Aperture [14,8,300,160], still relative to the 320x180 visible codec raster.
    bytes[tag + 20..tag + 24].copy_from_slice(&4_i32.to_be_bytes());
    bytes[tag + 28..tag + 32].copy_from_slice(&(-2_i32).to_be_bytes());
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(&bytes).unwrap();
    let mut cropped = SourceDecoder::open(file, DecodeLimits::default(), control()).unwrap();
    let mut original = open("cfr-bframes.mp4", 1);
    for _ in 0..120 {
        let full = original.next_i420(control()).unwrap().unwrap();
        let actual = cropped.next_i420(control()).unwrap().unwrap();
        assert_eq!(actual.metadata, full.metadata);
        let mut expected = region(&full.i420[..320 * 180], [320, 180], [14, 8, 300, 160], 1);
        for plane in full.i420[320 * 180..].chunks_exact(160 * 90) {
            expected.extend(region(plane, [160, 90], [7, 4, 150, 80], 1));
        }
        assert_eq!(actual.i420, expected);
        assert_eq!(cropped.copy_current_i420(control()).unwrap(), actual);
    }
    assert!(cropped.next_i420(control()).unwrap().is_none());
}
