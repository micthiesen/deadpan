#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::collections::BTreeMap;
use std::io::Cursor;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use deadpan_audio::{
    AudioSourceProvider, PcmWindow, PreparationError, PreparedSource, ResampleRecipe, Resampler,
    StageAudio, StageAudioError, StageLimits, StereoMatrix,
};
use deadpan_core::*;
use deadpan_media::audio_index::AudioChannelLayout;
use deadpan_media::audio_session::{AudioSession, AudioSessionLimits};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_plan::{
    AudioDefinitionSelector, AudioRootPlacement, ReferenceSample, RenderPlan, SignalSample,
};
use sha2::{Digest, Sha256};

const TIMEOUT: Duration = Duration::from_secs(10);

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn ratio(numerator: i128, denominator: i128) -> ExactRatio {
    ExactRatio::new(numerator, denominator).unwrap()
}

fn selector() -> AudioDefinitionSelector {
    AudioDefinitionSelector::RepeatGap {
        repeat: id("repeat"),
    }
}

fn audio(selected: Range<i64>) -> SourceAudio {
    let time_base = SourceTimeBase::new(1, 44_100).unwrap();
    SourceAudio {
        asset: AssetId::new("media").unwrap(),
        span: SourceSpan::new(
            SourceTimestamp {
                ticks: selected.start,
                time_base,
            },
            SourceTimestamp {
                ticks: selected.end,
                time_base,
            },
        )
        .unwrap(),
    }
}

fn gap(duration: i64, audio: HoldAudio) -> HoldRecipe {
    HoldRecipe {
        picture_context: None,
        duration: FrameDuration::new(duration).unwrap(),
        video: HoldVideo::Background,
        audio,
    }
}

fn room_gap(duration: i64, selected: Range<i64>) -> HoldRecipe {
    gap(
        duration,
        HoldAudio::RoomTone {
            source: audio(selected),
        },
    )
}

