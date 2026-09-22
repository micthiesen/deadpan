//! Deterministic room-tone loops over an explicitly selected, prepared source.
//! The exact source extent sets the overlap period; storage rounding does not.

use std::sync::atomic::AtomicBool;

use deadpan_core::{AudioSample, ExactRatio, TimeError};
use deadpan_media::audio_index::AudioChannelLayout;

use crate::{
    MAX_INPUT_MAGNITUDE, MAX_OUTPUT_FRAMES, PcmWindow, PreparationError, ResampleRecipe, Resampler,
    StereoBlock, StereoMatrix, check_cancel,
};

pub const ROOM_TONE_ID: &str = "deadpan-room-tone-exact-linear-overlap-v1";
const MAX_INPUT_FRAMES: i128 = 1_048_576;
const MAX_HOLD_FRAMES: u32 = 8_388_608;
const CROSSFADE_FRAMES: i64 = 96;

/// A 48 kHz source extent and an independent output allocation. The first pass
/// starts at the selected head. Later passes overlap the preceding tail using
/// complementary linear weights, preserving level without normalization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomToneRecipe {
    source_extent: ExactRatio,
    input_frames: usize,
    output_frames: u32,
    crossfade: ExactRatio,
    period: ExactRatio,
}

impl RoomToneRecipe {
    pub fn new(source_extent: ExactRatio, output_frames: u32) -> Result<Self, PreparationError> {
        if source_extent.numerator() <= 0 || source_extent.ceil()? > MAX_INPUT_FRAMES {
            return Err(PreparationError::InvalidRecipe(
                "room-tone source must contain 1..1048576 stored frames",
            ));
        }
        if output_frames == 0 || output_frames > MAX_HOLD_FRAMES {
            return Err(PreparationError::InvalidRecipe(
                "room-tone output must contain 1..8388608 frames",
            ));
        }
        let crossfade = if source_extent.compare_integer(2 * CROSSFADE_FRAMES).is_lt() {
            source_extent.checked_div(ExactRatio::integer(2))?
        } else {
            ExactRatio::integer(CROSSFADE_FRAMES)
        };
        Ok(Self {
            source_extent,
            input_frames: usize::try_from(source_extent.ceil()?)
                .map_err(|_| TimeError::Overflow)?,
            output_frames,
            crossfade,
            period: source_extent.checked_sub(crossfade)?,
        })
    }

    pub fn source_extent(&self) -> ExactRatio {
        self.source_extent
    }

    pub fn crossfade(&self) -> ExactRatio {
        self.crossfade
    }

    pub fn period(&self) -> ExactRatio {
        self.period
    }

    pub fn output_frames(&self) -> u32 {
        self.output_frames
    }

    fn phase(&self, at: AudioSample) -> Result<(ExactRatio, bool), PreparationError> {
        let at = u128::try_from(at.0)
            .map_err(|_| PreparationError::InvalidRecipe("negative room-tone output coordinate"))?;
        let numerator = self.period.numerator() as u128;
        let denominator = self.period.denominator() as u128;
        let remainder = multiply_modulo(at, denominator, numerator);
        Ok((
            ExactRatio::new(remainder as i128, self.period.denominator())?,
            self.period.compare_integer(at as i64).is_gt(),
        ))
    }
}

/// Borrows already prepared stereo PCM and validates it once. Construction and
/// rendering belong on the preparation worker, never on a device callback.
pub struct RoomTone<'a> {
    recipe: RoomToneRecipe,
    source: &'a [[f32; 2]],
    matrix: StereoMatrix,
}

