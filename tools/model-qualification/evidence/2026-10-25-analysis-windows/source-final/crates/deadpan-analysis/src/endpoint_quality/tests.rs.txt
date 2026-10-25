use super::*;

fn rgb(width: u32, height: u32, mut pixel: impl FnMut(u32, u32) -> [u8; 3]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(width as usize * height as usize * 3);
    for y in 0..height {
        for x in 0..width {
            bytes.extend_from_slice(&pixel(x, y));
        }
    }
    bytes
}

fn view(bytes: &[u8], width: u32, height: u32) -> RgbView<'_> {
    RgbView::new(bytes, width, height, width as usize * 3, PixelChannels::Rgb).unwrap()
}

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-12,
        "{actual} differs from expected {expected}"
    );
}

#[test]
fn identical_and_modest_rgb_changes_are_measured_without_gross_coverage() {
    let left = rgb(3, 5, |_, _| [10, 40, 80]);
    let same = rgb(3, 5, |_, _| [10, 40, 80]);
    let modest = rgb(3, 5, |_, _| [20, 50, 90]);

    let identical = compare(view(&left, 3, 5), view(&same, 3, 5), [0, 0, 3, 5], 64.0).unwrap();
    close(identical.mean_absolute_rgb_difference, 0.0);
    close(identical.gross_cell_fraction, 0.0);

    let changed = compare(view(&left, 3, 5), view(&modest, 3, 5), [0, 0, 3, 5], 64.0).unwrap();
    close(changed.mean_absolute_rgb_difference, 10.0);
    close(changed.gross_cell_fraction, 0.0);
}

#[test]
fn threshold_is_inclusive_and_gross_replacements_cover_their_area() {
    let black = rgb(64, 36, |_, _| [0, 0, 0]);
    let exact = rgb(64, 36, |_, _| [64, 64, 64]);
    let white = rgb(64, 36, |_, _| [255, 255, 255]);

    let at_threshold = compare(
        view(&black, 64, 36),
        view(&exact, 64, 36),
        [0, 0, 64, 36],
        64.0,
    )
    .unwrap();
    close(at_threshold.mean_absolute_rgb_difference, 64.0);
    close(at_threshold.gross_cell_fraction, 1.0);

    let above_threshold = compare(
        view(&black, 64, 36),
        view(&exact, 64, 36),
        [0, 0, 64, 36],
        64.000_001,
    )
    .unwrap();
    close(above_threshold.gross_cell_fraction, 0.0);

    let replacement = compare(
        view(&black, 64, 36),
        view(&white, 64, 36),
        [0, 0, 64, 36],
        64.0,
    )
    .unwrap();
    close(replacement.mean_absolute_rgb_difference, 255.0);
    close(replacement.gross_cell_fraction, 1.0);
}

#[test]
fn opposite_chromatic_changes_do_not_cancel_at_matching_encoded_luma() {
    let left_pixel = [255, 0, 0];
    let right_pixel = [0, 130, 5];
    let code_luma = |pixel: [u8; 3]| {
        (77 * u32::from(pixel[0]) + 150 * u32::from(pixel[1]) + 29 * u32::from(pixel[2]) + 128) >> 8
    };
    assert_eq!(code_luma(left_pixel), code_luma(right_pixel));

    let left = rgb(1, 1, |_, _| left_pixel);
    let right = rgb(1, 1, |_, _| right_pixel);
    let difference = compare(view(&left, 1, 1), view(&right, 1, 1), [0, 0, 1, 1], 64.0).unwrap();
    close(difference.mean_absolute_rgb_difference, 130.0);
    close(difference.gross_cell_fraction, 1.0);
}

