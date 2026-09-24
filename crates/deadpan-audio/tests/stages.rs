#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::collections::{BTreeMap, BTreeSet};
use std::io::Cursor;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use deadpan_audio::{
    AudioSourceProvider, PcmWindow, PreparationError, PreparedSource, ResampleRecipe, Resampler,
    RootSignalTransfer, SequenceAudio, SequenceAudioError, StageAudio, StageAudioError,
    StageLimits, StereoMatrix, TimeMappedBlock,
};
use deadpan_core::*;
use deadpan_dsp::{CanonicalRecipe, CanonicalStretch, StereoPcm, StretchRate};
use deadpan_media::audio_index::AudioChannelLayout;
use deadpan_media::audio_session::{AudioSession, AudioSessionLimits};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_plan::{RenderPlan, SignalSample};
use sha2::{Digest, Sha256};

const TIMEOUT: Duration = Duration::from_secs(10);

#[path = "stages/insert_time.rs"]
mod insert_time;

#[path = "stages/preparation.rs"]
mod preparation;

#[path = "stages/limited.rs"]
mod limited;

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
    audio_at_rate(start, end, 48_000)
}

fn audio_at_rate(start: i64, end: i64, sample_rate: u32) -> SourceAudio {
    let clock = SourceTimeBase::new(1, sample_rate).unwrap();
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
        framing: None,
        audio_edges: Default::default(),
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

fn room_tone(frames: i64, source: SourceAudio) -> BeatNode {
    BeatNode::hold(
        "Chosen room tone",
        HoldRecipe {
            duration: duration(frames),
            video: HoldVideo::Background,
            audio: HoldAudio::RoomTone { source },
        },
    )
}

fn retime(child: &str, frames: i64, selected: Range<i64>, pitch: PitchPolicy) -> BeatNode {
    BeatNode {
        framing: None,
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
    plan_with_asset(rate, children, nodes, overrides, audio(0, 8197).span)
}

fn plan_with_asset(
    rate: FrameRate,
    children: &[&str],
    nodes: impl IntoIterator<Item = (&'static str, BeatNode)>,
    overrides: BTreeMap<NodeId, PlayOverrides>,
    asset_span: SourceSpan,
) -> Arc<RenderPlan> {
    Arc::new(
        RenderPlan::compile(&document_with_asset(
            rate, children, nodes, overrides, asset_span,
        ))
        .unwrap(),
    )
}

fn document_with_asset(
    rate: FrameRate,
    children: &[&str],
    nodes: impl IntoIterator<Item = (&'static str, BeatNode)>,
    overrides: BTreeMap<NodeId, PlayOverrides>,
    asset_span: SourceSpan,
) -> ProjectDocument {
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
            audio: Some(asset_span),
            still_image: true,
            frame_count: None,
            source_qualification: None,
        },
    )]))
    .unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

struct FixtureProvider {
    source: PreparedSource,
    revisions: BTreeSet<RevisionId>,
    calls: usize,
    cancel_on_call: bool,
}

fn split_command(
    document: &ProjectDocument,
    target: &NodeId,
    at: i64,
    name: &str,
) -> ProjectDocument {
    let request = CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new(name).unwrap(),
        command: Command::Split {
            node: target.clone(),
            at: duration(at),
            identities: SplitIdentities {
                nodes: (0..document.nodes().len() + 4)
                    .map(|index| id(&format!("{name}-{index}")))
                    .collect(),
            },
        },
    };
    let transaction = apply(document, &request).unwrap();
    assert_eq!(transaction.duration_delta, 0);
    let divided = transaction.forward.apply(document).unwrap();
    assert_eq!(transaction.inverse.apply(&divided).unwrap(), *document);
    divided
}

