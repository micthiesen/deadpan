//! The versioned stereo speaker policy, independent of source sample rate.

use deadpan_media::audio_index::AudioChannelLayout;

use crate::{MAX_INPUT_MAGNITUDE, PreparationError};

// FFmpeg 8.0.3 AVChannel identities. Native interleaved order is ascending bit
// order, including the two unsupported front-of-center positions at bits 6/7.
const FRONT_LEFT: u64 = 1 << 0;
const FRONT_RIGHT: u64 = 1 << 1;
const FRONT_CENTER: u64 = 1 << 2;
const LOW_FREQUENCY: u64 = 1 << 3;
const BACK_LEFT: u64 = 1 << 4;
const BACK_RIGHT: u64 = 1 << 5;
const BACK_CENTER: u64 = 1 << 8;
const SIDE_LEFT: u64 = 1 << 9;
const SIDE_RIGHT: u64 = 1 << 10;
const SPEAKERS: [u64; 9] = [
    FRONT_LEFT,
    FRONT_RIGHT,
    FRONT_CENTER,
    LOW_FREQUENCY,
    BACK_LEFT,
    BACK_RIGHT,
    BACK_CENTER,
    SIDE_LEFT,
    SIDE_RIGHT,
];
const SUPPORTED_MASK: u64 = FRONT_LEFT
    | FRONT_RIGHT
    | FRONT_CENTER
    | LOW_FREQUENCY
    | BACK_LEFT
    | BACK_RIGHT
    | BACK_CENTER
    | SIDE_LEFT
    | SIDE_RIGHT;

/// Explicit fixed stereo routing. This does not infer a layout, normalize gain,
/// clip peaks, or perform bass management. Original multichannel PCM is retained
/// by the media layer; omitted LFE applies only to this stereo interpretation.
#[derive(Debug, Clone, PartialEq)]
pub struct StereoMatrix {
    layout: AudioChannelLayout,
    coefficients: Box<[[f64; 2]]>,
}

impl StereoMatrix {
    /// Admit a native layout containing only the documented speaker identities.
    /// Native center-only mono is duplicated at unity. A center in a larger
    /// layout and each side/back speaker contribute at -3.0103 dB; back center
    /// contributes at -6.0206 dB to each side. LFE contributes nothing.
    pub fn new(layout: AudioChannelLayout) -> Result<Self, PreparationError> {
        let AudioChannelLayout::Native { channels, mask } = layout else {
            return Err(PreparationError::UnsupportedLayout);
        };
        if !(1..=32).contains(&channels)
            || mask.count_ones() != channels
            || mask & !SUPPORTED_MASK != 0
            || mask & !LOW_FREQUENCY == 0
        {
            return Err(PreparationError::UnsupportedLayout);
        }

        let coefficients = SPEAKERS
            .into_iter()
            .filter(|speaker| mask & speaker != 0)
            .map(|speaker| match speaker {
                FRONT_LEFT => [1.0, 0.0],
                FRONT_RIGHT => [0.0, 1.0],
                FRONT_CENTER if mask == FRONT_CENTER => [1.0, 1.0],
                FRONT_CENTER => [std::f64::consts::FRAC_1_SQRT_2; 2],
                BACK_LEFT | SIDE_LEFT => [std::f64::consts::FRAC_1_SQRT_2, 0.0],
                BACK_RIGHT | SIDE_RIGHT => [0.0, std::f64::consts::FRAC_1_SQRT_2],
                BACK_CENTER => [0.5, 0.5],
                _ => [0.0, 0.0], // The only remaining admitted identity is LFE.
            })
            .collect();
        Ok(Self {
            layout,
            coefficients,
        })
    }

    pub fn layout(&self) -> AudioChannelLayout {
        self.layout
    }

    /// Validate every channel, including omitted LFE, before exposing a result.
    /// Accumulation follows native input order in f64 with fixed coefficients.
    pub(crate) fn mix_frame(&self, frame: &[f32]) -> Result<[f64; 2], PreparationError> {
        if frame.len() != self.coefficients.len() {
            return Err(PreparationError::InvalidSamples);
        }
        let mut stereo = [0.0; 2];
        for (&sample, coefficients) in frame.iter().zip(&self.coefficients) {
            if !sample.is_finite() || sample.abs() > MAX_INPUT_MAGNITUDE {
                return Err(PreparationError::InvalidSamples);
            }
            stereo[0] += f64::from(sample) * coefficients[0];
            stereo[1] += f64::from(sample) * coefficients[1];
        }
        Ok(stereo)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matrix(mask: u64) -> StereoMatrix {
        StereoMatrix::new(AudioChannelLayout::Native {
            channels: mask.count_ones(),
            mask,
        })
        .unwrap()
    }

    fn assert_close(actual: [f64; 2], expected: [f64; 2]) {
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert!((actual - expected).abs() < 1e-14, "{actual} != {expected}");
        }
    }

