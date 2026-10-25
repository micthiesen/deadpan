use super::*;
use deadpan_core::{
    CapturedCanvas, CapturedFit, CapturedFraming, ExactRatio, SourceTimeBase, SourceTimestamp,
};
use deadpan_render::{
    CleanAperture, FitMode, FrameMetadata, FramingLayer, Primaries, Rotation, SampleAspectRatio,
    SourceColor, Transfer,
};
use eframe::egui::vec2;

fn metadata(width: u32, height: u32) -> FrameMetadata {
    FrameMetadata {
        width,
        height,
        row_stride_bytes: width * 4,
        clean_aperture: None,
        sample_aspect_ratio: SampleAspectRatio::SQUARE,
        rotation: Rotation::None,
        color: SourceColor {
            transfer: Transfer::Srgb,
            primaries: Primaries::Rec709,
        },
        pts: SourceTimestamp {
            ticks: 0,
            time_base: SourceTimeBase::new(1, 30).unwrap(),
        },
    }
}

fn projection() -> Projection {
    Projection::new(
        Rect::from_min_size(pos2(10.0, 20.0), vec2(1000.0, 1000.0)),
        PictureGeometry::new(&metadata(100, 100), 100, 100, FitMode::Fit).unwrap(),
    )
    .unwrap()
}

fn screen(x: f32, y: f32) -> Pos2 {
    pos2(10.0 + 1000.0 * x, 20.0 + 1000.0 * y)
}

fn region(center: [u32; 2], size: [u32; 2]) -> TargetGeometry {
    TargetRegion { center, size }.into()
}

fn near(actual: [f64; 2], expected: [f64; 2]) {
    for axis in 0..2 {
        assert!(
            (actual[axis] - expected[axis]).abs() < 0.000_001,
            "{actual:?} != {expected:?}"
        );
    }
}

#[test]
fn projection_rejects_nonfinite_empty_and_inverted_painted_rectangles() {
    for painted in [
        Rect::from_min_max(pos2(0.0, 0.0), pos2(0.0, 10.0)),
        Rect::from_min_max(pos2(10.0, 0.0), pos2(0.0, 10.0)),
        Rect::from_min_max(pos2(f32::NAN, 0.0), pos2(10.0, 10.0)),
        Rect::from_min_max(pos2(0.0, 0.0), pos2(f32::INFINITY, 10.0)),
    ] {
        let geometry = PictureGeometry::new(&metadata(100, 100), 100, 100, FitMode::Fit).unwrap();
        assert!(Projection::new(painted, geometry).is_none());
    }
}

#[test]
fn inverse_uses_actual_painted_rectangle_and_rejects_bars_and_outside() {
    let p = Projection::new(
        Rect::from_min_size(pos2(40.0, 60.0), vec2(800.0, 800.0)),
        PictureGeometry::new(&metadata(200, 100), 100, 100, FitMode::Fit).unwrap(),
    )
    .unwrap();
    near(p.source_at(pos2(240.0, 360.0)).unwrap(), [0.25, 0.25]);
    assert_eq!(p.source_at(pos2(240.0, 100.0)), None);
    assert_eq!(p.source_at(pos2(240.0, 820.0)), None);
    assert_eq!(p.source_at(pos2(39.0, 460.0)), None);
    assert_eq!(p.source_at(pos2(f32::NAN, 460.0)), None);
    assert_eq!(p.screen_point([0.25, 0.25]), Some(pos2(240.0, 360.0)));
}

#[test]
fn clean_aperture_anamorphic_and_quarter_rotation_use_upright_source_axes() {
    for rotation in [
        Rotation::None,
        Rotation::Clockwise90,
        Rotation::Clockwise180,
        Rotation::Clockwise270,
    ] {
        let mut meta = metadata(720, 576);
        let q = ExactRatio::integer;
        meta.clean_aperture = Some(CleanAperture::new([q(8), q(6), q(704), q(560)]).unwrap());
        meta.sample_aspect_ratio = SampleAspectRatio::new(16, 15).unwrap();
        meta.rotation = rotation;
        let d = if matches!(rotation, Rotation::Clockwise90 | Rotation::Clockwise270) {
            [560.0, 704.0 * 16.0 / 15.0]
        } else {
            [704.0 * 16.0 / 15.0, 560.0]
        };
        let scale = (160.0_f64 / d[0]).min(90.0 / d[1]);
        let offset = [(160.0 - d[0] * scale) / 2.0, (90.0 - d[1] * scale) / 2.0];
        let p = Projection::new(
            Rect::from_min_size(pos2(17.0, 29.0), vec2(800.0, 450.0)),
            PictureGeometry::framed(&meta, [160, 90], [320, 180], FitMode::Fit, &[]).unwrap(),
        )
        .unwrap();
        let at = pos2(
            (17.0 + 5.0 * (offset[0] + 0.25 * d[0] * scale)) as f32,
            (29.0 + 5.0 * (offset[1] + 0.75 * d[1] * scale)) as f32,
        );
        near(p.source_at(at).unwrap(), [0.25, 0.75]);
    }
}

