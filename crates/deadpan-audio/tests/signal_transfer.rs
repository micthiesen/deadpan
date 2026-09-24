#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use deadpan_audio::{
    MAX_SOURCE_FRAMES, PcmWindow, PreparationError, ResampleRecipe, Resampler, RetainedRootPolicy,
    RootSignalBlock, RootSignalTransfer, SignalTransferError, StereoMatrix,
};
use deadpan_core::*;
use deadpan_dsp::{CanonicalRecipe, CanonicalStretch, StereoPcm, StretchRate};
use deadpan_media::audio_index::AudioChannelLayout;
use deadpan_plan::{
    AudioQueryLimits, AudioReferencePlan, ReferenceAudioContent, ReferenceSample, SignalSample,
    SilenceReason,
};

fn ratio(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}

fn roots(start: i64, end: i64) -> Range<AudioSample> {
    AudioSample(start)..AudioSample(end)
}

fn signals(start: i64, end: i64) -> Range<SignalSample> {
    SignalSample(start)..SignalSample(end)
}

struct Carrier {
    start: i64,
    samples: Vec<[f32; 2]>,
    suppressed: Vec<Range<AudioSample>>,
}

impl Carrier {
    fn support(&self) -> Range<AudioSample> {
        roots(
            self.start,
            self.start + i64::try_from(self.samples.len()).unwrap(),
        )
    }

    fn read(&self, start: AudioSample, frames: u32) -> RootSignalBlock {
        assert!((1..=256).contains(&frames));
        let end = start.0 + i64::from(frames);
        assert!(start >= self.support().start && AudioSample(end) <= self.support().end);
        let offset = usize::try_from(start.0 - self.start).unwrap();
        RootSignalBlock {
            start,
            samples: self.samples[offset..offset + frames as usize].to_vec(),
            suppressed: self
                .suppressed
                .iter()
                .filter_map(|range| {
                    let left = range.start.0.max(start.0);
                    let right = range.end.0.min(end);
                    (left < right).then(|| roots(left, right))
                })
                .collect(),
        }
    }

    fn materialized(&self, premask: bool) -> Vec<[f32; 2]> {
        let mut result = self.samples.clone();
        if premask {
            for range in &self.suppressed {
                let left = usize::try_from(range.start.0 - self.start).unwrap();
                let right = usize::try_from(range.end.0 - self.start).unwrap();
                result[left..right].fill([0.0; 2]);
            }
        }
        result
    }
}

#[derive(Clone)]
struct Mapping {
    root_at_anchor: ExactRatio,
    anchor: SignalSample,
    step: ExactRatio,
    output: Range<SignalSample>,
}

impl Mapping {
    fn transfer(&self, carrier: &Carrier) -> RootSignalTransfer {
        RootSignalTransfer::new(
            carrier.support(),
            self.root_at_anchor,
            self.anchor,
            self.step,
            self.output.clone(),
        )
        .unwrap()
    }

    fn root_at(&self, sample: SignalSample) -> ExactRatio {
        self.root_at_anchor
            .checked_add(
                ExactRatio::integer(sample.0 - self.anchor.0)
                    .checked_mul(self.step)
                    .unwrap(),
            )
            .unwrap()
    }
}