fn document(rate: FrameRate, recipe: Option<HoldRecipe>, revision: &str) -> ProjectDocument {
    let empty = ProjectDocument::new(
        ProjectId::new("gap-definition").unwrap(),
        RevisionId::new(revision).unwrap(),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: rate,
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let child_audio = audio(1000..4000);
    let child = BeatNode {
        framing: None,
        label: "Audible child that is not the gap".into(),
        audio_treatments: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Source {
            source: SourceNode {
                duration: FrameDuration::new(3).unwrap(),
                video: SourceVideo::Blank,
                video_mapping: SourceVideoMapping::FitBeat,
                audio_mapping: SourceAudioMapping::natural_rate(child_audio.span, rate).unwrap(),
                audio: Some(child_audio),
                audio_offset: AudioSample(0),
                link: LinkRelation::Independent,
            },
        },
    };
    let repeat = BeatNode {
        framing: None,
        label: "One play, configured gap".into(),
        audio_treatments: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Repeat {
            child: id("child"),
            iterations: IterationOrder::new(RevisionId::new("plays").unwrap(), 1).unwrap(),
            gap: recipe,
        },
    };
    let mut wire = serde_json::to_value(empty).unwrap();
    wire["nodes"] = serde_json::to_value(BTreeMap::from([
        (id("root"), BeatNode::sequence("Root", vec![id("repeat")])),
        (id("repeat"), repeat),
        (id("child"), child),
    ]))
    .unwrap();
    wire["assets"] = serde_json::to_value(BTreeMap::from([(
        AssetId::new("media").unwrap(),
        AssetRecord {
            label: "Synthetic 44.1 kHz PCM".into(),
            content_hash: "a".repeat(64),
            video: None,
            audio: Some(audio(0..44_117).span),
            still_image: true,
            frame_count: None,
            source_qualification: None,
        },
    )]))
    .unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn compile(document: &ProjectDocument, frozen: bool) -> Arc<RenderPlan> {
    Arc::new(if frozen {
        let context = FrozenAudioContext::capture(document).unwrap();
        let restored = FrozenAudioContext::from_json(&context.to_json().unwrap()).unwrap();
        RenderPlan::compile_audio_context(&restored).unwrap()
    } else {
        RenderPlan::compile(document).unwrap()
    })
}

struct FixtureProvider {
    prepared: PreparedSource,
    revision: RevisionId,
    calls: usize,
    context_calls: usize,
    unavailable: bool,
    cancel_on_call: bool,
}

impl FixtureProvider {
    fn new(revision: &str) -> Self {
        let bytes = std::fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../native/deadpan-source/tests/audio-fixtures/pcm-mono-44100.wav"),
        )
        .unwrap();
        let cancelled = AtomicBool::new(false);
        let session = AudioSession::open_verified(
            &mut Cursor::new(&bytes),
            SourceContentIdentity::new(Sha256::digest(&bytes).into(), bytes.len() as u64).unwrap(),
            0,
            AudioSessionLimits::default(),
            &cancelled,
        )
        .unwrap();
        let index = session.index().clone();
        let prepared = PreparedSource::with_layout(
            session,
            &index,
            AudioChannelLayout::Native {
                channels: 1,
                mask: 4,
            },
            &cancelled,
        )
        .unwrap();
        Self {
            prepared,
            revision: RevisionId::new(revision).unwrap(),
            calls: 0,
            context_calls: 0,
            unavailable: false,
            cancel_on_call: false,
        }
    }
}

impl AudioSourceProvider for FixtureProvider {
    fn source(
        &mut self,
        project: &ProjectId,
        revision: &RevisionId,
        asset: &AssetId,
        cancelled: &AtomicBool,
    ) -> Result<&PreparedSource, PreparationError> {
        assert_eq!(project, &ProjectId::new("gap-definition").unwrap());
        assert_eq!(revision, &self.revision);
        assert_eq!(asset, &AssetId::new("media").unwrap());
        assert!(!cancelled.load(Ordering::Relaxed));
        self.calls += 1;
        if self.unavailable {
            return Err(PreparationError::SourceUnavailable(
                "gap media revoked".into(),
            ));
        }
        if self.cancel_on_call {
            cancelled.store(true, Ordering::Relaxed);
        }
        Ok(&self.prepared)
    }

    fn source_for_context(
        &mut self,
        project: &ProjectId,
        revision: &RevisionId,
        asset: &AssetId,
        expected: &AssetRecord,
        cancelled: &AtomicBool,
    ) -> Result<&PreparedSource, PreparationError> {
        self.context_calls += 1;
        assert_eq!(expected.content_hash, "a".repeat(64));
        assert_eq!(expected.audio, Some(audio(0..44_117).span));
        self.source(project, revision, asset, cancelled)
    }
}

fn chunks(
    length: usize,
    sizes: &[u32],
    mut read: impl FnMut(i64, u32) -> Vec<[f32; 2]>,
) -> Vec<[f32; 2]> {
    let mut result = Vec::with_capacity(length);
    let mut part = 0;
    while result.len() < length {
        let count = sizes[part % sizes.len()].min(u32::try_from(length - result.len()).unwrap());
        result.extend(read(i64::try_from(result.len()).unwrap(), count));
        part += 1;
    }
    result
}

// The oracle selects all coordinates independently of the plan under test.
// Resampler supplies only the qualified kernel, not phase, support or duration.
fn sample_reference(
    selection: Range<i64>,
    origin: ExactRatio,
    step: ExactRatio,
    length: usize,
    value: impl Fn(i64) -> [f32; 2],
) -> Vec<[f32; 2]> {
    let sampler = Resampler::new(
        ResampleRecipe::new(
            selection,
            origin,
            AudioSample(0),
            step,
            AudioSample(0)..AudioSample(i64::try_from(length).unwrap()),
        )
        .unwrap(),
        StereoMatrix::new(AudioChannelLayout::Native {
            channels: 2,
            mask: 3,
        })
        .unwrap(),
    );
    chunks(length, &[256], |offset, count| {
        let at = AudioSample(offset);
        let window = sampler
            .required_source_range(at, count)
            .unwrap()
            .map(|range| PcmWindow {
                start: range.start,
                samples: range.flat_map(&value).collect(),
            });
        sampler
            .render(at, count, window, &AtomicBool::new(false))
            .unwrap()
            .samples
    })
}

fn room_reference(selected: Range<i64>, length: usize) -> Vec<[f32; 2]> {
    let extent = ratio(i128::from(selected.end - selected.start) * 160, 147);
    let input = sample_reference(
        selected.clone(),
        ExactRatio::integer(selected.start),
        ratio(147, 160),
        usize::try_from(extent.ceil().unwrap()).unwrap(),
        |at| [(((at * 73) % 65_536 - 32_768) as f32) / 32_768.0; 2],
    );
    // This fixture's 221 source samples span 35360/147 mix samples. The
    // 96-sample crossfade leaves the exact loop period 21248/147, not 145.
    let fade = ExactRatio::integer(96);
    let period = extent.checked_sub(fade).unwrap();
    let sample = |at| {
        sample_reference(0..input.len() as i64, at, ExactRatio::ONE, 1, |index| {
            input[index as usize]
        })[0]
    };
    (0..length)
        .map(|frame| {
            let at = ExactRatio::integer(i64::try_from(frame).unwrap());
            let cycle = at.checked_div(period).unwrap().floor();
            let local = at
                .checked_sub(period.checked_mul(ratio(cycle, 1)).unwrap())
                .unwrap();
            let head = sample(local);
            if cycle == 0 || local.checked_sub(fade).unwrap().compare_integer(0).is_ge() {
                head
            } else {
                let tail = sample(period.checked_add(local).unwrap());
                let weight = local.checked_div(fade).unwrap();
                let weight = weight.numerator() as f64 / weight.denominator() as f64;
                std::array::from_fn(|channel| {
                    (f64::from(tail[channel]) * (1.0 - weight) + f64::from(head[channel]) * weight)
                        as f32
                })
            }
        })
        .collect()
}

fn assert_pcm_close(actual: &[[f32; 2]], expected: &[[f32; 2]]) {
    assert_eq!(actual.len(), expected.len());
    for (frame, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        for channel in 0..2 {
            assert!(actual[channel].is_finite());
            assert!(
                (actual[channel] - expected[channel]).abs() <= 2e-6,
                "frame {frame}, channel {channel}: {} != {}",
                actual[channel],
                expected[channel]
            );
        }
    }
}

#[test]
fn one_play_gap_renders_44100_pcm_without_child_leakage_or_loop_phase_rounding() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let doc = document(rate, Some(room_gap(3, 100..321)), "r0");
    // Three NTSC frames have 4804.8 points of support and own 4805 PointCeil samples.
    let expected = room_reference(100..321, 4805);
    let cancelled = AtomicBool::new(false);
    for frozen in [false, true] {
        let plan = compile(&doc, frozen);
        let definition = plan.audio_definition(selector()).unwrap();
        assert_eq!(
            definition.signal().sample_count().unwrap(),
            SignalSample(4805)
        );
        let mut provider = FixtureProvider::new("r0");
        let mut renderer = StageAudio::new(Arc::clone(&plan));
        let actual = chunks(expected.len(), &[1, 73, 256, 17, 91], |at, count| {
            let block = renderer
                .read_definition(
                    &mut provider,
                    &definition,
                    SignalSample(at),
                    count,
                    TIMEOUT,
                    &cancelled,
                )
                .unwrap();
            assert_eq!(block.definition, selector());
            assert_eq!(block.root, id("repeat"));
            assert!(block.suppressed.is_empty());
            block.samples
        });
        assert_pcm_close(&actual, &expected);
        assert!(actual.iter().all(|sample| sample[0] == sample[1]));
        assert_eq!(renderer.cached_stage_count(), 1);
        assert_eq!(provider.context_calls > 0, frozen);
        let child = renderer
            .read(&mut provider, AudioSample(0), 128, TIMEOUT, &cancelled)
            .unwrap();
        assert_ne!(
            child.samples,
            actual[..128],
            "one play has no root gap occurrence"
        );
        for (at, count) in [(4786, 19), (143, 21), (720, 127), (0, 128)] {
            let mut fresh = StageAudio::new(Arc::clone(&plan));
            let sought = fresh
                .read_definition(
                    &mut provider,
                    &definition,
                    SignalSample(at),
                    count,
                    TIMEOUT,
                    &cancelled,
                )
                .unwrap();
            assert_eq!(
                sought.samples,
                actual[at as usize..at as usize + count as usize]
            );
        }
    }
}

