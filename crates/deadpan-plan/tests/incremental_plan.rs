//! Fragment reuse (`RenderPlan::compile_from`) against full compilation.
//!
//! Every `compile_from` here runs under `with_full_compile_oracle`, which
//! compiles the same document from scratch and panics unless both plans are
//! identical in every compiled value. Edits come from the real core reducer,
//! including their inverse patches (Undo).

use std::collections::BTreeMap;

use deadpan_core::*;
use deadpan_plan::{RenderPlan, with_full_compile_oracle};

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}
fn duration(frames: i64) -> FrameDuration {
    FrameDuration::new(frames).unwrap()
}
fn clock() -> SourceTimeBase {
    SourceTimeBase::new(1, 30_000).unwrap()
}
fn span(start: i64, end: i64) -> SourceSpan {
    SourceSpan::new(
        SourceTimestamp {
            ticks: start,
            time_base: clock(),
        },
        SourceTimestamp {
            ticks: end,
            time_base: clock(),
        },
    )
    .unwrap()
}
fn plain(label: &str, kind: NodeKind) -> BeatNode {
    BeatNode {
        framing: None,
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        label: label.into(),
        kind,
        cutaways: Vec::new(),
        captions: Vec::new(),
    }
}
fn source(frames: i64, start: i64, end: i64) -> BeatNode {
    plain(
        "Original speech",
        NodeKind::Source {
            source: SourceNode {
                edit_window: None,
                duration: duration(frames),
                video_mapping: SourceVideoMapping::FitBeat,
                video: SourceVideo::Stream {
                    asset: AssetId::new("video").unwrap(),
                    span: span(start, end),
                },
                audio: None,
                link: LinkRelation::Independent,
                audio_mapping: SourceAudioMapping::FitBeat,
                audio_offset: AudioSample(0),
            },
        },
    )
}
fn hold(frames: i64) -> BeatNode {
    BeatNode::hold(
        "Pause",
        HoldRecipe {
            duration: duration(frames),
            video: HoldVideo::Freeze {
                asset: AssetId::new("video").unwrap(),
                timestamp: SourceTimestamp {
                    ticks: 2_002,
                    time_base: clock(),
                },
            },
            picture_context: None,
            audio: HoldAudio::Silence,
        },
    )
}
fn repeat(child: &str, plays: u32, allocation: &str) -> BeatNode {
    plain(
        allocation,
        NodeKind::Repeat {
            child: id(child),
            iterations: IterationOrder::new(revision(allocation), plays).unwrap(),
            gap: Some(HoldRecipe {
                duration: duration(2),
                video: HoldVideo::Background,
                picture_context: None,
                audio: HoldAudio::Silence,
            }),
            escalation: None,
        },
    )
}
fn retime(child: &str, frames: i64, start: i64, end: i64) -> BeatNode {
    plain(
        "Exact retime",
        NodeKind::Retime {
            purpose: RetimePurpose::Edit,
            child: id(child),
            duration: duration(frames),
            mapping: FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap(),
            pitch: PitchPolicy::Preserve,
        },
    )
}

