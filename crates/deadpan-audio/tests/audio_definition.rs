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
    RoomTone, RoomToneRecipe, StageAudio, StageAudioError, StageLimits, StereoMatrix,
};
use deadpan_core::*;
use deadpan_dsp::{CanonicalRecipe, CanonicalStretch, StereoPcm, StretchRate};
use deadpan_media::audio_index::AudioChannelLayout;
use deadpan_media::audio_session::{AudioSession, AudioSessionLimits};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_plan::{AudioDefinitionSelector, RenderPlan, SignalSample};
use sha2::{Digest, Sha256};

const TIMEOUT: Duration = Duration::from_secs(10);

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn ratio(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}
fn node(value: &str) -> AudioDefinitionSelector {
    AudioDefinitionSelector::Node { node: id(value) }
}

fn audio(selected: Range<i64>) -> SourceAudio {
    let time_base = SourceTimeBase::new(1, 48_000).unwrap();
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

fn source(rate: FrameRate, duration: i64, selected: Range<i64>) -> BeatNode {
    let audio = audio(selected);
    BeatNode {
        framing: None,
        label: "Measured source".into(),
        audio_edges: Default::default(),
        kind: NodeKind::Source {
            source: SourceNode {
                duration: frames(duration),
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

fn hold(duration: i64, audio: HoldAudio) -> BeatNode {
    BeatNode::hold(
        "Hold",
        HoldRecipe {
            duration: frames(duration),
            video: HoldVideo::Background,
            audio,
        },
    )
}

fn retime(child: &str, duration: i64, selection: Range<i64>, pitch: PitchPolicy) -> BeatNode {
    BeatNode {
        framing: None,
        label: "Retime".into(),
        audio_edges: Default::default(),
        kind: NodeKind::Retime {
            purpose: RetimePurpose::Edit,
            child: id(child),
            duration: frames(duration),
            mapping: FrameRange::new(ProjectFrame(selection.start), ProjectFrame(selection.end))
                .unwrap(),
            pitch,
        },
    }
}

fn document(
    rate: FrameRate,
    children: &[&str],
    nodes: impl IntoIterator<Item = (&'static str, BeatNode)>,
    overrides: BTreeMap<NodeId, PlayOverrides>,
) -> ProjectDocument {
    let empty = ProjectDocument::new(
        ProjectId::new("definition-project").unwrap(),
        RevisionId::new("definition-revision").unwrap(),
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
        .map(|(name, value)| (id(name), value))
        .collect();
    nodes.insert(
        id("root"),
        BeatNode::sequence("Root", children.iter().map(|name| id(name)).collect()),
    );
    let asset = AssetRecord {
        label: "Known WAV".into(),
        content_hash: "a".repeat(64),
        video: None,
        audio: Some(audio(0..8197).span),
        still_image: true,
        frame_count: None,
        source_qualification: None,
    };
    let mut picture = asset.clone();
    picture.video = picture.audio.take();
    picture.still_image = false;
    let mut wire = serde_json::to_value(empty).unwrap();
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    wire["overrides"] = serde_json::to_value(overrides).unwrap();
    wire["assets"] = serde_json::to_value(BTreeMap::from([
        (AssetId::new("media").unwrap(), asset),
        (AssetId::new("picture").unwrap(), picture),
    ]))
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
    calls: usize,
    context_calls: usize,
    unavailable: bool,
    cancel_on_call: bool,
}

impl FixtureProvider {
    fn new() -> Self {
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
        let index = session.index().clone();
        let prepared = PreparedSource::with_layout(session, &index, layout(), &cancelled).unwrap();
        Self {
            prepared,
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
        assert_eq!(project, &ProjectId::new("definition-project").unwrap());
        assert_eq!(revision, &RevisionId::new("definition-revision").unwrap());
        assert_eq!(asset, &AssetId::new("media").unwrap());
        assert!(!cancelled.load(Ordering::Relaxed));
        self.calls += 1;
        if self.unavailable {
            return Err(PreparationError::SourceUnavailable(
                "source admission revoked".into(),
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
        self.source(project, revision, asset, cancelled)
    }
}

fn layout() -> AudioChannelLayout {
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

// The oracles select their own exact phase, trim and canonical stage lengths;
// none obtains transform or recipe values from the definition under test.
fn sample_reference(
    selection: Range<i64>,
    origin: ExactRatio,
    step: ExactRatio,
    length: u32,
    value: impl Fn(i64) -> [f32; 2],
) -> Vec<[f32; 2]> {
    let sampler = Resampler::new(
        ResampleRecipe::new(
            selection,
            origin,
            AudioSample(0),
            step,
            AudioSample(0)..AudioSample(i64::from(length)),
        )
        .unwrap(),
        StereoMatrix::new(layout()).unwrap(),
    );
    let mut output = Vec::new();
    while output.len() < length as usize {
        let at = AudioSample(output.len() as i64);
        let count = (length - output.len() as u32).min(256);
        let window = sampler
            .required_source_range(at, count)
            .unwrap()
            .map(|range| PcmWindow {
                start: range.start,
                samples: range.flat_map(&value).collect(),
            });
        output.extend(
            sampler
                .render(at, count, window, &AtomicBool::new(false))
                .unwrap()
                .samples,
        );
    }
    output
}

fn stretch(input: &[[f32; 2]], length: u32, numerator: u64, denominator: u64) -> Vec<[f32; 2]> {
    let input = StereoPcm::new(
        input.iter().map(|value| value[0]).collect(),
        input.iter().map(|value| value[1]).collect(),
    )
    .unwrap();
    let recipe = CanonicalRecipe::with_rate(
        input.frames(),
        length,
        StretchRate::new(numerator, denominator).unwrap(),
        0,
    )
    .unwrap();
    let mut renderer = CanonicalStretch::new(recipe, input).unwrap();
    let mut result = Vec::new();
    while result.len() < length as usize {
        let count = (length as usize - result.len()).min(256);
        let mut left = vec![0.0; count];
        let mut right = vec![0.0; count];
        assert_eq!(
            renderer
                .read(&mut left, &mut right, &AtomicBool::new(false))
                .unwrap(),
            count
        );
        result.extend(
            left.into_iter()
                .zip(right)
                .map(|(left, right)| [left, right]),
        );
    }
    result
}

#[test]
fn all_overridden_repeat_still_renders_its_unheard_default_definition() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let doc = document(
        rate,
        &["sibling", "repeat"],
        [
            ("sibling", source(rate, 64, 0..64)),
            ("default-source", source(rate, 512, 512..1024)),
            (
                "default-stage",
                retime("default-source", 768, 0..512, PitchPolicy::Preserve),
            ),
            ("first-override", source(rate, 768, 6144..6912)),
            ("second-override", source(rate, 768, 6912..7680)),
            (
                "repeat",
                BeatNode {
                    framing: None,
                    label: "All plays overridden".into(),
                    audio_edges: Default::default(),
                    kind: NodeKind::Repeat {
                        child: id("default-stage"),
                        iterations: IterationOrder::new(RevisionId::new("plays").unwrap(), 2)
                            .unwrap(),
                        gap: None,
                    },
                },
            ),
        ],
        BTreeMap::from([(
            id("repeat"),
            PlayOverrides::try_from(vec![
                PlayOverride {
                    iteration: IterationId {
                        allocation: RevisionId::new("plays").unwrap(),
                        ordinal: 0,
                    },
                    root: id("first-override"),
                },
                PlayOverride {
                    iteration: IterationId {
                        allocation: RevisionId::new("plays").unwrap(),
                        ordinal: 1,
                    },
                    root: id("second-override"),
                },
            ])
            .unwrap(),
        )]),
    );
    let selector = AudioDefinitionSelector::RepeatDefault {
        repeat: id("repeat"),
    };
    let expected = stretch(
        &(512..1024).map(fixture_sample).collect::<Vec<_>>(),
        768,
        2,
        3,
    );
    for frozen in [false, true] {
        let planned = compile(&doc, frozen);
        let definition = planned.audio_definition(selector.clone()).unwrap();
        assert_eq!(definition.root(), &id("default-stage"));
        assert_eq!(definition.selector(), &selector);
        assert_eq!(
            definition.signal().sample_count().unwrap(),
            SignalSample(768)
        );
        let mut renderer = StageAudio::new(Arc::clone(&planned));
        let mut provider = FixtureProvider::new();
        for (start, count) in [(600, 168), (0, 193), (193, 211), (404, 196)] {
            let block = renderer
                .read_definition(
                    &mut provider,
                    &definition,
                    SignalSample(start),
                    count,
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap();
            assert_eq!(
                block.samples,
                expected[start as usize..start as usize + count as usize]
            );
            assert_eq!(block.start, SignalSample(start));
            assert_eq!(block.definition, selector);
            assert_eq!(block.root, id("default-stage"));
            assert_eq!(block.revision_id, *doc.revision_id());
            assert!(block.suppressed.is_empty());
        }
        for (start, first_source) in [(0, 0), (64, 6144), (832, 6912)] {
            let actual = renderer
                .read(
                    &mut provider,
                    AudioSample(start),
                    64,
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap();
            assert_eq!(
                actual.samples,
                (first_source..first_source + 64)
                    .map(fixture_sample)
                    .collect::<Vec<_>>()
            );
            assert_ne!(actual.samples, expected[..64]);
        }
        assert_eq!(provider.context_calls > 0, frozen);
        assert_eq!(renderer.cached_stage_count(), 1);
        provider.unavailable = true;
        assert!(matches!(
            renderer.read_definition(
                &mut provider,
                &definition,
                SignalSample(0),
                1,
                TIMEOUT,
                &AtomicBool::new(false)
            ),
            Err(StageAudioError::Preparation(
                PreparationError::SourceUnavailable(_)
            ))
        ));
    }
}

#[test]
fn ntsc_definition_uses_local_zero_and_point_ceil_instead_of_root_allocation() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let doc = document(
        rate,
        &["prefix", "stage"],
        [
            ("prefix", hold(1, HoldAudio::Silence)),
            ("a", source(rate, 5, 0..8197)),
            ("stage", retime("a", 2, 1..4, PitchPolicy::Preserve)),
        ],
        BTreeMap::new(),
    );
    let planned = compile(&doc, false);
    let definition = planned.audio_definition(node("stage")).unwrap();
    assert_eq!(
        definition.signal().sample_count().unwrap(),
        SignalSample(3204)
    );
    // The selected source begins at 1601.6; 4804.8 mix samples require 4805
    // canonical input points and 3203.2 output samples require 3204 points.
    let input = sample_reference(
        1602..6407,
        ratio(8008, 5),
        ExactRatio::ONE,
        4805,
        fixture_sample,
    );
    let expected = stretch(&input, 3204, 3, 2);
    let root_reference = sample_reference(0..3204, ratio(2, 5), ExactRatio::ONE, 3203, |at| {
        expected[at as usize]
    });
    assert_ne!(expected[..256], root_reference[..256]);
    let mut renderer = StageAudio::new(Arc::clone(&planned));
    let mut provider = FixtureProvider::new();
    let actual_root = renderer
        .read(
            &mut provider,
            AudioSample(1602),
            256,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(actual_root.samples, root_reference[..256]);
    assert_eq!(renderer.cached_stage_count(), 1);
    for start in (0..3204).step_by(197).collect::<Vec<_>>().into_iter().rev() {
        let count = (3204 - start).min(197) as u32;
        let block = renderer
            .read_definition(
                &mut provider,
                &definition,
                SignalSample(start as i64),
                count,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(block.samples, expected[start..start + count as usize]);
        assert!(block.suppressed.is_empty());
    }
    assert_eq!(
        renderer.cached_stage_count(),
        2,
        "root and definition scopes retain separate stages with the same node alias"
    );
    assert_eq!(
        renderer
            .read(
                &mut provider,
                AudioSample(1602),
                256,
                TIMEOUT,
                &AtomicBool::new(false)
            )
            .unwrap()
            .samples,
        root_reference[..256]
    );
    assert_eq!(renderer.cached_stage_count(), 2);
}

fn nested_fixture() -> (ProjectDocument, Vec<[f32; 2]>, Vec<[f32; 2]>) {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let mut absent = source(rate, 128, 0..128);
    let NodeKind::Source { source: picture } = &mut absent.kind else {
        unreachable!()
    };
    picture.audio = None;
    picture.audio_mapping = SourceAudioMapping::FitBeat;
    picture.video = SourceVideo::Stream {
        asset: AssetId::new("picture").unwrap(),
        span: audio(0..128).span,
    };
    let doc = document(
        rate,
        &["prefix", "crop"],
        [
            ("prefix", source(rate, 64, 4096..4160)),
            ("a", source(rate, 256, 0..256)),
            (
                "room",
                hold(
                    256,
                    HoldAudio::RoomTone {
                        source: audio(512..640),
                    },
                ),
            ),
            ("absent", absent),
            ("silent", hold(128, HoldAudio::Silence)),
            ("b", source(rate, 256, 1024..1280)),
            (
                "cuts",
                BeatNode::sequence(
                    "Input policies",
                    vec![id("a"), id("room"), id("absent"), id("silent"), id("b")],
                ),
            ),
            (
                "inner",
                retime("cuts", 1536, 0..1024, PitchPolicy::Preserve),
            ),
            (
                "outer",
                retime("inner", 1280, 0..1536, PitchPolicy::Preserve),
            ),
            (
                "crop",
                retime("outer", 256, 1000..1256, PitchPolicy::FollowSpeed),
            ),
        ],
        BTreeMap::new(),
    );
    let room_source: Vec<_> = (512..640).map(fixture_sample).collect();
    let room = RoomTone::new(
        RoomToneRecipe::new(ExactRatio::integer(128), 256).unwrap(),
        &room_source,
        &AtomicBool::new(false),
    )
    .unwrap()
    .render(AudioSample(0), 256, &AtomicBool::new(false))
    .unwrap()
    .samples;
    let input: Vec<_> = (0..256)
        .map(fixture_sample)
        .chain(room.iter().copied())
        .chain([[0.0; 2]; 256])
        .chain((1024..1280).map(fixture_sample))
        .collect();
    let mut inner = stretch(&input, 1536, 2, 3);
    inner[960..1152].fill([0.0; 2]);
    let mut expected = stretch(&inner, 1280, 6, 5);
    expected[800..960].fill([0.0; 2]);
    (doc, expected, room)
}

#[test]
fn definitions_keep_nested_history_silence_room_phase_and_distinct_cache_scopes() {
    let (doc, expected, room) = nested_fixture();
    assert!(
        expected[640..704]
            .iter()
            .flatten()
            .any(|sample| sample.abs() > 1e-6),
        "absent Source audio keeps processed decay"
    );
    for frozen in [false, true] {
        let planned = compile(&doc, frozen);
        let definition = planned.audio_definition(node("outer")).unwrap();
        let mut renderer = StageAudio::new(Arc::clone(&planned));
        let mut provider = FixtureProvider::new();
        // Root sees only the late crop. The definition must still prepare the
        // complete RoomTone and both canonical Preserve histories from zero.
        assert_eq!(
            renderer
                .read(
                    &mut provider,
                    AudioSample(64),
                    256,
                    TIMEOUT,
                    &AtomicBool::new(false)
                )
                .unwrap()
                .samples,
            expected[1000..1256]
        );
        assert_eq!(renderer.cached_stage_count(), 3);
        for start in (0..1280).step_by(173).collect::<Vec<_>>().into_iter().rev() {
            let count = (1280 - start).min(173) as u32;
            let block = renderer
                .read_definition(
                    &mut provider,
                    &definition,
                    SignalSample(start as i64),
                    count,
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap();
            assert_eq!(block.samples, expected[start..start + count as usize]);
            let left = start.max(800);
            let right = (start + count as usize).min(960);
            let suppressed = if left < right {
                vec![SignalSample(left as i64)..SignalSample(right as i64)]
            } else {
                vec![]
            };
            assert_eq!(block.suppressed, suppressed);
        }
        assert_eq!(
            renderer.cached_stage_count(),
            6,
            "nested stages and RoomTone must all carry definition scope"
        );
        let room_definition = planned.audio_definition(node("room")).unwrap();
        assert_eq!(
            renderer
                .read_definition(
                    &mut provider,
                    &room_definition,
                    SignalSample(0),
                    256,
                    TIMEOUT,
                    &AtomicBool::new(false)
                )
                .unwrap()
                .samples,
            room
        );
        assert_eq!(
            renderer.cached_stage_count(),
            7,
            "same RoomTone alias under a different definition is a separate preparation"
        );
        assert_eq!(provider.context_calls > 0, frozen);
        provider.unavailable = true;
        for target in [&definition, &room_definition] {
            assert!(matches!(
                renderer.read_definition(
                    &mut provider,
                    target,
                    SignalSample(0),
                    1,
                    TIMEOUT,
                    &AtomicBool::new(false)
                ),
                Err(StageAudioError::Preparation(
                    PreparationError::SourceUnavailable(_)
                ))
            ));
        }
    }
}

#[test]
fn definition_preserve_retains_silent_policy_without_an_input_grid_point() {
    let rate = FrameRate::new(192_000, 1).unwrap();
    let doc = document(
        rate,
        &["outer"],
        [
            ("a", source(rate, 1, 0..1)),
            ("silent", hold(1, HoldAudio::Silence)),
            ("b", source(rate, 1, 1..2)),
            (
                "cuts",
                BeatNode::sequence("Subsample Hold", vec![id("a"), id("silent"), id("b")]),
            ),
            ("inner", retime("cuts", 24, 0..3, PitchPolicy::Preserve)),
            ("outer", retime("inner", 48, 0..24, PitchPolicy::Preserve)),
        ],
        BTreeMap::new(),
    );
    let planned = compile(&doc, false);
    let definition = planned.audio_definition(node("outer")).unwrap();
    assert_eq!(
        definition.signal().sample_count().unwrap(),
        SignalSample(12)
    );
    let mut inner = stretch(&[fixture_sample(0)], 6, 1, 8);
    inner[2..4].fill([0.0; 2]);
    let mut expected = stretch(&inner, 12, 1, 2);
    expected[4..8].fill([0.0; 2]);
    let mut renderer = StageAudio::new(Arc::clone(&planned));
    let block = renderer
        .read_definition(
            &mut FixtureProvider::new(),
            &definition,
            SignalSample(0),
            12,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(block.samples, expected);
    assert_eq!(block.suppressed, vec![SignalSample(4)..SignalSample(8)]);
    assert!(
        block.samples[..4]
            .iter()
            .chain(&block.samples[8..])
            .flatten()
            .any(|value| value.abs() > 1e-5)
    );
}

#[test]
fn definitions_reject_foreign_handles_invalid_ranges_cancel_and_bounded_work() {
    let (doc, _, _) = nested_fixture();
    let planned = compile(&doc, false);
    let foreign = compile(&doc, false);
    let definition = planned.audio_definition(node("outer")).unwrap();
    assert!(!definition.belongs_to(&foreign));
    let mut provider = FixtureProvider::new();
    let mut renderer = StageAudio::new(foreign);
    assert!(
        renderer
            .read_definition(
                &mut provider,
                &definition,
                SignalSample(0),
                1,
                TIMEOUT,
                &AtomicBool::new(false)
            )
            .is_err()
    );
    assert_eq!(provider.calls, 0);
    let mut renderer = StageAudio::new(Arc::clone(&planned));
    for (start, count, timeout) in [
        (-1, 1, TIMEOUT),
        (1280, 1, TIMEOUT),
        (1279, 2, TIMEOUT),
        (0, 0, TIMEOUT),
        (0, 257, TIMEOUT),
        (i64::MAX, 1, TIMEOUT),
        (0, 1, Duration::ZERO),
        (0, 1, Duration::from_secs(61)),
    ] {
        assert!(
            renderer
                .read_definition(
                    &mut provider,
                    &definition,
                    SignalSample(start),
                    count,
                    timeout,
                    &AtomicBool::new(false)
                )
                .is_err()
        );
        assert_eq!(provider.calls, 0);
    }
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
    for limits in [
        StageLimits {
            maximum_input_frames: 1535,
            ..Default::default()
        },
        StageLimits {
            maximum_resident_frames: 100,
            ..Default::default()
        },
        StageLimits {
            maximum_prepared_stages: 1,
            ..Default::default()
        },
        StageLimits {
            maximum_prepared_frames: 5375,
            ..Default::default()
        },
    ] {
        let mut limited = StageAudio::with_limits(Arc::clone(&planned), limits).unwrap();
        assert!(matches!(
            limited.read_definition(
                &mut provider,
                &definition,
                SignalSample(0),
                1,
                TIMEOUT,
                &AtomicBool::new(false)
            ),
            Err(StageAudioError::Limit(_))
        ));
        assert_eq!(limited.cached_stage_count(), 0);
        assert_eq!(provider.calls, 0);
    }
    provider.cancel_on_call = true;
    assert!(
        renderer
            .read_definition(
                &mut provider,
                &definition,
                SignalSample(0),
                1,
                TIMEOUT,
                &AtomicBool::new(false)
            )
            .unwrap_err()
            .is_cancelled()
    );
    assert_eq!(renderer.cached_stage_count(), 0);
}

#[test]
fn historical_definition_survives_live_deletion_and_rechecks_cached_context_admission() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let original = document(
        rate,
        &["stage"],
        [
            ("a", source(rate, 512, 512..1024)),
            ("stage", retime("a", 768, 0..512, PitchPolicy::Preserve)),
        ],
        BTreeMap::new(),
    );
    let frozen = compile(&original, true);
    let deleted = apply(
        &original,
        &CommandRequest {
            project_id: original.project_id().clone(),
            expected_revision: original.revision_id().clone(),
            new_revision: RevisionId::new("after-deletion").unwrap(),
            command: Command::Delete { node: id("stage") },
        },
    )
    .unwrap()
    .forward
    .apply(&original)
    .unwrap();
    assert!(!deleted.nodes().contains_key(&id("stage")));
    assert!(!deleted.nodes().contains_key(&id("a")));
    assert_eq!(
        compile(&deleted, false).audio_duration().unwrap(),
        AudioSample(0)
    );
    let definition = frozen.audio_definition(node("stage")).unwrap();
    let expected = stretch(
        &(512..1024).map(fixture_sample).collect::<Vec<_>>(),
        768,
        2,
        3,
    );
    let mut renderer = StageAudio::new(Arc::clone(&frozen));
    let mut provider = FixtureProvider::new();
    // Admission still names the immutable historical revision. Current node
    // lookup could not resolve either this definition or its source dependency.
    for start in [600, 0] {
        let actual = renderer
            .read_definition(
                &mut provider,
                &definition,
                SignalSample(start),
                64,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(
            actual.samples,
            expected[start as usize..start as usize + 64]
        );
        assert_eq!(&actual.revision_id, original.revision_id());
    }
    assert_eq!(renderer.cached_stage_count(), 1);
    let calls = provider.context_calls;
    provider.unavailable = true;
    assert!(matches!(
        renderer.read_definition(
            &mut provider,
            &definition,
            SignalSample(0),
            1,
            TIMEOUT,
            &AtomicBool::new(false),
        ),
        Err(StageAudioError::Preparation(
            PreparationError::SourceUnavailable(_)
        ))
    ));
    assert_eq!(provider.context_calls, calls + 1);
}

#[test]
fn owned_source_root_clock_retains_signed_ntsc_phase_and_exact_filter_crop() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let doc = document(
        rate,
        &["a"],
        [("a", source(rate, 3, 0..4805))],
        BTreeMap::new(),
    );
    let planned = compile(&doc, false);
    let definition = planned.audio_definition(node("a")).unwrap();
    let placement =
        deadpan_plan::AudioRootPlacement::new(ratio(-13, 7), ratio(3, 2), ratio(1, 3)..ratio(5, 3))
            .unwrap();
    let domain = definition.in_root_clock(placement.clone()).unwrap();
    // Independent arithmetic: root support is [-19/14,9/14) frames,
    // rounding to [-2174,1030). Source support [8008/15,8008/3)
    // admits discrete taps [534,2670). The first source phase is 533.6,
    // not the cropped first tap and not a phase restarted at root zero.
    assert_eq!(domain.root_samples(), AudioSample(-2174)..AudioSample(1030));
    let first_span = &domain
        .audio(AudioSample(-2174)..AudioSample(-2173), Default::default())
        .unwrap()
        .spans[0];
    assert!(
        !first_span.boundaries.start.is_empty(),
        "cropped support owns its incoming envelope edge"
    );
    assert!(
        !first_span.boundaries.end.is_empty(),
        "cropped support owns its outgoing envelope edge"
    );
    let expected = sample_reference(534..2670, ratio(2668, 5), ratio(2, 3), 3204, fixture_sample);
    let uncropped = sample_reference(0..4805, ratio(2668, 5), ratio(2, 3), 256, fixture_sample);
    assert_ne!(expected[..256], uncropped);
    let mut renderer = StageAudio::new(Arc::clone(&planned));
    let mut provider = FixtureProvider::new();
    for offset in (0..3204).step_by(191).collect::<Vec<_>>().into_iter().rev() {
        let count = (3204 - offset).min(191) as u32;
        let actual = renderer
            .read_domain(
                &mut provider,
                &domain,
                AudioSample(-2174 + offset as i64),
                count,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap_or_else(|error| {
                panic!("signed crop offset {offset}, count {count}: {error:?}")
            });
        assert_eq!(actual.samples, expected[offset..offset + count as usize]);
        assert_eq!(actual.definition, Some(node("a")));
        assert_eq!(actual.placement, Some(placement.clone()));
        assert!(actual.suppressed.is_empty());
    }
    assert!(provider.calls > 0);
    assert_eq!(provider.context_calls, 0);
}

struct RevisionProvider {
    fixture: FixtureProvider,
    revision: RevisionId,
}

impl AudioSourceProvider for RevisionProvider {
    fn source(
        &mut self,
        project: &ProjectId,
        revision: &RevisionId,
        asset: &AssetId,
        cancelled: &AtomicBool,
    ) -> Result<&PreparedSource, PreparationError> {
        assert_eq!(
            revision, &self.revision,
            "owned media uses the current revision"
        );
        self.fixture.source(
            project,
            &RevisionId::new("definition-revision").unwrap(),
            asset,
            cancelled,
        )
    }
}

#[test]
fn owned_nested_preserve_uses_edited_room_tone_in_the_same_root_clock() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let silent_gap = HoldRecipe {
        duration: frames(128),
        video: HoldVideo::Background,
        audio: HoldAudio::Silence,
    };
    let original = document(
        rate,
        &["outer"],
        [
            ("a", source(rate, 256, 512..768)),
            (
                "repeat",
                BeatNode {
                    framing: None,
                    label: "Editable gap".into(),
                    audio_edges: Default::default(),
                    kind: NodeKind::Repeat {
                        child: id("a"),
                        iterations: IterationOrder::new(RevisionId::new("plays").unwrap(), 2)
                            .unwrap(),
                        gap: Some(silent_gap.clone()),
                    },
                },
            ),
            (
                "inner",
                retime("repeat", 960, 0..640, PitchPolicy::Preserve),
            ),
            (
                "outer",
                retime("inner", 1280, 0..960, PitchPolicy::Preserve),
            ),
        ],
        BTreeMap::new(),
    );
    let edit = apply(
        &original,
        &CommandRequest {
            project_id: original.project_id().clone(),
            expected_revision: original.revision_id().clone(),
            new_revision: RevisionId::new("room-tone-revision").unwrap(),
            command: Command::SetRepeat {
                node: id("repeat"),
                plays: 2,
                gap: Some(HoldRecipe {
                    audio: HoldAudio::RoomTone {
                        source: audio(6144..6272),
                    },
                    ..silent_gap
                }),
            },
        },
    )
    .unwrap();
    let changed = edit.forward.apply(&original).unwrap();
    assert_eq!(edit.inverse.apply(&changed).unwrap(), original);
    let source_pcm: Vec<_> = (512..768).map(fixture_sample).collect();
    let room_pcm: Vec<_> = (6144..6272).map(fixture_sample).collect();
    let room = RoomTone::new(
        RoomToneRecipe::new(ExactRatio::integer(128), 128).unwrap(),
        &room_pcm,
        &AtomicBool::new(false),
    )
    .unwrap()
    .render(AudioSample(0), 128, &AtomicBool::new(false))
    .unwrap()
    .samples;
    let mut rendered = Vec::new();
    for (doc, gap, mute) in [
        (&original, vec![[0.0; 2]; 128], true),
        (&changed, room, false),
    ] {
        let input: Vec<_> = source_pcm
            .iter()
            .copied()
            .chain(gap)
            .chain(source_pcm.iter().copied())
            .collect();
        let mut inner = stretch(&input, 960, 2, 3);
        if mute {
            inner[384..576].fill([0.0; 2]);
        }
        let mut expected = stretch(&inner, 1280, 3, 4);
        if mute {
            expected[512..768].fill([0.0; 2]);
        }
        let planned = compile(doc, false);
        let definition = planned.audio_definition(node("outer")).unwrap();
        let domain = definition
            .in_root_clock(
                deadpan_plan::AudioRootPlacement::new(
                    ExactRatio::integer(-64),
                    ExactRatio::ONE,
                    ExactRatio::ZERO..ExactRatio::integer(1280),
                )
                .unwrap(),
            )
            .unwrap();
        let mut renderer = StageAudio::new(Arc::clone(&planned));
        let mut provider = RevisionProvider {
            fixture: FixtureProvider::new(),
            revision: doc.revision_id().clone(),
        };
        for offset in [1024, 768, 512, 256, 0] {
            let block = renderer
                .read_domain(
                    &mut provider,
                    &domain,
                    AudioSample(offset - 64),
                    256,
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap();
            assert_eq!(
                block.samples,
                expected[offset as usize..offset as usize + 256]
            );
            assert_eq!(&block.revision_id, doc.revision_id());
            let suppression = if mute && offset == 512 {
                vec![AudioSample(448)..AudioSample(704)]
            } else {
                vec![]
            };
            assert_eq!(block.suppressed, suppression);
        }
        assert!(provider.fixture.calls > 0);
        provider.fixture.unavailable = true;
        assert!(matches!(
            renderer.read_domain(
                &mut provider,
                &domain,
                AudioSample(448),
                16,
                TIMEOUT,
                &AtomicBool::new(false)
            ),
            Err(StageAudioError::Preparation(
                PreparationError::SourceUnavailable(_)
            ))
        ));
        rendered.push(expected);
    }
    assert!(rendered[0][512..768].iter().all(|value| *value == [0.0; 2]));
    assert!(
        rendered[1][512..768]
            .iter()
            .flatten()
            .any(|value| value.abs() > 1e-5)
    );
    assert_ne!(rendered[0], rendered[1]);
}

#[test]
fn owned_root_placement_keeps_silent_hold_without_any_preserve_input_point() {
    let rate = FrameRate::new(192_000, 1).unwrap();
    let doc = document(
        rate,
        &["outer"],
        [
            ("a", source(rate, 1, 0..1)),
            ("silent", hold(1, HoldAudio::Silence)),
            ("b", source(rate, 1, 1..2)),
            (
                "cuts",
                BeatNode::sequence("Subsample Hold", vec![id("a"), id("silent"), id("b")]),
            ),
            ("inner", retime("cuts", 24, 0..3, PitchPolicy::Preserve)),
            ("outer", retime("inner", 48, 0..24, PitchPolicy::Preserve)),
        ],
        BTreeMap::new(),
    );
    let planned = compile(&doc, false);
    let definition = planned.audio_definition(node("outer")).unwrap();
    let domain = definition
        .in_root_clock(
            deadpan_plan::AudioRootPlacement::new(
                ratio(-1, 8),
                ExactRatio::ONE,
                ExactRatio::ZERO..ExactRatio::integer(48),
            )
            .unwrap(),
        )
        .unwrap();
    let mut inner = stretch(&[fixture_sample(0)], 6, 1, 8);
    inner[2..4].fill([0.0; 2]);
    let mut outer = stretch(&inner, 12, 1, 2);
    outer[4..8].fill([0.0; 2]);
    let mut expected = sample_reference(0..12, ratio(1, 32), ExactRatio::ONE, 12, |at| {
        outer[at as usize]
    });
    expected[4..8].fill([0.0; 2]);
    let mut renderer = StageAudio::new(Arc::clone(&planned));
    let block = renderer
        .read_domain(
            &mut FixtureProvider::new(),
            &domain,
            AudioSample(0),
            12,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(block.samples, expected);
    assert_eq!(block.suppressed, vec![AudioSample(4)..AudioSample(8)]);
    assert!(
        block.samples[..4]
            .iter()
            .chain(&block.samples[8..])
            .flatten()
            .any(|value| value.abs() > 1e-5)
    );
}

#[test]
fn owned_root_placements_share_intrinsic_preparation_without_rebinding_clocks() {
    let (doc, expected, _) = nested_fixture();
    let planned = compile(&doc, false);
    let definition = planned.audio_definition(node("outer")).unwrap();
    let placement = |origin| {
        deadpan_plan::AudioRootPlacement::new(
            ExactRatio::integer(origin),
            ExactRatio::ONE,
            ExactRatio::ZERO..ExactRatio::integer(1280),
        )
        .unwrap()
    };
    let first = definition.in_root_clock(placement(-64)).unwrap();
    let same = definition.in_root_clock(placement(-64)).unwrap();
    let moved = definition.in_root_clock(placement(384)).unwrap();
    let mut renderer = StageAudio::new(Arc::clone(&planned));
    let mut provider = FixtureProvider::new();
    for (domain, start) in [(&first, -64), (&same, -64), (&moved, 384)] {
        let block = renderer
            .read_domain(
                &mut provider,
                domain,
                AudioSample(start),
                256,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(block.samples, expected[..256]);
        assert_eq!(block.placement.as_ref(), domain.placement());
    }
    let count = renderer.cached_stage_count();
    assert_eq!(
        renderer
            .read_definition(
                &mut provider,
                &definition,
                SignalSample(0),
                256,
                TIMEOUT,
                &AtomicBool::new(false)
            )
            .unwrap()
            .samples,
        expected[..256]
    );
    assert_eq!(
        renderer.cached_stage_count(),
        count,
        "root placement cannot restart an unchanged intrinsic preparation"
    );
    let ordinary_domain = planned
        .audio_domain_at(AudioSample(64), Default::default())
        .unwrap();
    assert_eq!(
        renderer
            .read_domain(
                &mut provider,
                &ordinary_domain,
                AudioSample(64),
                256,
                TIMEOUT,
                &AtomicBool::new(false)
            )
            .unwrap()
            .samples,
        expected[1000..1256]
    );
    assert_eq!(
        renderer
            .read(
                &mut provider,
                AudioSample(64),
                256,
                TIMEOUT,
                &AtomicBool::new(false)
            )
            .unwrap()
            .samples,
        expected[1000..1256]
    );
    provider.unavailable = true;
    for domain in [&first, &same, &moved, &ordinary_domain] {
        assert!(matches!(
            renderer.read_domain(
                &mut provider,
                domain,
                domain.visible_samples().start,
                1,
                TIMEOUT,
                &AtomicBool::new(false)
            ),
            Err(StageAudioError::Preparation(
                PreparationError::SourceUnavailable(_)
            ))
        ));
    }
}

#[test]
fn owned_root_domain_rejects_foreign_handles_ranges_cancellation_and_work_limits() {
    let (doc, _, _) = nested_fixture();
    let planned = compile(&doc, false);
    let foreign = compile(&doc, false);
    let definition = planned.audio_definition(node("outer")).unwrap();
    let domain = definition
        .in_root_clock(
            deadpan_plan::AudioRootPlacement::new(
                ExactRatio::integer(-64),
                ExactRatio::ONE,
                ExactRatio::ZERO..ExactRatio::integer(1280),
            )
            .unwrap(),
        )
        .unwrap();
    let mut provider = FixtureProvider::new();
    let mut wrong = StageAudio::new(foreign);
    assert!(matches!(
        wrong.read_domain(
            &mut provider,
            &domain,
            AudioSample(-64),
            1,
            TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::ForeignDomain)
    ));
    let mut renderer = StageAudio::new(Arc::clone(&planned));
    for (start, count, timeout) in [
        (-65, 1, TIMEOUT),
        (1216, 1, TIMEOUT),
        (1215, 2, TIMEOUT),
        (-64, 0, TIMEOUT),
        (-64, 257, TIMEOUT),
        (i64::MAX, 1, TIMEOUT),
        (-64, 1, Duration::ZERO),
    ] {
        assert!(
            renderer
                .read_domain(
                    &mut provider,
                    &domain,
                    AudioSample(start),
                    count,
                    timeout,
                    &AtomicBool::new(false)
                )
                .is_err()
        );
        assert_eq!(provider.calls, 0);
    }
    assert!(
        renderer
            .read_domain(
                &mut provider,
                &domain,
                AudioSample(-64),
                1,
                TIMEOUT,
                &AtomicBool::new(true)
            )
            .unwrap_err()
            .is_cancelled()
    );
    let mut limited = StageAudio::with_limits(
        Arc::clone(&planned),
        StageLimits {
            maximum_prepared_stages: 1,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(matches!(
        limited.read_domain(
            &mut provider,
            &domain,
            AudioSample(-64),
            1,
            TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::Limit(_))
    ));
    assert_eq!(provider.calls, 0);
    assert_eq!(limited.cached_stage_count(), 0);
    provider.cancel_on_call = true;
    assert!(
        renderer
            .read_domain(
                &mut provider,
                &domain,
                AudioSample(-64),
                1,
                TIMEOUT,
                &AtomicBool::new(false)
            )
            .unwrap_err()
            .is_cancelled()
    );
    assert_eq!(renderer.cached_stage_count(), 0);
}