#[test]
fn root_and_point_gap_clocks_keep_signed_ntsc_phase_and_full_finite_filter_context() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let plan = compile(&document(rate, Some(room_gap(3, 100..321)), "r0"), false);
    let definition = plan.audio_definition(selector()).unwrap();
    let intrinsic = room_reference(100..321, 4805);
    let samples_per_frame = ratio(8008, 5);
    let grid_origin = ratio(5, 7);
    let cancelled = AtomicBool::new(false);
    let mut provider = FixtureProvider::new("r0");
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    for (origin, scale, support) in [
        (ratio(-7, 3), ratio(3, 2), ratio(1, 7)..ratio(17, 7)),
        (
            ratio(11, 7),
            ratio(5, 4),
            ExactRatio::ZERO..ExactRatio::integer(3),
        ),
    ] {
        let placement = AudioRootPlacement::new(origin, scale, support.clone()).unwrap();
        let start = origin
            .checked_add(support.start.checked_mul(scale).unwrap())
            .unwrap();
        let end = origin
            .checked_add(support.end.checked_mul(scale).unwrap())
            .unwrap();
        let root_start = i64::try_from(
            start
                .checked_mul(samples_per_frame)
                .unwrap()
                .round_even()
                .unwrap(),
        )
        .unwrap();
        let root_end = i64::try_from(
            end.checked_mul(samples_per_frame)
                .unwrap()
                .round_even()
                .unwrap(),
        )
        .unwrap();
        let root = definition.in_root_clock(placement.clone()).unwrap();
        assert_eq!(
            root.root_samples(),
            AudioSample(root_start)..AudioSample(root_end)
        );
        let phase = ExactRatio::integer(root_start)
            .checked_sub(origin.checked_mul(samples_per_frame).unwrap())
            .unwrap()
            .checked_div(scale)
            .unwrap();
        let step = ExactRatio::ONE.checked_div(scale).unwrap();
        let expected = sample_reference(
            0..4805,
            phase,
            step,
            (root_end - root_start) as usize,
            |at| intrinsic[at as usize],
        );
        let actual = chunks(expected.len(), &[91, 256, 1, 33], |offset, count| {
            let block = renderer
                .read_domain(
                    &mut provider,
                    &root,
                    AudioSample(root_start + offset),
                    count,
                    TIMEOUT,
                    &cancelled,
                )
                .unwrap();
            assert_eq!(block.definition, Some(selector()));
            assert_eq!(block.instance.node, id("repeat"));
            assert!(block.instance.repeats.is_empty());
            assert!(block.gap_after.is_none());
            assert!(block.suppressed.is_empty());
            block.samples
        });
        assert_pcm_close(&actual, &expected);
        let mut fresh = StageAudio::new(Arc::clone(&plan));
        let sought = fresh
            .read_domain(
                &mut provider,
                &root,
                AudioSample(root_end - 23),
                23,
                TIMEOUT,
                &cancelled,
            )
            .unwrap();
        assert_eq!(sought.samples, actual[actual.len() - 23..]);

        let point_start = i64::try_from(
            start
                .checked_sub(grid_origin)
                .unwrap()
                .checked_mul(samples_per_frame)
                .unwrap()
                .ceil()
                .unwrap(),
        )
        .unwrap();
        let point_end = i64::try_from(
            end.checked_sub(grid_origin)
                .unwrap()
                .checked_mul(samples_per_frame)
                .unwrap()
                .ceil()
                .unwrap(),
        )
        .unwrap();
        let point = definition.in_point_clock(placement, grid_origin).unwrap();
        assert_eq!(
            point.reference_samples(),
            ReferenceSample(point_start)..ReferenceSample(point_end)
        );
        let phase = grid_origin
            .checked_sub(origin)
            .unwrap()
            .checked_mul(samples_per_frame)
            .unwrap()
            .checked_add(ExactRatio::integer(point_start))
            .unwrap()
            .checked_div(scale)
            .unwrap();
        let expected = sample_reference(
            0..4805,
            phase,
            step,
            (point_end - point_start) as usize,
            |at| intrinsic[at as usize],
        );
        let actual = chunks(expected.len(), &[17, 129, 256, 3], |offset, count| {
            let block = renderer
                .read_point_domain(
                    &mut provider,
                    &point,
                    ReferenceSample(point_start + offset),
                    count,
                    TIMEOUT,
                    &cancelled,
                )
                .unwrap();
            assert_eq!(block.definition, selector());
            assert_eq!(block.reference_grid.frame_origin(), grid_origin);
            assert!(block.suppressed.is_empty());
            block.samples
        });
        assert_pcm_close(&actual, &expected);
        let mut fresh = StageAudio::new(Arc::clone(&plan));
        let sought = fresh
            .read_point_domain(
                &mut provider,
                &point,
                ReferenceSample(point_end - 23),
                23,
                TIMEOUT,
                &cancelled,
            )
            .unwrap();
        assert_eq!(sought.samples, actual[actual.len() - 23..]);
        let calls = provider.calls;
        assert!(
            renderer
                .read_domain(
                    &mut provider,
                    &root,
                    AudioSample(root_end),
                    1,
                    TIMEOUT,
                    &cancelled
                )
                .is_err()
        );
        assert!(
            renderer
                .read_point_domain(
                    &mut provider,
                    &point,
                    ReferenceSample(point_start - 1),
                    1,
                    TIMEOUT,
                    &cancelled
                )
                .is_err()
        );
        assert!(
            renderer
                .read_point_domain(
                    &mut provider,
                    &point,
                    ReferenceSample(point_end),
                    1,
                    TIMEOUT,
                    &cancelled
                )
                .is_err()
        );
        assert_eq!(provider.calls, calls);
    }
    assert_eq!(
        renderer.cached_stage_count(),
        1,
        "placement does not reset the intrinsic loop"
    );
}