#[test]
fn inverse_does_not_resurrect_descendant_clips_after_ancestor_zoom_out() {
    let center = [ExactRatio::new(1, 2).unwrap(); 2];
    let layers = [
        FramingLayer::new(center, ExactRatio::integer(2)).unwrap(),
        FramingLayer::new(center, ExactRatio::new(1, 2).unwrap()).unwrap(),
    ];
    let p = Projection::new(
        Rect::from_min_size(pos2(0.0, 0.0), vec2(1000.0, 1000.0)),
        PictureGeometry::framed(
            &metadata(100, 100),
            [100, 100],
            [250, 250],
            FitMode::Fit,
            &layers,
        )
        .unwrap(),
    )
    .unwrap();
    // The net affine map is identity, but the child clipped off both quarters.
    assert_eq!(p.geometry.canvas_to_source([0.1, 0.5]).unwrap(), [0.1, 0.5]);
    assert_eq!(p.source_at(pos2(100.0, 500.0)), None);
    assert_eq!(p.screen_point([0.1, 0.5]), None);
    near(p.source_at(pos2(500.0, 500.0)).unwrap(), [0.5, 0.5]);
}

#[test]
fn inverse_preserves_retained_fill_clip_fitted_into_later_canvas() {
    let captured = CapturedFraming {
        canvases: vec![CapturedCanvas {
            width: 100,
            height: 100,
            fit: CapturedFit::Fill,
            layers: Vec::new(),
        }],
    };
    let p = Projection::new(
        Rect::from_min_size(pos2(0.0, 0.0), vec2(1000.0, 500.0)),
        PictureGeometry::composed(
            &metadata(200, 100),
            Some(&captured),
            [200, 100],
            [1000, 500],
            FitMode::Fit,
            &[],
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(p.source_at(pos2(100.0, 250.0)), None);
    near(p.source_at(pos2(375.0, 250.0)).unwrap(), [0.375, 0.5]);
    assert!(Gesture::start(&p, region([500_000; 2], [200_000; 2]), pos2(100.0, 250.0)).is_none());
}

#[test]
fn body_move_preserves_size_and_grab_offset_without_accumulating_rounding() {
    let p = projection();
    let original = region([500_000, 450_000], [400_000, 300_000]);
    let gesture = Gesture::start(&p, original, screen(0.55, 0.48)).unwrap();
    assert_eq!(gesture.kind(), GestureKind::Move);
    assert_eq!(
        gesture.update(screen(0.65, 0.58)),
        Some(region([600_000, 550_000], [400_000, 300_000]))
    );
    assert_eq!(
        gesture.update(screen(0.750_001, 0.680_001)),
        Some(region([700_001, 650_001], [400_000, 300_000]))
    );
    assert_eq!(gesture.update(screen(0.55, 0.48)), Some(original));
    assert_eq!(gesture.update(screen(1.1, 0.6)), None);
    assert_eq!(
        gesture.update(screen(0.65, 0.58)),
        Some(region([600_000, 550_000], [400_000, 300_000]))
    );
}

#[test]
fn body_move_clamps_center_but_keeps_complete_rectangle_size() {
    let p = projection();
    let gesture = Gesture::start(&p, region([100_000; 2], [800_000; 2]), screen(0.1, 0.1)).unwrap();
    assert_eq!(
        gesture.update(screen(0.0, 0.0)),
        Some(region([0; 2], [800_000; 2]))
    );
    assert_eq!(
        gesture.update(screen(1.0, 1.0)),
        Some(region([1_000_000; 2], [800_000; 2]))
    );
    let screen = p
        .screen_region(TargetRegion {
            center: [0; 2],
            size: [800_000; 2],
        })
        .unwrap();
    assert_eq!(screen.left(), -390.0);
}

#[test]
fn all_corners_resize_against_opposite_corner_and_allow_crossing() {
    let p = projection();
    let original = region([500_000; 2], [400_000; 2]);
    for (press, end, expected_center, expected_size) in [
        (
            [0.3, 0.3],
            [0.2, 0.1],
            [450_000, 400_000],
            [500_000, 600_000],
        ),
        (
            [0.7, 0.3],
            [0.8, 0.1],
            [550_000, 400_000],
            [500_000, 600_000],
        ),
        (
            [0.3, 0.7],
            [0.2, 0.9],
            [450_000, 600_000],
            [500_000, 600_000],
        ),
        (
            [0.7, 0.7],
            [0.8, 0.9],
            [550_000, 600_000],
            [500_000, 600_000],
        ),
    ] {
        let gesture = Gesture::start(&p, original, screen(press[0], press[1])).unwrap();
        assert_eq!(gesture.kind(), GestureKind::Resize);
        assert_eq!(
            gesture.update(screen(end[0], end[1])),
            Some(region(expected_center, expected_size))
        );
    }
    let gesture = Gesture::start(&p, original, screen(0.3, 0.3)).unwrap();
    assert_eq!(
        gesture.update(screen(0.9, 0.8)),
        Some(region([800_000, 750_000], [200_000, 100_000]))
    );
}

#[test]
fn corner_grab_offset_and_minimum_keep_the_opposite_corner() {
    let p = projection();
    let original = region([500_000; 2], [400_000; 2]);
    // Press six points inward from the top-left handle. Merely holding that
    // position cannot jump the target, and the captured offset persists.
    let gesture = Gesture::start(&p, original, screen(0.306, 0.3)).unwrap();
    assert_eq!(gesture.kind(), GestureKind::Resize);
    assert_eq!(gesture.update(screen(0.306, 0.3)), Some(original));
    assert_eq!(
        gesture.update(screen(0.206, 0.2)),
        Some(region([450_000; 2], [500_000; 2]))
    );
    let exact = Gesture::start(&p, original, screen(0.3, 0.3)).unwrap();
    assert_eq!(
        exact.update(screen(0.7, 0.7)),
        Some(region([695_000; 2], [MIN_REGION_SIZE; 2]))
    );
}

#[test]
fn background_draw_supports_reverse_direction_and_one_percent_minimum() {
    let p = projection();
    let gesture = Gesture::start(&p, region([500_000; 2], [200_000; 2]), screen(0.1, 0.9)).unwrap();
    assert_eq!(gesture.kind(), GestureKind::Draw);
    assert_eq!(
        gesture.update(screen(0.8, 0.2)),
        Some(region([450_000, 550_000], [700_000; 2]))
    );
    assert_eq!(
        gesture.update(screen(0.101, 0.899)),
        Some(region([105_000, 895_000], [MIN_REGION_SIZE; 2]))
    );
}

#[test]
fn minimum_rectangle_body_stays_movable_when_corner_hit_areas_overlap() {
    let p = projection();
    let original = region([500_000; 2], [MIN_REGION_SIZE; 2]);
    // Ten screen points per side: every corner is less than nine points
    // from the center, but the rectangle still has a movable body.
    let gesture = Gesture::start(&p, original, screen(0.5, 0.5)).unwrap();
    assert_eq!(gesture.kind(), GestureKind::Move);
    assert_eq!(
        gesture.update(screen(0.6, 0.7)),
        Some(region([600_000, 700_000], [MIN_REGION_SIZE; 2]))
    );
    for corner in [
        screen(0.495, 0.495),
        screen(0.505, 0.495),
        screen(0.495, 0.505),
        screen(0.505, 0.505),
    ] {
        assert_eq!(
            Gesture::start(&p, original, corner).unwrap().kind(),
            GestureKind::Resize
        );
    }
}

#[test]
fn point_moves_preserve_kind_and_marker_grab_offset() {
    let p = projection();
    let point = TargetGeometry::Point(TargetPoint {
        center: [500_000; 2],
    });
    let marker = Gesture::start(&p, point, screen(0.505, 0.5)).unwrap();
    assert_eq!(marker.kind(), GestureKind::Move);
    assert_eq!(
        marker.update(screen(0.605, 0.7)),
        Some(
            TargetPoint {
                center: [600_000, 700_000]
            }
            .into()
        )
    );
    let background = Gesture::start(&p, point, screen(0.2, 0.3)).unwrap();
    assert_eq!(
        background.update(screen(0.25, 0.4)),
        Some(
            TargetPoint {
                center: [250_000, 400_000]
            }
            .into()
        )
    );
}
