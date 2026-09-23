//! Retained fade progress on the output sample clock, independent of allocation.

use std::ops::Range;

use deadpan_core::AudioSample;
use serde::Serialize;

use crate::PlanError;

/// Samples outside the retained half-open envelope contribute no audio. A Hard
/// edge disables a fade inside this domain, never extends the domain itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioEnvelopeEndpoint {
    Silence,
}

/// A sample-centered envelope keeps its full original length when its audio is
/// partitioned or shifted. Progress advances by one per output sample. This
/// value does not allocate samples or author a new fade at an inserted seam.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct AudioEnvelope {
    length: u64,
    #[serde(serialize_with = "serialize_progress")]
    progress_at_anchor: i128,
    anchor: AudioSample,
    endpoint: AudioEnvelopeEndpoint,
}

impl AudioEnvelope {
    /// Length is positive and can span the full distance between signed sample
    /// endpoints. Signed progress is
    /// deliberately allowed outside `[0, length)`; its endpoint policy is silence.
    pub fn new(
        length: u64,
        progress_at_anchor: i128,
        anchor: AudioSample,
    ) -> Result<Self, PlanError> {
        if length == 0 {
            return Err(PlanError::AudioRangeOutOfRange);
        }
        Ok(Self {
            length,
            progress_at_anchor,
            anchor,
            endpoint: AudioEnvelopeEndpoint::Silence,
        })
    }

    /// Construct the ordinary, unshifted envelope from its full sample domain.
    pub fn from_samples(samples: Range<AudioSample>) -> Result<Self, PlanError> {
        let length = i128::from(samples.end.0) - i128::from(samples.start.0);
        let length = u64::try_from(length).map_err(|_| PlanError::AudioRangeOutOfRange)?;
        Self::new(length, 0, samples.start)
    }

    pub const fn length(self) -> u64 {
        self.length
    }

    pub const fn endpoint(self) -> AudioEnvelopeEndpoint {
        self.endpoint
    }

    /// Calculate before narrowing, so distant signed anchors may cancel with
    /// retained progress without overflowing an intermediate subtraction.
    pub fn progress_at(self, sample: AudioSample) -> Result<i128, PlanError> {
        self.progress_at_anchor
            .checked_add(i128::from(sample.0) - i128::from(self.anchor.0))
            .ok_or(PlanError::AudioRangeOutOfRange)
    }

    /// Resume the old sample's exact progress at a new output sample. Keep the
    /// old length and endpoint; a new allocation must never resize the fade.
    pub fn reanchor(
        self,
        old_sample: AudioSample,
        new_sample: AudioSample,
    ) -> Result<Self, PlanError> {
        Self::new(self.length, self.progress_at(old_sample)?, new_sample)
    }
}

// Like ExactRatio, preserve exact inspection beyond JavaScript/u64 integers.
fn serialize_progress<S: serde::Serializer>(
    progress: &i128,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.collect_str(progress)
}

#[cfg(test)]
mod tests {
    use deadpan_core::{FrameRate, ProjectFrame};

    use super::*;

    #[test]
    fn ntsc_resume_retains_the_old_domain_instead_of_the_longer_allocation() {
        let rate = FrameRate::new(30_000, 1001).unwrap();
        let boundary = |frame| rate.audio_boundary(ProjectFrame(frame)).unwrap();
        assert_eq!(boundary(1), AudioSample(1602));
        assert_eq!(boundary(2), AudioSample(3203));
        assert_eq!(boundary(3), AudioSample(4805));
        let old = AudioEnvelope::from_samples(boundary(0)..boundary(2)).unwrap();
        let resumed = old.reanchor(boundary(1), boundary(2)).unwrap();
        assert_eq!(resumed.length(), 3203);
        assert_eq!(resumed.progress_at(boundary(2)).unwrap(), 1602);
        assert_eq!(resumed.progress_at(AudioSample(4803)).unwrap(), 3202);
        assert_eq!(resumed.progress_at(AudioSample(4804)).unwrap(), 3203);
        assert_eq!(resumed.endpoint(), AudioEnvelopeEndpoint::Silence);
        let inspection = serde_json::to_value(resumed).unwrap();
        assert_eq!(
            inspection,
            serde_json::json!({
                "length": 3203,
                "progress_at_anchor": "1602",
                "anchor": 3203,
                "endpoint": "silence"
            })
        );
    }

