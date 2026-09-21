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
    StageAudio, StageAudioError, StageLimits, StereoMatrix, TimeMappedBlock,
};
use deadpan_core::*;
use deadpan_dsp::{CanonicalRecipe, CanonicalStretch, StereoPcm, StretchRate};
use deadpan_media::audio_index::AudioChannelLayout;
use deadpan_media::audio_session::{AudioSession, AudioSessionLimits};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_plan::RenderPlan;
use sha2::{Digest, Sha256};

const TIMEOUT: Duration = Duration::from_secs(10);

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn ratio(numerator: i128, denominator: i128) -> ExactRatio {
    ExactRatio::new(numerator, denominator).unwrap()
}

fn duration(frames: i64) -> FrameDuration {
    FrameDuration::new(frames).unwrap()
}

fn audio(start: i64, end: i64) -> SourceAudio {
    let clock = SourceTimeBase::new(1, 48_000).unwrap();
    SourceAudio {
        asset: AssetId::new("media").unwrap(),
        span: SourceSpan::new(
            SourceTimestamp {
                ticks: start,
                time_base: clock,
            },
            SourceTimestamp {
                ticks: end,
                time_base: clock,
            },
        )
        .unwrap(),
    }
}

fn source(rate: FrameRate, frames: i64, selected: Range<i64>) -> BeatNode {
    let audio = audio(selected.start, selected.end);
    BeatNode {
        label: "Original speech".into(),
        kind: NodeKind::Source {
            source: SourceNode {
                duration: duration(frames),
                video: SourceVideo::Blank,
                video_mapping: SourceVideoMapping::FitBeat,
                audio_mapping: SourceAudioMapping::natural_rate(audio.span, rate).unwrap(),
                audio: Some(audio),
                audio_offset: AudioSample(0),
                link: LinkRelation::Independent,
            },
        },
    }
}

fn hold(frames: i64) -> BeatNode {
    BeatNode::hold(
        "Inserted silence",
        HoldRecipe {
            duration: duration(frames),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        },
    )
}

fn retime(child: &str, frames: i64, selected: Range<i64>, pitch: PitchPolicy) -> BeatNode {
    BeatNode {
        label: "Explicit retime".into(),
        kind: NodeKind::Retime {
            child: id(child),
            duration: duration(frames),
            mapping: FrameRange::new(ProjectFrame(selected.start), ProjectFrame(selected.end))
                .unwrap(),
            pitch,
        },
    }
}

