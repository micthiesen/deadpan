//! Deterministic Hold audio recipes that transform an explicitly selected,
//! already prepared 48 kHz source: reversal and hanging effect tails.
//!
//! Both run once over the complete Hold on a preparation worker and are cached
//! as one canonical block, so a read that starts mid-Hold hears exactly the
//! samples a read from its start would. Preview and export share this code.

use std::sync::atomic::AtomicBool;

use deadpan_core::TailEffect;

use crate::{MAX_INPUT_MAGNITUDE, MAX_OUTPUT_FRAMES, PreparationError, check_cancel};

pub const REVERSE_ID: &str = "deadpan-reverse-sample-order-v1";
pub const TAIL_REVERB_ID: &str = "deadpan-tail-reverb-stereo-comb8-allpass4-v1";
pub const TAIL_DELAY_ID: &str = "deadpan-tail-delay-300ms-half-v1";

const MAX_INPUT_FRAMES: usize = 1_048_576;
const MAX_HOLD_FRAMES: u32 = 8_388_608;
/// The ring's end fades linearly to exact zero over at most 50 ms.
const TAIL_FADE_FRAMES: u32 = 2_400;

/// The stable algorithm identity of a tail effect, part of its cache key and
/// its documented qualification.
pub fn tail_effect_id(effect: TailEffect) -> &'static str {
    match effect {
        TailEffect::Reverb => TAIL_REVERB_ID,
        TailEffect::Delay => TAIL_DELAY_ID,
    }
}

fn validate_input(source: &[[f32; 2]], cancelled: &AtomicBool) -> Result<(), PreparationError> {
    if source.is_empty() || source.len() > MAX_INPUT_FRAMES {
        return Err(PreparationError::InvalidRecipe(
            "Hold effect source must contain 1..1048576 frames",
        ));
    }
    for chunk in source.chunks(MAX_OUTPUT_FRAMES as usize) {
        check_cancel(cancelled)?;
        if chunk
            .iter()
            .flatten()
            .any(|sample| !sample.is_finite() || sample.abs() > MAX_INPUT_MAGNITUDE)
        {
            return Err(PreparationError::InvalidSamples);
        }
    }
    Ok(())
}

fn validate_output(output_frames: u32) -> Result<(), PreparationError> {
    if output_frames == 0 || output_frames > MAX_HOLD_FRAMES {
        return Err(PreparationError::InvalidRecipe(
            "Hold effect output must contain 1..8388608 frames",
        ));
    }
    Ok(())
}

/// The source in reverse sample order, then exact digital silence. Output
/// sample `n` is source sample `len - 1 - n`: a sample-centered reversal that
/// neither repeats nor drops a sample. Nothing is normalized or faded here;
/// shared edge processing owns the Hold's boundaries.
pub fn reverse(
    source: &[[f32; 2]],
    output_frames: u32,
    cancelled: &AtomicBool,
) -> Result<Vec<[f32; 2]>, PreparationError> {
    validate_input(source, cancelled)?;
    validate_output(output_frames)?;
    let mut output = Vec::with_capacity(output_frames as usize);
    for chunk in source.rchunks(MAX_OUTPUT_FRAMES as usize) {
        check_cancel(cancelled)?;
        let room = output_frames as usize - output.len();
        output.extend(chunk.iter().rev().take(room));
        if output.len() == output_frames as usize {
            break;
        }
    }
    output.resize(output_frames as usize, [0.0; 2]);
    check_cancel(cancelled)?;
    Ok(output)
}

