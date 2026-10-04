//! Shot boundaries in the Original's pictures.
//!
//! Every decoded picture is reduced to a [`PictureSignature`]: the mean luma
//! and chroma of a 32 × 18 grid of cells and a 32-bin luma histogram. Each
//! picture stores a [`PictureChange`]: the mean absolute cell difference from
//! its predecessor (which rises for cuts and fast motion), the histogram
//! distance from it (which rises for cuts but little for motion within one
//! scene), and the cell difference from the picture two before (which stays
//! low when a one-picture flash returns to its scene). The changes are
//! stored, so the boundary rule can be run again exactly on any later read.
//! Shots are proposals; they never cut.
//!
//! [`SHOT_RULE`] names the rule [`ShotAnalysis::boundaries`] applies: picture
//! `k` begins a shot when
//!
//! 1. its cell change is at least 24 (of 255) and its histogram change at least
//!    48 (of 255);
//! 2. its cell change is at least three times the mean cell change of the up
//!    to eight pictures on each side (excluding itself), so sustained motion
//!    does not read as a run of cuts; and
//! 3. picture `k` differs from picture `k - 2` by a cell change of at least
//!    24, and picture `k + 1` (when it exists) does too, so a one-picture
//!    flash returning to its scene is not a cut; and
//! 4. at least six pictures have passed since the previous boundary (or the
//!    first picture).
//!
//! Gradual transitions such as dissolves are not detected.

use thiserror::Error;

/// The versioned boundary rule implemented by [`ShotAnalysis::boundaries`].
pub const SHOT_RULE: &str = "deadpan-shots-1";
/// The measurement stored changes come from: [`PictureSignature`] and
/// [`PictureSignature::change`]. Stored analyses are keyed by it.
pub const SIGNATURE_VERSION: &str = "deadpan-picture-signature-1";
pub const GRID_COLUMNS: usize = 32;
pub const GRID_ROWS: usize = 18;
pub const HISTOGRAM_BINS: usize = 32;
/// Ten hours at 120 pictures per second.
pub const MAX_SHOT_PICTURES: usize = 10 * 60 * 60 * 120;

const MIN_CELL_CHANGE: u8 = 24;
const MIN_HISTOGRAM_CHANGE: u8 = 48;
const LOCAL_RATIO: u32 = 3;
const LOCAL_RADIUS: usize = 8;
const MIN_SHOT_PICTURES: usize = 6;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ShotError {
    #[error("shot analysis exceeds its picture limit")]
    Limit,
    #[error("shot analysis is invalid: {0}")]
    Invalid(&'static str),
}

/// Changes into one picture: `[cell, histogram, skip]`, where `cell` and
/// `histogram` compare it with the previous picture and `skip` compares its
/// cells with the picture two before (or the previous one for picture 1).
/// Picture 0 has no predecessor and stores `[0, 0, 0]`.
pub type PictureChange = [u8; 3];

/// A reduced picture: cell means and a normalized luma histogram.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PictureSignature {
    /// Luma, blue-difference and red-difference means per cell, row-major.
    cells: Vec<[u8; 3]>,
    /// Luma histogram with counts scaled to sum to 65,536 (rounded).
    histogram: [u32; HISTOGRAM_BINS],
}

impl PictureSignature {
    /// Reduce packed RGBA8 rows. Alpha is ignored. Each cell covers the pixels
    /// whose centers fall in it, so every pixel counts exactly once.
    pub fn from_rgba(
        rgba: &[u8],
        width: u32,
        height: u32,
        row_stride_bytes: usize,
    ) -> Result<Self, ShotError> {
        let (width, height) = (width as usize, height as usize);
        if width < GRID_COLUMNS || height < GRID_ROWS {
            return Err(ShotError::Invalid("picture smaller than the grid"));
        }
        let row_bytes = width
            .checked_mul(4)
            .ok_or(ShotError::Invalid("picture width"))?;
        if row_stride_bytes < row_bytes || rgba.len() < row_stride_bytes * (height - 1) + row_bytes
        {
            return Err(ShotError::Invalid("RGBA buffer smaller than the picture"));
        }
        let mut sums = vec![[0_u64; 3]; GRID_COLUMNS * GRID_ROWS];
        let mut counts = vec![0_u64; GRID_COLUMNS * GRID_ROWS];
        let mut histogram = [0_u64; HISTOGRAM_BINS];
        let column_of: Vec<usize> = (0..width).map(|x| x * GRID_COLUMNS / width).collect();
        for y in 0..height {
            let row = &rgba[y * row_stride_bytes..y * row_stride_bytes + row_bytes];
            let cell_row = y * GRID_ROWS / height * GRID_COLUMNS;
            for (x, pixel) in row.chunks_exact(4).enumerate() {
                let (r, g, b) = (
                    i32::from(pixel[0]),
                    i32::from(pixel[1]),
                    i32::from(pixel[2]),
                );
                // Integer Rec.601 full-range YCbCr; only differences matter.
                let luma = (77 * r + 150 * g + 29 * b + 128) >> 8;
                let blue = ((-43 * r - 85 * g + 128 * b + 128) >> 8) + 128;
                let red = ((128 * r - 107 * g - 21 * b + 128) >> 8) + 128;
                let cell = cell_row + column_of[x];
                sums[cell][0] += luma as u64;
                sums[cell][1] += blue.clamp(0, 255) as u64;
                sums[cell][2] += red.clamp(0, 255) as u64;
                counts[cell] += 1;
                histogram[luma.clamp(0, 255) as usize * HISTOGRAM_BINS / 256] += 1;
            }
        }
        let cells = sums
            .iter()
            .zip(&counts)
            .map(|(sum, count)| sum.map(|value| ((value + count / 2) / count) as u8))
            .collect();
        let pixels = (width * height) as u64;
        let histogram = histogram.map(|count| ((count * 65_536 + pixels / 2) / pixels) as u32);
        Ok(Self { cells, histogram })
    }

