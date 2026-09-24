use std::collections::BTreeMap;

use deadpan_core::*;
use deadpan_plan::{
    AudioBoundaryRule, AudioContent, AudioQueryLimits, AudioReferencePlan, AudioSignalContent,
    PlanError, ReferenceAudioContent, ReferenceAudioSpan, ReferenceProcessingKind, ReferenceSample,
    RenderPlan, SignalSample, SilenceReason,
};

fn id(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}
fn duration(frames: i64) -> FrameDuration {
    FrameDuration::new(frames).unwrap()
}
fn ratio(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}
fn samples(start: i64, end: i64) -> std::ops::Range<ReferenceSample> {
    ReferenceSample(start)..ReferenceSample(end)
}
fn instance(name: &str) -> InstancePath {
    InstancePath {
        node: id(name),
        repeats: Vec::new(),
    }
}

fn audio() -> SourceAudio {
    let time_base = SourceTimeBase::new(1, 48_000).unwrap();
    SourceAudio {
        asset: AssetId::new("original").unwrap(),
        span: SourceSpan::new(
            SourceTimestamp {
                ticks: 0,
                time_base,
            },
            SourceTimestamp {
                ticks: 48_000,
                time_base,
            },
        )
        .unwrap(),
    }
}

fn source(frames: i64) -> BeatNode {
    BeatNode {
        label: "Original".into(),
        framing: None,
        audio_edges: Default::default(),
        kind: NodeKind::Source {
            source: SourceNode {
                duration: duration(frames),
                video: SourceVideo::Blank,
                video_mapping: SourceVideoMapping::FitBeat,
                audio: Some(audio()),
                audio_mapping: SourceAudioMapping::FitBeat,
                audio_offset: AudioSample(0),
                link: LinkRelation::Independent,
            },
        },
    }
}

fn hold(frames: i64, audio: HoldAudio) -> BeatNode {
    BeatNode::hold(
        "Policy",
        HoldRecipe {
            duration: duration(frames),
            video: HoldVideo::Background,
            audio,
        },
    )
}

fn retime(child: &str, frames: i64, start: i64, end: i64, pitch: PitchPolicy) -> BeatNode {
    BeatNode {
        label: "Retime".into(),
        framing: None,
        audio_edges: Default::default(),
        kind: NodeKind::Retime {
            child: id(child),
            duration: duration(frames),
            mapping: FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap(),
            pitch,
            purpose: RetimePurpose::Edit,
        },
    }
}