#[test]
fn current_gap_recipe_changes_audio_while_preserving_explicit_clock() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let placement = AudioRootPlacement::new(
        ratio(-1, 3),
        ExactRatio::ONE,
        ExactRatio::ZERO..ExactRatio::integer(3),
    )
    .unwrap();
    let cancelled = AtomicBool::new(false);
    let mut outputs = Vec::new();
    for (revision, selected) in [("r0", 100..321), ("r1", 700..921)] {
        let plan = compile(
            &document(rate, Some(room_gap(3, selected.clone())), revision),
            true,
        );
        let definition = plan.audio_definition(selector()).unwrap();
        let domain = definition
            .in_point_clock(placement.clone(), ratio(1, 7))
            .unwrap();
        assert_eq!(
            domain.reference_samples(),
            ReferenceSample(-762)..ReferenceSample(4043)
        );
        let intrinsic = room_reference(selected, 4805);
        let expected = sample_reference(0..4805, ratio(2, 3), ExactRatio::ONE, 128, |at| {
            intrinsic[at as usize]
        });
        let mut provider = FixtureProvider::new(revision);
        let mut renderer = StageAudio::new(plan.clone());
        let actual = renderer
            .read_point_domain(
                &mut provider,
                &domain,
                ReferenceSample(-762),
                128,
                TIMEOUT,
                &cancelled,
            )
            .unwrap();
        assert_pcm_close(&actual.samples, &expected);
        outputs.push(actual.samples);
    }
    assert_ne!(outputs[0], outputs[1]);
    let plan = compile(
        &document(rate, Some(gap(3, HoldAudio::Silence)), "r2"),
        true,
    );
    let definition = plan.audio_definition(selector()).unwrap();
    let domain = definition.in_point_clock(placement, ratio(1, 7)).unwrap();
    let mut provider = FixtureProvider::new("r2");
    let mut renderer = StageAudio::new(plan.clone());
    let silent = renderer
        .read_point_domain(
            &mut provider,
            &domain,
            ReferenceSample(-762),
            128,
            TIMEOUT,
            &cancelled,
        )
        .unwrap();
    assert_eq!(silent.samples, vec![[0.0; 2]; 128]);
    assert_eq!(
        silent.suppressed,
        vec![ReferenceSample(-762)..ReferenceSample(-634)]
    );
    assert_eq!(provider.calls, 0);
}

