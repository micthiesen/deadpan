//! Ungroup inside Repeat contents: one explicit play isolates that occurrence
//! and ungroups its copy; All plays ungroups the shared definition. Both
//! distribute the group's effects with fresh wrapper identities. Captions over
//! a group land on its hosts.

use super::*;
use crate::{AudioTreatments, ClipGain, Framing, GainDb, OccurrenceEdit, PauseSite};

/// outer Repeat x3 of body [lead 2, grp [a 4, b 3], tail 2]. The group has a
/// camera path and gain; `b` has its own framing, so it needs a wrapper.
fn grouped() -> ProjectDocument {
    let pose = |x: i128| {
        Framing::static_pose(
            FramingPose::new(
                ExactRatio::new(x, 10).unwrap(),
                ExactRatio::new(1, 2).unwrap(),
                ExactRatio::new(3, 2).unwrap(),
            )
            .unwrap(),
        )
        .unwrap()
    };
    let mut group = BeatNode::sequence("Group", vec![node("a"), node("b")]);
    group.framing = Some(pose(4));
    group.audio_treatments = AudioTreatments::from_clip_gain(
        ClipGain::default().with_trim(GainDb::new(-6_000).unwrap()),
    );
    let mut b = hold(3);
    b.framing = Some(pose(6));
    let document = tree(
        &["outer"],
        vec![
            ("outer", repeated("outer", "body", 3)),
            (
                "body",
                BeatNode::sequence("body", vec![node("lead"), node("grp"), node("tail")]),
            ),
            ("lead", hold(2)),
            ("grp", group),
            ("a", hold(4)),
            ("b", b),
            ("tail", hold(2)),
        ],
    );
    document.validate().unwrap();
    document
}

fn scope(play: Option<u32>) -> SemanticContext {
    let presentation = InstancePath {
        node: node("grp"),
        repeats: vec![RepeatInstance {
            node: node("outer"),
            iteration: iteration("outer", 1),
        }],
    };
    SemanticContext {
        selected_child: Some(node("outer")),
        scoped: Some(SemanticScopedContext {
            root: node("outer"),
            target: ScopedNodeTarget {
                node: node("grp"),
                repeats: vec![RepeatEditStep {
                    repeat: node("outer"),
                    branch: play.map_or(RepeatEditBranch::Default, |ordinal| {
                        RepeatEditBranch::Play {
                            iteration: iteration("outer", ordinal),
                        }
                    }),
                }],
            },
            presentation,
        }),
        // Play 1 starts at 11; its group starts two frames later.
        ..context("root", 13)
    }
}

fn plan_ungroup(
    document: &ProjectDocument,
    context: &SemanticContext,
    instructions: Vec<SemanticInstruction>,
) -> Result<SemanticPlan, EditError> {
    plan_semantic_with_framing(
        document,
        context,
        &program(instructions),
        SemanticRegisterBank {
            entries: &BTreeMap::new(),
            version: 3,
        },
        revision("ungroup-outer"),
        super::super::allocate,
        no_original,
        |_| Err(speech_unavailable()),
        |_: &ProjectDocument, _: PauseSite| panic!("no pause"),
        None,
        state,
    )
}

fn only_leaf(plan: &SemanticPlan, before: &ProjectDocument) -> Command {
    let Command::Compound { transaction } = &plan.request.as_ref().unwrap().command else {
        panic!("one Compound")
    };
    assert_eq!(transaction.steps().len(), 1);
    transaction.steps()[0]
        .edit()
        .unwrap()
        .request(before)
        .command
}

fn replays_and_undoes(before: &ProjectDocument, plan: &SemanticPlan) {
    let request = plan.request.as_ref().unwrap();
    let replay = crate::replay_compound::<EditError>(before, request, |_| Ok(())).unwrap();
    assert_eq!(replay.document, plan.document);
    assert_eq!(replay.edit.inverse.apply(&plan.document).unwrap(), *before);
    let mapped = plan.context.scoped.as_ref().unwrap();
    assert!(
        mapped
            .target
            .matches_instance(&plan.document, &mapped.presentation)
            .unwrap()
    );
}

