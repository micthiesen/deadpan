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

fn captured_canvas(
    width: u32,
    height: u32,
    fit: CapturedFit,
    layers: &[FramingLayer],
) -> CapturedCanvas {
    CapturedCanvas {
        width,
        height,
        fit,
        layers: layers.iter().map(|layer| layer.pose()).collect(),
    }
}

fn assert_same_sampling(left: &PictureGeometry, right: &PictureGeometry) {
    assert_eq!(left.rectangle, right.rectangle);
    assert_eq!(left.visible.0, right.visible.0);
    assert_eq!(left.coverage, right.coverage);
    assert_eq!(left.raster, right.raster);
    for y in 0..left.raster[1] {
        for x in 0..left.raster[0] {
            assert_eq!(left.pixel_uv(x, y), right.pixel_uv(x, y), "({x}, {y})");
        }
    }
}

#[test]
fn retained_same_canvas_freeze_matches_direct_framing_with_sar_and_rotation() {
    let child = layer(3, 5, ExactRatio::new(27, 20).unwrap());
    let parent = layer(2, 5, ExactRatio::new(3, 4).unwrap());
    let captured = CapturedFraming {
        canvases: vec![captured_canvas(
            32,
            18,
            CapturedFit::Fit,
            &[child, FramingLayer::identity()],
        )],
    };
    let live = [parent, FramingLayer::identity()];
    for rotation in [
        Rotation::None,
        Rotation::Clockwise90,
        Rotation::Clockwise180,
        Rotation::Clockwise270,
    ] {
        let mut source = metadata(7, 5);
        source.sample_aspect_ratio = SampleAspectRatio::new(4, 3).unwrap();
        source.rotation = rotation;
        let composed = PictureGeometry::composed(
            &source,
            Some(&captured),
            [32, 18],
            [31, 17],
            FitMode::Fit,
            &live,
        )
        .unwrap();
        let direct = PictureGeometry::framed(
            &source,
            [32, 18],
            [31, 17],
            FitMode::Fit,
            &[
                child,
                FramingLayer::identity(),
                parent,
                FramingLayer::identity(),
            ],
        )
        .unwrap();
        assert_same_sampling(&composed, &direct);
        assert_eq!(
            composed.source_to_input(0, [0.0, 0.0]).unwrap(),
            direct.source_to_input(2, [0.0, 0.0]).unwrap(),
            "Camera input indices start after retained context for {rotation:?}"
        );
        assert!(matches!(
            composed.source_to_input(2, [0.5, 0.5]),
            Err(RenderError::FramingScope)
        ));
    }
}

#[test]
fn retained_canvas_fits_old_aspect_into_current_canvas_without_resurrecting_black() {
    let captured = CapturedFraming {
        canvases: vec![captured_canvas(4, 4, CapturedFit::Fill, &[])],
    };
    let source = metadata(8, 4);
    let retained =
        PictureGeometry::composed(&source, Some(&captured), [8, 4], [8, 4], FitMode::Fit, &[])
            .unwrap();
    let direct = PictureGeometry::framed(&source, [8, 4], [8, 4], FitMode::Fill, &[]).unwrap();
    assert_eq!(retained.coverage, [2, 0, 6, 4]);
    assert_eq!(direct.coverage, [0, 0, 8, 4]);
    let frame = Rgba8Frame::new(
        metadata(8, 4),
        (0..32)
            .flat_map(|index| [u8::try_from(index * 7).unwrap(), 40, 180, 255])
            .collect(),
    )
    .unwrap();
    for x in 0..8 {
        for y in 0..4 {
            let pixel = reference_pixel_with_geometry(&frame, &retained, x, y).unwrap();
            assert_eq!(pixel[3], 255);
            if !(2..6).contains(&x) {
                assert_eq!(pixel, [0, 0, 0, 255]);
            }
        }
    }
}

