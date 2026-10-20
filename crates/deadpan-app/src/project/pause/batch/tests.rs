use super::*;
use deadpan_core::{
    BeatNode, ColorPolicy, FrameRate, HoldAudio, HoldRecipe, HoldVideo, IterationOrder, NodeId,
    PlayOverride, PlayOverrides, PresentationBasis, ProjectId, RepeatEditStep, RevisionId, Subtree,
};
use std::collections::BTreeMap;

fn node(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}

fn revision(name: &str) -> RevisionId {
    RevisionId::new(name).unwrap()
}

fn iteration(name: &str, ordinal: u32) -> IterationId {
    IterationId {
        allocation: revision(name),
        ordinal,
    }
}

fn play(name: &str, ordinal: u32) -> RepeatEditBranch {
    RepeatEditBranch::Play {
        iteration: iteration(name, ordinal),
    }
}

fn fixture_insert(before: &ProjectDocument, request: CommandRequest) -> ProjectDocument {
    let Command::Insert { subtree, .. } = request.command else {
        panic!("fixture insertion")
    };
    // These fixtures describe existing copied Repeat branches, whose stable
    // iteration identities agree. Importing a new Subtree deliberately assigns
    // fresh identities, so construct the validated stored document directly.
    let mut nodes: BTreeMap<_, _> = before
        .nodes()
        .iter()
        .map(|(id, beat)| (id.clone(), beat.clone()))
        .collect();
    let NodeKind::Sequence { children } = &mut nodes.get_mut(before.root()).unwrap().kind else {
        panic!("fixture root Sequence")
    };
    children.push(subtree.root);
    nodes.extend(subtree.nodes);
    let mut overrides = before.overrides().clone();
    overrides.extend(subtree.overrides);
    let mut json: serde_json::Value = serde_json::from_str(&before.to_json().unwrap()).unwrap();
    json["nodes"] = serde_json::to_value(nodes).unwrap();
    json["overrides"] = serde_json::to_value(overrides).unwrap();
    json["revision_id"] = serde_json::to_value(request.new_revision).unwrap();
    ProjectDocument::from_json(&serde_json::to_string(&json).unwrap()).unwrap()
}

fn fixture() -> ProjectDocument {
    let before = ProjectDocument::new(
        ProjectId::new("pause-batch").unwrap(),
        revision("initial"),
        PresentationBasis {
            width: 64,
            height: 36,
            frame_rate: FrameRate::new(30, 1).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )
    .unwrap();
    let repeat = |name: &str, child: &str, allocation: &str| {
        let mut beat = BeatNode::sequence(name, Vec::new());
        beat.kind = NodeKind::Repeat {
            child: node(child),
            iterations: IterationOrder::new(revision(allocation), 3).unwrap(),
            gap: None,
            escalation: None,
        };
        beat
    };
    let hold = |name: &str| {
        BeatNode::hold(
            name,
            HoldRecipe {
                duration: FrameDuration::new(10).unwrap(),
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
                picture_context: None,
            },
        )
    };
    let request = CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: revision("fixture"),
        command: Command::Insert {
            parent: node("root"),
            index: 0,
            subtree: Subtree {
                root: node("outer"),
                nodes: BTreeMap::from([
                    (node("outer"), repeat("outer", "inner", "outer")),
                    (node("inner"), repeat("inner", "a", "inner")),
                    (node("a"), hold("a")),
                    (
                        node("inner-owned"),
                        repeat("inner-owned", "a-owned", "inner"),
                    ),
                    (node("a-owned"), hold("a-owned")),
                    (node("a-override"), hold("a-override")),
                ]),
                overrides: BTreeMap::from([
                    (
                        node("outer"),
                        PlayOverrides::try_from(vec![PlayOverride {
                            iteration: iteration("outer", 1),
                            root: node("inner-owned"),
                        }])
                        .unwrap(),
                    ),
                    (
                        node("inner-owned"),
                        PlayOverrides::try_from(vec![PlayOverride {
                            iteration: iteration("inner", 0),
                            root: node("a-override"),
                        }])
                        .unwrap(),
                    ),
                ]),
                gap_overrides: BTreeMap::new(),
            },
        },
    };
    fixture_insert(&before, request)
}

fn captured(document: &ProjectDocument) -> scoped::Target {
    scoped::Target {
        session: 1,
        navigation: 0,
        project: document.project_id().clone(),
        revision: document.revision_id().clone(),
        scope: Default::default(),
        root: node("outer"),
        target: ScopedNodeTarget {
            node: node("a"),
            repeats: vec![
                RepeatEditStep {
                    repeat: node("outer"),
                    branch: play("outer", 0),
                },
                RepeatEditStep {
                    repeat: node("inner"),
                    branch: RepeatEditBranch::Default,
                },
            ],
        },
        presentation: Some(InstancePath {
            node: node("a"),
            repeats: vec![
                RepeatInstance {
                    node: node("outer"),
                    iteration: iteration("outer", 0),
                },
                RepeatInstance {
                    node: node("inner"),
                    iteration: iteration("inner", 0),
                },
            ],
        }),
        cursor: ProjectFrame(4),
        also: Vec::new(),
    }
}

