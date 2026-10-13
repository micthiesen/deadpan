use super::*;
use crate::{AudioDefinitionSelector, Picture, SignalSample};

fn ratio(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}

fn extent_document() -> ProjectDocument {
    let original = document();
    let mut left = original.nodes()[&id("source")].clone();
    let mut right = original.nodes()[&id("alternate")].clone();
    for node in [&mut left, &mut right] {
        let NodeKind::Source { source } = &mut node.kind else {
            unreachable!()
        };
        source.duration = frames(2);
    }
    let retime = |child| {
        let mut node = hold(1);
        node.kind = NodeKind::Retime {
            child: id(child),
            duration: frames(1),
            mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(2)).unwrap(),
            pitch: PitchPolicy::FollowSpeed,
            purpose: RetimePurpose::Edit,
        };
        node.framing = Some(Framing {
            clock: FramingClock::OwnerOutput,
            value: FramingValue::Envelope {
                envelope: FramingEnvelope {
                    initial: FramingPose::identity(),
                    segments: vec![FramingSegment {
                        end: ExactRatio::ONE,
                        pose: FramingPose {
                            scale: ExactRatio::integer(2),
                            ..FramingPose::identity()
                        },
                        curve: FramingCurve::Linear,
                    }],
                },
            },
        });
        node
    };
    let nodes = BTreeMap::from([
        (
            id("root"),
            BeatNode::sequence("Root", vec![id("first"), id("last")]),
        ),
        (id("first"), retime("left")),
        (id("last"), retime("right")),
        (id("left"), left),
        (id("right"), right),
    ]);
    let mut wire = serde_json::to_value(original).unwrap();
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    wire["overrides"] = serde_json::json!({});
    wire["gap_overrides"] = serde_json::json!({});
    wire["derived_clocks"] = serde_json::to_value(BTreeMap::from([
        (
            id("first"),
            DerivedRetimeClock {
                extent: ratio(1, 2),
                child_start: ExactRatio::ZERO,
                child_end: ratio(2, 1),
            },
        ),
        (
            id("last"),
            DerivedRetimeClock {
                extent: ratio(3, 2),
                child_start: ExactRatio::ZERO,
                child_end: ratio(2, 1),
            },
        ),
    ]))
    .unwrap();
    let authored = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let reopened = ProjectDocument::from_json(&authored.to_compact_json().unwrap()).unwrap();
    assert_eq!(reopened, authored);
    reopened
}

fn extent_plan() -> RenderPlan {
    RenderPlan::compile(&extent_document()).unwrap()
}

#[test]
fn persisted_repeat_uses_exact_prefixes_and_actual_project_duration() {
    let mut wire = serde_json::to_value(extent_document()).unwrap();
    let mut repeated = hold(1);
    let iterations = IterationOrder::new(RevisionId::new("plays").unwrap(), 3).unwrap();
    repeated.kind = NodeKind::Repeat {
        child: id("first"),
        iterations: iterations.clone(),
        gap: None,
        escalation: None,
    };
    wire["nodes"]["repeat"] = serde_json::to_value(repeated).unwrap();
    wire["nodes"]["root"] =
        serde_json::to_value(BeatNode::sequence("Root", vec![id("repeat"), id("last")])).unwrap();
    let doc = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let plan = RenderPlan::compile(&doc).unwrap();
    assert_eq!(
        doc.recipe_durations_shared().unwrap()[doc.root()],
        frames(4)
    );
    assert_eq!(plan.metadata().duration, frames(3));
    assert_eq!(plan.node_exact_duration(&id("repeat")), Some(ratio(3, 2)));
    for play in 0..3 {
        let picture = plan
            .definition_picture(
                &id("root"),
                ratio(1 + 2 * i128::from(play), 4),
                Default::default(),
            )
            .unwrap();
        let Picture::Source { point, .. } = picture.picture else {
            panic!("source")
        };
        assert_eq!(point.ticks, ratio(3200, 1));
        assert_eq!(
            picture.instance.repeats[0].iteration,
            iterations.at(play).unwrap()
        );
    }
    let spans = plan
        .audio(
            AudioSample(0)..plan.audio_duration().unwrap(),
            Default::default(),
        )
        .unwrap()
        .spans;
    assert_eq!(
        spans
            .iter()
            .map(|span| span.allocated_samples.clone())
            .collect::<Vec<_>>(),
        [
            AudioSample(0)..AudioSample(801),
            AudioSample(801)..AudioSample(1602),
            AudioSample(1602)..AudioSample(2402),
            AudioSample(2402)..AudioSample(4805)
        ]
    );
}

#[test]
fn fractional_allocations_keep_picture_mapping_and_exact_framing_progress() {
    let plan = extent_plan();
    assert_eq!(plan.node_exact_duration(&id("last")), Some(ratio(3, 2)));
    assert_eq!(plan.node_duration(&id("last")), None);
    assert_eq!(plan.node_duration(&id("root")), Some(frames(2)));
    assert_eq!(
        plan.single_occurrence_exact_range(&id("last")).unwrap(),
        Some(ExactFrameRange::new(ratio(1, 2), ratio(2, 1)).unwrap())
    );
    assert_eq!(plan.single_occurrence_range(&id("last")), None);
    // Each midpoint lands at the midpoint of its independently retained child
    // and its normalized camera path, despite different allocated extents.
    for (at, owner, ticks) in [(ratio(1, 4), "first", 3200), (ratio(5, 4), "last", 10400)] {
        let picture = plan
            .definition_picture(&id("root"), at, Default::default())
            .unwrap();
        let Picture::Source { point, .. } = picture.picture else {
            panic!("source")
        };
        assert_eq!(point.ticks, ExactRatio::integer(ticks));
        let layer = picture
            .framing
            .iter()
            .find(|layer| layer.instance.node == id(owner))
            .unwrap();
        assert_eq!(layer.duration, frames(1));
        assert_eq!(
            layer.effective_duration,
            plan.node_exact_duration(&id(owner)).unwrap()
        );
        assert_eq!(layer.pose.unwrap().scale, ratio(3, 2));
    }
    let tail = plan
        .definition_picture(&id("last"), ratio(5, 4), Default::default())
        .unwrap();
    assert_eq!(tail.local_position, ratio(5, 3));
    assert!(matches!(
        plan.definition_picture(&id("last"), ratio(3, 2), Default::default()),
        Err(PlanError::DefinitionPictureOutOfRange { .. })
    ));
    for frame in 0..2 {
        let picture = plan.picture(ProjectFrame(frame)).unwrap();
        assert_eq!(picture.instance.node, id("right"));
    }
}

