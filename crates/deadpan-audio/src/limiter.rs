//! Fixed-work, linked stereo master gain on the canonical project sample grid.
//!
//! The caller supplies real project context, not a separately zero-extended
//! playback block. Construction belongs on a preparation worker. No source I/O,
//! device access, authored state, normalization, or fallback processing lives here.

mod coefficients;

use std::{
    collections::VecDeque,
    ops::Range,
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

use deadpan_core::AudioSample;
use deadpan_dsp::FixedPeakBank;

use crate::true_peak::FIR;

/// Pinned native FFT schedule, coefficients, trigger and final direct verifier.
pub const LIMITER_ID: &str = "deadpan-master-kaiser64-bs4-trigger17-q32-four-pass-fft1280-v1";
pub const LIMITER_MAX_CONTEXT_FRAMES: usize = 131_072;
pub const LIMITER_MAX_OUTPUT_FRAMES: usize = 8_192;
/// Four passes of detector, cap dilation, release history and FFT tile context.
pub const LIMITER_LEFT_HALO: i64 = 37_376;
/// Four passes of detector, cap dilation, attack lookahead and FFT tile context.
pub const LIMITER_RIGHT_HALO: i64 = 20_992;
const RADIUS: usize = 64;
const TILE: usize = 1_024;
const INPUT_TILE: usize = TILE + 2 * RADIUS;
const PASSES: usize = 4;
const Q: u64 = 1 << 32;
const ATTACK: u64 = 1 << 20;
const RELEASE: u64 = 1 << 19;
const CEILING: f64 = 0.891_250_938_133_745_6;
const TARGET: f64 = 0.822_242_649_947_071_2;

/// Owned, unmodified bus samples inside the actual complete project interval.
/// Admission, including finite sample validation, occurs in [`LimitedTile::prepare`].
#[derive(Debug)]
pub struct LimiterContext {
    pub project_samples: Range<AudioSample>,
    pub start: AudioSample,
    pub samples: Vec<[f32; 2]>,
}

#[derive(Debug, thiserror::Error)]
pub enum LimiterError {
    #[error("invalid master sample interval or project allocation")]
    Range,
    #[error("master context exceeds 131072 stereo frames")]
    ContextBudget,
    #[error("master output exceeds 8192 stereo frames")]
    OutputBudget,
    #[error("master context omits required project samples {required:?}")]
    MissingContext { required: Range<AudioSample> },
    #[error("master input must contain finite samples with magnitude at most 16")]
    InvalidSamples,
    #[error("master preparation was cancelled")]
    Cancelled,
    #[error("master preparation exceeded its shared deadline")]
    Deadline,
    #[error("native master detector failed: {0}")]
    Native(String),
    #[error("master calculation produced invalid numerical values")]
    Numerical,
    #[error(
        "master final {bank} phase {phase} channel {channel} at anchor {anchor} exceeds -1 dBTP: {peak}"
    )]
    Ceiling {
        bank: &'static str,
        phase: usize,
        channel: usize,
        anchor: i128,
        peak: f64,
    },
}

/// Only the requested, fully prepared and verified inner interval is published.
#[derive(Debug)]
pub struct LimitedTile {
    pub start: AudioSample,
    pub samples: Vec<[f32; 2]>,
    /// One common multiplier applied to both original float32 channels.
    pub gain: Vec<f64>,
    /// Largest final finite estimate at this interval's owned detector anchors.
    pub peak: f64,
}