#[test]
fn actual_split_and_refinement_preserve_ntsc_44100_pcm_and_two_sample_envelopes() {
    for tiny in [false, true] {
        let rate = if tiny {
            FrameRate::new(48_000, 1).unwrap()
        } else {
            FrameRate::new(30_000, 1001).unwrap()
        };
        let mut leaf = source(rate, if tiny { 2 } else { 4 }, 0..2);
        let span = if tiny {
            audio(0, 8197).span
        } else {
            let selected = audio_at_rate(100, 6100, 44_100);
            let NodeKind::Source { source } = &mut leaf.kind else {
                unreachable!()
            };
            source.audio = Some(selected.clone());
            source.audio_mapping = SourceAudioMapping::Placement {
                start: ratio(-1, 7),
                frames: SourceAudioMapping::natural_rate(selected.span, rate)
                    .unwrap()
                    .duration_frames(duration(4))
                    .unwrap(),
            };
            source.audio_offset = AudioSample(3);
            audio_at_rate(0, 44_117, 44_100).span
        };
        let original =
            document_with_asset(rate, &["source"], [("source", leaf)], BTreeMap::new(), span);
        let first = split_command(&original, &id("source"), 1, "cut-one");
        let divided = if tiny {
            first
        } else {
            let NodeKind::Sequence { children } = &first.nodes()[first.root()].kind else {
                unreachable!()
            };
            split_command(&first, &children[1], 1, "cut-two")
        };
        let whole_plan = Arc::new(RenderPlan::compile(&original).unwrap());
        let divided_plan = Arc::new(RenderPlan::compile(&divided).unwrap());
        let mut whole = StageAudio::new(Arc::clone(&whole_plan));
        let mut split = StageAudio::new(Arc::clone(&divided_plan));
        let mut provider = if tiny {
            FixtureProvider::new()
        } else {
            FixtureProvider::from_fixture(
                "pcm-mono-44100.wav",
                AudioChannelLayout::Native {
                    channels: 1,
                    mask: 4,
                },
            )
        };
        provider.revisions.insert(divided.revision_id().clone());
        let expected = faded_all(&mut whole, &mut provider, &[256, 3, 129]);
        if tiny {
            assert_eq!(
                expected,
                (0..2)
                    .map(|at| fixture_sample(at).map(|value| value * 0.5))
                    .collect::<Vec<_>>()
            );
        }
        let visits = if tiny {
            vec![(1, 1), (0, 1)]
        } else {
            vec![
                (3203, 11),
                (1601, 3),
                (0, 129),
                (6389, 17),
                (1594, 200),
                (3197, 17),
            ]
        };
        let whole_sources = SequenceAudio::new(whole_plan);
        let split_sources = SequenceAudio::new(divided_plan);
        for (start, count) in visits {
            let actual = split
                .read_edge_faded(
                    &mut provider,
                    AudioSample(start),
                    count,
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap();
            assert_eq!(
                actual.samples,
                expected[start as usize..start as usize + count as usize]
            );
            let before = whole_sources
                .read_sources(
                    &mut provider,
                    AudioSample(start),
                    count,
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap();
            let after = split_sources
                .read_sources(
                    &mut provider,
                    AudioSample(start),
                    count,
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap();
            assert_eq!(after.samples, before.samples);
        }
        assert_eq!(
            faded_all(&mut split, &mut provider, &[1, 199, 37]),
            expected
        );
    }
}

#[test]
fn actual_split_preserves_mixed_retime_and_room_tone_history_when_suffix_is_read_first() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    for room in [false, true] {
        for (inner, outer) in [
            (PitchPolicy::Preserve, PitchPolicy::FollowSpeed),
            (PitchPolicy::FollowSpeed, PitchPolicy::Preserve),
        ] {
            let original = document_with_asset(
                rate,
                &["outer"],
                [
                    (
                        "leaf",
                        if room {
                            room_tone(2048, audio(512, 833))
                        } else {
                            source(rate, 2048, 512..2560)
                        },
                    ),
                    ("inner", retime("leaf", 3072, 0..2048, inner)),
                    ("outer", retime("inner", 2048, 0..3072, outer)),
                ],
                BTreeMap::new(),
                audio(0, 8197).span,
            );
            let first = split_command(&original, &id("outer"), 1001, "mixed-one");
            let NodeKind::Sequence { children } = &first.nodes()[first.root()].kind else {
                unreachable!()
            };
            let divided = split_command(&first, &children[1], 333, "mixed-two");
            let mut whole = StageAudio::new(Arc::new(RenderPlan::compile(&original).unwrap()));
            let mut split = StageAudio::new(Arc::new(RenderPlan::compile(&divided).unwrap()));
            let mut provider = FixtureProvider::new();
            provider.revisions.insert(divided.revision_id().clone());
            let expected = faded_all(&mut whole, &mut provider, &[127, 256, 3]);
            for (start, count) in [(1334, 127), (1001, 127), (987, 41), (0, 193), (1987, 61)] {
                let actual = split
                    .read_edge_faded(
                        &mut provider,
                        AudioSample(start),
                        count,
                        TIMEOUT,
                        &AtomicBool::new(false),
                    )
                    .unwrap();
                assert_eq!(
                    actual.samples,
                    expected[start as usize..start as usize + count as usize],
                    "room {room}, inner {inner:?}, outer {outer:?}"
                );
            }
            assert_eq!(
                faded_all(&mut split, &mut provider, &[1, 53, 256]),
                expected
            );
        }
    }
}

impl FixtureProvider {
    fn new() -> Self {
        Self::with_layout(stereo_layout())
    }

    fn with_layout(layout: AudioChannelLayout) -> Self {
        Self::from_fixture("pcm-stereo-48000.wav", layout)
    }

    fn from_fixture(name: &str, layout: AudioChannelLayout) -> Self {
        let bytes = std::fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../native/deadpan-source/tests/audio-fixtures")
                .join(name),
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
            revisions: BTreeSet::from([RevisionId::new("stage-revision").unwrap()]),
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
        assert!(
            self.revisions.contains(revision),
            "unqualified fixture revision: {revision:?}"
        );
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
fn transferred_root_uses_qualified_pcm_without_baking_in_creative_fades() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let planned = plan(rate, &["source"], [("source", source(rate, 8197, 0..8197))]);
    let mut provider = FixtureProvider::new();
    let mut renderer = StageAudio::new(planned);
    let transfer = RootSignalTransfer::new(
        AudioSample(0)..AudioSample(8197),
        ratio(211, 3),
        SignalSample(400),
        ratio(3, 2),
        SignalSample(400)..SignalSample(656),
    )
    .unwrap();
    let expected = sample_reference(0..8197, ratio(211, 3), ratio(3, 2), 256, fixture_sample);
    for (offset, count) in [(173, 83), (0, 1), (1, 199), (200, 56)] {
        let actual = renderer
            .read_transferred(
                &mut provider,
                &transfer,
                SignalSample(400 + offset),
                count,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(actual.schema_version, 1);
        assert_eq!(actual.stage, "root_signal_on_point_grid_before_effects");
        assert_eq!(actual.transfer, transfer);
        assert_eq!(actual.project_id, ProjectId::new("stage-project").unwrap());
        assert_eq!(
            actual.revision_id,
            RevisionId::new("stage-revision").unwrap()
        );
        assert_eq!(actual.start, SignalSample(400 + offset));
        assert_eq!(
            actual.samples,
            expected[offset as usize..offset as usize + count as usize]
        );
        assert!(actual.suppressed.is_empty());
    }
    // A new stage consumes raw endpoint samples, without the old 2ms fade.
    let identity = RootSignalTransfer::new(
        AudioSample(0)..AudioSample(8197),
        ExactRatio::ZERO,
        SignalSample(0),
        ExactRatio::ONE,
        SignalSample(0)..SignalSample(2),
    )
    .unwrap();
    let raw = renderer
        .read_transferred(
            &mut provider,
            &identity,
            SignalSample(0),
            2,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(raw.samples, vec![fixture_sample(0), fixture_sample(1)]);
    let faded = renderer
        .read_edge_faded(
            &mut provider,
            AudioSample(0),
            2,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_ne!(raw.samples, faded.samples);
}

#[test]
fn transferred_preserve_keeps_root_silence_and_intrinsic_preparation_on_fractional_grid() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let planned = plan(
        rate,
        &["preserve"],
        [
            ("left", source(rate, 4, 0..6406)),
            ("quiet", hold(2)),
            ("right", room_tone(4, audio(512, 833))),
            (
                "input",
                BeatNode::sequence(
                    "Speech, pause and room",
                    vec![id("left"), id("quiet"), id("right")],
                ),
            ),
            ("preserve", retime("input", 5, 0..10, PitchPolicy::Preserve)),
        ],
    );
    let mut provider = FixtureProvider::new();
    let mut original = StageAudio::new(Arc::clone(&planned));
    let pcm = read_all(&mut original, &mut provider, &[256]);
    assert_eq!(pcm.len(), 8008);
    assert!(pcm[..3203].iter().flatten().any(|v| *v != 0.0));
    assert!(pcm[3203..4805].iter().all(|v| *v == [0.0; 2]));
    assert!(pcm[4805..].iter().flatten().any(|v| *v != 0.0));
    let mut renderer = StageAudio::new(planned);
    for origin in [ratio(16011, 5), ratio(24021, 5)] {
        let transfer = RootSignalTransfer::new(
            AudioSample(0)..AudioSample(8008),
            origin,
            SignalSample(17),
            ratio(3, 4),
            SignalSample(17)..SignalSample(273),
        )
        .unwrap();
        let mut expected = sample_reference(0..8008, origin, ratio(3, 4), 256, |n| pcm[n as usize]);
        for (n, sample) in expected.iter_mut().enumerate() {
            let old = origin.checked_add(ratio(n as i128 * 3, 4)).unwrap();
            if old.compare_integer(3203).is_ge() && old.compare_integer(4805).is_lt() {
                *sample = [0.0; 2];
            }
        }
        for (offset, count) in [(173, 83), (0, 1), (1, 199), (200, 56)] {
            let actual = renderer
                .read_transferred(
                    &mut provider,
                    &transfer,
                    SignalSample(17 + offset),
                    count,
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap();
            assert_eq!(
                actual.samples,
                expected[offset as usize..offset as usize + count as usize]
            );
            for (n, sample) in actual.samples.iter().enumerate() {
                let old = origin
                    .checked_add(ratio((offset as i128 + n as i128) * 3, 4))
                    .unwrap();
                let silent = old.compare_integer(3203).is_ge() && old.compare_integer(4805).is_lt();
                assert_eq!(
                    actual
                        .suppressed
                        .iter()
                        .any(|range| range.contains(&SignalSample(17 + offset + n as i64))),
                    silent
                );
                if silent {
                    assert_eq!(*sample, [0.0; 2]);
                }
            }
        }
        assert_eq!(
            renderer.cached_stage_count(),
            2,
            "whole Preserve and room-tone contexts are reused"
        );
    }
}

#[test]
fn transferred_halo_shares_preparation_work_and_rejects_invalid_context_before_io() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let planned = plan(
        rate,
        &["one", "two", "three"],
        [
            ("one", room_tone(512, audio(0, 223))),
            ("two", room_tone(512, audio(512, 833))),
            ("three", room_tone(512, audio(4096, 4353))),
        ],
    );
    let mut renderer = StageAudio::with_limits(
        planned,
        StageLimits {
            maximum_prepared_stages: 2,
            ..Default::default()
        },
    )
    .unwrap();
    let mut provider = FixtureProvider::new();
    let recipe = |end| {
        RootSignalTransfer::new(
            AudioSample(0)..AudioSample(end),
            ExactRatio::ZERO,
            SignalSample(0),
            ExactRatio::integer(4),
            SignalSample(0)..SignalSample(256),
        )
        .unwrap()
    };
    let invalid = recipe(1535);
    assert!(matches!(
        renderer.read_transferred(
            &mut provider,
            &invalid,
            SignalSample(0),
            256,
            TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::Range)
    ));
    let transfer = recipe(1536);
    for (timeout, cancelled) in [
        (Duration::ZERO, false),
        (Duration::from_secs(61), false),
        (TIMEOUT, true),
    ] {
        assert!(
            renderer
                .read_transferred(
                    &mut provider,
                    &transfer,
                    SignalSample(0),
                    256,
                    timeout,
                    &AtomicBool::new(cancelled)
                )
                .is_err()
        );
    }
    assert_eq!(provider.calls, 0);
    assert!(matches!(
        renderer.read_transferred(
            &mut provider,
            &transfer,
            SignalSample(0),
            256,
            Duration::from_nanos(1),
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::Timeout)
    ));
    assert_eq!(provider.calls, 0);
    assert!(matches!(
        renderer.read_transferred(
            &mut provider,
            &transfer,
            SignalSample(0),
            256,
            TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::Limit("prepared stages per read"))
    ));
    assert_eq!(
        renderer.cached_stage_count(),
        2,
        "the third preparation cannot reset the halo's work allowance"
    );
    assert!(
        renderer
            .read_transferred(
                &mut provider,
                &transfer,
                SignalSample(0),
                256,
                TIMEOUT,
                &AtomicBool::new(false)
            )
            .is_ok()
    );
    assert_eq!(renderer.cached_stage_count(), 3);
}

#[test]
fn transferred_halo_rejects_changed_source_provenance_between_input_blocks() {
    struct ChangingProvider {
        first: FixtureProvider,
        later: FixtureProvider,
        calls: usize,
    }
    impl AudioSourceProvider for ChangingProvider {
        fn source(
            &mut self,
            project: &ProjectId,
            revision: &RevisionId,
            asset: &AssetId,
            cancelled: &AtomicBool,
        ) -> Result<&PreparedSource, PreparationError> {
            self.calls += 1;
            if self.calls == 1 {
                self.first.source(project, revision, asset, cancelled)
            } else {
                self.later.source(project, revision, asset, cancelled)
            }
        }
    }
    let rate = FrameRate::new(48_000, 1).unwrap();
    let mut renderer = StageAudio::new(plan(
        rate,
        &["source"],
        [("source", source(rate, 8197, 0..8197))],
    ));
    let mut provider = ChangingProvider {
        first: FixtureProvider::new(),
        later: FixtureProvider::with_layout(AudioChannelLayout::Native {
            channels: 2,
            mask: 5,
        }),
        calls: 0,
    };
    let transfer = RootSignalTransfer::new(
        AudioSample(0)..AudioSample(8197),
        ratio(1001, 3),
        SignalSample(0),
        ratio(3, 2),
        SignalSample(0)..SignalSample(256),
    )
    .unwrap();
    assert!(matches!(
        renderer.read_transferred(
            &mut provider,
            &transfer,
            SignalSample(0),
            256,
            TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::Preparation(
            PreparationError::IndexMismatch
        ))
    ));
    assert_eq!(provider.calls, 2);
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
        framing: None,
        audio_edges: Default::default(),
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
        framing: None,
        audio_edges: Default::default(),
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
fn unsupported_effect_tails_cannot_hide_between_input_grid_samples() {
    let rate = FrameRate::new(192_000, 1).unwrap();
    let policy = HoldAudio::Tail {
        source: audio(0, 1),
        maximum: duration(1),
    };
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
                    framing: None,
                    audio_edges: Default::default(),
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

fn room_reference(input: &[[f32; 2]], extent: ExactRatio, frames: u32) -> Vec<[f32; 2]> {
    let half = extent.checked_div(ExactRatio::integer(2)).unwrap();
    let fade = if half.compare_integer(96).is_gt() {
        ExactRatio::integer(96)
    } else {
        half
    };
    let period = extent.checked_sub(fade).unwrap();
    let sample = |position| {
        sample_reference(0..input.len() as i64, position, ExactRatio::ONE, 1, |at| {
            input[at as usize]
        })[0]
    };
    (0..frames)
        .map(|frame| {
            let position = ExactRatio::integer(i64::from(frame));
            let cycle = position.checked_div(period).unwrap().floor();
            let local = position
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
            assert!(
                (actual[channel] - expected[channel]).abs() <= 2e-6,
                "frame {frame} channel {channel}: {} != {}",
                actual[channel],
                expected[channel]
            );
        }
    }
}

#[test]
fn room_tone_keeps_fractional_44100_source_extent_and_long_hold_duration_across_loops() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let planned = plan_with_asset(
        rate,
        &["room"],
        [("room", room_tone(1000, audio_at_rate(100, 321, 44_100)))],
        BTreeMap::new(),
        audio_at_rate(0, 44_117, 44_100).span,
    );
    let mut provider = FixtureProvider::from_fixture(
        "pcm-mono-44100.wav",
        AudioChannelLayout::Native {
            channels: 1,
            mask: 4,
        },
    );
    // 221 original samples span exactly 35360/147 mix samples. Rounding the
    // loop to 241 frames would drift at every seam. Its period is 21248/147.
    let original = |at: i64| [(((at * 73) % 65_536 - 32_768) as f32) / 32_768.0; 2];
    let input = sample_reference(
        100..321,
        ExactRatio::integer(100),
        ratio(147, 160),
        241,
        original,
    );
    let expected = room_reference(&input, ratio(35360, 147), 1000);
    let rounded = room_reference(&input, ExactRatio::integer(241), 1000);
    assert_ne!(
        expected, rounded,
        "fixture must expose rounded loop-period drift"
    );
    let mut renderer = StageAudio::new(Arc::clone(&planned));
    assert_eq!(renderer.plan().audio_duration().unwrap(), AudioSample(1000));
    let actual = read_all(&mut renderer, &mut provider, &[1, 73, 256, 17, 91]);
    assert_pcm_close(&actual, &expected);
    assert!(actual.iter().all(|frame| frame[0] == frame[1]));
    assert_eq!(renderer.cached_stage_count(), 1);
    for (start, count) in [(143, 21), (280, 33), (720, 127), (981, 19)] {
        let block = read_block(&mut renderer, &mut provider, start, count);
        assert_eq!(
            block.samples,
            actual[start as usize..start as usize + count as usize]
        );
        assert!(block.suppressed.is_empty());
    }
    let mut fresh = StageAudio::new(planned);
    assert_eq!(
        read_block(&mut fresh, &mut provider, 720, 127).samples,
        actual[720..847]
    );
}

#[test]
fn repeated_room_tone_and_override_restart_locally_with_two_distinct_gap_caches() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let repeated = BeatNode {
        framing: None,
        audio_edges: Default::default(),
        label: "Three room-tone plays".into(),
        kind: NodeKind::Repeat {
            child: id("default"),
            iterations: IterationOrder::new(RevisionId::new("plays").unwrap(), 3).unwrap(),
            gap: Some(HoldRecipe {
                duration: duration(181),
                video: HoldVideo::Background,
                audio: HoldAudio::RoomTone {
                    source: audio(512, 611),
                },
            }),
        },
    };
    let planned = plan_with_overrides(
        rate,
        &["repeat"],
        [
            ("default", room_tone(289, audio(0, 223))),
            ("alternate", room_tone(317, audio(4096, 4353))),
            ("repeat", repeated),
        ],
        BTreeMap::from([(
            id("repeat"),
            PlayOverrides::try_from(vec![PlayOverride {
                iteration: IterationId {
                    allocation: RevisionId::new("plays").unwrap(),
                    ordinal: 1,
                },
                root: id("alternate"),
            }])
            .unwrap(),
        )]),
    );
    let default = room_reference(
        &(0..223).map(fixture_sample).collect::<Vec<_>>(),
        ExactRatio::integer(223),
        289,
    );
    let alternate = room_reference(
        &(4096..4353).map(fixture_sample).collect::<Vec<_>>(),
        ExactRatio::integer(257),
        317,
    );
    let gap = room_reference(
        &(512..611).map(fixture_sample).collect::<Vec<_>>(),
        ExactRatio::integer(99),
        181,
    );
    let expected: Vec<_> = default
        .iter()
        .chain(&gap)
        .chain(&alternate)
        .chain(&gap)
        .chain(&default)
        .copied()
        .collect();
    let mut renderer = StageAudio::new(planned);
    let mut provider = FixtureProvider::new();
    assert_eq!(renderer.plan().audio_duration().unwrap(), AudioSample(1257));
    let actual = read_all(&mut renderer, &mut provider, &[251, 17, 103]);
    assert_pcm_close(&actual, &expected);
    assert_eq!(
        renderer.cached_stage_count(),
        5,
        "three occurrence identities and two gap_after identities must remain distinct"
    );
    assert_eq!(actual[289..470], actual[787..968]);
    assert_eq!(actual[..289], actual[968..1257]);
    for (start, count) in [(280, 37), (787, 181), (980, 200)] {
        let block = read_block(&mut renderer, &mut provider, start, count);
        assert_eq!(
            block.samples,
            actual[start as usize..start as usize + count as usize]
        );
        assert!(block.suppressed.is_empty());
    }
}

#[test]
fn room_tone_retimes_keep_pitch_order_and_outer_crop_keeps_the_intrinsic_loop_history() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let input: Vec<_> = (512..833).map(fixture_sample).collect();
    let room = room_reference(&input, ExactRatio::integer(321), 2048);
    let preserved = stretch_reference(&room, 3072, 2, 3);
    let preserve_then_follow =
        sample_reference(0..3072, ExactRatio::ZERO, ratio(3, 2), 2048, |at| {
            preserved[at as usize]
        });
    let followed = sample_reference(0..2048, ExactRatio::ZERO, ratio(2, 3), 3072, |at| {
        room[at as usize]
    });
    let follow_then_preserve = stretch_reference(&followed, 2048, 3, 2);
    assert_ne!(preserve_then_follow, follow_then_preserve);
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
                ("room", room_tone(2048, audio(512, 833))),
                ("inner", retime("room", 3072, 0..2048, inner)),
                ("outer", retime("inner", 2048, 0..3072, outer)),
            ],
        ));
        assert_pcm_close(
            &read_all(&mut renderer, &mut provider, &[117, 256, 3]),
            &expected,
        );
    }
    let mut cropped = StageAudio::new(plan(
        rate,
        &["crop"],
        [
            ("room", room_tone(2048, audio(512, 833))),
            (
                "inner",
                retime("room", 3072, 0..2048, PitchPolicy::Preserve),
            ),
            (
                "crop",
                retime("inner", 512, 1001..1513, PitchPolicy::FollowSpeed),
            ),
        ],
    ));
    assert_pcm_close(
        &read_block(&mut cropped, &mut provider, 181, 127).samples,
        &preserved[1182..1309],
    );
    assert_pcm_close(
        &read_all(&mut cropped, &mut provider, &[31, 256, 79]),
        &preserved[1001..1513],
    );
}

