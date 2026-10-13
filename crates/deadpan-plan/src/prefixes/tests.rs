use std::collections::BTreeMap;

use deadpan_core::*;

use super::*;

fn id(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}
fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn hold(value: i64) -> BeatNode {
    BeatNode::hold(
        "Hold",
        HoldRecipe {
            duration: frames(value),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
            picture_context: None,
        },
    )
}
fn document() -> ProjectDocument {
    let allocation = RevisionId::new("plays").unwrap();
    let asset = AssetId::new("original").unwrap();
    let time_base = SourceTimeBase::new(1, 48_000).unwrap();
    let span = |start, end| {
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
    };
    let source = |duration, start, end| {
        let mut node = hold(duration);
        node.kind = NodeKind::Source {
            source: SourceNode {
                duration: frames(duration),
                video: SourceVideo::Stream {
                    asset: asset.clone(),
                    span: span(start, end),
                },
                video_mapping: SourceVideoMapping::FitBeat,
                audio: Some(SourceAudio {
                    asset: asset.clone(),
                    span: span(start, end),
                }),
                audio_mapping: SourceAudioMapping::FitBeat,
                audio_offset: AudioSample(0),
                link: LinkRelation::Linked,
                edit_window: None,
            },
        };
        node
    };
    let order = IterationOrder::new(allocation, 3).unwrap();
    let mut repeat = hold(1);
    let NodeKind::Hold { recipe } = hold(1).kind else {
        unreachable!()
    };
    repeat.kind = NodeKind::Repeat {
        child: id("body"),
        iterations: order.clone(),
        gap: Some(recipe),
        escalation: None,
    };
    let mut retime = hold(11);
    retime.kind = NodeKind::Retime {
        child: id("repeat"),
        duration: frames(11),
        mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(16)).unwrap(),
        pitch: PitchPolicy::Preserve,
        purpose: RetimePurpose::Edit,
    };
    let nodes = BTreeMap::from([
        (
            id("root"),
            BeatNode::sequence("Root", vec![id("lead"), id("retime"), id("tail")]),
        ),
        (id("lead"), hold(2)),
        (id("retime"), retime),
        (id("repeat"), repeat),
        (
            id("body"),
            BeatNode::sequence("Body", vec![id("source"), id("pause")]),
        ),
        (id("source"), source(3, 0, 6400)),
        (id("pause"), hold(1)),
        (id("alternate"), source(5, 6400, 14400)),
        (id("gap"), hold(2)),
        (id("tail"), BeatNode::sequence("Tail", vec![id("last")])),
        (id("last"), hold(2)),
    ]);
    let empty = ProjectDocument::new(
        ProjectId::new("prefixes").unwrap(),
        RevisionId::new("current").unwrap(),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut wire = serde_json::to_value(empty).unwrap();
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    wire["assets"] = serde_json::to_value(BTreeMap::from([(
        asset,
        AssetRecord {
            label: "Original".into(),
            content_hash: "a".repeat(64),
            source_qualification: None,
            video: Some(span(0, 48000)),
            audio: Some(span(0, 48000)),
            still_image: false,
            frame_count: Some(frames(30)),
        },
    )]))
    .unwrap();
    wire["overrides"] = serde_json::to_value(BTreeMap::from([(
        id("repeat"),
        PlayOverrides::try_from(vec![PlayOverride {
            iteration: order.at(1).unwrap(),
            root: id("alternate"),
        }])
        .unwrap(),
    )]))
    .unwrap();
    wire["gap_overrides"] = serde_json::to_value(BTreeMap::from([(
        id("repeat"),
        PlayOverrides::try_from(vec![PlayOverride {
            iteration: order.at(0).unwrap(),
            root: id("gap"),
        }])
        .unwrap(),
    )]))
    .unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn assert_json(a: &impl Serialize, b: &impl Serialize) {
    assert_eq!(
        serde_json::to_value(a).unwrap(),
        serde_json::to_value(b).unwrap()
    );
}

#[test]
fn exact_and_integer_repeat_storage_agree_through_production_walkers() {
    let document = document();
    let integer = RenderPlan::compile(&document).unwrap();
    let mut exact = integer.clone();
    let durations = document
        .durations()
        .unwrap()
        .into_iter()
        .map(|(id, length)| (id, ExactRatio::integer(length.frames())))
        .collect();
    for (id, node) in document.nodes() {
        let NodeKind::Repeat {
            child,
            iterations,
            gap,
            ..
        } = &node.kind
        else {
            continue;
        };
        let CompiledKind::Repeat { layout, .. } = &mut exact.nodes[exact.by_id[id]].kind else {
            unreachable!()
        };
        *layout = ExactRepeatLayout::compile_with_gap_overrides(
            iterations,
            child,
            document.overrides().get(id),
            gap.as_ref().map_or(ExactRatio::ZERO, |gap| {
                ExactRatio::integer(gap.duration.frames())
            }),
            document.gap_overrides().get(id),
            &durations,
        )
        .unwrap();
    }
    for frame in 0..integer.duration().frames() {
        assert_eq!(
            integer.picture(ProjectFrame(frame)).unwrap(),
            exact.picture(ProjectFrame(frame)).unwrap()
        );
    }
    let end = integer.audio_duration().unwrap();
    // Cold, differently partitioned queries cover rounded boundaries, explicit
    // and implicit gaps, the retained Preserve stage and fractional local time.
    for width in [1, 31, 1601] {
        let mut start = 0;
        while start < end.0 {
            let samples = AudioSample(start)..AudioSample((start + width).min(end.0));
            assert_json(
                &integer.audio(samples.clone(), Default::default()).unwrap(),
                &exact.audio(samples.clone(), Default::default()).unwrap(),
            );
            assert_json(
                &integer
                    .audio_processing(samples.clone(), Default::default())
                    .unwrap(),
                &exact.audio_processing(samples, Default::default()).unwrap(),
            );
            start += width;
        }
    }
    // Root processing stops at Preserve. Query the owned definitions directly
    // as well so both Repeat storage paths reach the inner PointCeil walker.
    for name in ["repeat", "body"] {
        let selector = crate::AudioDefinitionSelector::Node { node: id(name) };
        let a = integer.audio_definition(selector.clone()).unwrap().signal();
        let b = exact.audio_definition(selector).unwrap().signal();
        let end = a.sample_count().unwrap();
        assert_eq!(end, b.sample_count().unwrap());
        for width in [1, 31, 1601] {
            let mut start = 0;
            while start < end.0 {
                let samples =
                    crate::SignalSample(start)..crate::SignalSample((start + width).min(end.0));
                assert_json(
                    &a.query(samples.clone(), Default::default()).unwrap(),
                    &b.query(samples.clone(), Default::default()).unwrap(),
                );
                assert_json(
                    &a.query_flattened(samples.clone(), Default::default())
                        .unwrap(),
                    &b.query_flattened(samples, Default::default()).unwrap(),
                );
                start += width;
            }
        }
    }
    for id in document.nodes().keys() {
        let a = integer
            .audio_owner_occurrences(id, AudioSample(0)..end, Default::default())
            .unwrap();
        let b = exact
            .audio_owner_occurrences(id, AudioSample(0)..end, Default::default())
            .unwrap();
        assert_eq!(a.occurrences(), b.occurrences());
        assert_eq!(a.work(), b.work());
        assert_eq!(
            integer.single_occurrence_range(id),
            exact.single_occurrence_range(id)
        );
        assert_eq!(
            integer.single_occurrence_exact_range(id).unwrap(),
            exact.single_occurrence_exact_range(id).unwrap()
        );
    }
    let targets = integer.authored_hold_targets(Default::default()).unwrap();
    for target in targets {
        assert_json(
            &integer
                .scoped_hold_boundaries(&target, Default::default())
                .unwrap(),
            &exact
                .scoped_hold_boundaries(&target, Default::default())
                .unwrap(),
        );
    }
}

#[test]
fn exact_occurrence_query_keeps_fractional_offsets_and_reports_arithmetic_failure() {
    let mut plan = RenderPlan::compile(&document()).unwrap();
    assert!(
        plan.single_occurrence_exact_range(&id("source"))
            .unwrap()
            .is_none()
    );
    assert!(
        plan.single_occurrence_exact_range(&id("missing"))
            .unwrap()
            .is_none()
    );
    let tail = plan.by_id[&id("tail")];
    let CompiledKind::Sequence { entries } = &mut plan.nodes[plan.root].kind else {
        unreachable!()
    };
    // Inject a derived prefix at the compiled-index boundary. This qualifies
    // the coordinate query only, not a persisted fractional document or render.
    entries
        .iter_mut()
        .find(|entry| entry.child == tail)
        .unwrap()
        .start = ExactRatio::new(27, 2).unwrap();
    let range = plan
        .single_occurrence_exact_range(&id("last"))
        .unwrap()
        .unwrap();
    assert_eq!(range.start, ExactRatio::new(27, 2).unwrap());
    assert_eq!(range.end, ExactRatio::new(31, 2).unwrap());
    assert!(plan.single_occurrence_range(&id("last")).is_none());
    let CompiledKind::Sequence { entries } = &mut plan.nodes[plan.root].kind else {
        unreachable!()
    };
    entries
        .iter_mut()
        .find(|entry| entry.child == tail)
        .unwrap()
        .start = ExactRatio::new(i128::MAX, 1).unwrap();
    assert!(matches!(
        plan.single_occurrence_exact_range(&id("last")),
        Err(PlanError::Time(TimeError::Overflow))
    ));
}
