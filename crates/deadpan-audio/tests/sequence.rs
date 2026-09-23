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
    SequenceAudio, SequenceAudioError, StereoMatrix,
};
use deadpan_core::*;
use deadpan_media::audio_index::AudioChannelLayout;
use deadpan_media::audio_session::{AudioSession, AudioSessionLimits};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_plan::RenderPlan;
use sha2::{Digest, Sha256};

const TIMEOUT: Duration = Duration::from_secs(2);

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn ratio(numerator: i128, denominator: i128) -> ExactRatio {
    ExactRatio::new(numerator, denominator).unwrap()
}

fn duration(frames: i64) -> FrameDuration {
    FrameDuration::new(frames).unwrap()
}

fn samples(start: i64, end: i64) -> Range<AudioSample> {
    AudioSample(start)..AudioSample(end)
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

fn source(rate: FrameRate, frames: i64, selected: Range<i64>, offset: i64) -> BeatNode {
    let audio = audio(selected.start, selected.end);
    BeatNode {
        audio_edges: Default::default(),
        label: "Original speech".into(),
        kind: NodeKind::Source {
            source: SourceNode {
                duration: duration(frames),
                video: SourceVideo::Blank,
                video_mapping: SourceVideoMapping::FitBeat,
                audio_mapping: SourceAudioMapping::natural_rate(audio.span, rate).unwrap(),
                audio: Some(audio),
                audio_offset: AudioSample(offset),
                link: LinkRelation::Independent,
            },
        },
    }
}

fn hold_recipe(frames: i64, audio: HoldAudio) -> HoldRecipe {
    HoldRecipe {
        duration: duration(frames),
        video: HoldVideo::Background,
        audio,
    }
}

fn hold(frames: i64) -> BeatNode {
    BeatNode::hold("Inserted silence", hold_recipe(frames, HoldAudio::Silence))
}

fn retime(child: &str, frames: i64, selected: Range<i64>, pitch: PitchPolicy) -> BeatNode {
    BeatNode {
        audio_edges: Default::default(),
        label: "Explicit retime".into(),
        kind: NodeKind::Retime {
            purpose: deadpan_core::RetimePurpose::Edit,
            child: id(child),
            duration: duration(frames),
            mapping: FrameRange::new(ProjectFrame(selected.start), ProjectFrame(selected.end))
                .unwrap(),
            pitch,
        },
    }
}

fn partition(child: &str, selected: Range<i64>) -> BeatNode {
    let mut node = retime(
        child,
        selected.end - selected.start,
        selected,
        PitchPolicy::FollowSpeed,
    );
    if let NodeKind::Retime { purpose, .. } = &mut node.kind {
        *purpose = RetimePurpose::Partition;
    }
    node
}

fn repeat(child: &str, plays: u32, gap: i64) -> BeatNode {
    BeatNode {
        audio_edges: Default::default(),
        label: "Repeated speech".into(),
        kind: NodeKind::Repeat {
            child: id(child),
            iterations: IterationOrder::new(RevisionId::new("plays").unwrap(), plays).unwrap(),
            gap: (gap > 0).then(|| hold_recipe(gap, HoldAudio::Silence)),
        },
    }
}

fn renderer(
    rate: FrameRate,
    children: &[&str],
    nodes: impl IntoIterator<Item = (&'static str, BeatNode)>,
) -> SequenceAudio {
    renderer_with_asset(rate, children, nodes, audio(0, 8197).span)
}

fn renderer_with_asset(
    rate: FrameRate,
    children: &[&str],
    nodes: impl IntoIterator<Item = (&'static str, BeatNode)>,
    asset_span: SourceSpan,
) -> SequenceAudio {
    let empty = ProjectDocument::new(
        ProjectId::new("sequence-project").unwrap(),
        RevisionId::new("immutable-revision").unwrap(),
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
    wire["assets"] = serde_json::to_value(BTreeMap::from([(
        AssetId::new("media").unwrap(),
        AssetRecord {
            label: "Known PCM fixture".into(),
            content_hash: "a".repeat(64),
            video: None,
            audio: Some(asset_span),
            still_image: true,
            frame_count: None,
            source_qualification: None,
        },
    )]))
    .unwrap();
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    SequenceAudio::new(Arc::new(RenderPlan::compile(&document).unwrap()))
}

struct FixtureProvider {
    source: PreparedSource,
    calls: usize,
}

impl FixtureProvider {
    fn new(maximum_read_frames: u32) -> Self {
        Self::from_fixture(
            "pcm-stereo-48000.wav",
            AudioChannelLayout::Native {
                channels: 2,
                mask: 3,
            },
            maximum_read_frames,
        )
    }

    fn from_fixture(name: &str, layout: AudioChannelLayout, maximum_read_frames: u32) -> Self {
        let bytes = std::fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../native/deadpan-source/tests/audio-fixtures")
                .join(name),
        )
        .unwrap();
        let session = AudioSession::open_verified(
            &mut Cursor::new(&bytes),
            SourceContentIdentity::new(Sha256::digest(&bytes).into(), bytes.len() as u64).unwrap(),
            0,
            AudioSessionLimits {
                maximum_read_frames,
                ..AudioSessionLimits::default()
            },
            &AtomicBool::new(false),
        )
        .unwrap();
        let expected = session.index().clone();
        // This interpretation is part of the synthetic fixture recipe. Plain
        // PCM WAV carries no speaker mask, so channel count never decides it.
        let source =
            PreparedSource::with_layout(session, &expected, layout, &AtomicBool::new(false))
                .unwrap();
        Self { source, calls: 0 }
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
        assert_eq!(*project, ProjectId::new("sequence-project").unwrap());
        assert_eq!(*revision, RevisionId::new("immutable-revision").unwrap());
        assert_eq!(*asset, AssetId::new("media").unwrap());
        assert!(!cancelled.load(Ordering::Relaxed));
        self.calls += 1;
        Ok(&self.source)
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

fn read(
    renderer: &SequenceAudio,
    provider: &mut FixtureProvider,
    start: i64,
    count: u32,
) -> Vec<[f32; 2]> {
    let block = renderer
        .read_sources(
            provider,
            AudioSample(start),
            count,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(block.schema_version, 1);
    assert_eq!(block.stage, "source_pcm_before_effects");
    assert_eq!(
        block.project_id,
        ProjectId::new("sequence-project").unwrap()
    );
    assert_eq!(
        block.revision_id,
        RevisionId::new("immutable-revision").unwrap()
    );
    assert_eq!(block.start, AudioSample(start));
    assert_eq!(block.samples.len(), count as usize);
    block.samples
}

// The expected recipe is written explicitly by each test. The oracle shares
// the already-qualified sampling kernel, but never obtains timing, selection,
// phase, or rate from the structural plan or the sequence renderer.
fn reference(recipe: ResampleRecipe, start: i64, count: u32) -> Vec<[f32; 2]> {
    let sampler = Resampler::new(
        recipe,
        StereoMatrix::new(AudioChannelLayout::Native {
            channels: 2,
            mask: 3,
        })
        .unwrap(),
    );
    let window = sampler
        .required_source_range(AudioSample(start), count)
        .unwrap()
        .map(|range| PcmWindow {
            start: range.start,
            samples: range.flat_map(fixture_sample).collect(),
        });
    sampler
        .render(AudioSample(start), count, window, &AtomicBool::new(false))
        .unwrap()
        .samples
}

#[test]
fn source_placement_and_sequence_order_preserve_original_samples_and_revision_identity() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let renderer = renderer(
        rate,
        &["positive", "negative"],
        [
            ("positive", source(rate, 24, 512..528, 3)),
            ("negative", source(rate, 12, 1024..1040, -4)),
        ],
    );
    let mut provider = FixtureProvider::new(256);
    let expected: Vec<_> = (0..36)
        .map(|at| match at {
            3..19 => fixture_sample(512 + at - 3),
            24..36 => fixture_sample(1028 + at - 24),
            _ => [0.0; 2],
        })
        .collect();
    assert_eq!(read(&renderer, &mut provider, 0, 36), expected);
    assert_eq!(renderer.plan().audio_duration().unwrap(), AudioSample(36));
    assert_eq!(provider.calls, 2);
}

#[test]
fn inserted_silence_resumes_untouched_speech_and_repeat_has_three_total_plays() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let held = renderer(
        rate,
        &["before", "hold", "after"],
        [
            ("before", source(rate, 8, 0..8, 0)),
            ("hold", hold(5)),
            ("after", source(rate, 8, 8..16, 0)),
        ],
    );
    let mut provider = FixtureProvider::new(256);
    let expected: Vec<_> = (0..8)
        .map(fixture_sample)
        .chain([[0.0; 2]; 5])
        .chain((8..16).map(fixture_sample))
        .collect();
    assert_eq!(read(&held, &mut provider, 0, 21), expected);
    let repeated = renderer(
        rate,
        &["repeat"],
        [
            ("child", source(rate, 7, 512..519, 0)),
            ("repeat", repeat("child", 3, 3)),
        ],
    );
    let expected: Vec<_> = (0..27)
        .map(|at| {
            if at % 10 < 7 {
                fixture_sample(512 + at % 10)
            } else {
                [0.0; 2]
            }
        })
        .collect();
    assert_eq!(repeated.plan().audio_duration().unwrap(), AudioSample(27));
    assert_eq!(read(&repeated, &mut provider, 0, 27), expected);
}

#[test]
fn ntsc_boundaries_keep_exact_fractional_phase_across_partitions_and_seeks() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let renderer = renderer(
        rate,
        &["a", "b", "c"],
        [
            ("a", source(rate, 1, 0..2000, 0)),
            ("b", source(rate, 1, 0..2000, 0)),
            ("c", source(rate, 1, 0..2000, 0)),
        ],
    );
    // One frame is exactly 1601.6 samples. Absolute ties-even boundaries are
    // 0, 1602, 3203, 4805, while source phase remains 0, +0.4, -0.2.
    let mut expected = Vec::new();
    for (start, end, phase) in [
        (0, 1602, ratio(0, 1)),
        (1602, 3203, ratio(2, 5)),
        (3203, 4805, ratio(-1, 5)),
    ] {
        let recipe = ResampleRecipe::new(
            0..1602,
            phase,
            AudioSample(start),
            ExactRatio::ONE,
            samples(start, end),
        )
        .unwrap();
        let mut at = start;
        while at < end {
            let count = u32::try_from((end - at).min(256)).unwrap();
            expected.extend(reference(recipe.clone(), at, count));
            at += i64::from(count);
        }
    }
    assert_eq!(renderer.plan().audio_duration().unwrap(), AudioSample(4805));
    let mut provider = FixtureProvider::new(65_536);
    let mut actual = Vec::new();
    for count in [1, 31, 3, 97, 256, 2, 121].into_iter().cycle() {
        if actual.len() == expected.len() {
            break;
        }
        let count = count.min((expected.len() - actual.len()) as u32);
        actual.extend(read(&renderer, &mut provider, actual.len() as i64, count));
    }
    assert_eq!(actual, expected);
    for (start, count) in [
        (1590, 35),
        (3203, 31),
        (1602, 1),
        (0, 1),
        (4793, 12),
        (3199, 16),
    ] {
        assert_eq!(
            read(&renderer, &mut provider, start, count),
            expected[start as usize..start as usize + count as usize]
        );
    }
}

#[test]
fn nested_follow_speed_retimes_keep_the_exact_composed_mapping() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let renderer = renderer(
        rate,
        &["outer"],
        [
            ("source", source(rate, 200, 0..200, 0)),
            (
                "inner",
                retime("source", 100, 20..180, PitchPolicy::FollowSpeed),
            ),
            (
                "outer",
                retime("inner", 60, 10..90, PitchPolicy::FollowSpeed),
            ),
        ],
    );
    // source = 20 + (8/5)*(10 + (4/3)*output) = 36 + (32/15)*output.
    let recipe = ResampleRecipe::new(
        36..164,
        ExactRatio::integer(36),
        AudioSample(0),
        ratio(32, 15),
        samples(0, 60),
    )
    .unwrap();
    let expected = reference(recipe, 0, 60);
    let mut provider = FixtureProvider::new(128);
    assert_eq!(read(&renderer, &mut provider, 0, 60), expected);
    assert_eq!(read(&renderer, &mut provider, 29, 31), expected[29..]);
}

#[test]
fn original_44100_clock_is_resampled_to_the_mix_clock_and_mono_stays_centered() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let clock = SourceTimeBase::new(1, 44_100).unwrap();
    let span = |start, end| {
        SourceSpan::new(
            SourceTimestamp {
                ticks: start,
                time_base: clock,
            },
            SourceTimestamp {
                ticks: end,
                time_base: clock,
            },
        )
        .unwrap()
    };
    let mut node = source(rate, 483, 100..541, 3);
    let NodeKind::Source { source } = &mut node.kind else {
        unreachable!()
    };
    source.audio = Some(SourceAudio {
        asset: AssetId::new("media").unwrap(),
        span: span(100, 541),
    });
    source.audio_mapping = SourceAudioMapping::natural_rate(span(100, 541), rate).unwrap();
    let renderer = renderer_with_asset(rate, &["mono"], [("mono", node)], span(0, 44_117));
    let layout = AudioChannelLayout::Native {
        channels: 1,
        mask: 4,
    };
    let mut provider = FixtureProvider::from_fixture("pcm-mono-44100.wav", layout, 65_536);
    // Exactly 441 original samples occupy 480 mix samples, after a three-sample
    // placement delay. The oracle uses the fixture's integer PCM recipe.
    let sampler = Resampler::new(
        ResampleRecipe::new(
            100..541,
            ExactRatio::integer(100),
            AudioSample(3),
            ratio(147, 160),
            samples(3, 483),
        )
        .unwrap(),
        StereoMatrix::new(layout).unwrap(),
    );
    let expected = |start, count| {
        let range = sampler
            .required_source_range(AudioSample(start), count)
            .unwrap()
            .unwrap();
        let input = PcmWindow {
            start: range.start,
            samples: range
                .map(|at| (((at * 73) % 65_536 - 32_768) as f32) / 32_768.0)
                .collect(),
        };
        sampler
            .render(
                AudioSample(start),
                count,
                Some(input),
                &AtomicBool::new(false),
            )
            .unwrap()
            .samples
    };
    let mut first = vec![[0.0; 2]; 3];
    first.extend(expected(3, 253));
    assert_eq!(read(&renderer, &mut provider, 0, 256), first);
    assert_eq!(read(&renderer, &mut provider, 478, 5), expected(478, 5));
    assert!(first.iter().all(|frame| frame[0] == frame[1]));
    assert!(first.iter().any(|frame| frame[0].abs() > 0.5));
    assert_eq!(renderer.plan().audio_duration().unwrap(), AudioSample(483));
}

#[test]
fn structural_crops_exclude_filter_context_and_use_half_open_discrete_samples() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let cropped = renderer(
        rate,
        &["crop"],
        [
            ("source", source(rate, 8197, 0..8197, 0)),
            (
                "crop",
                retime("source", 8, 512..528, PitchPolicy::FollowSpeed),
            ),
        ],
    );
    let mut provider = FixtureProvider::new(16);
    let recipe = ResampleRecipe::new(
        512..528,
        ExactRatio::integer(512),
        AudioSample(0),
        ratio(2, 1),
        samples(0, 8),
    )
    .unwrap();
    assert_eq!(read(&cropped, &mut provider, 0, 8), reference(recipe, 0, 8));
    // The original source contains thousands of samples outside these nested
    // selections. A one-frame read ceiling proves they never enter the halo.
    let mut provider = FixtureProvider::new(1);
    let fractional = renderer(
        rate,
        &["crop"],
        [
            ("source", source(rate, 8197, 0..8197, 0)),
            (
                "slow",
                retime("source", 81_970, 0..8197, PitchPolicy::FollowSpeed),
            ),
            (
                "crop",
                retime("slow", 2, 5119..5121, PitchPolicy::FollowSpeed),
            ),
        ],
    );
    let recipe = ResampleRecipe::new(
        512..513,
        ratio(5119, 10),
        AudioSample(0),
        ratio(1, 10),
        samples(0, 2),
    )
    .unwrap();
    assert_eq!(
        read(&fractional, &mut provider, 0, 2),
        reference(recipe, 0, 2)
    );
    let empty = renderer(
        rate,
        &["crop"],
        [
            ("source", source(rate, 8197, 0..8197, 0)),
            (
                "slow",
                retime("source", 81_970, 0..8197, PitchPolicy::FollowSpeed),
            ),
            (
                "crop",
                retime("slow", 1, 5121..5122, PitchPolicy::FollowSpeed),
            ),
        ],
    );
    assert_eq!(read(&empty, &mut provider, 0, 1), [[0.0; 2]]);
}

