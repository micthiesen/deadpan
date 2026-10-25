use crate::{
    AlignError, Control, GLOBAL_STATS_SHA256, MAX_FEATURE_FRAMES, MAX_SAMPLES, MEL_BINS,
    assembly::verify_hash,
};
use rustfft::{FftPlanner, num_complex::Complex32};

const HANN: &[u8] = include_bytes!("../assets/hann.f32le");
const MEL: &[u8] = include_bytes!("../assets/mel-filter.f32le");
const STATS_MAX_BYTES: usize = 16_384;

/// Verified original global statistics. These learned values live in the
/// installed model pack, never in the executable or a derived persistent cache.
#[derive(Clone, Debug)]
pub struct Normalization {
    mean: [f32; MEL_BINS],
    inverse_stddev: [f32; MEL_BINS],
}

impl Normalization {
    pub fn from_json(bytes: &[u8]) -> Result<Self, AlignError> {
        if bytes.len() > STATS_MAX_BYTES || !verify_hash(bytes, GLOBAL_STATS_SHA256) {
            return Err(AlignError::Integrity(
                "original global statistics SHA256 or size",
            ));
        }
        let value: serde_json::Value = serde_json::from_slice(bytes)
            .map_err(|_| AlignError::Input("global statistics JSON"))?;
        let read = |name: &str| -> Result<[f32; MEL_BINS], AlignError> {
            let array = value
                .get(name)
                .and_then(serde_json::Value::as_array)
                .filter(|v| v.len() == MEL_BINS)
                .ok_or(AlignError::Input("global statistics dimensions"))?;
            let mut result = [0_f32; MEL_BINS];
            for (destination, source) in result.iter_mut().zip(array) {
                *destination = source
                    .as_f64()
                    .ok_or(AlignError::Input("global statistics number"))?
                    as f32;
            }
            Ok(result)
        };
        Self::validated(read("mean")?, read("invstddev")?)
    }

    /// Byte-exact reference vectors, intended for feature parity qualification.
    pub fn from_f32le(mean: &[u8], inverse_stddev: &[u8]) -> Result<Self, AlignError> {
        if !verify_hash(
            mean,
            "c709c46f355ff99ced996a70938b7b38ddbc337d436c283e4e6e78cb008cb1bb",
        ) || !verify_hash(
            inverse_stddev,
            "4b565c3c2267a8a2a0e67201d10fd7153c4d30f087bb50508d5427841270cd31",
        ) {
            return Err(AlignError::Integrity("global statistics vectors SHA256"));
        }
        Self::validated(read_array(mean)?, read_array(inverse_stddev)?)
    }

    fn validated(
        mean: [f32; MEL_BINS],
        inverse_stddev: [f32; MEL_BINS],
    ) -> Result<Self, AlignError> {
        if mean.iter().any(|v| !v.is_finite())
            || inverse_stddev.iter().any(|v| !v.is_finite() || *v <= 0.0)
        {
            return Err(AlignError::Input("nonfinite or invalid global statistics"));
        }
        Ok(Self {
            mean,
            inverse_stddev,
        })
    }
}

fn read_array<const N: usize>(bytes: &[u8]) -> Result<[f32; N], AlignError> {
    if bytes.len() != N * 4 {
        return Err(AlignError::Input("float32 vector dimensions"));
    }
    let mut result = [0_f32; N];
    for (destination, chunk) in result.iter_mut().zip(bytes.chunks_exact(4)) {
        *destination = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
    }
    Ok(result)
}

/// Owned [1, F, 80] contiguous float32 model input.
#[derive(Clone, Debug)]
pub struct Features {
    frames: usize,
    values: Vec<f32>,
}

impl Features {
    /// Admit an already computed feature tensor for reference comparisons.
    /// Production callers use [`extract_features`] from their captured PCM.
    pub fn from_values(frames: usize, values: Vec<f32>) -> Result<Self, AlignError> {
        if !(2..=MAX_FEATURE_FRAMES).contains(&frames)
            || frames.checked_mul(MEL_BINS) != Some(values.len())
            || values.iter().any(|v| !v.is_finite())
        {
            return Err(AlignError::Input("feature tensor shape or finite values"));
        }
        Ok(Self { frames, values })
    }

