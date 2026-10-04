//! Speech activity and pauses in the Original's audio.
//!
//! A voice activity detector (Silero through whisper.cpp) reports one speech
//! probability per 512 analysis samples (32 ms at 16 kHz). The host measures
//! the energy of every 10 ms of the same analysis PCM. Both are stored,
//! quantized, so a pause rule can be re-run exactly on any later read. Pauses
//! are proposals; they never edit the project.
//!
//! [`SILENCE_RULE`] names the rule [`SpeechActivity::pauses`] applies:
//!
//! 1. Hysteresis over the probabilities: a hop starts speech at 0.5 or more and
//!    speech continues until a hop falls below 0.35. Every other hop is quiet.
//! 2. Each maximal run of quiet hops is a candidate. Its noise floor is the
//!    20th percentile of the 10 ms energies that lie wholly inside it.
//! 3. The detector's 32 ms hops are coarse, so the edges are refined by energy:
//!    each side first retreats over frames louder than the floor plus 12 dB
//!    (the end of a word inside a quiet hop), then advances into the adjoining
//!    speech over at most six frames (60 ms) no louder than the floor plus
//!    6 dB; Silero marks speech ending 30–50 ms after its energy falls.
//! 4. A refined candidate lasting at least 150 ms is a pause; one that
//!    advanced over a speech blip into the previous pause merges with it. Pauses between
//!    phrases in conversational speech measure about 180–220 ms.

use deadpan_core::{ExactRatio, TimeError};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::ANALYSIS_SAMPLE_RATE;

/// The versioned pause rule implemented by [`SpeechActivity::pauses`].
pub const SILENCE_RULE: &str = "deadpan-silence-1";
/// Analysis samples per detector probability.
pub const VAD_HOP: u64 = 512;
/// Analysis samples per energy measurement (10 ms).
pub const ENERGY_HOP: u64 = 160;
/// Three hours of analysis PCM.
pub const MAX_ACTIVITY_SAMPLES: u64 = 3 * 60 * 60 * ANALYSIS_SAMPLE_RATE as u64;
/// Shortest pause, in analysis samples (150 ms).
pub const MIN_PAUSE_SAMPLES: u64 = ANALYSIS_SAMPLE_RATE as u64 * 3 / 20;
/// Quantized energy of digital silence; one step is half a decibel above
/// -120 dBFS.
pub const ENERGY_FLOOR_DB: f64 = -120.0;

const SPEECH_START: u8 = 128; // 0.5 · 255, rounded
const SPEECH_CONTINUE: u8 = 90; // 0.35 · 255, rounded up
const RETREAT_ABOVE_STEPS: i32 = 24; // 12 dB
const ADVANCE_WITHIN_STEPS: i32 = 12; // 6 dB
const ADVANCE_FRAMES: u64 = 6;

#[derive(Debug, Error, PartialEq)]
pub enum ActivityError {
    #[error("speech activity exceeds its {0} limit")]
    Limit(&'static str),
    #[error("speech activity is invalid: {0}")]
    Invalid(&'static str),
    #[error(transparent)]
    Time(#[from] TimeError),
}

/// The Original audio that analysis PCM covers, in analysis samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivityAudio {
    /// Original audio sample at which the analysis PCM begins.
    pub origin: i64,
    /// The Original audio stream's sample rate.
    pub sample_rate: u32,
    /// Analysis PCM length in 16 kHz samples.
    pub samples: u64,
}

/// A pause: a half-open interval of analysis samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pause {
    pub start: u64,
    pub end: u64,
}

/// Validated, quantized detector probabilities and energies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeechActivity {
    audio: ActivityAudio,
    /// One probability per [`VAD_HOP`] samples, `round(p · 255)`.
    speech: Vec<u8>,
    /// One energy per [`ENERGY_HOP`] samples, half-decibel steps above
    /// [`ENERGY_FLOOR_DB`], at most 240 (0 dBFS).
    energy: Vec<u8>,
}

impl SpeechActivity {
    /// Restore stored values, checking every bound.
    pub fn new(
        audio: ActivityAudio,
        speech: Vec<u8>,
        energy: Vec<u8>,
    ) -> Result<Self, ActivityError> {
        if audio.sample_rate == 0 {
            return Err(ActivityError::Invalid("zero sample rate"));
        }
        if audio.samples > MAX_ACTIVITY_SAMPLES {
            return Err(ActivityError::Limit("duration"));
        }
        if speech.len() as u64 != audio.samples.div_ceil(VAD_HOP) {
            return Err(ActivityError::Invalid(
                "probability count differs from the audio",
            ));
        }
        if energy.len() as u64 != audio.samples.div_ceil(ENERGY_HOP) {
            return Err(ActivityError::Invalid(
                "energy count differs from the audio",
            ));
        }
        if energy.iter().any(|value| *value > 240) {
            return Err(ActivityError::Invalid("energy above 0 dBFS"));
        }
        Ok(Self {
            audio,
            speech,
            energy,
        })
    }

