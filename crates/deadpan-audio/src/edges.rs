//! Stateless, per-voice fades using retained progress on the 48 kHz output clock.
use std::ops::Range;

use deadpan_core::{AudioEdgePolicy, AudioSample, ExactRatio};
use deadpan_plan::{AudioFadeSpan, AudioSpan};

use crate::StageAudioError;

/// Sample-centered linear edges after all time/pitch mapping, before treatments.
pub const EDGE_FADE_ID: &str = "deadpan-voice-edge-sample-centered-linear-2ms-v1";

/// Admit the complete derived creative envelope before media access. The
/// independent endpoint policy owns exhaustion, including zero/one-point spans.
pub(crate) fn validate_creative_fades(
    start: AudioSample,
    frames: usize,
    spans: &[AudioFadeSpan],
) -> Result<(), StageAudioError> {
    creative_gains(start, frames, spans).map(|_| ())
}

pub(crate) fn apply_creative_fades(
    start: AudioSample,
    samples: &mut [[f32; 2]],
    spans: &[AudioFadeSpan],
) -> Result<(), StageAudioError> {
    // Calculate and validate every gain before changing PCM. Fractional
    // envelope progress never becomes an accumulated floating-point clock.
    let gains = creative_gains(start, samples.len(), spans)?;
    for (sample, gain) in samples.iter_mut().zip(gains) {
        if gain == 0.0 {
            *sample = [0.0; 2];
        } else {
            for channel in sample {
                *channel *= gain;
            }
        }
    }
    Ok(())
}

fn creative_gains(
    start: AudioSample,
    frames: usize,
    spans: &[AudioFadeSpan],
) -> Result<Vec<f32>, StageAudioError> {
    if frames == 0 || frames > crate::MAX_OUTPUT_FRAMES as usize || spans.len() > frames {
        return Err(StageAudioError::Range);
    }
    let end = start
        .0
        .checked_add(i64::try_from(frames).map_err(|_| StageAudioError::Range)?)
        .ok_or(StageAudioError::Range)?;
    let mut cursor = start.0;
    let mut gains = vec![1.0; frames];
    for span in spans {
        if span.samples.start.0 != cursor
            || span.samples.start >= span.samples.end
            || span.samples.end.0 > end
            || (span.length >= 2
                && (span.boundaries.start.is_empty() || span.boundaries.end.is_empty()))
        {
            return Err(StageAudioError::Range);
        }
        cursor = span.samples.end.0;
        let left =
            usize::try_from(span.samples.start.0 - start.0).map_err(|_| StageAudioError::Range)?;
        let right =
            usize::try_from(span.samples.end.0 - start.0).map_err(|_| StageAudioError::Range)?;
        let start_fade = !span
            .boundaries
            .start
            .iter()
            .any(|origin| origin.policy == AudioEdgePolicy::Hard);
        let end_fade = !span
            .boundaries
            .end
            .iter()
            .any(|origin| origin.policy == AudioEdgePolicy::Hard);
        for (offset, gain) in gains[left..right].iter_mut().enumerate() {
            let progress = span.progress_at_start.checked_add(ExactRatio::integer(
                i64::try_from(offset).map_err(|_| StageAudioError::Range)?,
            ))?;
            *gain = creative_gain(span.length, progress, start_fade, end_fade)?;
        }
    }
    if cursor != end {
        return Err(StageAudioError::Range);
    }
    Ok(gains)
}

fn creative_gain(
    length: u64,
    progress: ExactRatio,
    start: bool,
    end: bool,
) -> Result<f32, StageAudioError> {
    if length < 2 || (!start && !end) {
        return Ok(1.0);
    }
    let twice_width = ExactRatio::new(i128::from(length.min(192)), 1)?;
    let ramp = |distance: ExactRatio| -> Result<f32, StageAudioError> {
        // The fade never extends beyond 96 output samples. Clamp far-away
        // progress before adding its half-sample center or scaling a ratio.
        if distance.compare_integer(-1).is_le() {
            return Ok(0.0);
        }
        if distance.compare_integer(96).is_ge() {
            return Ok(1.0);
        }
        let value = distance
            .checked_mul(ExactRatio::integer(2))?
            .checked_add(ExactRatio::ONE)?
            .checked_div(twice_width)?;
        if value.compare_integer(0).is_le() {
            return Ok(0.0);
        }
        if value.compare_integer(1).is_ge() {
            return Ok(1.0);
        }
        Ok((value.numerator() as f64 / value.denominator() as f64) as f32)
    };
    let left = if start { ramp(progress)? } else { 1.0 };
    let right = if end {
        ramp(ExactRatio::new(i128::from(length) - 1, 1)?.checked_sub(progress)?)?
    } else {
        1.0
    };
    Ok(left.min(right))
}

