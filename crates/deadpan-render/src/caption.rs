//! Caption text over the composed picture. Glyph outlines come from the
//! bundled Inter variable font (SIL Open Font License; see
//! `assets/brand/source/Inter-OFL.txt`) at a fixed bold instance; coverage is
//! rasterized here on the CPU with exact signed-area accumulation and a fixed
//! evaluation order, so preview and export raster identical coverage for the
//! same caption, canvas and target. The shared GPU pass then composites it in
//! linear working light: a dark outline under a white fill.
//!
//! Layout is one centered line per caption, scaled to the canvas: no wrapping,
//! kerning or complex-script shaping. Characters the font lacks draw its
//! missing-glyph box rather than disappearing.

use deadpan_core::CaptionPlacement;
use skrifa::{
    FontRef, GlyphId, MetadataProvider,
    instance::{Location, LocationRef, Size},
    outline::{DrawSettings, OutlinePen},
};

use crate::{RenderError, surface::validate_dimensions};

/// Identity of the caption face and style, part of what a caption looks like.
pub const CAPTION_STYLE_ID: &str = "inter-4.001-wght700-6pct-white-on-dark-outline-v1";

static FONT: &[u8] = include_bytes!("../../../assets/brand/source/Inter-Variable.ttf");

/// Text height as a fraction of the canvas height.
const SIZE_OF_HEIGHT: f32 = 0.06;
/// Distance from the canvas edge for top and bottom placements.
const MARGIN_OF_HEIGHT: f32 = 0.06;
/// A line never exceeds this fraction of the canvas width; longer text shrinks.
const MAX_WIDTH: f32 = 0.9;
/// Outline radius as a fraction of the text size.
const OUTLINE_OF_SIZE: f32 = 0.08;

/// One caption line to draw.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CaptionLine {
    pub text: String,
    pub placement: CaptionPlacement,
}

/// Fill and outline coverage at the exact render-target raster, packed as
/// RGBA8 rows (red is fill, green is outline). Empty pixels are zero.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptionOverlay {
    width: u32,
    height: u32,
    bytes: Vec<u8>,
    /// Identity of the inputs it was drawn from, so a renderer can keep an
    /// already uploaded copy instead of uploading the same pixels again.
    identity: u64,
}

