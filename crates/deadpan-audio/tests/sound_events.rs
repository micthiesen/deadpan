#![cfg(any(target_os = "macos", target_os = "linux"))]

//! Admission failures use a deliberate provider sentinel. Reaching that sentinel
//! on a later request proves the renderer released its prior reservations without
//! making decoder availability or native media setup part of these tests.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use deadpan_audio::{
    AudioSourceProvider, PreparationError, PreparedSource, StageAudio, StageAudioError, StageLimits,
};
use deadpan_core::*;
use deadpan_plan::RenderPlan;

const TIMEOUT: Duration = Duration::from_secs(10);
const PROVIDER_SENTINEL: &str = "synthetic provider reached after admission";

fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn asset(value: &str) -> AssetId {
    AssetId::new(value).unwrap()
}

fn duration(frames: i64) -> FrameDuration {
    FrameDuration::new(frames).unwrap()
}

fn span(samples: i64) -> SourceSpan {
    let time_base = SourceTimeBase::new(1, 48_000).unwrap();
    SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base,
        },
        SourceTimestamp {
            ticks: samples,
            time_base,
        },
    )
    .unwrap()
}

fn record(samples: i64, qualified: bool) -> AssetRecord {
    AssetRecord {
        label: "Synthetic source contract".into(),
        content_hash: "a".repeat(64),
        video: None,
        audio: Some(span(samples)),
        frame_count: None,
        still_image: false,
        source_qualification: qualified
            .then(|| SourceQualificationId::new("b".repeat(64)).unwrap()),
    }
}

fn planned(
    mut nodes: BTreeMap<NodeId, BeatNode>,
    mut assets: BTreeMap<AssetId, AssetRecord>,
    child: &str,
    sound_samples: i64,
) -> Arc<RenderPlan> {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let empty = ProjectDocument::new(
        ProjectId::new("sound-admission").unwrap(),
        RevisionId::new("sound-revision").unwrap(),
        PresentationBasis {
            width: 320,
            height: 180,
            frame_rate: rate,
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )
    .unwrap();
    nodes.insert(
        node("root"),
        BeatNode::sequence("Sequence", vec![node(child)]),
    );
    assets.insert(asset("effect"), record(sound_samples, true));
    let source = SourceAudio {
        asset: asset("effect"),
        span: span(sound_samples),
    };
    let sound = SoundEvent {
        owner: node("root"),
        label: "Authored effect".into(),
        mapping: SourceAudioMapping::natural_rate(source.span, rate).unwrap(),
        source,
        offset: AudioSample(0),
        gain_millidecibels: 0,
        start_edge: AudioEdgePolicy::Hard,
        end_edge: AudioEdgePolicy::Hard,
        overflow: SoundOverflowPolicy::Reject,
    };
    let mut wire = serde_json::to_value(empty).unwrap();
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    wire["assets"] = serde_json::to_value(assets).unwrap();
    wire["sounds"] =
        serde_json::to_value(BTreeMap::from([(SoundId::new("effect").unwrap(), sound)])).unwrap();
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    Arc::new(RenderPlan::compile(&document).unwrap())
}

fn held_sound() -> Arc<RenderPlan> {
    planned(
        BTreeMap::from([(
            node("silence"),
            BeatNode::hold(
                "Silent hold",
                HoldRecipe {
                    duration: duration(256),
                    video: HoldVideo::Background,
                    audio: HoldAudio::Silence,
                    picture_context: None,
                },
            ),
        )]),
        BTreeMap::new(),
        "silence",
        256,
    )
}

fn many_original_dependencies(count: u32) -> Arc<RenderPlan> {
    let mut nodes = BTreeMap::new();
    let mut assets = BTreeMap::new();
    let mut children = Vec::new();
    for index in 0..count {
        let name = format!("original-{index:04}");
        let id = node(&name);
        assets.insert(asset(&name), record(1, false));
        nodes.insert(
            id.clone(),
            BeatNode {
                label: name.clone(),
                framing: None,
                audio_edges: Default::default(),
                kind: NodeKind::Source {
                    source: SourceNode {
                        duration: duration(1),
                        video: SourceVideo::Blank,
                        video_mapping: SourceVideoMapping::FitBeat,
                        audio: Some(SourceAudio {
                            asset: asset(&name),
                            span: span(1),
                        }),
                        audio_mapping: SourceAudioMapping::Duration {
                            frames: ExactRatio::ONE,
                        },
                        audio_offset: AudioSample(0),
                        link: LinkRelation::Independent,
                    },
                },
            },
        );
        children.push(id);
    }
    nodes.insert(
        node("inputs"),
        BeatNode::sequence("Continuous Original", children),
    );
    nodes.insert(
        node("preserve"),
        BeatNode {
            label: "Complete Original processing history".into(),
            framing: None,
            audio_edges: Default::default(),
            kind: NodeKind::Retime {
                child: node("inputs"),
                duration: duration(i64::from(count) * 2),
                mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(i64::from(count))).unwrap(),
                pitch: PitchPolicy::Preserve,
                purpose: RetimePurpose::Edit,
            },
        },
    );
    planned(nodes, assets, "preserve", 1)
}