/// A sine of `frequency_hz` at `level_millidecibels` below full scale for
/// `output_frames`, with linear 2 ms ramps (shorter for tiny Holds) to exact
/// zero at both ends. The phase of sample `n` is `n * f mod 48000` in exact
/// integers, so no rounding accumulates however long the tone runs.
pub fn tone(
    frequency_hz: u32,
    level_millidecibels: i32,
    output_frames: u32,
    cancelled: &AtomicBool,
) -> Result<Vec<[f32; 2]>, PreparationError> {
    validate_output(output_frames)?;
    if !(20..=20_000).contains(&frequency_hz) || level_millidecibels > 0 {
        return Err(PreparationError::InvalidRecipe(
            "a tone is 20 Hz to 20 kHz at or below full scale",
        ));
    }
    let amplitude = 10.0_f64.powf(f64::from(level_millidecibels) / 20_000.0);
    let frames = output_frames as usize;
    let ramp = TONE_RAMP_FRAMES.min(frames / 2).max(1);
    let mut output = Vec::with_capacity(frames);
    while output.len() < frames {
        check_cancel(cancelled)?;
        let end = (output.len() + MAX_OUTPUT_FRAMES as usize).min(frames);
        for index in output.len()..end {
            let cycle = (index as u64 * u64::from(frequency_hz)) % 48_000;
            let phase = std::f64::consts::TAU * cycle as f64 / 48_000.0;
            let edge = index.min(frames - 1 - index);
            let gain = if edge < ramp {
                edge as f64 / ramp as f64
            } else {
                1.0
            };
            let value = (amplitude * gain * phase.sin()) as f32;
            output.push([value, value]);
        }
    }
    Ok(output)
}

/// 2 ms at 48 kHz.
const TONE_RAMP_FRAMES: usize = 96;
pub const TONE_ID: &str = "deadpan-tone-sine-exact-phase-2ms-ramps-v1";

/// A hanging tail: the effect's wet output after its complete source input
/// has ended, `ring_frames` long, then exact silence up to `output_frames`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TailRecipe {
    effect: TailEffect,
    ring_frames: u32,
    output_frames: u32,
}

impl TailRecipe {
    pub fn new(
        effect: TailEffect,
        ring_frames: u32,
        output_frames: u32,
    ) -> Result<Self, PreparationError> {
        validate_output(output_frames)?;
        if ring_frames == 0 || ring_frames > output_frames {
            return Err(PreparationError::InvalidRecipe(
                "tail ring must contain 1..output frames",
            ));
        }
        Ok(Self {
            effect,
            ring_frames,
            output_frames,
        })
    }

    pub fn effect(&self) -> TailEffect {
        self.effect
    }

    pub fn ring_frames(&self) -> u32 {
        self.ring_frames
    }

    pub fn output_frames(&self) -> u32 {
        self.output_frames
    }
}

/// Feed the whole source through the effect, keep only what rings after it
/// ends, and fade that ring to exact zero at its end. The dry source is never
/// part of the output. All state is `f64` in a fixed evaluation order, so the
/// result is identical for every caller.
pub fn render_tail(
    recipe: TailRecipe,
    source: &[[f32; 2]],
    cancelled: &AtomicBool,
) -> Result<Vec<[f32; 2]>, PreparationError> {
    validate_input(source, cancelled)?;
    let mut effect: Box<dyn Effect> = match recipe.effect {
        TailEffect::Reverb => Box::new(Reverb::new()),
        TailEffect::Delay => Box::new(Delay::new()),
    };
    for chunk in source.chunks(MAX_OUTPUT_FRAMES as usize) {
        check_cancel(cancelled)?;
        for frame in chunk {
            effect.process([f64::from(frame[0]), f64::from(frame[1])]);
        }
    }
    let ring = recipe.ring_frames as usize;
    let fade = TAIL_FADE_FRAMES.min(recipe.ring_frames / 2).max(1) as usize;
    let mut output = Vec::with_capacity(recipe.output_frames as usize);
    while output.len() < ring {
        check_cancel(cancelled)?;
        let end = (output.len() + MAX_OUTPUT_FRAMES as usize).min(ring);
        for index in output.len()..end {
            let wet = effect.process([0.0; 2]);
            // Linear fade whose last ring sample is exactly zero.
            let remaining = ring - 1 - index;
            let gain = if remaining < fade {
                remaining as f64 / fade as f64
            } else {
                1.0
            };
            output.push([(wet[0] * gain) as f32, (wet[1] * gain) as f32]);
        }
    }
    output.resize(recipe.output_frames as usize, [0.0; 2]);
    check_cancel(cancelled)?;
    Ok(output)
}