// A complete, premasked PCM carrier is the oracle. This uses the existing
// resampler directly; it never asks the adapter which taps or masks to read.
fn oracle(
    carrier: &Carrier,
    mapping: &Mapping,
    start: SignalSample,
    frames: u32,
    premask: bool,
) -> (Vec<[f32; 2]>, Vec<Range<SignalSample>>) {
    let materialized = carrier.materialized(premask);
    let support = carrier.support();
    let sampler = Resampler::new(
        ResampleRecipe::new(
            support.start.0..support.end.0,
            mapping.root_at_anchor,
            AudioSample(mapping.anchor.0),
            mapping.step,
            AudioSample(mapping.output.start.0)..AudioSample(mapping.output.end.0),
        )
        .unwrap(),
        StereoMatrix::new(AudioChannelLayout::Native {
            channels: 2,
            mask: 3,
        })
        .unwrap(),
    );
    let input = sampler
        .required_source_range(AudioSample(start.0), frames)
        .unwrap()
        .map(|range| {
            let left = usize::try_from(range.start - carrier.start).unwrap();
            let right = usize::try_from(range.end - carrier.start).unwrap();
            PcmWindow {
                start: range.start,
                samples: materialized[left..right]
                    .iter()
                    .flatten()
                    .copied()
                    .collect(),
            }
        });
    let mut actual = sampler
        .render(AudioSample(start.0), frames, input, &AtomicBool::new(false))
        .unwrap()
        .samples;
    let mut suppressed: Vec<Range<SignalSample>> = Vec::new();
    // Point membership independently determines the exact half-open output
    // mask, including fractional boundaries and exhausted root support.
    for (index, sample) in actual.iter_mut().enumerate() {
        let at = SignalSample(start.0 + i64::try_from(index).unwrap());
        let root = mapping.root_at(at);
        let inside = root.compare_integer(support.start.0).is_ge()
            && root.compare_integer(support.end.0).is_lt();
        let quiet = carrier.suppressed.iter().any(|range| {
            root.compare_integer(range.start.0).is_ge() && root.compare_integer(range.end.0).is_lt()
        });
        if !inside || quiet {
            *sample = [0.0; 2];
            if let Some(previous) = suppressed.last_mut()
                && previous.end == at
            {
                previous.end.0 += 1;
            } else {
                suppressed.push(at..SignalSample(at.0 + 1));
            }
        }
    }
    (actual, suppressed)
}

fn waveform(at: i64) -> [f32; 2] {
    if at.rem_euclid(41) == 0 {
        [0.0; 2]
    } else {
        [
            (0.31 * (at as f64 * 0.071).sin() + 0.17) as f32,
            (0.23 * (at as f64 * 0.037).cos() - 0.09) as f32,
        ]
    }
}

#[test]
fn materialized_carrier_matches_fractional_nonzero_grids_and_suffix_first_queries() {
    let carrier = Carrier {
        start: 0,
        samples: (0..40_000).map(waveform).collect(),
        suppressed: vec![roots(151, 188), roots(337, 400), roots(8017, 8139)],
    };
    for (origin, step) in [
        (ratio(1001, 3), ratio(147, 160)),
        (ExactRatio::integer(333), ExactRatio::ONE),
        (ratio(667, 2), ExactRatio::ONE),
        (ratio(1001, 3), ratio(1001, 300)),
        (ratio(1001, 3), ratio(1, 64)),
        (ratio(1001, 3), ratio(64, 1)),
    ] {
        let mapping = Mapping {
            root_at_anchor: origin,
            anchor: SignalSample(713),
            step,
            output: signals(701, 1213),
        };
        let transfer = mapping.transfer(&carrier);
        let expected = oracle(&carrier, &mapping, SignalSample(713), 256, true);
        let mut requested = Vec::new();
        let whole = transfer
            .render(
                SignalSample(713),
                256,
                &AtomicBool::new(false),
                |start, frames| {
                    requested.push(roots(start.0, start.0 + i64::from(frames)));
                    Ok::<_, SignalTransferError>(carrier.read(start, frames))
                },
            )
            .unwrap();
        assert_eq!(whole.start, SignalSample(713));
        assert_eq!(whole.samples, expected.0);
        assert_eq!(whole.suppressed, expected.1);
        let total: i64 = requested.iter().map(|r| r.end.0 - r.start.0).sum();
        assert!(total <= i64::from(MAX_SOURCE_FRAMES));
        assert!(
            requested
                .windows(2)
                .all(|pair| pair[0].end == pair[1].start)
        );
        if step == ratio(64, 1) {
            assert!(requested.len() > 32, "exercise a multi-block input halo");
        }
        // Seek to the suffix first. Every partition must have the same phase,
        // input policy and output suppression as the complete materialization.
        for (offset, count) in [(173, 83), (0, 1), (1, 31), (32, 97), (129, 44)] {
            let start = SignalSample(713 + offset);
            let part = transfer
                .render(start, count, &AtomicBool::new(false), |at, frames| {
                    Ok::<_, SignalTransferError>(carrier.read(at, frames))
                })
                .unwrap();
            let expected_part = oracle(&carrier, &mapping, start, count, true);
            let left = usize::try_from(offset).unwrap();
            assert_eq!(part.samples, whole.samples[left..left + count as usize]);
            assert_eq!(part.samples, expected_part.0);
            assert_eq!(part.suppressed, expected_part.1);
        }
    }
}

