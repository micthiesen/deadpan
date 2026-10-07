//! Bounded RGB differences between two generated-picture endpoints.
//!
//! This is a gross change screen, not a perceptual similarity or motion
//! measurement. Pixels are compared in encoded, full-range sRGB code values.

use thiserror::Error;

use crate::generation_quality::{GRID_CELLS, GRID_HEIGHT, GRID_WIDTH, MAX_PIXELS};

/// The declared packed color layout of an endpoint image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelChannels {
    /// Three bytes per pixel, in red, green, blue order.
    Rgb,
    /// Four bytes per pixel, in red, green, blue, alpha order. Alpha is ignored.
    Rgba,
}

impl PixelChannels {
    const fn bytes_per_pixel(self) -> usize {
        match self {
            Self::Rgb => 3,
            Self::Rgba => 4,
        }
    }
}

/// A borrowed packed RGB/RGBA8 image with a validated row layout.
///
/// The caller declares the bytes to be canonical full-range sRGB. `row_stride`
/// is measured in bytes and may include padding after each row. The view keeps
/// no copy, path, or mutable access to the supplied pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RgbView<'a> {
    bytes: &'a [u8],
    width: u32,
    height: u32,
    row_stride: usize,
    channels: PixelChannels,
}

impl<'a> RgbView<'a> {
    /// Validate a borrowed image layout before any pixel is read.
    pub fn new(
        bytes: &'a [u8],
        width: u32,
        height: u32,
        row_stride: usize,
        channels: PixelChannels,
    ) -> Result<Self, Error> {
        if width == 0 || height == 0 {
            return Err(Error::EmptyPicture);
        }
        if u64::from(width) * u64::from(height) > MAX_PIXELS {
            return Err(Error::PixelLimit);
        }

        let width = usize::try_from(width).map_err(|_| Error::LayoutOverflow)?;
        let height = usize::try_from(height).map_err(|_| Error::LayoutOverflow)?;
        let row_bytes = width
            .checked_mul(channels.bytes_per_pixel())
            .ok_or(Error::LayoutOverflow)?;
        if row_stride < row_bytes {
            return Err(Error::InvalidStride);
        }
        let required = row_stride
            .checked_mul(height - 1)
            .and_then(|preceding| preceding.checked_add(row_bytes))
            .ok_or(Error::LayoutOverflow)?;
        if bytes.len() < required {
            return Err(Error::BufferTooShort);
        }

        Ok(Self {
            bytes,
            width: u32::try_from(width).map_err(|_| Error::LayoutOverflow)?,
            height: u32::try_from(height).map_err(|_| Error::LayoutOverflow)?,
            row_stride,
            channels,
        })
    }

    fn dimensions(self) -> (u32, u32) {
        (self.width, self.height)
    }
}

/// Errors returned for invalid endpoint layouts or comparison parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum Error {
    /// An endpoint has a zero width or height.
    #[error("picture dimensions must be positive")]
    EmptyPicture,
    /// An endpoint has more than 4096 squared pixels.
    #[error("picture exceeds the 4096 squared pixel limit")]
    PixelLimit,
    /// A declared row layout cannot be represented by this process.
    #[error("picture layout overflows the address space")]
    LayoutOverflow,
    /// A row stride is shorter than its packed pixel bytes.
    #[error("row stride is shorter than the packed RGB row")]
    InvalidStride,
    /// The supplied buffer does not contain the declared rows.
    #[error("pixel buffer does not contain its declared rows")]
    BufferTooShort,
    /// Endpoint dimensions differ.
    #[error("endpoint dimensions differ")]
    DimensionMismatch,
    /// The half-open region is empty, reversed, or outside the endpoints.
    #[error("region must be positive and within the endpoint dimensions")]
    InvalidRegion,
    /// The gross-cell threshold is non-finite or outside 0 through 255.
    #[error("gross-cell threshold must be finite and in 0 through 255")]
    InvalidThreshold,
}

/// Mean RGB difference and the area affected by grossly different grid cells.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EndpointDifference {
    /// Arithmetic mean of absolute R, G, and B code-value differences for all
    /// pixels in the requested region, in the range 0 through 255.
    pub mean_absolute_rgb_difference: f64,
    /// Fraction of region pixel area in nonempty 64 by 36 cells whose mean RGB
    /// difference is at least `gross_cell_threshold`. Cells are weighted by
    /// their number of source pixels, so partial edge cells count only by area.
    pub gross_cell_fraction: f64,
}