#[test]
fn local_change_has_local_coverage_while_broad_change_counts_by_area() {
    let black = rgb(64, 36, |_, _| [0, 0, 0]);
    let local = rgb(64, 36, |x, y| {
        if x < 8 && y < 9 {
            [255, 255, 255]
        } else {
            [0, 0, 0]
        }
    });
    let broad = rgb(
        64,
        36,
        |x, _| {
            if x < 48 { [255, 255, 255] } else { [0, 0, 0] }
        },
    );

    let local_difference = compare(
        view(&black, 64, 36),
        view(&local, 64, 36),
        [0, 0, 64, 36],
        64.0,
    )
    .unwrap();
    close(local_difference.gross_cell_fraction, 72.0 / 2304.0);
    assert!(local_difference.mean_absolute_rgb_difference < 64.0);

    let broad_difference = compare(
        view(&black, 64, 36),
        view(&broad, 64, 36),
        [0, 0, 64, 36],
        64.0,
    )
    .unwrap();
    close(broad_difference.gross_cell_fraction, 0.75);
    assert!(broad_difference.mean_absolute_rgb_difference >= 64.0);
}

#[test]
fn nondivisible_grid_edges_are_weighted_by_source_pixel_area() {
    let width = 65;
    let height = 37;
    let black = rgb(width, height, |_, _| [0, 0, 0]);
    // Center-based mapping puts these four pixels in one 64x36 grid cell.
    let one_cell = rgb(width, height, |x, y| {
        if (32..34).contains(&x) && (18..20).contains(&y) {
            [255, 255, 255]
        } else {
            [0, 0, 0]
        }
    });
    let difference = compare(
        view(&black, width, height),
        view(&one_cell, width, height),
        [0, 0, width, height],
        64.0,
    )
    .unwrap();
    close(
        difference.gross_cell_fraction,
        4.0 / f64::from(width * height),
    );
}

#[test]
fn the_supplied_region_is_half_open_and_does_not_trim_black_bars() {
    let black = rgb(4, 4, |_, _| [0, 0, 0]);
    let changed_outside = rgb(4, 4, |x, y| {
        if x == 0 || x == 3 || y == 0 || y == 3 {
            [255, 255, 255]
        } else {
            [0, 0, 0]
        }
    });
    let interior = compare(
        view(&black, 4, 4),
        view(&changed_outside, 4, 4),
        [1, 1, 2, 2],
        64.0,
    )
    .unwrap();
    close(interior.mean_absolute_rgb_difference, 0.0);
    close(interior.gross_cell_fraction, 0.0);

    // Two unchanged black-bar rows remain in the caller's requested region's
    // denominator; no automatic content crop removes them.
    let content_only = rgb(2, 4, |_, y| {
        if (1..3).contains(&y) {
            [255, 255, 255]
        } else {
            [0, 0, 0]
        }
    });
    let all_black = rgb(2, 4, |_, _| [0, 0, 0]);
    let full = compare(
        view(&all_black, 2, 4),
        view(&content_only, 2, 4),
        [0, 0, 2, 4],
        64.0,
    )
    .unwrap();
    close(full.mean_absolute_rgb_difference, 127.5);
    close(full.gross_cell_fraction, 0.5);
}

#[test]
fn alpha_is_ignored_and_row_padding_is_not_read_as_pixels() {
    let left_rgba = [20, 80, 140, 0];
    let right_rgba = [20, 80, 140, 255];
    let alpha_difference = compare(
        RgbView::new(&left_rgba, 1, 1, 4, PixelChannels::Rgba).unwrap(),
        RgbView::new(&right_rgba, 1, 1, 4, PixelChannels::Rgba).unwrap(),
        [0, 0, 1, 1],
        64.0,
    )
    .unwrap();
    close(alpha_difference.mean_absolute_rgb_difference, 0.0);
    close(alpha_difference.gross_cell_fraction, 0.0);

    let width = 2;
    let height = 2;
    let stride = 9;
    let row_bytes = width as usize * 3;
    let length = stride * (height as usize - 1) + row_bytes;
    let mut left = vec![0x11; length];
    let mut right = vec![0xee; length];
    for y in 0..height as usize {
        for x in 0..width as usize {
            let offset = y * stride + x * 3;
            left[offset..offset + 3].copy_from_slice(&[20, 80, 140]);
            right[offset..offset + 3].copy_from_slice(&[20, 80, 140]);
        }
    }
    let padding_only = compare(
        RgbView::new(&left, width, height, stride, PixelChannels::Rgb).unwrap(),
        RgbView::new(&right, width, height, stride, PixelChannels::Rgb).unwrap(),
        [0, 0, width, height],
        64.0,
    )
    .unwrap();
    close(padding_only.mean_absolute_rgb_difference, 0.0);
    close(padding_only.gross_cell_fraction, 0.0);
}