#[test]
fn silence_policy_survives_a_zero_point_gap_and_later_expanded_placement() {
    let rate = FrameRate::new(192_000, 1).unwrap();
    let plan = compile(
        &document(rate, Some(gap(1, HoldAudio::Silence)), "r0"),
        false,
    );
    let definition = plan.audio_definition(selector()).unwrap();
    let empty = definition
        .in_point_clock(
            AudioRootPlacement::new(
                ExactRatio::ONE,
                ExactRatio::ONE,
                ExactRatio::ZERO..ExactRatio::ONE,
            )
            .unwrap(),
            ExactRatio::ZERO,
        )
        .unwrap();
    assert_eq!(
        empty.reference_samples(),
        ReferenceSample(1)..ReferenceSample(1)
    );
    assert_eq!(
        empty.signal().support(),
        ExactRatio::ONE..ExactRatio::integer(2)
    );
    let expanded = definition
        .in_point_clock(
            AudioRootPlacement::new(
                ExactRatio::integer(8),
                ExactRatio::integer(8),
                ExactRatio::ZERO..ExactRatio::ONE,
            )
            .unwrap(),
            ExactRatio::ZERO,
        )
        .unwrap();
    let mut provider = FixtureProvider::new("r0");
    let mut renderer = StageAudio::new(plan.clone());
    let block = renderer
        .read_point_domain(
            &mut provider,
            &expanded,
            ReferenceSample(2),
            2,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(block.samples, vec![[0.0; 2]; 2]);
    assert_eq!(
        block.suppressed,
        vec![ReferenceSample(2)..ReferenceSample(4)]
    );
    assert_eq!(provider.calls, 0);
    assert_eq!(renderer.cached_stage_count(), 0);
}

#[test]
fn gap_reads_recheck_cached_admission_and_enforce_preparation_limits_and_cancellation() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let plan = compile(&document(rate, Some(room_gap(3, 100..321)), "r0"), true);
    let definition = plan.audio_definition(selector()).unwrap();
    let mut provider = FixtureProvider::new("r0");
    let cancelled = AtomicBool::new(false);
    let mut limited = StageAudio::with_limits(
        plan.clone(),
        StageLimits {
            maximum_output_frames: 4804,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(matches!(
        limited.read_definition(
            &mut provider,
            &definition,
            SignalSample(720),
            1,
            TIMEOUT,
            &cancelled
        ),
        Err(StageAudioError::Limit("output frames"))
    ));
    assert_eq!(provider.calls, 0);
    assert_eq!(limited.cached_stage_count(), 0);
    let mut renderer = StageAudio::new(plan.clone());
    assert!(
        renderer
            .read_definition(
                &mut provider,
                &definition,
                SignalSample(0),
                1,
                TIMEOUT,
                &AtomicBool::new(true)
            )
            .unwrap_err()
            .is_cancelled()
    );
    assert_eq!(provider.calls, 0);
    provider.cancel_on_call = true;
    assert!(
        renderer
            .read_definition(
                &mut provider,
                &definition,
                SignalSample(0),
                1,
                TIMEOUT,
                &cancelled
            )
            .unwrap_err()
            .is_cancelled()
    );
    assert_eq!(renderer.cached_stage_count(), 0);
    provider.cancel_on_call = false;
    let cancelled = AtomicBool::new(false);
    renderer
        .read_definition(
            &mut provider,
            &definition,
            SignalSample(720),
            127,
            TIMEOUT,
            &cancelled,
        )
        .unwrap();
    assert_eq!(renderer.cached_stage_count(), 1);
    let calls = provider.calls;
    provider.unavailable = true;
    assert!(matches!(
        renderer.read_definition(
            &mut provider,
            &definition,
            SignalSample(720),
            1,
            TIMEOUT,
            &cancelled
        ),
        Err(StageAudioError::Preparation(
            PreparationError::SourceUnavailable(_)
        ))
    ));
    assert_eq!(provider.calls, calls + 1);
}

#[test]
fn missing_and_zero_gaps_are_rejected_and_tail_remains_explicitly_unsupported() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let plan = compile(&document(rate, None, "r0"), false);
    assert!(plan.audio_definition(selector()).is_err());
    let valid = document(rate, Some(gap(1, HoldAudio::Silence)), "r0");
    let mut invalid = serde_json::to_value(&valid).unwrap();
    invalid["nodes"]["repeat"]["kind"]["gap"]["duration"] = serde_json::json!(0);
    assert_eq!(
        ProjectDocument::from_json(&invalid.to_string())
            .unwrap_err()
            .code,
        deadpan_core::DocumentErrorCode::InvalidDuration
    );
    let plan = compile(
        &document(
            rate,
            Some(gap(
                3,
                HoldAudio::Tail {
                    source: audio(100..321),
                    maximum: FrameDuration::new(2).unwrap(),
                },
            )),
            "r0",
        ),
        false,
    );
    let definition = plan.audio_definition(selector()).unwrap();
    let mut provider = FixtureProvider::new("r0");
    let mut renderer = StageAudio::new(plan.clone());
    assert!(matches!(
        renderer.read_definition(
            &mut provider,
            &definition,
            SignalSample(0),
            128,
            TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::Unsupported("effect tails"))
    ));
    assert_eq!(provider.calls, 0);
}
