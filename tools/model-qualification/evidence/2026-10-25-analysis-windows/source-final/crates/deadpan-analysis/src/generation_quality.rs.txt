//! Bounded picture measurements for AI candidate qualification.
//!
//! These measurements do not decide whether to reject a candidate. Motion is
//! block displacement, not a renamed picture difference. Low texture, repeated
//! patterns, poor matches and insufficient coverage leave it unavailable.

use std::fmt;

pub const GRID_WIDTH: usize = 64;
pub const GRID_HEIGHT: usize = 36;
pub const GRID_CELLS: usize = GRID_WIDTH * GRID_HEIGHT;
pub const MAX_PIXELS: u64 = 4096 * 4096;

const BLOCK_RADIUS: usize = 2;
const BLOCK_SAMPLES: usize = 25;
const SEARCH_RADIUS: i32 = 4;
const BLOCK_STEP: usize = 6;
const MARGIN: usize = BLOCK_RADIUS + SEARCH_RADIUS as usize;
const MIN_TEXTURE: f64 = 8.0;
const MAX_MATCH_ERROR: f64 = 8.0;
const MIN_UNIQUENESS: f64 = 2.0;
const MIN_MATCHED_BLOCKS: usize = 6;
const LIGHTING_TOLERANCE: f64 = 2.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualityInputError {
    EmptyPicture,
    PixelLimit,
    LayoutOverflow,
    InvalidStride,
    BufferTooShort,
}

impl fmt::Display for QualityInputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::EmptyPicture => "picture dimensions must be positive",
            Self::PixelLimit => "picture exceeds 4096 squared pixels",
            Self::LayoutOverflow => "picture layout overflows the address space",
            Self::InvalidStride => "picture stride is shorter than an RGBA row",
            Self::BufferTooShort => "picture buffer does not contain its declared rows",
        })
    }
}

impl std::error::Error for QualityInputError {}

/// Mean encoded luma on a fixed normalized grid. Alpha and row padding are
/// ignored. Compare pictures in the same declared color space.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LumaGrid {
    cells: [u8; GRID_CELLS],
}

impl LumaGrid {
    /// Reduce at most [`MAX_PIXELS`] RGBA8 pixels in one pass. Grid cells with
    /// no source pixel center (when the source is smaller than the grid) use
    /// the nearest source pixel. Work is bounded by source pixels plus cells.
    pub fn from_rgba(
        rgba: &[u8],
        width: u32,
        height: u32,
        row_stride_bytes: usize,
    ) -> Result<Self, QualityInputError> {
        if width == 0 || height == 0 {
            return Err(QualityInputError::EmptyPicture);
        }
        if u64::from(width) * u64::from(height) > MAX_PIXELS {
            return Err(QualityInputError::PixelLimit);
        }
        let width = usize::try_from(width).map_err(|_| QualityInputError::LayoutOverflow)?;
        let height = usize::try_from(height).map_err(|_| QualityInputError::LayoutOverflow)?;
        let row_bytes = width
            .checked_mul(4)
            .ok_or(QualityInputError::LayoutOverflow)?;
        if row_stride_bytes < row_bytes {
            return Err(QualityInputError::InvalidStride);
        }
        let required = row_stride_bytes
            .checked_mul(height - 1)
            .and_then(|preceding| preceding.checked_add(row_bytes))
            .ok_or(QualityInputError::LayoutOverflow)?;
        if rgba.len() < required {
            return Err(QualityInputError::BufferTooShort);
        }

        let mut sums = [0_u64; GRID_CELLS];
        let mut counts = [0_u32; GRID_CELLS];
        for y in 0..height {
            let grid_y = (2 * y + 1) * GRID_HEIGHT / (2 * height);
            let row = &rgba[y * row_stride_bytes..y * row_stride_bytes + row_bytes];
            for (x, pixel) in row.chunks_exact(4).enumerate() {
                let grid_x = (2 * x + 1) * GRID_WIDTH / (2 * width);
                let cell = grid_y * GRID_WIDTH + grid_x;
                sums[cell] += u64::from(luma(pixel));
                counts[cell] += 1;
            }
        }
        let cells = std::array::from_fn(|index| {
            let count = u64::from(counts[index]);
            if let Some(mean) = (sums[index] + count / 2).checked_div(count) {
                // A rounded mean of u8 samples is itself in 0..=255.
                mean as u8
            } else {
                let x = (2 * (index % GRID_WIDTH) + 1) * width / (2 * GRID_WIDTH);
                let y = (2 * (index / GRID_WIDTH) + 1) * height / (2 * GRID_HEIGHT);
                luma(&rgba[y * row_stride_bytes + x * 4..][..4])
            }
        });
        Ok(Self { cells })
    }