impl LimitedTile {
    pub fn algorithm(&self) -> &'static str {
        LIMITER_ID
    }

    pub fn min_gain(&self) -> f64 {
        self.gain.iter().copied().fold(1.0, f64::min)
    }

    pub fn max_gain(&self) -> f64 {
        self.gain.iter().copied().fold(0.0, f64::max)
    }

    /// Zero gain has no finite decibel representation. An empty output has none.
    pub fn maximum_reduction_db(&self) -> Option<f64> {
        let gain = self.min_gain();
        (!self.gain.is_empty() && gain > 0.0).then(|| -20.0 * gain.log10())
    }

    /// Includes the direct final verifier's radius exactly once. Only complete
    /// project boundaries permit clipping; request and cache boundaries do not.
    pub fn required_context(
        project: &Range<AudioSample>,
        requested: &Range<AudioSample>,
    ) -> Result<Range<AudioSample>, LimiterError> {
        validate_range(project, requested)?;
        if requested.is_empty() {
            return Ok(requested.clone());
        }
        let first =
            (i128::from(requested.start.0) - i128::from(LIMITER_LEFT_HALO) - RADIUS as i128)
                .max(i128::from(project.start.0));
        let last = (i128::from(requested.end.0) + i128::from(LIMITER_RIGHT_HALO) + RADIUS as i128)
            .min(i128::from(project.end.0));
        Ok(AudioSample(narrow(first)?)..AudioSample(narrow(last)?))
    }

    pub fn prepare(
        context: LimiterContext,
        requested: Range<AudioSample>,
        deadline: Instant,
        cancelled: &AtomicBool,
    ) -> Result<Self, LimiterError> {
        let control = Control {
            deadline,
            cancelled,
        };
        control.check()?;
        let required = Self::required_context(&context.project_samples, &requested)?;
        if context.samples.len() > LIMITER_MAX_CONTEXT_FRAMES {
            return Err(LimiterError::ContextBudget);
        }
        let first = i128::from(context.start.0);
        let last = first + context.samples.len() as i128;
        if first < i128::from(context.project_samples.start.0)
            || last > i128::from(context.project_samples.end.0)
        {
            return Err(LimiterError::Range);
        }
        if first > i128::from(required.start.0) || last < i128::from(required.end.0) {
            return Err(LimiterError::MissingContext { required });
        }
        let mut all_zero = true;
        for chunk in context.samples.chunks(TILE) {
            control.check()?;
            if chunk
                .iter()
                .flatten()
                .any(|value| !value.is_finite() || value.abs() > crate::MAX_INPUT_MAGNITUDE)
            {
                return Err(LimiterError::InvalidSamples);
            }
            all_zero &= chunk.iter().flatten().all(|value| *value == 0.0);
        }
        if requested.is_empty() {
            return Ok(Self {
                start: requested.start,
                samples: Vec::new(),
                gain: Vec::new(),
                peak: 0.0,
            });
        }
        let offset = index(i128::from(requested.start.0) - first)?;
        let length = index(i128::from(requested.end.0) - i128::from(requested.start.0))?;
        if all_zero {
            // Complete required context was admitted above. Every finite detector
            // row and correction is exactly zero/unity, including project edges.
            // Copy the original bits so authored negative zero remains intact.
            let output = Self {
                start: requested.start,
                samples: context.samples[offset..offset + length].to_vec(),
                gain: vec![1.0; length],
                peak: 0.0,
            };
            control.check()?;
            return Ok(output);
        }
        let mut bank = FixedPeakBank::new(coefficients::KAISER)
            .map_err(|error| LimiterError::Native(error.to_string()))?;
        control.check()?;
        let mut samples = context.samples.clone();
        let mut gain = vec![1.0; samples.len()];
        for _ in 0..PASSES {
            let peaks = detect(&samples, first, &mut bank, &control)?;
            let caps = dilated_caps(&peaks, samples.len(), &control)?;
            if caps.iter().all(|cap| *cap == Q) {
                // A unity correction changes neither cumulative gain nor f32
                // output. All remaining deterministic passes are identical.
                break;
            }
            apply_envelope(&context.samples, &mut samples, &mut gain, caps, &control)?;
        }
        let owned = owned_anchors(&context.project_samples, &requested);
        let peak = verify(&samples, first, owned, &control)?;
        control.check()?;
        let output = Self {
            start: requested.start,
            samples: samples[offset..offset + length].to_vec(),
            gain: gain[offset..offset + length].to_vec(),
            peak,
        };
        control.check()?;
        Ok(output)
    }
}

