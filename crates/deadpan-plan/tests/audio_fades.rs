use std::collections::BTreeMap;

use deadpan_core::*;
use deadpan_plan::{AudioFadeQuery, RenderPlan};

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
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
fn query(plan: &RenderPlan, start: i64, end: i64) -> AudioFadeQuery {
    plan.audio_fades(AudioSample(start)..AudioSample(end), Default::default())
        .unwrap()
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
    // A play seam is a cut, so the fragment's sides fade on both sides of
    // the seam whether or not they were marked; Hard suppresses that fade.
    assert_eq!(unmarked, [(true, false), (true, false)]);
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
