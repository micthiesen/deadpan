#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use deadpan_audio::{
    AudioSourceProvider, PreparationError, PreparedSource, SequenceAudio, SequenceAudioError,
    StageAudio, StageAudioError,
};
use deadpan_core::*;
use deadpan_media::audio_index::AudioChannelLayout;
use deadpan_media::audio_session::{AudioSession, AudioSessionLimits};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_plan::RenderPlan;
use sha2::{Digest, Sha256};

const TIMEOUT: Duration = Duration::from_secs(10);

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}

fn audio(start: i64, end: i64) -> SourceAudio {
    let time_base = SourceTimeBase::new(1, 48_000).unwrap();
    SourceAudio {
        asset: AssetId::new("media").unwrap(),
        span: SourceSpan::new(
            SourceTimestamp {
                ticks: start,
                time_base,
            },
            SourceTimestamp {
                ticks: end,
                time_base,
            },
        )
        .unwrap(),
    }
}

fn source(length: i64, selection: SourceAudio, offset: i64) -> BeatNode {
    BeatNode {
        framing: None,
        label: "Original speech".into(),
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: AudioEdgePolicies {
            node_start: AudioEdgePolicy::Hard,
            node_end: AudioEdgePolicy::Hard,
            ..Default::default()
        },
        kind: NodeKind::Source {
            source: SourceNode {
                edit_window: None,
                duration: frames(length),
                video: SourceVideo::Blank,
                video_mapping: SourceVideoMapping::FitBeat,
                audio: Some(selection),
                audio_mapping: SourceAudioMapping::FitBeat,
                audio_offset: AudioSample(offset),
                link: LinkRelation::Independent,
            },
        },
        cutaways: Vec::new(),
        captions: Vec::new(),
    }
}

fn picture_only(length: i64) -> BeatNode {
    let mut node = source(length, audio(0, length), 0);
    if let NodeKind::Source { source } = &mut node.kind {
        source.audio = None;
        source.video = SourceVideo::Stream {
            asset: AssetId::new("picture").unwrap(),
            span: audio(0, length).span,
        };
    }
    node.audio_edges.source_placement_start = AudioEdgePolicy::Hard;
    node.audio_edges.source_placement_end = AudioEdgePolicy::Hard;
    node
}

fn hold(length: i64, selected: HoldAudio) -> BeatNode {
    BeatNode::hold(
        "Hold",
        HoldRecipe {
            picture_context: None,
            duration: frames(length),
            video: HoldVideo::Background,
            audio: selected,
        },
    )
}

fn retime(
    child: &str,
    length: i64,
    selection: std::ops::Range<i64>,
    pitch: PitchPolicy,
) -> BeatNode {
    BeatNode {
        framing: None,
        label: "Retime".into(),
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Retime {
            purpose: RetimePurpose::Edit,
            child: id(child),
            duration: frames(length),
            mapping: FrameRange::new(ProjectFrame(selection.start), ProjectFrame(selection.end))
                .unwrap(),
            pitch,
        },
        cutaways: Vec::new(),
        captions: Vec::new(),
    }
}

