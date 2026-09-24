//! Stateless, per-voice fades using retained progress on the 48 kHz output clock.
use std::ops::Range;

use deadpan_core::{AudioEdgePolicy, AudioSample};
use deadpan_plan::AudioSpan;

use crate::StageAudioError;

/// Sample-centered linear edges after all time/pitch mapping, before treatments.
pub const EDGE_FADE_ID: &str = "deadpan-voice-edge-sample-centered-linear-2ms-v1";

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
