use std::collections::HashMap;
use std::f64::consts::PI;
use std::ops::Range;
use std::sync::atomic::AtomicBool;

use deadpan_core::{AudioSample, ExactRatio, TimeError};
use serde::Serialize;

use crate::{MAX_OUTPUT_FRAMES, MAX_SOURCE_FRAMES, PreparationError, StereoMatrix, check_cancel};

/// Exact source sample coordinates, independent of the requested block and seek.
/// Forward constant rates in [1/64, 64] are supported. This performs tape-speed
/// sampling; it does not implement pitch-preserving or variable-rate stretch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResampleRecipe {
    selection: Range<i64>,
    source_origin: ExactRatio,
    output_origin: AudioSample,
    source_step: ExactRatio,
    output_range: Range<AudioSample>,
}

impl ResampleRecipe {
    pub fn new(
        selection: Range<i64>,
        source_origin: ExactRatio,
        output_origin: AudioSample,
        source_step: ExactRatio,
        output_range: Range<AudioSample>,
    ) -> Result<Self, PreparationError> {
        if output_range.start.0 < 0 {
            return Err(PreparationError::InvalidRecipe(
                "empty or negative output range",
            ));
        }
        Self::on_signed_grid(
            selection,
            source_origin,
            output_origin,
            source_step,
            output_range,
        )
    }

    /// Internal physical-domain reads retain a captured absolute root grid,
    /// whose hidden context can precede sample zero. This changes no source
    /// support, rate or phase. Public root and point-grid entrypoints retain
    /// their own nonnegative output admission.
    pub(crate) fn on_signed_grid(
        selection: Range<i64>,
        source_origin: ExactRatio,
        output_origin: AudioSample,
        source_step: ExactRatio,
        output_range: Range<AudioSample>,
    ) -> Result<Self, PreparationError> {
        if selection.start >= selection.end {
            return Err(PreparationError::InvalidRecipe("empty source selection"));
        }
        if output_range.start >= output_range.end {
            return Err(PreparationError::InvalidRecipe(
                "empty or negative output range",
            ));
        }
        if source_step.numerator() <= 0
            || source_step.compare_integer(64).is_gt()
            || source_step
                .checked_mul(ExactRatio::integer(64))?
                .compare_integer(1)
                .is_lt()
        {
            return Err(PreparationError::InvalidRecipe(
                "source step outside 1/64..64",
            ));
        }
        let recipe = Self {
            selection,
            source_origin,
            output_origin,
            source_step,
            output_range,
        };
        // Check the complete authored interval, not merely the first block.
        recipe.position(recipe.output_range.start)?;
        recipe.position(AudioSample(recipe.output_range.end.0 - 1))?;
        Ok(recipe)
    }

    pub fn selection(&self) -> Range<i64> {
        self.selection.clone()
    }
    pub fn source_origin(&self) -> ExactRatio {
        self.source_origin
    }
    pub fn output_origin(&self) -> AudioSample {
        self.output_origin
    }
    pub fn source_step(&self) -> ExactRatio {
        self.source_step
    }
    pub fn output_range(&self) -> Range<AudioSample> {
        self.output_range.clone()
    }

    /// Compute from the common origin for every sample, never by adding a
    /// floating-point increment to the preceding sample's position.
    pub fn source_at(&self, output: AudioSample) -> Result<ExactRatio, TimeError> {
        self.source_origin.checked_add(
            ExactRatio::new(i128::from(output.0) - i128::from(self.output_origin.0), 1)?
                .checked_mul(self.source_step)?,
        )
    }

    fn position(&self, output: AudioSample) -> Result<(i64, f64), PreparationError> {
        let position = self.source_at(output)?;
        let floor = i64::try_from(position.floor()).map_err(|_| TimeError::Overflow)?;
        // Never convert a large absolute source timestamp to floating point.
        let fraction = position.numerator().rem_euclid(position.denominator()) as f64
            / position.denominator() as f64;
        Ok((floor, fraction))
    }
}

/// Exactly the requested original interleaved sample window, in matrix order.
/// No padding is accepted here. The sampler supplies its own named zero context
/// outside the authored selection; unavailable selected PCM remains an error.
#[derive(Debug)]
pub struct PcmWindow {
    pub start: i64,
    pub samples: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StereoBlock {
    pub start: AudioSample,
    pub samples: Vec<[f32; 2]>,
}

#[derive(Debug, Clone)]
pub struct Resampler {
    recipe: ResampleRecipe,
    matrix: StereoMatrix,
}

impl Resampler {
    pub fn new(recipe: ResampleRecipe, matrix: StereoMatrix) -> Self {
        Self { recipe, matrix }
    }
    pub fn recipe(&self) -> &ResampleRecipe {
        &self.recipe
    }
    pub fn matrix(&self) -> &StereoMatrix {
        &self.matrix
    }