#[test]
fn transparent_partition_retains_full_filter_support_while_an_authored_crop_excludes_it() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let mut provider = FixtureProvider::new(65_536);
    for selected in [5119..5121, 5121..5122] {
        let transparent = renderer(
            rate,
            &["partition"],
            [
                ("source", source(rate, 8197, 0..8197, 0)),
                (
                    "slow",
                    retime("source", 81_970, 0..8197, PitchPolicy::FollowSpeed),
                ),
                ("partition", partition("slow", selected.clone())),
            ],
        );
        let count = u32::try_from(selected.end - selected.start).unwrap();
        let expected = reference(
            ResampleRecipe::new(
                0..8197,
                ratio(i128::from(selected.start), 10),
                AudioSample(0),
                ratio(1, 10),
                samples(0, i64::from(count)),
            )
            .unwrap(),
            0,
            count,
        );
        assert_eq!(read(&transparent, &mut provider, 0, count), expected);
        assert!(expected.iter().any(|frame| *frame != [0.0; 2]));
    }
}

#[test]
fn pure_partition_keeps_ntsc_sampling_phase_and_signed_source_placement() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    for offset in [-3, 0, 3] {
        let original = source(rate, 4, 0..6406, offset);
        let whole = renderer(rate, &["source"], [("source", original.clone())]);
        let split = renderer(
            rate,
            &["left", "right"],
            [
                ("left", partition("source-left", 0..1)),
                ("right", partition("source-right", 1..4)),
                ("source-left", original.clone()),
                ("source-right", original),
            ],
        );
        let mut provider = FixtureProvider::new(65_536);
        for (start, count) in [
            (1601, 3),
            (0, 129),
            (6389, 17),
            (1594, 200),
            (1602, 1),
            (1601, 1),
        ] {
            assert_eq!(
                read(&split, &mut provider, start, count),
                read(&whole, &mut provider, start, count),
                "offset {offset}, query {start}+{count}"
            );
        }
    }
}