#[test]
fn fractional_allocations_keep_root_rounding_point_grids_and_owner_windows() {
    let plan = extent_plan();
    let end = plan.audio_duration().unwrap();
    assert_eq!(end, AudioSample(3203));
    let query = plan.audio(AudioSample(0)..end, Default::default()).unwrap();
    assert_eq!(query.spans.len(), 2);
    for (span, owner, start, end, extent, center, ticks) in [
        (
            &query.spans[0],
            "left",
            0,
            801,
            ExactRatio::ZERO..ratio(1, 2),
            ratio(1, 4),
            3200,
        ),
        (
            &query.spans[1],
            "right",
            801,
            3203,
            ratio(1, 2)..ratio(2, 1),
            ratio(5, 4),
            10400,
        ),
    ] {
        assert_eq!(span.instance.node, id(owner));
        assert_eq!(span.allocated_samples, AudioSample(start)..AudioSample(end));
        assert_eq!(span.project_extent, extent);
        assert_eq!(
            span.source_point_at_project_frame(center).unwrap().ticks,
            ExactRatio::integer(ticks)
        );
    }
    let definition = plan
        .audio_definition(AudioDefinitionSelector::Node { node: id("last") })
        .unwrap();
    assert_eq!(definition.duration(), frames(1));
    assert_eq!(definition.exact_duration(), ratio(3, 2));
    let signal = definition.signal();
    let count = signal.sample_count().unwrap();
    assert_eq!(count, SignalSample(2403));
    for width in [1, 31, 1601] {
        let mut start = 0;
        while start < count.0 {
            let selected = SignalSample(start)..SignalSample((start + width).min(count.0));
            signal.query(selected.clone(), Default::default()).unwrap();
            definition.owners(selected, Default::default()).unwrap();
            start += width;
        }
    }
    let occurrences = plan
        .audio_owner_occurrences(&id("last"), AudioSample(0)..end, Default::default())
        .unwrap();
    assert_eq!(occurrences.occurrences().len(), 1);
    assert_eq!(
        occurrences.occurrences()[0].visible_root_frames,
        ratio(1, 2)..ratio(2, 1)
    );
}

#[test]
fn changed_preserve_allocation_cannot_resize_its_intrinsic_operand() {
    let mut plan = extent_plan();
    let node = plan.by_id[&id("last")];
    let CompiledKind::Retime { pitch, .. } = &mut plan.nodes[node].kind else {
        unreachable!()
    };
    *pitch = PitchPolicy::Preserve;
    assert!(matches!(
        plan.audio_processing(AudioSample(802)..AudioSample(803), Default::default()),
        Err(PlanError::InvalidPlan(
            "derived Preserve allocation requires an owned processing route"
        ))
    ));
}

#[test]
fn independent_voice_validates_against_exact_owner_extent_without_fitting() {
    let mut plan = extent_plan();
    let asset = AssetId::new("original").unwrap();
    plan.audio_assets
        .get_mut(&asset)
        .unwrap()
        .source_qualification = Some(SourceQualificationId::new("a".repeat(64)).unwrap());
    let rate = plan.metadata.presentation_basis.frame_rate;
    let voice = |ticks| {
        let time_base = SourceTimeBase::new(1, 48_000).unwrap();
        let span = SourceSpan::new(
            SourceTimestamp {
                ticks: 0,
                time_base,
            },
            SourceTimestamp { ticks, time_base },
        )
        .unwrap();
        crate::AudioSourceVoiceRecipe {
            source: SourceAudio {
                asset: asset.clone(),
                span,
            },
            mapping: SourceAudioMapping::natural_rate(span, rate).unwrap(),
            offset: AudioSample(0),
        }
    };
    let owner = InstancePath {
        node: id("last"),
        repeats: Vec::new(),
    };
    // 1,800 source samples exceed one frame but fit the actual 3/2-frame
    // owner. Its original rate and independent sample count stay unchanged.
    let occurrence = plan
        .source_voice_occurrence(owner.clone(), voice(1800), Default::default())
        .unwrap();
    assert_eq!(occurrence.extent(), ratio(1, 2)..ratio(2, 1));
    assert_eq!(
        occurrence.audible_samples().unwrap(),
        AudioSample(801)..AudioSample(2601)
    );
    assert!(matches!(
        plan.source_voice_occurrence(owner.clone(), voice(3000), Default::default()),
        Err(PlanError::InvalidAudioSourceOccurrence(
            "source occurrence selection exceeds its owner"
        ))
    ));
    let mut fitted = voice(1800);
    fitted.mapping = SourceAudioMapping::FitBeat;
    assert!(matches!(
        plan.source_voice_occurrence(owner, fitted, Default::default()),
        Err(PlanError::InvalidPlan(
            "source voice requires an explicit natural-rate mapping"
        ))
    ));
}