    fn identity(&self) -> bool {
        self.recipe.source_step == ExactRatio::ONE && self.recipe.source_origin.denominator() == 1
    }

    fn radius(&self) -> Result<i64, PreparationError> {
        if self.recipe.source_step.compare_integer(1).is_gt() {
            Ok(i64::try_from(
                self.recipe
                    .source_step
                    .checked_mul(ExactRatio::integer(128))?
                    .ceil()?,
            )
            .map_err(|_| TimeError::Overflow)?)
        } else {
            Ok(128)
        }
    }

    fn checked_end(
        &self,
        start: AudioSample,
        frames: u32,
    ) -> Result<AudioSample, PreparationError> {
        if frames == 0 || frames > MAX_OUTPUT_FRAMES {
            return Err(PreparationError::InvalidRecipe(
                "block must contain 1..256 frames",
            ));
        }
        let end = AudioSample(
            start
                .0
                .checked_add(i64::from(frames))
                .ok_or(TimeError::Overflow)?,
        );
        if start < self.recipe.output_range.start || end > self.recipe.output_range.end {
            return Err(PreparationError::InvalidRecipe(
                "block outside authored output interval",
            ));
        }
        Ok(end)
    }

    /// One bounded, contiguous halo read suffices for each output block. The
    /// range is intersected with the authored trim before any provider I/O.
    pub fn required_source_range(
        &self,
        start: AudioSample,
        frames: u32,
    ) -> Result<Option<Range<i64>>, PreparationError> {
        let end = self.checked_end(start, frames)?;
        let (first, _) = self.recipe.position(start)?;
        let (last, _) = self.recipe.position(AudioSample(end.0 - 1))?;
        let (left, right) = if self.identity() {
            (i128::from(first), i128::from(last) + 1)
        } else {
            let radius = i128::from(self.radius()?);
            (
                i128::from(first) - radius + 1,
                i128::from(last) + radius + 1,
            )
        };
        let left = left.max(i128::from(self.recipe.selection.start));
        let right = right.min(i128::from(self.recipe.selection.end));
        if left >= right {
            return Ok(None);
        }
        if right - left > i128::from(MAX_SOURCE_FRAMES) {
            return Err(PreparationError::InvalidRecipe(
                "source halo exceeds frame budget",
            ));
        }
        Ok(Some(
            i64::try_from(left).map_err(|_| TimeError::Overflow)?
                ..i64::try_from(right).map_err(|_| TimeError::Overflow)?,
        ))
    }

    /// Worker-only: validates and stages a complete block before returning it.
    /// A provider must fail if any sample within the required range is missing.
    pub fn render(
        &self,
        start: AudioSample,
        frames: u32,
        window: Option<PcmWindow>,
        cancelled: &AtomicBool,
    ) -> Result<StereoBlock, PreparationError> {
        check_cancel(cancelled)?;
        let required = self.required_source_range(start, frames)?;
        let mixed = self.mix_window(&required, window, cancelled)?;
        let mut samples = Vec::with_capacity(frames as usize);
        let radius = self.radius()?;
        let step = self.recipe.source_step.numerator() as f64
            / self.recipe.source_step.denominator() as f64;
        let cutoff = 0.95 / step.max(1.0);
        let mut kernels = KernelPhases::default();
        let mut positions = PositionWalk::new(&self.recipe, start)?;
        for offset in 0..frames {
            check_cancel(cancelled)?;
            let (floor, fraction) = match &mut positions {
                Some(walk) => walk.next()?,
                None => self
                    .recipe
                    .position(AudioSample(start.0 + i64::from(offset)))?,
            };
            let value = if self.identity() {
                sample(&required, &mixed, i128::from(floor))
            } else {
                let (weights, dc) = kernels.at(fraction, radius, cutoff);
                let mut sum = [0.0; 2];
                let first = i128::from(floor) - i128::from(radius) + 1;
                let last = i128::from(floor) + i128::from(radius);
                match &required {
                    Some(range)
                        if first >= i128::from(range.start) && last < i128::from(range.end) =>
                    {
                        let start = (first - i128::from(range.start)) as usize;
                        let taps = &mixed[start..start + weights.len()];
                        for (weight, source) in weights.iter().zip(taps) {
                            sum[0] += weight * source[0];
                            sum[1] += weight * source[1];
                        }
                    }
                    _ => {
                        for (tap, weight) in ((-radius + 1)..=radius).zip(weights) {
                            let source =
                                sample(&required, &mixed, i128::from(floor) + i128::from(tap));
                            sum[0] += weight * source[0];
                            sum[1] += weight * source[1];
                        }
                    }
                }
                // Full-kernel DC calibration, including zero-extended taps.
                // This is independent of source amplitude and selected length.
                [sum[0] / dc, sum[1] / dc]
            };
            let value = [value[0] as f32, value[1] as f32];
            if value.iter().any(|value| !value.is_finite()) {
                return Err(PreparationError::InvalidSamples);
            }
            samples.push(value);
        }
        check_cancel(cancelled)?;
        Ok(StereoBlock { start, samples })
    }