#[test]
fn one_play_ungroup_isolates_the_play_and_supplies_its_wrapper() {
    let before = grouped();
    assert_eq!(before.ungroup_requirements(&node("grp")).unwrap(), 1);
    let plan = plan_ungroup(&before, &scope(Some(1)), vec![SemanticInstruction::Ungroup]).unwrap();
    let Command::EditOccurrence {
        instance,
        edit: OccurrenceEdit::Ungroup { wrappers },
        identities,
    } = only_leaf(&plan, &before)
    else {
        panic!("one occurrence Ungroup")
    };
    assert_eq!(instance.node, node("grp"));
    assert_eq!(wrappers.len(), 1);
    assert!(!identities.nodes.contains(&wrappers[0]));
    replays_and_undoes(&before, &plan);
    // The definition keeps its group for plays 0 and 2.
    assert_eq!(
        plan.document.nodes()[&node("grp")],
        before.nodes()[&node("grp")]
    );
    let copy = plan.document.overrides()[&node("outer")]
        .get(&iteration("outer", 1))
        .unwrap();
    let children: Vec<_> = plan.document.children(copy).cloned().collect();
    assert_eq!(children.len(), 4, "lead, a, wrapped b, tail");
    let wrapper = &plan.document.nodes()[&wrappers[0]];
    assert_eq!(children[2], wrappers[0]);
    assert!(wrapper.framing.is_some() && !wrapper.audio_treatments.is_empty());
    let first = &plan.document.nodes()[&children[1]];
    assert!(first.framing.is_some() && !first.audio_treatments.is_empty());
    assert_eq!(
        plan.document.duration().unwrap(),
        before.duration().unwrap()
    );
    // The first promoted child stays selected at the group's start.
    let mapped = plan.context.scoped.as_ref().unwrap();
    assert_eq!(mapped.target.node, children[1]);
    assert_eq!(plan.context.cursor, ProjectFrame(13));
}

#[test]
fn all_plays_ungroup_changes_the_shared_definition_with_its_wrapper() {
    let before = grouped();
    let plan = plan_ungroup(&before, &scope(None), vec![SemanticInstruction::Ungroup]).unwrap();
    assert_eq!(
        only_leaf(&plan, &before),
        Command::Ungroup {
            node: node("grp"),
            wrappers: vec![node("ungroup-0-0")],
        }
    );
    replays_and_undoes(&before, &plan);
    assert!(!plan.document.nodes().contains_key(&node("grp")));
    assert_eq!(
        plan.document
            .children(&node("body"))
            .cloned()
            .collect::<Vec<_>>(),
        vec![node("lead"), node("a"), node("ungroup-0-0"), node("tail")]
    );
    let mapped = plan.context.scoped.as_ref().unwrap();
    assert_eq!(mapped.target.node, node("a"));
    assert_eq!(mapped.target.repeats[0].branch, RepeatEditBranch::Default);
}

#[test]
fn all_plays_admits_only_ungroup_and_effects_on_the_mix_still_refuse() {
    let before = grouped();
    let error = plan_ungroup(&before, &scope(None), vec![SemanticInstruction::Split]).unwrap_err();
    assert!(
        error
            .message
            .starts_with("All plays contents support only ungrouping"),
        "{error}"
    );
    let mut saturated = before.clone();
    let group = saturated.nodes.get_mut(&node("grp")).unwrap();
    group.audio_treatments = group
        .audio_treatments
        .clone()
        .with_saturation(Some(
            crate::Saturation::new(GainDb::new(6_000).unwrap()).unwrap(),
        ))
        .unwrap();
    saturated.validate().unwrap();
    for play in [Some(1), None] {
        let error =
            plan_ungroup(&saturated, &scope(play), vec![SemanticInstruction::Ungroup]).unwrap_err();
        assert_eq!(
            error.message,
            "ungroup cannot keep this group's saturation, which acts on its children's combined mix and has no per-child equivalent; remove the group's saturation first, or keep the group"
        );
    }
}

#[test]
fn a_caption_on_a_selected_group_lands_on_each_host_in_one_apply() {
    let before = tree(
        &["grp"],
        vec![
            (
                "grp",
                BeatNode::sequence("Group", vec![node("a"), node("b")]),
            ),
            ("a", hold(4)),
            ("b", hold(3)),
        ],
    );
    let plan = plan_ungroup(
        &before,
        &SemanticContext {
            selected_child: Some(node("grp")),
            ..context("root", 0)
        },
        vec![SemanticInstruction::SetCaption {
            text: "Both beats".into(),
            placement: crate::CaptionPlacement::Bottom,
            delay: Some(crate::PauseLength::Frames {
                frames: NonZeroU32::new(2).unwrap(),
            }),
            reveal: None,
        }],
    )
    .unwrap();
    let ranges = |host: &str| {
        plan.document.nodes()[&node(host)]
            .captions
            .iter()
            .map(|caption| (caption.range.start().0, caption.range.end().0))
            .collect::<Vec<_>>()
    };
    assert_eq!(ranges("a"), [(2, 4)]);
    assert_eq!(ranges("b"), [(0, 3)]);
    let request = plan.request.as_ref().unwrap();
    let replay = crate::replay_compound::<EditError>(&before, request, |_| Ok(())).unwrap();
    assert_eq!(replay.edit.inverse.apply(&plan.document).unwrap(), before);
}