impl CaptionOverlay {
    /// Raster `lines` for a `canvas` shown fitted (letterboxed) inside a
    /// `target` raster. `None` when there is nothing to draw.
    pub fn rasterize(
        lines: &[CaptionLine],
        canvas: [u32; 2],
        target: [u32; 2],
    ) -> Result<Option<Self>, RenderError> {
        if lines.is_empty() {
            return Ok(None);
        }
        validate_dimensions(canvas[0], canvas[1])?;
        validate_dimensions(target[0], target[1])?;
        let font = FontRef::new(FONT).map_err(|_| RenderError::Caption("bundled font"))?;
        let location = font.axes().location([("wght", 700.0_f32)]);
        let [width, height] = target;
        let mut fill = vec![0.0_f32; width as usize * height as usize];
        let mut outline = vec![0.0_f32; fill.len()];
        // The canvas fitted into the target, centered.
        let scale = (target[0] as f32 / canvas[0] as f32).min(target[1] as f32 / canvas[1] as f32);
        let shown = [canvas[0] as f32 * scale, canvas[1] as f32 * scale];
        let origin = [
            (target[0] as f32 - shown[0]) / 2.0,
            (target[1] as f32 - shown[1]) / 2.0,
        ];
        for line in lines {
            draw_line(
                &font,
                &location,
                line,
                origin,
                shown,
                [width, height],
                &mut fill,
                &mut outline,
            )?;
        }
        if fill.iter().chain(&outline).all(|value| *value == 0.0) {
            return Ok(None);
        }
        let identity = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            (CAPTION_STYLE_ID, lines, canvas, target).hash(&mut hasher);
            hasher.finish()
        };
        let quantize = |value: f32| (value.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        let bytes = fill
            .iter()
            .zip(&outline)
            .flat_map(|(fill, outline)| [quantize(*fill), quantize(*outline), 0, 255])
            .collect();
        Ok(Some(Self {
            width,
            height,
            bytes,
            identity,
        }))
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    /// Equal for overlays drawn from the same lines, canvas and target.
    pub fn identity(&self) -> u64 {
        self.identity
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// RGBA8 rows of `width * 4` bytes: red fill, green outline coverage.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Fill and outline coverage at one pixel, each in 0..=255.
    pub fn coverage(&self, x: u32, y: u32) -> (u8, u8) {
        let at = (y as usize * self.width as usize + x as usize) * 4;
        (self.bytes[at], self.bytes[at + 1])
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_line(
    font: &FontRef<'_>,
    location: &Location,
    line: &CaptionLine,
    origin: [f32; 2],
    shown: [f32; 2],
    target: [u32; 2],
    fill: &mut [f32],
    outline: &mut [f32],
) -> Result<(), RenderError> {
    let location = LocationRef::from(location);
    let charmap = font.charmap();
    let glyphs: Vec<GlyphId> = line
        .text
        .chars()
        .map(|character| charmap.map(character).unwrap_or(GlyphId::NOTDEF))
        .collect();
    let width_at = |size: f32| -> f32 {
        let metrics = font.glyph_metrics(Size::new(size), location);
        glyphs
            .iter()
            .map(|glyph| metrics.advance_width(*glyph).unwrap_or(0.0))
            .sum()
    };
    let mut size = shown[1] * SIZE_OF_HEIGHT;
    let natural = width_at(size);
    if natural > shown[0] * MAX_WIDTH && natural > 0.0 {
        size *= shown[0] * MAX_WIDTH / natural;
    }
    if !(size.is_finite() && size >= 1.0) {
        return Ok(());
    }
    let metrics = font.metrics(Size::new(size), location);
    let glyph_metrics = font.glyph_metrics(Size::new(size), location);
    let line_width = width_at(size);
    let (ascent, descent) = (metrics.ascent, metrics.descent);
    let margin = shown[1] * MARGIN_OF_HEIGHT;
    let baseline = match line.placement {
        CaptionPlacement::Bottom => origin[1] + shown[1] - margin + descent,
        CaptionPlacement::Top => origin[1] + margin + ascent,
        CaptionPlacement::Center => origin[1] + shown[1] / 2.0 + (ascent + descent) / 2.0,
    };
    let left = origin[0] + (shown[0] - line_width) / 2.0;
    let radius = (size * OUTLINE_OF_SIZE).round().max(1.0) as usize;
    // A private box around the line keeps every outline point inside the
    // accumulation buffer; the result is clipped to the target afterwards.
    let bounds = metrics.bounds.unwrap_or(skrifa::metrics::BoundingBox {
        x_min: 0.0,
        y_min: descent,
        x_max: size,
        y_max: ascent,
    });
    let pad = radius as f32 + size * 0.25 + 2.0;
    let box_x = (left + bounds.x_min.min(0.0) - pad).floor();
    let box_y = (baseline - bounds.y_max.max(ascent) - pad).floor();
    let box_width = (line_width + (bounds.x_max - size).max(0.0) - bounds.x_min.min(0.0)
        + 2.0 * pad)
        .ceil() as usize
        + 2;
    let box_height =
        (bounds.y_max.max(ascent) - bounds.y_min.min(descent) + 2.0 * pad).ceil() as usize + 2;
    if box_width.saturating_mul(box_height) > 64 * 1024 * 1024 {
        return Err(RenderError::Caption("caption raster exceeds its bound"));
    }
    let mut raster = Raster::new(box_width, box_height);
    let outlines = font.outline_glyphs();
    let mut pen_x = left - box_x;
    for glyph in &glyphs {
        if let Some(shape) = outlines.get(*glyph) {
            let mut pen = Pen {
                raster: &mut raster,
                offset: [pen_x, baseline - box_y],
                start: [0.0; 2],
                last: [0.0; 2],
            };
            shape
                .draw(DrawSettings::unhinted(Size::new(size), location), &mut pen)
                .map_err(|_| RenderError::Caption("glyph outline"))?;
        }
        pen_x += glyph_metrics.advance_width(*glyph).unwrap_or(0.0);
    }
    let coverage = raster.coverage();
    let dilated = dilate(&coverage, box_width, box_height, radius);
    let (target_width, target_height) = (target[0] as i64, target[1] as i64);
    for row in 0..box_height {
        let y = box_y as i64 + row as i64;
        if !(0..target_height).contains(&y) {
            continue;
        }
        for column in 0..box_width {
            let x = box_x as i64 + column as i64;
            if !(0..target_width).contains(&x) {
                continue;
            }
            let from = row * box_width + column;
            let to = y as usize * target[0] as usize + x as usize;
            fill[to] = fill[to].max(coverage[from]);
            outline[to] = outline[to].max(dilated[from]);
        }
    }
    Ok(())
}

/// Separable square maximum: each pixel takes the largest coverage within
/// `radius` pixels on both axes.
fn dilate(coverage: &[f32], width: usize, height: usize, radius: usize) -> Vec<f32> {
    let mut rows = vec![0.0_f32; coverage.len()];
    for y in 0..height {
        for x in 0..width {
            let (from, to) = (x.saturating_sub(radius), (x + radius).min(width - 1));
            rows[y * width + x] = coverage[y * width + from..=y * width + to]
                .iter()
                .fold(0.0, |peak: f32, value| peak.max(*value));
        }
    }
    let mut result = vec![0.0_f32; coverage.len()];
    for x in 0..width {
        for y in 0..height {
            let (from, to) = (y.saturating_sub(radius), (y + radius).min(height - 1));
            result[y * width + x] = (from..=to)
                .map(|row| rows[row * width + x])
                .fold(0.0, f32::max);
        }
    }
    result
}

/// Signed-area accumulation of line segments (the font-rs method): each
/// segment adds its exact area contribution per row; a running sum then gives
/// nonzero-winding coverage.
struct Raster {
    width: usize,
    height: usize,
    area: Vec<f32>,
}

impl Raster {
    fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            area: vec![0.0; width * height + 2],
        }
    }

    fn line(&mut self, from: [f32; 2], to: [f32; 2]) {
        if (from[1] - to[1]).abs() <= f32::EPSILON {
            return;
        }
        let (direction, top, bottom) = if from[1] < to[1] {
            (1.0, from, to)
        } else {
            (-1.0, to, from)
        };
        let max_x = (self.width - 2) as f32;
        let clamp = |value: f32| value.clamp(0.0, max_x);
        let slope = (bottom[0] - top[0]) / (bottom[1] - top[1]);
        let mut x = top[0];
        if top[1] < 0.0 {
            x -= top[1] * slope;
        }
        let first = top[1].max(0.0) as usize;
        let last = (bottom[1].ceil().max(0.0) as usize).min(self.height);
        for y in first..last {
            let row = y * self.width;
            let dy = ((y + 1) as f32).min(bottom[1]) - (y as f32).max(top[1]);
            let next = x + slope * dy;
            let delta = dy * direction;
            let (x0, x1) = if x < next { (x, next) } else { (next, x) };
            let (x0, x1) = (clamp(x0), clamp(x1));
            let x0_floor = x0.floor();
            let x0_index = x0_floor as usize;
            let x1_ceil = x1.ceil();
            let x1_index = x1_ceil as usize;
            if x1_index <= x0_index + 1 {
                let middle = 0.5 * (x0 + x1) - x0_floor;
                self.area[row + x0_index] += delta - delta * middle;
                self.area[row + x0_index + 1] += delta * middle;
            } else {
                let inverse = (x1 - x0).recip();
                let x0_fraction = x0 - x0_floor;
                let first_area = 0.5 * inverse * (1.0 - x0_fraction) * (1.0 - x0_fraction);
                let x1_fraction = x1 - x1_ceil + 1.0;
                let last_area = 0.5 * inverse * x1_fraction * x1_fraction;
                self.area[row + x0_index] += delta * first_area;
                if x1_index == x0_index + 2 {
                    self.area[row + x0_index + 1] += delta * (1.0 - first_area - last_area);
                } else {
                    let second = inverse * (1.5 - x0_fraction);
                    self.area[row + x0_index + 1] += delta * (second - first_area);
                    for index in x0_index + 2..x1_index - 1 {
                        self.area[row + index] += delta * inverse;
                    }
                    let before_last = second + (x1_index - x0_index - 3) as f32 * inverse;
                    self.area[row + x1_index - 1] += delta * (1.0 - before_last - last_area);
                }
                self.area[row + x1_index] += delta * last_area;
            }
            x = next;
        }
    }

    fn coverage(&self) -> Vec<f32> {
        let mut sum = 0.0_f32;
        self.area[..self.width * self.height]
            .iter()
            .map(|area| {
                sum += area;
                sum.abs().min(1.0)
            })
            .collect()
    }
}

/// Outline points in pixels (y up from the baseline) into the private box.
struct Pen<'a> {
    raster: &'a mut Raster,
    offset: [f32; 2],
    start: [f32; 2],
    last: [f32; 2],
}

impl Pen<'_> {
    fn point(&self, x: f32, y: f32) -> [f32; 2] {
        [self.offset[0] + x, self.offset[1] - y]
    }