#[test]
fn explicit_silence_masks_input_taps_and_zero_pcm_does_not_create_policy() {
    let carrier = Carrier {
        start: 0,
        samples: vec![[0.75, -0.375]; 128],
        suppressed: vec![roots(32, 48)],
    };
    let mapping = Mapping {
        root_at_anchor: ratio(1, 2),
        anchor: SignalSample(907),
        step: ExactRatio::ONE,
        output: signals(907, 1035),
    };
    let actual = mapping
        .transfer(&carrier)
        .render(
            SignalSample(907),
            96,
            &AtomicBool::new(false),
            |at, count| Ok::<_, SignalTransferError>(carrier.read(at, count)),
        )
        .unwrap();
    let premasked = oracle(&carrier, &mapping, SignalSample(907), 96, true);
    let mask_after_only = oracle(&carrier, &mapping, SignalSample(907), 96, false);
    assert_eq!(actual.samples, premasked.0);
    assert_eq!(actual.suppressed, vec![signals(939, 955)]);
    assert!(
        actual
            .samples
            .iter()
            .zip(mask_after_only.0)
            .enumerate()
            .any(|(index, (good, wrong))| {
                !actual
                    .suppressed
                    .iter()
                    .any(|range| range.contains(&SignalSample(907 + index as i64)))
                    && (good[0] - wrong[0]).abs() > 1e-4
            })
    );
    let zeros = Carrier {
        start: 0,
        samples: vec![[0.0; 2]; 128],
        suppressed: vec![],
    };
    let zero = mapping
        .transfer(&zeros)
        .render(
            SignalSample(907),
            96,
            &AtomicBool::new(false),
            |at, count| Ok::<_, SignalTransferError>(zeros.read(at, count)),
        )
        .unwrap();
    assert_eq!(zero.samples, vec![[0.0; 2]; 96]);
    assert!(zero.suppressed.is_empty());
}

#[test]
fn negative_mapped_positions_and_support_endpoints_are_explicit_output_silence() {
    let carrier = Carrier {
        start: 7,
        samples: (7..39).map(waveform).collect(),
        suppressed: vec![roots(15, 18)],
    };
    for (origin, anchor) in [
        (ExactRatio::integer(-3), SignalSample(203)),
        (ratio(-7, 2), SignalSample(203)),
        (ExactRatio::integer(-168), SignalSample(-17)),
    ] {
        let mapping = Mapping {
            root_at_anchor: origin,
            anchor,
            step: ratio(3, 4),
            output: signals(203, 331),
        };
        let actual = mapping
            .transfer(&carrier)
            .render(
                SignalSample(203),
                128,
                &AtomicBool::new(false),
                |at, frames| Ok::<_, SignalTransferError>(carrier.read(at, frames)),
            )
            .unwrap();
        let expected = oracle(&carrier, &mapping, SignalSample(203), 128, true);
        assert_eq!(actual.samples, expected.0);
        assert_eq!(actual.suppressed, expected.1);
        assert_eq!(actual.suppressed.first().unwrap().start, SignalSample(203));
        assert_eq!(actual.suppressed.last().unwrap().end, SignalSample(331));
        assert!(actual.samples.iter().flatten().any(|v| v.abs() > 1e-4));
    }
    let outside = Mapping {
        root_at_anchor: ExactRatio::integer(-10_000),
        anchor: SignalSample(203),
        step: ExactRatio::ONE,
        output: signals(203, 331),
    };
    let silence = outside
        .transfer(&carrier)
        .render(
            SignalSample(203),
            128,
            &AtomicBool::new(false),
            |_, _| -> Result<RootSignalBlock, SignalTransferError> {
                panic!("exhausted root support must not request any PCM")
            },
        )
        .unwrap();
    assert_eq!(silence.samples, vec![[0.0; 2]; 128]);
    assert_eq!(silence.suppressed, vec![signals(203, 331)]);
}