    #[test]
    fn signed_anchors_and_repeated_resume_preserve_progress_without_accumulated_rounding() {
        for old_anchor in [i64::MIN, -3203, 0, i64::MAX - 2] {
            for new_anchor in [i64::MIN, -1602, 0, i64::MAX - 2] {
                let old = AudioEnvelope::new(3, -1, AudioSample(old_anchor)).unwrap();
                let resumed = old
                    .reanchor(AudioSample(old_anchor + 1), AudioSample(new_anchor))
                    .unwrap();
                assert_eq!(resumed.progress_at(AudioSample(new_anchor)).unwrap(), 0);
                assert_eq!(resumed.progress_at(AudioSample(new_anchor + 2)).unwrap(), 2);
                assert_eq!(resumed.length(), 3);
                let again = resumed
                    .reanchor(AudioSample(new_anchor + 1), AudioSample(old_anchor))
                    .unwrap();
                assert_eq!(again.progress_at(AudioSample(old_anchor + 1)).unwrap(), 2);
            }
        }
        // The difference itself exceeds i64, but the final progress fits.
        let across = AudioEnvelope::new(1, i128::from(i64::MIN), AudioSample(i64::MIN)).unwrap();
        assert_eq!(
            across.progress_at(AudioSample(i64::MAX)).unwrap(),
            i128::from(i64::MAX)
        );
        assert!(
            across
                .reanchor(AudioSample(i64::MAX), AudioSample(i64::MIN))
                .is_ok()
        );
        let backwards = AudioEnvelope::new(1, i128::from(i64::MAX), AudioSample(i64::MAX)).unwrap();
        assert_eq!(
            backwards.progress_at(AudioSample(i64::MIN)).unwrap(),
            i128::from(i64::MIN)
        );
    }

    #[test]
    fn invalid_lengths_and_unrepresentable_progress_fail_without_clamping() {
        assert!(AudioEnvelope::new(0, 0, AudioSample(0)).is_err());
        let wide =
            AudioEnvelope::from_samples(AudioSample(i64::MIN)..AudioSample(i64::MAX)).unwrap();
        assert_eq!(wide.length(), u64::MAX);
        assert_eq!(
            wide.progress_at(AudioSample(i64::MAX - 1)).unwrap(),
            i128::from(u64::MAX - 1)
        );
        let resumed = wide
            .reanchor(AudioSample(i64::MAX - 1), AudioSample(0))
            .unwrap();
        assert_eq!(
            resumed.progress_at(AudioSample(1)).unwrap(),
            i128::from(u64::MAX)
        );
        assert_eq!(
            serde_json::to_value(resumed).unwrap()["progress_at_anchor"],
            (u64::MAX - 1).to_string()
        );
        for (start, end) in [(0, 0), (1, 0)] {
            assert!(AudioEnvelope::from_samples(AudioSample(start)..AudioSample(end)).is_err());
        }
        for (progress, sample) in [(i128::MAX, 1), (i128::MIN, -1)] {
            let envelope = AudioEnvelope::new(1, progress, AudioSample(0)).unwrap();
            assert!(envelope.progress_at(AudioSample(sample)).is_err());
            assert!(
                envelope
                    .reanchor(AudioSample(sample), AudioSample(0))
                    .is_err()
            );
        }
        assert_eq!(
            AudioEnvelope::from_samples(AudioSample(i64::MIN)..AudioSample(-1))
                .unwrap()
                .length(),
            i64::MAX as u64
        );
    }
}
