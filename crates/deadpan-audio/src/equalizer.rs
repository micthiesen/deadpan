//! One finite, compensated linear-phase EQ recipe, shared by future authored
//! voice/group consumers. This numerical processor does not select or synthesize
//! hidden owner context, infer permission for tails, or mutate a document.

use std::sync::atomic::{AtomicBool, Ordering};

use deadpan_core::{EQ_BASS_CROSSOVER_HZ, EQ_TREBLE_CROSSOVER_HZ, EqualizerRecipe};
use deadpan_dsp::{
    FINITE_FILTER_INPUT_FRAMES as INPUT, FINITE_FILTER_OUTPUT_FRAMES as OUTPUT,
    FINITE_FILTER_RADIUS as RADIUS, FINITE_FILTER_TAPS as TAPS, FilterError, FiniteStereoFilter,
    MAX_FILTER_SAMPLE_MAGNITUDE,
};

pub const EQ_FILTER_ID: &str = "deadpan-eq-48k-blackman4097-complementary250-4000-f64-v1";
const NYQUIST: u16 = 24_000;

fn check(cancelled: &AtomicBool) -> Result<(), FilterError> {
    if cancelled.load(Ordering::Relaxed) {
        Err(FilterError::Cancelled)
    } else {
        Ok(())
    }
}

/// Compile exact static parameters into the finite kernel. Three complementary
/// band responses and both cuts are combined into ONE 4097-tap FIR, so enabling
/// cuts does not silently increase history or look-ahead. The common 2048-frame
/// delay is compensated by supplied future input, never a timestamp shift.
///
/// A flat recipe compiles to an exact central impulse. Uniform band gain without
/// cuts compiles to that single amplitude, preserving cancellation between band
/// boundaries. Nonuniform recipes use DC-normalized Blackman-windowed sinc
/// low-pass bases. Cutoffs/crossovers are transition centers (nominal -6 dB for
/// cuts), not infinitely sharp frequency boundaries. Call on a worker.
pub fn equalizer_coefficients(
    recipe: EqualizerRecipe,
    cancelled: &AtomicBool,
) -> Result<[f64; TAPS], FilterError> {
    check(cancelled)?;
    let low = recipe.low_cut().map_or(0, |value| value.hz());
    let high = recipe.high_cut().map_or(NYQUIST, |value| value.hz());
    let bands = [
        (0, EQ_BASS_CROSSOVER_HZ),
        (EQ_BASS_CROSSOVER_HZ, EQ_TREBLE_CROSSOVER_HZ),
        (EQ_TREBLE_CROSSOVER_HZ, NYQUIST),
    ];
    // At most six interval endpoints, consolidated before evaluating kernels.
    // Equal adjacent gains cancel exactly rather than leaving round-off ripple.
    let mut terms = [(0u16, 0.0_f64); 6];
    let mut count = 0;
    for ((begin, end), gain) in bands.into_iter().zip(recipe.gains()) {
        let begin = begin.max(low);
        let end = end.min(high);
        if begin >= end {
            continue;
        }
        let amplitude = 10.0_f64.powf(f64::from(gain.millidecibels()) / 20_000.0);
        for (hz, weight) in [(begin, -amplitude), (end, amplitude)] {
            if let Some(term) = terms[..count].iter_mut().find(|term| term.0 == hz) {
                term.1 += weight;
            } else {
                terms[count] = (hz, weight);
                count += 1;
            }
        }
    }
    let mut result = [0.0; TAPS];
    for &(hz, weight) in &terms[..count] {
        check(cancelled)?;
        if weight == 0.0 || hz == 0 {
            continue;
        }
        if hz == NYQUIST {
            result[RADIUS] += weight;
            continue;
        }
        let kernel = low_pass(hz, cancelled)?;
        for (output, input) in result.iter_mut().zip(kernel) {
            *output += weight * input;
        }
    }
    check(cancelled)?;
    Ok(result)
}

fn low_pass(hz: u16, cancelled: &AtomicBool) -> Result<[f64; TAPS], FilterError> {
    let frequency = f64::from(hz) / 48_000.0;
    let mut kernel = [0.0; TAPS];
    for offset in 0..=RADIUS {
        if offset % 128 == 0 {
            check(cancelled)?;
        }
        let x = offset as f64;
        let angle = std::f64::consts::PI * x / RADIUS as f64;
        let window = 0.42 + 0.5 * angle.cos() + 0.08 * (2.0 * angle).cos();
        let ideal = if offset == 0 {
            2.0 * frequency
        } else {
            (std::f64::consts::TAU * frequency * x).sin() / (std::f64::consts::PI * x)
        };
        let value = ideal * window;
        kernel[RADIUS + offset] = value;
        kernel[RADIUS - offset] = value;
    }
    let dc: f64 = kernel.iter().sum();
    for value in &mut kernel {
        *value /= dc;
    }
    Ok(kernel)
}

/// Worker-owned EQ with a fixed 2048-frame history and look-ahead in its input
/// clock. Retains the kernel for diagnostics and cache identity. Callers supply
/// the complete fixed-grid context and own policy gating and work admission.
/// The neutral recipe preserves each input sample's exact bits. Other recipes
/// use the bounded native FIR and retain signed f64 until later bus conversion.
pub struct Equalizer {
    recipe: EqualizerRecipe,
    coefficients: Box<[f64; TAPS]>,
    filter: Option<FiniteStereoFilter>,
}

impl Equalizer {
    pub fn new(recipe: EqualizerRecipe, cancelled: &AtomicBool) -> Result<Self, FilterError> {
        let coefficients = Box::new(equalizer_coefficients(recipe, cancelled)?);
        let filter = if recipe.is_unity() {
            None
        } else {
            Some(FiniteStereoFilter::new(&coefficients)?)
        };
        check(cancelled)?;
        Ok(Self {
            recipe,
            coefficients,
            filter,
        })
    }
    pub const fn recipe(&self) -> EqualizerRecipe {
        self.recipe
    }
    pub fn coefficients(&self) -> &[f64; TAPS] {
        &self.coefficients
    }

    /// Input contains 2048 history + 1024 core + 2048 look-ahead frames per
    /// channel. Both output arrays require 1024 slots and change only on success.
    pub fn process_tile(
        &mut self,
        left: &[f64],
        right: &[f64],
        output_left: &mut [f64],
        output_right: &mut [f64],
        cancelled: &AtomicBool,
    ) -> Result<(), FilterError> {
        if let Some(filter) = &mut self.filter {
            return filter.process_tile(left, right, output_left, output_right, cancelled);
        }
        // Bypass keeps the same admission/error contract as active processing,
        // including otherwise unused history and future input.
        if left.len() != INPUT || right.len() != INPUT {
            return Err(FilterError::InputLength);
        }
        if output_left.len() != OUTPUT || output_right.len() != OUTPUT {
            return Err(FilterError::OutputLength);
        }
        check(cancelled)?;
        if left
            .iter()
            .chain(right)
            .any(|value| !value.is_finite() || value.abs() > MAX_FILTER_SAMPLE_MAGNITUDE)
        {
            return Err(FilterError::InvalidInput);
        }
        check(cancelled)?;
        output_left.copy_from_slice(&left[RADIUS..RADIUS + OUTPUT]);
        output_right.copy_from_slice(&right[RADIUS..RADIUS + OUTPUT]);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
