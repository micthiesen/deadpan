use super::*;

fn texture(x: usize, y: usize) -> u8 {
    let mut value = (x as u32 + 1)
        .wrapping_mul(0x9e37_79b9)
        .wrapping_add((y as u32 + 1).wrapping_mul(0x85eb_ca6b));
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb_352d);
    value ^= value >> 15;
    32 + (value % 160) as u8
}

fn rgba(width: usize, height: usize, sample: impl Fn(usize, usize) -> u8) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        for x in 0..width {
            let value = sample(x, y);
            bytes.extend_from_slice(&[value, value, value, 255]);
        }
    }
    bytes
}

fn grid(sample: impl Fn(usize, usize) -> u8) -> LumaGrid {
    LumaGrid::from_rgba(
        &rgba(GRID_WIDTH, GRID_HEIGHT, sample),
        GRID_WIDTH as u32,
        GRID_HEIGHT as u32,
        GRID_WIDTH * 4,
    )
    .unwrap()
}

fn translated(dx: i32, dy: i32) -> LumaGrid {
    grid(|x, y| {
        let source_x = x as i32 - dx;
        let source_y = y as i32 - dy;
        if (0..GRID_WIDTH as i32).contains(&source_x) && (0..GRID_HEIGHT as i32).contains(&source_y)
        {
            texture(source_x as usize, source_y as usize)
        } else {
            112
        }
    })
}

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-10, "{actual} != {expected}");
}

#[test]
fn an_identical_textured_picture_has_confident_zero_motion() {
    let picture = grid(texture);
    let measured = compare(&picture, &picture);
    close(measured.mean_luma_shift, 0.0);
    close(measured.mean_absolute_luma_change, 0.0);
    close(measured.lighting_agreement_fraction, 1.0);
    assert_eq!(measured.motion.total_blocks, 36);
    assert_eq!(measured.motion.textured_blocks, 36);
    assert_eq!(measured.motion.matched_blocks, 36);
    close(measured.motion.coverage, 1.0);
    assert_eq!(
        measured.motion.displacement,
        Some(MotionDisplacement {
            maximum: 0.0,
            p95: 0.0
        })
    );
}

#[test]
fn translated_texture_measures_displacement_in_picture_coordinates() {
    let original = grid(texture);
    for (dx, dy) in [(2, 0), (0, -2), (-3, 1), (1, 3)] {
        let changed = translated(dx, dy);
        let measured = compare(&original, &changed);
        let expected =
            (f64::from(dx) / GRID_WIDTH as f64).hypot(f64::from(dy) / GRID_HEIGHT as f64);
        assert_eq!(measured.motion.matched_blocks, 36, "{dx}, {dy}");
        let displacement = measured.motion.displacement.unwrap();
        close(displacement.maximum, expected);
        close(displacement.p95, expected);
        assert!(measured.mean_absolute_luma_change > 20.0);
        assert!(measured.mean_luma_shift.abs() < 10.0);
    }
}

#[test]
fn a_uniform_lighting_step_does_not_become_motion() {
    let original = grid(texture);
    let brighter = grid(|x, y| texture(x, y) + 20);
    let measured = compare(&original, &brighter);
    close(measured.mean_luma_shift, 20.0);
    close(measured.mean_absolute_luma_change, 20.0);
    close(measured.lighting_agreement_fraction, 1.0);
    assert_eq!(measured.motion.matched_blocks, 36);
    close(measured.motion.displacement.unwrap().maximum, 0.0);

    let moved = compare(&original, &translated(2, 0));
    assert!(moved.motion.displacement.unwrap().p95 > 0.0);
    assert!(moved.lighting_agreement_fraction < measured.lighting_agreement_fraction);
}

#[test]
fn flash_entry_and_return_remain_visible_as_opposite_steps() {
    let original = grid(texture);
    let flash = grid(|x, y| texture(x, y) + 30);
    let entry = compare(&original, &flash);
    let exit = compare(&flash, &original);
    close(entry.mean_luma_shift, 30.0);
    close(exit.mean_luma_shift, -30.0);
    close(entry.lighting_agreement_fraction, 1.0);
    close(exit.lighting_agreement_fraction, 1.0);
    close(entry.motion.displacement.unwrap().p95, 0.0);
    close(exit.motion.displacement.unwrap().p95, 0.0);
}

#[test]
fn clipped_flash_and_a_distinct_region_do_not_hide_coherent_lighting() {
    let original = grid(|x, y| if y < 5 { 200 } else { texture(x, y) });
    let flash = grid(|_, _| 255);
    let measured = compare(&original, &flash);
    assert!(measured.mean_luma_shift > 100.0);
    assert!(measured.lighting_agreement_fraction > 0.8);
    let reversed = compare(&flash, &original);
    close(reversed.mean_luma_shift, -measured.mean_luma_shift);
    close(
        reversed.lighting_agreement_fraction,
        measured.lighting_agreement_fraction,
    );

    let plain = grid(|_, _| 100);
    let small_bright_region = grid(|_, y| if y < 4 { 255 } else { 100 });
    let local = compare(&plain, &small_bright_region);
    assert!(local.mean_luma_shift < 20.0);
    assert!(local.lighting_agreement_fraction < 0.12);
}