#[test]
fn unsupported_audio_policies_fail_before_any_source_is_requested() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let mut implicit = source(rate, 8, 100..116, 0);
    if let NodeKind::Source { source } = &mut implicit.kind {
        source.audio_mapping = SourceAudioMapping::FitBeat;
    }
    let bad_nodes = [
        (
            retime("retimed", 8, 0..16, PitchPolicy::Preserve),
            Some(source(rate, 16, 100..116, 0)),
        ),
        (
            BeatNode::hold(
                "Room tone",
                hold_recipe(
                    8,
                    HoldAudio::RoomTone {
                        source: audio(100, 108),
                    },
                ),
            ),
            None,
        ),
        (
            BeatNode::hold(
                "Tail",
                hold_recipe(
                    8,
                    HoldAudio::Tail {
                        source: audio(100, 108),
                        maximum: duration(4),
                    },
                ),
            ),
            None,
        ),
        (implicit, None),
    ];
    let mut provider = FixtureProvider::new(256);
    for (bad, child) in bad_nodes {
        let mut nodes = vec![("good", source(rate, 8, 0..8, 0)), ("unsupported", bad)];
        if let Some(child) = child {
            nodes.push(("retimed", child));
        }
        let renderer = renderer(rate, &["good", "unsupported"], nodes);
        assert!(matches!(
            renderer.read_sources(
                &mut provider,
                AudioSample(0),
                16,
                TIMEOUT,
                &AtomicBool::new(false)
            ),
            Err(SequenceAudioError::Unsupported { .. })
        ));
        assert_eq!(
            provider.calls, 0,
            "preflight must precede the supported prefix read"
        );
    }
}