/// Enforce retained-domain silence on every read. Optional creative edge fades
/// affect only samples inside that domain; raw time-mapped reads still exhaust.
pub(crate) fn apply_retained_envelope(
    span: &AudioSpan,
    samples: &mut [[f32; 2]],
    fades: bool,
) -> Result<(), StageAudioError> {
    let offset = span
        .envelope
        .progress_at(span.samples.start)
        .map_err(|_| StageAudioError::Range)?;
    let count = span
        .samples
        .end
        .0
        .checked_sub(span.samples.start.0)
        .and_then(|count| usize::try_from(count).ok())
        .ok_or(StageAudioError::Range)?;
    if count != samples.len()
        || span.samples.end > span.allocated_samples.end
        || span.samples.start < span.allocated_samples.start
        || span.allocated_samples.start >= span.allocated_samples.end
        || span.boundaries.start.is_empty()
        || span.boundaries.end.is_empty()
    {
        return Err(StageAudioError::Range);
    }
    // Validate the whole query before modifying PCM. Progress is monotonic,
    // so these endpoints prove every intervening signed addition fits too.
    if count > 0 {
        span.envelope
            .progress_at(AudioSample(span.samples.end.0 - 1))
            .map_err(|_| StageAudioError::Range)?;
    }
    // Automatic supplies the default, never a veto of an explicit creative
    // Hard from another exactly coincident owner. Rounded equality is irrelevant.
    let start = fades
        && !span
            .boundaries
            .start
            .iter()
            .any(|origin| origin.policy == AudioEdgePolicy::Hard);
    let end = fades
        && !span
            .boundaries
            .end
            .iter()
            .any(|origin| origin.policy == AudioEdgePolicy::Hard);
    for (index, sample) in samples.iter_mut().enumerate() {
        let at = offset
            .checked_add(i128::try_from(index).map_err(|_| StageAudioError::Range)?)
            .ok_or(StageAudioError::Range)?;
        let gain = edge_gain(span.envelope.length(), at, start, end);
        if gain == 0.0 {
            // Retained-envelope exhaustion is silence, including Hard edges.
            // It never clamps or repeats the last in-domain sample.
            *sample = [0.0; 2];
        } else {
            for channel in sample {
                *channel *= gain;
            }
        }
    }
    Ok(())
}

/// Endpoint audibility is retained through later interpolation independently of
/// creative fade gains. Call after the envelope pass has validated the span.
pub(crate) fn exhausted_ranges(
    span: &AudioSpan,
) -> Result<Vec<Range<AudioSample>>, StageAudioError> {
    let count = span
        .samples
        .end
        .0
        .checked_sub(span.samples.start.0)
        .filter(|count| *count > 0)
        .ok_or(StageAudioError::Range)?;
    let first = span.envelope.progress_at(span.samples.start)?;
    let last = span
        .envelope
        .progress_at(AudioSample(span.samples.end.0 - 1))?;
    let mut result = Vec::new();
    if first < 0 {
        // Saturating negation is exact after the count clamp even for MIN.
        let prefix = i64::try_from(first.saturating_neg().min(i128::from(count)))
            .map_err(|_| StageAudioError::Range)?;
        result.push(span.samples.start..AudioSample(span.samples.start.0 + prefix));
    }
    let length = i128::from(span.envelope.length());
    if last >= length {
        let suffix = i64::try_from((last - length + 1).min(i128::from(count)))
            .map_err(|_| StageAudioError::Range)?;
        result.push(AudioSample(span.samples.end.0 - suffix)..span.samples.end);
    }
    Ok(result)
}

fn edge_gain(length: u64, at: i128, start: bool, end: bool) -> f32 {
    let Ok(at) = u64::try_from(at) else {
        return 0.0;
    };
    if at >= length {
        return 0.0;
    }
    // Twice F=min(96,N/2) stays integral, including odd/tiny fragments. Clamp
    // integer distances before multiplication so even huge origins are safe.
    let twice_width = length.min(192);
    let edge = |distance: u64| {
        let numerator = (2 * distance.min(96) + 1).min(twice_width);
        numerator as f32 / twice_width as f32
    };
    let left = if start { edge(at) } else { 1.0 };
    let right = if end { edge(length - 1 - at) } else { 1.0 };
    left.min(right)
}

#[cfg(test)]
mod tests {
    use std::ops::Range;