trait Effect {
    /// One stereo frame in, the wet stereo frame out.
    fn process(&mut self, input: [f64; 2]) -> [f64; 2];
}

/// Flush subnormal recursion state so long decays stay fast and exact zero
/// input eventually yields exact zero.
fn flush(value: f64) -> f64 {
    if value.abs() < 1.0e-30 { 0.0 } else { value }
}

struct DelayLine {
    buffer: Vec<f64>,
    index: usize,
}

impl DelayLine {
    fn new(length: usize) -> Self {
        Self {
            buffer: vec![0.0; length],
            index: 0,
        }
    }

    fn read(&self) -> f64 {
        self.buffer[self.index]
    }

    fn write_advance(&mut self, value: f64) {
        self.buffer[self.index] = flush(value);
        self.index = (self.index + 1) % self.buffer.len();
    }
}

struct Comb {
    line: DelayLine,
    store: f64,
}

impl Comb {
    fn process(&mut self, input: f64) -> f64 {
        let output = self.line.read();
        self.store = flush(output * (1.0 - REVERB_DAMP) + self.store * REVERB_DAMP);
        self.line
            .write_advance(input + self.store * REVERB_FEEDBACK);
        output
    }
}

struct AllPass {
    line: DelayLine,
}

impl AllPass {
    fn process(&mut self, input: f64) -> f64 {
        let delayed = self.line.read();
        self.line.write_advance(input + delayed * 0.5);
        delayed - input
    }
}

// Schroeder/Moorer comb and all-pass lengths at 44.1 kHz, scaled to 48 kHz.
const COMB_LENGTHS: [usize; 8] = [1215, 1293, 1390, 1476, 1548, 1623, 1695, 1760];
const ALLPASS_LENGTHS: [usize; 4] = [605, 480, 371, 245];
/// The right channel's lines are longer by this many samples for width.
const STEREO_SPREAD: usize = 25;
const REVERB_FEEDBACK: f64 = 0.84;
const REVERB_DAMP: f64 = 0.2;
/// Twice the classic mono-sum input gain, since each channel feeds alone.
const REVERB_INPUT_GAIN: f64 = 0.03;

/// A deterministic parallel-comb, series-all-pass room reverb with about
/// 1.1 s of decay to -60 dB. Only the wet signal is returned.
struct Reverb {
    combs: [Vec<Comb>; 2],
    allpasses: [Vec<AllPass>; 2],
}

impl Reverb {
    fn new() -> Self {
        let channel = |spread: usize| {
            (
                COMB_LENGTHS
                    .iter()
                    .map(|length| Comb {
                        line: DelayLine::new(length + spread),
                        store: 0.0,
                    })
                    .collect(),
                ALLPASS_LENGTHS
                    .iter()
                    .map(|length| AllPass {
                        line: DelayLine::new(length + spread),
                    })
                    .collect(),
            )
        };
        let (left_combs, left_allpasses) = channel(0);
        let (right_combs, right_allpasses) = channel(STEREO_SPREAD);
        Self {
            combs: [left_combs, right_combs],
            allpasses: [left_allpasses, right_allpasses],
        }
    }
}

impl Effect for Reverb {
    fn process(&mut self, input: [f64; 2]) -> [f64; 2] {
        // Each channel feeds its own network, so opposite-polarity content
        // (common in stereo recordings) does not cancel before the reverb.
        std::array::from_fn(|channel| {
            let fed = input[channel] * REVERB_INPUT_GAIN;
            let mut sum = 0.0;
            for comb in &mut self.combs[channel] {
                sum += comb.process(fed);
            }
            for allpass in &mut self.allpasses[channel] {
                sum = allpass.process(sum);
            }
            sum
        })
    }
}

