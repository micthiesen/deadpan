use std::collections::BTreeMap;

use deadpan_core::*;
use deadpan_plan::{AudioContent, AudioQueryLimits, PlanError, RenderPlan, SilenceReason};

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}

fn audio(start: i64, end: i64) -> SourceAudio {
    let time_base = SourceTimeBase::new(1, 48_000).unwrap();
    SourceAudio {
        asset: AssetId::new("original").unwrap(),
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

fn source(length: i64, selection: Option<SourceAudio>, offset: i64) -> BeatNode {
    BeatNode {
        label: "Source".into(),
        framing: None,
        audio_edges: AudioEdgePolicies {
            node_start: AudioEdgePolicy::Hard,
            node_end: AudioEdgePolicy::Hard,
            source_placement_start: AudioEdgePolicy::Hard,
            source_placement_end: AudioEdgePolicy::Hard,
            ..Default::default()
        },
        kind: NodeKind::Source {
            source: SourceNode {
                duration: frames(length),
                video: if selection.is_none() {
                    SourceVideo::Stream {
                        asset: AssetId::new("picture").unwrap(),
                        span: audio(0, length).span,
                    }
                } else {
                    SourceVideo::Blank
                },
                video_mapping: SourceVideoMapping::FitBeat,
                audio: selection,
                audio_mapping: SourceAudioMapping::FitBeat,
                audio_offset: AudioSample(offset),
                link: LinkRelation::Independent,
            },
        },
    }
}

fn hold(length: i64, selected: HoldAudio) -> BeatNode {
    BeatNode::hold(
        "Hold",
        HoldRecipe {
            duration: frames(length),
            video: HoldVideo::Background,
            audio: selected,
        },
    )
}

fn retime(
    child: &str,
    length: i64,
    selected: std::ops::Range<i64>,
    pitch: PitchPolicy,
    purpose: RetimePurpose,
) -> BeatNode {
    BeatNode {
        label: "Retime".into(),
        framing: None,
        audio_edges: Default::default(),
        kind: NodeKind::Retime {
            purpose,
            child: id(child),
            duration: frames(length),
            mapping: FrameRange::new(ProjectFrame(selected.start), ProjectFrame(selected.end))
                .unwrap(),
            pitch,
        },
    }
}

fn document(
    children: &[&str],
    nodes: impl IntoIterator<Item = (&'static str, BeatNode)>,
    overrides: BTreeMap<NodeId, PlayOverrides>,
) -> ProjectDocument {
    let empty = ProjectDocument::new(
        ProjectId::new("context-project").unwrap(),
        RevisionId::new("capture-revision").unwrap(),
        PresentationBasis {
            width: 1920,
            height: 1080,
            frame_rate: FrameRate::new(48_000, 1).unwrap(),
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
        BeatNode::sequence("Root", children.iter().map(|name| id(name)).collect()),
    );
    let mut wire = serde_json::to_value(empty).unwrap();
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    wire["overrides"] = serde_json::to_value(overrides).unwrap();
    wire["assets"] = serde_json::to_value(BTreeMap::from([
        (
            AssetId::new("original").unwrap(),
            AssetRecord {
                label: "Qualified original".into(),
                content_hash: "a".repeat(64),
                video: None,
                audio: Some(audio(0, 256).span),
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

fn compare_audio(original: &RenderPlan, frozen: &RenderPlan, start: i64, end: i64) {
    let range = AudioSample(start)..AudioSample(end);
    let before = original
        .audio(range.clone(), AudioQueryLimits::default())
        .unwrap();
    let after = frozen
        .audio(range.clone(), AudioQueryLimits::default())
        .unwrap();
    assert_eq!(after, before, "audio query {start}..{end}");
    let before = original
        .audio_processing(range.clone(), AudioQueryLimits::default())
        .unwrap();
    let after = frozen
        .audio_processing(range, AudioQueryLimits::default())
        .unwrap();
    assert_eq!(
        serde_json::to_value(after).unwrap(),
        serde_json::to_value(before).unwrap()
    );
}

#[test]
fn restored_context_retains_source_offsets_edges_room_tone_and_preserve_clocks_after_delete() {
    let doc = document(
        &["positive", "negative", "picture-only", "tone", "partition"],
        [
            ("positive", source(8, Some(audio(0, 8)), 2)),
            ("negative", source(8, Some(audio(16, 24)), -2)),
            ("picture-only", source(4, None, 0)),
            (
                "tone",
                hold(
                    3,
                    HoldAudio::RoomTone {
                        source: audio(32, 40),
                    },
                ),
            ),
            ("nested-source", source(8, Some(audio(48, 56)), 0)),
            (
                "preserve",
                retime(
                    "nested-source",
                    12,
                    0..8,
                    PitchPolicy::Preserve,
                    RetimePurpose::Edit,
                ),
            ),
            (
                "partition",
                retime(
                    "preserve",
                    6,
                    3..9,
                    PitchPolicy::FollowSpeed,
                    RetimePurpose::Partition,
                ),
            ),
        ],
        BTreeMap::new(),
    );
    let original = RenderPlan::compile(&doc).unwrap();
    let context = FrozenAudioContext::capture(&doc).unwrap();
    assert_eq!(context.project_id(), doc.project_id());
    assert_eq!(context.revision_id(), doc.revision_id());
    assert_eq!(context.assets().len(), 1);
    assert_eq!(
        context.assets()[&AssetId::new("original").unwrap()],
        doc.assets()[&AssetId::new("original").unwrap()]
    );
    assert_eq!(context.inputs().len(), 4);
    assert!(context.inputs().get(&id("picture-only")).is_none());
    let restored = FrozenAudioContext::from_json(&context.to_json().unwrap()).unwrap();
    assert_eq!(restored, context);

    let deleted = apply(
        &doc,
        &CommandRequest {
            project_id: doc.project_id().clone(),
            expected_revision: doc.revision_id().clone(),
            new_revision: RevisionId::new("after-delete").unwrap(),
            command: Command::Delete {
                node: id("positive"),
            },
        },
    )
    .unwrap()
    .forward
    .apply(&doc)
    .unwrap();
    assert_ne!(deleted.revision_id(), context.revision_id());
    assert!(!deleted.nodes().contains_key(&id("positive")));

    let frozen = RenderPlan::compile_audio_context(&restored).unwrap();
    assert!(matches!(
        frozen.picture(ProjectFrame(0)),
        Err(PlanError::AudioOnlyContext)
    ));
    assert_eq!(
        frozen.audio_duration().unwrap(),
        original.audio_duration().unwrap()
    );
    for (start, end) in [(0, 8), (8, 16), (16, 20), (20, 23), (23, 29), (0, 29)] {
        compare_audio(&original, &frozen, start, end);
    }
    let silent = frozen
        .audio(
            AudioSample(16)..AudioSample(20),
            AudioQueryLimits::default(),
        )
        .unwrap();
    assert!(silent.spans.iter().all(|span| matches!(
        span.content,
        AudioContent::Silence {
            reason: SilenceReason::NoSourceAudio
        }
    )));
    assert!(
        silent
            .spans
            .iter()
            .flat_map(|span| span
                .boundaries
                .start
                .iter()
                .chain(span.boundaries.end.iter()))
            .any(|edge| edge.instance.node == id("picture-only"))
    );
}

#[test]
fn billion_play_context_keeps_sparse_override_and_bounded_last_seek() {
    let plays = 1_000_000_000;
    let repeat = BeatNode {
        label: "Repeat".into(),
        framing: None,
        audio_edges: Default::default(),
        kind: NodeKind::Repeat {
            child: id("ordinary"),
            iterations: IterationOrder::new(RevisionId::new("play-allocation").unwrap(), plays)
                .unwrap(),
            gap: None,
        },
    };
    let override_at = IterationId {
        allocation: RevisionId::new("play-allocation").unwrap(),
        ordinal: plays - 1,
    };
    let overrides = BTreeMap::from([(
        id("repeat"),
        PlayOverrides::try_from(vec![PlayOverride {
            iteration: override_at,
            root: id("last"),
        }])
        .unwrap(),
    )]);
    let doc = document(
        &["repeat"],
        [
            ("ordinary", source(1, Some(audio(0, 1)), 0)),
            ("last", source(1, Some(audio(100, 101)), 0)),
            ("repeat", repeat),
        ],
        overrides,
    );
    let original = RenderPlan::compile(&doc).unwrap();
    let restored = FrozenAudioContext::from_json(
        &FrozenAudioContext::capture(&doc)
            .unwrap()
            .to_json()
            .unwrap(),
    )
    .unwrap();
    let frozen = RenderPlan::compile_audio_context(&restored).unwrap();
    assert_eq!(frozen.metadata().storage.iteration_run_entries, 1);
    assert_eq!(frozen.metadata().storage.sparse_override_entries, 1);
    assert_eq!(frozen.metadata().storage.referenced_plays, u64::from(plays));
    let range = AudioSample(i64::from(plays) - 1)..AudioSample(i64::from(plays));
    let limits = AudioQueryLimits {
        maximum_spans: 1,
        maximum_work: 40,
    };
    let before = original.audio(range.clone(), limits).unwrap();
    let after = frozen.audio(range, limits).unwrap();
    assert_eq!(after, before);
    assert_eq!(
        after.spans[0].instance.repeats[0].iteration.ordinal,
        plays - 1
    );
    assert!(
        matches!(&after.spans[0].content, AudioContent::Source { source, .. } if source.span == audio(100, 101).span)
    );
    assert!(matches!(
        frozen.picture(ProjectFrame(i64::from(plays) - 1)),
        Err(PlanError::AudioOnlyContext)
    ));
}