    /// The change into `next` from `self` (its predecessor) and `before`
    /// (the picture two before it, when there is one).
    pub fn change(&self, next: &Self, before: Option<&Self>) -> PictureChange {
        let [cell, histogram] = self.difference(next);
        let skip = before.map_or(cell, |before| before.cell_difference(next));
        [cell, histogram, skip]
    }

    fn cell_difference(&self, next: &Self) -> u8 {
        self.difference(next)[0]
    }

    /// Quantized cell and histogram differences from `self` to `next`.
    fn difference(&self, next: &Self) -> [u8; 2] {
        let total: u32 = self
            .cells
            .iter()
            .zip(&next.cells)
            .map(|(a, b)| {
                (0..3)
                    .map(|channel| u32::from(a[channel].abs_diff(b[channel])))
                    .sum::<u32>()
            })
            .sum();
        let samples = (self.cells.len() * 3) as u32;
        let cell = (total + samples / 2) / samples;
        // Half the L1 distance of two normalized histograms lies in 0..=1.
        let distance: u32 = self
            .histogram
            .iter()
            .zip(&next.histogram)
            .map(|(a, b)| a.abs_diff(*b))
            .sum();
        let histogram = (u64::from(distance) * 255 + 65_536) / (2 * 65_536);
        [cell.min(255) as u8, histogram.min(255) as u8]
    }
}

/// Validated stored changes, one [`PictureChange`] per picture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShotAnalysis {
    changes: Vec<PictureChange>,
}

impl ShotAnalysis {
    pub fn new(changes: Vec<PictureChange>) -> Result<Self, ShotError> {
        if changes.len() > MAX_SHOT_PICTURES {
            return Err(ShotError::Limit);
        }
        if changes.first().is_some_and(|first| *first != [0, 0, 0]) {
            return Err(ShotError::Invalid("the first picture has no predecessor"));
        }
        Ok(Self { changes })
    }

    /// Changes between consecutive signatures, in picture order. Streaming
    /// callers keep only the previous signature and push each change.
    pub fn from_signatures(signatures: &[PictureSignature]) -> Result<Self, ShotError> {
        let changes = (0..signatures.len())
            .map(|index| match index {
                0 => [0, 0, 0],
                _ => signatures[index - 1].change(
                    &signatures[index],
                    index.checked_sub(2).map(|before| &signatures[before]),
                ),
            })
            .collect();
        Self::new(changes)
    }

    pub fn pictures(&self) -> usize {
        self.changes.len()
    }

    pub fn changes(&self) -> &[PictureChange] {
        &self.changes
    }

    /// Pictures that begin a shot under [`SHOT_RULE`], in order. Picture 0
    /// always begins the first shot and is not listed.
    pub fn boundaries(&self) -> Vec<usize> {
        let mut boundaries = Vec::new();
        let mut previous = 0;
        for (picture, [cell, histogram, skip]) in self.changes.iter().enumerate().skip(1) {
            let returns_next = self
                .changes
                .get(picture + 1)
                .is_some_and(|next| next[2] < MIN_CELL_CHANGE);
            if *cell < MIN_CELL_CHANGE
                || *histogram < MIN_HISTOGRAM_CHANGE
                || *skip < MIN_CELL_CHANGE
                || returns_next
                || picture - previous < MIN_SHOT_PICTURES
            {
                continue;
            }
            let first = picture.saturating_sub(LOCAL_RADIUS).max(1);
            let last = (picture + LOCAL_RADIUS).min(self.changes.len() - 1);
            let (sum, count) = (first..=last)
                .filter(|neighbor| *neighbor != picture)
                .fold((0_u32, 0_u32), |(sum, count), neighbor| {
                    (sum + u32::from(self.changes[neighbor][0]), count + 1)
                });
            // cell ≥ 3 · mean  ⇔  cell · count ≥ 3 · sum
            if u32::from(*cell) * count.max(1) >= LOCAL_RATIO * sum {
                boundaries.push(picture);
                previous = picture;
            }
        }
        boundaries
    }
}

#[cfg(test)]
mod tests;