struct Control<'a> {
    deadline: Instant,
    cancelled: &'a AtomicBool,
}

impl Control<'_> {
    fn check(&self) -> Result<(), LimiterError> {
        if self.cancelled.load(Ordering::Relaxed) {
            Err(LimiterError::Cancelled)
        } else if Instant::now() >= self.deadline {
            Err(LimiterError::Deadline)
        } else {
            Ok(())
        }
    }
}

fn validate_range(
    project: &Range<AudioSample>,
    requested: &Range<AudioSample>,
) -> Result<(), LimiterError> {
    if project.start.0 != 0
        || project.end < project.start
        || requested.start < project.start
        || requested.end < requested.start
        || requested.end > project.end
    {
        return Err(LimiterError::Range);
    }
    if i128::from(requested.end.0) - i128::from(requested.start.0)
        > LIMITER_MAX_OUTPUT_FRAMES as i128
    {
        return Err(LimiterError::OutputBudget);
    }
    Ok(())
}

fn narrow(value: i128) -> Result<i64, LimiterError> {
    i64::try_from(value).map_err(|_| LimiterError::Range)
}

fn index(value: i128) -> Result<usize, LimiterError> {
    usize::try_from(value).map_err(|_| LimiterError::Range)
}

fn sample_at(samples: &[[f32; 2]], start: i128, anchor: i128) -> [f64; 2] {
    let offset = anchor - start;
    if offset < 0 || offset >= samples.len() as i128 {
        [0.0; 2]
    } else {
        samples[offset as usize].map(f64::from)
    }
}

fn bs_at(samples: &[[f32; 2]], start: i128, anchor: i128) -> [[f64; 4]; 2] {
    let mut values = [[0.0; 4]; 2];
    // Matches the pilot's centered 13-slot correlation: published FIR
    // convolution index is anchor+5, with the thirteenth coefficient zero.
    for (tap, weights) in FIR.iter().enumerate() {
        let sample = sample_at(samples, start, anchor + 5 - tap as i128);
        for (channel, value) in values.iter_mut().enumerate() {
            for (phase, sum) in value.iter_mut().enumerate() {
                *sum += sample[channel] * weights[phase];
            }
        }
    }
    values
}

fn detect(
    samples: &[[f32; 2]],
    start: i128,
    bank: &mut FixedPeakBank,
    control: &Control<'_>,
) -> Result<Vec<f64>, LimiterError> {
    control.check()?;
    let first = start - RADIUS as i128;
    let last = start + samples.len() as i128 + RADIUS as i128;
    let mut peaks = vec![0.0_f64; samples.len() + 2 * RADIUS];
    let mut left = [0.0; INPUT_TILE];
    let mut right = [0.0; INPUT_TILE];
    let mut output = [0.0; TILE];
    let mut tile = first.div_euclid(TILE as i128) * TILE as i128;
    while tile < last {
        control.check()?;
        left.fill(0.0);
        right.fill(0.0);
        let input_first = tile - RADIUS as i128;
        let copy_first = input_first.max(start);
        let copy_last = (input_first + INPUT_TILE as i128).min(start + samples.len() as i128);
        if copy_first < copy_last {
            let source = index(copy_first - start)?;
            let destination = index(copy_first - input_first)?;
            let count = index(copy_last - copy_first)?;
            for (i, sample) in samples[source..source + count].iter().enumerate() {
                left[destination + i] = sample[0];
                right[destination + i] = sample[1];
            }
        }
        let processed = bank.process_tile(&left, &right, &mut output, control.cancelled);
        control.check()?;
        processed.map_err(|error| LimiterError::Native(error.to_string()))?;
        let low = tile.max(first);
        let high = (tile + TILE as i128).min(last);
        for anchor in low..high {
            let value = output[index(anchor - tile)?];
            if !value.is_finite() || value < 0.0 {
                return Err(LimiterError::Numerical);
            }
            peaks[index(anchor - first)?] = value;
        }
        tile += TILE as i128;
    }
    for (offset, peak) in peaks.iter_mut().enumerate() {
        if offset % TILE == 0 {
            control.check()?;
        }
        let anchor = first + offset as i128;
        for value in sample_at(samples, start, anchor)
            .into_iter()
            .chain(bs_at(samples, start, anchor).into_iter().flatten())
        {
            if !value.is_finite() {
                return Err(LimiterError::Numerical);
            }
            *peak = peak.max(value.abs());
        }
    }
    Ok(peaks)
}