#[test]
fn selected_pause_batch_requires_one_uniform_varied_level_and_distinct_plays() {
    let document = fixture();
    let mut capture = captured(&document);
    let other = scoped::retarget(&document, &capture.target, 0, &iteration("outer", 1)).unwrap();
    capture.also.push(other.clone());
    assert_eq!(uniform_level(&document, &capture).unwrap(), 0);
    capture.also.push(other);
    assert!(uniform_level(&document, &capture).is_err());

    // These supports are disjoint in the original document, but the second
    // shared edit would miss the subtree cloned for the first outer Play.
    capture = captured(&document);
    capture.target.repeats[1].branch = play("inner", 0);
    let mut heterogeneous = capture.target.clone();
    heterogeneous.repeats[0].branch = RepeatEditBranch::Default;
    heterogeneous.repeats[1].branch = play("inner", 1);
    heterogeneous.validate(&document).unwrap();
    capture.also = vec![heterogeneous];
    assert!(uniform_level(&document, &capture).is_err());

    // Different inferred inner plays still have disjoint support when the
    // same outer Repeat explicitly selects distinct plays under one prefix.
    capture = captured(&document);
    capture.target.repeats[1].branch = play("inner", 1);
    let mut other =
        scoped::retarget(&document, &capture.target, 0, &iteration("outer", 1)).unwrap();
    other.repeats[1].branch = play("inner", 2);
    other.validate(&document).unwrap();
    capture.also = vec![other];
    assert_eq!(uniform_level(&document, &capture).unwrap(), 0);
}

#[test]
fn selected_pause_batch_retains_holds_in_different_descendant_slots() {
    let before = fixture();
    let mut repeat = BeatNode::sequence("retained", Vec::new());
    repeat.kind = NodeKind::Repeat {
        child: node("body-0"),
        iterations: IterationOrder::new(revision("retained"), 2).unwrap(),
        gap: None,
        escalation: None,
    };
    let hold = |name: &str| {
        BeatNode::hold(
            name,
            HoldRecipe {
                duration: FrameDuration::new(3).unwrap(),
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
                picture_context: None,
            },
        )
    };
    let request = CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: revision("retained-fixture"),
        command: Command::Insert {
            parent: node("root"),
            index: 1,
            subtree: Subtree {
                root: node("retained"),
                nodes: BTreeMap::from([
                    (node("retained"), repeat),
                    (
                        node("body-0"),
                        BeatNode::sequence("body-0", vec![node("lead-0"), node("pause-0")]),
                    ),
                    (
                        node("body-1"),
                        BeatNode::sequence(
                            "body-1",
                            vec![node("lead-1"), node("fragment-1"), node("pause-1")],
                        ),
                    ),
                    (node("lead-0"), hold("lead-0")),
                    (node("pause-0"), hold("pause-0")),
                    (node("lead-1"), hold("lead-1")),
                    (node("fragment-1"), hold("fragment-1")),
                    (node("pause-1"), hold("pause-1")),
                ]),
                overrides: BTreeMap::from([(
                    node("retained"),
                    PlayOverrides::try_from(vec![PlayOverride {
                        iteration: iteration("retained", 1),
                        root: node("body-1"),
                    }])
                    .unwrap(),
                )]),
                gap_overrides: BTreeMap::new(),
            },
        },
    };
    let document = fixture_insert(&before, request);
    let mut capture = captured(&document);
    capture.root = node("retained");
    capture.target = ScopedNodeTarget {
        node: node("pause-0"),
        repeats: vec![RepeatEditStep {
            repeat: node("retained"),
            branch: play("retained", 0),
        }],
    };
    capture.also = vec![ScopedNodeTarget {
        node: node("pause-1"),
        repeats: vec![RepeatEditStep {
            repeat: node("retained"),
            branch: play("retained", 1),
        }],
    }];
    assert_ne!(
        scoped::retarget(&document, &capture.target, 0, &iteration("retained", 1)).unwrap(),
        capture.also[0],
        "the old positional proof finds the fragment instead of the retained Hold"
    );
    assert_eq!(uniform_level(&document, &capture).unwrap(), 0);
}

#[test]
fn selected_pause_batch_uses_a_visible_default_counterpart_and_exact_local_boundary() {
    let document = fixture();
    let capture = captured(&document);
    let primary = capture.presentation.unwrap();
    let other = scoped::retarget(&document, &capture.target, 0, &iteration("outer", 1)).unwrap();
    let counterpart = presentation(&document, &other, &primary).unwrap();
    assert_eq!(counterpart.node, node("a-owned"));
    assert_eq!(counterpart.repeats[0].iteration, iteration("outer", 1));
    assert_eq!(counterpart.repeats[1].iteration, iteration("inner", 1));
    let boundary = resolve(&document, &counterpart, ExactRatio::integer(4)).unwrap();
    assert_eq!(integral(&boundary).unwrap(), ProjectFrame(44));
    let fractional = resolve(&document, &counterpart, ExactRatio::new(1, 2).unwrap()).unwrap();
    assert!(integral(&fractional).is_err());
}