fn document() -> ProjectDocument {
    let empty = ProjectDocument::new(
        ProjectId::new("context-audio-project").unwrap(),
        RevisionId::new("context-audio-revision").unwrap(),
        PresentationBasis {
            width: 1920,
            height: 1080,
            frame_rate: FrameRate::new(48_000, 1).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let nodes = BTreeMap::from([
        (
            id("root"),
            BeatNode::sequence(
                "Root",
                vec![id("source"), id("room"), id("outer"), id("silence")],
            ),
        ),
        (id("source"), source(64, audio(0, 64), 2)),
        (
            id("room"),
            hold(
                16,
                HoldAudio::RoomTone {
                    source: audio(512, 640),
                },
            ),
        ),
        (id("nested-source"), source(48, audio(1024, 1072), 0)),
        (id("picture-only"), picture_only(16)),
        (
            id("inner-sequence"),
            BeatNode::sequence(
                "Speech and picture-only gap",
                vec![id("nested-source"), id("picture-only")],
            ),
        ),
        (
            id("inner"),
            retime("inner-sequence", 96, 0..64, PitchPolicy::Preserve),
        ),
        (
            id("outer"),
            retime("inner", 128, 0..96, PitchPolicy::Preserve),
        ),
        (id("silence"), hold(8, HoldAudio::Silence)),
    ]);
    let mut wire = serde_json::to_value(empty).unwrap();
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    wire["assets"] = serde_json::to_value(BTreeMap::from([
        (
            AssetId::new("media").unwrap(),
            AssetRecord {
                label: "Qualified stereo PCM".into(),
                content_hash: "a".repeat(64),
                video: None,
                audio: Some(audio(0, 8197).span),
                still_image: true,
                frame_count: None,
                source_qualification: None,
            },
        ),
        (
            AssetId::new("picture").unwrap(),
            AssetRecord {
                label: "Picture without audio".into(),
                content_hash: "b".repeat(64),
                video: Some(audio(0, 256).span),
                audio: None,
                still_image: false,
                frame_count: None,
                source_qualification: None,
            },
        ),
    ]))
    .unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn split(document: &ProjectDocument) -> ProjectDocument {
    let request = CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new("after-split").unwrap(),
        command: Command::Split {
            node: id("source"),
            at: frames(29),
            identities: SplitIdentities {
                nodes: (0..document.nodes().len() + 4)
                    .map(|index| id(&format!("split-{index}")))
                    .collect(),
            },
        },
    };
    apply(document, &request)
        .unwrap()
        .forward
        .apply(document)
        .unwrap()
}

struct FixtureProvider {
    prepared: PreparedSource,
    expected: AssetRecord,
    project: ProjectId,
    revision: RevisionId,
    normal_calls: usize,
    context_calls: usize,
}

impl FixtureProvider {
    fn new(document: &ProjectDocument) -> Self {
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
        let prepared = PreparedSource::with_layout(
            session,
            &index,
            AudioChannelLayout::Native {
                channels: 2,
                mask: 3,
            },
            &cancelled,
        )
        .unwrap();
        Self {
            prepared,
            expected: document.assets()[&AssetId::new("media").unwrap()].clone(),
            project: document.project_id().clone(),
            revision: document.revision_id().clone(),
            normal_calls: 0,
            context_calls: 0,
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
        assert_eq!(project, &self.project);
        assert_eq!(revision, &self.revision);
        assert_eq!(asset, &AssetId::new("media").unwrap());
        assert!(!cancelled.load(std::sync::atomic::Ordering::Relaxed));
        self.normal_calls += 1;
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
        if project != &self.project
            || revision != &self.revision
            || asset != &AssetId::new("media").unwrap()
            || expected != &self.expected
        {
            return Err(PreparationError::SourceUnavailable(
                "retained contract differs from qualified fixture".into(),
            ));
        }
        if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(PreparationError::Cancelled);
        }
        Ok(&self.prepared)
    }
}

struct LegacyProvider {
    calls: usize,
}

impl AudioSourceProvider for LegacyProvider {
    fn source(
        &mut self,
        _: &ProjectId,
        _: &RevisionId,
        _: &AssetId,
        _: &AtomicBool,
    ) -> Result<&PreparedSource, PreparationError> {
        self.calls += 1;
        Err(PreparationError::SourceUnavailable(
            "legacy provider called".into(),
        ))
    }
}

#[test]
fn restored_context_renders_qualified_source_room_tone_nested_preserve_split_and_silence() {
    let split = split(&document());
    let original = Arc::new(RenderPlan::compile(&split).unwrap());
    let context = FrozenAudioContext::capture(&split).unwrap();
    let restored = FrozenAudioContext::from_json(&context.to_json().unwrap()).unwrap();
    let frozen = Arc::new(RenderPlan::compile_audio_context(&restored).unwrap());
    let mut provider = FixtureProvider::new(&split);

    let deleted = apply(
        &split,
        &CommandRequest {
            project_id: split.project_id().clone(),
            expected_revision: split.revision_id().clone(),
            new_revision: RevisionId::new("after-deletion").unwrap(),
            command: Command::Delete { node: id("room") },
        },
    )
    .unwrap()
    .forward
    .apply(&split)
    .unwrap();
    assert!(!deleted.nodes().contains_key(&id("room")));
    assert_ne!(deleted.revision_id(), restored.revision_id());

    let mut before = StageAudio::new(Arc::clone(&original));
    let mut after = StageAudio::new(Arc::clone(&frozen));
    let cancelled = AtomicBool::new(false);
    let duration = original.audio_duration().unwrap().0;
    assert_eq!(duration, frozen.audio_duration().unwrap().0);
    assert_eq!(duration, 216);
    let mut complete = Vec::new();
    for start in (0..duration).step_by(127) {
        let count = u32::try_from((duration - start).min(127)).unwrap();
        complete.extend(
            before
                .read_edge_faded(
                    &mut provider,
                    AudioSample(start),
                    count,
                    TIMEOUT,
                    &cancelled,
                )
                .unwrap()
                .samples,
        );
    }
    // Request the suffix first to exercise retained Preserve and room-tone
    // preparation without relying on sequential output reads.
    for (start, count) in [
        (208, 8),
        (198, 10),
        (90, 33),
        (62, 19),
        (26, 40),
        (0, 29),
        (128, 70),
    ] {
        let actual = after
            .read_edge_faded(
                &mut provider,
                AudioSample(start),
                count,
                TIMEOUT,
                &cancelled,
            )
            .unwrap();
        assert_eq!(
            actual.samples,
            complete[start as usize..start as usize + count as usize]
        );
    }
    assert_eq!(
        after
            .read_edge_faded(&mut provider, AudioSample(208), 8, TIMEOUT, &cancelled)
            .unwrap()
            .suppressed,
        vec![AudioSample(208)..AudioSample(216)]
    );
    // This region maps to the picture-only Source inside two Preserve stages.
    // Its absent audio must retain Source edges and must not become an authored
    // Hold suppression that cuts off processing history.
    let picture_before = before
        .read(&mut provider, AudioSample(184), 16, TIMEOUT, &cancelled)
        .unwrap();
    let picture_after = after
        .read(&mut provider, AudioSample(184), 16, TIMEOUT, &cancelled)
        .unwrap();
    assert_eq!(picture_after.samples, picture_before.samples);
    assert!(picture_after.suppressed.is_empty());
    assert!(provider.normal_calls > 0);
    assert!(provider.context_calls > 0);

    let normal_sources = SequenceAudio::new(original);
    let context_sources = SequenceAudio::new(frozen);
    for (start, count) in [(0, 29), (29, 33), (208, 8)] {
        let expected = normal_sources
            .read_sources(
                &mut provider,
                AudioSample(start),
                count,
                TIMEOUT,
                &cancelled,
            )
            .unwrap();
        let actual = context_sources
            .read_sources(
                &mut provider,
                AudioSample(start),
                count,
                TIMEOUT,
                &cancelled,
            )
            .unwrap();
        assert_eq!(actual.samples, expected.samples);
    }
}

#[test]
fn context_provider_is_required_and_retained_asset_contract_is_checked() {
    let doc = document();
    let context = FrozenAudioContext::from_json(
        &FrozenAudioContext::capture(&doc)
            .unwrap()
            .to_json()
            .unwrap(),
    )
    .unwrap();
    let plan = Arc::new(RenderPlan::compile_audio_context(&context).unwrap());
    let cancelled = AtomicBool::new(false);
    let mut legacy = LegacyProvider { calls: 0 };
    let mut stage = StageAudio::new(Arc::clone(&plan));
    assert!(matches!(
        stage.read(&mut legacy, AudioSample(2), 1, TIMEOUT, &cancelled),
        Err(StageAudioError::Preparation(
            PreparationError::SourceUnavailable(_)
        ))
    ));
    assert_eq!(legacy.calls, 0);
    assert!(matches!(
        stage.read(&mut legacy, AudioSample(64), 1, TIMEOUT, &cancelled),
        Err(StageAudioError::Preparation(
            PreparationError::SourceUnavailable(_)
        ))
    ));
    assert_eq!(legacy.calls, 0);
    let sequence = SequenceAudio::new(plan);
    assert!(matches!(
        sequence.read_sources(&mut legacy, AudioSample(2), 1, TIMEOUT, &cancelled),
        Err(SequenceAudioError::Preparation(
            PreparationError::SourceUnavailable(_)
        ))
    ));
    assert_eq!(legacy.calls, 0);

    let mut provider = FixtureProvider::new(&doc);
    let asset = AssetId::new("media").unwrap();
    let expected = context.assets()[&asset].clone();
    assert!(
        provider
            .source_for_context(
                context.project_id(),
                context.revision_id(),
                &asset,
                &expected,
                &cancelled
            )
            .is_ok()
    );
    let mut forged = expected.clone();
    forged.content_hash = "b".repeat(64);
    assert!(matches!(
        provider.source_for_context(
            context.project_id(),
            context.revision_id(),
            &asset,
            &forged,
            &cancelled
        ),
        Err(PreparationError::SourceUnavailable(_))
    ));
    assert!(matches!(
        provider.source_for_context(
            context.project_id(),
            &RevisionId::new("later-revision").unwrap(),
            &asset,
            &expected,
            &cancelled
        ),
        Err(PreparationError::SourceUnavailable(_))
    ));
}

#[test]
fn cached_context_stages_recheck_admission_before_returning_pcm() {
    let doc = document();
    let context = FrozenAudioContext::capture(&doc).unwrap();
    let plan = Arc::new(RenderPlan::compile_audio_context(&context).unwrap());
    let cancelled = AtomicBool::new(false);
    for start in [64, 90] {
        // These locations prepare RoomTone and nested Preserve respectively.
        let mut stage = StageAudio::new(Arc::clone(&plan));
        let mut provider = FixtureProvider::new(&doc);
        let expected = stage
            .read(&mut provider, AudioSample(start), 4, TIMEOUT, &cancelled)
            .unwrap();
        let calls = provider.context_calls;
        let cached = stage
            .read(&mut provider, AudioSample(start), 4, TIMEOUT, &cancelled)
            .unwrap();
        assert_eq!(cached.samples, expected.samples);
        assert!(provider.context_calls > calls);
        assert_eq!(provider.normal_calls, 0);

        provider.expected.content_hash = "c".repeat(64);
        assert!(matches!(
            stage.read(&mut provider, AudioSample(start), 4, TIMEOUT, &cancelled),
            Err(StageAudioError::Preparation(
                PreparationError::SourceUnavailable(_)
            ))
        ));
        let mut legacy = LegacyProvider { calls: 0 };
        assert!(matches!(
            stage.read(&mut legacy, AudioSample(start), 4, TIMEOUT, &cancelled),
            Err(StageAudioError::Preparation(
                PreparationError::SourceUnavailable(_)
            ))
        ));
        assert_eq!(legacy.calls, 0);
    }
}

#[test]
fn tail_context_retains_its_effect_and_renders_the_live_tail() {
    let empty = document();
    let mut wire = serde_json::to_value(&empty).unwrap();
    let mut nodes: BTreeMap<NodeId, BeatNode> =
        serde_json::from_value(wire["nodes"].clone()).unwrap();
    nodes.insert(
        id("room"),
        hold(
            2048,
            HoldAudio::Tail {
                maximum: frames(2000),
                effect: TailEffect::Reverb,
            },
        ),
    );
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    let doc = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let context = FrozenAudioContext::from_json(
        &FrozenAudioContext::capture(&doc)
            .unwrap()
            .to_json()
            .unwrap(),
    )
    .unwrap();
    let mut stage = StageAudio::new(Arc::new(
        RenderPlan::compile_audio_context(&context).unwrap(),
    ));
    let mut provider = FixtureProvider::new(&doc);
    let retained = stage
        .read(
            &mut provider,
            AudioSample(1152),
            256,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    // The frozen context keeps the effect choice, so the retained Hold
    // renders exactly the live document's tail.
    let mut live = StageAudio::new(Arc::new(RenderPlan::compile(&doc).unwrap()));
    let mut provider = FixtureProvider::new(&doc);
    let current = live
        .read(
            &mut provider,
            AudioSample(1152),
            256,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    // The reverb's first reflections reach the pause after its shortest
    // comb delay; the retained and live renders are the same bytes.
    assert!(current.samples.iter().any(|frame| *frame != [0.0; 2]));
    assert_eq!(retained.samples, current.samples);
}

/// A frozen context of a document whose Hold `room` carries `audio` renders
/// the same samples at `at` as the live document.
fn retained_hold_matches_live(audio: HoldAudio, at: i64) -> Vec<[f32; 2]> {
    let empty = document();
    let mut wire = serde_json::to_value(&empty).unwrap();
    let mut nodes: BTreeMap<NodeId, BeatNode> =
        serde_json::from_value(wire["nodes"].clone()).unwrap();
    nodes.insert(id("room"), hold(2048, audio));
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    let doc = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let context = FrozenAudioContext::from_json(
        &FrozenAudioContext::capture(&doc)
            .unwrap()
            .to_json()
            .unwrap(),
    )
    .unwrap();
    let read = |plan: RenderPlan| {
        let mut stage = StageAudio::new(Arc::new(plan));
        let mut provider = FixtureProvider::new(&doc);
        stage
            .read(
                &mut provider,
                AudioSample(at),
                256,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap()
            .samples
    };
    let retained = read(RenderPlan::compile_audio_context(&context).unwrap());
    let current = read(RenderPlan::compile(&doc).unwrap());
    assert_eq!(retained, current);
    current
}

#[test]
fn reversed_and_tone_contexts_render_the_live_hold() {
    let reversed = retained_hold_matches_live(
        HoldAudio::Reverse {
            source: audio(512, 640),
        },
        64,
    );
    assert!(reversed.iter().any(|frame| *frame != [0.0; 2]));
    let tone = retained_hold_matches_live(
        HoldAudio::Tone {
            frequency_hz: 1_000,
            level: GainDb::new(-6_000).unwrap(),
        },
        512,
    );
    assert!(tone.iter().any(|frame| frame[0].abs() > 0.4));
}