    use deadpan_core::{ExactRatio, InstancePath, NodeId};
    use deadpan_plan::{
        AudioBoundaries, AudioBoundaryKind, AudioBoundaryOrigin, AudioBoundaryRule, AudioContent,
        AudioEnvelope, AudioSampleGrid, AudioSampleMap, AudioTransform, SilenceReason,
    };

    use super::*;

    #[test]
    fn derived_creative_gain_preserves_integer_fades_and_exact_fractional_progress() {
        for length in 2..256 {
            for at in 0..length {
                for (start, end) in [(false, false), (true, false), (false, true), (true, true)] {
                    assert_eq!(
                        creative_gain(
                            length,
                            ExactRatio::new(i128::from(at), 1).unwrap(),
                            start,
                            end
                        )
                        .unwrap(),
                        edge_gain(length, i128::from(at), start, end),
                    );
                }
            }
        }
        assert_eq!(
            creative_gain(4, ExactRatio::new(1, 3).unwrap(), true, true).unwrap(),
            5.0_f32 / 12.0
        );
        assert_eq!(
            creative_gain(4, ExactRatio::new(8, 3).unwrap(), true, true).unwrap(),
            5.0_f32 / 12.0
        );
        assert_eq!(
            creative_gain(400, ExactRatio::new(191, 2).unwrap(), true, true).unwrap(),
            1.0
        );
        for length in [0, 1] {
            for progress in [-100, 0, 100] {
                assert_eq!(
                    creative_gain(length, ExactRatio::integer(progress), true, true).unwrap(),
                    1.0,
                    "endpoint policy, not the creative ramp, owns tiny-domain exhaustion"
                );
            }
        }
    }

    #[test]
    fn derived_fade_validation_is_atomic_for_a_later_invalid_span() {
        let base = span(0..2, AudioEnvelope::new(2, 0, AudioSample(0)).unwrap());
        let first = AudioFadeSpan {
            samples: AudioSample(0)..AudioSample(2),
            length: 4,
            progress_at_start: ExactRatio::ZERO,
            boundaries: base.boundaries.clone(),
        };
        let invalid = AudioFadeSpan {
            samples: AudioSample(1)..AudioSample(3),
            length: 4,
            progress_at_start: ExactRatio::integer(1),
            boundaries: base.boundaries,
        };
        let mut samples = vec![[0.75, -0.5]; 4];
        let original = samples.clone();
        assert!(apply_creative_fades(AudioSample(0), &mut samples, &[first, invalid]).is_err());
        assert_eq!(samples, original);
    }

    fn span(samples: Range<i64>, envelope: AudioEnvelope) -> AudioSpan {
        let instance = InstancePath {
            node: NodeId::new("voice").unwrap(),
            repeats: Vec::new(),
        };
        let boundary = |kind| AudioBoundaryOrigin {
            instance: instance.clone(),
            gap_after: None,
            kind,
            policy: AudioEdgePolicy::Automatic,
            placement_support: false,
        };
        AudioSpan {
            definition: None,
            samples: AudioSample(samples.start)..AudioSample(samples.end),
            allocated_samples: AudioSample(samples.start)..AudioSample(samples.end),
            project_extent: ExactRatio::integer(samples.start)..ExactRatio::integer(samples.end),
            envelope_extent: ExactRatio::integer(0)..ExactRatio::integer(envelope.length() as i64),
            envelope_samples: AudioSample(0)..AudioSample(envelope.length() as i64),
            envelope,
            boundaries: AudioBoundaries {
                start: vec![boundary(AudioBoundaryKind::NodeStart)],
                end: vec![boundary(AudioBoundaryKind::NodeEnd)],
            },
            instance,
            gap_after: None,
            transform: AudioTransform {
                project_origin: ExactRatio::integer(0),
                project_frames_per_local_frame: ExactRatio::integer(1),
                project_frames_per_sample: ExactRatio::integer(1),
            },
            grid: AudioSampleGrid::new(
                ExactRatio::integer(0),
                ExactRatio::integer(1),
                AudioBoundaryRule::RoundEven,
            )
            .unwrap(),
            sampling: AudioSampleMap::new(
                AudioSample(0),
                ExactRatio::integer(0),
                ExactRatio::integer(1),
            )
            .unwrap(),
            retimes: Vec::new(),
            content: AudioContent::Silence {
                reason: SilenceReason::NoSourceAudio,
            },
        }
    }

    fn render(span: &AudioSpan) -> Vec<[f32; 2]> {
        let mut samples = vec![[1.0, 0.25]; (span.samples.end.0 - span.samples.start.0) as usize];
        apply_retained_envelope(span, &mut samples, true).unwrap();
        samples
    }

