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
        for offset in 0..frames {
            check_cancel(cancelled)?;
            let (floor, fraction) = self
                .recipe
                .position(AudioSample(start.0 + i64::from(offset)))?;
            let value = if self.identity() {
                sample(&required, &mixed, i128::from(floor))
            } else {
                let mut sum = [0.0; 2];
                let mut dc = 0.0;
                for tap in (-radius + 1)..=radius {
                    if (tap + radius - 1) % 256 == 0 {
                        check_cancel(cancelled)?;
                    }
                    let distance = tap as f64 - fraction;
                    let weight = kernel(distance, radius as f64, cutoff);
                    dc += weight;
                    let source = sample(&required, &mixed, i128::from(floor) + i128::from(tap));
                    sum[0] += weight * source[0];
                    sum[1] += weight * source[1];
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
