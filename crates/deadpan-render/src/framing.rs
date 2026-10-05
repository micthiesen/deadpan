//! Source interpretation and ordered canonical-canvas framing. This module has
//! no plan, serialized envelope, media identity, or UI dependencies.

use deadpan_core::{CapturedCanvas, CapturedFit, CapturedFraming, ExactRatio, FramingPose};

use crate::surface::validate_dimensions;
use crate::{FitMode, FrameMetadata, PictureGeometry, RenderError, Rotation};

#[cfg(test)]
mod tests;

pub const MAX_FRAMING_LAYERS: usize = deadpan_core::MAX_FRAMING_LAYERS;
/// Structural depth counts edges, so retain the root as well as an optional
/// synthetic provider scope for a Repeat gap.
/// Every structural scope, one extra escalation scope per Repeat and the
/// provider and gap clips.
pub const MAX_FRAMING_SCOPES: usize = 2 * deadpan_core::MAX_DOCUMENT_DEPTH + 2;
/// Retained collection limits share the authored grammar. Native dimensions and
/// cumulative spatial precision have independent renderer admission checks.
pub const MAX_CAPTURED_CANVASES: usize = deadpan_core::MAX_CAPTURED_CANVASES;
pub const MAX_CAPTURED_SCOPES: usize = deadpan_core::MAX_CAPTURED_SCOPES;
pub const MAX_CAPTURED_POSES: usize = deadpan_core::MAX_CAPTURED_POSES;

/// An evaluated node operation, independent of the authored envelope grammar.
/// Identity scopes remain meaningful: the first scope clips the provider after
/// its own framing, before any ancestor transforms the resulting canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FramingLayer {
    pose: Option<FramingPose>,
}

impl FramingLayer {
    pub const fn identity() -> Self {
        Self { pose: None }
    }

    pub const fn pose(self) -> Option<FramingPose> {
        self.pose
    }

