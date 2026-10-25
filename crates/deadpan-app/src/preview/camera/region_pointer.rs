//! Pointer geometry for the temporary target editor. The projection belongs to
//! one displayed picture and one painted raster. Its owner revokes a gesture
//! when that presentation changes; no gesture writes authored state.

use deadpan_core::{TARGET_UNITS, TargetGeometry, TargetPoint, TargetRegion};
use deadpan_render::PictureGeometry;
use eframe::egui::{Pos2, Rect, pos2};

use crate::navigation::camera::MIN_REGION_SIZE;

const HANDLE_RADIUS: f32 = 9.0;

#[derive(Clone, Debug)]
pub(super) struct Projection {
    painted: Rect,
    geometry: PictureGeometry,
}

impl Projection {
    /// `painted` is the actual retained texture rectangle, including any
    /// letterboxing within that texture, rather than a pending desired canvas.
    pub(super) fn new(painted: Rect, geometry: PictureGeometry) -> Option<Self> {
        (painted.min.is_finite()
            && painted.max.is_finite()
            && painted.width().is_finite()
            && painted.height().is_finite()
            && painted.width() > 0.0
            && painted.height() > 0.0)
            .then_some(Self { painted, geometry })
    }

    /// Bars and source pixels excluded by any provider or ancestor clip cannot
    /// become an edge target by clamping their inverse coordinates.
    pub(super) fn source_at(&self, pointer: Pos2) -> Option<[f64; 2]> {
        if !pointer.is_finite() || !self.painted.contains(pointer) {
            return None;
        }
        let canvas = [
            f64::from(pointer.x - self.painted.left()) / f64::from(self.painted.width()),
            f64::from(pointer.y - self.painted.top()) / f64::from(self.painted.height()),
        ];
        let source = self.geometry.canvas_to_source(canvas).ok()?;
        if !source
            .iter()
            .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
        {
            return None;
        }
        self.geometry.source_to_canvas(source).ok()??;
        Some(source)
    }

    pub(super) fn screen_point(&self, source: [f64; 2]) -> Option<Pos2> {
        self.to_screen(self.geometry.source_to_canvas(source).ok()??)
    }

    /// A rectangle may straddle the source edge. Its outline and handles still
    /// use the complete authored geometry; the caller clips their paint.
    pub(super) fn screen_region(&self, region: TargetRegion) -> Option<Rect> {
        let (center, size) = region_coordinates(region);
        let min = self.to_screen(
            self.geometry
                .source_to_canvas_unclipped([center[0] - size[0] / 2.0, center[1] - size[1] / 2.0])
                .ok()?,
        )?;
        let max = self.to_screen(
            self.geometry
                .source_to_canvas_unclipped([center[0] + size[0] / 2.0, center[1] + size[1] / 2.0])
                .ok()?,
        )?;
        Some(Rect::from_min_max(min, max))
    }