    pub fn cells(&self) -> &[u8; GRID_CELLS] {
        &self.cells
    }
}

fn luma(pixel: &[u8]) -> u8 {
    // Integer full-range Rec.601 encoded luma, matching the shot signatures.
    ((77 * u32::from(pixel[0]) + 150 * u32::from(pixel[1]) + 29 * u32::from(pixel[2]) + 128) >> 8)
        as u8
}

/// Euclidean displacement after dividing horizontal/vertical grid movement
/// by the picture's width/height. These are per comparison, not per second.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionDisplacement {
    pub maximum: f64,
    /// Nearest-rank 95th percentile of confidently matched blocks.
    pub p95: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionMeasurement {
    pub total_blocks: usize,
    pub textured_blocks: usize,
    pub matched_blocks: usize,
    /// Matched blocks divided by all tested block positions, including flat
    /// ones. This is grid coverage, not a claim about all moving objects.
    pub coverage: f64,
    /// Unavailable unless at least six blocks and one quarter of all tested
    /// positions match. A textureless frame never produces measured zero.
    pub displacement: Option<MotionDisplacement>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameChange {
    /// Mean of `after - before`, in encoded luma units (-255..=255).
    pub mean_luma_shift: f64,
    /// Mean absolute difference, in encoded luma units (0..=255).
    pub mean_absolute_luma_change: f64,
    /// Fraction of cells changing in the mean's direction by at least half
    /// its magnitude. This tolerates nonuniform exposure/clipping and a small
    /// different region without requiring every pixel to shift equally.
    /// For a mean shift within two luma units, counts near-unchanged cells.
    /// Use with the shift magnitude, never alone as a lighting verdict.
    pub lighting_agreement_fraction: f64,
    pub motion: MotionMeasurement,
}

pub fn compare(before: &LumaGrid, after: &LumaGrid) -> FrameChange {
    let differences: [i16; GRID_CELLS] =
        std::array::from_fn(|index| i16::from(after.cells[index]) - i16::from(before.cells[index]));
    let signed: i64 = differences.iter().map(|value| i64::from(*value)).sum();
    let absolute: u64 = differences
        .iter()
        .map(|value| u64::from(value.unsigned_abs()))
        .sum();
    let mean_luma_shift = signed as f64 / GRID_CELLS as f64;
    let agreeing = differences
        .iter()
        .filter(|value| {
            let change = f64::from(**value);
            if mean_luma_shift.abs() <= LIGHTING_TOLERANCE {
                change.abs() <= LIGHTING_TOLERANCE
            } else {
                change * mean_luma_shift.signum() >= mean_luma_shift.abs() * 0.5
            }
        })
        .count();
    FrameChange {
        mean_luma_shift,
        mean_absolute_luma_change: absolute as f64 / GRID_CELLS as f64,
        lighting_agreement_fraction: agreeing as f64 / GRID_CELLS as f64,
        motion: motion(before, after),
    }
}

fn block(grid: &LumaGrid, x: usize, y: usize) -> [i32; BLOCK_SAMPLES] {
    let mut values = [0; BLOCK_SAMPLES];
    for (row, grid_y) in (y - BLOCK_RADIUS..=y + BLOCK_RADIUS).enumerate() {
        for (column, grid_x) in (x - BLOCK_RADIUS..=x + BLOCK_RADIUS).enumerate() {
            values[row * 5 + column] = i32::from(grid.cells[grid_y * GRID_WIDTH + grid_x]);
        }
    }
    let sum: i32 = values.iter().sum();
    // Twenty-five times the zero-mean block, without rounding its mean.
    values.map(|value| value * BLOCK_SAMPLES as i32 - sum)
}

fn deviation(values: &[i32; BLOCK_SAMPLES]) -> f64 {
    let total: u32 = values.iter().map(|value| value.unsigned_abs()).sum();
    f64::from(total) / (BLOCK_SAMPLES * BLOCK_SAMPLES) as f64
}

fn match_error(a: &[i32; BLOCK_SAMPLES], b: &[i32; BLOCK_SAMPLES]) -> f64 {
    let total: u32 = a.iter().zip(b).map(|(a, b)| a.abs_diff(*b)).sum();
    f64::from(total) / (BLOCK_SAMPLES * BLOCK_SAMPLES) as f64
}

fn motion(before: &LumaGrid, after: &LumaGrid) -> MotionMeasurement {
    // 9 columns by 4 rows, 81 candidate locations per block, 25 samples per
    // candidate. All work and retained observations have fixed upper bounds.
    let mut distances = [0.0_f64; 36];
    let (mut total_blocks, mut textured_blocks, mut matched_blocks) = (0, 0, 0);
    for y in (MARGIN..GRID_HEIGHT - MARGIN).step_by(BLOCK_STEP) {
        for x in (MARGIN..GRID_WIDTH - MARGIN).step_by(BLOCK_STEP) {
            total_blocks += 1;
            let reference = block(before, x, y);
            if deviation(&reference) < MIN_TEXTURE {
                continue;
            }
            textured_blocks += 1;
            let (mut best, mut second) = (f64::INFINITY, f64::INFINITY);
            let mut offset = (0_i32, 0_i32);
            for dy in -SEARCH_RADIUS..=SEARCH_RADIUS {
                for dx in -SEARCH_RADIUS..=SEARCH_RADIUS {
                    // Centers have a complete block plus search margin.
                    let candidate =
                        block(after, (x as i32 + dx) as usize, (y as i32 + dy) as usize);
                    let error = match_error(&reference, &candidate);
                    if error < best {
                        second = best;
                        best = error;
                        offset = (dx, dy);
                    } else if error < second {
                        second = error;
                    }
                }
            }
            // Search-edge matches may be censored by the search bound; report
            // them as unavailable instead of understating larger motion.
            if best > MAX_MATCH_ERROR
                || second - best < MIN_UNIQUENESS
                || offset.0.abs() == SEARCH_RADIUS
                || offset.1.abs() == SEARCH_RADIUS
            {
                continue;
            }
            let dx = f64::from(offset.0) / GRID_WIDTH as f64;
            let dy = f64::from(offset.1) / GRID_HEIGHT as f64;
            distances[matched_blocks] = dx.hypot(dy);
            matched_blocks += 1;
        }
    }
    let displacement = if matched_blocks >= MIN_MATCHED_BLOCKS && matched_blocks * 4 >= total_blocks
    {
        distances[..matched_blocks].sort_unstable_by(f64::total_cmp);
        let percentile = (matched_blocks * 95).div_ceil(100) - 1;
        Some(MotionDisplacement {
            maximum: distances[matched_blocks - 1],
            p95: distances[percentile],
        })
    } else {
        None
    };
    MotionMeasurement {
        total_blocks,
        textured_blocks,
        matched_blocks,
        coverage: matched_blocks as f64 / total_blocks as f64,
        displacement,
    }
}

#[cfg(test)]
mod tests;