#[derive(Clone, Copy, Default)]
struct CellDifference {
    channel_difference_sum: u64,
    pixels: u64,
}

/// Compare two same-sized images in the half-open `[x, x + width) by
/// [y, y + height)` region. Each channel's absolute difference is measured before reduction,
/// so opposite color changes cannot cancel. The region is partitioned into a
/// fixed 64 by 36 grid. Empty cells do not dilute the result; gross coverage is
/// weighted by the number of region pixels in each qualifying cell.
pub fn compare(
    left: RgbView<'_>,
    right: RgbView<'_>,
    region: [u32; 4],
    gross_cell_threshold: f64,
) -> Result<EndpointDifference, Error> {
    if left.dimensions() != right.dimensions() {
        return Err(Error::DimensionMismatch);
    }
    if !gross_cell_threshold.is_finite() || !(0.0..=255.0).contains(&gross_cell_threshold) {
        return Err(Error::InvalidThreshold);
    }
    let [x, y, region_width, region_height] = region;
    let Some(x_end) = x.checked_add(region_width) else {
        return Err(Error::InvalidRegion);
    };
    let Some(y_end) = y.checked_add(region_height) else {
        return Err(Error::InvalidRegion);
    };
    if region_width == 0 || region_height == 0 || x_end > left.width || y_end > left.height {
        return Err(Error::InvalidRegion);
    }

    let region_width = usize::try_from(region_width).map_err(|_| Error::LayoutOverflow)?;
    let region_height = usize::try_from(region_height).map_err(|_| Error::LayoutOverflow)?;
    let x = usize::try_from(x).map_err(|_| Error::LayoutOverflow)?;
    let y = usize::try_from(y).map_err(|_| Error::LayoutOverflow)?;
    let pixel_count = u64::try_from(region_width)
        .ok()
        .and_then(|width| {
            u64::try_from(region_height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .ok_or(Error::LayoutOverflow)?;

    let left_channels = left.channels.bytes_per_pixel();
    let right_channels = right.channels.bytes_per_pixel();
    let mut cells = [CellDifference::default(); GRID_CELLS];
    let mut total_channel_difference = 0_u64;

    for local_y in 0..region_height {
        let image_y = y + local_y;
        let left_row = image_y * left.row_stride;
        let right_row = image_y * right.row_stride;
        let cell_y = center_cell(local_y, region_height, GRID_HEIGHT);
        for local_x in 0..region_width {
            let image_x = x + local_x;
            let left_offset = left_row + image_x * left_channels;
            let right_offset = right_row + image_x * right_channels;
            let left_pixel = &left.bytes[left_offset..left_offset + 3];
            let right_pixel = &right.bytes[right_offset..right_offset + 3];
            let difference = u64::from(left_pixel[0].abs_diff(right_pixel[0]))
                + u64::from(left_pixel[1].abs_diff(right_pixel[1]))
                + u64::from(left_pixel[2].abs_diff(right_pixel[2]));
            total_channel_difference += difference;

            let cell_x = center_cell(local_x, region_width, GRID_WIDTH);
            let cell = &mut cells[cell_y * GRID_WIDTH + cell_x];
            cell.channel_difference_sum += difference;
            cell.pixels += 1;
        }
    }

    let gross_pixels = cells
        .iter()
        .filter(|cell| {
            cell.pixels > 0
                && cell.channel_difference_sum as f64 / (cell.pixels as f64 * 3.0)
                    >= gross_cell_threshold
        })
        .map(|cell| cell.pixels)
        .sum::<u64>();

    Ok(EndpointDifference {
        mean_absolute_rgb_difference: total_channel_difference as f64 / (pixel_count as f64 * 3.0),
        gross_cell_fraction: gross_pixels as f64 / pixel_count as f64,
    })
}

fn center_cell(position: usize, extent: usize, cells: usize) -> usize {
    // Position each pixel center in normalized region coordinates. The input
    // bound keeps this product small, and the final cell index is < `cells`.
    (2 * position + 1) * cells / (2 * extent)
}

#[cfg(test)]
mod tests;