    fn mix_window(
        &self,
        required: &Option<Range<i64>>,
        window: Option<PcmWindow>,
        cancelled: &AtomicBool,
    ) -> Result<Vec<[f64; 2]>, PreparationError> {
        let (required, window) = match (required, window) {
            (None, None) => return Ok(Vec::new()),
            (Some(required), Some(window)) => (required, window),
            _ => return Err(PreparationError::InvalidSamples),
        };
        let channels = self.matrix.layout().channels() as usize;
        let frames =
            usize::try_from(required.end - required.start).map_err(|_| TimeError::Overflow)?;
        if window.start != required.start || window.samples.len() != frames * channels {
            return Err(PreparationError::InvalidSamples);
        }
        let mut mixed = Vec::with_capacity(frames);
        for (index, frame) in window.samples.chunks_exact(channels).enumerate() {
            if index % 256 == 0 {
                check_cancel(cancelled)?;
            }
            mixed.push(self.matrix.mix_frame(frame)?);
        }
        Ok(mixed)
    }
}

fn sample(required: &Option<Range<i64>>, mixed: &[[f64; 2]], at: i128) -> [f64; 2] {
    if let Some(range) = required
        && at >= i128::from(range.start)
        && at < i128::from(range.end)
    {
        mixed[(at - i128::from(range.start)) as usize]
    } else {
        [0.0; 2]
    }
}

/// Exact source positions of consecutive outputs in integers. `ExactRatio`
/// arithmetic per sample otherwise spent much of a render in 128-bit division.
struct PositionWalk {
    floor: i64,
    remainder: i64,
    step: i64,
    denominator: i64,
}

impl PositionWalk {
    /// `None` when a remainder or denominator may not convert to `f64`
    /// exactly, where dividing the reduced ratio could round differently.
    fn new(recipe: &ResampleRecipe, start: AudioSample) -> Result<Option<Self>, PreparationError> {
        const EXACT_F64: i128 = 1 << 53;
        let position = recipe.source_at(start)?;
        let (step, step_denominator) = (
            recipe.source_step.numerator(),
            recipe.source_step.denominator(),
        );
        let origin = recipe.source_origin.denominator();
        let Some(denominator) = origin
            .checked_mul(step_denominator / gcd(origin, step_denominator))
            .filter(|denominator| *denominator < EXACT_F64)
        else {
            return Ok(None);
        };
        let scale = denominator / position.denominator();
        let remainder = position.numerator().rem_euclid(position.denominator()) * scale;
        let step = step
            .checked_mul(denominator / step_denominator)
            .filter(|step| *step < i128::from(i64::MAX) / 2);
        let (Some(step), Ok(floor)) = (step, i64::try_from(position.floor())) else {
            return Ok(None);
        };
        Ok(Some(Self {
            floor,
            remainder: remainder as i64,
            step: step as i64,
            denominator: denominator as i64,
        }))
    }

