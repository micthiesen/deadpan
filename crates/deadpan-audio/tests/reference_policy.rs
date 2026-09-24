#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use deadpan_audio::{ReferencePolicyError, RetainedRootPolicy};
use deadpan_core::*;
use deadpan_dsp::{CanonicalRecipe, CanonicalStretch, StereoPcm, StretchRate};
use deadpan_plan::{
    AudioQueryLimits, AudioReferencePlan, ReferenceAudioContent, ReferenceSample, SilenceReason,
};

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn duration(frames: i64) -> FrameDuration {
    FrameDuration::new(frames).unwrap()
}
fn rate() -> FrameRate {
    FrameRate::new(30_000, 1001).unwrap()
}
fn boundary(frame: i64) -> AudioSample {
    rate().audio_boundary(ProjectFrame(frame)).unwrap()
}
fn samples(start: i64, end: i64) -> std::ops::Range<AudioSample> {
    AudioSample(start)..AudioSample(end)
}

fn document(hold_start: i64) -> ProjectDocument {
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
        label: "Synthetic input region".into(),
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
    let empty = ProjectDocument::new(
        ProjectId::new("reference-mask").unwrap(),
        RevisionId::new("before").unwrap(),
        PresentationBasis {
            width: 640,
            height: 360,
            frame_rate: rate(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut wire = serde_json::to_value(empty).unwrap();
    wire["nodes"] = serde_json::to_value(BTreeMap::from([
        (id("root"), BeatNode::sequence("Root", vec![id("preserve")])),
        (
            id("preserve"),
            BeatNode {
                label: "Whole history".into(),
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
            BeatNode::sequence("Input", vec![id("left"), id("hold"), id("right")]),
        ),
        (id("left"), source(hold_start * 2)),
        (
            id("hold"),
            BeatNode::hold(
                "Quiet",
                HoldRecipe {
                    duration: duration(2),
                    video: HoldVideo::Background,
                    audio: HoldAudio::Silence,
                },
            ),
        ),
        (id("right"), source(8 - hold_start * 2)),
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
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn plan(hold_start: i64) -> Arc<AudioReferencePlan> {
    reference_plan(&document(hold_start))
}

fn reference_plan(document: &ProjectDocument) -> Arc<AudioReferencePlan> {
    let captured = FrozenAudioLayout::capture(document).unwrap();
    // Round-trip the frozen values, not live document or media handles.
    let reopened = FrozenAudioLayout::from_json(&captured.to_json().unwrap()).unwrap();
    Arc::new(AudioReferencePlan::compile(&reopened).unwrap())
}

fn prepared(plan: &AudioReferencePlan, hold_start: i64) -> Vec<[f32; 2]> {
    let points = |frame| {
        ExactRatio::new(i128::from(frame) * 8008, 5)
            .unwrap()
            .ceil()
            .unwrap() as usize
    };
    let silent = points(hold_start * 2)..points((hold_start + 1) * 2);
    let input: Vec<[f32; 2]> = (0..16_016)
        .map(|n| {
            if silent.contains(&n) {
                [0.0; 2]
            } else {
                [
                    (0.4 * (n as f64 * 0.071).sin() + 0.1 * (n as f64 * 0.173).cos()) as f32,
                    (0.3 * (n as f64 * 0.043).cos()) as f32,
                ]
            }
        })
        .collect();
    let pcm = StereoPcm::new(
        input.iter().map(|s| s[0]).collect(),
        input.iter().map(|s| s[1]).collect(),
    )
    .unwrap();
    let recipe =
        CanonicalRecipe::with_rate(16_016, 8_008, StretchRate::new(2, 1).unwrap(), 0).unwrap();
    let mut dsp = CanonicalStretch::new(recipe, pcm).unwrap();
    let mut result = Vec::new();
    while result.len() < 8_008 {
        let count = (8_008 - result.len()).min(256);
        let mut left = vec![0.0; count];
        let mut right = vec![0.0; count];
        assert_eq!(
            dsp.read(&mut left, &mut right, &AtomicBool::new(false))
                .unwrap(),
            count
        );
        result.extend(left.into_iter().zip(right).map(|(l, r)| [l, r]));
    }
    // Reapply the intrinsic output policy on its own point grid, as canonical
    // stage preparation does. This is different from root rounding.
    let stage = InstancePath {
        node: id("preserve"),
        repeats: vec![],
    };
    for span in plan
        .preserve_output_clock(&stage)
        .unwrap()
        .query(
            ReferenceSample(0)..ReferenceSample(8_008),
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
            result[span.samples.start.0 as usize..span.samples.end.0 as usize].fill([0.0; 2]);
        }
    }
    result
}

#[test]
fn retained_root_silence_closes_the_prepared_point_grid_hole_after_resume() {
    let reference = plan(2);
    let pcm = prepared(&reference, 2);
    assert!(
        pcm[3203].iter().any(|v| v.abs() > 1e-5),
        "the prepared buffer must exhibit the missed root mute"
    );
    assert_eq!(pcm[3204], [0.0; 2]);
    let policy =
        RetainedRootPolicy::new(reference, boundary(1), boundary(2), samples(3203, 9_610)).unwrap();
    let current = [boundary(3)..boundary(4)];
    let mut actual = pcm[3202..3206].to_vec();
    let muted = policy
        .apply(
            AudioSample(4803),
            &mut actual,
            &current,
            Default::default(),
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(muted, samples_list(&[(4804, 4807)]));
    assert_eq!(actual[0], pcm[3202]);
    assert_eq!(actual[1..], [[0.0; 2]; 3]);
    // Applying only the current structural Hold would leave 4804 audible.
    assert!(pcm[3203].iter().any(|v| *v != 0.0));
}

fn samples_list(ranges: &[(i64, i64)]) -> Vec<std::ops::Range<AudioSample>> {
    ranges.iter().map(|(a, b)| samples(*a, *b)).collect()
}

#[test]
fn absent_source_and_placement_gaps_preserve_prepared_decay() {
    for outside_placement in [false, true] {
        let document = document(2);
        let mut quiet = document.nodes()[&id("left")].clone();
        let NodeKind::Source { source } = &mut quiet.kind else {
            panic!("source fixture");
        };
        source.duration = duration(2);
        if outside_placement {
            source.audio_mapping = SourceAudioMapping::Placement {
                start: ExactRatio::integer(4),
                frames: ExactRatio::integer(2),
            };
        } else {
            source.audio = None;
            source.video = SourceVideo::Still {
                asset: AssetId::new("signal").unwrap(),
            };
        }
        let mut wire = serde_json::to_value(document).unwrap();
        wire["nodes"]["hold"] = serde_json::to_value(quiet).unwrap();
        let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
        let reference = reference_plan(&document);
        // Zero input in this region does not force processed output to zero.
        // Preserve can spread neighboring energy into either kind of gap.
        let pcm = prepared(&reference, 2);
        let expected = &pcm[3204..3268];
        assert!(expected.iter().flatten().any(|v| v.abs() > 1e-5));
        let policy =
            RetainedRootPolicy::new(reference, boundary(1), boundary(2), samples(3203, 9610))
                .unwrap();
        let mut resumed = expected.to_vec();
        let muted = policy
            .apply(
                AudioSample(4805),
                &mut resumed,
                &[],
                Default::default(),
                &AtomicBool::new(false),
            )
            .unwrap();
        assert!(
            muted.is_empty(),
            "absence of a voice is not explicit suppression"
        );
        assert_eq!(resumed, expected);
    }
}

#[test]
fn current_silence_wins_when_rounding_starts_it_before_the_retained_mask() {
    let reference = plan(1);
    let pcm = prepared(&reference, 1);
    assert!(pcm[1601].iter().any(|v| v.abs() > 1e-5));
    let policy =
        RetainedRootPolicy::new(reference, boundary(0), boundary(1), samples(1602, 9_610)).unwrap();
    let mut without_current = pcm[1600..1604].to_vec();
    policy
        .apply(
            AudioSample(3202),
            &mut without_current,
            &[],
            Default::default(),
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(without_current[1], pcm[1601]);
    let mut both = pcm[1600..1604].to_vec();
    let muted = policy
        .apply(
            AudioSample(3202),
            &mut both,
            &[boundary(2)..boundary(3)],
            Default::default(),
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(muted, samples_list(&[(3203, 3206)]));
    assert_eq!(both[0], pcm[1600]);
    assert_eq!(both[1..], [[0.0; 2]; 3]);
}

#[test]
fn reference_masks_are_partition_independent_bounded_and_atomic() {
    let reference = plan(2);
    let policy = RetainedRootPolicy::new(
        Arc::clone(&reference),
        boundary(1),
        boundary(2),
        samples(3203, 9_610),
    )
    .unwrap();
    let current = [boundary(3)..boundary(4)];
    let mut expected = vec![[0.25, -0.5]; 256];
    policy
        .apply(
            AudioSample(4760),
            &mut expected,
            &current,
            Default::default(),
            &AtomicBool::new(false),
        )
        .unwrap();
    for (offset, count) in [(173, 83), (0, 17), (17, 1), (18, 155)] {
        let mut part = vec![[0.25, -0.5]; count];
        policy
            .apply(
                AudioSample(4760 + offset as i64),
                &mut part,
                &current,
                Default::default(),
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(part, expected[offset..offset + count]);
    }
    for (limits, cancelled, silence) in [
        (
            AudioQueryLimits {
                maximum_spans: 1,
                maximum_work: 65_536,
            },
            false,
            vec![],
        ),
        (
            AudioQueryLimits {
                maximum_spans: 4096,
                maximum_work: 1,
            },
            false,
            vec![],
        ),
        (
            AudioQueryLimits {
                maximum_spans: 0,
                maximum_work: 65_536,
            },
            false,
            vec![],
        ),
        (AudioQueryLimits::default(), true, vec![]),
        (AudioQueryLimits::default(), false, samples_list(&[(2, 1)])),
        (AudioQueryLimits::default(), false, vec![samples(0, 1); 257]),
    ] {
        let mut unchanged = vec![[0.25, -0.5]; 256];
        assert!(
            policy
                .apply(
                    AudioSample(4760),
                    &mut unchanged,
                    &silence,
                    limits,
                    &AtomicBool::new(cancelled)
                )
                .is_err()
        );
        assert_eq!(unchanged, vec![[0.25, -0.5]; 256]);
    }
    let outside = RetainedRootPolicy::new(
        reference,
        AudioSample(0),
        AudioSample(i64::MIN),
        samples(i64::MAX - 2, i64::MAX),
    )
    .unwrap();
    let mut exhausted = [[0.25, -0.5]; 2];
    assert_eq!(
        outside
            .apply(
                AudioSample(i64::MAX - 2),
                &mut exhausted,
                &[],
                Default::default(),
                &AtomicBool::new(false)
            )
            .unwrap(),
        samples_list(&[(i64::MAX - 2, i64::MAX)])
    );
    assert_eq!(exhausted, [[0.0; 2]; 2]);
    assert!(matches!(
        policy.apply(
            AudioSample(0),
            &mut [[0.0; 2]; 1],
            &[],
            Default::default(),
            &AtomicBool::new(false)
        ),
        Err(ReferencePolicyError::Range)
    ));
}
