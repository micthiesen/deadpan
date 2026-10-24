//! Independent composed-renderer geometry comparison for captured Follow.
use super::*;
use deadpan_core::{CapturedCanvas, CapturedFit, CapturedFraming, TargetPoint};
use deadpan_render::FramingLayer;

#[test]
fn captured_follow_centers_and_clips_agree_with_composed_renderer_geometry() {
    let q = |n, d| ExactRatio::new(n, d).unwrap();
    let pose = |x, scale| FramingPose::new(x, q(1, 2), scale).unwrap();
    let stage = |width, height, fit, layers| CapturedCanvas {
        width,
        height,
        fit,
        layers,
    };
    let contexts = [
        vec![stage(320, 180, CapturedFit::Fit, vec![])],
        // Fill can put a raw point outside before the first pose brings it in.
        vec![stage(
            180,
            320,
            CapturedFit::Fill,
            vec![Some(pose(q(1, 2), q(1, 2)))],
        )],
        vec![
            stage(
                320,
                180,
                CapturedFit::Fit,
                vec![Some(pose(q(2, 5), q(2, 1)))],
            ),
            stage(
                240,
                320,
                CapturedFit::Fill,
                vec![Some(pose(q(1, 2), q(1, 2))), None],
            ),
        ],
        // A later zoom-out cannot recover a point lost through an earlier clip.
        vec![
            stage(
                320,
                180,
                CapturedFit::Fit,
                vec![Some(pose(q(1, 5), q(4, 1)))],
            ),
            stage(
                640,
                360,
                CapturedFit::Fit,
                vec![Some(pose(q(1, 2), q(1, 4)))],
            ),
        ],
        vec![stage(
            320,
            180,
            CapturedFit::Fill,
            vec![None, Some(pose(q(1, 2), q(1, 2)))],
        )],
    ]
    .map(|stages| CapturedFraming::new(stages).unwrap());
    let asset = AssetId::new("original").unwrap();
    let point = SourcePoint {
        ticks: ExactRatio::integer(10),
        time_base: SourceTimeBase::new(1, 1000).unwrap(),
    };
    let mut visible = 0;
    let mut clipped = 0;
    for meta in [
        metadata(1920, 1080, [1, 1], Rotation::None),
        metadata(1080, 1920, [1, 1], Rotation::None),
        metadata(720, 480, [32, 27], Rotation::Clockwise90),
        metadata(720, 480, [32, 27], Rotation::Clockwise270),
    ] {
        for context in &contexts {
            for canvas in [[640, 360], [360, 640], [400, 400]] {
                for inner in [vec![], vec![Some(pose(q(2, 5), q(3, 2))), None]] {
                    let layers: Vec<_> = inner
                        .iter()
                        .map(|pose| match pose {
                            Some(pose) => {
                                FramingLayer::new([pose.center_x, pose.center_y], pose.scale)
                                    .unwrap()
                            }
                            None => FramingLayer::identity(),
                        })
                        .collect();
                    let geometry = PictureGeometry::composed(
                        &meta,
                        Some(context),
                        canvas,
                        canvas,
                        FitMode::Fit,
                        &layers,
                    )
                    .unwrap();
                    for center in [
                        [200000, 250000],
                        [500000, 500000],
                        [900000, 50000],
                        [680000, 500000],
                    ] {
                        let rendered = geometry
                            .source_to_canvas(center.map(|n| f64::from(n) / 1_000_000.0))
                            .unwrap();
                        for is_point in [false, true] {
                            let mut target = target(center, meta.display_aspect());
                            if is_point {
                                target.region = TargetPoint { center }.into();
                            }
                            let resolved = deadpan_plan::follow_pose_in_context(
                                &target,
                                &asset,
                                point,
                                canvas,
                                Some(context),
                                inner.iter().copied(),
                                ExactRatio::ONE,
                            );
                            assert_eq!(
                                resolved.is_some(),
                                rendered.is_some(),
                                "visibility: {meta:?} {context:?} {canvas:?} {inner:?} {center:?}"
                            );
                            if let (Some(resolved), Some(rendered)) = (resolved, rendered) {
                                for (axis, value_ratio) in [resolved.center_x, resolved.center_y]
                                    .into_iter()
                                    .enumerate()
                                {
                                    assert!(
                                        (value(value_ratio) - rendered[axis]).abs() < 1e-9,
                                        "position: {meta:?} {context:?} {canvas:?} {inner:?} {center:?}: {resolved:?} vs {rendered:?}"
                                    );
                                }
                                visible += 1;
                            } else {
                                clipped += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(
        visible > 0 && clipped > 0,
        "both projection and irreversible clipping must execute"
    );
}