#[test]
fn room_tone_cache_layout_changes_invalidate_both_holds_and_dependent_preserve_stages() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    for nested in [false, true] {
        let mut nodes = vec![("room", room_tone(1024, audio(512, 769)))];
        let root = if nested {
            nodes.push((
                "preserve",
                retime("room", 1536, 0..1024, PitchPolicy::Preserve),
            ));
            "preserve"
        } else {
            "room"
        };
        let planned = plan(rate, &[root], nodes);
        let mut renderer = StageAudio::new(Arc::clone(&planned));
        let mut provider = FixtureProvider::new();
        let before = read_block(&mut renderer, &mut provider, 151, 200).samples;
        let before_index = provider.source.index().clone();
        provider = FixtureProvider::with_layout(AudioChannelLayout::Native {
            channels: 2,
            mask: 5,
        });
        assert_eq!(provider.source.index(), &before_index);
        let changed = read_block(&mut renderer, &mut provider, 151, 200).samples;
        let mut fresh = StageAudio::new(planned);
        assert_eq!(
            changed,
            read_block(&mut fresh, &mut provider, 151, 200).samples
        );
        assert_ne!(before, changed);
        provider = FixtureProvider::new();
        assert_eq!(
            before,
            read_block(&mut renderer, &mut provider, 151, 200).samples
        );
    }
}