fn plan(
    rate: FrameRate,
    children: &[&str],
    nodes: impl IntoIterator<Item = (&'static str, BeatNode)>,
) -> Arc<RenderPlan> {
    plan_with_overrides(rate, children, nodes, BTreeMap::new())
}

fn plan_with_overrides(
    rate: FrameRate,
    children: &[&str],
    nodes: impl IntoIterator<Item = (&'static str, BeatNode)>,
    overrides: BTreeMap<NodeId, PlayOverrides>,
) -> Arc<RenderPlan> {
    let empty = ProjectDocument::new(
        ProjectId::new("stage-project").unwrap(),
        RevisionId::new("stage-revision").unwrap(),
        PresentationBasis {
            width: 1920,
            height: 1080,
            frame_rate: rate,
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut nodes: BTreeMap<_, _> = nodes
        .into_iter()
        .map(|(name, node)| (id(name), node))
        .collect();
    nodes.insert(
        id("root"),
        BeatNode::sequence("Sequence", children.iter().map(|name| id(name)).collect()),
    );
    let mut wire = serde_json::to_value(empty).unwrap();
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    wire["overrides"] = serde_json::to_value(overrides).unwrap();
    wire["assets"] = serde_json::to_value(BTreeMap::from([(
        AssetId::new("media").unwrap(),
        AssetRecord {
            label: "Known PCM fixture".into(),
            content_hash: "a".repeat(64),
            video: None,
            audio: Some(audio(0, 8197).span),
            still_image: true,
            frame_count: None,
            source_qualification: None,
        },
    )]))
    .unwrap();
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    Arc::new(RenderPlan::compile(&document).unwrap())
}

struct FixtureProvider {
    source: PreparedSource,
    calls: usize,
    cancel_on_call: bool,
}

impl FixtureProvider {
    fn new() -> Self {
        Self::with_layout(stereo_layout())
    }

    fn with_layout(layout: AudioChannelLayout) -> Self {
        let bytes = std::fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../native/deadpan-source/tests/audio-fixtures/pcm-stereo-48000.wav"),
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
        let expected = session.index().clone();
        // This known synthetic WAV has no speaker mask; declare its generated
        // channel order explicitly rather than infer it from the channel count.
        let source = PreparedSource::with_layout(session, &expected, layout, &cancelled).unwrap();
        Self {
            source,
            calls: 0,
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
        assert_eq!(*project, ProjectId::new("stage-project").unwrap());
        assert_eq!(*revision, RevisionId::new("stage-revision").unwrap());
        assert_eq!(*asset, AssetId::new("media").unwrap());
        assert!(!cancelled.load(Ordering::Relaxed));
        self.calls += 1;
        if self.cancel_on_call {
            cancelled.store(true, Ordering::Relaxed);
        }
        Ok(&self.source)
    }
}

fn stereo_layout() -> AudioChannelLayout {
    AudioChannelLayout::Native {
        channels: 2,
        mask: 3,
    }
}

fn fixture_sample(index: i64) -> [f32; 2] {
    let index = usize::try_from(index).unwrap();
    let left = match index % 2048 {
        0 => 24576,
        1 => -24576,
        512..=1023 => ((index * 97) % 16384) as i32 - 8192,
        _ => 0,
    };
    let right = match index % 257 {
        0 => -32768,
        1 => 32767,
        _ => (index % 97) as i32 * 3 - 48 * 3,
    };
    [left as f32 / 32768.0, right as f32 / 32768.0]
}

// These references share qualified DSP/filter primitives, never the plan's
// transforms or the stage renderer. Each test supplies its expected exact grid,
// stage order, retained selection and canonical input/output lengths itself.
fn sample_reference(
    selection: Range<i64>,
    origin: ExactRatio,
    step: ExactRatio,
    frames: u32,
    value: impl Fn(i64) -> [f32; 2],
) -> Vec<[f32; 2]> {
    let sampler = Resampler::new(
        ResampleRecipe::new(
            selection,
            origin,
            AudioSample(0),
            step,
            AudioSample(0)..AudioSample(i64::from(frames)),
        )
        .unwrap(),
        StereoMatrix::new(stereo_layout()).unwrap(),
    );
    let mut output = Vec::new();
    while output.len() < frames as usize {
        let start = AudioSample(output.len() as i64);
        let count = (frames - output.len() as u32).min(256);
        let window = sampler
            .required_source_range(start, count)
            .unwrap()
            .map(|range| PcmWindow {
                start: range.start,
                samples: range.flat_map(&value).collect(),
            });
        output.extend(
            sampler
                .render(start, count, window, &AtomicBool::new(false))
                .unwrap()
                .samples,
        );
    }
    output
}

fn stretch_reference(
    input: &[[f32; 2]],
    frames: u32,
    numerator: u64,
    denominator: u64,
) -> Vec<[f32; 2]> {
    let pcm = StereoPcm::new(
        input.iter().map(|frame| frame[0]).collect(),
        input.iter().map(|frame| frame[1]).collect(),
    )
    .unwrap();
    let recipe = CanonicalRecipe::with_rate(
        input.len() as u32,
        frames,
        StretchRate::new(numerator, denominator).unwrap(),
        0,
    )
    .unwrap();
    let mut renderer = CanonicalStretch::new(recipe, pcm).unwrap();
    let mut output = Vec::new();
    while output.len() < frames as usize {
        let count = (frames as usize - output.len()).min(256);
        let mut left = vec![0.0; count];
        let mut right = vec![0.0; count];
        assert_eq!(
            renderer
                .read(&mut left, &mut right, &AtomicBool::new(false))
                .unwrap(),
            count
        );
        output.extend(
            left.into_iter()
                .zip(right)
                .map(|(left, right)| [left, right]),
        );
    }
    output
}

fn read_block(
    renderer: &mut StageAudio,
    provider: &mut FixtureProvider,
    start: i64,
    frames: u32,
) -> TimeMappedBlock {
    let block = renderer
        .read(
            provider,
            AudioSample(start),
            frames,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(block.schema_version, 1);
    assert_eq!(block.stage, "time_mapped_pcm_before_effects");
    assert_eq!(block.project_id, ProjectId::new("stage-project").unwrap());
    assert_eq!(
        block.revision_id,
        RevisionId::new("stage-revision").unwrap()
    );
    assert_eq!(block.start, AudioSample(start));
    assert_eq!(block.samples.len(), frames as usize);
    block
}

fn read_all(
    renderer: &mut StageAudio,
    provider: &mut FixtureProvider,
    pieces: &[u32],
) -> Vec<[f32; 2]> {
    let frames = renderer.plan().audio_duration().unwrap().0 as usize;
    let mut output = Vec::new();
    for requested in pieces.iter().copied().cycle() {
        if output.len() == frames {
            return output;
        }
        let count = requested.min((frames - output.len()) as u32);
        output.extend(read_block(renderer, provider, output.len() as i64, count).samples);
    }
    unreachable!()
}

fn cut_stage_plan() -> Arc<RenderPlan> {
    let rate = FrameRate::new(48_000, 1).unwrap();
    plan(
        rate,
        &["stretch"],
        [
            ("first", source(rate, 2048, 512..2560)),
            ("second", source(rate, 2048, 4096..6144)),
            (
                "cuts",
                BeatNode::sequence("Edited speech", vec![id("first"), id("second")]),
            ),
            (
                "stretch",
                retime("cuts", 6144, 0..4096, PitchPolicy::Preserve),
            ),
        ],
    )
}

#[test]
fn preserve_processes_both_source_cuts_continuously_and_query_partitions_cannot_restart_it() {
    let input: Vec<_> = (512..2560).chain(4096..6144).map(fixture_sample).collect();
    let expected = stretch_reference(&input, 6144, 2, 3);
    let independently_reset: Vec<_> = stretch_reference(&input[..2048], 3072, 2, 3)
        .into_iter()
        .chain(stretch_reference(&input[2048..], 3072, 2, 3))
        .collect();
    assert_ne!(
        expected, independently_reset,
        "fixture must detect a reset at the source seam"
    );
    let mut provider = FixtureProvider::new();
    let mut renderer = StageAudio::new(cut_stage_plan());
    assert_eq!(
        read_all(&mut renderer, &mut provider, &[1, 73, 256, 3, 117]),
        expected
    );
    assert_eq!(renderer.cached_stage_count(), 1);
    for (start, count) in [(3011, 193), (6000, 144), (0, 31), (3072, 127)] {
        assert_eq!(
            read_block(&mut renderer, &mut provider, start, count).samples,
            expected[start as usize..start as usize + count as usize]
        );
    }
    assert_eq!(renderer.cached_stage_count(), 1);
    let mut fresh = StageAudio::new(cut_stage_plan());
    assert_eq!(
        read_block(&mut fresh, &mut provider, 3011, 193).samples,
        expected[3011..3204]
    );
}

#[test]
fn ntsc_fractional_input_and_output_origins_preserve_exact_three_over_two_speed() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let planned = plan(
        rate,
        &["prefix", "stretch"],
        [
            ("prefix", hold(1)),
            ("source", source(rate, 5, 0..8197)),
            ("stretch", retime("source", 2, 1..4, PitchPolicy::Preserve)),
        ],
    );
    // C=1601.6: selected input starts at 1601.6 and ends at 6406.4.
    // Its 4805 grid positions retain exact phase; output extent 3203.2
    // requires 3204 cached frames but does not change the authored 3/2 rate.
    let input = sample_reference(
        1602..6407,
        ratio(8008, 5),
        ExactRatio::ONE,
        4805,
        fixture_sample,
    );
    let prepared = stretch_reference(&input, 3204, 3, 2);
    assert_ne!(prepared, stretch_reference(&input, 3204, 4805, 3204));
    // The preceding frame places the stage at 1601.6. Absolute allocation
    // begins at 1602, so its first emitted coordinate is stage sample 0.4.
    let expected_stage = sample_reference(0..3204, ratio(2, 5), ExactRatio::ONE, 3203, |at| {
        prepared[at as usize]
    });
    let mut expected = vec![[0.0; 2]; 1602];
    expected.extend(expected_stage);
    let mut renderer = StageAudio::new(Arc::clone(&planned));
    let mut provider = FixtureProvider::new();
    assert_eq!(renderer.plan().audio_duration().unwrap(), AudioSample(4805));
    assert_eq!(
        read_all(&mut renderer, &mut provider, &[19, 256, 7, 91]),
        expected
    );
    assert_eq!(
        read_block(&mut renderer, &mut provider, 1597, 11).suppressed,
        vec![AudioSample(1597)..AudioSample(1602)]
    );
}

#[test]
fn mixed_pitch_policies_keep_their_order_even_when_overall_speed_is_unity() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let input: Vec<_> = (512..1536).map(fixture_sample).collect();
    let preserved = stretch_reference(&input, 1536, 2, 3);
    let preserve_then_follow =
        sample_reference(0..1536, ExactRatio::ZERO, ratio(3, 2), 1024, |at| {
            preserved[at as usize]
        });
    let followed = sample_reference(
        512..1536,
        ExactRatio::integer(512),
        ratio(2, 3),
        1536,
        fixture_sample,
    );
    let follow_then_preserve = stretch_reference(&followed, 1024, 3, 2);
    assert_ne!(preserve_then_follow, follow_then_preserve);
    assert_ne!(preserve_then_follow, input);
    assert_ne!(follow_then_preserve, input);
    let mut provider = FixtureProvider::new();
    for (inner, outer, expected) in [
        (
            PitchPolicy::Preserve,
            PitchPolicy::FollowSpeed,
            preserve_then_follow,
        ),
        (
            PitchPolicy::FollowSpeed,
            PitchPolicy::Preserve,
            follow_then_preserve,
        ),
    ] {
        let mut renderer = StageAudio::new(plan(
            rate,
            &["outer"],
            [
                ("source", source(rate, 1024, 512..1536)),
                ("inner", retime("source", 1536, 0..1024, inner)),
                ("outer", retime("inner", 1024, 0..1536, outer)),
            ],
        ));
        assert_eq!(read_all(&mut renderer, &mut provider, &[256]), expected);
    }
}

#[test]
fn nested_preserve_retains_inner_history_and_outer_crop_matches_full_render() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let input: Vec<_> = (0..4096).map(fixture_sample).collect();
    let inner = stretch_reference(&input, 6144, 2, 3);
    let expected = stretch_reference(&inner, 4096, 3, 2);
    let mut nested = StageAudio::new(plan(
        rate,
        &["outer"],
        [
            ("source", source(rate, 4096, 0..4096)),
            (
                "inner",
                retime("source", 6144, 0..4096, PitchPolicy::Preserve),
            ),
            (
                "outer",
                retime("inner", 4096, 0..6144, PitchPolicy::Preserve),
            ),
        ],
    ));
    let mut provider = FixtureProvider::new();
    assert_eq!(
        read_all(&mut nested, &mut provider, &[127, 256, 7]),
        expected
    );
    assert_eq!(nested.cached_stage_count(), 2);
    let mut cropped = StageAudio::new(plan(
        rate,
        &["crop"],
        [
            ("source", source(rate, 4096, 0..4096)),
            (
                "inner",
                retime("source", 6144, 0..4096, PitchPolicy::Preserve),
            ),
            (
                "crop",
                retime("inner", 512, 3000..3512, PitchPolicy::FollowSpeed),
            ),
        ],
    ));
    assert_eq!(
        read_all(&mut cropped, &mut provider, &[19, 256, 31]),
        inner[3000..3512]
    );
}

#[test]
fn silent_hold_is_suppressed_even_when_its_input_interval_owns_no_grid_sample() {
    let rate = FrameRate::new(192_000, 1).unwrap();
    let mut renderer = StageAudio::new(plan(
        rate,
        &["stretch"],
        [
            ("before", source(rate, 1, 0..1)),
            ("hold", hold(1)),
            ("after", source(rate, 1, 1..2)),
            (
                "cuts",
                BeatNode::sequence(
                    "Subsample silence",
                    vec![id("before"), id("hold"), id("after")],
                ),
            ),
            ("stretch", retime("cuts", 24, 0..3, PitchPolicy::Preserve)),
        ],
    ));
    // Child time spans only 0.75 samples. The Hold is [0.25,0.5), so a
    // one-sample prepared input cannot discover its policy by scanning PCM.
    let mut provider = FixtureProvider::new();
    let block = read_block(&mut renderer, &mut provider, 0, 6);
    assert_eq!(block.suppressed, vec![AudioSample(2)..AudioSample(4)]);
    assert_eq!(block.samples[2..4], [[0.0; 2]; 2]);
    assert!(
        block.samples[..2]
            .iter()
            .chain(&block.samples[4..])
            .flatten()
            .any(|sample| *sample != 0.0)
    );
    assert_eq!(
        read_block(&mut renderer, &mut provider, 3, 2).suppressed,
        vec![AudioSample(3)..AudioSample(4)]
    );
}

#[test]
fn repeat_and_override_occurrences_do_not_alias_prepared_history() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let repeated = BeatNode {
        label: "Three plays".into(),
        kind: NodeKind::Repeat {
            child: id("default-stage"),
            iterations: IterationOrder::new(RevisionId::new("plays").unwrap(), 3).unwrap(),
            gap: Some(HoldRecipe {
                duration: duration(16),
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
            }),
        },
    };
    let planned = plan_with_overrides(
        rate,
        &["repeat"],
        [
            ("default-source", source(rate, 512, 512..1024)),
            (
                "default-stage",
                retime("default-source", 768, 0..512, PitchPolicy::Preserve),
            ),
            ("alternate-source", source(rate, 256, 6144..6400)),
            (
                "alternate-stage",
                retime("alternate-source", 384, 0..256, PitchPolicy::Preserve),
            ),
            ("repeat", repeated),
        ],
        BTreeMap::from([(
            id("repeat"),
            PlayOverrides::try_from(vec![PlayOverride {
                iteration: IterationId {
                    allocation: RevisionId::new("plays").unwrap(),
                    ordinal: 1,
                },
                root: id("alternate-stage"),
            }])
            .unwrap(),
        )]),
    );
    let default = stretch_reference(
        &(512..1024).map(fixture_sample).collect::<Vec<_>>(),
        768,
        2,
        3,
    );
    let alternate = stretch_reference(
        &(6144..6400).map(fixture_sample).collect::<Vec<_>>(),
        384,
        2,
        3,
    );
    let expected: Vec<_> = default
        .iter()
        .copied()
        .chain([[0.0; 2]; 16])
        .chain(alternate)
        .chain([[0.0; 2]; 16])
        .chain(default.iter().copied())
        .collect();
    let mut renderer = StageAudio::new(Arc::clone(&planned));
    let mut provider = FixtureProvider::new();
    assert_eq!(
        read_all(&mut renderer, &mut provider, &[251, 17, 103]),
        expected
    );
    assert_eq!(renderer.cached_stage_count(), 3);
    for (start, count) in [(1184, 200), (784, 256), (0, 256), (760, 40)] {
        assert_eq!(
            read_block(&mut renderer, &mut provider, start, count).samples,
            expected[start as usize..start as usize + count as usize]
        );
    }
    let mut one_entry = StageAudio::with_limits(
        planned,
        StageLimits {
            maximum_cached_stages: 1,
            ..StageLimits::default()
        },
    )
    .unwrap();
    assert_eq!(
        read_all(&mut one_entry, &mut provider, &[251, 17, 103]),
        expected
    );
    assert_eq!(one_entry.cached_stage_count(), 1);
    assert_eq!(
        read_block(&mut one_entry, &mut provider, 0, 256).samples,
        expected[..256]
    );
}

#[test]
fn stage_budget_failures_and_initial_cancellation_do_not_decode_or_publish_cache_entries() {
    let planned = cut_stage_plan();
    let mut provider = FixtureProvider::new();
    let mut renderer = StageAudio::new(Arc::clone(&planned));
    assert!(matches!(
        renderer.read(
            &mut provider,
            AudioSample(0),
            1,
            TIMEOUT,
            &AtomicBool::new(true)
        ),
        Err(StageAudioError::Preparation(PreparationError::Cancelled))
    ));
    for limits in [
        StageLimits {
            maximum_input_frames: 4095,
            ..StageLimits::default()
        },
        StageLimits {
            maximum_output_frames: 6143,
            ..StageLimits::default()
        },
        StageLimits {
            maximum_resident_frames: 100,
            ..StageLimits::default()
        },
    ] {
        let mut renderer = StageAudio::with_limits(Arc::clone(&planned), limits).unwrap();
        assert!(matches!(
            renderer.read(
                &mut provider,
                AudioSample(0),
                1,
                TIMEOUT,
                &AtomicBool::new(false)
            ),
            Err(StageAudioError::Limit(_))
        ));
        assert_eq!(renderer.cached_stage_count(), 0);
        assert_eq!(provider.calls, 0);
    }
    for (start, frames, timeout) in [
        (0, 0, TIMEOUT),
        (0, 257, TIMEOUT),
        (-1, 1, TIMEOUT),
        (6144, 1, TIMEOUT),
        (i64::MAX, 1, TIMEOUT),
        (0, 1, Duration::ZERO),
        (0, 1, Duration::from_secs(61)),
    ] {
        assert!(
            renderer
                .read(
                    &mut provider,
                    AudioSample(start),
                    frames,
                    timeout,
                    &AtomicBool::new(false)
                )
                .is_err()
        );
    }
    assert_eq!(provider.calls, 0);
    assert_eq!(renderer.cached_stage_count(), 0);
    assert!(matches!(
        renderer.read(
            &mut provider,
            AudioSample(0),
            1,
            Duration::from_nanos(1),
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::Timeout)
    ));
    assert_eq!(provider.calls, 0);
    assert_eq!(renderer.cached_stage_count(), 0);
    let input: Vec<_> = (512..2560).chain(4096..6144).map(fixture_sample).collect();
    let expected = stretch_reference(&input, 6144, 2, 3);
    assert_eq!(
        read_block(&mut renderer, &mut provider, 0, 64).samples,
        expected[..64]
    );
    assert_eq!(renderer.cached_stage_count(), 1);
    assert!(matches!(
        StageAudio::with_limits(
            planned,
            StageLimits {
                maximum_depth: 0,
                ..StageLimits::default()
            }
        ),
        Err(StageAudioError::InvalidLimits)
    ));
}

#[test]
fn cancellation_during_source_access_leaves_no_partial_stage_and_retry_replays_exactly() {
    let mut provider = FixtureProvider::new();
    provider.cancel_on_call = true;
    let mut renderer = StageAudio::new(cut_stage_plan());
    let cancelled = AtomicBool::new(false);
    assert!(matches!(
        renderer.read(&mut provider, AudioSample(3000), 100, TIMEOUT, &cancelled),
        Err(StageAudioError::Preparation(PreparationError::Cancelled))
    ));
    assert_eq!(provider.calls, 1);
    assert_eq!(renderer.cached_stage_count(), 0);
    provider.cancel_on_call = false;
    cancelled.store(false, Ordering::Relaxed);
    let input: Vec<_> = (512..2560).chain(4096..6144).map(fixture_sample).collect();
    let expected = stretch_reference(&input, 6144, 2, 3);
    assert_eq!(
        renderer
            .read(&mut provider, AudioSample(3000), 100, TIMEOUT, &cancelled)
            .unwrap()
            .samples,
        expected[3000..3100]
    );
    assert_eq!(renderer.cached_stage_count(), 1);
}

#[test]
fn nested_depth_and_native_long_input_limits_fail_without_decoding_originals() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let nested = plan(
        rate,
        &["outer"],
        [
            ("source", source(rate, 1024, 512..1536)),
            (
                "inner",
                retime("source", 1536, 0..1024, PitchPolicy::Preserve),
            ),
            (
                "outer",
                retime("inner", 1024, 0..1536, PitchPolicy::Preserve),
            ),
        ],
    );
    let mut renderer = StageAudio::with_limits(
        nested,
        StageLimits {
            maximum_depth: 1,
            ..StageLimits::default()
        },
    )
    .unwrap();
    let mut provider = FixtureProvider::new();
    assert!(matches!(
        renderer.read(
            &mut provider,
            AudioSample(0),
            1,
            TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::Limit("nested stage depth"))
    ));
    assert_eq!(provider.calls, 0);
    assert_eq!(renderer.cached_stage_count(), 0);

    let repeated = BeatNode {
        label: "Long speech".into(),
        kind: NodeKind::Repeat {
            child: id("source"),
            iterations: IterationOrder::new(RevisionId::new("plays").unwrap(), 129).unwrap(),
            gap: None,
        },
    };
    let mut renderer = StageAudio::new(plan(
        rate,
        &["stretch"],
        [
            ("source", source(rate, 8192, 0..8192)),
            ("repeat", repeated),
            (
                "stretch",
                retime("repeat", 528_384, 0..1_056_768, PitchPolicy::Preserve),
            ),
        ],
    ));
    // A tiny late read still requires continuous full-stage history. It must
    // reject the native cap, never manufacture support by chunked resets.
    assert!(matches!(
        renderer.read(
            &mut provider,
            AudioSample(500_000),
            1,
            TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::Limit("input frames"))
    ));
    assert_eq!(provider.calls, 0);
    assert_eq!(renderer.cached_stage_count(), 0);
}

#[test]
fn unsupported_hold_policies_cannot_hide_between_input_grid_samples() {
    let rate = FrameRate::new(192_000, 1).unwrap();
    for policy in [
        HoldAudio::RoomTone {
            source: audio(0, 1),
        },
        HoldAudio::Tail {
            source: audio(0, 1),
            maximum: duration(1),
        },
    ] {
        for nested in [false, true] {
            let mut nodes = vec![
                ("before", source(rate, 1, 0..1)),
                (
                    "policy",
                    BeatNode::hold(
                        "Unsupported policy",
                        HoldRecipe {
                            duration: duration(1),
                            video: HoldVideo::Background,
                            audio: policy.clone(),
                        },
                    ),
                ),
                ("after", source(rate, 1, 1..2)),
                (
                    "cuts",
                    BeatNode::sequence(
                        "Subsample policy",
                        vec![id("before"), id("policy"), id("after")],
                    ),
                ),
                (
                    "inner",
                    retime(
                        "cuts",
                        if nested { 2 } else { 24 },
                        0..3,
                        PitchPolicy::Preserve,
                    ),
                ),
            ];
            let root = if nested {
                nodes.push(("outer", retime("inner", 16, 0..2, PitchPolicy::Preserve)));
                "outer"
            } else {
                "inner"
            };
            let planned = plan(rate, &[root], nodes);
            let mut provider = FixtureProvider::new();
            let mut renderer = StageAudio::new(planned);
            // The unsupported Hold is [0.25,0.5) on the input grid. Its
            // final output interval is [2,4), or [4/3,8/3) when nested.
            // In the nested case it owns no inner-output grid point either.
            // A first-sample read still requires admission before any PCM.
            assert!(matches!(
                renderer.read(
                    &mut provider,
                    AudioSample(0),
                    1,
                    TIMEOUT,
                    &AtomicBool::new(false)
                ),
                Err(StageAudioError::Unsupported(_))
            ));
            assert_eq!(provider.calls, 0);
            assert_eq!(renderer.cached_stage_count(), 0);
        }
    }
}

fn repeated_stage_plan() -> Arc<RenderPlan> {
    let rate = FrameRate::new(48_000, 1).unwrap();
    plan(
        rate,
        &["outer"],
        [
            ("source", source(rate, 4, 0..4)),
            ("inner", retime("source", 8, 0..4, PitchPolicy::Preserve)),
            (
                "repeat",
                BeatNode {
                    label: "Nine stage occurrences".into(),
                    kind: NodeKind::Repeat {
                        child: id("inner"),
                        iterations: IterationOrder::new(RevisionId::new("plays").unwrap(), 9)
                            .unwrap(),
                        gap: None,
                    },
                },
            ),
            ("outer", retime("repeat", 108, 0..72, PitchPolicy::Preserve)),
        ],
    )
}

#[test]
fn shared_per_read_work_limits_bound_repeated_stage_preparation_and_allow_warm_retry() {
    let planned = repeated_stage_plan();
    let inner = stretch_reference(&(0..4).map(fixture_sample).collect::<Vec<_>>(), 8, 1, 2);
    let repeated: Vec<_> = (0..9).flat_map(|_| inner.iter().copied()).collect();
    let expected = stretch_reference(&repeated, 108, 2, 3);
    for limits in [
        StageLimits {
            maximum_prepared_stages: 2,
            ..StageLimits::default()
        },
        // The outer miss costs 72+108 frames; one inner miss costs 4+8.
        StageLimits {
            maximum_prepared_frames: 192,
            ..StageLimits::default()
        },
    ] {
        let mut renderer = StageAudio::with_limits(Arc::clone(&planned), limits).unwrap();
        let mut provider = FixtureProvider::new();
        let mut failed_reads = 0;
        let mut completed = None;
        for _ in 0..10 {
            let before = renderer.cached_stage_count();
            match renderer.read(
                &mut provider,
                AudioSample(0),
                108,
                TIMEOUT,
                &AtomicBool::new(false),
            ) {
                Ok(block) => {
                    completed = Some(block.samples);
                    break;
                }
                Err(StageAudioError::Limit(_)) => {
                    failed_reads += 1;
                    assert_eq!(
                        renderer.cached_stage_count(),
                        before + 1,
                        "one read may prepare only one new inner occurrence after reserving the outer stage"
                    );
                }
                Err(error) => panic!("unexpected preparation failure: {error}"),
            }
        }
        assert_eq!(failed_reads, 8);
        assert_eq!(completed.unwrap(), expected);
        assert_eq!(renderer.cached_stage_count(), 10);
        // Cache hits need provenance validation but no new DSP preparation.
        assert_eq!(
            read_block(&mut renderer, &mut provider, 0, 108).samples,
            expected
        );
    }
}

#[test]
fn prepared_cache_rechecks_explicit_source_layout_including_nested_dependencies() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let changed_layout = AudioChannelLayout::Native {
        channels: 2,
        mask: 5,
    };
    for nested in [false, true] {
        let mut nodes = vec![
            ("source", source(rate, 1024, 512..1536)),
            (
                "inner",
                retime("source", 1536, 0..1024, PitchPolicy::Preserve),
            ),
        ];
        let root = if nested {
            nodes.push((
                "outer",
                retime("inner", 1024, 0..1536, PitchPolicy::Preserve),
            ));
            "outer"
        } else {
            "inner"
        };
        let planned = plan(rate, &[root], nodes);
        let mut renderer = StageAudio::new(Arc::clone(&planned));
        let mut provider = FixtureProvider::new();
        let original = read_block(&mut renderer, &mut provider, 101, 200).samples;
        let original_index = provider.source.index().clone();
        provider = FixtureProvider::with_layout(changed_layout);
        assert_eq!(
            provider.source.index(),
            &original_index,
            "only the explicit interpretation changes, not bytes, source clock or decoded index"
        );
        let changed = read_block(&mut renderer, &mut provider, 101, 200).samples;
        let mut fresh = StageAudio::new(planned);
        let fresh_changed = read_block(&mut fresh, &mut provider, 101, 200).samples;
        assert_eq!(changed, fresh_changed);
        assert_ne!(
            changed, original,
            "FL/FC interpretation must not reuse a cached FL/FR waveform"
        );
        provider = FixtureProvider::new();
        assert_eq!(
            read_block(&mut renderer, &mut provider, 101, 200).samples,
            original
        );
    }
}
