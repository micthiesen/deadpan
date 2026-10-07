//! Exact model-raster rectangles captured during boundary preparation.

use serde::{Deserialize, Serialize};

use crate::{BoundaryPicture, BridgeBoundaries};

/// A half-open rectangle in top-left-origin native raster pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RasterRectWire")]
pub struct RasterRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RasterRectWire {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl RasterRect {
    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Result<Self, &'static str> {
        let rectangle = Self {
            x,
            y,
            width,
            height,
        };
        rectangle.validate_shape()?;
        Ok(rectangle)
    }

    /// Center using the same integer floor placement as conditioning pixels.
    pub fn centered(width: u32, height: u32, native: [u32; 2]) -> Result<Self, &'static str> {
        if width > native[0] || height > native[1] {
            return Err("conditioning rectangle exceeds its native raster");
        }
        Self::new(
            (native[0] - width) / 2,
            (native[1] - height) / 2,
            width,
            height,
        )
    }

    fn validate_shape(self) -> Result<(), &'static str> {
        if self.width == 0
            || self.height == 0
            || self.x.checked_add(self.width).is_none()
            || self.y.checked_add(self.height).is_none()
        {
            return Err("conditioning rectangle is empty or overflows");
        }
        Ok(())
    }

    pub fn validate_within(self, native: [u32; 2]) -> Result<(), &'static str> {
        self.validate_shape()?;
        if self.x + self.width > native[0] || self.y + self.height > native[1] {
            return Err("conditioning rectangle exceeds its native raster");
        }
        Ok(())
    }

    fn validate_centered(self, native: [u32; 2]) -> Result<(), &'static str> {
        self.validate_within(native)?;
        if self != Self::centered(self.width, self.height, native)? {
            return Err("conditioning rectangle is not centered in its native raster");
        }
        Ok(())
    }

    fn contains(self, inner: Self) -> bool {
        // Callers first validate both rectangles against the native bounds.
        inner.x >= self.x
            && inner.y >= self.y
            && inner.x + inner.width <= self.x + self.width
            && inner.y + inner.height <= self.y + self.height
    }
}

impl TryFrom<RasterRectWire> for RasterRect {
    type Error = &'static str;

    fn try_from(wire: RasterRectWire) -> Result<Self, Self::Error> {
        Self::new(wire.x, wire.y, wire.width, wire.height)
    }
}

/// The visible crop and the actual content fitted into each prepared PNG.
/// A missing content rectangle means the boundary was authored black; black
/// pixels in an actual decoded picture still have a content rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConditioningGeometry {
    pub presentation: RasterRect,
    #[serde(deserialize_with = "deserialize_content")]
    pub left_content: Option<RasterRect>,
    #[serde(deserialize_with = "deserialize_content")]
    pub right_content: Option<RasterRect>,
}

// The fields must be present, even when their explicit value is null.
fn deserialize_content<'de, D>(deserializer: D) -> Result<Option<RasterRect>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<RasterRect>::deserialize(deserializer)
}

impl ConditioningGeometry {
    /// Validate public fields before interpreting them as pixel coordinates.
    pub fn validate(
        &self,
        native: [u32; 2],
        boundaries: &BridgeBoundaries,
    ) -> Result<(), &'static str> {
        self.presentation.validate_centered(native)?;
        for (content, boundary) in [
            (self.left_content, &boundaries.left),
            (self.right_content, &boundaries.right),
        ] {
            if content.is_none() != matches!(boundary, BoundaryPicture::AuthoredBlack { .. }) {
                return Err("conditioning content rectangle differs from its boundary kind");
            }
            if let Some(content) = content {
                content.validate_centered(native)?;
                if !self.presentation.contains(content) {
                    return Err("conditioning content exceeds its presentation rectangle");
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "conditioning_geometry_tests.rs"]
mod tests;
