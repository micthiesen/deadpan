use crate::surface::validate_dimensions;
use crate::{
    FrameMetadata, RenderError, Rgba8Frame, Rotation, source_to_working, working_to_display,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FitMode {
    /// Center the whole source; uncovered output pixels are opaque black.
    Fit,
    /// Center and crop the source to cover the output.
    Fill,
}

/// Source interpretation precedes fit/fill: pixel aspect stretches the source
/// horizontal axis, then clockwise rotation determines the displayed aspect.
#[derive(Debug, Clone, Copy)]
pub struct PictureGeometry {
    pub(crate) rectangle: [f64; 4],
    pub(crate) rotation: Rotation,
}

impl PictureGeometry {
    pub fn new(
        source: &FrameMetadata,
        width: u32,
        height: u32,
        mode: FitMode,
    ) -> Result<Self, RenderError> {
        validate_dimensions(source.width, source.height)?;
        validate_dimensions(width, height)?;
        let mut source_width = f64::from(source.width) * source.sample_aspect_ratio.as_f64();
        let mut source_height = f64::from(source.height);
        if matches!(
            source.rotation,
            Rotation::Clockwise90 | Rotation::Clockwise270
        ) {
            std::mem::swap(&mut source_width, &mut source_height);
        }
        let scales = [
            f64::from(width) / source_width,
            f64::from(height) / source_height,
        ];
        let scale = match mode {
            FitMode::Fit => scales[0].min(scales[1]),
            FitMode::Fill => scales[0].max(scales[1]),
        };
        let displayed_width = source_width * scale;
        let displayed_height = source_height * scale;
        Ok(Self {
            rectangle: [
                (f64::from(width) - displayed_width) / 2.0,
                (f64::from(height) - displayed_height) / 2.0,
                displayed_width,
                displayed_height,
            ],
            rotation: source.rotation,
        })
    }

    /// Map an output pixel center to original, unrotated normalized source UV.
    /// Returns None for black bars, using half-open display bounds.
    pub fn source_uv(&self, pixel_center: [f64; 2]) -> Option<[f64; 2]> {
        let [left, top, width, height] = self.rectangle;
        let u = (pixel_center[0] - left) / width;
        let v = (pixel_center[1] - top) / height;
        if !(0.0..1.0).contains(&u) || !(0.0..1.0).contains(&v) {
            return None;
        }
        Some(match self.rotation {
            Rotation::None => [u, v],
            Rotation::Clockwise90 => [v, 1.0 - u],
            Rotation::Clockwise180 => [1.0 - u, 1.0 - v],
            Rotation::Clockwise270 => [1.0 - v, u],
        })
    }
}

/// Independent CPU pixel reference: f64 geometry, inverse transfer before
/// bilinear interpolation, premultiplied compositing over black, then display
/// encoding. It intentionally omits GPU half-float quantization.
pub fn reference_pixel(
    frame: &Rgba8Frame,
    width: u32,
    height: u32,
    mode: FitMode,
    x: u32,
    y: u32,
) -> Result<[u8; 4], RenderError> {
    let geometry = PictureGeometry::new(frame.metadata(), width, height, mode)?;
    if x >= width || y >= height {
        return Err(RenderError::Dimensions);
    }
    let Some(uv) = geometry.source_uv([f64::from(x) + 0.5, f64::from(y) + 0.5]) else {
        return Ok([0, 0, 0, 255]);
    };
    let metadata = frame.metadata();
    let sx = uv[0] * f64::from(metadata.width) - 0.5;
    let sy = uv[1] * f64::from(metadata.height) - 0.5;
    let fx = sx - sx.floor();
    let fy = sy - sy.floor();
    let mut working = [0.0; 3];
    for (dx, wx) in [(0.0, 1.0 - fx), (1.0, fx)] {
        for (dy, wy) in [(0.0, 1.0 - fy), (1.0, fy)] {
            // Clamp-to-edge after source interpretation; dimensions are bounded.
            let px = (sx.floor() + dx).clamp(0.0, f64::from(metadata.width - 1)) as u32;
            let py = (sy.floor() + dy).clamp(0.0, f64::from(metadata.height - 1)) as u32;
            let rgba = frame.pixel(px, py).map(|value| f64::from(value) / 255.0);
            let rgb = source_to_working([rgba[0], rgba[1], rgba[2]], metadata.color);
            for channel in 0..3 {
                working[channel] += rgb[channel] * rgba[3] * wx * wy;
            }
        }
    }
    let rgb = working_to_display(working).map(|value| (value * 255.0).round() as u8);
    Ok([rgb[0], rgb[1], rgb[2], 255])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SampleAspectRatio, surface::tests::metadata};

    #[test]
    fn fit_fill_and_anamorphic_rotation_have_explicit_geometry() {
        let mut meta = metadata(4, 2);
        let fit = PictureGeometry::new(&meta, 4, 4, FitMode::Fit).expect("fit");
        assert_eq!(fit.rectangle, [0.0, 1.0, 4.0, 2.0]);
        assert_eq!(fit.source_uv([0.5, 0.5]), None);
        assert_eq!(fit.source_uv([0.5, 1.5]), Some([0.125, 0.25]));
        let fill = PictureGeometry::new(&meta, 4, 4, FitMode::Fill).expect("fill");
        assert_eq!(fill.rectangle, [-2.0, 0.0, 8.0, 4.0]);
        meta.sample_aspect_ratio = SampleAspectRatio::new(2, 1).expect("SAR");
        meta.rotation = Rotation::Clockwise90;
        let rotated = PictureGeometry::new(&meta, 4, 4, FitMode::Fit).expect("rotated");
        assert_eq!(rotated.rectangle, [1.5, 0.0, 1.0, 4.0]);
        assert_eq!(rotated.source_uv([2.0, 0.5]), Some([0.125, 0.5]));
    }

    #[test]
    fn every_right_angle_maps_asymmetric_corners() {
        for (rotation, expected) in [
            (Rotation::None, [0.25, 0.125]),
            (Rotation::Clockwise90, [0.125, 0.75]),
            (Rotation::Clockwise180, [0.75, 0.875]),
            (Rotation::Clockwise270, [0.875, 0.25]),
        ] {
            let mut meta = metadata(2, 2);
            meta.rotation = rotation;
            assert_eq!(
                PictureGeometry::new(&meta, 4, 4, FitMode::Fit)
                    .expect("geometry")
                    .source_uv([1.0, 0.5]),
                Some(expected)
            );
        }
    }

    #[test]
    fn interpolation_and_alpha_happen_in_linear_light() {
        let frame =
            Rgba8Frame::new(metadata(2, 1), vec![0, 0, 0, 255, 255, 255, 255, 255]).expect("frame");
        assert_eq!(
            reference_pixel(&frame, 1, 1, FitMode::Fill, 0, 0).expect("pixel"),
            [188, 188, 188, 255]
        );
        let transparent =
            Rgba8Frame::new(metadata(2, 1), vec![255, 0, 255, 0, 0, 255, 0, 255]).expect("frame");
        assert_eq!(
            reference_pixel(&transparent, 1, 1, FitMode::Fill, 0, 0).expect("pixel"),
            [0, 188, 0, 255]
        );
    }
}