#[test]
fn uniform_and_ambiguous_pictures_have_unavailable_motion() {
    let uniform = grid(|_, _| 100);
    let brighter = grid(|_, _| 140);
    for after in [&uniform, &brighter] {
        let measured = compare(&uniform, after);
        assert_eq!(measured.motion.textured_blocks, 0);
        assert_eq!(measured.motion.matched_blocks, 0);
        close(measured.motion.coverage, 0.0);
        assert_eq!(measured.motion.displacement, None);
    }
    let repeated = grid(|x, y| if (x + y) % 2 == 0 { 32 } else { 192 });
    let ambiguous = compare(&repeated, &repeated);
    assert_eq!(ambiguous.motion.textured_blocks, 36);
    assert_eq!(ambiguous.motion.matched_blocks, 0);
    assert_eq!(ambiguous.motion.displacement, None);
}

#[test]
fn too_little_coverage_or_search_edge_matches_do_not_claim_measured_motion() {
    let small_patch = grid(|x, y| {
        if (5..13).contains(&x) && (5..13).contains(&y) {
            texture(x, y)
        } else {
            112
        }
    });
    let measured = compare(&small_patch, &small_patch);
    assert!(measured.motion.matched_blocks > 0);
    assert!(measured.motion.matched_blocks < 9);
    assert_eq!(measured.motion.displacement, None);

    let original = grid(texture);
    let edge = compare(&original, &translated(SEARCH_RADIUS, 0));
    assert_eq!(edge.motion.matched_blocks, 0);
    assert_eq!(edge.motion.displacement, None);
}

#[test]
fn reduction_preserves_replicated_pixels_and_ignores_alpha_and_row_padding() {
    let expected = grid(texture);
    let enlarged = rgba(GRID_WIDTH * 2, GRID_HEIGHT * 2, |x, y| {
        texture(x / 2, y / 2)
    });
    let actual = LumaGrid::from_rgba(
        &enlarged,
        (GRID_WIDTH * 2) as u32,
        (GRID_HEIGHT * 2) as u32,
        GRID_WIDTH * 8,
    )
    .unwrap();
    assert_eq!(actual, expected);

    let row_bytes = GRID_WIDTH * 4;
    let stride = row_bytes + 11;
    let mut padded = vec![233; stride * (GRID_HEIGHT - 1) + row_bytes];
    let source = rgba(GRID_WIDTH, GRID_HEIGHT, texture);
    for y in 0..GRID_HEIGHT {
        padded[y * stride..y * stride + row_bytes]
            .copy_from_slice(&source[y * row_bytes..(y + 1) * row_bytes]);
        for x in 0..GRID_WIDTH {
            padded[y * stride + x * 4 + 3] = (x + y) as u8;
        }
    }
    assert_eq!(
        LumaGrid::from_rgba(&padded, GRID_WIDTH as u32, GRID_HEIGHT as u32, stride).unwrap(),
        expected
    );
}

#[test]
fn input_shape_limits_and_arithmetic_are_checked_before_reading_pixels() {
    assert_eq!(
        LumaGrid::from_rgba(&[], 0, 1, 4),
        Err(QualityInputError::EmptyPicture)
    );
    assert_eq!(
        LumaGrid::from_rgba(&[], 1, 0, 4),
        Err(QualityInputError::EmptyPicture)
    );
    assert_eq!(
        LumaGrid::from_rgba(&[], 4097, 4096, 4097 * 4),
        Err(QualityInputError::PixelLimit)
    );
    assert_eq!(
        LumaGrid::from_rgba(&[], u32::MAX, u32::MAX, 0),
        Err(QualityInputError::PixelLimit)
    );
    assert_eq!(
        LumaGrid::from_rgba(&[], 4096, 4096, 4096 * 4),
        Err(QualityInputError::BufferTooShort)
    );
    assert_eq!(
        LumaGrid::from_rgba(&[0; 4], 1, 1, 3),
        Err(QualityInputError::InvalidStride)
    );
    assert_eq!(
        LumaGrid::from_rgba(&[0; 3], 1, 1, 4),
        Err(QualityInputError::BufferTooShort)
    );
    assert_eq!(
        LumaGrid::from_rgba(&[0; 4], 1, 2, usize::MAX),
        Err(QualityInputError::LayoutOverflow)
    );
    let tiny = LumaGrid::from_rgba(&[128, 128, 128, 0], 1, 1, usize::MAX).unwrap();
    assert!(tiny.cells().iter().all(|value| *value == 128));
    assert_eq!(compare(&tiny, &tiny).motion.displacement, None);
}