impl<'a> RoomTone<'a> {
    pub fn new(
        recipe: RoomToneRecipe,
        source: &'a [[f32; 2]],
        cancelled: &AtomicBool,
    ) -> Result<Self, PreparationError> {
        check_cancel(cancelled)?;
        if source.len() != recipe.input_frames {
            return Err(PreparationError::InvalidSamples);
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
        Ok(Self {
            recipe,
            source,
            matrix: StereoMatrix::new(AudioChannelLayout::Native {
                channels: 2,
                mask: 3,
            })?,
        })
    }

    pub fn render(
        &self,
        start: AudioSample,
        frames: u32,
        cancelled: &AtomicBool,
    ) -> Result<StereoBlock, PreparationError> {
        check_cancel(cancelled)?;
        let end = start
            .0
            .checked_add(i64::from(frames))
            .ok_or(TimeError::Overflow)?;
        if start.0 < 0
            || frames == 0
            || frames > MAX_OUTPUT_FRAMES
            || end > i64::from(self.recipe.output_frames)
        {
            return Err(PreparationError::InvalidRecipe(
                "room-tone block outside its allocation or 1..256 frame budget",
            ));
        }
        let mut samples = Vec::with_capacity(frames as usize);
        let mut at = start;
        while at.0 < end {
            check_cancel(cancelled)?;
            let (phase, first_pass) = self.recipe.phase(at)?;
            let blending = !first_pass && phase.checked_sub(self.recipe.crossfade)?.numerator() < 0;
            let edge = if blending {
                self.recipe.crossfade
            } else {
                self.recipe.period
            };
            // A tiny period can skip many complete cycles between two integer
            // points. Compute each next point's phase directly; never enumerate
            // those empty cycles or accumulate rounded loop lengths.
            let count = u32::try_from(edge.checked_sub(phase)?.ceil()?.min(i128::from(end - at.0)))
                .map_err(|_| TimeError::Overflow)?;
            let head = self.sample(phase, at, count, cancelled)?;
            if blending {
                let tail =
                    self.sample(self.recipe.period.checked_add(phase)?, at, count, cancelled)?;
                for (offset, (head, tail)) in head.into_iter().zip(tail).enumerate() {
                    let weight = phase
                        .checked_add(ExactRatio::integer(offset as i64))?
                        .checked_div(self.recipe.crossfade)?;
                    let weight = weight.numerator() as f64 / weight.denominator() as f64;
                    samples.push(std::array::from_fn(|channel| {
                        (f64::from(tail[channel]) * (1.0 - weight)
                            + f64::from(head[channel]) * weight) as f32
                    }));
                }
            } else {
                samples.extend(head);
            }
            at.0 += i64::from(count);
        }
        check_cancel(cancelled)?;
        Ok(StereoBlock { start, samples })
    }

    fn sample(
        &self,
        source_origin: ExactRatio,
        start: AudioSample,
        frames: u32,
        cancelled: &AtomicBool,
    ) -> Result<Vec<[f32; 2]>, PreparationError> {
        let sampler = Resampler::new(
            ResampleRecipe::new(
                0..self.source.len() as i64,
                source_origin,
                start,
                ExactRatio::ONE,
                start..AudioSample(start.0 + i64::from(frames)),
            )?,
            self.matrix.clone(),
        );
        let window = sampler
            .required_source_range(start, frames)?
            .map(|range| {
                let selected = self
                    .source
                    .get(range.start as usize..range.end as usize)
                    .ok_or(PreparationError::InvalidSamples)?;
                Ok::<_, PreparationError>(PcmWindow {
                    start: range.start,
                    samples: selected.iter().flatten().copied().collect(),
                })
            })
            .transpose()?;
        Ok(sampler.render(start, frames, window, cancelled)?.samples)
    }
}

/// All operands are nonnegative and the modulus is at most i128::MAX. The
/// fallback therefore adds two reduced operands safely in u128. Its work is
/// bounded by the integer coordinate's bits, independent of skipped cycles.
fn multiply_modulo(mut a: u128, mut b: u128, modulus: u128) -> u128 {
    if let Some(product) = a.checked_mul(b) {
        return product % modulus;
    }
    b %= modulus;
    let mut result = 0;
    while a != 0 {
        if a & 1 != 0 {
            result = (result + b) % modulus;
        }
        a >>= 1;
        b = (b + b) % modulus;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ratio(numerator: i128, denominator: i128) -> ExactRatio {
        ExactRatio::new(numerator, denominator).unwrap()
    }

    fn source(frames: usize) -> Vec<[f32; 2]> {
        (0..frames)
            .map(|n| {
                let n = n as f32;
                [(n * 0.037).sin() * 0.7, (n * 0.011).cos() * 0.3]
            })
            .collect()
    }

    fn collect(room: &RoomTone<'_>, count: u32, partitions: &[u32]) -> Vec<[f32; 2]> {
        let mut result = Vec::new();
        let cancel = AtomicBool::new(false);
        let mut next = 0;
        while result.len() < count as usize {
            let block = partitions[next % partitions.len()].min(count - result.len() as u32);
            result.extend(
                room.render(AudioSample(result.len() as i64), block, &cancel)
                    .unwrap()
                    .samples,
            );
            next += 1;
        }
        result
    }

    // Independent scalar phase and weight arithmetic. This intentionally shares
    // only the separately qualified sampler, not loop segmentation/modulo code.
    fn oracle(source: &[[f32; 2]], extent: ExactRatio, at: i64) -> [f32; 2] {
        let fade = if extent.compare_integer(192).is_lt() {
            extent.checked_div(ExactRatio::integer(2)).unwrap()
        } else {
            ExactRatio::integer(96)
        };
        let period = extent.checked_sub(fade).unwrap();
        let cycle = ExactRatio::integer(at).checked_div(period).unwrap().floor();
        let phase = ExactRatio::integer(at)
            .checked_sub(period.checked_mul(ratio(cycle, 1)).unwrap())
            .unwrap();
        let sample = |point| {
            let sampler = Resampler::new(
                ResampleRecipe::new(
                    0..source.len() as i64,
                    point,
                    AudioSample(0),
                    ExactRatio::ONE,
                    AudioSample(0)..AudioSample(1),
                )
                .unwrap(),
                StereoMatrix::new(AudioChannelLayout::Native {
                    channels: 2,
                    mask: 3,
                })
                .unwrap(),
            );
            let window = sampler
                .required_source_range(AudioSample(0), 1)
                .unwrap()
                .map(|range| PcmWindow {
                    start: range.start,
                    samples: source[range.start as usize..range.end as usize]
                        .iter()
                        .flatten()
                        .copied()
                        .collect(),
                });
            sampler
                .render(AudioSample(0), 1, window, &AtomicBool::new(false))
                .unwrap()
                .samples[0]
        };
        let head = sample(phase);
        if cycle == 0 || phase.checked_sub(fade).unwrap().numerator() >= 0 {
            return head;
        }
        let tail = sample(period.checked_add(phase).unwrap());
        let weight = phase.checked_div(fade).unwrap();
        let weight = weight.numerator() as f64 / weight.denominator() as f64;
        std::array::from_fn(|c| {
            (f64::from(tail[c]) * (1.0 - weight) + f64::from(head[c]) * weight) as f32
        })
    }

    #[test]
    fn unity_weights_preserve_dc_and_levels_without_normalizing() {
        let source = vec![[1.75, -0.125]; 512];
        let room = RoomTone::new(
            RoomToneRecipe::new(ExactRatio::integer(512), 5000).unwrap(),
            &source,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(
            collect(&room, 5000, &[256, 17, 1])
                .iter()
                .all(|sample| *sample == [1.75, -0.125])
        );
    }

    #[test]
    fn integer_seam_follows_complementary_linear_overlap() {
        let source = source(512);
        let room = RoomTone::new(
            RoomToneRecipe::new(ExactRatio::integer(512), 2000).unwrap(),
            &source,
            &AtomicBool::new(false),
        )
        .unwrap();
        let rendered = collect(&room, 2000, &[256]);
        assert_eq!(rendered[415], source[415]);
        assert_eq!(rendered[416], source[416]);
        assert_eq!(rendered[512], source[96]);
        for n in [417, 421, 464, 511, 832, 900, 1665] {
            assert_eq!(
                rendered[n],
                oracle(&source, ExactRatio::integer(512), n as i64)
            );
        }
    }

    #[test]
    fn fractional_44100_extent_keeps_exact_period_and_partition_identity() {
        let extent = ratio(1001 * 160, 147);
        let source = source(extent.ceil().unwrap() as usize);
        let recipe = RoomToneRecipe::new(extent, 7000).unwrap();
        assert_eq!(recipe.period(), ratio(146048, 147));
        let room = RoomTone::new(recipe, &source, &AtomicBool::new(false)).unwrap();
        let rendered = collect(&room, 7000, &[256]);
        assert_eq!(rendered, collect(&room, 7000, &[1, 17, 255, 63, 2]));
        for n in [993, 994, 1000, 1089, 1987, 2981, 5000, 6999] {
            assert_eq!(rendered[n], oracle(&source, extent, n as i64));
        }
        let rounded = oracle(&source, ExactRatio::integer(source.len() as i64), 5000);
        assert_ne!(rendered[5000], rounded);
        assert_eq!(
            room.render(AudioSample(2929), 123, &AtomicBool::new(false))
                .unwrap()
                .samples,
            rendered[2929..3052]
        );
    }

    #[test]
    fn tiny_fragments_shorten_fades_and_skip_unobserved_cycles() {
        for extent in [ratio(15, 2), ratio(2, 5), ratio(1, 1_000_003)] {
            let source = source(extent.ceil().unwrap() as usize);
            let recipe = RoomToneRecipe::new(extent, 1024).unwrap();
            assert_eq!(
                recipe.crossfade(),
                extent.checked_div(ExactRatio::integer(2)).unwrap()
            );
            let room = RoomTone::new(recipe, &source, &AtomicBool::new(false)).unwrap();
            let rendered = collect(&room, 1024, &[256]);
            assert_eq!(rendered, collect(&room, 1024, &[3, 99, 1]));
            for n in [0, 1, 3, 4, 7, 17, 255, 1023] {
                assert_eq!(rendered[n], oracle(&source, extent, n as i64));
            }
        }
    }

    #[test]
    fn phase_modulo_remains_exact_beyond_float_integer_precision() {
        let recipe = RoomToneRecipe::new(ratio(1001 * 160, 147), 1).unwrap();
        let at = (1_i64 << 60) + 17;
        let expected = ratio((i128::from(at) * 147) % 146048, 147);
        assert_eq!(recipe.phase(AudioSample(at)).unwrap(), (expected, false));
        // Exercise the bounded overflow fallback without requiring an enormous
        // loop count or a representable `at / period` intermediate.
        assert_eq!(
            multiply_modulo(1 << 63, (i128::MAX - 1) as u128, i128::MAX as u128),
            i128::MAX as u128 - (1 << 63)
        );
    }

    #[test]
    fn rejects_invalid_extents_allocations_pcm_and_reads_and_cancels() {
        for extent in [
            ExactRatio::ZERO,
            ExactRatio::integer(-1),
            ratio(2_097_153, 2),
        ] {
            assert!(RoomToneRecipe::new(extent, 1).is_err());
        }
        for frames in [0, 8_388_609] {
            assert!(RoomToneRecipe::new(ExactRatio::ONE, frames).is_err());
        }
        let recipe = RoomToneRecipe::new(ExactRatio::ONE, 8).unwrap();
        let cancelled = AtomicBool::new(false);
        for source in [
            vec![],
            vec![[0.0; 2]; 2],
            vec![[f32::NAN, 0.0]],
            vec![[0.0, 16.1]],
        ] {
            assert!(matches!(
                RoomTone::new(recipe.clone(), &source, &cancelled),
                Err(PreparationError::InvalidSamples)
            ));
        }
        let source = [[0.25; 2]];
        let room = RoomTone::new(recipe.clone(), &source, &cancelled).unwrap();
        for (start, count) in [(-1, 1), (0, 0), (0, 257), (8, 1), (i64::MAX, 1)] {
            assert!(room.render(AudioSample(start), count, &cancelled).is_err());
        }
        let cancelled = AtomicBool::new(true);
        assert!(matches!(
            RoomTone::new(recipe, &source, &cancelled),
            Err(PreparationError::Cancelled)
        ));
        assert!(matches!(
            room.render(AudioSample(0), 1, &cancelled),
            Err(PreparationError::Cancelled)
        ));
    }
}