    pub fn new(center: [ExactRatio; 2], scale: ExactRatio) -> Result<Self, RenderError> {
        let pose = FramingPose {
            center_x: center[0],
            center_y: center[1],
            scale,
        };
        pose.validate()?;
        Ok(Self { pose: Some(pose) })
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Rect(pub [f64; 4]);

impl Rect {
    pub(crate) fn contains(self, point: [f64; 2], closed: bool) -> bool {
        let [x, y, width, height] = self.0;
        width > 0.0
            && height > 0.0
            && point.iter().all(|v| v.is_finite())
            && point[0] >= x
            && point[1] >= y
            && if closed {
                point[0] <= x + width && point[1] <= y + height
            } else {
                point[0] < x + width && point[1] < y + height
            }
    }

    fn intersect(self, other: Self) -> Self {
        let [x, y, width, height] = self.0;
        let [ox, oy, ow, oh] = other.0;
        let left = x.max(ox);
        let top = y.max(oy);
        Self([
            left,
            top,
            ((x + width).min(ox + ow) - left).max(0.0),
            ((y + height).min(oy + oh) - top).max(0.0),
        ])
    }

    fn transform(self, center: [f64; 2], scale: f64, canvas: [f64; 2]) -> Self {
        let [x, y, width, height] = self.0;
        Self([
            canvas[0] / 2.0 + scale * (x - canvas[0] * center[0]),
            canvas[1] / 2.0 + scale * (y - canvas[1] * center[1]),
            width * scale,
            height * scale,
        ])
    }

    fn fit_canvas(self, from: [u32; 2], to: [u32; 2], mode: FitMode) -> Result<Self, RenderError> {
        let source_canvas = from.map(f64::from);
        let target_canvas = to.map(f64::from);
        let scale = match mode {
            FitMode::Fit => {
                (target_canvas[0] / source_canvas[0]).min(target_canvas[1] / source_canvas[1])
            }
            FitMode::Fill => {
                (target_canvas[0] / source_canvas[0]).max(target_canvas[1] / source_canvas[1])
            }
        };
        let offset = [
            (target_canvas[0] - source_canvas[0] * scale) / 2.0,
            (target_canvas[1] - source_canvas[1] * scale) / 2.0,
        ];
        let [x, y, width, height] = self.0;
        let result = Self([
            offset[0] + x * scale,
            offset[1] + y * scale,
            width * scale,
            height * scale,
        ]);
        result.validate_spatial()?;
        Ok(result)
    }

    fn raster(self, scale: [f64; 2]) -> Self {
        let [x, y, width, height] = self.0;
        Self([
            x * scale[0],
            y * scale[1],
            width * scale[0],
            height * scale[1],
        ])
    }

    fn validate(self) -> Result<(), RenderError> {
        let [x, y, width, height] = self.0;
        if !self.0.iter().all(|v| v.is_finite())
            || width <= 0.0
            || height <= 0.0
            || !(x + width).is_finite()
            || !(y + height).is_finite()
            || x + width == x
            || y + height == y
        {
            return Err(RenderError::FramingGeometry);
        }
        Ok(())
    }

    fn validate_spatial(self) -> Result<(), RenderError> {
        let [x, y, width, height] = self.0;
        if !self.0.iter().all(|v| v.is_finite())
            || width < 0.0
            || height < 0.0
            || !(x + width).is_finite()
            || !(y + height).is_finite()
            || (width > 0.0 && x + width == x)
            || (height > 0.0 && y + height == y)
        {
            return Err(RenderError::FramingGeometry);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct InputGeometry {
    source: Rect,
    visible: Rect,
}

fn value(ratio: ExactRatio) -> f64 {
    ratio.numerator() as f64 / ratio.denominator() as f64
}

fn captured_fit(fit: CapturedFit) -> FitMode {
    match fit {
        CapturedFit::Fit => FitMode::Fit,
        CapturedFit::Fill => FitMode::Fill,
    }
}

fn initial_geometry(
    dimensions: [f64; 2],
    canvas: [u32; 2],
    mode: FitMode,
) -> Result<InputGeometry, RenderError> {
    validate_dimensions(canvas[0], canvas[1])?;
    if !dimensions
        .iter()
        .all(|value| value.is_finite() && *value > 0.0)
    {
        return Err(RenderError::FramingGeometry);
    }
    let c = canvas.map(f64::from);
    let k = match mode {
        FitMode::Fit => (c[0] / dimensions[0]).min(c[1] / dimensions[1]),
        FitMode::Fill => (c[0] / dimensions[0]).max(c[1] / dimensions[1]),
    };
    let base = Rect([
        (c[0] - dimensions[0] * k) / 2.0,
        (c[1] - dimensions[1] * k) / 2.0,
        dimensions[0] * k,
        dimensions[1] * k,
    ]);
    base.validate()?;
    Ok(InputGeometry {
        source: base,
        visible: base,
    })
}

fn validate_captured(context: &CapturedFraming) -> Result<(), RenderError> {
    if context.canvases.is_empty() || context.canvases.len() > MAX_CAPTURED_CANVASES {
        return Err(RenderError::FramingLayers);
    }
    let mut scopes = 0usize;
    let mut poses = 0usize;
    for canvas in &context.canvases {
        validate_dimensions(canvas.width, canvas.height)?;
        if canvas.width < 2 || canvas.height < 2 || canvas.width % 2 != 0 || canvas.height % 2 != 0
        {
            return Err(RenderError::Dimensions);
        }
        if canvas.layers.len() > MAX_CAPTURED_SCOPES - scopes {
            return Err(RenderError::FramingLayers);
        }
        scopes += canvas.layers.len();
        for pose in canvas.layers.iter().flatten() {
            poses += 1;
            if poses > MAX_CAPTURED_POSES {
                return Err(RenderError::FramingLayers);
            }
            pose.validate()?;
        }
    }
    Ok(())
}

fn fit_canvas_geometry(
    current: &mut InputGeometry,
    from: [u32; 2],
    to: [u32; 2],
    mode: FitMode,
) -> Result<(), RenderError> {
    validate_dimensions(from[0], from[1])?;
    validate_dimensions(to[0], to[1])?;
    current.source = current.source.fit_canvas(from, to, mode)?;
    current.visible = current.visible.fit_canvas(from, to, mode)?;
    current.source.validate()?;
    current.visible.validate_spatial()?;
    Ok(())
}

fn apply_captured_layers(
    current: &mut InputGeometry,
    stage: &CapturedCanvas,
    canvas: [u32; 2],
) -> Result<(), RenderError> {
    let c = canvas.map(f64::from);
    let canvas_rect = Rect([0.0, 0.0, c[0], c[1]]);
    let default = [None];
    for pose in if stage.layers.is_empty() {
        &default[..]
    } else {
        &stage.layers
    } {
        if let Some(pose) = pose {
            let center = [value(pose.center_x), value(pose.center_y)];
            let scale = value(pose.scale);
            current.source = current.source.transform(center, scale, c);
            current.visible = current.visible.transform(center, scale, c);
            current.source.validate()?;
            current.visible.validate_spatial()?;
        }
        current.visible = current.visible.intersect(canvas_rect);
        current.visible.validate_spatial()?;
    }
    Ok(())
}

impl PictureGeometry {
    /// Layers run provider to root, including identity provider scopes. Repeat
    /// gaps have no provider node: prepend a render-only identity scope. Empty
    /// layers mean one identity provider. Preview raster size never defines the
    /// authored canvas. All coverage and affine composition here use f64.
    pub fn framed(
        source: &FrameMetadata,
        canvas: [u32; 2],
        raster: [u32; 2],
        mode: FitMode,
        layers: &[FramingLayer],
    ) -> Result<Self, RenderError> {
        Self::composed(source, None, canvas, raster, mode, layers)
    }

    /// Replay bounded retained canvas geometry before the current provider-to-root
    /// scopes. A retained context is a sequence of complete, clipped canvases:
    /// Each stage fits its input and applies its first scope before clipping.
    /// Later stages use the complete previously clipped canvas as that input.
    /// Live input indices remain local to `layers` and exclude retained scopes.
    pub fn composed(
        source: &FrameMetadata,
        context: Option<&CapturedFraming>,
        canvas: [u32; 2],
        raster: [u32; 2],
        mode: FitMode,
        layers: &[FramingLayer],
    ) -> Result<Self, RenderError> {
        validate_dimensions(source.width, source.height)?;
        validate_dimensions(canvas[0], canvas[1])?;
        validate_dimensions(raster[0], raster[1])?;
        if layers.len() > MAX_FRAMING_SCOPES
            || layers.iter().filter(|layer| layer.pose.is_some()).count() > MAX_FRAMING_LAYERS
        {
            return Err(RenderError::FramingLayers);
        }
        let mut d = [
            f64::from(source.width) * source.sample_aspect_ratio.as_f64(),
            f64::from(source.height),
        ];
        if matches!(
            source.rotation,
            Rotation::Clockwise90 | Rotation::Clockwise270
        ) {
            d.swap(0, 1);
        }
        let mut current;
        if let Some(context) = context {
            validate_captured(context)?;
            let first = context
                .canvases
                .first()
                .ok_or(RenderError::FramingGeometry)?;
            let first_canvas = [first.width, first.height];
            current = initial_geometry(d, first_canvas, captured_fit(first.fit))?;
            apply_captured_layers(&mut current, first, first_canvas)?;
            let mut previous_canvas = first_canvas;
            for stage in context.canvases.iter().skip(1) {
                let stage_canvas = [stage.width, stage.height];
                fit_canvas_geometry(
                    &mut current,
                    previous_canvas,
                    stage_canvas,
                    captured_fit(stage.fit),
                )?;
                apply_captured_layers(&mut current, stage, stage_canvas)?;
                previous_canvas = stage_canvas;
            }
            fit_canvas_geometry(&mut current, previous_canvas, canvas, mode)?;
        } else {
            current = initial_geometry(d, canvas, mode)?;
        }
        let c = canvas.map(f64::from);
        let canvas_rect = Rect([0.0, 0.0, c[0], c[1]]);
        let mut inputs = Vec::with_capacity(layers.len().max(1));
        let default = [FramingLayer::identity()];
        for layer in if layers.is_empty() {
            &default[..]
        } else {
            layers
        } {
            inputs.push(current);
            if let Some(pose) = layer.pose {
                let center = [value(pose.center_x), value(pose.center_y)];
                let scale = value(pose.scale);
                current.source = current.source.transform(center, scale, c);
                current.visible = current.visible.transform(center, scale, c);
                current.source.validate()?;
                current.visible.validate_spatial()?;
            }
            current.visible = current.visible.intersect(canvas_rect);
            current.visible.validate_spatial()?;
        }
        let raster_scale = [f64::from(raster[0]) / c[0], f64::from(raster[1]) / c[1]];
        let rectangle = current.source.raster(raster_scale).0;
        let visible = current.visible.raster(raster_scale);
        let [x, y, w, h] = visible.0;
        let bound =
            |edge: f64, maximum: u32| (edge - 0.5).ceil().clamp(0.0, f64::from(maximum)) as u32;
        let coverage = [
            bound(x, raster[0]),
            bound(y, raster[1]),
            bound(x + w, raster[0]),
            bound(y + h, raster[1]),
        ];
        let result = Self {
            rectangle,
            rotation: source.rotation,
            coverage,
            visible,
            canvas,
            raster,
            inputs,
            output: current,
            source_metadata: *source,
        };
        // Admit GPU representability before any upload, even on a CPU-only call.
        result.sampling_parameters()?;
        Ok(result)
    }

    fn project(
        &self,
        input: InputGeometry,
        upright: [f64; 2],
    ) -> Result<Option<[f64; 2]>, RenderError> {
        if !upright
            .iter()
            .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
        {
            return Err(RenderError::FramingPoint);
        }
        let [x, y, width, height] = input.source.0;
        let point = [x + upright[0] * width, y + upright[1] * height];
        // Targets may name corners exactly. Pixel coverage remains half-open.
        Ok(input.visible.contains(point, true).then(|| {
            [
                point[0] / f64::from(self.canvas[0]),
                point[1] / f64::from(self.canvas[1]),
            ]
        }))
    }

    /// Normalized input-canvas position before the selected operation. None
    /// means a preceding child clip excluded this source target.
    pub fn source_to_input(
        &self,
        index: usize,
        upright: [f64; 2],
    ) -> Result<Option<[f64; 2]>, RenderError> {
        let input = *self.inputs.get(index).ok_or(RenderError::FramingScope)?;
        self.project(input, upright)
    }

    /// +1% of upright uncropped source X and Y, respectively, expressed in the
    /// selected operation's normalized input-canvas space. These are f64 spatial
    /// results; callers quantize authored edits under the core Q32 policy.
    pub fn source_steps(&self, index: usize) -> Result<[[f64; 2]; 2], RenderError> {
        let input = self.inputs.get(index).ok_or(RenderError::FramingScope)?;
        Ok([
            [input.source.0[2] / f64::from(self.canvas[0]) / 100.0, 0.0],
            [0.0, input.source.0[3] / f64::from(self.canvas[1]) / 100.0],
        ])
    }

    pub fn source_to_canvas(&self, upright: [f64; 2]) -> Result<Option<[f64; 2]>, RenderError> {
        self.project(self.output, upright)
    }

    /// Normalized output-canvas position of an upright source point, without
    /// clipping: drawing a source region whose corners leave the visible
    /// picture still needs those corners. Finite points only.
    pub fn source_to_canvas_unclipped(&self, upright: [f64; 2]) -> Result<[f64; 2], RenderError> {
        if !upright.iter().all(|v| v.is_finite()) {
            return Err(RenderError::FramingPoint);
        }
        let [x, y, width, height] = self.output.source.0;
        Ok([
            (x + upright[0] * width) / f64::from(self.canvas[0]),
            (y + upright[1] * height) / f64::from(self.canvas[1]),
        ])
    }

    /// The upright source point shown at a normalized output-canvas position,
    /// possibly outside `[0, 1]` where the canvas shows no picture.
    pub fn canvas_to_source(&self, canvas: [f64; 2]) -> Result<[f64; 2], RenderError> {
        let [x, y, width, height] = self.output.source.0;
        let point = [
            (canvas[0] * f64::from(self.canvas[0]) - x) / width,
            (canvas[1] * f64::from(self.canvas[1]) - y) / height,
        ];
        if point.iter().all(|v| v.is_finite()) {
            Ok(point)
        } else {
            Err(RenderError::FramingPoint)
        }
    }

    /// Inverse UV is anchored at the first covered pixel center, avoiding large
    /// translated rectangles in shader arithmetic. No per-pixel snapping occurs.
    pub(crate) fn sampling_parameters(&self) -> Result<[[f32; 4]; 2], RenderError> {
        let [left, top, right, bottom] = self.coverage;
        if left >= right || top >= bottom {
            return Ok([[0.0; 4]; 2]);
        }
        let origin = self.unclipped_uv([f64::from(left) + 0.5, f64::from(top) + 0.5]);
        let dx = 1.0 / self.rectangle[2];
        let dy = 1.0 / self.rectangle[3];
        let (horizontal, vertical) = match self.rotation {
            Rotation::None => ([dx, 0.0], [0.0, dy]),
            Rotation::Clockwise90 => ([0.0, -dx], [dy, 0.0]),
            Rotation::Clockwise180 => ([-dx, 0.0], [0.0, -dy]),
            Rotation::Clockwise270 => ([0.0, dx], [-dy, 0.0]),
        };
        let values = [
            [origin[0], origin[1], horizontal[0], horizontal[1]],
            [vertical[0], vertical[1], 0.0, 0.0],
        ];
        if values.iter().flatten().any(|v| {
            !v.is_finite()
                || !(*v as f32).is_finite()
                || (*v != 0.0 && ((*v as f32) == 0.0 || (*v as f32).is_subnormal()))
        }) {
            return Err(RenderError::FramingGeometry);
        }
        Ok(values.map(|row| row.map(|v| v as f32)))
    }
}
