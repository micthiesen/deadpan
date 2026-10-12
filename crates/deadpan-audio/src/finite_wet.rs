//! Finite wet-only effect kernels for worker-owned, random-access preparation.
//! This numerical layer does not choose voices, route returns, allocate time or
//! infer permission to sound across a silent interval. Callers own those rules.

use std::sync::atomic::{AtomicBool, Ordering};

use deadpan_core::TailEffect;
use deadpan_dsp::{CAUSAL_FILTER_OUTPUT_FRAMES, CausalFilterError, CausalStereoFilter};

pub const FINITE_REVERB_ID: &str = "deadpan-wet-reverb-comb8-allpass4-48k-fir4s-fade50ms-v1";
pub const FINITE_DELAY_ID: &str = "deadpan-wet-delay-300ms-half-48k-fir8s-fade50ms-v1";
const FADE_FRAMES: usize = 2400;

/// Exact sample bounds at 48 kHz. A causal kernel preserves output labels:
/// latency and look-ahead are zero. A nonzero input at n can affect output up
/// to n + tail_frames, but never authorizes extending the document allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WetEffectDescriptor {
    pub algorithm_id: &'static str,
    pub latency_frames: usize,
    pub history_frames: usize,
    pub tail_frames: usize,
    pub output_frames: usize,
}

pub const fn wet_effect_descriptor(effect: TailEffect) -> WetEffectDescriptor {
    let (algorithm_id, history_frames) = match effect {
        TailEffect::Reverb => (FINITE_REVERB_ID, 192_000),
        TailEffect::Delay => (FINITE_DELAY_ID, 384_000),
    };
    WetEffectDescriptor {
        algorithm_id,
        latency_frames: 0,
        history_frames,
        tail_frames: history_frames,
        output_frames: CAUSAL_FILTER_OUTPUT_FRAMES,
    }
}

fn check(cancelled: &AtomicBool) -> Result<(), CausalFilterError> {
    if cancelled.load(Ordering::Relaxed) {
        Err(CausalFilterError::Cancelled)
    } else {
        Ok(())
    }
}

/// Two independent channel impulse responses. The existing deterministic room
/// and echo networks generate a bounded kernel once; a 50 ms linear taper ends
/// at exact zero. The finite recipe has its own identity and does not replace
/// the existing allocated Hold tail algorithm. No dry impulse is added.
///
/// Reverb has four seconds of history. Delay has eight seconds, including 26
/// echoes at exact 14,400-sample spacings with amplitudes 1, 1/2, ..., 2^-25.
/// Kernels are never normalized. Preparation checks cancellation every 256
/// samples and owns no source media, shared cache or device.
pub fn finite_wet_coefficients(
    effect: TailEffect,
    cancelled: &AtomicBool,
) -> Result<[Vec<f64>; 2], CausalFilterError> {
    check(cancelled)?;
    let descriptor = wet_effect_descriptor(effect);
    let count = descriptor.history_frames + 1;
    let mut coefficients = [vec![0.0; count], vec![0.0; count]];
    let mut engine = crate::hold_effects::effect_engine(effect);
    let [left, right] = &mut coefficients;
    for (frame, (left, right)) in left.iter_mut().zip(right).enumerate() {
        if frame % 256 == 0 {
            check(cancelled)?;
        }
        let impulse = if frame == 0 { [1.0; 2] } else { [0.0; 2] };
        let wet = engine.process(impulse);
        let remaining = count - 1 - frame;
        let fade = (remaining.min(FADE_FRAMES) as f64) / FADE_FRAMES as f64;
        *left = wet[0] * fade;
        *right = wet[1] * fade;
    }
    check(cancelled)?;
    Ok(coefficients)
}

/// The complete-history numerical processor. Construction and reads belong on
/// one worker. Callers supply fixed-grid context, gate it by the consuming
/// sample's permissions and clip returned samples to the authored interval.
/// It retains no prior input, so a cold read and a warmed read of the same
/// fixed tile are bit-identical on the same qualified build and machine.
pub struct FiniteWetEffect {
    effect: TailEffect,
    filter: CausalStereoFilter,
}

impl FiniteWetEffect {
    pub fn new(effect: TailEffect, cancelled: &AtomicBool) -> Result<Self, CausalFilterError> {
        let coefficients = finite_wet_coefficients(effect, cancelled)?;
        let filter = CausalStereoFilter::new(&coefficients[0], &coefficients[1])?;
        check(cancelled)?;
        Ok(Self { effect, filter })
    }

    pub const fn descriptor(&self) -> WetEffectDescriptor {
        wet_effect_descriptor(self.effect)
    }

    pub fn input_frames(&self) -> usize {
        self.filter.input_frames()
    }