#[test]
fn tiny_images_have_no_empty_grid_cell_dilution() {
    let black = [0, 0, 0];
    let changed = [64, 64, 64];
    let result = compare(
        RgbView::new(&black, 1, 1, 3, PixelChannels::Rgb).unwrap(),
        RgbView::new(&changed, 1, 1, 3, PixelChannels::Rgb).unwrap(),
        [0, 0, 1, 1],
        64.0,
    )
    .unwrap();
    close(result.mean_absolute_rgb_difference, 64.0);
    close(result.gross_cell_fraction, 1.0);

    let odd_left = rgb(3, 5, |_, _| [0, 0, 0]);
    let odd_right = rgb(3, 5, |x, y| {
        if x == 1 && y == 2 {
            [0, 0, 0]
        } else {
            [255, 255, 255]
        }
    });
    let odd = compare(
        view(&odd_left, 3, 5),
        view(&odd_right, 3, 5),
        [0, 0, 3, 5],
        64.0,
    )
    .unwrap();
    close(odd.gross_cell_fraction, 14.0 / 15.0);
}

#[test]
fn invalid_dimensions_strides_buffers_and_pixel_bounds_are_rejected() {
    assert_eq!(
        RgbView::new(&[], 0, 1, 0, PixelChannels::Rgb),
        Err(Error::EmptyPicture)
    );
    assert_eq!(
        RgbView::new(&[], 1, 0, 0, PixelChannels::Rgb),
        Err(Error::EmptyPicture)
    );
    assert_eq!(
        RgbView::new(&[], 4097, 4096, 4097 * 3, PixelChannels::Rgb),
        Err(Error::PixelLimit)
    );
    assert_eq!(
        RgbView::new(&[], u32::MAX, u32::MAX, 0, PixelChannels::Rgb),
        Err(Error::PixelLimit)
    );
    assert_eq!(
        RgbView::new(&[0; 3], 1, 1, 2, PixelChannels::Rgb),
        Err(Error::InvalidStride)
    );
    assert_eq!(
        RgbView::new(&[0; 2], 1, 1, 3, PixelChannels::Rgb),
        Err(Error::BufferTooShort)
    );
    assert_eq!(
        RgbView::new(&[], 1, 2, usize::MAX, PixelChannels::Rgb),
        Err(Error::LayoutOverflow)
    );
    assert_eq!(
        RgbView::new(&[0; 4], 1, 1, 3, PixelChannels::Rgba),
        Err(Error::InvalidStride)
    );
}

#[test]
fn mismatched_dimensions_invalid_regions_and_thresholds_are_rejected() {
    let one = [0, 0, 0];
    let two_pixels = [0; 6];
    let one_view = RgbView::new(&one, 1, 1, 3, PixelChannels::Rgb).unwrap();
    let two_view = RgbView::new(&two_pixels, 2, 1, 6, PixelChannels::Rgb).unwrap();
    assert_eq!(
        compare(one_view, two_view, [0, 0, 1, 1], 64.0),
        Err(Error::DimensionMismatch)
    );

    let same = RgbView::new(&one, 1, 1, 3, PixelChannels::Rgb).unwrap();
    for region in [
        [0, 0, 0, 1],
        [0, 0, 1, 0],
        [1, 0, 1, 1],
        [0, 1, 1, 1],
        [u32::MAX, 0, 1, 1],
        [0, u32::MAX, 1, 1],
    ] {
        assert_eq!(
            compare(same, same, region, 64.0),
            Err(Error::InvalidRegion),
            "region {region:?}"
        );
    }
    for threshold in [f64::NAN, f64::INFINITY, -1.0, 256.0] {
        assert_eq!(
            compare(same, same, [0, 0, 1, 1], threshold),
            Err(Error::InvalidThreshold)
        );
    }
    let zero = compare(same, same, [0, 0, 1, 1], 0.0).unwrap();
    close(zero.gross_cell_fraction, 1.0);
}