    #[test]
    fn tiny_fragments_and_default_two_millisecond_edges_are_sample_centered() {
        assert_eq!(edge_gain(1, 0, true, true), 1.0);
        assert_eq!(edge_gain(2, 0, true, true), 0.5);
        assert_eq!(edge_gain(2, 1, true, true), 0.5);
        assert_eq!(edge_gain(3, 1, true, true), 1.0);
        for length in [3, 191, 192, 193, 1024, i64::MAX as u64, u64::MAX] {
            assert_eq!(
                edge_gain(length, 0, true, true),
                1.0 / length.min(192) as f32
            );
            assert_eq!(
                edge_gain(length, 0, true, true),
                edge_gain(length, i128::from(length - 1), true, true)
            );
            assert_eq!(edge_gain(length, 0, false, false), 1.0);
        }
        assert_eq!(edge_gain(1024, 95, true, true), 191.0 / 192.0);
        assert_eq!(edge_gain(1024, 96, true, true), 1.0);
        assert_eq!(edge_gain(2, 0, false, true), 1.0);
        assert_eq!(edge_gain(2, 1, false, true), 0.5);
    }

    #[test]
    fn ntsc_shift_resumes_old_progress_and_silences_the_extra_allocated_sample() {
        let old = AudioEnvelope::from_samples(AudioSample(0)..AudioSample(3203)).unwrap();
        let original = render(&span(0..3203, old));
        let resumed = span(
            3203..4805,
            old.reanchor(AudioSample(1602), AudioSample(3203)).unwrap(),
        );
        let actual = render(&resumed);
        assert_eq!(&actual[..1601], &original[1602..]);
        assert_eq!(actual[0], [1.0, 0.25], "resume adds no new seam fade");
        assert_eq!(actual[1600], [1.0 / 192.0, 0.25 / 192.0]);
        assert_eq!(actual[1601], [0.0; 2], "old half-open domain is exhausted");

        let mut partitioned = Vec::new();
        let mut start = resumed.samples.start.0;
        for count in [1, 7, 96, 409, 3, 1085, 1] {
            let mut query = resumed.clone();
            query.samples = AudioSample(start)..AudioSample(start + count);
            partitioned.extend(render(&query));
            start += count;
        }
        assert_eq!(start, resumed.samples.end.0);
        assert_eq!(
            partitioned, actual,
            "query boundaries cannot restart a fade"
        );
    }

    #[test]
    fn tiny_retained_envelopes_do_not_widen_to_fit_the_shifted_allocation() {
        for length in [1, 2, 3, 191, 192, 193] {
            let envelope = AudioEnvelope::new(length, -1, AudioSample(-10)).unwrap();
            let actual = render(&span(-10..-8 + length as i64, envelope));
            assert_eq!(actual[0], [0.0; 2]);
            for progress in 0..length {
                let gain = edge_gain(length, i128::from(progress), true, true);
                assert_eq!(actual[progress as usize + 1], [gain, gain * 0.25]);
                assert!(gain.is_finite() && (0.0..=1.0).contains(&gain));
            }
            assert_eq!(actual[length as usize + 1], [0.0; 2]);
        }
    }

    #[test]
    fn hard_coincident_owners_disable_only_in_domain_fades() {
        let mut voice = span(
            100..104,
            AudioEnvelope::new(2, -1, AudioSample(100)).unwrap(),
        );
        assert_eq!(
            render(&voice),
            vec![[0.0; 2], [0.5, 0.125], [0.5, 0.125], [0.0; 2]]
        );
        for edges in [&mut voice.boundaries.start, &mut voice.boundaries.end] {
            let mut hard = edges[0].clone();
            hard.instance.node = NodeId::new("hard-owner").unwrap();
            hard.policy = AudioEdgePolicy::Hard;
            edges.push(hard);
        }
        assert_eq!(
            render(&voice),
            vec![[0.0; 2], [1.0, 0.25], [1.0, 0.25], [0.0; 2]]
        );
        voice.boundaries.end.pop();
        assert_eq!(
            render(&voice),
            vec![[0.0; 2], [1.0, 0.25], [0.5, 0.125], [0.0; 2]]
        );
        for progress in [i128::MIN, -1, 2, i128::MAX] {
            assert_eq!(edge_gain(2, progress, false, false), 0.0);
        }
    }