    fn next(&mut self) -> Result<(i64, f64), PreparationError> {
        let current = (self.floor, self.remainder as f64 / self.denominator as f64);
        let total = self.remainder + self.step;
        self.floor = self
            .floor
            .checked_add(total / self.denominator)
            .ok_or(TimeError::Overflow)?;
        self.remainder = total % self.denominator;
        Ok(current)
    }
}

fn gcd(mut left: i128, mut right: i128) -> i128 {
    while right != 0 {
        (left, right) = (right, left % right);
    }
    left
}

/// Bounds one render's retained weights to 8 MiB.
const MAX_KERNEL_WEIGHTS: usize = 1 << 20;

/// Weights already evaluated in one render, keyed by exact fractional phase. A
/// rational rate repeats few phases, and trigonometry otherwise dominated
/// resampling. Equal inputs give the same bits, so sums are unchanged.
#[derive(Default)]
struct KernelPhases {
    phases: HashMap<u64, (Vec<f64>, f64)>,
    uncached: (Vec<f64>, f64),
}

impl KernelPhases {
    fn at(&mut self, fraction: f64, radius: i64, cutoff: f64) -> (&[f64], f64) {
        let key = fraction.to_bits();
        let taps = 2 * radius.unsigned_abs() as usize;
        let retain = (self.phases.len() + 1) * taps <= MAX_KERNEL_WEIGHTS;
        if retain && !self.phases.contains_key(&key) {
            self.phases
                .insert(key, kernel_weights(fraction, radius, cutoff));
        }
        let (weights, dc) = match self.phases.get(&key) {
            Some(entry) => entry,
            None => {
                self.uncached = kernel_weights(fraction, radius, cutoff);
                &self.uncached
            }
        };
        (weights, *dc)
    }
}

fn kernel_weights(fraction: f64, radius: i64, cutoff: f64) -> (Vec<f64>, f64) {
    let mut dc = 0.0;
    let weights = ((-radius + 1)..=radius)
        .map(|tap| {
            let weight = kernel(tap as f64 - fraction, radius as f64, cutoff);
            dc += weight;
            weight
        })
        .collect();
    (weights, dc)
}

fn kernel(distance: f64, radius: f64, cutoff: f64) -> f64 {
    if distance.abs() >= radius {
        return 0.0;
    }
    let phase = PI * distance / radius;
    let window = 0.35875
        + 0.48829 * phase.cos()
        + 0.14128 * (2.0 * phase).cos()
        + 0.01168 * (3.0 * phase).cos();
    let phase = PI * cutoff * distance;
    let sinc = if phase == 0.0 {
        1.0
    } else {
        phase.sin() / phase
    };
    cutoff * sinc * window
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bits(weights: &[f64], dc: f64) -> (Vec<u64>, u64) {
        (
            weights.iter().map(|weight| weight.to_bits()).collect(),
            dc.to_bits(),
        )
    }

    #[test]
    fn position_walk_matches_exact_positions_for_every_output() {
        for (origin, step) in [
            (
                ExactRatio::new(7, 3).unwrap(),
                ExactRatio::new(147, 160).unwrap(),
            ),
            (
                ExactRatio::new(-5, 7).unwrap(),
                ExactRatio::new(160, 147).unwrap(),
            ),
            (
                ExactRatio::integer(1_000_000),
                ExactRatio::new(1, 64).unwrap(),
            ),
            (ExactRatio::new(1, 2).unwrap(), ExactRatio::integer(64)),
        ] {
            let recipe = ResampleRecipe {
                selection: i64::MIN / 2..i64::MAX / 2,
                source_origin: origin,
                output_origin: AudioSample(-20),
                source_step: step,
                output_range: AudioSample(-20)..AudioSample(5_000),
            };
            let start = AudioSample(-11);
            let mut walk = PositionWalk::new(&recipe, start).unwrap().unwrap();
            for output in start.0..start.0 + 4_096 {
                let (floor, fraction) = recipe.position(AudioSample(output)).unwrap();
                let (walked, walked_fraction) = walk.next().unwrap();
                assert_eq!(walked, floor);
                assert_eq!(walked_fraction.to_bits(), fraction.to_bits());
            }
        }
    }

    #[test]
    fn cached_kernel_phases_keep_exact_bits_before_and_after_the_bound() {
        let radius = 8192;
        let taps = 2 * radius as usize;
        let mut kernels = KernelPhases::default();
        let capacity = MAX_KERNEL_WEIGHTS / taps;
        for phase in 0..capacity + 3 {
            let fraction = phase as f64 / 1024.0;
            let (weights, dc) = kernel_weights(fraction, radius, 0.5);
            let expected = bits(&weights, dc);
            for _ in 0..2 {
                let (weights, dc) = kernels.at(fraction, radius, 0.5);
                assert_eq!(bits(weights, dc), expected);
            }
        }
        assert_eq!(kernels.phases.len(), capacity);
    }
}