#[test]
fn unity_preserve_and_retimed_silence_need_no_unimplemented_dsp() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let renderer = renderer(
        rate,
        &["preserve", "silence"],
        [
            ("source", source(rate, 16, 0..16, 0)),
            (
                "preserve",
                retime("source", 16, 0..16, PitchPolicy::Preserve),
            ),
            ("hold", hold(16)),
            ("silence", retime("hold", 8, 0..16, PitchPolicy::Preserve)),
        ],
    );
    let mut provider = FixtureProvider::new(256);
    let expected: Vec<_> = (0..16).map(fixture_sample).chain([[0.0; 2]; 8]).collect();
    assert_eq!(read(&renderer, &mut provider, 0, 24), expected);
    assert_eq!(provider.calls, 1);
}

#[test]
fn cancellation_and_invalid_block_budgets_do_not_touch_the_provider() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let renderer = renderer(rate, &["source"], [("source", source(rate, 64, 0..64, 0))]);
    let mut provider = FixtureProvider::new(256);
    assert!(
        renderer
            .read_sources(
                &mut provider,
                AudioSample(0),
                1,
                TIMEOUT,
                &AtomicBool::new(true)
            )
            .is_err()
    );
    for (start, count, timeout) in [
        (0, 0, TIMEOUT),
        (0, 257, TIMEOUT),
        (-1, 1, TIMEOUT),
        (63, 2, TIMEOUT),
        (i64::MAX, 1, TIMEOUT),
        (0, 1, Duration::ZERO),
        (0, 1, Duration::from_secs(61)),
    ] {
        assert!(
            renderer
                .read_sources(
                    &mut provider,
                    AudioSample(start),
                    count,
                    timeout,
                    &AtomicBool::new(false)
                )
                .is_err()
        );
    }
    assert_eq!(provider.calls, 0);
    assert_eq!(read(&renderer, &mut provider, 0, 1), [fixture_sample(0)]);
}

#[test]
fn billion_play_repeat_seeks_the_last_play_without_a_trailing_gap() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let renderer = renderer(
        rate,
        &["repeat"],
        [
            ("source", source(rate, 1, 511..512, 0)),
            ("repeat", repeat("source", 1_000_000_000, 1)),
        ],
    );
    assert_eq!(
        renderer.plan().audio_duration().unwrap(),
        AudioSample(1_999_999_999)
    );
    let mut provider = FixtureProvider::new(1);
    assert_eq!(
        read(&renderer, &mut provider, 1_999_999_997, 2),
        [[0.0; 2], fixture_sample(511)]
    );
    assert_eq!(provider.calls, 1);
    assert!(
        renderer
            .read_sources(
                &mut provider,
                AudioSample(1_999_999_999),
                1,
                TIMEOUT,
                &AtomicBool::new(false)
            )
            .is_err()
    );
}