    #[test]
    fn raw_reads_keep_in_domain_pcm_and_silence_both_retained_endpoints() {
        let voice = span(
            3203..4806,
            AudioEnvelope::new(1601, -1, AudioSample(3203)).unwrap(),
        );
        let mut pcm = vec![[0.75, -0.25]; 1603];
        apply_retained_envelope(&voice, &mut pcm, false).unwrap();
        assert_eq!(pcm[0], [0.0; 2], "negative retained progress is silence");
        assert!(pcm[1..1602].iter().all(|sample| *sample == [0.75, -0.25]));
        assert_eq!(pcm[1602], [0.0; 2], "exhaustion also applies to raw reads");
        assert_eq!(
            exhausted_ranges(&voice).unwrap(),
            vec![
                AudioSample(3203)..AudioSample(3204),
                AudioSample(4805)..AudioSample(4806)
            ]
        );

        let old = AudioEnvelope::from_samples(AudioSample(0)..AudioSample(3203)).unwrap();
        let resumed = span(
            3203..4805,
            old.reanchor(AudioSample(1602), AudioSample(3203)).unwrap(),
        );
        let mut pcm = vec![[0.75, -0.25]; 1602];
        apply_retained_envelope(&resumed, &mut pcm, false).unwrap();
        assert!(pcm[..1601].iter().all(|sample| *sample == [0.75, -0.25]));
        assert_eq!(pcm[1601], [0.0; 2]);
        assert_eq!(
            exhausted_ranges(&resumed).unwrap(),
            vec![AudioSample(4804)..AudioSample(4805)]
        );
        for progress in [i128::MIN, i128::MAX] {
            let voice = span(
                0..1,
                AudioEnvelope::new(1, progress, AudioSample(0)).unwrap(),
            );
            assert_eq!(
                exhausted_ranges(&voice).unwrap(),
                vec![AudioSample(0)..AudioSample(1)]
            );
        }
    }

    #[test]
    fn bounded_pcm_keeps_wide_allocation_and_envelope_progress() {
        let allocation = AudioSample(i64::MIN)..AudioSample(i64::MAX);
        let envelope = AudioEnvelope::from_samples(allocation.clone()).unwrap();
        for at in [i64::MIN, 0, i64::MAX - 1] {
            let mut voice = span(at..at + 1, envelope);
            voice.allocated_samples = allocation.clone();
            voice.envelope_samples = allocation.clone();
            voice.envelope_extent = ExactRatio::integer(i64::MIN)..ExactRatio::integer(i64::MAX);
            for fades in [false, true] {
                let mut pcm = [[1.0, 0.25]];
                apply_retained_envelope(&voice, &mut pcm, fades).unwrap();
                let gain = if fades && at != 0 { 1.0 / 192.0 } else { 1.0 };
                assert_eq!(pcm, [[gain, gain * 0.25]]);
                assert!(exhausted_ranges(&voice).unwrap().is_empty());
            }
        }
    }

    #[test]
    fn invalid_queries_owners_and_progress_fail_before_modifying_pcm() {
        let original = span(0..2, AudioEnvelope::new(2, 0, AudioSample(0)).unwrap());
        let mut invalid = Vec::new();
        let mut voice = original.clone();
        voice.samples = AudioSample(-1)..AudioSample(1);
        invalid.push(voice);
        let mut voice = original.clone();
        voice.samples = AudioSample(1)..AudioSample(3);
        invalid.push(voice);
        let mut voice = original.clone();
        voice.samples = AudioSample(2)..AudioSample(0);
        invalid.push(voice);
        let mut voice = original.clone();
        voice.allocated_samples = AudioSample(1)..AudioSample(1);
        invalid.push(voice);
        let mut voice = original.clone();
        voice.allocated_samples = AudioSample(i64::MAX)..AudioSample(i64::MIN);
        invalid.push(voice);
        let mut voice = original.clone();
        voice.boundaries.start.clear();
        invalid.push(voice);
        let mut voice = original.clone();
        voice.boundaries.end.clear();
        invalid.push(voice);
        let mut voice = original.clone();
        voice.envelope = AudioEnvelope::new(2, i128::MAX, AudioSample(0)).unwrap();
        invalid.push(voice);
        let mut voice = original.clone();
        voice.envelope = AudioEnvelope::new(2, i128::MIN, AudioSample(1)).unwrap();
        invalid.push(voice);
        for voice in invalid {
            for fades in [false, true] {
                let mut pcm = [[0.5, -0.25]; 2];
                assert!(matches!(
                    apply_retained_envelope(&voice, &mut pcm, fades),
                    Err(StageAudioError::Range)
                ));
                assert_eq!(pcm, [[0.5, -0.25]; 2]);
            }
        }
        assert!(matches!(
            apply_retained_envelope(&original, &mut [[1.0; 2]], true),
            Err(StageAudioError::Range)
        ));
    }
}