#[test]
fn invalid_callbacks_and_cancellation_never_return_partial_audio() {
    let carrier = Carrier {
        start: 0,
        samples: vec![[0.25, -0.5]; 20_000],
        suppressed: vec![],
    };
    let mapping = Mapping {
        root_at_anchor: ExactRatio::integer(1000),
        anchor: SignalSample(19),
        step: ratio(8, 1),
        output: signals(19, 531),
    };
    let transfer = mapping.transfer(&carrier);
    for mutation in 0..12 {
        let result = transfer.render(
            SignalSample(19),
            256,
            &AtomicBool::new(false),
            |at, count| {
                let mut block = carrier.read(at, count);
                match mutation {
                    0 => block.start.0 += 1,
                    1 => {
                        block.samples.pop();
                    }
                    2 => block.samples.push([0.0; 2]),
                    3 => block.suppressed.push(roots(at.0 + 1, at.0)),
                    4 => block.suppressed.push(roots(at.0, at.0)),
                    5 => block.suppressed.push(roots(at.0 - 1, at.0 + 1)),
                    6 => block.samples[0][0] = f32::NAN,
                    7 => block.samples[0][1] = f32::INFINITY,
                    8 => {
                        block.samples[0][0] = 16.01;
                        block.suppressed.push(roots(at.0, at.0 + 1));
                    }
                    9 => {
                        block.samples[0][0] = f32::NAN;
                        block.suppressed.push(roots(at.0, at.0 + 1));
                    }
                    10 => block
                        .suppressed
                        .push(roots(at.0, at.0 + i64::from(count) + 1)),
                    11 => block.suppressed = vec![roots(at.0, at.0 + 1); 257],
                    _ => unreachable!(),
                }
                Ok::<_, SignalTransferError>(block)
            },
        );
        assert!(result.is_err(), "malformed callback case {mutation}");
    }
    let cancelled = AtomicBool::new(true);
    let result = transfer.render(
        SignalSample(19),
        256,
        &cancelled,
        |_, _| -> Result<RootSignalBlock, SignalTransferError> {
            panic!("cancelled work cannot request PCM")
        },
    );
    assert!(matches!(
        result,
        Err(SignalTransferError::Preparation(
            PreparationError::Cancelled
        ))
    ));
    let cancelled = AtomicBool::new(false);
    let mut calls = 0;
    let result = transfer.render(SignalSample(19), 256, &cancelled, |at, count| {
        calls += 1;
        cancelled.store(true, Ordering::Relaxed);
        Ok::<_, SignalTransferError>(carrier.read(at, count))
    });
    assert_eq!(calls, 1);
    assert!(matches!(
        result,
        Err(SignalTransferError::Preparation(
            PreparationError::Cancelled
        ))
    ));
    assert!(matches!(
        transfer.render(SignalSample(19), 256, &AtomicBool::new(false), |_, _| {
            Err::<RootSignalBlock, _>(SignalTransferError::Range)
        }),
        Err(SignalTransferError::Range)
    ));
    for (start, count) in [(19, 0), (19, 257), (18, 1), (530, 2), (i64::MAX, 1)] {
        let mut called = false;
        assert!(
            transfer
                .render(
                    SignalSample(start),
                    count,
                    &AtomicBool::new(false),
                    |at, n| {
                        called = true;
                        Ok::<_, SignalTransferError>(carrier.read(at, n))
                    }
                )
                .is_err()
        );
        assert!(!called);
    }
    for step in [ExactRatio::ZERO, ratio(-1, 1), ratio(1, 65), ratio(65, 1)] {
        assert!(
            RootSignalTransfer::new(
                carrier.support(),
                ExactRatio::ZERO,
                SignalSample(19),
                step,
                signals(19, 531)
            )
            .is_err()
        );
    }
    for support in [roots(7, 7), roots(8, 7), roots(-1, 7)] {
        assert!(
            RootSignalTransfer::new(
                support,
                ExactRatio::ZERO,
                SignalSample(19),
                ExactRatio::ONE,
                signals(19, 531)
            )
            .is_err()
        );
    }
    for output in [signals(19, 19), signals(20, 19), signals(-1, 19)] {
        assert!(
            RootSignalTransfer::new(
                carrier.support(),
                ExactRatio::ZERO,
                SignalSample(19),
                ExactRatio::ONE,
                output
            )
            .is_err()
        );
    }
}

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn duration(frames: i64) -> FrameDuration {
    FrameDuration::new(frames).unwrap()
}