/// A root Sequence of sources and holds with a nested group, a Repeat and a
/// Retime, so edits reach every compiled fragment kind and several depths.
fn initial() -> ProjectDocument {
    let empty = ProjectDocument::new(
        ProjectId::new("project").unwrap(),
        revision("initial"),
        PresentationBasis {
            width: 1920,
            height: 1080,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut nodes = BTreeMap::from([
        (
            id("root"),
            BeatNode::sequence(
                "Root",
                vec![id("a"), id("group"), id("loop"), id("slow"), id("e")],
            ),
        ),
        (id("a"), source(12, 0, 12_012)),
        (
            id("group"),
            BeatNode::sequence("Group", vec![id("b"), id("pause"), id("c")]),
        ),
        (id("b"), source(9, 12_012, 21_021)),
        (id("pause"), hold(5)),
        (id("c"), source(7, 21_021, 28_028)),
        (id("loop"), repeat("d", 3, "loop")),
        (id("d"), source(4, 30_030, 34_034)),
        (id("slow"), retime("f", 12, 0, 6)),
        (id("f"), source(6, 40_040, 46_046)),
        (id("e"), hold(3)),
    ]);
    nodes.insert(id("root"), nodes[&id("root")].clone());
    let mut wire = serde_json::to_value(empty).unwrap();
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    wire["assets"] = serde_json::to_value(BTreeMap::from([(
        AssetId::new("video").unwrap(),
        AssetRecord {
            source_qualification: None,
            label: "Original source".into(),
            content_hash: "a".repeat(64),
            video: Some(span(-1000, 100_000)),
            audio: None,
            still_image: false,
            frame_count: None,
        },
    )]))
    .unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

struct Lcg(u64);

impl Lcg {
    fn below(&mut self, bound: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        usize::try_from((self.0 >> 33) % u64::try_from(bound).unwrap()).unwrap()
    }
}

fn pick<'a>(random: &mut Lcg, ids: &'a [NodeId]) -> Option<&'a NodeId> {
    (!ids.is_empty()).then(|| &ids[random.below(ids.len())])
}

/// One random command over current identities. Many are refused by the
/// reducer (invalid targets); the caller skips those.
fn command(document: &ProjectDocument, random: &mut Lcg, step: usize) -> Option<Command> {
    let of = |predicate: fn(&NodeKind) -> bool| -> Vec<NodeId> {
        document
            .nodes()
            .iter()
            .filter(|(id, node)| *id != document.root() && predicate(&node.kind))
            .map(|(id, _)| id.clone())
            .collect()
    };
    let all = of(|_| true);
    let sequences: Vec<NodeId> = document
        .nodes()
        .iter()
        .filter(|(_, node)| matches!(node.kind, NodeKind::Sequence { .. }))
        .map(|(id, _)| id.clone())
        .collect();
    let leaves = of(|kind| matches!(kind, NodeKind::Source { .. } | NodeKind::Hold { .. }));
    let holds = of(|kind| matches!(kind, NodeKind::Hold { .. }));
    let repeats = of(|kind| matches!(kind, NodeKind::Repeat { .. }));
    let retimes = of(|kind| matches!(kind, NodeKind::Retime { .. }));
    let fresh = |name: &str| id(&format!("s{step}-{name}"));
    let identities = || SplitIdentities {
        nodes: (0..32).map(|index| fresh(&index.to_string())).collect(),
    };
    Some(match random.below(13) {
        0 | 1 => Command::Split {
            node: pick(random, &leaves)?.clone(),
            at: duration(i64::try_from(1 + random.below(5)).unwrap()),
            identities: identities(),
        },
        2 => Command::Rename {
            node: pick(random, &all)?.clone(),
            label: format!("label {step}"),
        },
        3 | 4 => Command::SetHoldDuration {
            node: pick(random, &holds)?.clone(),
            duration: duration(i64::try_from(1 + random.below(9)).unwrap()),
        },
        5 => Command::WrapRepeat {
            node: pick(random, &leaves)?.clone(),
            id: fresh("repeat"),
            plays: u32::try_from(2 + random.below(3)).unwrap(),
            gap: None,
            anchor_policy: WrapAnchorPolicy::First,
        },
        6 => Command::SetRepeat {
            node: pick(random, &repeats)?.clone(),
            plays: u32::try_from(1 + random.below(4)).unwrap(),
            gap: None,
        },
        7 => Command::InsertPlays {
            node: pick(random, &repeats)?.clone(),
            index: 0,
            count: 1,
        },
        8 => Command::Delete {
            node: pick(random, &all)?.clone(),
        },
        9 => {
            let parent = pick(random, &sequences)?.clone();
            let NodeKind::Sequence { children } = &document.nodes()[&parent].kind else {
                return None;
            };
            Command::Move {
                node: pick(random, &all)?.clone(),
                index: random.below(children.len() + 1),
                parent,
            }
        }
        10 => {
            let parent = pick(random, &sequences)?.clone();
            let NodeKind::Sequence { children } = &document.nodes()[&parent].kind else {
                return None;
            };
            let start = random.below(children.len().max(1));
            Command::Group {
                end: start + 1 + random.below(children.len().saturating_sub(start).max(1)),
                start,
                parent,
                id: fresh("group"),
                label: "Group".into(),
            }
        }
        11 => Command::SetRetime {
            node: pick(random, &retimes)?.clone(),
            duration: duration(i64::try_from(2 + random.below(20)).unwrap()),
            pitch: PitchPolicy::Preserve,
        },
        _ => Command::SetAudioEdge {
            node: pick(random, &all)?.clone(),
            edge: AudioBoundaryKind::NodeStart,
            policy: AudioEdgePolicy::Hard,
        },
    })
}

fn transact(
    document: &ProjectDocument,
    command: Command,
    name: &str,
) -> Option<(ProjectDocument, ProjectDocument)> {
    let transaction = apply(
        document,
        &CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: revision(name),
            command,
        },
    )
    .ok()?;
    let after = transaction.forward.apply(document).ok()?;
    let undone = transaction.inverse.apply(&after).ok()?;
    Some((after, undone))
}

