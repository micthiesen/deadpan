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
            audio: HoldAudio::Silence,
        },
    )
}
fn retime(child: &str, length: i64, start: i64, end: i64, pitch: PitchPolicy) -> BeatNode {
    BeatNode {
        label: "Timing".into(),
        audio_edges: Default::default(),
        kind: NodeKind::Retime {
            child: id(child),
            duration: frames(length),
            mapping: FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap(),
            pitch,
            purpose: RetimePurpose::Edit,
        },
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
        lattice: AudioPlacementTemplate {
            reference: AudioReferenceClock {
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
                    span.length,
                    span.progress_at_start
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
fn selected_origin_point_clock_retains_ceil_width_after_exposure_at_root() {
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
    assert_eq!(
        samples(query(&plan, 2, 4)),
        vec![(2, ExactRatio::ZERO), (2, ExactRatio::ONE)]
    );
    let outside = query(&plan, 0, 2);
    assert_eq!(outside.spans[0].length, 0);
    assert!(outside.spans[0].boundaries.start.is_empty());

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
    assert_eq!(whole.spans[0].length, 2);
    assert_eq!(whole.spans[0].progress_at_start, ratio(3, 16));
    assert_eq!(whole.spans[1].length, 1);
    assert_eq!(whole.spans[1].progress_at_start, ratio(11, 16));
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
    assert_eq!(result.spans[0].length, 2);
    assert_eq!(result.spans[0].progress_at_start, ExactRatio::ZERO);

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
            .boundaries
            .start
            .iter()
            .any(|origin| origin.policy == AudioEdgePolicy::Hard)
    );
    assert!(
        untrimmed.spans[0]
            .boundaries
            .end
            .iter()
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
    assert_eq!(fades.spans[0].length, 2);
    assert!(
        fades.spans[0]
            .boundaries
            .start
            .iter()
            .all(|origin| origin.policy != AudioEdgePolicy::Hard)
    );
    assert!(
        fades.spans[0]
            .boundaries
            .end
            .iter()
            .all(|origin| origin.policy != AudioEdgePolicy::Hard)
    );
    assert!(
        fades.spans[0]
            .boundaries
            .start
            .iter()
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
            audio_edges: Default::default(),
            kind: NodeKind::Source {
                source: SourceNode {
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
    assert_eq!(query(&plan, 0, 1).spans[0].length, 1);
    let faster = bind(
        &source_document(true),
        &original,
        "voice",
        AudioClockRoot::ProjectRootRoundEven,
        None,
    );
    let plan = RenderPlan::compile(&faster).unwrap();
    let fade = query(&plan, 0, 1);
    assert_eq!(fade.spans[0].length, 0);
    assert!(!fade.spans[0].boundaries.start.is_empty());
    assert!(!fade.spans[0].boundaries.end.is_empty());
}