    /// Quantize detector probabilities and measure the energy of the analysis
    /// PCM they were computed from.
    pub fn measure(
        audio: ActivityAudio,
        pcm: &[f32],
        probabilities: &[f32],
    ) -> Result<Self, ActivityError> {
        if pcm.len() as u64 != audio.samples {
            return Err(ActivityError::Invalid("PCM length differs from the audio"));
        }
        if probabilities.len() as u64 > MAX_ACTIVITY_SAMPLES.div_ceil(VAD_HOP) {
            return Err(ActivityError::Limit("probability count"));
        }
        let speech = probabilities
            .iter()
            .map(|probability| {
                if probability.is_finite() && (0.0..=1.0).contains(probability) {
                    Ok((probability * 255.0).round() as u8)
                } else {
                    Err(ActivityError::Invalid("probability outside 0..=1"))
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        let energy = pcm
            .chunks(ENERGY_HOP as usize)
            .map(quantized_energy)
            .collect::<Result<Vec<_>, _>>()?;
        Self::new(audio, speech, energy)
    }

    pub fn audio(&self) -> ActivityAudio {
        self.audio
    }

    pub fn speech(&self) -> &[u8] {
        &self.speech
    }

    pub fn energy(&self) -> &[u8] {
        &self.energy
    }

    /// Pauses under [`SILENCE_RULE`], in order.
    pub fn pauses(&self) -> Vec<Pause> {
        let mut pauses = Vec::new();
        let mut speaking = false;
        let mut quiet_from: Option<u64> = None;
        for (hop, probability) in self.speech.iter().enumerate() {
            let hop = hop as u64;
            speaking = if speaking {
                *probability >= SPEECH_CONTINUE
            } else {
                *probability >= SPEECH_START
            };
            match (speaking, quiet_from) {
                (false, None) => quiet_from = Some(hop),
                (true, Some(first)) => {
                    quiet_from = None;
                    push(&mut pauses, self.refine(first, hop));
                }
                _ => {}
            }
        }
        if let Some(first) = quiet_from {
            push(&mut pauses, self.refine(first, self.speech.len() as u64));
        }
        pauses
    }

    /// Refine the quiet hops `first..end` by energy; `None` when too short.
    fn refine(&self, first: u64, end: u64) -> Option<Pause> {
        let samples = self.audio.samples;
        let start = first * VAD_HOP;
        let end = (end * VAD_HOP).min(samples);
        // Energy frames wholly inside the candidate.
        let inner_first = start.div_ceil(ENERGY_HOP);
        let inner_end = if end == samples {
            samples.div_ceil(ENERGY_HOP)
        } else {
            end / ENERGY_HOP
        };
        if inner_first >= inner_end {
            return None;
        }
        let frame = |index: u64| i32::from(self.energy[index as usize]);
        let mut inside: Vec<u8> = (inner_first..inner_end)
            .map(|index| self.energy[index as usize])
            .collect();
        inside.sort_unstable();
        let floor = i32::from(inside[(inside.len() - 1) / 5]);

        // Retreat over loud frames at each edge.
        let mut low = inner_first;
        while low < inner_end && frame(low) > floor + RETREAT_ABOVE_STEPS {
            low += 1;
        }
        let mut high = inner_end;
        while high > low && frame(high - 1) > floor + RETREAT_ABOVE_STEPS {
            high -= 1;
        }
        if low >= high {
            return None;
        }
        // Advance into adjoining speech over near-floor frames, but only from
        // an unretreated edge.
        let (mut start, mut end) = (low * ENERGY_HOP, (high * ENERGY_HOP).min(samples));
        if low == inner_first {
            let mut steps = 0;
            while steps < ADVANCE_FRAMES
                && start >= ENERGY_HOP
                && frame(start / ENERGY_HOP - 1) <= floor + ADVANCE_WITHIN_STEPS
            {
                start -= ENERGY_HOP;
                steps += 1;
            }
        }
        if high == inner_end && end < samples {
            let mut steps = 0;
            let frames = self.energy.len() as u64;
            while steps < ADVANCE_FRAMES
                && end % ENERGY_HOP == 0
                && end / ENERGY_HOP < frames
                && frame(end / ENERGY_HOP) <= floor + ADVANCE_WITHIN_STEPS
            {
                end = (end + ENERGY_HOP).min(samples);
                steps += 1;
            }
        }
        (end - start >= MIN_PAUSE_SAMPLES).then_some(Pause { start, end })
    }

    /// Container time in seconds of an analysis sample: the Original audio
    /// sample `origin + k · rate / 16000`, divided by the rate.
    pub fn seconds(&self, sample: u64) -> Result<ExactRatio, ActivityError> {
        let origin = ExactRatio::new(
            i128::from(self.audio.origin),
            i128::from(self.audio.sample_rate),
        )?;
        let offset = ExactRatio::new(i128::from(sample), i128::from(ANALYSIS_SAMPLE_RATE))?;
        Ok(origin.checked_add(offset)?)
    }
}

/// Append a pause, merging it with the previous one when refinement advanced
/// across a speech blip whose frames sit at the noise floor.
fn push(pauses: &mut Vec<Pause>, pause: Option<Pause>) {
    let Some(pause) = pause else { return };
    match pauses.last_mut() {
        Some(last) if pause.start <= last.end => last.end = last.end.max(pause.end),
        _ => pauses.push(pause),
    }
}

/// Half-decibel energy steps of one 10 ms frame above -120 dBFS.
fn quantized_energy(frame: &[f32]) -> Result<u8, ActivityError> {
    let mut sum = 0.0_f64;
    for sample in frame {
        if !sample.is_finite() {
            return Err(ActivityError::Invalid("non-finite PCM"));
        }
        sum += f64::from(*sample) * f64::from(*sample);
    }
    let mean = sum / frame.len() as f64;
    if mean <= 0.0 {
        return Ok(0);
    }
    let decibels = 10.0 * mean.log10();
    Ok(((decibels - ENERGY_FLOOR_DB) * 2.0)
        .round()
        .clamp(0.0, 240.0) as u8)
}

#[cfg(test)]
mod tests;
