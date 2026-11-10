//! Tape-pitch audio for a continuous speed ramp (`PitchPolicy::TapeRamp`).
//!
//! The stage's input and output are both 48 kHz point grids on the project's
//! frame clock: input sample `k` is child-local frame `start + k·s` and output
//! sample `j` is ramp-output frame `j·s`, with `s = num / (48000·den)` frames
//! per sample for frame rate `num/den`. Substituting into the ramp's exact
//! integral (`deadpan_core::SpeedRamp`) gives the input position of every
//! output sample as one integer quotient,
//!
//! `q(j) = L·(2A·D·c·j + (B − A)·n·j²) / ((A + B)·D²·c)`
//!
//! with input length `L` and output duration `D` in frames, weights `A:B`,
//! `c = 48000·den` and `n = num`. Each sample's phase is evaluated directly
//! from that quotient, never accumulated from its neighbour, so block
//! boundaries and seeks cannot drift. The windowed-sinc kernel's cutoff and
//! width follow the local rate `q'(j)`, so speeds above 1x stay band-limited.

use deadpan_core::{FrameRate, SpeedRamp, TimeError};

/// Kernel half-width in input samples at or below unity speed; it widens
/// with the local rate above unity.
pub(crate) const RAMP_KERNEL_RADIUS: f64 = 64.0;

/// The same passband fraction the constant tape resampler uses.
const CUTOFF: f64 = 0.95;

#[derive(Debug, Clone)]
pub(crate) struct RampSampler {
    // Numerator coefficients of q(j) = (linear·j + quadratic·j²) / denominator.
    linear: i128,
    quadratic: i128,
    denominator: i128,
    input_count: i64,
    output_count: i64,
}

impl RampSampler {
    /// `input_frames`/`output_frames` are the Retime's integral input length
    /// and output duration; `input_count`/`output_count` the stage's point
    /// counts on the two grids.
    pub(crate) fn new(
        ramp: SpeedRamp,
        input_frames: i64,
        output_frames: i64,
        rate: FrameRate,
        input_count: u32,
        output_count: u32,
    ) -> Result<Self, TimeError> {
        if input_frames <= 0 || output_frames <= 0 {
            return Err(TimeError::InvalidRatio);
        }
        let (a, b) = ramp.weights()?;
        let l = i128::from(input_frames);
        let d = i128::from(output_frames);
        let c = 48_000 * i128::from(rate.denominator());
        let n = i128::from(rate.numerator());
        let mul = |x: i128, y: i128| x.checked_mul(y).ok_or(TimeError::Overflow);
        let sampler = Self {
            linear: mul(mul(mul(l, 2 * a)?, d)?, c)?,
            quadratic: mul(mul(l, b - a)?, n)?,
            denominator: mul(mul(mul(a + b, d)?, d)?, c)?,
            input_count: i64::from(input_count),
            output_count: i64::from(output_count),
        };
        // Prove the complete output range evaluates without overflow before
        // any input is read.
        sampler.position(sampler.output_count)?;
        Ok(sampler)
    }

    /// `q(j) = (linear·j + quadratic·j²) / denominator`, the exact mapping
    /// a pitch-preserving ramp's stretch schedule follows too.
    pub(crate) fn coefficients(&self) -> (i128, i128, i128) {
        (self.linear, self.quadratic, self.denominator)
    }

    fn numerator(&self, j: i64) -> Result<i128, TimeError> {
        let j = i128::from(j);
        self.linear
            .checked_mul(j)
            .and_then(|linear| {
                self.quadratic
                    .checked_mul(j)?
                    .checked_mul(j)?
                    .checked_add(linear)
            })
            .ok_or(TimeError::Overflow)
    }

    /// The exact input position of output sample `j`, as its floor and the
    /// fractional remainder (converted to `f64` only after the exact split).
    pub(crate) fn position(&self, j: i64) -> Result<(i64, f64), TimeError> {
        let numerator = self.numerator(j)?;
        let floor = numerator.div_euclid(self.denominator);
        let remainder = numerator.rem_euclid(self.denominator);
        Ok((
            i64::try_from(floor).map_err(|_| TimeError::Overflow)?,
            remainder as f64 / self.denominator as f64,
        ))
    }

    /// The local rate `q'(j)` in input samples per output sample.
    pub(crate) fn rate(&self, j: i64) -> f64 {
        (self.linear as f64 + 2.0 * self.quadratic as f64 * j as f64) / self.denominator as f64
    }