#[derive(Default)]
struct SentinelProvider {
    calls: Vec<AssetId>,
    cancel_on_call: bool,
}

impl AudioSourceProvider for SentinelProvider {
    fn source(
        &mut self,
        project: &ProjectId,
        revision: &RevisionId,
        asset: &AssetId,
        cancelled: &AtomicBool,
    ) -> Result<&PreparedSource, PreparationError> {
        assert_eq!(project.as_str(), "sound-admission");
        assert_eq!(revision.as_str(), "sound-revision");
        assert!(!cancelled.load(Ordering::Relaxed));
        self.calls.push(asset.clone());
        if self.cancel_on_call {
            cancelled.store(true, Ordering::Relaxed);
            return Err(PreparationError::Cancelled);
        }
        Err(PreparationError::SourceUnavailable(
            PROVIDER_SENTINEL.into(),
        ))
    }
}

fn reached_provider(error: StageAudioError) {
    assert!(matches!(
        error,
        StageAudioError::Preparation(PreparationError::SourceUnavailable(message))
            if message == PROVIDER_SENTINEL
    ));
}

#[test]
fn original_and_sound_dependencies_share_one_limit_before_provider_resolution() {
    let active = AtomicBool::new(false);
    let mut provider = SentinelProvider::default();
    // A one-sample output read needs the Original's complete Preserve history.
    // Those 1024 sources fit alone, but the independently authored effect adds
    // the 1025th dependency even before any of its samples are requested.
    let mut renderer = StageAudio::new(many_original_dependencies(1024));
    assert!(matches!(
        renderer.prepare_edge_faded(&mut provider, AudioSample(0), 1, TIMEOUT, &active),
        Err(StageAudioError::Limit("source dependencies"))
    ));
    assert!(provider.calls.is_empty());
    assert_eq!(renderer.cached_stage_count(), 0);

    // The exact boundary must pass static admission and reach the decoder
    // provider rather than accidentally counting the same source twice.
    let mut boundary = StageAudio::new(many_original_dependencies(1023));
    reached_provider(
        boundary
            .prepare_edge_faded(&mut provider, AudioSample(0), 1, TIMEOUT, &active)
            .unwrap_err(),
    );
    assert_eq!(provider.calls, [asset("original-0000")]);
    assert_eq!(boundary.cached_stage_count(), 0);
}

#[test]
fn resident_budget_rejection_and_provider_failure_release_bus_reservations() {
    let mut renderer = StageAudio::with_limits(
        held_sound(),
        StageLimits {
            maximum_resident_frames: 640,
            ..Default::default()
        },
    )
    .unwrap();
    let mut provider = SentinelProvider::default();
    let active = AtomicBool::new(false);
    assert!(matches!(
        renderer.prepare_edge_faded(&mut provider, AudioSample(0), 256, TIMEOUT, &active),
        Err(StageAudioError::Limit("resident PCM or stage cache"))
    ));
    assert!(provider.calls.is_empty());
    assert_eq!(renderer.cached_stage_count(), 0);
    // Even a fully Hold-gated effect must still be admitted. Two smaller reads
    // each fit the budget and reach that provider; the second catches a leaked
    // reservation on the first provider's deliberate failure.
    for expected_calls in 1..=2 {
        reached_provider(
            renderer
                .prepare_edge_faded(&mut provider, AudioSample(0), 128, TIMEOUT, &active)
                .unwrap_err(),
        );
        assert_eq!(provider.calls.len(), expected_calls);
        assert_eq!(provider.calls.last(), Some(&asset("effect")));
        assert_eq!(renderer.cached_stage_count(), 0);
    }
}

#[test]
fn initial_and_inflight_cancellation_leave_the_same_sound_renderer_reusable() {
    let mut renderer = StageAudio::with_limits(
        held_sound(),
        StageLimits {
            maximum_resident_frames: 640,
            ..Default::default()
        },
    )
    .unwrap();
    let mut provider = SentinelProvider::default();
    let cancelled = AtomicBool::new(true);
    assert!(
        renderer
            .prepare_edge_faded(&mut provider, AudioSample(0), 128, TIMEOUT, &cancelled,)
            .unwrap_err()
            .is_cancelled()
    );
    assert!(provider.calls.is_empty());

    cancelled.store(false, Ordering::Relaxed);
    provider.cancel_on_call = true;
    assert!(
        renderer
            .prepare_edge_faded(&mut provider, AudioSample(0), 128, TIMEOUT, &cancelled,)
            .unwrap_err()
            .is_cancelled()
    );
    assert!(cancelled.load(Ordering::Relaxed));
    assert_eq!(provider.calls, [asset("effect")]);
    assert_eq!(renderer.cached_stage_count(), 0);

    cancelled.store(false, Ordering::Relaxed);
    provider.cancel_on_call = false;
    reached_provider(
        renderer
            .prepare_edge_faded(&mut provider, AudioSample(0), 128, TIMEOUT, &cancelled)
            .unwrap_err(),
    );
    assert_eq!(provider.calls, [asset("effect"), asset("effect")]);
    assert_eq!(renderer.cached_stage_count(), 0);
}
