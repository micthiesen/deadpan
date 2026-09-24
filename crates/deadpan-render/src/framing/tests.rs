use super::*;
use crate::{
    Rgba8Frame, SampleAspectRatio, reference_pixel_with_geometry, surface::tests::metadata,
};

fn layer(x: i128, denominator: i128, scale: ExactRatio) -> FramingLayer {
    FramingLayer::new(
        [
            ExactRatio::new(x, denominator).unwrap(),
            ExactRatio::new(1, 2).unwrap(),
        ],
        scale,
    )
    .unwrap()
}

fn geometry(layers: &[FramingLayer]) -> PictureGeometry {
    PictureGeometry::framed(&metadata(4, 4), [4, 4], [4, 4], FitMode::Fit, layers).unwrap()
}

#[test]
fn ancestor_identity_preserves_child_pan_and_selected_input_scope() {
    let g = geometry(&[layer(3, 4, ExactRatio::ONE), FramingLayer::identity()]);
    assert_eq!(g.source_uv([0.5, 0.5]), Some([0.375, 0.125]));
    assert_eq!(
        g.source_to_input(0, [0.75, 0.5]).unwrap(),
        Some([0.75, 0.5])
    );
    assert_eq!(g.source_to_input(1, [0.75, 0.5]).unwrap(), Some([0.5, 0.5]));
    assert_eq!(g.source_to_canvas([0.75, 0.5]).unwrap(), Some([0.5, 0.5]));
    assert_eq!(g.source_to_input(1, [0.1, 0.5]).unwrap(), None);
    assert_eq!(g.source_steps(1).unwrap(), [[0.01, 0.0], [0.0, 0.01]]);
}

#[test]
fn group_zoom_out_does_not_resurrect_leaf_crop() {
    let g = geometry(&[
        layer(1, 2, ExactRatio::integer(2)),
        layer(1, 2, ExactRatio::new(1, 2).unwrap()),
    ]);
    assert_eq!(g.rectangle, [0.0, 0.0, 4.0, 4.0]);
    assert_eq!(g.coverage, [1, 1, 3, 3]);
    assert_eq!(g.source_steps(1).unwrap(), [[0.02, 0.0], [0.0, 0.02]]);
    assert_eq!(g.source_to_input(1, [0.1, 0.5]).unwrap(), None);
    let frame = Rgba8Frame::new(metadata(4, 4), vec![255; 4 * 4 * 4]).unwrap();
    for y in 0..4 {
        for x in 0..4 {
            let expected = if (1..3).contains(&x) && (1..3).contains(&y) {
                [255; 4]
            } else {
                [0, 0, 0, 255]
            };
            assert_eq!(
                reference_pixel_with_geometry(&frame, &g, x, y).unwrap(),
                expected
            );
        }
    }
}

#[test]
fn provider_framing_precedes_its_first_clip_and_group_framing_follows_it() {
    let zoom = layer(1, 2, ExactRatio::new(1, 2).unwrap());
    let leaf =
        PictureGeometry::framed(&metadata(8, 4), [4, 4], [4, 4], FitMode::Fill, &[zoom]).unwrap();
    let group = PictureGeometry::framed(
        &metadata(8, 4),
        [4, 4],
        [4, 4],
        FitMode::Fill,
        &[FramingLayer::identity(), zoom],
    )
    .unwrap();
    assert_eq!(leaf.rectangle, group.rectangle);
    assert_eq!(leaf.coverage, [0, 1, 4, 3]);
    assert_eq!(group.coverage, [1, 1, 3, 3]);
    // The provider may aim at a source corner outside its initial Fill canvas.
    assert_eq!(
        leaf.source_to_input(0, [0.0, 0.0]).unwrap(),
        Some([-0.5, 0.0])
    );
    assert_eq!(group.source_to_input(1, [0.0, 0.0]).unwrap(), None);
}

#[test]
fn upright_sar_steps_and_targets_do_not_depend_on_preview_rounding() {
    let mut meta = metadata(4, 2);
    meta.sample_aspect_ratio = SampleAspectRatio::new(2, 1).unwrap();
    meta.rotation = Rotation::Clockwise90;
    for raster in [[4, 4], [129, 65], [319, 181]] {
        let g = PictureGeometry::framed(
            &meta,
            [4, 4],
            raster,
            FitMode::Fit,
            &[FramingLayer::identity()],
        )
        .unwrap();
        assert_eq!(g.source_steps(0).unwrap(), [[0.0025, 0.0], [0.0, 0.01]]);
        assert_eq!(
            g.source_to_input(0, [0.0, 0.0]).unwrap(),
            Some([0.375, 0.0])
        );
        assert_eq!(g.source_to_canvas([1.0, 1.0]).unwrap(), Some([0.625, 1.0]));
    }
}