    fn radius(&self, rate: f64) -> i64 {
        (RAMP_KERNEL_RADIUS * rate.max(1.0)).ceil() as i64
    }

    /// The input samples read by outputs `[start, end)`, clipped to the
    /// stage input; outside it the input is zero.
    pub(crate) fn required_input(
        &self,
        start: i64,
        end: i64,
    ) -> Result<std::ops::Range<i64>, TimeError> {
        // The rate is linear in j, so its maximum over the block is at an end.
        let radius = self.radius(self.rate(start).max(self.rate(end - 1)));
        let (first, _) = self.position(start)?;
        let (last, _) = self.position(end - 1)?;
        let left = (first - radius + 1).clamp(0, self.input_count);
        let right = (last + radius + 1).clamp(0, self.input_count);
        Ok(left..right.max(left))
    }

    /// Outputs `[start, start + frames)` from an input window that begins at
    /// input sample `window_start` and covers [`Self::required_input`].
    pub(crate) fn render(
        &self,
        start: i64,
        frames: usize,
        window_start: i64,
        window: &[[f32; 2]],
    ) -> Result<Vec<[f32; 2]>, TimeError> {
        let mut output = Vec::with_capacity(frames);
        for j in start..start + frames as i64 {
            let (floor, fraction) = self.position(j)?;
            let rate = self.rate(j).max(1.0);
            let radius = self.radius(rate);
            let cutoff = CUTOFF / rate;
            let mut sum = [0.0_f64; 2];
            let mut dc = 0.0;
            let mut taps = KernelTaps::new(1.0 - radius as f64 - fraction, radius as f64, cutoff);
            for tap in (-radius + 1)..=radius {
                let weight = taps.next_weight();
                dc += weight;
                let at = floor + tap;
                if at < 0 || at >= self.input_count {
                    continue;
                }
                let index = usize::try_from(at - window_start).map_err(|_| TimeError::Overflow)?;
                let sample = window.get(index).ok_or(TimeError::Overflow)?;
                sum[0] += weight * f64::from(sample[0]);
                sum[1] += weight * f64::from(sample[1]);
            }
            output.push([(sum[0] / dc) as f32, (sum[1] / dc) as f32]);
        }
        Ok(output)
    }
}

/// The constant tape resampler's Blackman-Harris windowed sinc, evaluated at
/// consecutive unit-spaced distances. Each sample's taps start from two exact
/// trigonometric evaluations and advance by rotation, so a 16x ramp's 2,048
/// taps need no per-tap trigonometry; the rotation's rounding stays near
/// 1e-13 over a kernel, far below f32 output precision.
struct KernelTaps {
    distance: f64,
    radius: f64,
    cutoff: f64,
    // cos/sin of the window phase π·d/R and the sinc phase π·cutoff·d.
    window: (f64, f64),
    window_step: (f64, f64),
    sinc: (f64, f64),
    sinc_step: (f64, f64),
}

impl KernelTaps {
    fn new(first: f64, radius: f64, cutoff: f64) -> Self {
        use std::f64::consts::PI;
        let window = PI * first / radius;
        let sinc = PI * cutoff * first;
        Self {
            distance: first,
            radius,
            cutoff,
            window: (window.cos(), window.sin()),
            window_step: ((PI / radius).cos(), (PI / radius).sin()),
            sinc: (sinc.cos(), sinc.sin()),
            sinc_step: ((PI * cutoff).cos(), (PI * cutoff).sin()),
        }
    }

