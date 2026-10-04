use std::collections::BTreeMap;

use deadpan_core::*;
use deadpan_plan::{AudioDefinitionSelector, AudioFadeQuery, AudioRootPlacement, RenderPlan};

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn ratio(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}
fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn hold(length: i64) -> BeatNode {
    BeatNode::hold(
        "Voice",
        HoldRecipe {
            duration: frames(length),
            video: HoldVideo::Background,
            picture_context: None,
            audio: HoldAudio::Silence,
        },
    )
}
fn retime(child: &str, length: i64, start: i64, end: i64, pitch: PitchPolicy) -> BeatNode {
    BeatNode {
        label: "Timing".into(),
        framing: None,
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Retime {
            child: id(child),
            duration: frames(length),
            mapping: FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap(),
            pitch,
            purpose: RetimePurpose::Edit,
        },
        cutaways: Vec::new(),
    }
}
fn document(rate: u32, children: &[&str], nodes: Vec<(&str, BeatNode)>) -> ProjectDocument {
    let document = ProjectDocument::new(
        ProjectId::new("fade-query").unwrap(),
        RevisionId::new("current").unwrap(),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(rate, 1).unwrap(),
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
    let mut wire = serde_json::to_value(document).unwrap();
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}
fn bind(
    current: &ProjectDocument,
    reference: &ProjectDocument,
    physical: &str,
    clock: AudioClockRoot,
    phase: Option<ExactRatio>,
) -> ProjectDocument {
    let timing = AudioTimingId {
        allocation: RevisionId::new("fade-clock").unwrap(),
        ordinal: 0,
    };
    let binding = OwnedAudioBinding {
        reanchors: Vec::new(),
        lattice: AudioPlacementTemplate {
            reference_local_offset: deadpan_core::ExactRatio::ZERO,
            gap_after: None,
            reference: AudioReferenceClock {
                recipe: AudioRecipeKind::Node,
                timing: timing.clone(),
                root: clock,
                physical: id(physical),
            },
            arguments: vec![],
            births: vec![],
        },
        resume: phase.map(|constant| AudioResume {
            local_boundary: ExactRatio::ZERO,
            phase: AudioLocalPhase {
                constant,
                terms: vec![],
            },
        }),
    };
    let state = AudioBindingState::new(
        vec![AudioTimingRecord {
            id: timing,
            layout: FrozenAudioLayout::capture(reference).unwrap(),
        }],
        BTreeMap::from([(id(physical), binding)]),
    )
    .unwrap();
    let mut wire = serde_json::to_value(current).unwrap();
    wire["audio_bindings"] = serde_json::to_value(state).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}
fn query(plan: &RenderPlan, start: i64, end: i64) -> AudioFadeQuery {
    plan.audio_fades(AudioSample(start)..AudioSample(end), Default::default())
        .unwrap()
}
fn samples(query: AudioFadeQuery) -> Vec<(u64, ExactRatio)> {
    query
        .spans
        .into_iter()
        .flat_map(|span| {
            (span.samples.start.0..span.samples.end.0).map(move |sample| {
                (
                    span.start.first().map_or(0, |edge| edge.length),
                    span.start
                        .first()
                        .map_or(ExactRatio::ZERO, |edge| edge.distance_at_start)
                        .checked_add(ExactRatio::integer(sample - span.samples.start.0))
                        .unwrap(),
                )
            })
        })
        .collect()
}

#[test]
fn translated_output_retains_tiny_width_and_rate_changes_use_a_stable_virtual_origin() {
    let original = document(32_000, &["voice"], vec![("voice", hold(1))]);
    let moved = document(
        32_000,
        &["prefix", "voice"],
        vec![("prefix", hold(1)), ("voice", hold(1))],
    );
    let moved = RenderPlan::compile(&bind(
        &moved,
        &original,
        "voice",
        AudioClockRoot::ProjectRootRoundEven,
        None,
    ))
    .unwrap();
    assert_eq!(samples(query(&moved, 2, 3)), vec![(2, ExactRatio::ZERO)]);

    for prefix in [false, true] {
        let mut nodes = vec![
            ("voice", hold(1)),
            ("slow", retime("voice", 2, 0, 1, PitchPolicy::FollowSpeed)),
        ];
        let children = if prefix {
            nodes.push(("prefix", hold(1)));
            vec!["prefix", "slow"]
        } else {
            vec!["slow"]
        };
        let current = document(32_000, &children, nodes);
        let plan = RenderPlan::compile(&bind(
            &current,
            &original,
            "voice",
            AudioClockRoot::ProjectRootRoundEven,
            None,
        ))
        .unwrap();
        let start = if prefix { 2 } else { 0 };
        let end = if prefix { 4 } else { 3 };
        assert_eq!(
            samples(query(&plan, start, end)),
            (0..end - start)
                .map(|p| (3, ExactRatio::integer(p)))
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn opaque_preserve_uses_output_geometry_and_does_not_transport_input_fades() {
    let current = document(
        32_000,
        &["preserve"],
        vec![
            ("voice", hold(1)),
            ("preserve", retime("voice", 2, 0, 1, PitchPolicy::Preserve)),
        ],
    );
    let unbound = RenderPlan::compile(&current).unwrap();
    let bound = RenderPlan::compile(&bind(
        &current,
        &current,
        "voice",
        AudioClockRoot::PreserveInputPointCeil {
            stage: id("preserve"),
        },
        None,
    ))
    .unwrap();
    assert_eq!(query(&unbound, 0, 3).spans, query(&bound, 0, 3).spans);
    assert_eq!(
        samples(query(&bound, 0, 3)),
        vec![
            (3, ExactRatio::ZERO),
            (3, ExactRatio::ONE),
            (3, ExactRatio::integer(2))
        ]
    );
}

#[test]
fn selected_origin_point_clock_uses_current_support_without_resetting_phase() {
    let reference = document(
        32_000,
        &["preserve"],
        vec![
            ("voice", hold(3)),
            ("preserve", retime("voice", 2, 1, 2, PitchPolicy::Preserve)),
        ],
    );
    let current = document(32_000, &["voice"], vec![("voice", hold(3))]);
    let plan = RenderPlan::compile(&bind(
        &current,
        &reference,
        "voice",
        AudioClockRoot::PreserveInputPointCeil {
            stage: id("preserve"),
        },
        None,
    ))
    .unwrap();
    // Removing the Preserve exposes the current complete Hold [0,3), not
    // its former selected input [1,2). The retained point origin is still 1:
    // ceil((0-1)*1.5)=-1 and ceil((3-1)*1.5)=3 give four fade samples.
    // The original anchor maps root sample s to q=s-2. Thus s=0 is before
    // support and s=1 begins its fade; the clock did not reset to local zero.
    assert_eq!(
        samples(query(&plan, 0, 4)),
        vec![
            (0, ExactRatio::ZERO),
            (4, ExactRatio::ZERO),
            (4, ExactRatio::ONE),
            (4, ExactRatio::integer(2)),
        ]
    );
    let outside = query(&plan, 0, 1);
    assert_eq!(
        outside.spans[0].start.first().map_or(0, |edge| edge.length),
        0
    );
    assert!(outside.spans[0].start.is_empty());

    // A still-owned meaningful crop must constrain that same retained clock.
    // Selecting [1,2) gives ceil(1.5)-ceil(0)=2, even though its child is 3f.
    let cropped = document(
        32_000,
        &["crop"],
        vec![
            ("voice", hold(3)),
            ("crop", retime("voice", 1, 1, 2, PitchPolicy::FollowSpeed)),
        ],
    );
    let cropped = RenderPlan::compile(&bind(
        &cropped,
        &reference,
        "voice",
        AudioClockRoot::PreserveInputPointCeil {
            stage: id("preserve"),
        },
        None,
    ))
    .unwrap();
    let cropped = query(&cropped, 0, 2);
    assert!(
        cropped.spans[0]
            .start
            .iter()
            .flat_map(|edge| &edge.origins)
            .any(|origin| origin.instance.node == id("crop"))
    );
    assert_eq!(
        samples(cropped),
        vec![(2, ExactRatio::ZERO), (2, ExactRatio::ONE)]
    );

    // A canonical birth clock is independently point-ceil: ceil(4.5) is
    // five, while the visible project root rounds the same end to four.
    let birth = RenderPlan::compile(&bind(
        &current,
        &reference,
        "voice",
        AudioClockRoot::DefinitionPointCeil { root: id("voice") },
        None,
    ))
    .unwrap();
    assert_eq!(
        samples(query(&birth, 0, 4)),
        (0..4)
            .map(|progress| (5, ExactRatio::integer(progress)))
            .collect::<Vec<_>>()
    );
}

#[test]
fn fractional_progress_and_inverse_ceil_partitioning_are_request_independent() {
    let reference = document(
        48_000,
        &["preserve"],
        vec![
            ("a", hold(1)),
            ("b", hold(1)),
            ("pair", BeatNode::sequence("Pair", vec![id("a"), id("b")])),
            ("preserve", retime("pair", 4, 0, 2, PitchPolicy::Preserve)),
        ],
    );
    let current = document(
        48_000,
        &["speed"],
        vec![
            ("a", hold(1)),
            ("b", hold(1)),
            ("pair", BeatNode::sequence("Pair", vec![id("a"), id("b")])),
            ("preserve", retime("pair", 4, 0, 2, PitchPolicy::Preserve)),
            (
                "speed",
                retime("preserve", 3, 0, 4, PitchPolicy::FollowSpeed),
            ),
        ],
    );
    let plan = RenderPlan::compile(&bind(
        &current,
        &reference,
        "preserve",
        AudioClockRoot::ProjectRootRoundEven,
        Some(ratio(1, 4)),
    ))
    .unwrap();
    let whole = query(&plan, 0, 3);
    assert_eq!(whole.spans.len(), 2);
    assert_eq!(whole.spans[0].samples, AudioSample(0)..AudioSample(2));
    assert_eq!(
        whole.spans[0].start.first().map_or(0, |edge| edge.length),
        2
    );
    assert_eq!(whole.spans[0].start[0].distance_at_start, ratio(3, 16));
    assert_eq!(
        whole.spans[1].start.first().map_or(0, |edge| edge.length),
        1
    );
    assert_eq!(whole.spans[1].start[0].distance_at_start, ratio(11, 16));
    let paged = (0..3)
        .flat_map(|sample| samples(query(&plan, sample, sample + 1)))
        .collect::<Vec<_>>();
    assert_eq!(samples(whole), paged);
    assert!(
        plan.audio_fades(
            AudioSample(0)..AudioSample(3),
            deadpan_plan::AudioQueryLimits {
                maximum_spans: 1,
                maximum_work: 65_536
            }
        )
        .is_err()
    );
    assert!(
        plan.audio_fades(
            AudioSample(0)..AudioSample(3),
            deadpan_plan::AudioQueryLimits {
                maximum_spans: 16,
                maximum_work: 3
            }
        )
        .is_err()
    );
}

#[test]
fn signed_domain_and_retained_negative_origin_keep_absolute_tie_parity() {
    let current = document(32_000, &["voice"], vec![("voice", hold(4))]);
    let plan = RenderPlan::compile(&current).unwrap();
    let domain = plan
        .audio_definition(AudioDefinitionSelector::Node { node: id("voice") })
        .unwrap()
        .in_root_clock(
            AudioRootPlacement::new(
                ratio(-1, 3),
                ratio(1, 2),
                ExactRatio::ZERO..ExactRatio::integer(4),
            )
            .unwrap(),
        )
        .unwrap();
    let result = domain
        .fades(AudioSample(0)..AudioSample(2), Default::default())
        .unwrap();
    assert_eq!(
        result.spans[0].start.first().map_or(0, |edge| edge.length),
        2
    );
    assert_eq!(result.spans[0].start[0].distance_at_start, ExactRatio::ZERO);

    let mut partition = retime("voice", 2, 2, 4, PitchPolicy::FollowSpeed);
    let NodeKind::Retime { purpose, .. } = &mut partition.kind else {
        unreachable!()
    };
    *purpose = RetimePurpose::Partition;
    let reference = document(
        32_000,
        &["partition"],
        vec![("voice", hold(4)), ("partition", partition)],
    );
    let slowed = document(
        32_000,
        &["prefix", "speed"],
        vec![
            ("prefix", hold(1)),
            ("voice", hold(4)),
            ("speed", retime("voice", 2, 0, 4, PitchPolicy::FollowSpeed)),
        ],
    );
    let plan = RenderPlan::compile(&bind(
        &slowed,
        &reference,
        "voice",
        AudioClockRoot::ProjectRootRoundEven,
        None,
    ))
    .unwrap();
    assert_eq!(
        samples(query(&plan, 2, 4)),
        vec![(3, ExactRatio::ZERO), (3, ExactRatio::ONE)]
    );
}

#[test]
fn current_coincident_hard_edges_survive_while_noncoincident_crop_edges_do_not_inherit_them() {
    let mut voice = hold(4);
    voice.audio_edges = AudioEdgePolicies {
        node_start: AudioEdgePolicy::Hard,
        node_end: AudioEdgePolicy::Hard,
        ..Default::default()
    };
    let reference = document(48_000, &["voice"], vec![("voice", hold(4))]);
    let untrimmed = document(48_000, &["voice"], vec![("voice", voice.clone())]);
    let untrimmed = RenderPlan::compile(&bind(
        &untrimmed,
        &reference,
        "voice",
        AudioClockRoot::ProjectRootRoundEven,
        None,
    ))
    .unwrap();
    let untrimmed = query(&untrimmed, 0, 4);
    assert!(
        untrimmed.spans[0]
            .start
            .iter()
            .flat_map(|edge| &edge.origins)
            .any(|origin| origin.policy == AudioEdgePolicy::Hard)
    );
    assert!(
        untrimmed.spans[0]
            .end
            .iter()
            .flat_map(|edge| &edge.origins)
            .any(|origin| origin.policy == AudioEdgePolicy::Hard)
    );
    let current = document(
        48_000,
        &["crop"],
        vec![
            ("voice", voice),
            ("crop", retime("voice", 2, 1, 3, PitchPolicy::FollowSpeed)),
        ],
    );
    let plan = RenderPlan::compile(&bind(
        &current,
        &reference,
        "voice",
        AudioClockRoot::ProjectRootRoundEven,
        None,
    ))
    .unwrap();
    let fades = query(&plan, 0, 2);
    assert_eq!(
        fades.spans[0].start.first().map_or(0, |edge| edge.length),
        2
    );
    assert!(
        fades.spans[0]
            .start
            .iter()
            .flat_map(|edge| &edge.origins)
            .all(|origin| origin.policy != AudioEdgePolicy::Hard)
    );
    assert!(
        fades.spans[0]
            .end
            .iter()
            .flat_map(|edge| &edge.origins)
            .all(|origin| origin.policy != AudioEdgePolicy::Hard)
    );
    assert!(
        fades.spans[0]
            .start
            .iter()
            .flat_map(|edge| &edge.origins)
            .any(|origin| origin.instance.node == id("crop"))
    );
}

#[test]
fn virtual_source_support_can_have_zero_or_one_fade_samples() {
    let source_document = |speed: bool| {
        let current = if speed {
            document(
                32_000,
                &["speed"],
                vec![
                    ("voice", hold(4)),
                    ("speed", retime("voice", 1, 0, 4, PitchPolicy::FollowSpeed)),
                ],
            )
        } else {
            document(32_000, &["voice"], vec![("voice", hold(4))])
        };
        let time_base = SourceTimeBase::new(1, 48_000).unwrap();
        let span = SourceSpan::new(
            SourceTimestamp {
                ticks: 0,
                time_base,
            },
            SourceTimestamp {
                ticks: 64,
                time_base,
            },
        )
        .unwrap();
        let source = BeatNode {
            label: "Voice".into(),
            framing: None,
            audio_treatments: Default::default(),
            audio_editorial_edges: Default::default(),
            audio_edges: Default::default(),
            kind: NodeKind::Source {
                source: SourceNode {
                    edit_window: None,
                    duration: frames(4),
                    video: SourceVideo::Blank,
                    video_mapping: SourceVideoMapping::FitBeat,
                    audio: Some(SourceAudio {
                        asset: AssetId::new("original").unwrap(),
                        span,
                    }),
                    audio_mapping: SourceAudioMapping::Placement {
                        start: ratio(3, 10),
                        frames: ratio(2, 5),
                    },
                    audio_offset: AudioSample(0),
                    link: LinkRelation::Independent,
                },
            },
            cutaways: Vec::new(),
        };
        let mut wire = serde_json::to_value(current).unwrap();
        wire["nodes"]["voice"] = serde_json::to_value(source).unwrap();
        wire["assets"] = serde_json::to_value(BTreeMap::from([(
            AssetId::new("original").unwrap(),
            AssetRecord {
                label: "Original".into(),
                content_hash: "a".repeat(64),
                video: None,
                audio: Some(span),
                still_image: true,
                frame_count: None,
                source_qualification: None,
            },
        )]))
        .unwrap();
        ProjectDocument::from_json(&wire.to_string()).unwrap()
    };
    let original = source_document(false);
    let plan = RenderPlan::compile(&original).unwrap();
    assert_eq!(query(&plan, 0, 1).spans[0].start[0].length, 1);
    let faster = bind(
        &source_document(true),
        &original,
        "voice",
        AudioClockRoot::ProjectRootRoundEven,
        None,
    );
    let plan = RenderPlan::compile(&faster).unwrap();
    let fade = query(&plan, 0, 1);
    assert_eq!(fade.spans[0].start.first().map_or(0, |edge| edge.length), 0);
    assert!(!fade.spans[0].start[0].origins.is_empty());
    assert!(!fade.spans[0].end[0].origins.is_empty());
}

fn with_editorial(
    document: &ProjectDocument,
    node: &str,
    start: bool,
    end: bool,
    hard: bool,
) -> ProjectDocument {
    let mut owner = document.nodes()[&id(node)].clone();
    owner.audio_editorial_edges = AudioEditorialEdges { start, end };
    if hard {
        if start {
            owner.audio_edges.node_start = AudioEdgePolicy::Hard;
        }
        if end {
            owner.audio_edges.node_end = AudioEdgePolicy::Hard;
        }
    }
    let mut wire = serde_json::to_value(document).unwrap();
    wire["nodes"][node] = serde_json::to_value(owner).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

#[test]
fn editorial_entry_uses_current_grid_without_reclocking_the_retained_opposite_edge() {
    let reference = document(32_000, &["voice"], vec![("voice", hold(4))]);
    let mut crop = retime("voice", 3, 1, 4, PitchPolicy::FollowSpeed);
    let NodeKind::Retime { purpose, .. } = &mut crop.kind else {
        unreachable!()
    };
    *purpose = RetimePurpose::Partition;
    let current = document(
        32_000,
        &["lead", "crop"],
        vec![("lead", hold(1)), ("voice", hold(4)), ("crop", crop)],
    );
    let current = bind(
        &current,
        &reference,
        "voice",
        AudioClockRoot::ProjectRootRoundEven,
        Some(ratio(1, 3)),
    );
    let before = RenderPlan::compile(&current).unwrap();
    let marked = with_editorial(&current, "crop", true, false, false);
    let after = RenderPlan::compile(&marked).unwrap();
    // At 32k fps, frame1 begins at round_even(1.5)=2. The retained
    // reference phase is independent of this newly authored sample boundary.
    let old = query(&before, 2, 3);
    let new = query(&after, 2, 3);
    let entry = new.spans[0]
        .start
        .iter()
        .find(|edge| edge.editorial)
        .unwrap();
    assert_eq!(entry.at, ExactRatio::ONE);
    assert_eq!(entry.distance_at_start, ExactRatio::ZERO);
    assert_eq!(new.spans[0].end, old.spans[0].end);
    assert_eq!(
        after
            .audio(AudioSample(2)..AudioSample(6), Default::default())
            .unwrap()
            .spans,
        before
            .audio(AudioSample(2)..AudioSample(6), Default::default())
            .unwrap()
            .spans,
        "editorial markers must not alter allocation, raw envelope or sampling support",
    );
    let next = query(&after, 3, 4);
    assert_eq!(
        next.spans[0]
            .start
            .iter()
            .find(|edge| edge.editorial)
            .unwrap()
            .distance_at_start,
        ExactRatio::ONE
    );
}

#[test]
fn tiny_voice_shortens_a_large_marked_owner_and_frozen_context_keeps_the_marker() {
    for length in [1, 2] {
        let plain = document(
            48_000,
            &["group"],
            vec![
                ("voice", hold(length)),
                ("rest", hold(200)),
                (
                    "group",
                    BeatNode::sequence("Group", vec![id("voice"), id("rest")]),
                ),
            ],
        );
        let marked = with_editorial(&plain, "group", true, false, false);
        let plan = RenderPlan::compile(&marked).unwrap();
        let result = query(&plan, 0, 1);
        assert_eq!(
            result.spans[0].start.len(),
            1,
            "coincident origins form one ramp"
        );
        let entry = &result.spans[0].start[0];
        assert!(entry.editorial);
        assert_eq!(entry.length, length as u64);
        assert_eq!(entry.distance_at_start, ExactRatio::ZERO);
        assert!(
            entry
                .origins
                .iter()
                .any(|origin| origin.instance.node == id("group"))
        );
        let context = FrozenAudioContext::capture(&marked).unwrap();
        assert!(context.layout().nodes()[&id("group")].editorial_edges.start);
        let retained = RenderPlan::compile_audio_context(&context).unwrap();
        assert_eq!(query(&retained, 0, 1).spans, result.spans);
        let untouched = RenderPlan::compile(&plain).unwrap();
        assert_eq!(
            query(&plan, length, length + 2).spans,
            query(&untouched, length, length + 2).spans,
            "the first voice's short envelope cannot leak into its later sibling",
        );
    }
}

#[test]
fn editorial_hard_precedence_uses_exact_coordinates_not_rounded_samples() {
    let plain = document(
        96_000,
        &["lead", "voice"],
        vec![("lead", hold(1)), ("voice", hold(8))],
    );
    let mut wire = serde_json::to_value(&plain).unwrap();
    let mut root = plain.nodes()[&id("root")].clone();
    root.audio_edges.node_start = AudioEdgePolicy::Hard;
    wire["nodes"]["root"] = serde_json::to_value(root).unwrap();
    let plain = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let marked = with_editorial(&plain, "voice", true, false, false);
    let plan = RenderPlan::compile(&marked).unwrap();
    let fades = query(&plan, 0, 1);
    let entry = fades.spans[0]
        .start
        .iter()
        .find(|edge| edge.editorial)
        .unwrap();
    // Both frame0 and frame1 round to sample0, but root's Hard at frame0
    // does not belong to the voice's exact frame1 boundary.
    assert_eq!(entry.at, ExactRatio::ONE);
    assert!(
        entry
            .origins
            .iter()
            .all(|origin| origin.policy == AudioEdgePolicy::Automatic)
    );
    let hard = with_editorial(&marked, "voice", true, false, true);
    let hard = query(&RenderPlan::compile(&hard).unwrap(), 0, 1);
    assert!(
        hard.spans[0].start[0]
            .origins
            .iter()
            .any(|origin| origin.policy == AudioEdgePolicy::Hard)
    );
}

#[test]
fn bound_preserve_inner_leaf_cannot_lend_hard_policy_to_a_different_current_leaf() {
    let mut first = hold(2);
    first.audio_edges.node_start = AudioEdgePolicy::Hard;
    let base = document(
        30,
        &["preserve"],
        vec![
            ("first", first),
            ("second", hold(2)),
            (
                "group",
                BeatNode::sequence("Input", vec![id("first"), id("second")]),
            ),
            ("preserve", retime("group", 3, 0, 4, PitchPolicy::Preserve)),
        ],
    );
    let marked = with_editorial(&base, "second", true, false, false);
    let bound = bind(
        &marked,
        &base,
        "preserve",
        AudioClockRoot::ProjectRootRoundEven,
        Some(ratio(-1, 3200)),
    );
    let plan = RenderPlan::compile(&bound).unwrap();
    // Current geometry selects second at frame3/2 (sample2400), while
    // retained q=2399.5 still belongs to first. Its Hard at0 is not at3/2.
    let fades = query(&plan, 2400, 2402);
    let first_span = &fades.spans[0];
    assert!(first_span.start.iter().any(|edge| {
        edge.at == ExactRatio::ZERO
            && edge.origins.iter().any(|origin| {
                origin.instance.node == id("first") && origin.policy == AudioEdgePolicy::Hard
            })
    }));
    assert!(
        first_span.start.iter().all(|edge| !edge.editorial),
        "a marker on second cannot attach while the retained sample still belongs to first",
    );
    let next = query(&plan, 2401, 2402);
    assert_eq!(next.spans, fades.spans[1..]);
    assert_eq!(
        next.spans[0]
            .start
            .iter()
            .find(|edge| edge.editorial)
            .unwrap()
            .distance_at_start,
        ExactRatio::ONE
    );
    let marker = next.spans[0]
        .start
        .iter()
        .find(|edge| edge.editorial)
        .unwrap();
    assert_eq!(marker.at, ratio(3, 2));
    assert!(
        marker
            .origins
            .iter()
            .all(|origin| origin.policy == AudioEdgePolicy::Automatic)
    );
}

#[test]
fn later_partition_drops_ancestor_marker_but_retains_its_own_hidden_owner_edge() {
    let mut crop = retime("voice", 200, 1, 201, PitchPolicy::FollowSpeed);
    let NodeKind::Retime { purpose, .. } = &mut crop.kind else {
        unreachable!()
    };
    *purpose = RetimePurpose::Partition;
    let plain = document(
        48_000,
        &["group"],
        vec![
            ("lead", hold(1)),
            ("voice", hold(201)),
            ("crop", crop),
            (
                "group",
                BeatNode::sequence("Group", vec![id("lead"), id("crop")]),
            ),
        ],
    );
    let ancestor = with_editorial(&plain, "group", true, false, false);
    let reference = RenderPlan::compile(&plain).unwrap();
    let plan = RenderPlan::compile(&ancestor).unwrap();
    assert_eq!(
        query(&plan, 1, 97).spans,
        query(&reference, 1, 97).spans,
        "the later allocation starts at1 even though its hidden voice extends to0",
    );
    let owned = with_editorial(&plain, "voice", true, false, false);
    for bound in [false, true] {
        let current = if bound {
            bind(
                &owned,
                &plain,
                "voice",
                AudioClockRoot::ProjectRootRoundEven,
                None,
            )
        } else {
            owned.clone()
        };
        let plan = RenderPlan::compile(&current).unwrap();
        let result = query(&plan, 1, 2);
        let edge = result.spans[0]
            .start
            .iter()
            .find(|edge| edge.editorial)
            .unwrap();
        assert_eq!(edge.at, ExactRatio::ZERO);
        assert_eq!(edge.distance_at_start, ExactRatio::ONE);
        assert!(
            edge.origins
                .iter()
                .any(|origin| origin.instance.node == id("voice"))
        );
    }
}

#[test]
fn bound_outer_edge_fades_the_voice_delivered_at_the_cut_not_the_next_inner_voice() {
    let nodes = || {
        vec![
            ("first", hold(2)),
            ("second", hold(2)),
            (
                "group",
                BeatNode::sequence("Input", vec![id("first"), id("second")]),
            ),
            ("preserve", retime("group", 6, 0, 4, PitchPolicy::Preserve)),
        ]
    };
    let reference = document(30, &["preserve"], nodes());
    let mut crop = retime("preserve", 3, 3, 6, PitchPolicy::FollowSpeed);
    let NodeKind::Retime { purpose, .. } = &mut crop.kind else {
        unreachable!()
    };
    *purpose = RetimePurpose::Partition;
    let mut current_nodes = nodes();
    current_nodes.extend([("lead", hold(3)), ("crop", crop)]);
    let current = document(30, &["lead", "crop"], current_nodes);
    let marked = with_editorial(&current, "crop", true, false, false);
    let bound = bind(
        &marked,
        &reference,
        "preserve",
        AudioClockRoot::ProjectRootRoundEven,
        Some(ratio(-1, 160)),
    );
    let plan = RenderPlan::compile(&bound).unwrap();
    // The structural seam is frame3/B=4800. Resume shifts its incident
    // sample to q=4790, so ten samples of first remain after the new cut.
    let result = query(&plan, 4800, 4812);
    assert_eq!(result.spans.len(), 2);
    assert_eq!(
        result.spans[0].samples,
        AudioSample(4800)..AudioSample(4810)
    );
    let edge = result.spans[0]
        .start
        .iter()
        .find(|edge| edge.editorial)
        .unwrap();
    assert_eq!(edge.at, ExactRatio::integer(3));
    assert_eq!(edge.length, 10);
    assert_eq!(edge.distance_at_start, ExactRatio::ZERO);
    assert!(
        edge.origins
            .iter()
            .any(|origin| origin.instance.node == id("crop"))
    );
    assert!(result.spans[1].start.iter().all(|edge| !edge.editorial));
    let middle = query(&plan, 4804, 4807);
    let edge = middle.spans[0]
        .start
        .iter()
        .find(|edge| edge.editorial)
        .unwrap();
    assert_eq!(edge.length, 10);
    assert_eq!(edge.distance_at_start, ExactRatio::integer(4));
}

#[test]
fn new_bound_edge_width_uses_delivered_samples_instead_of_virtual_envelope_rounding() {
    let nodes = || {
        vec![
            ("a", hold(1)),
            ("b", hold(1)),
            ("c", hold(1)),
            ("d", hold(1)),
            (
                "group",
                BeatNode::sequence("Input", vec![id("a"), id("b"), id("c"), id("d")]),
            ),
            ("preserve", retime("group", 6, 0, 4, PitchPolicy::Preserve)),
        ]
    };
    let reference = document(48_000, &["preserve"], nodes());
    for (start_edge, crop_start, crop_end) in [(true, 5, 9), (false, 0, 6)] {
        let mut crop = retime(
            "speed",
            crop_end - crop_start,
            crop_start,
            crop_end,
            PitchPolicy::FollowSpeed,
        );
        let NodeKind::Retime { purpose, .. } = &mut crop.kind else {
            unreachable!()
        };
        *purpose = RetimePurpose::Partition;
        let mut current_nodes = nodes();
        current_nodes.extend([
            (
                "speed",
                retime("preserve", 9, 0, 6, PitchPolicy::FollowSpeed),
            ),
            ("crop", crop),
        ]);
        let children = if crop_start > 0 {
            current_nodes.push(("lead", hold(crop_start)));
            vec!["lead", "crop"]
        } else {
            vec!["crop"]
        };
        let current = document(48_000, &children, current_nodes);
        let marked = with_editorial(&current, "crop", start_edge, !start_edge, false);
        let current = bind(
            &marked,
            &reference,
            "preserve",
            AudioClockRoot::ProjectRootRoundEven,
            None,
        );
        let plan = RenderPlan::compile(&current).unwrap();
        // At output5, q=10/3 belongs to c's reference interval[3,4).
        // Inverting q at its endpoints yields output[9/2,6), which
        // contains exactly sample5. The old virtual width remains3.
        let result = query(&plan, 5, 6);
        let span = &result.spans[0];
        assert_eq!(span.samples, AudioSample(5)..AudioSample(6));
        assert_eq!(
            span.start
                .iter()
                .find(|edge| !edge.editorial)
                .unwrap()
                .length,
            3
        );
        let edges = if start_edge { &span.start } else { &span.end };
        let edge = edges.iter().find(|edge| edge.editorial).unwrap();
        assert_eq!(edge.at, ExactRatio::integer(if start_edge { 5 } else { 6 }));
        assert_eq!(edge.length, 1, "a one-sample voice must keep unity gain");
        assert_eq!(edge.distance_at_start, ExactRatio::ZERO);
        let all = query(&plan, crop_start, crop_end);
        assert_eq!(
            all.spans
                .iter()
                .find(|span| span.samples.start == AudioSample(5))
                .unwrap(),
            span
        );
    }
}

#[test]
fn transported_bound_owner_edges_are_not_reintroduced_after_eligibility_rejects_them() {
    let plain = document(48_000, &["voice"], vec![("voice", hold(200))]);
    for (start, end, phase, range) in [(true, false, -10, 10..20), (false, true, 10, 180..190)] {
        let bound = bind(
            &plain,
            &plain,
            "voice",
            AudioClockRoot::ProjectRootRoundEven,
            Some(ExactRatio::integer(phase)),
        );
        let marked = with_editorial(&bound, "voice", start, end, false);
        let original = RenderPlan::compile(&bound).unwrap();
        let plan = RenderPlan::compile(&marked).unwrap();
        // q at the authored marker's incident output sample is outside
        // support. Starting the retained walk at voice must not add that
        // same marker again with structural rather than sample eligibility.
        assert_eq!(
            query(&plan, range.start, range.end).spans,
            query(&original, range.start, range.end).spans
        );
        let query = query(&plan, range.start, range.start + 1);
        assert!(
            query.spans[0]
                .start
                .iter()
                .chain(&query.spans[0].end)
                .all(|edge| !edge.editorial)
        );
        let edge = if start {
            &query.spans[0].start[0]
        } else {
            &query.spans[0].end[0]
        };
        assert_eq!(edge.length, 200);
        assert_eq!(
            edge.distance_at_start,
            ExactRatio::integer(if start { 0 } else { 9 })
        );
    }
}

#[test]
fn bound_opaque_owner_suppression_keeps_a_distinct_inner_marker() {
    let plain = document(
        48_000,
        &["preserve"],
        vec![
            ("a", hold(150)),
            ("b", hold(150)),
            ("group", BeatNode::sequence("Input", vec![id("a"), id("b")])),
            (
                "preserve",
                retime("group", 200, 0, 300, PitchPolicy::Preserve),
            ),
        ],
    );
    let inner = with_editorial(&plain, "b", true, false, false);
    let bound = bind(
        &inner,
        &plain,
        "preserve",
        AudioClockRoot::ProjectRootRoundEven,
        Some(ExactRatio::integer(-10)),
    );
    let marked = with_editorial(&bound, "preserve", true, false, false);
    let original = RenderPlan::compile(&bound).unwrap();
    let plan = RenderPlan::compile(&marked).unwrap();
    assert_eq!(query(&plan, 10, 20).spans, query(&original, 10, 20).spans);
    let result = query(&plan, 110, 111);
    let edge = result.spans[0]
        .start
        .iter()
        .find(|edge| edge.editorial)
        .unwrap();
    assert_eq!(edge.at, ExactRatio::integer(100));
    assert_eq!(edge.distance_at_start, ExactRatio::integer(10));
    assert!(
        edge.origins
            .iter()
            .any(|origin| origin.instance.node == id("b"))
    );
    assert!(
        edge.origins
            .iter()
            .all(|origin| origin.instance.node != id("preserve"))
    );
}