fn document(rate: FrameRate, children: &[&str], nodes: Vec<(&str, BeatNode)>) -> ProjectDocument {
    let empty = ProjectDocument::new(
        ProjectId::new("reference-project").unwrap(),
        RevisionId::new("initial").unwrap(),
        PresentationBasis {
            width: 640,
            height: 360,
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
        BeatNode::sequence("Edit", children.iter().map(|name| id(name)).collect()),
    );
    let mut wire = serde_json::to_value(empty).unwrap();
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    wire["assets"] = serde_json::to_value(BTreeMap::from([(
        audio().asset,
        AssetRecord {
            label: "Original".into(),
            content_hash: "a".repeat(64),
            video: None,
            audio: Some(audio().span),
            still_image: true,
            frame_count: None,
            source_qualification: None,
        },
    )]))
    .unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn compile(document: &ProjectDocument) -> AudioReferencePlan {
    AudioReferencePlan::compile(&FrozenAudioLayout::capture(document).unwrap()).unwrap()
}

fn policy(content: &AudioContent) -> ReferenceAudioContent {
    match content {
        AudioContent::Source { .. } => ReferenceAudioContent::Source,
        AudioContent::RoomTone { .. } => ReferenceAudioContent::RoomTone,
        AudioContent::Tail { maximum, .. } => ReferenceAudioContent::Tail { maximum: *maximum },
        AudioContent::Silence { reason } => ReferenceAudioContent::Silence { reason: *reason },
    }
}

fn at(spans: &[ReferenceAudioSpan], sample: i64) -> ReferenceAudioContent {
    spans
        .iter()
        .find(|span| span.samples.contains(&ReferenceSample(sample)))
        .unwrap()
        .content
}

#[test]
fn ntsc_old_root_and_preserve_output_keep_both_rounding_phases() {
    for silent_start in [4, 6] {
        let doc = document(
            FrameRate::new(30_000, 1001).unwrap(),
            &["preserve"],
            vec![
                (
                    "preserve",
                    retime("sequence", 6, 0, 12, PitchPolicy::Preserve),
                ),
                (
                    "sequence",
                    BeatNode::sequence("Context", vec![id("a"), id("quiet"), id("b")]),
                ),
                ("a", source(silent_start)),
                ("quiet", hold(2, HoldAudio::Silence)),
                ("b", source(10 - silent_start)),
            ],
        );
        let frozen = compile(&doc);
        let live = RenderPlan::compile(&doc).unwrap();
        let root = frozen.root_clock();
        let output = frozen.preserve_output_clock(&instance("preserve")).unwrap();
        assert_eq!(root.grid().boundary_rule(), AudioBoundaryRule::RoundEven);
        assert_eq!(output.grid().boundary_rule(), AudioBoundaryRule::PointCeil);
        let root_query = root.query(samples(3000, 6800), Default::default()).unwrap();
        let live_query = live
            .audio(AudioSample(3000)..AudioSample(6800), Default::default())
            .unwrap();
        for (old, current) in root_query.spans.iter().zip(&live_query.spans) {
            assert_eq!(old.content, policy(&current.content));
            assert_eq!(
                old.allocated_samples,
                samples(
                    current.allocated_samples.start.0,
                    current.allocated_samples.end.0
                )
            );
            assert_eq!(old.extent, current.project_extent);
            assert_eq!(old.instance, current.instance);
        }
        assert_eq!(root_query.spans.len(), live_query.spans.len());
        let point_query = output
            .query(samples(3000, 6800), Default::default())
            .unwrap();
        let boundary = if silent_start == 4 { 3203 } else { 6406 };
        let silent = ReferenceAudioContent::Silence {
            reason: SilenceReason::SilentHold,
        };
        if silent_start == 4 {
            assert_eq!(at(&root_query.spans, boundary), silent);
            assert_eq!(
                at(&point_query.spans, boundary),
                ReferenceAudioContent::Source
            );
        } else {
            assert_eq!(
                at(&root_query.spans, boundary),
                ReferenceAudioContent::Source
            );
            assert_eq!(at(&point_query.spans, boundary), silent);
        }
        // Query order and cropping cannot alter full old allocated boundaries.
        for range in [
            samples(6400, 6800),
            samples(3000, 3204),
            samples(3204, 6400),
        ] {
            for partial in root.query(range.clone(), Default::default()).unwrap().spans {
                let full = root_query
                    .spans
                    .iter()
                    .find(|full| full.allocated_samples == partial.allocated_samples)
                    .unwrap();
                assert_eq!(partial.content, full.content);
                assert_eq!(partial.extent, full.extent);
                assert_eq!(partial.instance, full.instance);
            }
        }
    }
}

#[test]
fn preserve_input_uses_selected_origin_and_flattens_nested_policies() {
    let doc = document(
        FrameRate::new(30_000, 1001).unwrap(),
        &["outer"],
        vec![
            ("outer", retime("inner", 6, 1, 13, PitchPolicy::Preserve)),
            ("inner", retime("sequence", 14, 0, 7, PitchPolicy::Preserve)),
            (
                "sequence",
                BeatNode::sequence("Context", vec![id("a"), id("quiet"), id("b")]),
            ),
            ("a", source(2)),
            ("quiet", hold(1, HoldAudio::Silence)),
            ("b", source(4)),
        ],
    );
    let frozen = compile(&doc);
    let live = RenderPlan::compile(&doc).unwrap();
    let clock = frozen.preserve_input_clock(&instance("outer")).unwrap();
    assert_eq!(clock.grid().frame_origin(), ExactRatio::ONE);
    assert_eq!(
        clock.grid().at(ReferenceSample(0)).unwrap(),
        ExactRatio::ONE
    );
    assert_eq!(clock.support(), ExactRatio::ONE..ExactRatio::integer(13));
    let query = clock
        .query(
            samples(0, clock.sample_count().unwrap().0),
            Default::default(),
        )
        .unwrap();
    let processing = live
        .audio_processing(AudioSample(0)..AudioSample(1), Default::default())
        .unwrap();
    let AudioSignalContent::Stage(stage) = &processing.spans[0].content else {
        panic!("outer Preserve")
    };
    let expected = stage
        .input_signal()
        .query_flattened(
            SignalSample(0)..SignalSample(clock.sample_count().unwrap().0),
            Default::default(),
        )
        .unwrap();
    assert_eq!(query.spans.len(), expected.spans.len());
    for (old, current) in query.spans.iter().zip(expected.spans) {
        let AudioSignalContent::Leaf(content) = current.content else {
            panic!("flattened policy")
        };
        assert_eq!(old.content, policy(&content));
        assert_eq!(
            old.allocated_samples,
            samples(
                current.allocated_samples.start.0,
                current.allocated_samples.end.0
            )
        );
        assert_eq!(old.extent, current.signal_extent);
    }
}

#[test]
fn tiny_zero_allocated_leaves_and_source_placement_remain_explicit() {
    let tiny = document(
        FrameRate::new(48_000, 1).unwrap(),
        &["preserve"],
        vec![
            (
                "preserve",
                retime("sequence", 1, 0, 3, PitchPolicy::Preserve),
            ),
            (
                "sequence",
                BeatNode::sequence("Tiny", vec![id("a"), id("quiet"), id("tone")]),
            ),
            ("a", source(1)),
            ("quiet", hold(1, HoldAudio::Silence)),
            ("tone", hold(1, HoldAudio::RoomTone { source: audio() })),
        ],
    );
    let frozen = compile(&tiny);
    let root = frozen
        .root_clock()
        .query(samples(0, 1), Default::default())
        .unwrap();
    assert_eq!(root.spans.len(), 1);
    assert_eq!(root.spans[0].instance.node, id("quiet"));
    assert_eq!(root.spans[0].extent, ratio(1, 3)..ratio(2, 3));
    let point = frozen
        .preserve_output_clock(&instance("preserve"))
        .unwrap()
        .query(samples(0, 1), Default::default())
        .unwrap();
    assert_eq!(point.spans[0].instance.node, id("a"));

    let mut placed = source(10);
    let NodeKind::Source { source } = &mut placed.kind else {
        unreachable!()
    };
    source.audio_mapping = SourceAudioMapping::Placement {
        start: ExactRatio::integer(2),
        frames: ExactRatio::integer(3),
    };
    let doc = document(
        FrameRate::new(48_000, 1).unwrap(),
        &["placed", "tail"],
        vec![
            ("placed", placed),
            (
                "tail",
                hold(
                    2,
                    HoldAudio::Tail {
                        source: audio(),
                        maximum: duration(2),
                    },
                ),
            ),
        ],
    );
    let plan = compile(&doc);
    let query = plan
        .root_clock()
        .query(samples(0, 12), Default::default())
        .unwrap();
    assert_eq!(
        query
            .spans
            .iter()
            .map(|span| (span.allocated_samples.clone(), span.content))
            .collect::<Vec<_>>(),
        vec![
            (
                samples(0, 2),
                ReferenceAudioContent::Silence {
                    reason: SilenceReason::OutsideSourcePlacement
                }
            ),
            (samples(2, 5), ReferenceAudioContent::Source),
            (
                samples(5, 10),
                ReferenceAudioContent::Silence {
                    reason: SilenceReason::OutsideSourcePlacement
                }
            ),
            (
                samples(10, 12),
                ReferenceAudioContent::Tail {
                    maximum: duration(2)
                }
            ),
        ]
    );
    let before = plan
        .root_clock()
        .processing_domain_at(ReferenceSample(1), Default::default())
        .unwrap();
    let audible = plan
        .root_clock()
        .processing_domain_at(ReferenceSample(2), Default::default())
        .unwrap();
    let after = plan
        .root_clock()
        .processing_domain_at(ReferenceSample(5), Default::default())
        .unwrap();
    assert_eq!(before.instance(), audible.instance());
    assert_eq!(before.kind(), after.kind());
    assert_eq!(before.meaningful_samples(), samples(0, 2));
    assert_eq!(audible.meaningful_samples(), samples(2, 5));
    assert_eq!(after.meaningful_samples(), samples(5, 10));
    assert!(!before.same_domain(&audible));
    assert!(!before.same_domain(&after));
}

fn edit(doc: &ProjectDocument, revision: &str, command: Command) -> ProjectDocument {
    apply(
        doc,
        &CommandRequest {
            project_id: doc.project_id().clone(),
            expected_revision: doc.revision_id().clone(),
            new_revision: RevisionId::new(revision).unwrap(),
            command,
        },
    )
    .unwrap()
    .forward
    .apply(doc)
    .unwrap()
}

#[test]
fn ntsc_resume_keeps_later_domains_first_sample_and_composes_active_phase() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let doc = document(rate, &["a", "b"], vec![("a", source(2)), ("b", source(2))]);
    let plan = compile(&doc);
    let clock = plan.root_clock();
    let a = clock
        .processing_domain_at(ReferenceSample(1602), Default::default())
        .unwrap();
    let b = clock
        .processing_domain_at(ReferenceSample(3203), Default::default())
        .unwrap();
    assert_eq!(a.instance(), &instance("a"));
    assert_eq!(a.meaningful_samples(), samples(0, 3203));
    assert_eq!(b.instance(), &instance("b"));
    assert_eq!(b.meaningful_samples(), samples(3203, 6406));
    assert!(!a.same_domain(&b));

    // Insert one frame at frame one. Round absolute endpoints once: the cut
    // moves 1602 -> 3203, but the next genuine domain moves 3203 -> 4805.
    let resumed_a = a
        .place_root(AudioSample(0))
        .unwrap()
        .resume(AudioSample(1602), AudioSample(3203))
        .unwrap();
    let placed_b = b.place_root(AudioSample(4805)).unwrap();
    assert_eq!(
        resumed_a.reference_position(AudioSample(3203)).unwrap(),
        ratio(1602, 1)
    );
    assert_eq!(
        placed_b.reference_position(AudioSample(4805)).unwrap(),
        ratio(3203, 1)
    );
    // A blanket shift would lose B's first sample. A's continued map is
    // intentionally unclamped, so the discrepancy is observable here.
    assert_eq!(
        resumed_a.reference_position(AudioSample(4805)).unwrap(),
        ratio(3204, 1)
    );
    assert!(resumed_a.domain().same_domain(&a));
    assert!(placed_b.domain().same_domain(&b));

    let long = document(rate, &["a"], vec![("a", source(8))]);
    let long_plan = compile(&long);
    let long_a = long_plan
        .root_clock()
        .processing_domain_at(ReferenceSample(1602), Default::default())
        .unwrap();
    let first = long_a
        .place_root(AudioSample(0))
        .unwrap()
        .resume(AudioSample(1602), AudioSample(3203))
        .unwrap();
    let second = first.resume(AudioSample(4805), AudioSample(6406)).unwrap();
    assert_eq!(second.output_anchor(), AudioSample(6406));
    assert_eq!(second.reference_at_anchor(), ratio(3204, 1));
    for (current, old) in [(6405, 3203), (6406, 3204), (7000, 3798)] {
        assert_eq!(
            second.reference_position(AudioSample(current)).unwrap(),
            ratio(old, 1)
        );
    }
    // Resuming the current mapping must not recompute phase from frame two.
    assert_ne!(second.reference_at_anchor(), ratio(3203, 1));
}

#[test]
fn processing_domains_keep_preserve_opaque_and_preparation_clocks_distinct() {
    let doc = document(
        FrameRate::new(30_000, 1001).unwrap(),
        &["preserve"],
        vec![
            (
                "preserve",
                retime("sequence", 6, 0, 12, PitchPolicy::Preserve),
            ),
            (
                "sequence",
                BeatNode::sequence("Context", vec![id("a"), id("quiet"), id("tone"), id("b")]),
            ),
            ("a", source(4)),
            ("quiet", hold(2, HoldAudio::Silence)),
            ("tone", hold(2, HoldAudio::RoomTone { source: audio() })),
            ("b", source(4)),
        ],
    );
    let plan = compile(&doc);
    let root = plan.root_clock();
    let first = root
        .processing_domain_at(
            ReferenceSample(0),
            AudioQueryLimits {
                maximum_spans: 1,
                maximum_work: 3,
            },
        )
        .unwrap();
    assert_eq!(first.lookup().visited_nodes, 2);
    assert_eq!(first.instance(), &instance("preserve"));
    assert_eq!(
        first.kind(),
        &ReferenceProcessingKind::Preserve {
            selection: ratio(0, 1)..ratio(12, 1),
            duration: duration(6),
            rate: ratio(2, 1),
        }
    );
    assert_eq!(first.meaningful_samples(), samples(0, 9610));
    for sample in [3203, 4805, 6406, 9609] {
        let domain = root
            .processing_domain_at(ReferenceSample(sample), Default::default())
            .unwrap();
        assert!(first.same_domain(&domain));
        assert_eq!(domain.allocated_samples(), samples(0, 9610));
    }
    let policy = root.query(samples(3203, 3204), Default::default()).unwrap();
    assert_eq!(
        policy.spans[0].content,
        ReferenceAudioContent::Silence {
            reason: SilenceReason::SilentHold
        }
    );

    let input = plan
        .preserve_input_clock(&instance("preserve"))
        .unwrap()
        .processing_domain_at(ReferenceSample(6407), Default::default())
        .unwrap();
    assert_eq!(input.instance(), &instance("quiet"));
    assert_eq!(
        input.kind(),
        &ReferenceProcessingKind::Leaf {
            content: ReferenceAudioContent::Silence {
                reason: SilenceReason::SilentHold
            },
        }
    );
    let output = plan
        .preserve_output_clock(&instance("preserve"))
        .unwrap()
        .processing_domain_at(ReferenceSample(0), Default::default())
        .unwrap();
    assert_eq!(output.instance(), first.instance());
    assert!(!output.same_domain(&first));
    assert!(!input.same_domain(&output));
    for domain in [&input, &output] {
        assert!(matches!(
            domain.place_root(AudioSample(0)),
            Err(PlanError::InvalidPlan(_))
        ));
    }

    // Even equal serialized layouts compiled twice have separate ownership.
    let foreign = compile(&doc);
    let foreign_domain = foreign
        .root_clock()
        .processing_domain_at(ReferenceSample(0), Default::default())
        .unwrap();
    assert_eq!(foreign_domain.kind(), first.kind());
    assert_eq!(
        foreign_domain.meaningful_samples(),
        first.meaningful_samples()
    );
    assert!(!foreign_domain.same_domain(&first));
}

#[test]
fn partition_allocation_keeps_full_context_but_edit_crops_are_meaningful() {
    for purpose in [RetimePurpose::Partition, RetimePurpose::Edit] {
        let mut selection = retime("a", 2, 2, 4, PitchPolicy::Preserve);
        let NodeKind::Retime {
            purpose: selected_purpose,
            ..
        } = &mut selection.kind
        else {
            unreachable!()
        };
        *selected_purpose = purpose;
        let doc = document(
            FrameRate::new(48_000, 1).unwrap(),
            &["selection"],
            vec![("selection", selection), ("a", source(6))],
        );
        let plan = compile(&doc);
        let domain = plan
            .root_clock()
            .processing_domain_at(ReferenceSample(1), Default::default())
            .unwrap();
        assert_eq!(domain.instance(), &instance("a"));
        assert_eq!(domain.extent(), ratio(0, 1)..ratio(2, 1));
        assert_eq!(domain.allocated_samples(), samples(0, 2));
        assert_eq!(domain.local_at(ReferenceSample(1)).unwrap(), ratio(3, 1));
        assert_eq!(domain.local_at(ReferenceSample(-3)).unwrap(), ratio(-1, 1));
        let meaningful = if purpose == RetimePurpose::Partition {
            samples(-2, 4)
        } else {
            samples(0, 2)
        };
        assert_eq!(domain.meaningful_samples(), meaningful);
        assert_eq!(
            domain.meaningful_extent(),
            ratio(meaningful.start.0.into(), 1)..ratio(meaningful.end.0.into(), 1)
        );
        let placed = domain.place_root(AudioSample(10)).unwrap();
        assert_eq!(
            placed.reference_position(AudioSample(10)).unwrap(),
            ratio(meaningful.start.0.into(), 1)
        );
    }

    let doc = document(
        FrameRate::new(48_000, 1).unwrap(),
        &["a"],
        vec![("a", source(6))],
    );
    let split = edit(
        &doc,
        "split",
        Command::Split {
            node: id("a"),
            at: duration(2),
            identities: SplitIdentities {
                nodes: (0..8).map(|n| id(&format!("split-{n}"))).collect(),
            },
        },
    );
    let plan = compile(&split);
    let left = plan
        .root_clock()
        .processing_domain_at(ReferenceSample(1), Default::default())
        .unwrap();
    let right = plan
        .root_clock()
        .processing_domain_at(ReferenceSample(2), Default::default())
        .unwrap();
    assert_eq!(left.allocated_samples(), samples(0, 2));
    assert_eq!(right.allocated_samples(), samples(2, 6));
    assert_eq!(left.meaningful_samples(), samples(0, 6));
    assert_eq!(right.meaningful_samples(), samples(0, 6));
    assert_eq!(
        left.local_at(ReferenceSample(2)).unwrap(),
        right.local_at(ReferenceSample(2)).unwrap()
    );
    // Explicit authored lineage relates the copies without erasing their
    // distinct physical identity or admitting media/PCM.
    assert_ne!(left.instance(), right.instance());
    assert!(!left.same_domain(&right));
    assert!(left.shares_copy_lineage(&right));
}

#[test]
fn retained_root_maps_keep_wide_exhausted_coordinates_and_fractional_phase() {
    let doc = document(
        FrameRate::new(48_000, 1).unwrap(),
        &["a"],
        vec![("a", source(2))],
    );
    let plan = compile(&doc);
    let domain = plan
        .root_clock()
        .processing_domain_at(ReferenceSample(0), Default::default())
        .unwrap();
    let map = domain.place_root(AudioSample(i64::MIN)).unwrap();
    let width = i128::from(i64::MAX) - i128::from(i64::MIN);
    assert_eq!(
        map.reference_position(AudioSample(i64::MAX)).unwrap(),
        ratio(width, 1)
    );
    let resumed = map
        .resume(AudioSample(i64::MAX), AudioSample(i64::MIN))
        .unwrap();
    assert_eq!(
        resumed.reference_position(AudioSample(i64::MAX)).unwrap(),
        ratio(width * 2, 1)
    );
    assert_eq!(
        domain.local_at(ReferenceSample(i64::MIN)).unwrap(),
        ratio(i128::from(i64::MIN), 1)
    );
    let normal = domain.place_root(AudioSample(7)).unwrap();
    assert_eq!(
        normal.reference_position_at(ratio(15, 2)).unwrap(),
        ratio(1, 2)
    );
    assert_eq!(
        normal.reference_position_at(ratio(13, 2)).unwrap(),
        ratio(-1, 2)
    );
}

#[test]
fn copy_lineage_survives_root_split_refinement_but_rejects_detached_or_moved_audio() {
    let doc = document(
        FrameRate::new(48_000, 1).unwrap(),
        &["a"],
        vec![("a", source(8))],
    );
    let split = edit(
        &doc,
        "root-split",
        Command::Split {
            node: id("root"),
            at: duration(2),
            identities: SplitIdentities {
                nodes: (0..12).map(|i| id(&format!("s-{i}"))).collect(),
            },
        },
    );
    let right = split.children(split.root()).nth(1).unwrap().clone();
    let refined = edit(
        &split,
        "refine",
        Command::Split {
            node: right,
            at: duration(2),
            identities: SplitIdentities {
                nodes: (0..12).map(|i| id(&format!("r-{i}"))).collect(),
            },
        },
    );
    let plan = compile(&refined);
    let domains = [1, 3, 5].map(|sample| {
        plan.root_clock()
            .processing_domain_at(ReferenceSample(sample), Default::default())
            .unwrap()
    });
    for pair in domains.windows(2) {
        assert!(!pair[0].same_domain(&pair[1]));
        assert!(pair[0].shares_copy_lineage(&pair[1]));
        assert!(pair[1].shares_copy_lineage(&pair[0]));
    }
    let foreign = compile(&refined);
    let foreign_domain = foreign
        .root_clock()
        .processing_domain_at(ReferenceSample(1), Default::default())
        .unwrap();
    assert!(!domains[0].shares_copy_lineage(&foreign_domain));
    let mut legacy = serde_json::to_value(FrozenAudioLayout::capture(&refined).unwrap()).unwrap();
    legacy.as_object_mut().unwrap().remove("audio_lineage");
    let legacy =
        AudioReferencePlan::compile(&FrozenAudioLayout::from_json(&legacy.to_string()).unwrap())
            .unwrap();
    let left = legacy
        .root_clock()
        .processing_domain_at(ReferenceSample(1), Default::default())
        .unwrap();
    let right = legacy
        .root_clock()
        .processing_domain_at(ReferenceSample(5), Default::default())
        .unwrap();
    assert!(!left.shares_copy_lineage(&right));
    let changed = edit(
        &refined,
        "changed-audio",
        Command::SetSourceAudioMapping {
            node: domains[2].instance().node.clone(),
            mapping: SourceAudioMapping::FitBeat,
            offset: AudioSample(1),
        },
    );
    let changed = compile(&changed);
    let left = changed
        .root_clock()
        .processing_domain_at(ReferenceSample(1), Default::default())
        .unwrap();
    let right = changed
        .root_clock()
        .processing_domain_at(ReferenceSample(5), Default::default())
        .unwrap();
    assert!(!left.shares_copy_lineage(&right));
    let moved = edit(
        &refined,
        "move",
        Command::Move {
            node: refined.children(refined.root()).nth(2).unwrap().clone(),
            parent: id("root"),
            index: 0,
        },
    );
    let moved = compile(&moved);
    let moved_right = moved
        .root_clock()
        .processing_domain_at(ReferenceSample(1), Default::default())
        .unwrap();
    let moved_left = moved
        .root_clock()
        .processing_domain_at(ReferenceSample(5), Default::default())
        .unwrap();
    assert!(!moved_right.shares_copy_lineage(&moved_left));
}

#[test]
fn copy_lineage_keeps_compact_repeat_paths_and_gap_identity_explicit() {
    let iterations = IterationOrder::new(RevisionId::new("plays").unwrap(), 1_000_000_000).unwrap();
    let doc = document(
        FrameRate::new(48_000, 1).unwrap(),
        &["repeat"],
        vec![
            (
                "repeat",
                BeatNode {
                    label: "Compact".into(),
                    framing: None,
                    audio_edges: Default::default(),
                    kind: NodeKind::Repeat {
                        child: id("a"),
                        iterations: iterations.clone(),
                        gap: Some(HoldRecipe {
                            duration: duration(4),
                            video: HoldVideo::Background,
                            audio: HoldAudio::RoomTone { source: audio() },
                        }),
                    },
                },
            ),
            ("a", source(6)),
        ],
    );
    for (cut, before, after) in [(3, 2, 3), (8, 7, 8)] {
        let split = edit(
            &doc,
            "split",
            Command::Split {
                node: id("repeat"),
                at: duration(cut),
                identities: SplitIdentities {
                    nodes: (0..12).map(|i| id(&format!("s-{i}"))).collect(),
                },
            },
        );
        let plan = compile(&split);
        assert!(plan.layout().to_json().unwrap().len() < 6000);
        let clock = plan.root_clock();
        let domains = [before, after, after + 10].map(|sample| {
            clock
                .processing_domain_at(
                    ReferenceSample(sample),
                    AudioQueryLimits {
                        maximum_spans: 1,
                        maximum_work: 30,
                    },
                )
                .unwrap()
        });
        assert!(!domains[0].same_domain(&domains[1]));
        assert!(domains[0].shares_copy_lineage(&domains[1]));
        assert!(!domains[1].shares_copy_lineage(&domains[2]));
        for domain in &domains {
            assert!(domain.lookup().visited_nodes < 8);
        }
        if cut == 8 {
            assert_eq!(domains[0].gap_after(), iterations.at(0).as_ref());
        }
    }
}

#[test]
fn nested_occurrence_split_retains_copy_lineage_without_expanding_repeats() {
    let outer = IterationOrder::new(RevisionId::new("outer-plays").unwrap(), 2).unwrap();
    let inner =
        IterationOrder::new(RevisionId::new("inner-plays").unwrap(), 1_000_000_000).unwrap();
    let doc = document(
        FrameRate::new(48_000, 1).unwrap(),
        &["outer"],
        vec![
            (
                "outer",
                BeatNode {
                    label: "Outer".into(),
                    framing: None,
                    audio_edges: Default::default(),
                    kind: NodeKind::Repeat {
                        child: id("inner"),
                        iterations: outer.clone(),
                        gap: None,
                    },
                },
            ),
            (
                "inner",
                BeatNode {
                    label: "Inner".into(),
                    framing: None,
                    audio_edges: Default::default(),
                    kind: NodeKind::Repeat {
                        child: id("a"),
                        iterations: inner.clone(),
                        gap: None,
                    },
                },
            ),
            ("a", source(6)),
        ],
    );
    let split = edit(
        &doc,
        "nested-split",
        Command::EditOccurrence {
            instance: InstancePath {
                node: id("a"),
                repeats: vec![
                    RepeatInstance {
                        node: id("outer"),
                        iteration: outer.at(1).unwrap(),
                    },
                    RepeatInstance {
                        node: id("inner"),
                        iteration: inner.at(1).unwrap(),
                    },
                ],
            },
            edit: OccurrenceEdit::Split {
                at: duration(2),
                identities: SplitIdentities {
                    nodes: (0..12).map(|i| id(&format!("s-{i}"))).collect(),
                },
            },
            identities: OccurrenceIdentities {
                nodes: (0..12).map(|i| id(&format!("isolate-{i}"))).collect(),
                marks: vec![],
            },
        },
    );
    let plan = compile(&split);
    assert!(plan.layout().to_json().unwrap().len() < 8000);
    let clock = plan.root_clock();
    let domains = [6_000_000_007, 6_000_000_008].map(|sample| {
        clock
            .processing_domain_at(
                ReferenceSample(sample),
                AudioQueryLimits {
                    maximum_spans: 1,
                    maximum_work: 40,
                },
            )
            .unwrap()
    });
    assert_eq!(domains[0].instance().repeats.len(), 2);
    assert_eq!(
        domains[0].instance().repeats[1].iteration,
        inner.at(1).unwrap()
    );
    assert!(!domains[0].same_domain(&domains[1]));
    assert!(domains[0].shares_copy_lineage(&domains[1]));
}

#[test]
fn copied_preserve_lineage_does_not_merge_distinct_preparation_clocks() {
    let doc = document(
        FrameRate::new(48_000, 1).unwrap(),
        &["preserve"],
        vec![
            ("preserve", retime("a", 8, 0, 4, PitchPolicy::Preserve)),
            ("a", source(4)),
        ],
    );
    let split = edit(
        &doc,
        "split",
        Command::Split {
            node: id("preserve"),
            at: duration(3),
            identities: SplitIdentities {
                nodes: (0..12).map(|i| id(&format!("s-{i}"))).collect(),
            },
        },
    );
    let plan = compile(&split);
    let domains = [1, 4].map(|sample| {
        plan.root_clock()
            .processing_domain_at(ReferenceSample(sample), Default::default())
            .unwrap()
    });
    assert!(matches!(
        domains[0].kind(),
        ReferenceProcessingKind::Preserve { .. }
    ));
    assert!(!domains[0].same_domain(&domains[1]));
    assert!(domains[0].shares_copy_lineage(&domains[1]));
    let prepared = domains
        .iter()
        .map(|domain| {
            plan.preserve_input_clock(domain.instance())
                .unwrap()
                .processing_domain_at(ReferenceSample(1), Default::default())
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        prepared[0].meaningful_extent(),
        prepared[1].meaningful_extent()
    );
    assert_eq!(prepared[0].kind(), prepared[1].kind());
    assert_eq!(
        plan.layout()
            .audio_lineage()
            .get(&prepared[0].instance().node),
        plan.layout()
            .audio_lineage()
            .get(&prepared[1].instance().node)
    );
    assert!(!prepared[0].shares_copy_lineage(&prepared[1]));
    assert!(!prepared[0].same_domain(&prepared[1]));
}

#[test]
fn tail_maximum_retains_local_hold_and_gap_units_through_retime() {
    let tail = HoldAudio::Tail {
        source: audio(),
        maximum: duration(2),
    };
    for gap in [false, true] {
        let (frames, nodes) = if gap {
            (
                12,
                vec![
                    (
                        "context",
                        BeatNode {
                            label: "Tail gap".into(),
                            framing: None,
                            audio_edges: Default::default(),
                            kind: NodeKind::Repeat {
                                child: id("a"),
                                iterations: IterationOrder::new(
                                    RevisionId::new("plays").unwrap(),
                                    2,
                                )
                                .unwrap(),
                                gap: Some(HoldRecipe {
                                    duration: duration(8),
                                    video: HoldVideo::Background,
                                    audio: tail.clone(),
                                }),
                            },
                        },
                    ),
                    ("a", source(2)),
                ],
            )
        } else {
            (8, vec![("context", hold(8, tail.clone()))])
        };
        let mut nodes = nodes;
        nodes.push((
            "preserve",
            retime("context", frames / 2, 0, frames, PitchPolicy::Preserve),
        ));
        let doc = document(FrameRate::new(24, 1).unwrap(), &["preserve"], nodes);
        let frozen = compile(&doc);
        let live = RenderPlan::compile(&doc).unwrap();
        let root = frozen.root_clock();
        let query = root
            .query(
                samples(0, root.sample_count().unwrap().0),
                Default::default(),
            )
            .unwrap();
        let expected = live
            .audio(
                AudioSample(0)..AudioSample(root.sample_count().unwrap().0),
                Default::default(),
            )
            .unwrap();
        for (actual, expected) in query.spans.iter().zip(expected.spans) {
            assert_eq!(actual.content, policy(&expected.content));
        }
        let retained = query
            .spans
            .iter()
            .find(|span| matches!(span.content, ReferenceAudioContent::Tail { .. }))
            .unwrap();
        assert_eq!(
            retained.content,
            ReferenceAudioContent::Tail {
                maximum: duration(2)
            }
        );
        assert_eq!(
            retained.allocated_samples.end.0 - retained.allocated_samples.start.0,
            8000
        );
        assert_eq!(retained.gap_after.is_some(), gap);
        // The final sample still reports policy, even beyond the maximum. The
        // reference planner does not implement the currently unsupported DSP.
        let final_tail_sample = retained.samples.end.0 - 1;
        assert_eq!(
            at(
                &root
                    .query(
                        samples(final_tail_sample, final_tail_sample + 1),
                        Default::default()
                    )
                    .unwrap()
                    .spans,
                final_tail_sample
            ),
            retained.content
        );
        for clock in [
            frozen.preserve_input_clock(&instance("preserve")).unwrap(),
            frozen.preserve_output_clock(&instance("preserve")).unwrap(),
        ] {
            let query = clock
                .query(
                    samples(0, clock.sample_count().unwrap().0),
                    Default::default(),
                )
                .unwrap();
            let tail = query
                .spans
                .iter()
                .find(|span| matches!(span.content, ReferenceAudioContent::Tail { .. }))
                .unwrap();
            assert_eq!(tail.content, retained.content);
        }
    }
}

#[test]
fn billion_play_reference_keeps_old_order_gap_identity_after_current_edits() {
    let iterations = IterationOrder::new(RevisionId::new("plays").unwrap(), 1_000_000_000).unwrap();
    let doc = document(
        FrameRate::new(48_000, 1).unwrap(),
        &["repeat"],
        vec![
            (
                "repeat",
                BeatNode {
                    label: "Compact".into(),
                    framing: None,
                    audio_edges: Default::default(),
                    kind: NodeKind::Repeat {
                        child: id("a"),
                        iterations: iterations.clone(),
                        gap: Some(HoldRecipe {
                            duration: duration(1),
                            video: HoldVideo::Background,
                            audio: HoldAudio::RoomTone { source: audio() },
                        }),
                    },
                },
            ),
            ("a", source(1)),
        ],
    );
    let old = compile(&doc);
    let clock = old.root_clock();
    let expected = clock
        .query(
            samples(1_999_999_994, 1_999_999_999),
            AudioQueryLimits {
                maximum_spans: 5,
                maximum_work: 100,
            },
        )
        .unwrap();
    assert_eq!(expected.spans.len(), 5);
    assert!(expected.lookup.visited_nodes < 20);
    assert!(expected.lookup.iteration_run_comparisons <= 5);
    assert_eq!(expected.spans[1].content, ReferenceAudioContent::RoomTone);
    assert_eq!(expected.spans[1].gap_after, iterations.at(999_999_997));
    let domains = [1_999_999_994, 1_999_999_995, 1_999_999_996].map(|sample| {
        clock
            .processing_domain_at(
                ReferenceSample(sample),
                AudioQueryLimits {
                    maximum_spans: 1,
                    maximum_work: 20,
                },
            )
            .unwrap()
    });
    assert_eq!(
        domains[0].instance().repeats[0].iteration,
        iterations.at(999_999_997).unwrap()
    );
    assert_eq!(domains[1].gap_after(), iterations.at(999_999_997).as_ref());
    assert_eq!(domains[1].instance(), &instance("repeat"));
    assert_eq!(
        domains[1].kind(),
        &ReferenceProcessingKind::Leaf {
            content: ReferenceAudioContent::RoomTone
        }
    );
    assert_eq!(
        domains[1].meaningful_samples(),
        samples(1_999_999_995, 1_999_999_996)
    );
    assert!(!domains[0].same_domain(&domains[2]));
    for domain in &domains {
        assert!(domain.lookup().visited_nodes <= 3);
        assert!(domain.lookup().iteration_run_comparisons <= 1);
    }
    let split = edit(
        &doc,
        "split",
        Command::Split {
            node: id("repeat"),
            at: duration(5),
            identities: SplitIdentities {
                nodes: (0..16).map(|n| id(&format!("split-{n}"))).collect(),
            },
        },
    );
    assert!(split.nodes().len() > doc.nodes().len());
    assert_eq!(
        clock
            .query(samples(1_999_999_994, 1_999_999_999), Default::default())
            .unwrap()
            .spans,
        expected.spans
    );
    let moved = edit(
        &doc,
        "move",
        Command::MovePlays {
            node: id("repeat"),
            start: 999_999_999,
            end: 1_000_000_000,
            destination: 0,
        },
    );
    let moved_plan = compile(&moved);
    let moved_first = moved_plan
        .root_clock()
        .processing_domain_at(ReferenceSample(0), Default::default())
        .unwrap();
    assert_eq!(
        moved_first.instance().repeats[0].iteration,
        iterations.at(999_999_999).unwrap()
    );
    let old_last = clock
        .processing_domain_at(ReferenceSample(1_999_999_998), Default::default())
        .unwrap();
    assert_eq!(old_last.instance().repeats, moved_first.instance().repeats);
    assert_eq!(
        old_last.meaningful_samples(),
        samples(1_999_999_998, 1_999_999_999)
    );
    assert!(!old_last.same_domain(&moved_first));
    let shrunk = edit(
        &moved,
        "shrink",
        Command::SetRepeat {
            node: id("repeat"),
            plays: 2,
            gap: None,
        },
    );
    let removed = edit(&shrunk, "delete", Command::Delete { node: id("repeat") });
    assert_eq!(removed.duration().unwrap(), FrameDuration::ZERO);
    assert_eq!(
        clock
            .query(samples(1_999_999_994, 1_999_999_999), Default::default())
            .unwrap()
            .spans,
        expected.spans
    );
}

#[test]
fn sparse_override_clock_validates_its_effective_play_and_retains_partition_allocation() {
    let doc = document(
        FrameRate::new(24, 1).unwrap(),
        &["repeat"],
        vec![
            (
                "repeat",
                BeatNode {
                    label: "Repeated".into(),
                    framing: None,
                    audio_edges: Default::default(),
                    kind: NodeKind::Repeat {
                        child: id("default"),
                        iterations: IterationOrder::new(RevisionId::new("plays").unwrap(), 3)
                            .unwrap(),
                        gap: None,
                    },
                },
            ),
            ("default", retime("a", 2, 0, 4, PitchPolicy::Preserve)),
            ("a", source(4)),
        ],
    );
    let iteration = IterationId {
        allocation: RevisionId::new("plays").unwrap(),
        ordinal: 1,
    };
    let overridden = edit(
        &doc,
        "override",
        Command::SetPlayOverride {
            node: id("repeat"),
            iteration: iteration.clone(),
            subtree: Subtree {
                root: id("override"),
                nodes: BTreeMap::from([
                    (
                        id("override"),
                        retime("quiet", 3, 0, 4, PitchPolicy::Preserve),
                    ),
                    (id("quiet"), hold(4, HoldAudio::Silence)),
                ]),
                overrides: BTreeMap::new(),
            },
        },
    );
    let frozen = compile(&overridden);
    let path = InstancePath {
        node: id("override"),
        repeats: vec![RepeatInstance {
            node: id("repeat"),
            iteration: iteration.clone(),
        }],
    };
    let clock = frozen.preserve_input_clock(&path).unwrap();
    assert_eq!(clock.sample_count().unwrap(), ReferenceSample(8000));
    let query = clock.query(samples(0, 1), Default::default()).unwrap();
    assert_eq!(query.spans[0].instance.node, id("quiet"));
    assert_eq!(query.spans[0].instance.repeats, path.repeats);
    let root = frozen.root_clock();
    let overridden_domain = root
        .processing_domain_at(ReferenceSample(4000), Default::default())
        .unwrap();
    assert_eq!(overridden_domain.instance(), &path);
    assert_eq!(overridden_domain.meaningful_samples(), samples(4000, 10000));
    assert!(matches!(
        overridden_domain.kind(),
        ReferenceProcessingKind::Preserve { .. }
    ));
    assert_eq!(
        overridden_domain.local_at(ReferenceSample(4000)).unwrap(),
        ExactRatio::ZERO
    );
    let first_default = root
        .processing_domain_at(ReferenceSample(0), Default::default())
        .unwrap();
    let last_default = root
        .processing_domain_at(ReferenceSample(10000), Default::default())
        .unwrap();
    assert_eq!(first_default.instance().node, last_default.instance().node);
    assert_ne!(
        first_default.instance().repeats,
        last_default.instance().repeats
    );
    assert!(!first_default.same_domain(&last_default));
    assert!(!first_default.same_domain(&overridden_domain));
    let wrong = InstancePath {
        node: id("default"),
        ..path
    };
    assert!(frozen.preserve_output_clock(&wrong).is_err());
    let split = edit(
        &overridden,
        "split",
        Command::Split {
            node: id("repeat"),
            at: duration(3),
            identities: SplitIdentities {
                nodes: (0..20).map(|n| id(&format!("split-{n}"))).collect(),
            },
        },
    );
    let split_reference = compile(&split);
    let live = RenderPlan::compile(&split).unwrap();
    let actual = split_reference
        .root_clock()
        .query(samples(0, 14000), Default::default())
        .unwrap();
    let expected = live
        .audio(AudioSample(0)..AudioSample(14000), Default::default())
        .unwrap();
    assert_eq!(actual.spans.len(), expected.spans.len());
    for (actual, expected) in actual.spans.iter().zip(expected.spans) {
        assert_eq!(actual.content, policy(&expected.content));
        assert_eq!(
            actual.allocated_samples,
            samples(
                expected.allocated_samples.start.0,
                expected.allocated_samples.end.0
            )
        );
        assert_eq!(actual.instance, expected.instance);
    }
}

#[test]
fn invalid_owner_paths_ranges_and_budgets_fail_before_results() {
    let doc = document(
        FrameRate::new(24, 1).unwrap(),
        &["repeat"],
        vec![
            (
                "repeat",
                BeatNode {
                    label: "Repeated".into(),
                    framing: None,
                    audio_edges: Default::default(),
                    kind: NodeKind::Repeat {
                        child: id("preserve"),
                        iterations: IterationOrder::new(RevisionId::new("plays").unwrap(), 2)
                            .unwrap(),
                        gap: None,
                    },
                },
            ),
            ("preserve", retime("a", 2, 0, 4, PitchPolicy::Preserve)),
            ("a", source(4)),
        ],
    );
    let plan = compile(&doc);
    assert!(plan.preserve_input_clock(&instance("a")).is_err());
    assert!(plan.preserve_input_clock(&instance("preserve")).is_err());
    assert!(plan.preserve_output_clock(&instance("missing")).is_err());
    let mut selected = instance("preserve");
    selected.repeats.push(RepeatInstance {
        node: id("repeat"),
        iteration: IterationId {
            allocation: RevisionId::new("plays").unwrap(),
            ordinal: 1,
        },
    });
    assert!(plan.preserve_input_clock(&selected).is_ok());
    selected.repeats[0].iteration.ordinal = 2;
    assert!(plan.preserve_input_clock(&selected).is_err());
    let root = plan.root_clock();
    for sample in [-1, 8000, i64::MAX] {
        assert!(matches!(
            root.processing_domain_at(ReferenceSample(sample), Default::default()),
            Err(PlanError::AudioRangeOutOfRange)
        ));
    }
    assert!(matches!(
        root.processing_domain_at(
            ReferenceSample(0),
            AudioQueryLimits {
                maximum_spans: 1,
                maximum_work: 1
            }
        ),
        Err(PlanError::AudioQueryLimit("structural work"))
    ));
    for range in [samples(-1, 0), samples(2, 1), samples(0, 8001)] {
        assert!(matches!(
            root.query(range, Default::default()),
            Err(PlanError::AudioRangeOutOfRange)
        ));
    }
    assert!(
        root.query(samples(0, 0), Default::default())
            .unwrap()
            .spans
            .is_empty()
    );
    assert!(matches!(
        root.query(
            samples(0, 1),
            AudioQueryLimits {
                maximum_spans: 1,
                maximum_work: 1
            }
        ),
        Err(PlanError::AudioQueryLimit(_))
    ));
    assert!(matches!(
        root.query(
            samples(0, 8000),
            AudioQueryLimits {
                maximum_spans: 1,
                maximum_work: 100
            }
        ),
        Err(PlanError::AudioQueryLimit("span count"))
    ));
    for limits in [
        AudioQueryLimits {
            maximum_spans: 0,
            maximum_work: 100,
        },
        AudioQueryLimits {
            maximum_spans: 1,
            maximum_work: 65_537,
        },
    ] {
        assert!(matches!(
            root.processing_domain_at(ReferenceSample(0), limits),
            Err(PlanError::InvalidAudioLimits)
        ));
        assert!(matches!(
            root.query(samples(0, 1), limits),
            Err(PlanError::InvalidAudioLimits)
        ));
    }
}
