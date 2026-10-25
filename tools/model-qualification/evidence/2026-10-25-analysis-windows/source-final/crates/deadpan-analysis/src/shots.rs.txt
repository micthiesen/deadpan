//! Shot boundaries in the Original's pictures.
//!
//! Every decoded picture is reduced to a [`PictureSignature`]: the mean luma
//! and chroma of a 32 × 18 grid of cells and a 32-bin luma histogram. Each
//! picture stores a [`PictureMeasure`] ([`SIGNATURE_VERSION`]):
//!
//! - its [`PictureChange`]: the mean absolute cell difference from its
//!   predecessor (which rises for cuts and fast motion), the histogram
//!   distance from it (which rises for cuts but little for motion within one
//!   scene), and the cell difference from the picture two before (which stays
//!   low when a one-picture flash returns to its scene);
//! - its mean cell luma and the mean absolute deviation of cell luma from that
//!   mean, which identify black pictures; and
//! - for each half-span `h` of [`HALF_SPANS`], the cell difference between the
//!   pictures `h` before and `h` after it (`across`) and how far it lies from
//!   their mean (`residual`, half the mean absolute value of
//!   `2·middle − before − after`). A blend of two pictures lies on the line
//!   between them, so a picture at the middle of a dissolve or fade has a
//!   small residual and a large `across`; motion, cuts and flashes do not.
//!
//! The measures are stored, so the rules can be run again exactly on any
//! later read. Shots are proposals; they never cut.
//!
//! [`SHOT_RULE`] names the rule [`ShotAnalysis::boundaries`] applies. Hard
//! cuts ([`ShotAnalysis::cuts`]) are found exactly as by `deadpan-shots-1`:
//! picture `k` begins a shot when
//!
//! 1. its cell change is at least 24 (of 255) and its histogram change at least
//!    48 (of 255);
//! 2. its cell change is at least three times the mean cell change of the up
//!    to eight pictures on each side (excluding itself), so sustained motion
//!    does not read as a run of cuts; and
//! 3. picture `k` differs from picture `k - 2` by a cell change of at least
//!    24, and picture `k + 1` (when it exists) does too, so a one-picture
//!    flash returning to its scene is not a cut; and
//! 4. at least six pictures have passed since the previous cut (or the
//!    first picture).
//!
//! Gradual transitions ([`ShotAnalysis::transitions`]) are described in the
//! `gradual` module; each yields one boundary, and the boundaries of both
//! kinds are merged keeping six pictures between them.

use thiserror::Error;

mod context;
mod gradual;
mod measurer;
mod signatures;

pub use context::{
    CONTEXT_SHOT_PADDING, CONTEXT_SHOT_RULE, ContextSeamChange, ContextShotCoverage,
    ContextShotQualification, ContextShotTransition, MAX_CONTEXT_SHOT_SIGNATURES,
    context_seam_change, context_shot_window, qualify_context,
};
pub use gradual::{GradualTransition, TransitionKind};
pub use measurer::{REPLAY_PICTURES, ShotMeasurer, ShotProgress, ShotProgressTail};
pub use signatures::{
    CONTEXT_SIGNATURE_ENCODING, CONTEXT_SIGNATURE_HEADER_BYTES, MAX_CONTEXT_SIGNATURE_BYTES,
    PICTURE_SIGNATURE_BYTES, decode_context_signatures, encode_context_signatures,
};

/// The versioned boundary rule implemented by [`ShotAnalysis::boundaries`].
pub const SHOT_RULE: &str = "deadpan-shots-2";
/// The measurement stored per picture: [`PictureMeasure`]. Stored analyses
/// are keyed by it.
pub const SIGNATURE_VERSION: &str = "deadpan-picture-signature-2";
pub const GRID_COLUMNS: usize = 32;
pub const GRID_ROWS: usize = 18;
pub const HISTOGRAM_BINS: usize = 32;
/// Ten hours at 120 pictures per second.
pub const MAX_SHOT_PICTURES: usize = 10 * 60 * 60 * 120;
/// Half-spans of the `across`/`residual` comparisons, in pictures. The
/// largest window, `2 · 25 + 1` pictures, encloses a transition of up to 49
/// blended pictures; [`MAX_TRANSITION_PICTURES`] is 48.
pub const HALF_SPANS: [usize; 9] = [2, 3, 4, 6, 8, 12, 16, 20, 25];
/// Stored bytes per picture: change (3), luma, spread and two per half-span.
pub const MEASURE_BYTES: usize = 5 + 2 * HALF_SPANS.len();
/// Shortest and longest reported gradual transition, in blended pictures.
pub const MIN_TRANSITION_PICTURES: usize = 3;
pub const MAX_TRANSITION_PICTURES: usize = 48;