    fn next_weight(&mut self) -> f64 {
        use std::f64::consts::PI;
        let weight = if self.distance.abs() >= self.radius {
            0.0
        } else {
            let c = self.window.0;
            let window = 0.35875
                + 0.48829 * c
                + 0.14128 * (2.0 * c * c - 1.0)
                + 0.01168 * (4.0 * c * c * c - 3.0 * c);
            let phase = PI * self.cutoff * self.distance;
            let sinc = if phase.abs() < 1e-12 {
                1.0
            } else {
                self.sinc.1 / phase
            };
            self.cutoff * sinc * window
        };
        let rotate = |(c, s): (f64, f64), (dc, ds): (f64, f64)| (c * dc - s * ds, s * dc + c * ds);
        self.window = rotate(self.window, self.window_step);
        self.sinc = rotate(self.sinc, self.sinc_step);
        self.distance += 1.0;
        weight
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadpan_core::ExactRatio;

    fn ramp(from: (i128, i128), to: (i128, i128)) -> SpeedRamp {
        SpeedRamp::new(
            ExactRatio::new(from.0, from.1).unwrap(),
            ExactRatio::new(to.0, to.1).unwrap(),
        )
        .unwrap()
    }

    /// Output and input sample counts of a stage, as the plan's PointCeil
    /// grids count them.
    fn counts(input_frames: i64, output_frames: i64, rate: FrameRate) -> (u32, u32) {
        let samples = |frames: i64| {
            ExactRatio::new(
                i128::from(frames) * 48_000 * i128::from(rate.denominator()),
                i128::from(rate.numerator()),
            )
            .unwrap()
            .ceil()
            .unwrap() as u32
        };
        (samples(input_frames), samples(output_frames))
    }

    #[test]
    fn every_sample_position_is_the_exact_integral_of_the_core_ramp() {
        let rate = FrameRate::new(30_000, 1_001).unwrap();
        let ramp = ramp((1, 2), (2, 1));
        let (input_count, output_count) = counts(90, 72, rate);
        let sampler = RampSampler::new(ramp, 90, 72, rate, input_count, output_count).unwrap();
        let s = ExactRatio::new(30_000, 48_000 * 1_001).unwrap();
        for j in [0_i64, 1, 777, 50_000, i64::from(output_count) - 1] {
            let t = ExactRatio::integer(j).checked_mul(s).unwrap();
            let x = ramp
                .input_offset(t, ExactRatio::integer(72), ExactRatio::integer(90))
                .unwrap()
                .checked_div(s)
                .unwrap();
            let (floor, fraction) = sampler.position(j).unwrap();
            assert_eq!(i128::from(floor), x.floor());
            let exact = x.numerator().rem_euclid(x.denominator()) as f64 / x.denominator() as f64;
            assert!((fraction - exact).abs() < 1e-12, "{j}: {fraction} {exact}");
        }
    }

    /// A pitch-preserving ramp's stretch schedule is built from the same
    /// coefficients: at every 256-sample block boundary its input position is
    /// the core ramp's exact integral, rounded half-even, and the positions
    /// never decrease.
    #[test]
    fn the_stretch_schedule_meets_the_core_ramp_at_every_block_boundary() {
        let rate = FrameRate::new(30_000, 1_001).unwrap();
        let s = ExactRatio::new(30_000, 48_000 * 1_001).unwrap();
        for (from, to, input_frames, output_frames) in
            [((1, 2), (2, 1), 90_i64, 72_i64), ((8, 1), (1, 8), 650, 160)]
        {
            let ramp = ramp(from, to);
            let (input_count, output_count) = counts(input_frames, output_frames, rate);
            let sampler = RampSampler::new(
                ramp,
                input_frames,
                output_frames,
                rate,
                input_count,
                output_count,
            )
            .unwrap();
            let (linear, quadratic, denominator) = sampler.coefficients();
            let schedule =
                deadpan_dsp::RampSchedule::new(linear, quadratic, denominator, output_count)
                    .unwrap();
            let mut previous = i64::MIN;
            for j in (0..=i64::from(output_count)).step_by(256) {
                let t = ExactRatio::integer(j).checked_mul(s).unwrap();
                let exact = ramp
                    .input_offset(
                        t,
                        ExactRatio::integer(output_frames),
                        ExactRatio::integer(input_frames),
                    )
                    .unwrap()
                    .checked_div(s)
                    .unwrap()
                    .round_even()
                    .unwrap();
                let boundary = schedule.boundary(j).unwrap();
                assert_eq!(i128::from(boundary), exact, "{from:?}->{to:?} at {j}");
                assert!(boundary >= previous);
                previous = boundary;
            }
            eprintln!(
                "{from:?}->{to:?}: linearisation bound {} samples",
                schedule.linearisation_bound()
            );
        }
    }

    /// A pure input tone resampled through the ramp must equal the tone
    /// evaluated at the exact input position: an analytic reference that
    /// shares nothing with the kernel. Rates stay below the cutoff so the
    /// tone is in the passband everywhere.
    #[test]
    fn a_sine_through_the_ramp_matches_its_analytic_phase() {
        let rate = FrameRate::new(30_000, 1_001).unwrap();
        for (from, to, input_frames, output_frames) in
            [((1, 2), (2, 1), 90_i64, 72_i64), ((3, 1), (1, 3), 60, 36)]
        {
            let ramp = ramp(from, to);
            let (input_count, output_count) = counts(input_frames, output_frames, rate);
            let sampler = RampSampler::new(
                ramp,
                input_frames,
                output_frames,
                rate,
                input_count,
                output_count,
            )
            .unwrap();
            // 1 kHz at 48 kHz; at 3x the local frequency is 3 kHz.
            let omega = 2.0 * std::f64::consts::PI * 1_000.0 / 48_000.0;
            let input = (0..input_count)
                .map(|k| {
                    let value = (omega * f64::from(k)).sin() as f32;
                    [value, -value]
                })
                .collect::<Vec<_>>();
            let output = sampler.render(0, output_count as usize, 0, &input).unwrap();
            // High-precision reference: the exact rational position, reduced
            // modulo the tone period in integers before conversion.
            let s = ExactRatio::new(30_000, 48_000 * 1_001).unwrap();
            let mut worst = 0.0_f64;
            // Away from the zero-extended ends the kernel sees real input.
            let margin = 4 * RAMP_KERNEL_RADIUS as i64 * 3;
            for j in margin..i64::from(output_count) - margin {
                let t = ExactRatio::integer(j).checked_mul(s).unwrap();
                let x = ramp
                    .input_offset(
                        t,
                        ExactRatio::integer(output_frames),
                        ExactRatio::integer(input_frames),
                    )
                    .unwrap()
                    .checked_div(s)
                    .unwrap();
                let period = x.numerator().rem_euclid(48 * x.denominator());
                let phase = period as f64 / x.denominator() as f64;
                let expected = (omega * phase).sin();
                let got = f64::from(output[j as usize][0]);
                assert_eq!(output[j as usize][1], -output[j as usize][0]);
                worst = worst.max((got - expected).abs());
            }
            eprintln!("{from:?}->{to:?}: worst error {worst:e}");
            assert!(worst < 2e-6, "{from:?}->{to:?}: worst error {worst}");
        }
    }

    /// Rendering in blocks of any size, from any window, gives bit-identical
    /// samples: no phase is carried between blocks, so seams cannot click.
    /// The rotated taps equal the directly evaluated kernel.
    #[test]
    fn rotated_taps_match_the_direct_kernel() {
        use std::f64::consts::PI;
        let direct = |distance: f64, radius: f64, cutoff: f64| {
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
        };
        for (fraction, radius, cutoff) in [
            (0.0, 64.0, 0.95),
            (0.37, 1024.0, 0.95 / 16.0),
            (0.999, 97.0, 0.6),
        ] {
            let mut taps = KernelTaps::new(1.0 - radius - fraction, radius, cutoff);
            for tap in (1 - radius as i64)..=radius as i64 {
                let expected = direct(tap as f64 - fraction, radius, cutoff);
                assert!(
                    (taps.next_weight() - expected).abs() < 1e-12,
                    "{fraction} {radius} {tap}"
                );
            }
        }
    }

    #[test]
    fn blocks_and_seeks_are_bit_identical_to_one_render() {
        let rate = FrameRate::new(24, 1).unwrap();
        let ramp = ramp((1, 4), (4, 1));
        let (input_count, output_count) = counts(48, 38, rate);
        let sampler = RampSampler::new(ramp, 48, 38, rate, input_count, output_count).unwrap();
        let input = (0..input_count)
            .map(|k| {
                let value = ((k * 7919 % 1000) as f32 / 500.0) - 1.0;
                [value, value * 0.5]
            })
            .collect::<Vec<_>>();
        let whole = sampler.render(0, output_count as usize, 0, &input).unwrap();
        let mut blocks = Vec::new();
        let mut at = 0_i64;
        for size in [1_usize, 255, 256, 17, 4096].iter().cycle() {
            if at >= i64::from(output_count) {
                break;
            }
            let frames = (*size).min((i64::from(output_count) - at) as usize);
            let need = sampler.required_input(at, at + frames as i64).unwrap();
            let window = &input[need.start as usize..need.end as usize];
            blocks.extend(sampler.render(at, frames, need.start, window).unwrap());
            at += frames as i64;
        }
        assert_eq!(whole, blocks);
        // A smooth input gives a smooth output: no second-difference spike at
        // any block seam beyond what the interior shows.
        let tone = (0..input_count)
            .map(|k| {
                let value = (f64::from(k) * 0.01).sin() as f32;
                [value, value]
            })
            .collect::<Vec<_>>();
        let smooth = sampler.render(0, output_count as usize, 0, &tone).unwrap();
        let interior = &smooth[2_000..smooth.len() - 2_000];
        let curvature = interior
            .windows(3)
            .map(|w| (w[0][0] - 2.0 * w[1][0] + w[2][0]).abs())
            .fold(0.0_f32, f32::max);
        assert!(curvature < 0.01, "curvature {curvature}");
    }
}