    /// Bound on native storage plus the temporary stereo kernel used during
    /// construction. Caller-owned input/output buffers and any graph caches
    /// require separate admission. No kernel is retained after construction.
    pub fn preparation_bytes_bound(effect: TailEffect) -> usize {
        let taps = wet_effect_descriptor(effect).history_frames + 1;
        // Both declared recipes are within the native adapter's fixed bound.
        // 1 MiB also covers the temporary network's delay lines and metadata.
        CausalStereoFilter::native_bytes_bound(taps).expect("bounded wet effect recipe")
            + taps * 2 * size_of::<f64>()
            + 1_048_576
    }

    pub fn process_tile(
        &mut self,
        left: &[f64],
        right: &[f64],
        output_left: &mut [f64],
        output_right: &mut [f64],
        cancelled: &AtomicBool,
    ) -> Result<(), CausalFilterError> {
        self.filter
            .process_tile(left, right, output_left, output_right, cancelled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn never() -> AtomicBool {
        AtomicBool::new(false)
    }

    fn run(effect: &mut FiniteWetEffect, a: &[f64], b: &[f64]) -> [Vec<f64>; 2] {
        let count = effect.descriptor().output_frames;
        let mut left = vec![f64::NAN; count];
        let mut right = vec![f64::NAN; count];
        effect
            .process_tile(a, b, &mut left, &mut right, &never())
            .unwrap();
        [left, right]
    }

    #[test]
    fn declared_delay_is_26_exact_unscaled_echoes_with_no_dry_signal() {
        let descriptor = wet_effect_descriptor(TailEffect::Delay);
        assert_eq!(descriptor.latency_frames, 0);
        assert_eq!(descriptor.history_frames, 384_000);
        assert_eq!(descriptor.tail_frames, 384_000);
        let coefficients = finite_wet_coefficients(TailEffect::Delay, &never()).unwrap();
        assert_eq!(coefficients[0], coefficients[1]);
        for (lag, &value) in coefficients[0].iter().enumerate() {
            let expected = if lag > 0 && lag % 14_400 == 0 {
                0.5_f64.powi((lag / 14_400 - 1) as i32)
            } else {
                0.0
            };
            assert_eq!(value, expected, "lag {lag}");
        }
        assert_eq!(coefficients[0].iter().filter(|x| **x != 0.0).count(), 26);
    }

    #[test]
    fn reverb_retains_stereo_network_then_tapers_to_exact_finite_end() {
        let descriptor = wet_effect_descriptor(TailEffect::Reverb);
        assert_eq!(descriptor.latency_frames, 0);
        assert_eq!(descriptor.history_frames, 192_000);
        assert_eq!(descriptor.tail_frames, 192_000);
        let coefficients = finite_wet_coefficients(TailEffect::Reverb, &never()).unwrap();
        let mut engine = crate::hold_effects::effect_engine(TailEffect::Reverb);
        assert_eq!(coefficients[0][0], 0.0);
        assert_eq!(coefficients[1][0], 0.0);
        assert!(coefficients[0][..1215].iter().all(|x| *x == 0.0));
        assert_eq!(coefficients[0][1215], 0.03);
        assert!(coefficients[1][..1240].iter().all(|x| *x == 0.0));
        assert_eq!(coefficients[1][1240], 0.03);
        for (lag, (&left, &right)) in coefficients[0].iter().zip(&coefficients[1]).enumerate() {
            let raw = engine.process(if lag == 0 { [1.0; 2] } else { [0.0; 2] });
            let remaining = descriptor.history_frames - lag;
            let fade = remaining.min(2400) as f64 / 2400.0;
            assert_eq!(left, raw[0] * fade);
            assert_eq!(right, raw[1] * fade);
        }
        assert_ne!(coefficients[0], coefficients[1]);
        for channel in &coefficients {
            assert_eq!(*channel.last().unwrap(), 0.0);
            assert!(channel.iter().all(|x| x.is_finite()));
            assert!(channel.iter().any(|x| *x < -0.001));
        }
    }

    #[test]
    fn continuous_delay_matches_direct_all_echo_sum_at_exact_sample_positions() {
        let mut effect = FiniteWetEffect::new(TailEffect::Delay, &never()).unwrap();
        let history = effect.descriptor().history_frames;
        let input: Vec<_> = (0..effect.input_frames())
            .map(|i| ((i * 43 % 1009) as f64 - 504.0) / 100.0)
            .collect();
        let right: Vec<_> = input.iter().map(|x| -x * 0.25).collect();
        let actual = run(&mut effect, &input, &right);
        for frame in 0..CAUSAL_FILTER_OUTPUT_FRAMES {
            let expected: f64 = (1..=26)
                .map(|echo| input[history + frame - echo * 14_400] * 0.5_f64.powi(echo as i32 - 1))
                .sum();
            assert!((actual[0][frame] - expected).abs() < 2e-12);
            assert!((actual[1][frame] + expected * 0.25).abs() < 2e-12);
        }
        assert!(
            actual[0].iter().any(|x| x.abs() > 1.0),
            "no implicit limiter"
        );
    }

    #[test]
    fn reverb_matches_linear_network_within_untapered_history_and_never_crossfeeds() {
        let mut effect = FiniteWetEffect::new(TailEffect::Reverb, &never()).unwrap();
        let history = effect.descriptor().history_frames;
        let mut input = vec![0.0; effect.input_frames()];
        for (frame, sample) in input[history - 48_000..history - 47_500]
            .iter_mut()
            .enumerate()
        {
            *sample = ((frame * 31 % 127) as f64 - 63.0) / 16.0;
        }
        let silence = vec![0.0; input.len()];
        let actual = run(&mut effect, &input, &silence);
        let mut reference = crate::hold_effects::effect_engine(TailEffect::Reverb);
        for (at, sample) in input.iter().enumerate() {
            let expected = reference.process([*sample, 0.0]);
            if at >= history {
                assert!((actual[0][at - history] - expected[0]).abs() < 2e-12);
                assert_eq!(actual[1][at - history], 0.0);
            }
        }
        assert!(actual[0].iter().any(|x| x.abs() > 1e-6));
    }

    #[test]
    fn both_effects_seek_bit_identically_and_forget_samples_outside_complete_history() {
        for recipe in [TailEffect::Reverb, TailEffect::Delay] {
            let mut effect = FiniteWetEffect::new(recipe, &never()).unwrap();
            let count = effect.input_frames();
            let input: Vec<_> = (0..count + 2 * CAUSAL_FILTER_OUTPUT_FRAMES)
                .map(|i| if i % 977 == 0 { 0.5 } else { 0.0 })
                .collect();
            let mut linear = Vec::new();
            for tile in 0..3 {
                let begin = tile * CAUSAL_FILTER_OUTPUT_FRAMES;
                let slice = &input[begin..begin + count];
                linear.push(run(&mut effect, slice, slice));
            }
            for tile in [2, 0, 1] {
                let begin = tile * CAUSAL_FILTER_OUTPUT_FRAMES;
                let slice = &input[begin..begin + count];
                assert_eq!(run(&mut effect, slice, slice), linear[tile]);
                let mut cold = FiniteWetEffect::new(recipe, &never()).unwrap();
                assert_eq!(run(&mut cold, slice, slice), linear[tile]);
            }
            assert_eq!(
                run(&mut effect, &vec![0.0; count], &vec![0.0; count]),
                [
                    vec![0.0; CAUSAL_FILTER_OUTPUT_FRAMES],
                    vec![0.0; CAUSAL_FILTER_OUTPUT_FRAMES]
                ]
            );
        }
    }

    #[test]
    fn cancellation_and_invalid_context_do_not_publish_partial_output() {
        for recipe in [TailEffect::Reverb, TailEffect::Delay] {
            let cancelled = AtomicBool::new(true);
            assert_eq!(
                finite_wet_coefficients(recipe, &cancelled),
                Err(CausalFilterError::Cancelled)
            );
            assert!(matches!(
                FiniteWetEffect::new(recipe, &cancelled),
                Err(CausalFilterError::Cancelled)
            ));
            let mut effect = FiniteWetEffect::new(recipe, &never()).unwrap();
            let input = vec![0.0; effect.input_frames()];
            let mut left = vec![123.0; CAUSAL_FILTER_OUTPUT_FRAMES];
            let mut right = vec![-321.0; CAUSAL_FILTER_OUTPUT_FRAMES];
            assert_eq!(
                effect.process_tile(&input, &input, &mut left, &mut right, &cancelled),
                Err(CausalFilterError::Cancelled)
            );
            assert_eq!(
                effect.process_tile(&input[1..], &input, &mut left, &mut right, &never()),
                Err(CausalFilterError::InputLength)
            );
            assert_eq!(left, vec![123.0; CAUSAL_FILTER_OUTPUT_FRAMES]);
            assert_eq!(right, vec![-321.0; CAUSAL_FILTER_OUTPUT_FRAMES]);
            effect
                .process_tile(&input, &input, &mut left, &mut right, &never())
                .unwrap();
            assert_eq!(left, vec![0.0; CAUSAL_FILTER_OUTPUT_FRAMES]);
            assert_eq!(right, left);
        }
    }

    #[test]
    fn admission_includes_native_plan_and_temporary_kernel_and_network() {
        for effect in [TailEffect::Reverb, TailEffect::Delay] {
            let descriptor = wet_effect_descriptor(effect);
            let native =
                CausalStereoFilter::native_bytes_bound(descriptor.history_frames + 1).unwrap();
            let bound = FiniteWetEffect::preparation_bytes_bound(effect);
            assert!(bound > native + (descriptor.history_frames + 1) * 16);
            assert!(bound < 73 * 1024 * 1024);
        }
    }
}
