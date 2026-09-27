use std::collections::BTreeMap;

use deadpan_core::*;
use deadpan_plan::{
    AudioContent, AudioQueryLimits, AudioSignalContent, AudioSignalTape, AudioSignalTapeRun,
    AudioSourceVoiceRecipe, PlanError, RenderPlan, SignalSample,
};

fn ratio(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}

fn span(start: i64, end: i64, rate: u32) -> SourceSpan {
    let time_base = SourceTimeBase::new(1, rate).unwrap();
    SourceSpan::new(
        SourceTimestamp {
            ticks: start,
            time_base,
        },
        SourceTimestamp {
            ticks: end,
            time_base,
        },
    )
    .unwrap()
}

fn document(qualified: bool) -> ProjectDocument {
    let root = NodeId::new("root").unwrap();
    let mut wire = serde_json::to_value(
        ProjectDocument::new(
            ProjectId::new("source-voice").unwrap(),
            RevisionId::new("revision").unwrap(),
            PresentationBasis {
                width: 16,
                height: 16,
                frame_rate: FrameRate::new(48_000, 1).unwrap(),
                color_policy: ColorPolicy::SdrRec709,
            },
            root.clone(),
        )
        .unwrap(),
    )
    .unwrap();
    let hold = NodeId::new("hold").unwrap();
    wire["nodes"] = serde_json::to_value(BTreeMap::from([
        (root, BeatNode::sequence("Root", vec![hold.clone()])),
        (
            hold,
            BeatNode::hold(
                "Hold",
                HoldRecipe {
                    duration: FrameDuration::new(64).unwrap(),
                    picture_context: None,
                    video: HoldVideo::Background,
                    audio: HoldAudio::Silence,
                },
            ),
        ),
    ]))
    .unwrap();
    wire["assets"] = serde_json::to_value(BTreeMap::from([(
        AssetId::new("catalog-only").unwrap(),
        AssetRecord {
            label: "Independent sound".into(),
            content_hash: "a".repeat(64),
            video: None,
            audio: Some(span(0, 100, 48_000)),
            still_image: false,
            frame_count: None,
            source_qualification: qualified
                .then(|| SourceQualificationId::new("b".repeat(64)).unwrap()),
        },
    )]))
    .unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn recipe() -> AudioSourceVoiceRecipe {
    AudioSourceVoiceRecipe {
        source: SourceAudio {
            asset: AssetId::new("catalog-only").unwrap(),
            span: span(10, 42, 48_000),
        },
        mapping: SourceAudioMapping::Placement {
            start: ratio(5, 2),
            frames: ratio(32, 1),
        },
        offset: AudioSample(-1),
    }
}

#[test]
fn catalog_operand_preserves_fractional_phase_without_adding_structure() {
    let doc = document(true);
    let before = doc.clone();
    let plan = RenderPlan::compile(&doc).unwrap();
    assert!(plan.audio_context_assets().is_none());
    let voice = plan.audio_signal().source_voice(recipe()).unwrap();
    let result = voice
        .input_signal()
        .query(
            SignalSample(0)..SignalSample(40),
            AudioQueryLimits::default(),
        )
        .unwrap();
    assert_eq!(
        result
            .spans
            .iter()
            .map(|s| s.samples.clone())
            .collect::<Vec<_>>(),
        vec![
            SignalSample(0)..SignalSample(2),
            SignalSample(2)..SignalSample(34),
            SignalSample(34)..SignalSample(40),
        ]
    );
    let source = &result.spans[1];
    assert_eq!(
        source.source_point(SignalSample(2)).unwrap().ticks,
        ratio(21, 2)
    );
    assert_eq!(
        source.source_point(SignalSample(33)).unwrap().ticks,
        ratio(83, 2)
    );
    let AudioSignalContent::Leaf(AudioContent::Source { support, .. }) = &source.content else {
        panic!("source voice missing");
    };
    assert_eq!(support.start.ticks, ratio(10, 1));
    assert_eq!(support.end.ticks, ratio(42, 1));
    assert!(source.retimes.is_empty());
    assert_eq!(doc, before);
    assert_eq!(doc.nodes().len(), 2);
    assert!(
        doc.nodes()
            .values()
            .all(|node| !matches!(node.kind, NodeKind::Source { .. }))
    );
}

#[test]
fn recipes_require_qualified_catalog_bounds_and_explicit_natural_rate() {
    let plan = RenderPlan::compile(&document(true)).unwrap();
    let owner = plan.audio_signal();
    let mut bad = recipe();
    bad.source.asset = AssetId::new("absent").unwrap();
    assert!(owner.source_voice(bad).is_err());
    for selection in [
        span(-1, 32, 48_000),
        span(0, 101, 48_000),
        span(10, 42, 44_100),
    ] {
        let mut bad = recipe();
        bad.source.span = selection;
        assert!(owner.source_voice(bad).is_err());
    }
    for mapping in [
        SourceAudioMapping::FitBeat,
        SourceAudioMapping::Duration {
            frames: ratio(31, 1),
        },
        SourceAudioMapping::SelectedPlacement {
            start: ExactRatio::ZERO,
            frames: ratio(32, 1),
            selection: ExactFrameRange {
                start: ratio(-1, 1),
                end: ratio(32, 1),
            },
        },
    ] {
        let mut bad = recipe();
        bad.mapping = mapping;
        assert!(owner.source_voice(bad).is_err());
    }
    let legacy = RenderPlan::compile(&document(false)).unwrap();
    assert!(legacy.audio_signal().source_voice(recipe()).is_err());
}

#[test]
fn identity_survives_views_without_admitting_other_plans_or_nested_providers() {
    let plan = RenderPlan::compile(&document(true)).unwrap();
    let other = plan.clone();
    let voice = plan.audio_signal().source_voice(recipe()).unwrap();
    assert_eq!(voice.identity(), voice.clone().identity());
    assert_ne!(
        voice.identity(),
        plan.audio_signal()
            .source_voice(recipe())
            .unwrap()
            .identity()
    );
    assert!(voice.input_signal().source_voice(recipe()).is_err());
    assert!(voice.output_signal().source_voice(recipe()).is_err());
    let support = ratio(0, 1)..ratio(64, 1);
    assert!(
        AudioSignalTape::new(
            &other,
            support.clone(),
            vec![AudioSignalTapeRun::new(
                support.clone(),
                support,
                voice.input_signal()
            ),]
        )
        .is_err()
    );
}

#[test]
fn each_source_interval_spends_work_and_chunk_queries_keep_allocation() {
    let plan = RenderPlan::compile(&document(true)).unwrap();
    let signal = plan
        .audio_signal()
        .source_voice(recipe())
        .unwrap()
        .input_signal();
    let limits = AudioQueryLimits {
        maximum_work: 3,
        maximum_spans: 3,
    };
    let query = signal
        .query(SignalSample(0)..SignalSample(40), limits)
        .unwrap();
    assert_eq!(query.work, 3);
    assert!(matches!(
        signal.query(
            SignalSample(0)..SignalSample(40),
            AudioQueryLimits {
                maximum_work: 2,
                ..limits
            }
        ),
        Err(PlanError::AudioQueryLimit(_))
    ));
    assert!(matches!(
        signal.query(
            SignalSample(0)..SignalSample(40),
            AudioQueryLimits {
                maximum_spans: 2,
                ..limits
            }
        ),
        Err(PlanError::AudioQueryLimit(_))
    ));
    for sample in [33, 4, 12, 2] {
        let chunk = signal
            .query(SignalSample(sample)..SignalSample(sample + 1), limits)
            .unwrap();
        assert_eq!(
            chunk.spans[0].allocated_samples,
            query.spans[1].allocated_samples
        );
        assert_eq!(chunk.spans[0].sampling, query.spans[1].sampling);
        assert_eq!(chunk.spans[0].content, query.spans[1].content);
    }
}