#[test]
fn reused_fragments_equal_a_full_compilation_over_random_edits_and_undo() {
    with_full_compile_oracle(|| {
        let unrelated = RenderPlan::compile(&initial()).unwrap();
        let mut committed = 0;
        let mut reused = 0;
        for seed in 1..=6_u64 {
            let mut random = Lcg(seed);
            let mut document = initial();
            let mut plan = RenderPlan::compile(&document).unwrap();
            let mut history = vec![(document.clone(), plan.clone())];
            for step in 0..60 {
                let Some(command) = command(&document, &mut random, step) else {
                    continue;
                };
                let name = format!("r{seed}-{step}");
                let Some((after, undone)) = transact(&document, command, &name) else {
                    continue;
                };
                committed += 1;
                // Undo from the new plan, then redo from the undone plan.
                let next = RenderPlan::compile_from(&plan, &after).unwrap();
                let back = RenderPlan::compile_from(&next, &undone).unwrap();
                assert_eq!(back.duration(), plan.duration());
                let redo = RenderPlan::compile_from(&back, &after).unwrap();
                reused += next.reused_fragments() + redo.reused_fragments();
                // Any older or unrelated plan is an equally valid base.
                let older = &history[random.below(history.len())].1;
                RenderPlan::compile_from(older, &after).unwrap();
                RenderPlan::compile_from(&unrelated, &after).unwrap();
                document = after;
                plan = redo;
                history.push((document.clone(), plan.clone()));
            }
        }
        assert!(committed >= 100, "only {committed} edits were admitted");
        assert!(reused > 0);
    });
}

#[test]
fn a_parameter_edit_recompiles_only_its_node_and_a_resize_its_ancestors() {
    with_full_compile_oracle(|| {
        let document = initial();
        let plan = RenderPlan::compile(&document).unwrap();
        assert_eq!(plan.reused_fragments(), 0);
        let nodes = document.nodes().len();

        // A label leaves every extent unchanged: only that node recompiles.
        let (renamed, _) = transact(
            &document,
            Command::Rename {
                node: id("pause"),
                label: "Renamed".into(),
            },
            "renamed",
        )
        .unwrap();
        let next = RenderPlan::compile_from(&plan, &renamed).unwrap();
        assert_eq!(next.reused_fragments(), nodes - 1);

        // A resized Hold changes its own extent and its parents' prefix
        // entries: the Hold, its group and the root recompile.
        let (resized, _) = transact(
            &renamed,
            Command::SetHoldDuration {
                node: id("pause"),
                duration: duration(11),
            },
            "resized",
        )
        .unwrap();
        let resized_plan = RenderPlan::compile_from(&next, &resized).unwrap();
        assert_eq!(resized_plan.reused_fragments(), nodes - 3);
        assert_eq!(
            resized_plan.duration().frames(),
            next.duration().frames() + 6
        );

        // The same document reuses everything.
        let again = RenderPlan::compile_from(&resized_plan, &resized).unwrap();
        assert_eq!(again.reused_fragments(), nodes);
    });
}

#[test]
fn the_oracle_comparison_sees_a_change_that_keeps_every_duration() {
    // The oracle compares complete Debug renderings. A change to one leaf's
    // compiled value, with every duration and identity kept, must show.
    let document = initial();
    let (edged, _) = transact(
        &document,
        Command::SetAudioEdge {
            node: id("c"),
            edge: AudioBoundaryKind::NodeEnd,
            policy: AudioEdgePolicy::Hard,
        },
        "edged",
    )
    .unwrap();
    let mut wire: serde_json::Value = serde_json::from_str(&edged.to_json().unwrap()).unwrap();
    wire["revision_id"] = serde_json::json!("initial");
    let edged = ProjectDocument::from_json(&wire.to_string()).unwrap();
    assert_ne!(
        format!("{:?}", RenderPlan::compile(&document).unwrap()),
        format!("{:?}", RenderPlan::compile(&edged).unwrap()),
    );
}

#[test]
fn compensating_child_resizes_recompile_the_unchanged_parent() {
    // Growing one root child and shrinking another by the same amount keeps
    // the root's node and extent equal; its prefix entries still change.
    with_full_compile_oracle(|| {
        let document = initial();
        let plan = RenderPlan::compile(&document).unwrap();
        let (grown, _) = transact(
            &document,
            Command::SetHoldDuration {
                node: id("pause"),
                duration: duration(7),
            },
            "grown",
        )
        .unwrap();
        let (balanced, _) = transact(
            &grown,
            Command::SetHoldDuration {
                node: id("e"),
                duration: duration(1),
            },
            "balanced",
        )
        .unwrap();
        let next = RenderPlan::compile_from(&plan, &balanced).unwrap();
        assert_eq!(next.duration(), plan.duration());
        assert_eq!(next.reused_fragments(), document.nodes().len() - 4);
    });
}