/// 300 ms at 48 kHz.
const DELAY_FRAMES: usize = 14_400;
const DELAY_FEEDBACK: f64 = 0.5;

/// A feedback echo: each repeat arrives 300 ms after the last, 6 dB quieter.
struct Delay {
    lines: [DelayLine; 2],
}

impl Delay {
    fn new() -> Self {
        Self {
            lines: [DelayLine::new(DELAY_FRAMES), DelayLine::new(DELAY_FRAMES)],
        }
    }
}

impl Effect for Delay {
    fn process(&mut self, input: [f64; 2]) -> [f64; 2] {
        std::array::from_fn(|channel| {
            let line = &mut self.lines[channel];
            let delayed = line.read();
            line.write_advance(input[channel] + delayed * DELAY_FEEDBACK);
            delayed
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn never() -> AtomicBool {
        AtomicBool::new(false)
    }

    #[test]
    fn reverse_plays_each_sample_once_in_reverse_order_then_silence() {
        let source = (0..5)
            .map(|n| [n as f32 / 8.0, -(n as f32) / 8.0])
            .collect::<Vec<_>>();
        let output = reverse(&source, 7, &never()).unwrap();
        assert_eq!(
            output,
            vec![
                [0.5, -0.5],
                [0.375, -0.375],
                [0.25, -0.25],
                [0.125, -0.125],
                [0.0, 0.0],
                [0.0, 0.0],
                [0.0, 0.0]
            ]
        );
        // A shorter Hold hears the end of the source first.
        assert_eq!(
            reverse(&source, 2, &never()).unwrap(),
            source[3..].iter().rev().copied().collect::<Vec<_>>()
        );
        // Reversal crosses block boundaries without reordering whole blocks.
        let long = (0..1000)
            .map(|n| [n as f32 / 1000.0, 0.0])
            .collect::<Vec<_>>();
        let output = reverse(&long, 1000, &never()).unwrap();
        assert!(
            output
                .iter()
                .enumerate()
                .all(|(n, frame)| frame[0] == (999 - n) as f32 / 1000.0)
        );
    }

    #[test]
    fn reverse_rejects_invalid_samples_and_empty_output() {
        assert!(reverse(&[[f32::NAN, 0.0]], 1, &never()).is_err());
        assert!(reverse(&[[0.0, 0.0]], 0, &never()).is_err());
        assert!(reverse(&[], 1, &never()).is_err());
    }

    fn impulse(frames: usize) -> Vec<[f32; 2]> {
        let mut source = vec![[0.0; 2]; frames];
        source[0] = [0.5, 0.5];
        source
    }

    #[test]
    fn delay_tail_repeats_every_300ms_at_half_level_after_the_source() {
        // The impulse is 100 ms before the source ends, so its first echo
        // lands 200 ms into the ring at its own level, and each next echo
        // 300 ms later at half the level.
        let source = impulse(4_800);
        let recipe = TailRecipe::new(TailEffect::Delay, 48_000, 60_000).unwrap();
        let output = render_tail(recipe, &source, &never()).unwrap();
        assert_eq!(output.len(), 60_000);
        let peaks = output
            .iter()
            .enumerate()
            .filter(|(_, frame)| frame[0] != 0.0)
            .map(|(index, frame)| (index, frame[0]))
            .collect::<Vec<_>>();
        assert_eq!(peaks, vec![(9_600, 0.5), (24_000, 0.25), (38_400, 0.125)]);
        // Exact silence after the ring.
        assert!(output[48_000..].iter().all(|frame| *frame == [0.0; 2]));
    }

    #[test]
    fn reverb_tail_rings_decays_and_ends_in_exact_silence() {
        let source = (0..24_000)
            .map(|n| {
                let value = (n as f32 * 0.05).sin() * 0.5;
                [value, value]
            })
            .collect::<Vec<_>>();
        let recipe = TailRecipe::new(TailEffect::Reverb, 24_000, 48_000).unwrap();
        let output = render_tail(recipe, &source, &never()).unwrap();
        let energy = |range: std::ops::Range<usize>| {
            output[range]
                .iter()
                .map(|frame| f64::from(frame[0]).powi(2) + f64::from(frame[1]).powi(2))
                .sum::<f64>()
        };
        let early = energy(0..4_800);
        let late = energy(14_400..19_200);
        assert!(early > 0.0, "the tail must ring");
        assert!(late < early, "the tail must decay");
        // The two channels differ: the reverb has stereo width.
        assert!(output[..4_800].iter().any(|frame| frame[0] != frame[1]));
        assert_eq!(output[23_999], [0.0; 2]);
        assert!(output[24_000..].iter().all(|frame| *frame == [0.0; 2]));
        // Deterministic: the same input renders the same bytes.
        assert_eq!(output, render_tail(recipe, &source, &never()).unwrap());
    }

    /// Seconds for the reverb's impulse response, in 10 ms energy blocks,
    /// to fall 60 dB below its loudest block.
    fn reverb_rt60() -> f64 {
        let mut source = vec![[0.0; 2]; 48];
        source[0] = [0.5, 0.5];
        let recipe = TailRecipe::new(TailEffect::Reverb, 240_000, 240_000).unwrap();
        let ring = render_tail(recipe, &source, &never()).unwrap();
        let blocks: Vec<f64> = ring[..230_000]
            .chunks(480)
            .map(|block| block.iter().map(|frame| f64::from(frame[0]).powi(2)).sum())
            .collect();
        let peak = blocks.iter().copied().fold(0.0, f64::max);
        let loudest = blocks.iter().position(|energy| *energy == peak).unwrap();
        let quiet = blocks[loudest..]
            .iter()
            .position(|energy| *energy < peak * 1e-6)
            .unwrap();
        quiet as f64 * 0.01
    }

    #[test]
    fn reverb_decays_sixty_decibels_in_about_one_point_one_seconds() {
        let rt60 = reverb_rt60();
        // Measured 1.09 s; the documented "about 1.1 s" must stay true.
        assert!((1.0..1.2).contains(&rt60), "{rt60}");
    }

    #[test]
    fn tail_recipe_bounds_its_ring() {
        assert!(TailRecipe::new(TailEffect::Reverb, 0, 10).is_err());
        assert!(TailRecipe::new(TailEffect::Reverb, 11, 10).is_err());
        assert!(TailRecipe::new(TailEffect::Delay, 10, 10).is_ok());
    }

    #[test]
    fn tones_hold_their_frequency_level_and_exact_zero_ends() {
        let output = tone(1_000, -6_000, 4_800, &never()).unwrap();
        assert_eq!(output.len(), 4_800);
        assert_eq!(output[0], [0.0; 2]);
        assert_eq!(output[4_799], [0.0; 2]);
        let peak = output
            .iter()
            .map(|frame| frame[0].abs())
            .fold(0.0_f32, f32::max);
        assert!((peak - 0.501).abs() < 0.002, "{peak}");
        // One cycle every 48 samples: the steady part repeats exactly.
        assert_eq!(output[200], output[248]);
        assert!(output.iter().all(|frame| frame[0] == frame[1]));
        assert!(tone(10, 0, 10, &never()).is_err());
        assert!(tone(1_000, 1, 10, &never()).is_err());
    }

    #[test]
    fn cancellation_stops_both_recipes() {
        let cancelled = AtomicBool::new(true);
        assert!(matches!(
            reverse(&impulse(10), 10, &cancelled),
            Err(PreparationError::Cancelled)
        ));
        assert!(matches!(
            render_tail(
                TailRecipe::new(TailEffect::Reverb, 10, 10).unwrap(),
                &impulse(10),
                &cancelled
            ),
            Err(PreparationError::Cancelled)
        ));
    }
}