#[test]
fn identity_capture_retains_letterboxing_when_the_canvas_aspect_changes() {
    let captured =
        CapturedFraming::capture(None, captured_canvas(8, 4, CapturedFit::Fit, &[])).unwrap();
    let source = metadata(4, 4);
    let retained =
        PictureGeometry::composed(&source, Some(&captured), [4, 4], [4, 4], FitMode::Fit, &[])
            .unwrap();
    let raw = PictureGeometry::new(&source, 4, 4, FitMode::Fit).unwrap();
    assert_eq!(retained.coverage, [1, 1, 3, 3]);
    assert_eq!(raw.coverage, [0, 0, 4, 4]);
    assert_eq!(retained.pixel_uv(1, 1), Some([0.25, 0.25]));
}

#[test]
fn each_canvas_places_input_before_its_first_operation_and_clip() {
    let first = captured_canvas(8, 4, CapturedFit::Fit, &[]);
    let zoom_out = layer(1, 2, ExactRatio::new(1, 2).unwrap());
    let staged = CapturedFraming {
        canvases: vec![
            first.clone(),
            captured_canvas(4, 4, CapturedFit::Fill, &[zoom_out]),
        ],
    };
    let source = metadata(8, 4);
    let retained =
        PictureGeometry::composed(&source, Some(&staged), [4, 4], [4, 4], FitMode::Fit, &[])
            .unwrap();
    // A premature clip after Fill would lose half the placed input and produce
    // [1, 1, 3, 3]. The next stage's zoom acts before that stage's first clip.
    assert_eq!(retained.coverage, [0, 1, 4, 3]);
    let live = PictureGeometry::composed(
        &source,
        Some(&CapturedFraming {
            canvases: vec![first],
        }),
        [4, 4],
        [4, 4],
        FitMode::Fill,
        &[zoom_out],
    )
    .unwrap();
    assert_same_sampling(&retained, &live);
    let direct =
        PictureGeometry::framed(&source, [4, 4], [4, 4], FitMode::Fill, &[zoom_out]).unwrap();
    assert_same_sampling(&retained, &direct);
}

#[test]
fn retained_clip_survives_zoomout_and_multiple_canvas_changes() {
    let zoom_in = layer(1, 2, ExactRatio::integer(2));
    let zoom_out = layer(1, 2, ExactRatio::new(1, 2).unwrap());
    let first = CapturedFraming {
        canvases: vec![captured_canvas(4, 4, CapturedFit::Fit, &[zoom_in])],
    };
    let live = PictureGeometry::composed(
        &metadata(4, 4),
        Some(&first),
        [4, 4],
        [4, 4],
        FitMode::Fit,
        &[zoom_out],
    )
    .unwrap();
    assert_eq!(live.coverage, [1, 1, 3, 3]);
    assert_eq!(live.source_to_input(0, [0.1, 0.5]).unwrap(), None);
    assert_eq!(
        live.source_to_input(0, [0.5, 0.5]).unwrap(),
        Some([0.5, 0.5])
    );

    let multiple = CapturedFraming {
        canvases: vec![
            captured_canvas(4, 4, CapturedFit::Fit, &[]),
            captured_canvas(8, 4, CapturedFit::Fit, &[FramingLayer::identity()]),
        ],
    };
    let changed = PictureGeometry::composed(
        &metadata(8, 4),
        Some(&multiple),
        [16, 4],
        [16, 4],
        FitMode::Fit,
        &[],
    )
    .unwrap();
    assert_eq!(changed.coverage, [6, 1, 10, 3]);
}