    pub fn frames(&self) -> usize {
        self.frames
    }
    pub fn values(&self) -> &[f32] {
        &self.values
    }
}

/// Frozen deterministic TorchAudio-compatible preprocessing at 16 kHz.
/// FFT and platform logarithm rounding are qualified numerically, not promised
/// bit-identical to PyTorch. No waveform normalization is performed.
pub fn extract_features(
    pcm: &[f32],
    stats: &Normalization,
    control: &Control,
) -> Result<Features, AlignError> {
    control.check()?;
    if !(401..=MAX_SAMPLES).contains(&pcm.len()) || pcm.iter().any(|v| !v.is_finite()) {
        return Err(AlignError::Input(
            "mono 16 kHz PCM must contain 401..480000 finite samples",
        ));
    }
    if !verify_hash(
        HANN,
        "c692d19f0b2bd745f0dfc967261c3e3d21a2473e485a677d219e9b4a73e0218e",
    ) || !verify_hash(
        MEL,
        "69a241210255f24e37926762346701739943709f3179732f821f0823605fe6d7",
    ) {
        return Err(AlignError::Integrity(
            "compiled mathematical feature constants",
        ));
    }
    let hann: [f32; 400] = read_array(HANN)?;
    let mel: [f32; 16080] = read_array(MEL)?;
    let frames = pcm.len() / 160 + 1;
    let mut values = Vec::with_capacity(frames * MEL_BINS);
    let fft = FftPlanner::<f32>::new().plan_fft_forward(400);
    let mut window = [Complex32::new(0.0, 0.0); 400];
    let mut scratch = vec![Complex32::new(0.0, 0.0); fft.get_inplace_scratch_len()];
    let mut power = [0_f32; 201];
    for frame in 0..frames {
        control.check()?;
        for (i, slot) in window.iter_mut().enumerate() {
            // N>400 and only 200 samples of padding make one reflection enough.
            let centered = (frame * 160 + i) as isize - 200;
            let index = if centered < 0 {
                (-centered) as usize
            } else if centered as usize >= pcm.len() {
                2 * pcm.len() - 2 - centered as usize
            } else {
                centered as usize
            };
            *slot = Complex32::new(pcm[index] * hann[i], 0.0);
        }
        fft.process_with_scratch(&mut window, &mut scratch);
        for (destination, complex) in power.iter_mut().zip(&window) {
            *destination = complex.re * complex.re + complex.im * complex.im;
        }
        for bin in 0..MEL_BINS {
            let mut value = 0_f32;
            for frequency in 0..201 {
                value += power[frequency] * mel[frequency * MEL_BINS + bin];
            }
            value *= 1_073_676_288_f32;
            // The SECOND mask deliberately sees the mutated logarithm.
            if value > std::f32::consts::E {
                value = value.ln();
            }
            if value <= std::f32::consts::E {
                value /= std::f32::consts::E;
            }
            value = (value - stats.mean[bin]) * stats.inverse_stddev[bin];
            if !value.is_finite() {
                return Err(AlignError::Input("feature transform overflow"));
            }
            values.push(value);
        }
    }
    control.check()?;
    Features::from_values(frames, values)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_bad_statistics_and_features() {
        assert!(Normalization::from_json(b"{}").is_err());
        assert!(Normalization::from_f32le(&[0; 320], &[0; 320]).is_err());
        assert!(Normalization::validated([0.; 80], [0.; 80]).is_err());
        assert!(Normalization::validated([f32::NAN; 80], [1.; 80]).is_err());
        assert!(Features::from_values(3002, vec![]).is_err());
        assert!(Features::from_values(3, vec![f32::INFINITY; 240]).is_err());
    }
}
