//! Stateless, per-voice fades on the final allocated 48 kHz output clock.
use deadpan_core::AudioEdgePolicy;
use deadpan_plan::AudioSpan;

use crate::StageAudioError;

/// Sample-centered linear edges after all time/pitch mapping, before treatments.
pub const EDGE_FADE_ID: &str = "deadpan-voice-edge-sample-centered-linear-2ms-v1";

pub(crate) fn apply_edge_fades(
    span: &AudioSpan,
    samples: &mut [[f32; 2]],
) -> Result<(), StageAudioError> {
    let length = span
        .allocated_samples
        .end
        .0
        .checked_sub(span.allocated_samples.start.0)
        .filter(|length| *length > 0)
        .ok_or(StageAudioError::Range)?;
    let offset = span
        .samples
        .start
        .0
        .checked_sub(span.allocated_samples.start.0)
        .filter(|offset| *offset >= 0)
        .ok_or(StageAudioError::Range)?;
    let count = span
        .samples
        .end
        .0
        .checked_sub(span.samples.start.0)
        .and_then(|count| usize::try_from(count).ok())
        .ok_or(StageAudioError::Range)?;
    if count != samples.len()
        || span.samples.end > span.allocated_samples.end
        || span.boundaries.start.is_empty()
        || span.boundaries.end.is_empty()
    {
        return Err(StageAudioError::Range);
    }
    // Automatic supplies the default, never a veto of an explicit creative
    // Hard from another exactly coincident owner. Rounded equality is irrelevant.
    let start = !span
        .boundaries
        .start
        .iter()
        .any(|origin| origin.policy == AudioEdgePolicy::Hard);
    let end = !span
        .boundaries
        .end
        .iter()
        .any(|origin| origin.policy == AudioEdgePolicy::Hard);
    for (index, sample) in samples.iter_mut().enumerate() {
        let at = offset
            .checked_add(i64::try_from(index).map_err(|_| StageAudioError::Range)?)
            .ok_or(StageAudioError::Range)?;
        let gain = edge_gain(length, at, start, end);
        for channel in sample {
            *channel *= gain;
        }
    }
    Ok(())
}

fn edge_gain(length: i64, at: i64, start: bool, end: bool) -> f32 {
    // Twice F=min(96,N/2) stays integral, including odd/tiny fragments. Clamp
    // integer distances before multiplication so even huge origins are safe.
    let twice_width = length.min(192);
    let edge = |distance: i64| {
        let numerator = (2 * distance.min(96) + 1).min(twice_width);
        numerator as f32 / twice_width as f32
    };
    let left = if start { edge(at) } else { 1.0 };
    let right = if end { edge(length - 1 - at) } else { 1.0 };
    left.min(right)
}

#[cfg(test)]
mod tests {
    use super::edge_gain;

    #[test]
    fn tiny_fragments_and_default_two_millisecond_edges_are_sample_centered() {
        assert_eq!(edge_gain(1, 0, true, true), 1.0);
        assert_eq!(edge_gain(2, 0, true, true), 0.5);
        assert_eq!(edge_gain(2, 1, true, true), 0.5);
        assert_eq!(edge_gain(3, 1, true, true), 1.0);
        for length in [3, 191, 192, 193, 1024, i64::MAX] {
            assert_eq!(
                edge_gain(length, 0, true, true),
                1.0 / length.min(192) as f32
            );
            assert_eq!(
                edge_gain(length, 0, true, true),
                edge_gain(length, length - 1, true, true)
            );
            assert_eq!(edge_gain(length, 0, false, false), 1.0);
        }
        assert_eq!(edge_gain(1024, 95, true, true), 191.0 / 192.0);
        assert_eq!(edge_gain(1024, 96, true, true), 1.0);
        assert_eq!(edge_gain(2, 0, false, true), 1.0);
        assert_eq!(edge_gain(2, 1, false, true), 0.5);
    }
}