const MAX_HALF_SPAN: usize = HALF_SPANS[HALF_SPANS.len() - 1];
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

/// One picture's stored measurement; see the module documentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PictureMeasure {
    pub change: PictureChange,
    /// Mean cell luma, full range 0..=255.
    pub luma: u8,
    /// Mean absolute deviation of cell luma from `luma`.
    pub spread: u8,
    /// `[across, residual]` per [`HALF_SPANS`] entry; `[0, 0]` where the
    /// window would reach outside the pictures.
    pub spans: [[u8; 2]; HALF_SPANS.len()],
}

impl PictureMeasure {
    /// A measure with only consecutive changes (no luma or span
    /// comparisons), for synthetic analyses.
    pub fn from_change(change: PictureChange) -> Self {
        Self {
            change,
            ..Self::default()
        }
    }

    pub fn to_bytes(&self) -> [u8; MEASURE_BYTES] {
        let mut bytes = [0; MEASURE_BYTES];
        bytes[..3].copy_from_slice(&self.change);
        bytes[3] = self.luma;
        bytes[4] = self.spread;
        for (index, span) in self.spans.iter().enumerate() {
            bytes[5 + 2 * index..7 + 2 * index].copy_from_slice(span);
        }
        bytes
    }

    pub fn from_bytes(bytes: &[u8; MEASURE_BYTES]) -> Self {
        let mut spans = [[0; 2]; HALF_SPANS.len()];
        for (index, span) in spans.iter_mut().enumerate() {
            *span = [bytes[5 + 2 * index], bytes[6 + 2 * index]];
        }
        Self {
            change: [bytes[0], bytes[1], bytes[2]],
            luma: bytes[3],
            spread: bytes[4],
            spans,
        }
    }

    fn is_black(&self) -> bool {
        self.luma <= gradual::BLACK_LUMA && self.spread <= gradual::BLACK_SPREAD
    }
}

/// Encode measures as [`MEASURE_BYTES`] bytes each, in picture order.
pub fn encode_measures(measures: &[PictureMeasure]) -> Vec<u8> {
    measures.iter().flat_map(PictureMeasure::to_bytes).collect()
}