    #[test]
    fn native_channel_impulses_follow_speaker_bits_with_holes() {
        // FL, FR, FC, LFE, BL, BR, BC, SL, SR. Bits 6 and 7 are absent,
        // so BC is interleaved channel 6 rather than channel 8.
        let matrix = matrix(0x73f);
        let half_power = 1.0 / 2.0_f64.sqrt();
        let expected = [
            [1.0, 0.0],
            [0.0, 1.0],
            [half_power, half_power],
            [0.0, 0.0],
            [half_power, 0.0],
            [0.0, half_power],
            [0.5, 0.5],
            [half_power, 0.0],
            [0.0, half_power],
        ];
        for (channel, expected) in expected.into_iter().enumerate() {
            let mut frame = [0.0; 9];
            frame[channel] = 1.0;
            assert_close(matrix.mix_frame(&frame).unwrap(), expected);
        }
        assert_eq!(
            matrix.layout(),
            AudioChannelLayout::Native {
                channels: 9,
                mask: 0x73f
            }
        );
    }

    #[test]
    fn sparse_surround_layout_uses_native_interleaved_order() {
        // Standard side 5.1: FL, FR, FC, LFE, SL, SR, without back channels.
        let matrix = matrix(0x60f);
        assert_close(
            matrix
                .mix_frame(&[0.25, -0.5, 1.0, 8.0, 2.0, -4.0])
                .unwrap(),
            [2.371_320_343_559_643, -2.621_320_343_559_643],
        );
    }

    #[test]
    fn mono_and_stereo_retain_unity_gain_and_channel_separation() {
        let mono = matrix(0x4);
        let stereo = matrix(0x3);
        for sample in [0.0, 0.125, -0.25, 1.0, -16.0, 16.0] {
            assert_eq!(mono.mix_frame(&[sample]).unwrap(), [f64::from(sample); 2]);
            assert_eq!(
                stereo.mix_frame(&[sample, -sample]).unwrap(),
                [f64::from(sample), f64::from(-sample)]
            );
        }
        // A multichannel center is not treated as center-only mono.
        assert_close(
            matrix(0x7).mix_frame(&[0.0, 0.0, 1.0]).unwrap(),
            [1.0 / 2.0_f64.sqrt(); 2],
        );
    }

    #[test]
    fn ambiguous_invalid_and_unsupported_layouts_fail_explicitly() {
        for channels in [0, 1, 2, 6, 32, u32::MAX] {
            assert!(matches!(
                StereoMatrix::new(AudioChannelLayout::Unspecified { channels }),
                Err(PreparationError::UnsupportedLayout)
            ));
        }
        for (channels, mask) in [
            (0, 0),
            (1, 0),
            (2, 4),
            (1, 3),
            (1, 8), // LFE alone has no stereo interpretation.
            (1, 1 << 6),
            (2, 1 | (1 << 7)),
            (1, 1 << 11),
            (1, 1 << 63),
            (33, (1 << 33) - 1),
            (64, u64::MAX),
        ] {
            assert!(matches!(
                StereoMatrix::new(AudioChannelLayout::Native { channels, mask }),
                Err(PreparationError::UnsupportedLayout)
            ));
        }
    }

    #[test]
    fn every_supported_non_lfe_speaker_can_be_interpreted() {
        for speaker in [1, 2, 4, 16, 32, 256, 512, 1024] {
            let isolated = matrix(speaker);
            let result = isolated.mix_frame(&[1.0]).unwrap();
            assert!(result[0] > 0.0 || result[1] > 0.0);
            assert_eq!(isolated.layout().channels(), 1);
        }
    }

    #[test]
    fn malformed_pcm_and_invalid_omitted_lfe_are_rejected() {
        let matrix = matrix(0xf);
        for frame in [vec![], vec![0.0], vec![0.0; 3], vec![0.0; 5]] {
            assert!(matches!(
                matrix.mix_frame(&frame),
                Err(PreparationError::InvalidSamples)
            ));
        }
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 16.001, -16.001] {
            for channel in 0..4 {
                let mut frame = [0.0; 4];
                frame[channel] = invalid;
                assert!(matches!(
                    matrix.mix_frame(&frame),
                    Err(PreparationError::InvalidSamples)
                ));
            }
        }
        assert_eq!(matrix.mix_frame(&[0.0, 0.0, 0.0, 16.0]).unwrap(), [0.0; 2]);
    }

    #[test]
    fn fixed_matrix_preserves_dynamics_without_normalizing_or_clipping() {
        let matrix = matrix(0x73f);
        let quiet = matrix.mix_frame(&[0.125; 9]).unwrap();
        let louder = matrix.mix_frame(&[0.25; 9]).unwrap();
        let loudest = matrix.mix_frame(&[1.0; 9]).unwrap();
        assert_eq!(louder, quiet.map(|value| value * 2.0));
        assert_eq!(loudest, quiet.map(|value| value * 8.0));
        assert!(loudest[0] > 3.6 && loudest[1] > 3.6);
        assert_eq!(
            matrix.mix_frame(&[-1.0; 9]).unwrap(),
            loudest.map(|value| -value)
        );
    }
}