fn dilated_caps(
    peaks: &[f64],
    count: usize,
    control: &Control<'_>,
) -> Result<Vec<u64>, LimiterError> {
    let width = 2 * RADIUS + 1;
    let mut minima: VecDeque<(usize, u64)> = VecDeque::with_capacity(width);
    let mut caps = vec![Q; count];
    for (i, peak) in peaks.iter().copied().enumerate() {
        if i % TILE == 0 {
            control.check()?;
        }
        let cap = if peak > CEILING {
            let units = (TARGET / peak * Q as f64).floor();
            if !units.is_finite() || !(0.0..=Q as f64).contains(&units) {
                return Err(LimiterError::Numerical);
            }
            // The checked closed interval is exactly representable in u64.
            units as u64
        } else {
            Q
        };
        while minima.back().is_some_and(|(_, value)| *value >= cap) {
            minima.pop_back();
        }
        if minima.front().is_some_and(|(at, _)| *at + width <= i) {
            minima.pop_front();
        }
        minima.push_back((i, cap));
        if i >= width - 1 {
            let at = i - (width - 1);
            if at < count {
                caps[at] = minima.front().ok_or(LimiterError::Numerical)?.1;
            }
        }
    }
    Ok(caps)
}

fn apply_envelope(
    original: &[[f32; 2]],
    samples: &mut [[f32; 2]],
    gain: &mut [f64],
    mut caps: Vec<u64>,
    control: &Control<'_>,
) -> Result<(), LimiterError> {
    let mut next = Q;
    for i in (0..caps.len()).rev() {
        if i % TILE == 0 {
            control.check()?;
        }
        caps[i] = caps[i].min(next + ATTACK);
        next = caps[i];
    }
    let mut previous = Q;
    for (i, cap) in caps.into_iter().enumerate() {
        if i % TILE == 0 {
            control.check()?;
        }
        let value = cap.min(previous + RELEASE);
        previous = value;
        gain[i] *= value as f64 / Q as f64;
        if !gain[i].is_finite() || !(0.0..=1.0).contains(&gain[i]) {
            return Err(LimiterError::Numerical);
        }
        samples[i] = original[i].map(|input| (f64::from(input) * gain[i]) as f32);
        if samples[i].iter().any(|sample| !sample.is_finite()) {
            return Err(LimiterError::Numerical);
        }
    }
    Ok(())
}

fn owned_anchors(project: &Range<AudioSample>, requested: &Range<AudioSample>) -> Range<i128> {
    let mut first = i128::from(requested.start.0);
    let mut last = i128::from(requested.end.0);
    if requested.start == project.start {
        first -= RADIUS as i128;
    }
    if requested.end == project.end {
        last += RADIUS as i128;
    }
    first..last
}