/// Decode [`encode_measures`] output; the length must be a whole number of
/// measures. Values are validated by [`ShotAnalysis::new`] or
/// [`ShotProgress::new`].
pub fn decode_measures(bytes: &[u8]) -> Result<Vec<PictureMeasure>, ShotError> {
    if !bytes.len().is_multiple_of(MEASURE_BYTES) {
        return Err(ShotError::Invalid("measure bytes are not whole pictures"));
    }
    if bytes.len() / MEASURE_BYTES > MAX_SHOT_PICTURES {
        return Err(ShotError::Limit);
    }
    Ok(bytes
        .chunks_exact(MEASURE_BYTES)
        .map(|chunk| PictureMeasure::from_bytes(chunk.try_into().expect("exact chunk")))
        .collect())
}

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
        let geometry = Geometry {
            height,
            row_bytes,
            row_stride_bytes,
            // Pixel columns of grid column c: x with x · columns / width == c.
            column_starts: (0..=GRID_COLUMNS)
                .map(|column| (column * width).div_ceil(GRID_COLUMNS))
                .collect(),
        };
        // Large pictures are reduced in row bands on scoped threads; integer
        // sums make the result independent of the split.
        let bands = if width * height >= PARALLEL_PIXELS {
            std::thread::available_parallelism()
                .map_or(1, usize::from)
                .clamp(1, MAX_SIGNATURE_THREADS)
        } else {
            1
        };
        let rows: Vec<_> = (0..bands)
            .map(|band| band * height / bands..(band + 1) * height / bands)
            .collect();
        let mut partials = std::thread::scope(|scope| {
            let workers: Vec<_> = rows[1..]
                .iter()
                .map(|rows| {
                    let geometry = &geometry;
                    let rows = rows.clone();
                    scope.spawn(move || geometry.reduce(rgba, rows))
                })
                .collect();
            let mut partials = vec![geometry.reduce(rgba, rows[0].clone())];
            partials.extend(
                workers
                    .into_iter()
                    .map(|worker| worker.join().expect("signature band panicked")),
            );
            partials
        })
        .into_iter();
        let Partial {
            mut sums,
            mut counts,
            mut histogram,
        } = partials.next().expect("at least one band");
        for partial in partials {
            for (total, part) in sums.iter_mut().zip(&partial.sums) {
                for (total, part) in total.iter_mut().zip(part) {
                    *total += part;
                }
            }
            for (total, part) in counts.iter_mut().zip(&partial.counts) {
                *total += part;
            }
            for (total, part) in histogram.iter_mut().zip(&partial.histogram) {
                *total += part;
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

    /// Mean cell luma and the mean absolute deviation of cell luma from it.
    pub fn luma(&self) -> [u8; 2] {
        let count = self.cells.len() as u32;
        let total: u32 = self.cells.iter().map(|cell| u32::from(cell[0])).sum();
        // Deviation from the exact mean, total / count, scaled by count.
        let deviation: u32 = self
            .cells
            .iter()
            .map(|cell| (u32::from(cell[0]) * count).abs_diff(total))
            .sum();
        let squared = count * count;
        [
            ((total + count / 2) / count).min(255) as u8,
            ((deviation + squared / 2) / squared).min(255) as u8,
        ]
    }

    /// `[across, residual]` of `self` as the middle of `before` and `after`:
    /// their cell difference, and half the mean absolute value of
    /// `2·self − before − after` over all cell samples.
    pub fn span(&self, before: &Self, after: &Self) -> [u8; 2] {
        let across = before.cell_difference(after);
        let total: u32 = self
            .cells
            .iter()
            .zip(&before.cells)
            .zip(&after.cells)
            .map(|((middle, before), after)| {
                (0..3)
                    .map(|channel| {
                        (2 * i32::from(middle[channel])
                            - i32::from(before[channel])
                            - i32::from(after[channel]))
                        .unsigned_abs()
                    })
                    .sum::<u32>()
            })
            .sum();
        let samples = (self.cells.len() * 3) as u32 * 2;
        [across, ((total + samples / 2) / samples).min(255) as u8]
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

/// Pictures of at least this many pixels are reduced on several threads.
const PARALLEL_PIXELS: usize = 1 << 20;
const MAX_SIGNATURE_THREADS: usize = 4;

struct Geometry {
    height: usize,
    row_bytes: usize,
    row_stride_bytes: usize,
    column_starts: Vec<usize>,
}

/// Cell sums, cell pixel counts and the luma histogram of some rows.
struct Partial {
    sums: Vec<[u64; 3]>,
    counts: Vec<u64>,
    histogram: [u64; HISTOGRAM_BINS],
}

impl Geometry {
    fn reduce(&self, rgba: &[u8], rows: std::ops::Range<usize>) -> Partial {
        let mut sums = vec![[0_u64; 3]; GRID_COLUMNS * GRID_ROWS];
        let mut counts = vec![0_u64; GRID_COLUMNS * GRID_ROWS];
        // Four interleaved histograms shorten the increment dependency chain.
        let mut histograms = [[0_u32; HISTOGRAM_BINS]; 4];
        for y in rows {
            let start = y * self.row_stride_bytes;
            let row = &rgba[start..start + self.row_bytes];
            let cell_row = y * GRID_ROWS / self.height * GRID_COLUMNS;
            for column in 0..GRID_COLUMNS {
                let (first, end) = (self.column_starts[column], self.column_starts[column + 1]);
                let mut sum = [0_u32; 3];
                for (index, pixel) in row[first * 4..end * 4].chunks_exact(4).enumerate() {
                    let (r, g, b) = (
                        i32::from(pixel[0]),
                        i32::from(pixel[1]),
                        i32::from(pixel[2]),
                    );
                    // Integer Rec.601 full-range YCbCr; only differences
                    // matter. Luma lies in 0..=255 and both differences in
                    // 1..=256 before clamping, so only the top needs a clamp.
                    let luma = (77 * r + 150 * g + 29 * b + 128) >> 8;
                    let blue = (((-43 * r - 85 * g + 128 * b + 128) >> 8) + 128).min(255);
                    let red = (((128 * r - 107 * g - 21 * b + 128) >> 8) + 128).min(255);
                    sum[0] += luma as u32;
                    sum[1] += blue as u32;
                    sum[2] += red as u32;
                    histograms[index & 3][luma as usize >> 3] += 1;
                }
                let cell = cell_row + column;
                for (total, part) in sums[cell].iter_mut().zip(sum) {
                    *total += u64::from(part);
                }
                counts[cell] += (end - first) as u64;
            }
        }
        let mut histogram = [0_u64; HISTOGRAM_BINS];
        for part in &histograms {
            for (total, count) in histogram.iter_mut().zip(part) {
                *total += u64::from(*count);
            }
        }
        Partial {
            sums,
            counts,
            histogram,
        }
    }
}

/// Spans of picture `picture` that cannot exist in `pictures` pictures, or
/// that are not yet measured when only `measured` pictures have arrived,
/// must be zero.
fn spans_are_canonical(measure: &PictureMeasure, picture: usize, measured: usize) -> bool {
    HALF_SPANS
        .iter()
        .zip(&measure.spans)
        .all(|(half, span)| (picture >= *half && picture + half < measured) || *span == [0, 0])
}

/// Validated stored measures, one [`PictureMeasure`] per picture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShotAnalysis {
    measures: Vec<PictureMeasure>,
}

impl ShotAnalysis {
    pub fn new(measures: Vec<PictureMeasure>) -> Result<Self, ShotError> {
        if measures.len() > MAX_SHOT_PICTURES {
            return Err(ShotError::Limit);
        }
        if measures
            .first()
            .is_some_and(|first| first.change != [0, 0, 0])
        {
            return Err(ShotError::Invalid("the first picture has no predecessor"));
        }
        let pictures = measures.len();
        if !measures
            .iter()
            .enumerate()
            .all(|(picture, measure)| spans_are_canonical(measure, picture, pictures))
        {
            return Err(ShotError::Invalid("a span reaches outside the pictures"));
        }
        Ok(Self { measures })
    }

    /// An analysis of consecutive changes only (no luma or span
    /// comparisons, so no gradual transitions), for synthetic analyses.
    pub fn from_changes(changes: Vec<PictureChange>) -> Result<Self, ShotError> {
        Self::new(
            changes
                .into_iter()
                .map(PictureMeasure::from_change)
                .collect(),
        )
    }

    /// Measures of complete signatures, in picture order.
    pub fn from_signatures(signatures: &[PictureSignature]) -> Result<Self, ShotError> {
        let mut measurer = ShotMeasurer::new(signatures.len())?;
        for (ordinal, signature) in signatures.iter().enumerate() {
            measurer.push(ordinal, signature.clone())?;
        }
        measurer.finish()
    }

    pub fn pictures(&self) -> usize {
        self.measures.len()
    }

    pub fn measures(&self) -> &[PictureMeasure] {
        &self.measures
    }

    /// The consecutive change into `picture`.
    pub fn change(&self, picture: usize) -> Option<PictureChange> {
        self.measures.get(picture).map(|measure| measure.change)
    }

    /// Pictures that begin a shot under [`SHOT_RULE`], in order: hard cuts
    /// and one boundary per gradual transition, at least six pictures
    /// apart (a gradual boundary too close to a cut is dropped). Picture 0
    /// always begins the first shot and is not listed.
    pub fn boundaries(&self) -> Vec<usize> {
        let cuts = self.cuts();
        let mut boundaries = cuts.clone();
        for transition in self.transitions_with(&cuts) {
            let boundary = transition.boundary;
            let position = boundaries.partition_point(|existing| *existing < boundary);
            let clear = boundary >= MIN_SHOT_PICTURES
                && boundaries
                    .get(position)
                    .is_none_or(|next| next - boundary >= MIN_SHOT_PICTURES)
                && position
                    .checked_sub(1)
                    .is_none_or(|previous| boundary - boundaries[previous] >= MIN_SHOT_PICTURES);
            if clear {
                boundaries.insert(position, boundary);
            }
        }
        boundaries
    }

    /// Gradual transitions under [`SHOT_RULE`], in order.
    pub fn transitions(&self) -> Vec<GradualTransition> {
        self.transitions_with(&self.cuts())
    }

    fn transitions_with(&self, cuts: &[usize]) -> Vec<GradualTransition> {
        gradual::transitions(&self.measures, cuts)
    }

    /// Hard cuts, exactly as `deadpan-shots-1` proposed them.
    pub fn cuts(&self) -> Vec<usize> {
        let mut boundaries = Vec::new();
        let mut previous = 0;
        for (picture, measure) in self.measures.iter().enumerate().skip(1) {
            let [cell, histogram, skip] = measure.change;
            let returns_next = self
                .measures
                .get(picture + 1)
                .is_some_and(|next| next.change[2] < MIN_CELL_CHANGE);
            if cell < MIN_CELL_CHANGE
                || histogram < MIN_HISTOGRAM_CHANGE
                || skip < MIN_CELL_CHANGE
                || returns_next
                || picture - previous < MIN_SHOT_PICTURES
            {
                continue;
            }
            let first = picture.saturating_sub(LOCAL_RADIUS).max(1);
            let last = (picture + LOCAL_RADIUS).min(self.measures.len() - 1);
            let (sum, count) = (first..=last).filter(|neighbor| *neighbor != picture).fold(
                (0_u32, 0_u32),
                |(sum, count), neighbor| {
                    (
                        sum + u32::from(self.measures[neighbor].change[0]),
                        count + 1,
                    )
                },
            );
            // cell ≥ 3 · mean  ⇔  cell · count ≥ 3 · sum
            if u32::from(cell) * count.max(1) >= LOCAL_RATIO * sum {
                boundaries.push(picture);
                previous = picture;
            }
        }
        boundaries
    }
}

#[cfg(test)]
mod tests;
