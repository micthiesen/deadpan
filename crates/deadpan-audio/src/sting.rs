//! The bundled "wrongly triumphant" sting (specification §8.3 "Wrongly
//! triumphant sting"): an original brass-like fanfare synthesized here, so
//! Deadpan ships no third-party sound. Three short pickup notes on G4 lead to
//! a held C major chord with a slow vibrato and release. It is written as a
//! 48 kHz stereo 16-bit PCM WAV and imported like any user sound, so placing,
//! moving, gain and export use the ordinary catalog-sound path.

/// The sting's sample rate.
pub const STING_SAMPLE_RATE: u32 = 48_000;
/// The sting's length in samples (1.8 s).
pub const STING_FRAMES: usize = 86_400;
/// The catalog label of the imported sting.
pub const STING_LABEL: &str = "Triumphant sting";
/// The file name the app writes before importing it, so the catalog shows
/// it by name; `STING_VERSION` names the synthesis recipe's directory, so a
/// new recipe never replaces an imported one silently.
pub const STING_FILE_NAME: &str = "Triumphant sting.wav";
/// The recipe version, naming the directory the app writes the file into.
pub const STING_VERSION: &str = "sting-v1";

/// One note: frequency, start and end in seconds, and stereo position
/// (-1 left, 1 right).
const NOTES: [(f64, f64, f64, f64); 6] = [
    (392.00, 0.00, 0.12, 0.0),
    (392.00, 0.14, 0.26, 0.0),
    (392.00, 0.28, 0.40, 0.0),
    (523.25, 0.44, 1.62, 0.0),
    (659.25, 0.44, 1.62, -0.35),
    (783.99, 0.44, 1.62, 0.35),
];

/// The sting as interleaved stereo samples in [-1, 1], peaking at -3 dBFS.
pub fn triumphant_sting() -> Vec<[f64; 2]> {
    let rate = f64::from(STING_SAMPLE_RATE);
    let mut out = vec![[0.0_f64; 2]; STING_FRAMES];
    for (frequency, start, end, pan) in NOTES {
        let first = (start * rate).round() as usize;
        let last = ((end + RELEASE) * rate).round().min(STING_FRAMES as f64) as usize;
        let (left, right) = (((1.0 - pan) / 2.0).sqrt(), ((1.0 + pan) / 2.0).sqrt());
        let mut phase = 0.0_f64;
        for (index, frame) in out.iter_mut().enumerate().take(last).skip(first) {
            let t = (index - first) as f64 / rate;
            // A gentle vibrato once the chord has sounded for 0.3 s.
            let vibrato = if t > 0.3 {
                1.0 + 0.004 * ((t - 0.3) * 5.5 * std::f64::consts::TAU).sin()
            } else {
                1.0
            };
            phase += frequency * vibrato / rate;
            phase -= phase.floor();
            // Brass-like: odd and even harmonics falling off, brighter on attack.
            let brightness = 1.0 + 0.6 * (-t / 0.08).exp();
            let mut value = 0.0;
            for harmonic in 1..=6 {
                let k = f64::from(harmonic);
                value += (phase * k * std::f64::consts::TAU).sin()
                    / k.powf(1.6 - 0.4 * (brightness - 1.0));
            }
            let level = envelope(t, end - start) * value;
            frame[0] += level * left;
            frame[1] += level * right;
        }
    }
    let peak = out
        .iter()
        .flat_map(|frame| frame.iter())
        .fold(0.0_f64, |peak, sample| peak.max(sample.abs()));
    // -3 dBFS keeps the sting below the limiter ceiling before user gain.
    let scale = if peak > 0.0 {
        0.707_945_784 / peak
    } else {
        0.0
    };
    for frame in &mut out {
        frame[0] *= scale;
        frame[1] *= scale;
    }
    out
}

const ATTACK: f64 = 0.012;
const DECAY: f64 = 0.08;
const SUSTAIN: f64 = 0.75;
const RELEASE: f64 = 0.15;

/// Attack, decay to sustain, then a release after the note's length.
fn envelope(t: f64, length: f64) -> f64 {
    let held = if t < ATTACK {
        t / ATTACK
    } else if t < ATTACK + DECAY {
        1.0 - (1.0 - SUSTAIN) * (t - ATTACK) / DECAY
    } else {
        SUSTAIN
    };
    if t <= length {
        held
    } else {
        (held * (1.0 - (t - length) / RELEASE)).max(0.0)
    }
}

/// The sting as a canonical 48 kHz stereo 16-bit PCM WAV file.
pub fn triumphant_sting_wav() -> Vec<u8> {
    let samples = triumphant_sting();
    let data_bytes = (samples.len() * 4) as u32;
    let mut wav = Vec::with_capacity(44 + data_bytes as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&2_u16.to_le_bytes()); // channels
    wav.extend_from_slice(&STING_SAMPLE_RATE.to_le_bytes());
    wav.extend_from_slice(&(STING_SAMPLE_RATE * 4).to_le_bytes());
    wav.extend_from_slice(&4_u16.to_le_bytes()); // block align
    wav.extend_from_slice(&16_u16.to_le_bytes()); // bits
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_bytes.to_le_bytes());
    for frame in samples {
        for sample in frame {
            let value = (sample * 32_767.0).round().clamp(-32_768.0, 32_767.0) as i16;
            wav.extend_from_slice(&value.to_le_bytes());
        }
    }
    wav
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sting_is_a_bounded_fanfare_ending_in_silence() {
        let samples = triumphant_sting();
        assert_eq!(samples.len(), STING_FRAMES);
        let peak = samples
            .iter()
            .flat_map(|frame| frame.iter())
            .fold(0.0_f64, |peak, sample| peak.max(sample.abs()));
        assert!((peak - 0.707_945_784).abs() < 1e-9, "{peak}");
        // Three pickups with silence between them, then the held chord.
        let rms = |from: f64, to: f64| {
            let range = (from * 48_000.0) as usize..(to * 48_000.0) as usize;
            let count = range.len() as f64;
            (samples[range]
                .iter()
                .map(|frame| frame[0] * frame[0])
                .sum::<f64>()
                / count)
                .sqrt()
        };
        assert!(rms(0.02, 0.10) > 0.05);
        assert!(rms(0.70, 1.40) > rms(0.02, 0.10), "the chord is louder");
        assert!(rms(1.78, 1.80) < 1e-9, "it ends in exact silence");
        // The chord's voices spread across both channels.
        let balance: f64 = samples[48_000..60_000]
            .iter()
            .map(|frame| frame[0].abs() - frame[1].abs())
            .sum();
        assert!(balance.abs() < 1_000.0);
        let wav = triumphant_sting_wav();
        assert_eq!(wav.len(), 44 + STING_FRAMES * 4);
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(wav, triumphant_sting_wav(), "deterministic");
        // Golden bytes: a change to the recipe must change STING_VERSION too,
        // so an imported sting is never silently replaced.
        use sha2::Digest;
        let digest = sha2::Sha256::digest(&wav);
        let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
        if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            assert_eq!(hex, GOLDEN_SHA256, "update STING_VERSION with a new recipe");
        }
    }

    /// Measured on Apple Silicon macOS; `f64::sin` and `powf` come from the
    /// platform libm, so another platform may need its own reference.
    const GOLDEN_SHA256: &str = "1162335b6a17ae9f56347a90dae68c4a1d541761d330e8157b9fd49d0c175597";
}