fn ntsc() -> FrameRate {
    FrameRate::new(30_000, 1001).unwrap()
}

fn boundary(frame: i64) -> AudioSample {
    ntsc().audio_boundary(ProjectFrame(frame)).unwrap()
}

#[derive(Clone, Copy)]
enum InputGap {
    SilentHold,
    AbsentSource,
    OutsidePlacement,
}

fn reference(gap: InputGap) -> Arc<AudioReferencePlan> {
    let clock = SourceTimeBase::new(1, 48_000).unwrap();
    let audio = SourceAudio {
        asset: AssetId::new("signal").unwrap(),
        span: SourceSpan::new(
            SourceTimestamp {
                ticks: 0,
                time_base: clock,
            },
            SourceTimestamp {
                ticks: 16_016,
                time_base: clock,
            },
        )
        .unwrap(),
    };
    let source = |frames| BeatNode {
        framing: None,
        label: "Synthetic input".into(),
        audio_edges: Default::default(),
        kind: NodeKind::Source {
            source: SourceNode {
                duration: duration(frames),
                video: SourceVideo::Blank,
                video_mapping: SourceVideoMapping::FitBeat,
                audio: Some(audio.clone()),
                audio_mapping: SourceAudioMapping::FitBeat,
                audio_offset: AudioSample(0),
                link: LinkRelation::Independent,
            },
        },
    };
    let quiet = match gap {
        InputGap::SilentHold => BeatNode::hold(
            "Silent",
            HoldRecipe {
                duration: duration(2),
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
            },
        ),
        InputGap::AbsentSource | InputGap::OutsidePlacement => {
            let mut quiet = source(2);
            let NodeKind::Source { source } = &mut quiet.kind else {
                unreachable!()
            };
            if matches!(gap, InputGap::AbsentSource) {
                source.audio = None;
                source.video = SourceVideo::Still {
                    asset: audio.asset.clone(),
                };
            } else {
                source.audio_mapping = SourceAudioMapping::Placement {
                    start: ExactRatio::integer(4),
                    frames: ExactRatio::integer(2),
                };
            }
            quiet
        }
    };
    let document = ProjectDocument::new(
        ProjectId::new("root-signal-transfer").unwrap(),
        RevisionId::new("before").unwrap(),
        PresentationBasis {
            width: 640,
            height: 360,
            frame_rate: ntsc(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut wire = serde_json::to_value(document).unwrap();
    wire["nodes"] = serde_json::to_value(BTreeMap::from([
        (id("root"), BeatNode::sequence("Root", vec![id("preserve")])),
        (
            id("preserve"),
            BeatNode {
                framing: None,
                label: "Full Preserve history".into(),
                audio_edges: Default::default(),
                kind: NodeKind::Retime {
                    child: id("input"),
                    duration: duration(5),
                    mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(10)).unwrap(),
                    pitch: PitchPolicy::Preserve,
                    purpose: RetimePurpose::Edit,
                },
            },
        ),
        (
            id("input"),
            BeatNode::sequence("Input", vec![id("left"), id("quiet"), id("right")]),
        ),
        (id("left"), source(4)),
        (id("quiet"), quiet),
        (id("right"), source(4)),
    ]))
    .unwrap();
    wire["assets"] = serde_json::to_value(BTreeMap::from([(
        audio.asset.clone(),
        AssetRecord {
            label: "Synthetic PCM".into(),
            content_hash: "a".repeat(64),
            video: None,
            audio: Some(audio.span),
            still_image: true,
            frame_count: None,
            source_qualification: None,
        },
    )]))
    .unwrap();
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let frozen = FrozenAudioLayout::capture(&document).unwrap();
    let reopened = FrozenAudioLayout::from_json(&frozen.to_json().unwrap()).unwrap();
    Arc::new(AudioReferencePlan::compile(&reopened).unwrap())
}

fn stretch(input: &[[f32; 2]], output: usize, rate: StretchRate) -> Vec<[f32; 2]> {
    let pcm = StereoPcm::new(
        input.iter().map(|s| s[0]).collect(),
        input.iter().map(|s| s[1]).collect(),
    )
    .unwrap();
    let recipe = CanonicalRecipe::with_rate(
        u32::try_from(input.len()).unwrap(),
        u32::try_from(output).unwrap(),
        rate,
        0,
    )
    .unwrap();
    let mut dsp = CanonicalStretch::new(recipe, pcm).unwrap();
    let mut result = Vec::with_capacity(output);
    while result.len() < output {
        let count = (output - result.len()).min(256);
        let mut left = vec![0.0; count];
        let mut right = vec![0.0; count];
        assert_eq!(
            dsp.read(&mut left, &mut right, &AtomicBool::new(false))
                .unwrap(),
            count
        );
        result.extend(left.into_iter().zip(right).map(|(l, r)| [l, r]));
    }
    result
}

fn first_preserve(reference: &AudioReferencePlan) -> Vec<[f32; 2]> {
    // The input quiet region is the point-ceil interval [4,6) frames. The
    // canonical stage compresses ten input frames into five output frames.
    let input: Vec<_> = (0..16_016)
        .map(|n| {
            if (6407..9610).contains(&n) {
                [0.0; 2]
            } else {
                [
                    (0.4 * (n as f64 * 0.071).sin() + 0.1 * (n as f64 * 0.173).cos()) as f32,
                    (0.3 * (n as f64 * 0.043).cos()) as f32,
                ]
            }
        })
        .collect();
    let mut output = stretch(&input, 8008, StretchRate::new(2, 1).unwrap());
    let stage = InstancePath {
        node: id("preserve"),
        repeats: vec![],
    };
    for span in reference
        .preserve_output_clock(&stage)
        .unwrap()
        .query(
            ReferenceSample(0)..ReferenceSample(8008),
            AudioQueryLimits::default(),
        )
        .unwrap()
        .spans
    {
        if matches!(
            span.content,
            ReferenceAudioContent::Silence {
                reason: SilenceReason::SilentHold
            }
        ) {
            output[span.samples.start.0 as usize..span.samples.end.0 as usize].fill([0.0; 2]);
        }
    }
    output
}

fn resumed_root(
    reference: Arc<AudioReferencePlan>,
    prepared: &[[f32; 2]],
    current: &[Range<AudioSample>],
) -> (Carrier, Vec<[f32; 2]>) {
    assert_eq!((boundary(1).0, boundary(2).0), (1602, 3203));
    let policy =
        RetainedRootPolicy::new(reference, boundary(1), boundary(2), roots(3203, 9610)).unwrap();
    let mut carrier = Carrier {
        start: 0,
        samples: (0..9610)
            .map(|n| {
                if n < 1602 {
                    prepared[n]
                } else if n < 3203 {
                    [0.0; 2]
                } else {
                    prepared.get(n - 1601).copied().unwrap_or([0.0; 2])
                }
            })
            .collect(),
        suppressed: vec![roots(1602, 3203)],
    };
    let mut materialized = carrier.samples.clone();
    for start in (3203..9610).step_by(256) {
        let end = (start + 256).min(9610);
        let mask = policy
            .apply(
                AudioSample(start as i64),
                &mut materialized[start..end],
                current,
                Default::default(),
                &AtomicBool::new(false),
            )
            .unwrap();
        carrier.suppressed.extend(mask);
    }
    assert_eq!(carrier.materialized(true), materialized);
    (carrier, materialized)
}

#[test]
fn real_preserve_root_rounding_hole_stays_closed_through_transfer_and_another_preserve() {
    let reference = reference(InputGap::SilentHold);
    let prepared = first_preserve(&reference);
    assert!(prepared[3203].iter().any(|v| v.abs() > 1e-5));
    assert_eq!(prepared[3204], [0.0; 2]);
    let current = [boundary(3)..boundary(4)];
    let (carrier, full_root) = resumed_root(reference, &prepared, &current);
    assert_eq!(carrier.samples[4804], prepared[3203]);
    assert_eq!(full_root[4804], [0.0; 2]);
    let mapping = Mapping {
        root_at_anchor: ratio(110 * 147, 160).checked_add(ratio(1, 3)).unwrap(),
        anchor: SignalSample(17113),
        step: ratio(147, 160),
        output: signals(17003, 27463),
    };
    let transfer = mapping.transfer(&carrier);
    let mut parts = Vec::new();
    let mut cursor = 17003;
    for count in [173, 1, 255, 31, 97].into_iter().cycle() {
        if cursor == 27463 {
            break;
        }
        let frames = count.min(27463 - cursor);
        parts.push((cursor, frames as u32));
        cursor += frames;
    }
    let mut actual = vec![[0.0; 2]; 10_460];
    let mut expected = actual.clone();
    // Materialize in reverse query order so the resumed suffix prepares first.
    for (start, count) in parts.into_iter().rev() {
        let part = transfer
            .render(
                SignalSample(start),
                count,
                &AtomicBool::new(false),
                |at, frames| Ok::<_, SignalTransferError>(carrier.read(at, frames)),
            )
            .unwrap();
        let oracle = oracle(&carrier, &mapping, SignalSample(start), count, true);
        assert_eq!(part.samples, oracle.0);
        assert_eq!(part.suppressed, oracle.1);
        let offset = usize::try_from(start - 17003).unwrap();
        actual[offset..offset + count as usize].copy_from_slice(&part.samples);
        expected[offset..offset + count as usize].copy_from_slice(&oracle.0);
    }
    let rate = StretchRate::new(5, 4).unwrap();
    let after_second = stretch(&actual, 8368, rate);
    assert_eq!(after_second, stretch(&expected, 8368, rate));
    assert!(after_second.iter().flatten().any(|v| v.abs() > 1e-4));

    // A root consumer using only the current Hold misses old-root sample 3203
    // after resume. The error survives both sampling and a later real Preserve.
    let wrong = Carrier {
        start: carrier.start,
        samples: carrier.samples.clone(),
        suppressed: vec![roots(1602, 3203), current[0].clone(), roots(9609, 9610)],
    };
    let mut wrong_input = Vec::new();
    while wrong_input.len() < actual.len() {
        let count = (actual.len() - wrong_input.len()).min(256) as u32;
        wrong_input.extend(
            oracle(
                &wrong,
                &mapping,
                SignalSample(17003 + wrong_input.len() as i64),
                count,
                true,
            )
            .0,
        );
    }
    let wrong_output = stretch(&wrong_input, 8368, rate);
    assert!(after_second.iter().zip(wrong_output).any(|(good, wrong)| {
        (good[0] - wrong[0]).abs() > 1e-7 || (good[1] - wrong[1]).abs() > 1e-7
    }));
}

#[test]
fn old_no_input_regions_keep_real_preserve_decay_after_root_to_signal_transfer() {
    for gap in [InputGap::AbsentSource, InputGap::OutsidePlacement] {
        let reference = reference(gap);
        let prepared = first_preserve(&reference);
        let decay = &prepared[3204..3268];
        assert!(decay.iter().flatten().any(|v| v.abs() > 1e-5));
        let (carrier, full_root) = resumed_root(reference, &prepared, &[]);
        assert_eq!(&full_root[4805..4869], decay);
        let mapping = Mapping {
            root_at_anchor: ExactRatio::integer(4805),
            anchor: SignalSample(9803),
            step: ExactRatio::ONE,
            output: signals(9803, 9867),
        };
        let actual = mapping
            .transfer(&carrier)
            .render(
                SignalSample(9803),
                64,
                &AtomicBool::new(false),
                |at, count| Ok::<_, SignalTransferError>(carrier.read(at, count)),
            )
            .unwrap();
        assert_eq!(actual.samples, decay);
        assert!(actual.suppressed.is_empty());
    }
}

#[test]
fn retained_domain_resume_drives_fractional_transfer_into_another_real_preserve() {
    let reference = reference(InputGap::SilentHold);
    let domain = reference
        .root_clock()
        .processing_domain_at(ReferenceSample(1602), Default::default())
        .unwrap();
    assert!(matches!(
        domain.kind(),
        deadpan_plan::ReferenceProcessingKind::Preserve { .. }
    ));
    let resumed = domain
        .place_root(AudioSample(0))
        .unwrap()
        .resume(boundary(1), boundary(2))
        .unwrap()
        .resume(boundary(3), boundary(4))
        .unwrap();
    assert_eq!(
        resumed.reference_position(boundary(4)).unwrap(),
        ExactRatio::integer(3204)
    );

    // Full intrinsic DSP history is retained. The old root's explicit silent
    // interval differs from its prepared-point interval at sample 3203.
    let carrier = Carrier {
        start: 0,
        samples: first_preserve(&reference),
        suppressed: vec![roots(3203, 4805)],
    };
    assert!(carrier.samples[3203].iter().any(|v| v.abs() > 1e-5));
    let mapping = Mapping {
        root_at_anchor: resumed
            .reference_position_at(ExactRatio::integer(6406).checked_add(ratio(1, 3)).unwrap())
            .unwrap(),
        anchor: SignalSample(10_000),
        step: ratio(147, 160),
        output: signals(10_000, 13_000),
    };
    // Independent expected anchor, not reconstructed from the current picture
    // frame or obtained from the domain map under test.
    let expected_mapping = Mapping {
        root_at_anchor: ratio(9613, 3),
        ..mapping.clone()
    };
    assert_eq!(mapping.root_at_anchor, expected_mapping.root_at_anchor);
    let transfer = mapping.transfer(&carrier);
    let mut actual = vec![[0.0; 2]; 3000];
    let mut expected = actual.clone();
    let mut wrong = actual.clone();
    let wrong_mapping = Mapping {
        root_at_anchor: ratio(9610, 3),
        ..mapping.clone()
    };
    // Suffix-first reads force the map to be independent of request order.
    for offset in (0..3000).step_by(173).collect::<Vec<_>>().into_iter().rev() {
        let frames = (3000 - offset).min(173) as u32;
        let start = SignalSample(10_000 + offset as i64);
        let block = transfer
            .render(start, frames, &AtomicBool::new(false), |at, count| {
                Ok::<_, SignalTransferError>(carrier.read(at, count))
            })
            .unwrap();
        let oracle = oracle(&carrier, &expected_mapping, start, frames, true);
        assert_eq!(block.samples, oracle.0);
        assert_eq!(block.suppressed, oracle.1);
        actual[offset..offset + frames as usize].copy_from_slice(&block.samples);
        expected[offset..offset + frames as usize].copy_from_slice(&oracle.0);
        wrong[offset..offset + frames as usize]
            .copy_from_slice(&self::oracle(&carrier, &wrong_mapping, start, frames, true).0);
    }
    let rate = StretchRate::new(6, 5).unwrap();
    let after = stretch(&actual, 2500, rate);
    assert_eq!(after, stretch(&expected, 2500, rate));
    let wrong = stretch(&wrong, 2500, rate);
    assert!(
        after
            .iter()
            .zip(wrong)
            .any(|(a, b)| (a[0] - b[0]).abs() > 1e-6)
    );
}