#[test]
fn a_subsample_room_tone_hold_renders_through_preserve_without_becoming_silence() {
    let rate = FrameRate::new(192_000, 1).unwrap();
    let planned = plan(
        rate,
        &["preserve"],
        [
            ("room", room_tone(1, audio(0, 1))),
            ("preserve", retime("room", 8, 0..1, PitchPolicy::Preserve)),
        ],
    );
    let mut provider = FixtureProvider::new();
    let source_only = SequenceAudio::new(Arc::clone(&planned));
    assert!(matches!(
        source_only.read_sources(
            &mut provider,
            AudioSample(0),
            2,
            TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(SequenceAudioError::Unsupported { .. })
    ));
    assert_eq!(provider.calls, 0);
    let mut renderer = StageAudio::new(planned);
    let expected = stretch_reference(&[fixture_sample(0)], 2, 1, 8);
    let block = read_block(&mut renderer, &mut provider, 0, 2);
    assert_pcm_close(&block.samples, &expected);
    assert!(block.samples.iter().flatten().any(|sample| *sample != 0.0));
    assert!(block.suppressed.is_empty());
    assert_eq!(renderer.cached_stage_count(), 2);
    let faded = renderer
        .read_edge_faded(
            &mut provider,
            AudioSample(0),
            2,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(
        faded.samples,
        block
            .samples
            .iter()
            .map(|sample| sample.map(|value| value * 0.5))
            .collect::<Vec<_>>()
    );
}

fn faded_all(
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
        let block = renderer
            .read_edge_faded(
                provider,
                AudioSample(output.len() as i64),
                count,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(block.stage, "edge_faded_pcm_before_voice_effects");
        assert_eq!(block.engine, deadpan_audio::EDGE_FADE_ID);
        assert_eq!(block.processing_order, ["time_pitch_mapping", "edge_fades"]);
        output.extend(block.samples);
    }
    unreachable!()
}

#[test]
fn pure_partition_preserves_a_two_sample_envelope_and_standalone_fragment_offset() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let original = source(rate, 2, 0..2);
    let mut whole = StageAudio::new(plan(rate, &["source"], [("source", original.clone())]));
    let mut split = StageAudio::new(plan(
        rate,
        &["left", "right"],
        [
            ("left", partition("source-left", 0..1)),
            ("right", partition("source-right", 1..2)),
            ("source-left", original.clone()),
            ("source-right", original.clone()),
        ],
    ));
    let mut provider = FixtureProvider::new();
    let expected: Vec<_> = (0..2)
        .map(|at| fixture_sample(at).map(|value| value * 0.5))
        .collect();
    assert_eq!(faded_all(&mut whole, &mut provider, &[2]), expected);
    assert_eq!(faded_all(&mut split, &mut provider, &[1]), expected);
    let mut suffix = StageAudio::new(plan(
        rate,
        &["right"],
        [("right", partition("source", 1..2)), ("source", original)],
    ));
    assert_eq!(faded_all(&mut suffix, &mut provider, &[1]), expected[1..]);
}

#[test]
fn ntsc_partition_keeps_44100_filter_phase_placement_and_original_fades_in_both_readers() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let selected = audio_at_rate(100, 6100, 44_100);
    let mut original = source(rate, 4, 100..6100);
    let NodeKind::Source { source } = &mut original.kind else {
        unreachable!()
    };
    source.audio = Some(selected.clone());
    source.audio_mapping = SourceAudioMapping::Placement {
        start: ratio(-1, 7),
        frames: SourceAudioMapping::natural_rate(selected.span, rate)
            .unwrap()
            .duration_frames(duration(4))
            .unwrap(),
    };
    source.audio_offset = AudioSample(3);
    let asset_span = audio_at_rate(0, 44_117, 44_100).span;
    let whole = plan_with_asset(
        rate,
        &["source"],
        [("source", original.clone())],
        BTreeMap::new(),
        asset_span,
    );
    let split = plan_with_asset(
        rate,
        &["left", "right"],
        [
            ("left", partition("source-left", 0..1)),
            ("right", partition("source-right", 1..4)),
            ("source-left", original.clone()),
            ("source-right", original),
        ],
        BTreeMap::new(),
        asset_span,
    );
    let mut provider = FixtureProvider::from_fixture(
        "pcm-mono-44100.wav",
        AudioChannelLayout::Native {
            channels: 1,
            mask: 4,
        },
    );
    let whole_sources = SequenceAudio::new(Arc::clone(&whole));
    let split_sources = SequenceAudio::new(Arc::clone(&split));
    let mut whole_stage = StageAudio::new(whole);
    let mut split_stage = StageAudio::new(split);
    let expected = faded_all(&mut whole_stage, &mut provider, &[256, 3, 129]);
    assert_eq!(
        faded_all(&mut split_stage, &mut provider, &[1, 199, 37]),
        expected
    );
    for (start, count) in [
        (1601, 3),
        (0, 129),
        (6389, 17),
        (1594, 200),
        (1602, 1),
        (1601, 1),
    ] {
        let before = whole_sources
            .read_sources(
                &mut provider,
                AudioSample(start),
                count,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        let after = split_sources
            .read_sources(
                &mut provider,
                AudioSample(start),
                count,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(before.samples, after.samples);
        assert_eq!(
            read_block(&mut split_stage, &mut provider, start, count).samples,
            before.samples
        );
        let faded = split_stage
            .read_edge_faded(
                &mut provider,
                AudioSample(start),
                count,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(
            faded.samples,
            expected[start as usize..start as usize + count as usize]
        );
    }
}

#[test]
fn partitions_of_mixed_retimes_retain_full_preserve_and_room_tone_histories() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    for room in [false, true] {
        for (inner, outer) in [
            (PitchPolicy::Preserve, PitchPolicy::FollowSpeed),
            (PitchPolicy::FollowSpeed, PitchPolicy::Preserve),
        ] {
            let leaf = if room {
                room_tone(2048, audio(512, 833))
            } else {
                source(rate, 2048, 512..2560)
            };
            let whole = plan(
                rate,
                &["outer"],
                [
                    ("leaf", leaf.clone()),
                    ("inner", retime("leaf", 3072, 0..2048, inner)),
                    ("outer", retime("inner", 2048, 0..3072, outer)),
                ],
            );
            let split = plan(
                rate,
                &["left", "right"],
                [
                    ("left", partition("outer-left", 0..1001)),
                    ("right", partition("outer-right", 1001..2048)),
                    ("leaf-left", leaf.clone()),
                    ("inner-left", retime("leaf-left", 3072, 0..2048, inner)),
                    ("outer-left", retime("inner-left", 2048, 0..3072, outer)),
                    ("leaf-right", leaf),
                    ("inner-right", retime("leaf-right", 3072, 0..2048, inner)),
                    ("outer-right", retime("inner-right", 2048, 0..3072, outer)),
                ],
            );
            let mut provider = FixtureProvider::new();
            let mut whole = StageAudio::new(whole);
            let mut split = StageAudio::new(split);
            let expected = faded_all(&mut whole, &mut provider, &[127, 256, 3]);
            // Visit the suffix first, forcing its preparation from retained
            // history rather than the preceding fragment's live decoder state.
            for (start, count) in [(1001, 127), (987, 41), (0, 193), (1987, 61)] {
                let actual = split
                    .read_edge_faded(
                        &mut provider,
                        AudioSample(start),
                        count,
                        TIMEOUT,
                        &AtomicBool::new(false),
                    )
                    .unwrap();
                assert_eq!(
                    actual.samples,
                    expected[start as usize..start as usize + count as usize],
                    "room {room}, inner {inner:?}, outer {outer:?}"
                );
            }
            assert_eq!(
                faded_all(&mut split, &mut provider, &[1, 53, 256]),
                expected
            );
        }
    }
}

#[test]
fn partition_inside_preserve_input_retains_full_source_filter_context() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let whole = plan(
        rate,
        &["preserve"],
        [
            ("source", source(rate, 1024, 512..1536)),
            (
                "slow",
                retime("source", 2048, 0..1024, PitchPolicy::FollowSpeed),
            ),
            (
                "preserve",
                retime("slow", 3072, 0..2048, PitchPolicy::Preserve),
            ),
        ],
    );
    let split = plan(
        rate,
        &["preserve"],
        [
            ("source-left", source(rate, 1024, 512..1536)),
            (
                "slow-left",
                retime("source-left", 2048, 0..1024, PitchPolicy::FollowSpeed),
            ),
            ("source-right", source(rate, 1024, 512..1536)),
            (
                "slow-right",
                retime("source-right", 2048, 0..1024, PitchPolicy::FollowSpeed),
            ),
            ("left", partition("slow-left", 0..1001)),
            ("right", partition("slow-right", 1001..2048)),
            (
                "joined",
                BeatNode::sequence("Partitioned signal", vec![id("left"), id("right")]),
            ),
            (
                "preserve",
                retime("joined", 3072, 0..2048, PitchPolicy::Preserve),
            ),
        ],
    );
    let mut provider = FixtureProvider::new();
    let mut whole = StageAudio::new(whole);
    let mut split = StageAudio::new(split);
    let expected = faded_all(&mut whole, &mut provider, &[256]);
    assert_eq!(
        faded_all(&mut split, &mut provider, &[3, 211, 71]),
        expected
    );
}

#[test]
fn partition_inside_a_repeat_gap_keeps_its_full_room_tone_origin() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let repeated = |child: &str| BeatNode {
        framing: None,
        label: "Repeated room tone".into(),
        audio_edges: Default::default(),
        kind: NodeKind::Repeat {
            child: id(child),
            iterations: IterationOrder::new(RevisionId::new("plays").unwrap(), 3).unwrap(),
            gap: Some(HoldRecipe {
                duration: duration(181),
                video: HoldVideo::Background,
                audio: HoldAudio::RoomTone {
                    source: audio(512, 611),
                },
            }),
        },
    };
    let whole = plan(
        rate,
        &["repeat"],
        [
            ("source", source(rate, 128, 0..128)),
            ("repeat", repeated("source")),
        ],
    );
    let split = plan(
        rate,
        &["left", "right"],
        [
            ("source-left", source(rate, 128, 0..128)),
            ("source-right", source(rate, 128, 0..128)),
            ("repeat-left", repeated("source-left")),
            ("repeat-right", repeated("source-right")),
            ("left", partition("repeat-left", 0..199)),
            ("right", partition("repeat-right", 199..746)),
        ],
    );
    let mut provider = FixtureProvider::new();
    let mut whole = StageAudio::new(whole);
    let mut split = StageAudio::new(split);
    let expected = faded_all(&mut whole, &mut provider, &[211, 67]);
    let suffix = split
        .read_edge_faded(
            &mut provider,
            AudioSample(199),
            110,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(suffix.samples, expected[199..309]);
    assert_eq!(
        faded_all(&mut split, &mut provider, &[1, 199, 3, 256]),
        expected
    );
}

fn reference_edges(samples: &mut [[f32; 2]], start: bool, end: bool) {
    let length = samples.len() as f32;
    let width = 96.0_f32.min(length / 2.0);
    for (at, sample) in samples.iter_mut().enumerate() {
        let left = if start {
            (at as f32 + 0.5) / width
        } else {
            1.0
        };
        let right = if end {
            (length - at as f32 - 0.5) / width
        } else {
            1.0
        };
        let gain = 1.0_f32.min(left).min(right);
        *sample = sample.map(|value| value * gain);
    }
}

#[test]
fn edge_fades_follow_continuous_preserve_and_inner_cuts_without_query_edges() {
    let planned = cut_stage_plan();
    let input: Vec<_> = (512..2560).chain(4096..6144).map(fixture_sample).collect();
    let raw = stretch_reference(&input, 6144, 2, 3);
    let mut expected = raw.clone();
    reference_edges(&mut expected[..3072], true, true);
    reference_edges(&mut expected[3072..], true, true);
    let mut provider = FixtureProvider::new();
    let mut renderer = StageAudio::new(Arc::clone(&planned));
    assert_eq!(faded_all(&mut renderer, &mut provider, &[256]), expected);
    assert_eq!(
        faded_all(&mut renderer, &mut provider, &[1, 73, 17, 251]),
        expected
    );
    assert_eq!(read_all(&mut renderer, &mut provider, &[137, 256]), raw);
    for (start, count) in [(0, 97), (2971, 203), (3072, 127), (6000, 144)] {
        let mut fresh = StageAudio::new(Arc::clone(&planned));
        let block = fresh
            .read_edge_faded(
                &mut provider,
                AudioSample(start),
                count,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(
            block.samples,
            expected[start as usize..start as usize + count as usize]
        );
    }
    assert_eq!(renderer.cached_stage_count(), 1);
}

#[test]
fn edge_fades_follow_nested_mixed_room_tone_retimes_and_authored_crops() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let mut provider = FixtureProvider::new();
    for (inner, outer) in [
        (PitchPolicy::Preserve, PitchPolicy::FollowSpeed),
        (PitchPolicy::FollowSpeed, PitchPolicy::Preserve),
    ] {
        let planned = plan(
            rate,
            &["outer"],
            [
                ("room", room_tone(2048, audio(512, 833))),
                ("inner", retime("room", 3072, 0..2048, inner)),
                ("outer", retime("inner", 2048, 0..3072, outer)),
            ],
        );
        let mut renderer = StageAudio::new(planned);
        let raw = read_all(&mut renderer, &mut provider, &[256]);
        let mut expected = raw.clone();
        reference_edges(&mut expected, true, true);
        assert_eq!(
            faded_all(&mut renderer, &mut provider, &[19, 256, 7]),
            expected
        );
        assert_eq!(
            expected[96..1952],
            raw[96..1952],
            "loop seams gain no extra edge fade"
        );
    }
    let planned = plan(
        rate,
        &["crop"],
        [
            ("room", room_tone(2048, audio(512, 833))),
            (
                "inner",
                retime("room", 3072, 0..2048, PitchPolicy::Preserve),
            ),
            (
                "crop",
                retime("inner", 512, 1001..1513, PitchPolicy::FollowSpeed),
            ),
        ],
    );
    let mut renderer = StageAudio::new(planned);
    let mut expected = read_all(&mut renderer, &mut provider, &[256]);
    reference_edges(&mut expected, true, true);
    assert_eq!(
        faded_all(&mut renderer, &mut provider, &[31, 256, 79]),
        expected
    );
    let cropped = renderer
        .read_edge_faded(
            &mut provider,
            AudioSample(181),
            127,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(cropped.samples, expected[181..308]);
}

#[test]
fn placement_hard_end_does_not_mistake_the_host_start_for_its_later_onset() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    for offset in [0_usize, 64] {
        let mut placed = source(rate, 512, 512..640);
        placed.audio_edges.node_start = AudioEdgePolicy::Hard;
        placed.audio_edges.source_placement_end = AudioEdgePolicy::Hard;
        let NodeKind::Source { source } = &mut placed.kind else {
            unreachable!()
        };
        source.audio_offset = AudioSample(offset as i64);
        let planned = plan(rate, &["source"], [("source", placed)]);
        let mut provider = FixtureProvider::new();
        let mut renderer = StageAudio::new(planned);
        let raw = read_all(&mut renderer, &mut provider, &[256]);
        let mut expected = raw.clone();
        reference_edges(&mut expected[offset..offset + 128], offset != 0, false);
        let actual = faded_all(&mut renderer, &mut provider, &[73, 17, 256]);
        assert_eq!(actual, expected);
        assert_eq!(actual[offset + 127], raw[offset + 127]);
        if offset == 0 {
            assert_eq!(
                actual[0], raw[0],
                "coincident placement Automatic does not cancel node Hard"
            );
        } else {
            assert_ne!(actual[offset], raw[offset]);
        }
        assert!(
            actual[..offset]
                .iter()
                .chain(&actual[offset + 128..])
                .all(|sample| *sample == [0.0; 2])
        );
    }
}

#[test]
fn one_hard_repeat_override_and_room_tone_gap_edges_preserve_silence_masks() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let mut alternate = room_tone(128, audio(512, 611));
    alternate.audio_edges.node_start = AudioEdgePolicy::Hard;
    alternate.audio_edges.node_end = AudioEdgePolicy::Hard;
    let repeated = BeatNode {
        framing: None,
        audio_edges: AudioEdgePolicies {
            repeat_gap_start: AudioEdgePolicy::Hard,
            ..Default::default()
        },
        label: "Three room tone plays".into(),
        kind: NodeKind::Repeat {
            child: id("default"),
            iterations: IterationOrder::new(RevisionId::new("plays").unwrap(), 3).unwrap(),
            gap: Some(HoldRecipe {
                duration: duration(24),
                video: HoldVideo::Background,
                audio: HoldAudio::RoomTone {
                    source: audio(512, 611),
                },
            }),
        },
    };
    let planned = plan_with_overrides(
        rate,
        &["repeat", "silence"],
        [
            ("default", room_tone(128, audio(512, 611))),
            ("alternate", alternate),
            ("repeat", repeated),
            ("silence", hold(16)),
        ],
        BTreeMap::from([(
            id("repeat"),
            PlayOverrides::try_from(vec![PlayOverride {
                iteration: IterationId {
                    allocation: RevisionId::new("plays").unwrap(),
                    ordinal: 1,
                },
                root: id("alternate"),
            }])
            .unwrap(),
        )]),
    );
    let mut provider = FixtureProvider::new();
    let mut renderer = StageAudio::new(planned);
    let raw = read_all(&mut renderer, &mut provider, &[256]);
    let mut expected = raw.clone();
    for (range, start, end) in [
        (0..128, true, true),
        (128..152, false, true),
        (152..280, false, false),
        (280..304, false, true),
        (304..432, true, true),
    ] {
        reference_edges(&mut expected[range], start, end);
    }
    assert_eq!(
        faded_all(&mut renderer, &mut provider, &[13, 117, 256]),
        expected
    );
    assert_eq!(expected[152..280], raw[152..280]);
    assert_eq!(expected[0..128], expected[304..432]);
    let block = renderer
        .read_edge_faded(
            &mut provider,
            AudioSample(425),
            23,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(block.suppressed, vec![AudioSample(432)..AudioSample(448)]);
    assert_eq!(block.samples[7..], [[0.0; 2]; 16]);
}
