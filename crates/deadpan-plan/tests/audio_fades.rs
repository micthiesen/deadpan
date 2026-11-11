use std::collections::BTreeMap;

use deadpan_core::*;
use deadpan_plan::{AudioFadeQuery, RenderPlan};

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
        captions: Vec::new(),
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

/// `:edge plays` on a Repeat of a split fragment: the fragment's sides add no
/// fade until marked editorial; marked, Automatic fades each play seam and
/// Hard cuts it.
#[test]
fn marked_fragment_play_seams_fade_or_cut_by_their_policy() {
    let fragment = BeatNode {
        label: "Fragment".into(),
        kind: NodeKind::Retime {
            child: id("voice"),
            duration: frames(4),
            mapping: FrameRange::new(ProjectFrame(2), ProjectFrame(6)).unwrap(),
            pitch: PitchPolicy::Preserve,
            purpose: RetimePurpose::Partition,
        },
        ..hold(1)
    };
    let base = document(
        30,
        &["fragment"],
        vec![("voice", hold(8)), ("fragment", fragment)],
    );
    let edit = |document: &ProjectDocument, revision: &str, command: Command| {
        apply(
            document,
            &CommandRequest {
                project_id: document.project_id().clone(),
                expected_revision: document.revision_id().clone(),
                new_revision: RevisionId::new(revision).unwrap(),
                command,
            },
        )
        .unwrap()
        .forward
        .apply(document)
        .unwrap()
    };
    let looped = edit(
        &base,
        "loop",
        Command::WrapRepeat {
            node: id("fragment"),
            id: id("loop"),
            plays: 2,
            gap: None,
            anchor_policy: WrapAnchorPolicy::First,
        },
    );
    // The seam between the plays is Edit frame 4 (sample 6,400 at 30 fps):
    // each edge there is (editorial, any origin Hard).
    let seam = |document: &ProjectDocument| -> Vec<(bool, bool)> {
        let plan = RenderPlan::compile(document).unwrap();
        query(&plan, 0, 12_800)
            .spans
            .iter()
            .flat_map(|span| span.start.iter().chain(&span.end))
            .filter(|edge| edge.at == ExactRatio::integer(4))
            .map(|edge| {
                (
                    edge.editorial,
                    edge.origins
                        .iter()
                        .any(|origin| origin.policy == AudioEdgePolicy::Hard),
                )
            })
            .collect()
    };
    let unmarked = seam(&looped);
    let marked = edit(
        &looped,
        "marked",
        Command::SetEditorialEdges {
            node: id("fragment"),
            edges: AudioEditorialEdges {
                start: true,
                end: true,
            },
        },
    );
    let faded = seam(&marked);
    let mut hard = marked.clone();
    for (revision, edge) in [
        ("hard-start", AudioBoundaryKind::NodeStart),
        ("hard-end", AudioBoundaryKind::NodeEnd),
    ] {
        hard = edit(
            &hard,
            revision,
            Command::SetAudioEdge {
                node: id("fragment"),
                edge,
                policy: AudioEdgePolicy::Hard,
            },
        );
    }
    let cut = seam(&hard);
    // Unmarked fragment sides add no seam edge; marked ones fade both sides
    // of the seam automatically, and Hard suppresses that fade.
    assert!(unmarked.is_empty(), "{unmarked:?}");
    assert_eq!(faded, [(true, false), (true, false)]);
    assert_eq!(cut, [(true, true), (true, true)]);
    // Marking needs a fragment.
    assert!(
        apply(
            &base,
            &CommandRequest {
                project_id: base.project_id().clone(),
                expected_revision: base.revision_id().clone(),
                new_revision: RevisionId::new("wrong").unwrap(),
                command: Command::SetEditorialEdges {
                    node: id("voice"),
                    edges: AudioEditorialEdges {
                        start: true,
                        end: false,
                    },
                },
            },
        )
        .is_err()
    );
}