#[test]
fn integer_coverage_obeys_half_open_pixel_center_edges_without_color_tolerance() {
    let exact = geometry(&[layer(1, 2, ExactRatio::new(1, 4).unwrap())]);
    assert_eq!(exact.coverage, [1, 1, 2, 2]);
    let half = ExactRatio::new(1, 2).unwrap();
    let tiny = ExactRatio::new(1, 1 << 32).unwrap();
    let moved = FramingLayer::new(
        [half.checked_sub(tiny).unwrap(); 2],
        ExactRatio::new(1, 4).unwrap(),
    )
    .unwrap();
    let moved = geometry(&[moved]);
    assert_eq!(moved.coverage, [2, 2, 3, 3]);
    assert!(exact.pixel_uv(1, 1).is_some());
    assert!(exact.pixel_uv(2, 2).is_none());
    assert!(moved.pixel_uv(1, 1).is_none());
    assert!(moved.pixel_uv(2, 2).is_some());
}

#[test]
fn nested_sampling_parameters_match_independent_cpu_coordinates() {
    for rotation in [
        Rotation::None,
        Rotation::Clockwise90,
        Rotation::Clockwise180,
        Rotation::Clockwise270,
    ] {
        let mut meta = metadata(7, 5);
        meta.rotation = rotation;
        meta.sample_aspect_ratio = SampleAspectRatio::new(4, 3).unwrap();
        let layers = [
            layer(3, 5, ExactRatio::new(27, 20).unwrap()),
            layer(2, 5, ExactRatio::new(3, 4).unwrap()),
        ];
        let g = PictureGeometry::framed(&meta, [16, 9], [31, 17], FitMode::Fit, &layers).unwrap();
        let p = g.sampling_parameters().unwrap();
        for y in 0..17 {
            for x in 0..31 {
                if let Some(expected) = g.pixel_uv(x, y) {
                    let dx = (x - g.coverage[0]) as f32;
                    let dy = (y - g.coverage[1]) as f32;
                    let actual = [
                        p[0][0] + dx * p[0][2] + dy * p[1][0],
                        p[0][1] + dx * p[0][3] + dy * p[1][1],
                    ];
                    for i in 0..2 {
                        assert!((f64::from(actual[i]) - expected[i]).abs() < 2e-7);
                    }
                }
            }
        }
    }
}

#[test]
fn pose_scope_and_numeric_admission_are_bounded() {
    let half = ExactRatio::new(1, 2).unwrap();
    assert!(FramingLayer::new([half; 2], ExactRatio::ZERO).is_err());
    assert!(FramingLayer::new([ExactRatio::integer(18); 2], ExactRatio::ONE).is_err());
    let maximum = PictureGeometry::framed(
        &metadata(4, 4),
        [4, 4],
        [4, 4],
        FitMode::Fit,
        &[FramingLayer::identity(); MAX_FRAMING_SCOPES],
    )
    .unwrap();
    assert_eq!(
        maximum
            .source_to_input(MAX_FRAMING_SCOPES - 1, [0.5, 0.5])
            .unwrap(),
        Some([0.5, 0.5])
    );
    assert!(matches!(
        PictureGeometry::framed(
            &metadata(4, 4),
            [4, 4],
            [4, 4],
            FitMode::Fit,
            &[FramingLayer::identity(); MAX_FRAMING_SCOPES + 1]
        ),
        Err(RenderError::FramingLayers)
    ));
    let active = layer(1, 2, ExactRatio::ONE);
    assert!(matches!(
        PictureGeometry::framed(
            &metadata(4, 4),
            [4, 4],
            [4, 4],
            FitMode::Fit,
            &[active; MAX_FRAMING_LAYERS + 1]
        ),
        Err(RenderError::FramingLayers)
    ));
    assert!(geometry(&[]).source_to_input(1, [0.5, 0.5]).is_err());
    assert!(geometry(&[]).source_to_canvas([f64::NAN, 0.5]).is_err());
    // Tiny cumulative extents cannot silently collapse on the f64 canvas grid.
    let tiny = layer(1, 2, ExactRatio::new(1, 64).unwrap());
    assert!(matches!(
        PictureGeometry::framed(
            &metadata(4, 4),
            [4, 4],
            [4, 4],
            FitMode::Fit,
            &[tiny; MAX_FRAMING_LAYERS]
        ),
        Err(RenderError::FramingGeometry)
    ));
}