fn verify(
    samples: &[[f32; 2]],
    start: i128,
    anchors: Range<i128>,
    control: &Control<'_>,
) -> Result<f64, LimiterError> {
    let mut maximum = 0.0_f64;
    for anchor in anchors {
        if anchor.rem_euclid(64) == 0 {
            control.check()?;
        }
        let mut check = |value: f64, bank, channel, phase| {
            if !value.is_finite() {
                return Err(LimiterError::Numerical);
            }
            let peak = value.abs();
            if peak > CEILING {
                return Err(LimiterError::Ceiling {
                    bank,
                    phase,
                    channel,
                    anchor,
                    peak,
                });
            }
            maximum = maximum.max(peak);
            Ok(())
        };
        for (channel, sample) in sample_at(samples, start, anchor).into_iter().enumerate() {
            check(sample, "sample", channel, 0)?;
        }
        for (channel, phases) in bs_at(samples, start, anchor).into_iter().enumerate() {
            for (phase, value) in phases.into_iter().enumerate() {
                check(value, "bs1770", channel, phase)?;
            }
        }
        for (phase, row) in coefficients::KAISER.iter().enumerate() {
            let mut values = [0.0; 2];
            for (tap, weight) in row.iter().enumerate() {
                let sample = sample_at(samples, start, anchor + tap as i128 - RADIUS as i128);
                for (channel, value) in values.iter_mut().enumerate() {
                    *value += sample[channel] * weight;
                }
            }
            for (channel, value) in values.into_iter().enumerate() {
                check(value, "kaiser64", channel, phase + 1)?;
            }
        }
    }
    control.check()?;
    Ok(maximum)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::time::Duration;

    #[test]
    fn coefficient_bits_match_the_retained_pilot() {
        let mut hash = Sha256::new();
        for value in coefficients::KAISER.into_iter().flatten() {
            hash.update(value.to_le_bytes());
        }
        let digest: [u8; 32] = hash.finalize().into();
        assert_eq!(
            digest,
            [
                0x48, 0x8b, 0x81, 0x86, 0x2c, 0x8d, 0x68, 0x7f, 0xc9, 0xd6, 0x12, 0xa0, 0x0d, 0x10,
                0xcd, 0x82, 0xb6, 0x8f, 0x30, 0x41, 0xf6, 0xf7, 0x16, 0xb4, 0x42, 0x46, 0x1e, 0x0f,
                0x78, 0x4b, 0x24, 0x1c,
            ]
        );
    }

    #[test]
    fn exact_integer_envelope_has_the_declared_finite_horizons() {
        let cancelled = AtomicBool::new(false);
        let control = Control {
            deadline: Instant::now() + Duration::from_secs(5),
            cancelled: &cancelled,
        };
        let input = vec![[1.0; 2]; 30_000];
        let mut output = input.clone();
        let mut gain = vec![1.0; input.len()];
        let mut caps = vec![Q; input.len()];
        let cut = 10_000;
        caps[cut] = 0;
        apply_envelope(&input, &mut output, &mut gain, caps, &control).unwrap();
        assert_eq!(gain[cut - 4_096], 1.0);
        assert_eq!(gain[cut - 4_095], (Q - ATTACK) as f64 / Q as f64);
        assert_eq!(gain[cut], 0.0);
        assert_eq!(gain[cut + 8_191], (Q - RELEASE) as f64 / Q as f64);
        assert_eq!(gain[cut + 8_192], 1.0);
    }

    #[test]
    fn final_verification_rejects_peaks_on_actual_outer_project_rows() {
        let cancelled = AtomicBool::new(false);
        let control = Control {
            deadline: Instant::now() + Duration::from_secs(5),
            cancelled: &cancelled,
        };
        let project = AudioSample(0)..AudioSample(2);
        let result = verify(
            &[[0.85, 0.0], [-0.85, 0.0]],
            0,
            owned_anchors(&project, &project),
            &control,
        );
        assert!(matches!(
            result,
            Err(LimiterError::Ceiling { anchor: -1, .. })
        ));
        // An unrelated incomplete work-buffer edge is not an owned anchor.
        let mut work = vec![[0.0; 2]; 256];
        work[0] = [1.0; 2];
        assert_eq!(verify(&work, 0, 128..129, &control).unwrap(), 0.0);
    }
}
