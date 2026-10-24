//! Exact target-point projection through the renderer's retained canvas order.

use deadpan_core::{CapturedFit, CapturedFraming, ExactRatio, FramingPose};

pub(super) fn visible(point: [ExactRatio; 2]) -> bool {
    point.into_iter().all(|value| {
        !value.compare(ExactRatio::ZERO).is_lt() && !value.compare(ExactRatio::ONE).is_gt()
    })
}

fn fit(
    point: [ExactRatio; 2],
    aspect: ExactRatio,
    canvas: [u32; 2],
    mode: CapturedFit,
) -> Option<[ExactRatio; 2]> {
    if canvas.contains(&0) || !aspect.compare(ExactRatio::ZERO).is_gt() {
        return None;
    }
    let target = ExactRatio::new(i128::from(canvas[0]), i128::from(canvas[1])).ok()?;
    let one = ExactRatio::ONE;
    let covered = match (aspect.compare(target), mode) {
        (std::cmp::Ordering::Greater, CapturedFit::Fit) => [one, target.checked_div(aspect).ok()?],
        (std::cmp::Ordering::Less, CapturedFit::Fit) => [aspect.checked_div(target).ok()?, one],
        (std::cmp::Ordering::Greater, CapturedFit::Fill) => [aspect.checked_div(target).ok()?, one],
        (std::cmp::Ordering::Less, CapturedFit::Fill) => [one, target.checked_div(aspect).ok()?],
        (std::cmp::Ordering::Equal, _) => [one, one],
    };
    let half = ExactRatio::new(1, 2).expect("constant ratio");
    let project = |value: ExactRatio, coverage| {
        value
            .checked_sub(half)
            .and_then(|v| v.checked_mul(coverage))
            .and_then(|v| v.checked_add(half))
            .ok()
    };
    Some([
        project(point[0], covered[0])?,
        project(point[1], covered[1])?,
    ])
}

fn pose(point: [ExactRatio; 2], pose: FramingPose) -> Option<[ExactRatio; 2]> {
    let half = ExactRatio::new(1, 2).expect("constant ratio");
    let axis = |value: ExactRatio, center| {
        value
            .checked_sub(center)
            .and_then(|v| v.checked_mul(pose.scale))
            .and_then(|v| v.checked_add(half))
            .ok()
    };
    Some([
        axis(point[0], pose.center_x)?,
        axis(point[1], pose.center_y)?,
    ])
}

pub(super) fn project(
    mut point: [ExactRatio; 2],
    mut aspect: ExactRatio,
    context: &CapturedFraming,
    canvas: [u32; 2],
) -> Option<[ExactRatio; 2]> {
    context.validate().ok()?;
    if !visible(point) {
        return None;
    }
    for stage in &context.canvases {
        let dimensions = [stage.width, stage.height];
        point = fit(point, aspect, dimensions, stage.fit)?;
        // Fitting may put the target outside a Fill canvas. The provider's
        // first pose can still bring it in before that scope's first clip.
        if stage.layers.is_empty() {
            if !visible(point) {
                return None;
            }
        } else {
            for layer in &stage.layers {
                if let Some(layer) = layer {
                    point = pose(point, *layer)?;
                }
                if !visible(point) {
                    return None;
                }
            }
        }
        // The next stage fits the complete clipped canvas, including its bars.
        aspect = ExactRatio::new(i128::from(stage.width), i128::from(stage.height)).ok()?;
    }
    // The current renderer fits the last captured canvas into the live canvas.
    let result = fit(point, aspect, canvas, CapturedFit::Fit)?;
    visible(result).then_some(result)
}