    fn segment(&mut self, to: [f32; 2]) {
        self.raster.line(self.last, to);
        self.last = to;
    }

    /// A fixed subdivision per curve, from its control polygon length.
    fn steps(points: &[[f32; 2]]) -> usize {
        let length: f32 = points
            .windows(2)
            .map(|pair| {
                ((pair[1][0] - pair[0][0]).powi(2) + (pair[1][1] - pair[0][1]).powi(2)).sqrt()
            })
            .sum();
        ((length / 2.0).ceil() as usize).clamp(1, 32)
    }
}

impl OutlinePen for Pen<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.start = self.point(x, y);
        self.last = self.start;
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let to = self.point(x, y);
        self.segment(to);
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        let (from, control, to) = (self.last, self.point(cx0, cy0), self.point(x, y));
        let steps = Self::steps(&[from, control, to]);
        for step in 1..=steps {
            let t = step as f32 / steps as f32;
            let u = 1.0 - t;
            let point = std::array::from_fn(|axis| {
                u * u * from[axis] + 2.0 * u * t * control[axis] + t * t * to[axis]
            });
            self.segment(point);
        }
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let (from, first, second, to) = (
            self.last,
            self.point(cx0, cy0),
            self.point(cx1, cy1),
            self.point(x, y),
        );
        let steps = Self::steps(&[from, first, second, to]);
        for step in 1..=steps {
            let t = step as f32 / steps as f32;
            let u = 1.0 - t;
            let point = std::array::from_fn(|axis| {
                u * u * u * from[axis]
                    + 3.0 * u * u * t * first[axis]
                    + 3.0 * u * t * t * second[axis]
                    + t * t * t * to[axis]
            });
            self.segment(point);
        }
    }

    fn close(&mut self) {
        if self.last != self.start {
            let start = self.start;
            self.segment(start);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str, placement: CaptionPlacement) -> CaptionLine {
        CaptionLine {
            text: text.into(),
            placement,
        }
    }

    fn rows_with_ink(overlay: &CaptionOverlay) -> std::ops::Range<u32> {
        let inked: Vec<u32> = (0..overlay.height())
            .filter(|y| (0..overlay.width()).any(|x| overlay.coverage(x, *y) != (0, 0)))
            .collect();
        *inked.first().unwrap()..inked.last().unwrap() + 1
    }

    #[test]
    fn captions_raster_white_fill_inside_a_wider_outline_where_they_are_placed() {
        let overlay = CaptionOverlay::rasterize(
            &[line("Are we done?", CaptionPlacement::Bottom)],
            [1920, 1080],
            [1920, 1080],
        )
        .unwrap()
        .unwrap();
        let rows = rows_with_ink(&overlay);
        // Bottom placement: the line sits in the lowest fifth of the canvas.
        assert!(rows.start > 1080 * 4 / 5 && rows.end < 1080, "{rows:?}");
        let fill = (0..1920 * 1080)
            .filter(|index| overlay.bytes()[index * 4] > 0)
            .count();
        let outline = (0..1920 * 1080)
            .filter(|index| overlay.bytes()[index * 4 + 1] > 0)
            .count();
        assert!(fill > 5_000, "{fill}");
        assert!(outline > fill, "the outline surrounds the fill");
        // Fully covered stem pixels exist.
        assert!(overlay.bytes().chunks(4).any(|pixel| pixel[0] == 255));
        // Deterministic: the same caption rasters the same bytes.
        let again = CaptionOverlay::rasterize(
            &[line("Are we done?", CaptionPlacement::Bottom)],
            [1920, 1080],
            [1920, 1080],
        )
        .unwrap()
        .unwrap();
        assert_eq!(overlay, again);
        let top = CaptionOverlay::rasterize(
            &[line("Are we done?", CaptionPlacement::Top)],
            [1920, 1080],
            [1920, 1080],
        )
        .unwrap()
        .unwrap();
        assert!(rows_with_ink(&top).end < 1080 / 5);
    }

    #[test]
    fn captions_scale_with_the_fitted_canvas_and_shrink_long_lines() {
        // A half-size target holds the same layout at half scale.
        let full = CaptionOverlay::rasterize(
            &[line("Hello", CaptionPlacement::Center)],
            [1920, 1080],
            [1920, 1080],
        )
        .unwrap()
        .unwrap();
        let half = CaptionOverlay::rasterize(
            &[line("Hello", CaptionPlacement::Center)],
            [1920, 1080],
            [960, 540],
        )
        .unwrap()
        .unwrap();
        let (a, b) = (rows_with_ink(&full), rows_with_ink(&half));
        let center = |rows: &std::ops::Range<u32>| (rows.start + rows.end) as f32 / 2.0;
        assert!((center(&a) / 2.0 - center(&b)).abs() < 2.0, "{a:?} {b:?}");
        // A letterboxed target keeps the text inside the canvas area.
        let boxed = CaptionOverlay::rasterize(
            &[line("Hello", CaptionPlacement::Bottom)],
            [1920, 1080],
            [1920, 1440],
        )
        .unwrap()
        .unwrap();
        assert!(rows_with_ink(&boxed).end <= 180 + 1080);
        // A very long line is scaled down to fit the canvas width.
        let long = "A".repeat(120);
        let wide = CaptionOverlay::rasterize(
            &[line(&long, CaptionPlacement::Bottom)],
            [640, 360],
            [640, 360],
        )
        .unwrap()
        .unwrap();
        let columns: Vec<u32> = (0..640)
            .filter(|x| (0..360).any(|y| wide.coverage(*x, y).0 > 0))
            .collect();
        assert!(
            *columns.first().unwrap() >= 20 && *columns.last().unwrap() < 620,
            "{columns:?}"
        );
        assert_eq!(
            CaptionOverlay::rasterize(&[], [640, 360], [640, 360]).unwrap(),
            None
        );
    }

    #[test]
    fn unknown_characters_draw_the_missing_glyph_box() {
        let overlay = CaptionOverlay::rasterize(
            &[line("\u{10ffff}", CaptionPlacement::Center)],
            [640, 360],
            [640, 360],
        )
        .unwrap();
        assert!(overlay.is_some());
    }
}