    fn to_screen(&self, canvas: [f64; 2]) -> Option<Pos2> {
        let point = pos2(
            (f64::from(self.painted.left()) + canvas[0] * f64::from(self.painted.width())) as f32,
            (f64::from(self.painted.top()) + canvas[1] * f64::from(self.painted.height())) as f32,
        );
        point.is_finite().then_some(point)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GestureKind {
    Move,
    Resize,
    Draw,
}

#[derive(Clone, Debug)]
enum Operation {
    Move {
        origin: [f64; 2],
        geometry: TargetGeometry,
    },
    Point {
        offset: [f64; 2],
    },
    Rectangle {
        fixed: [f64; 2],
        offset: [f64; 2],
        direction: [f64; 2],
        kind: GestureKind,
    },
}

#[derive(Clone, Debug)]
pub(super) struct Gesture {
    projection: Projection,
    operation: Operation,
}

impl Gesture {
    /// Called only after a primary press has been admitted by the viewer's
    /// egui response. Geometric containment alone cannot claim an obscured UI.
    pub(super) fn start(
        projection: &Projection,
        geometry: TargetGeometry,
        pointer: Pos2,
    ) -> Option<Self> {
        let origin = projection.source_at(pointer)?;
        let operation = match geometry {
            TargetGeometry::Point(point) => {
                let center = point.center.map(normalized);
                if center.iter().any(|value| *value > 1.0) {
                    return None;
                }
                let on_marker = projection
                    .screen_point(center)
                    .is_some_and(|screen| screen.distance(pointer) <= HANDLE_RADIUS);
                Operation::Point {
                    offset: if on_marker {
                        [center[0] - origin[0], center[1] - origin[1]]
                    } else {
                        [0.0; 2]
                    },
                }
            }
            TargetGeometry::Region(region) => {
                if region.center.iter().any(|value| *value > TARGET_UNITS)
                    || region
                        .size
                        .iter()
                        .any(|value| !(MIN_REGION_SIZE..=TARGET_UNITS).contains(value))
                {
                    return None;
                }
                let screen = projection.screen_region(region)?;
                let corners = [
                    screen.left_top(),
                    screen.right_top(),
                    screen.left_bottom(),
                    screen.right_bottom(),
                ];
                let corner = corners
                    .iter()
                    .enumerate()
                    .filter_map(|(index, corner)| {
                        let distance = corner.distance(pointer);
                        (distance <= HANDLE_RADIUS).then_some((index, distance))
                    })
                    .min_by(|left, right| left.1.total_cmp(&right.1));
                // Corner hit areas overlap on small targets. Keep the center
                // usable as a body move while retaining perimeter handles.
                if screen.shrink2(screen.size() * 0.25).contains(pointer) {
                    Operation::Move { origin, geometry }
                } else if let Some((index, _)) = corner {
                    let (center, size) = region_coordinates(region);
                    let opposite = [
                        center[0] + if index % 2 == 0 { 0.5 } else { -0.5 } * size[0],
                        center[1] + if index < 2 { 0.5 } else { -0.5 } * size[1],
                    ];
                    let moving = [
                        center[0] - if index % 2 == 0 { 0.5 } else { -0.5 } * size[0],
                        center[1] - if index < 2 { 0.5 } else { -0.5 } * size[1],
                    ];
                    Operation::Rectangle {
                        fixed: opposite,
                        offset: std::array::from_fn(|axis| moving[axis] - origin[axis]),
                        direction: std::array::from_fn(|axis| {
                            (moving[axis] - opposite[axis]).signum()
                        }),
                        kind: GestureKind::Resize,
                    }
                } else if screen.contains(pointer) {
                    Operation::Move { origin, geometry }
                } else {
                    Operation::Rectangle {
                        fixed: origin,
                        offset: [0.0; 2],
                        direction: [1.0; 2],
                        kind: GestureKind::Draw,
                    }
                }
            }
        };
        Some(Self {
            projection: projection.clone(),
            operation,
        })
    }

    pub(super) const fn kind(&self) -> GestureKind {
        match self.operation {
            Operation::Move { .. } | Operation::Point { .. } => GestureKind::Move,
            Operation::Rectangle { kind, .. } => kind,
        }
    }

    /// Every result starts from the captured press and initial geometry. A
    /// pointer outside visible source support retains the last draft until it
    /// returns; it never invents coordinates in a black bar or a hidden clip.
    pub(super) fn update(&self, pointer: Pos2) -> Option<TargetGeometry> {
        let at = self.projection.source_at(pointer)?;
        Some(match self.operation {
            Operation::Move { origin, geometry } => match geometry {
                TargetGeometry::Region(region) => TargetRegion {
                    center: std::array::from_fn(|axis| {
                        units(normalized(region.center[axis]) + at[axis] - origin[axis])
                    }),
                    size: region.size,
                }
                .into(),
                TargetGeometry::Point(_) => return None,
            },
            Operation::Point { offset } => TargetPoint {
                center: std::array::from_fn(|axis| units(at[axis] + offset[axis])),
            }
            .into(),
            Operation::Rectangle {
                fixed,
                offset,
                direction,
                ..
            } => {
                let moving: [f64; 2] = std::array::from_fn(|axis| at[axis] + offset[axis]);
                let extent: [f64; 2] = std::array::from_fn(|axis| {
                    (fixed[axis] - moving[axis])
                        .abs()
                        .clamp(normalized(MIN_REGION_SIZE), 1.0)
                });
                TargetRegion {
                    center: std::array::from_fn(|axis| {
                        let sign = if moving[axis] == fixed[axis] {
                            direction[axis]
                        } else {
                            (moving[axis] - fixed[axis]).signum()
                        };
                        units(fixed[axis] + sign * extent[axis] / 2.0)
                    }),
                    size: extent.map(units),
                }
                .into()
            }
        })
    }
}

fn normalized(value: u32) -> f64 {
    f64::from(value) / f64::from(TARGET_UNITS)
}

fn units(value: f64) -> u32 {
    (value * f64::from(TARGET_UNITS))
        .round()
        .clamp(0.0, f64::from(TARGET_UNITS)) as u32
}

fn region_coordinates(region: TargetRegion) -> ([f64; 2], [f64; 2]) {
    (region.center.map(normalized), region.size.map(normalized))
}

#[cfg(test)]
mod tests;