#[test]
fn same_canvas_merge_keeps_fill_clip_before_zoomout_and_identity_evaluation() {
    let zoom_out = layer(1, 2, ExactRatio::new(1, 2).unwrap());
    let two_stages = CapturedFraming {
        canvases: vec![
            captured_canvas(4, 4, CapturedFit::Fill, &[]),
            captured_canvas(4, 4, CapturedFit::Fit, &[zoom_out]),
        ],
    };
    // Canonicalization must materialize the first stage's implicit clip before
    // appending a nonidentity operation from the same-canvas recapture.
    let merged = CapturedFraming::capture(
        Some(&CapturedFraming {
            canvases: vec![two_stages.canvases[0].clone()],
        }),
        two_stages.canvases[1].clone(),
    )
    .unwrap();
    assert_eq!(merged.canvases[0].layers, vec![None, zoom_out.pose()]);
    let source = metadata(8, 4);
    let two = PictureGeometry::composed(
        &source,
        Some(&two_stages),
        [4, 4],
        [4, 4],
        FitMode::Fit,
        &[],
    )
    .unwrap();
    let one = PictureGeometry::composed(&source, Some(&merged), [4, 4], [4, 4], FitMode::Fit, &[])
        .unwrap();
    assert_same_sampling(&two, &one);
    assert_eq!(two.coverage, [1, 1, 3, 3]);

    let explicit_identity = CapturedFraming {
        canvases: vec![captured_canvas(
            4,
            4,
            CapturedFit::Fit,
            &[FramingLayer {
                pose: Some(FramingPose::identity()),
            }],
        )],
    };
    assert!(explicit_identity.canvases[0].layers[0].is_some());
    let identity_path = PictureGeometry::composed(
        &metadata(7, 5),
        Some(&explicit_identity),
        [4, 4],
        [4, 4],
        FitMode::Fit,
        &[],
    )
    .unwrap();
    assert!(
        identity_path
            .source_to_canvas([0.5, 0.5])
            .unwrap()
            .is_some()
    );
}

#[test]
fn retained_geometry_limits_and_cumulative_precision_fail_before_sampling() {
    let maximum_canvases = CapturedFraming {
        canvases: vec![captured_canvas(4, 4, CapturedFit::Fit, &[]); MAX_CAPTURED_CANVASES],
    };
    assert!(
        PictureGeometry::composed(
            &metadata(4, 4),
            Some(&maximum_canvases),
            [4, 4],
            [4, 4],
            FitMode::Fit,
            &[],
        )
        .is_ok()
    );
    let too_many_canvases = CapturedFraming {
        canvases: vec![captured_canvas(4, 4, CapturedFit::Fit, &[]); MAX_CAPTURED_CANVASES + 1],
    };
    assert!(matches!(
        PictureGeometry::composed(
            &metadata(4, 4),
            Some(&too_many_canvases),
            [4, 4],
            [4, 4],
            FitMode::Fit,
            &[]
        ),
        Err(RenderError::FramingLayers)
    ));
    let oversized_canvas = CapturedFraming {
        canvases: vec![captured_canvas(16_384, 4, CapturedFit::Fit, &[])],
    };
    assert!(matches!(
        PictureGeometry::composed(
            &metadata(4, 4),
            Some(&oversized_canvas),
            [4, 4],
            [4, 4],
            FitMode::Fit,
            &[]
        ),
        Err(RenderError::Dimensions)
    ));
    let too_many_scopes = CapturedFraming {
        canvases: vec![CapturedCanvas {
            width: 4,
            height: 4,
            fit: CapturedFit::Fit,
            layers: vec![None; MAX_CAPTURED_SCOPES + 1],
        }],
    };
    assert!(matches!(
        PictureGeometry::composed(
            &metadata(4, 4),
            Some(&too_many_scopes),
            [4, 4],
            [4, 4],
            FitMode::Fit,
            &[]
        ),
        Err(RenderError::FramingLayers)
    ));

    let maximum_zoom = FramingPose::new(
        ExactRatio::new(1, 2).unwrap(),
        ExactRatio::new(1, 2).unwrap(),
        ExactRatio::integer(64),
    )
    .unwrap();
    let accumulated = CapturedFraming {
        canvases: vec![CapturedCanvas {
            width: 4,
            height: 4,
            fit: CapturedFit::Fit,
            layers: vec![Some(maximum_zoom); MAX_CAPTURED_POSES],
        }],
    };
    assert!(matches!(
        PictureGeometry::composed(
            &metadata(4, 4),
            Some(&accumulated),
            [4, 4],
            [4, 4],
            FitMode::Fit,
            &[]
        ),
        Err(RenderError::FramingGeometry)
    ));
}
