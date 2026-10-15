use super::*;
use crate::*;

fn id(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}
fn revision(name: &str) -> RevisionId {
    RevisionId::new(name).unwrap()
}
fn frames(n: i64) -> FrameDuration {
    FrameDuration::new(n).unwrap()
}
fn range(start: i64, end: i64) -> ExactFrameRange {
    ExactFrameRange::new(ExactRatio::integer(start), ExactRatio::integer(end)).unwrap()
}
fn target(name: &str) -> InstancePath {
    InstancePath {
        node: id(name),
        repeats: vec![],
    }
}
fn run(destination: ExactFrameRange, source: ExactFrameRange, name: &str) -> PreserveInputRun {
    PreserveInputRun {
        destination,
        source,
        provider: PreserveInputProvider::Current {
            target: target(name),
        },
    }
}
fn hold(n: i64) -> BeatNode {
    BeatNode::hold(
        "Hold",
        HoldRecipe {
            duration: frames(n),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
            picture_context: None,
        },
    )
}
fn stage(child: &str, input: i64, output: i64) -> BeatNode {
    let mut node = BeatNode::sequence("Preserve", vec![]);
    node.kind = NodeKind::Retime {
        child: id(child),
        duration: frames(output),
        mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(input)).unwrap(),
        pitch: PitchPolicy::Preserve,
        purpose: RetimePurpose::Edit,
    };
    node
}
fn projection() -> PreserveInputProjection {
    PreserveInputProjection::new(vec![
        run(range(0, 2), range(0, 2), "left"),
        run(range(2, 4), range(0, 2), "right"),
    ])
    .unwrap()
}
fn document() -> ProjectDocument {
    let mut doc = ProjectDocument::new(
        ProjectId::new("owned").unwrap(),
        revision("before"),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    doc.nodes = BTreeMap::from([
        (
            id("root"),
            BeatNode::sequence("Root", vec![id("stage"), id("outside")]),
        ),
        (id("stage"), stage("group", 4, 2)),
        (
            id("group"),
            BeatNode::sequence("Group", vec![id("left"), id("pause"), id("right")]),
        ),
        (id("left"), hold(4)),
        (id("right"), hold(4)),
        (id("pause"), hold(2)),
        (id("outside"), hold(2)),
    ]);
    doc.derived_clocks.insert(
        id("stage"),
        DerivedRetimeClock {
            extent: ExactRatio::integer(3),
            child_start: ExactRatio::ZERO,
            child_end: ExactRatio::integer(10),
            preserve_input: Some(projection()),
        },
    );
    doc.validate().unwrap();
    doc
}
fn replace(doc: &mut ProjectDocument, runs: Vec<PreserveInputRun>) {
    doc.derived_clocks
        .get_mut(&id("stage"))
        .unwrap()
        .preserve_input = Some(PreserveInputProjection::new(runs).unwrap());
}
fn rejected(doc: &ProjectDocument, message: &str) {
    let error = doc.validate().unwrap_err();
    assert!(error.message.contains(message), "{error:?}");
    assert!(ProjectDocument::from_json(&serde_json::to_string(doc).unwrap()).is_err());
}

#[test]
fn preserve_input_partition_is_strict_bounded_and_roundtrips() {
    let input = projection();
    let wire = serde_json::to_string(&input).unwrap();
    assert_eq!(
        serde_json::from_str::<PreserveInputProjection>(&wire).unwrap(),
        input
    );
    assert!(
        serde_json::from_str::<PreserveInputProjection>(&wire.replacen("{", "{\"extra\":0,", 1))
            .is_err()
    );
    assert!(
        serde_json::from_str::<PreserveInputProjection>(&wire.replacen(
            "\"runs\":",
            "\"runs\":[],\"runs\":",
            1
        ))
        .is_err()
    );
    assert!(PreserveInputProjection::new(vec![]).is_err());
    for case in 0..6 {
        let mut runs = input.runs().to_vec();
        match case {
            0 => runs[0].destination.start = ExactRatio::ONE,
            1 => runs[1].destination.start = ExactRatio::ONE,
            2 => runs[0].source.start = ExactRatio::integer(-1),
            3 => runs[0].source.end = ExactRatio::ZERO,
            4 => runs[0].destination.end = ExactRatio::ZERO,
            5 => {
                let PreserveInputProvider::Current { target } = &mut runs[0].provider else {
                    panic!()
                };
                target.repeats = vec![
                    RepeatInstance {
                        node: id("repeat"),
                        iteration: IterationOrder::new(revision("plays"), 1)
                            .unwrap()
                            .at(0)
                            .unwrap()
                    };
                    MAX_DOCUMENT_DEPTH + 1
                ];
            }
            _ => unreachable!(),
        }
        assert!(PreserveInputProjection::new(runs).is_err(), "case {case}");
    }
    let runs: Vec<_> = (0..=MAX_PRESERVE_INPUT_RUNS)
        .map(|i| run(range(i as i64, i as i64 + 1), range(0, 1), "left"))
        .collect();
    assert!(PreserveInputProjection::new(runs.clone()).is_err());
    assert!(
        serde_json::from_value::<PreserveInputProjection>(serde_json::json!({"runs": runs}))
            .is_err()
    );
    assert!(PreserveInputProjection::new(runs[..MAX_PRESERVE_INPUT_RUNS].to_vec()).is_ok());
}

#[test]
fn preserve_input_refuses_escape_missing_owner_window_and_unqualified_intrinsic() {
    for name in ["root", "stage", "outside", "absent"] {
        let mut doc = document();
        let mut runs = projection().runs().to_vec();
        runs[0].provider = PreserveInputProvider::Current {
            target: target(name),
        };
        replace(&mut doc, runs);
        rejected(
            &doc,
            if name == "absent" {
                "missing"
            } else {
                "outside its owned subtree"
            },
        );
    }
    let mut doc = document();
    replace(&mut doc, vec![run(range(0, 3), range(0, 2), "left")]);
    rejected(&doc, "complete intrinsic operand");
    replace(&mut doc, vec![run(range(0, 4), range(0, 5), "left")]);
    rejected(&doc, "exceeds its provider");
    replace(
        &mut doc,
        vec![PreserveInputRun {
            provider: PreserveInputProvider::Intrinsic {
                target: target("left"),
            },
            ..run(range(0, 4), range(0, 2), "left")
        }],
    );
    rejected(&doc, "no owned projection");
    doc.derived_clocks
        .get_mut(&id("stage"))
        .unwrap()
        .preserve_input = None;
    rejected(&doc, "speed change with exact pause timing");
}

#[test]
fn preserve_input_requires_nonunity_preserve_and_explicit_nested_intrinsic() {
    let mut doc = document();
    doc.nodes.get_mut(&id("root")).unwrap().kind = NodeKind::Sequence {
        children: vec![id("outer"), id("outside")],
    };
    doc.nodes.insert(id("outer"), stage("stage", 2, 1));
    let input = PreserveInputProjection::new(vec![PreserveInputRun {
        destination: range(0, 2),
        source: range(0, 2),
        provider: PreserveInputProvider::Intrinsic {
            target: target("stage"),
        },
    }])
    .unwrap();
    doc.derived_clocks.insert(
        id("outer"),
        DerivedRetimeClock {
            extent: ExactRatio::integer(2),
            child_start: ExactRatio::ZERO,
            child_end: ExactRatio::integer(3),
            preserve_input: Some(input),
        },
    );
    doc.validate().unwrap();
    let mut invalid = doc.clone();
    invalid
        .derived_clocks
        .get_mut(&id("outer"))
        .unwrap()
        .preserve_input =
        Some(PreserveInputProjection::new(vec![run(range(0, 2), range(0, 2), "stage")]).unwrap());
    rejected(&invalid, "select intrinsic output explicitly");
    let mut invalid = doc.clone();
    invalid
        .derived_clocks
        .get_mut(&id("outer"))
        .unwrap()
        .preserve_input
        .as_mut()
        .unwrap()
        .runs[0]
        .source = range(0, 3);
    rejected(&invalid, "exceeds its provider");
    for pitch in [PitchPolicy::FollowSpeed, PitchPolicy::Preserve] {
        let mut invalid = doc.clone();
        let NodeKind::Retime {
            pitch: p, duration, ..
        } = &mut invalid.nodes.get_mut(&id("stage")).unwrap().kind
        else {
            panic!()
        };
        *p = pitch;
        if pitch == PitchPolicy::Preserve {
            *duration = frames(4);
        }
        rejected(&invalid, "nonunity Preserve recipe");
    }
}

#[test]
fn preserve_input_paths_select_actual_repeat_default_override_and_gap_branches() {
    let mut doc = document();
    let order = IterationOrder::new(revision("plays"), 2).unwrap();
    doc.nodes.get_mut(&id("group")).unwrap().kind = NodeKind::Sequence {
        children: vec![id("repeat"), id("pause"), id("right")],
    };
    let mut repeat = BeatNode::sequence("Repeat", vec![]);
    repeat.kind = NodeKind::Repeat {
        child: id("left"),
        iterations: order.clone(),
        gap: Some(HoldRecipe {
            duration: frames(2),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
            picture_context: None,
        }),
        escalation: None,
    };
    doc.nodes.insert(id("repeat"), repeat);
    doc.nodes.insert(id("override"), hold(4));
    doc.nodes.insert(id("gap"), hold(2));
    doc.overrides.insert(
        id("repeat"),
        PlayOverrides::try_from(vec![PlayOverride {
            iteration: order.at(1).unwrap(),
            root: id("override"),
        }])
        .unwrap(),
    );
    doc.gap_overrides.insert(
        id("repeat"),
        PlayOverrides::try_from(vec![PlayOverride {
            iteration: order.at(0).unwrap(),
            root: id("gap"),
        }])
        .unwrap(),
    );
    rejected(&doc, "path is incomplete");
    for (name, play) in [("left", 0), ("override", 1), ("gap", 0)] {
        let mut runs = projection().runs().to_vec();
        runs[0].provider = PreserveInputProvider::Current {
            target: InstancePath {
                node: id(name),
                repeats: vec![RepeatInstance {
                    node: id("repeat"),
                    iteration: order.at(play).unwrap(),
                }],
            },
        };
        replace(&mut doc, runs);
        doc.validate().unwrap();
        let mut bad = doc.clone();
        let PreserveInputProvider::Current { target } = &mut bad
            .derived_clocks
            .get_mut(&id("stage"))
            .unwrap()
            .preserve_input
            .as_mut()
            .unwrap()
            .runs[0]
            .provider
        else {
            panic!()
        };
        target.repeats[0].iteration = order.at(1 - play).unwrap();
        rejected(&bad, "selects another branch");
        target_reject_extra_path(&doc);
    }
}

fn target_reject_extra_path(doc: &ProjectDocument) {
    let mut bad = doc.clone();
    let PreserveInputProvider::Current { target } = &mut bad
        .derived_clocks
        .get_mut(&id("stage"))
        .unwrap()
        .preserve_input
        .as_mut()
        .unwrap()
        .runs[0]
        .provider
    else {
        panic!()
    };
    target.repeats.push(target.repeats[0].clone());
    rejected(&bad, "path is incomplete");
}

#[test]
fn preserve_input_only_patch_reopens_inverts_and_detects_stale_routes() {
    let before = document();
    let mut after = before.clone();
    after.revision_id = revision("after");
    let mut runs = projection().runs().to_vec();
    runs[0].source = range(2, 4);
    replace(&mut after, runs);
    let reopened = ProjectDocument::from_json(&after.to_compact_json().unwrap()).unwrap();
    assert_eq!(reopened, after);
    let edit = apply(
        &before,
        &CommandRequest {
            project_id: before.project_id.clone(),
            expected_revision: before.revision_id.clone(),
            new_revision: after.revision_id.clone(),
            command: Command::RestoreSnapshot {
                snapshot: Box::new(after.clone()),
            },
        },
    )
    .unwrap();
    assert!(edit.forward.nodes.is_empty());
    assert_eq!(edit.forward.derived_clocks.len(), 1);
    assert_eq!(edit.changed_ids, vec![id("stage")]);
    assert_eq!(edit.duration_delta, 0);
    let decoded: EditTransaction =
        serde_json::from_str(&serde_json::to_string(&edit).unwrap()).unwrap();
    assert_eq!(decoded.forward.apply(&before).unwrap(), after);
    assert_eq!(decoded.inverse.apply(&after).unwrap(), before);
    assert!(decoded.inverse.restores(&after, &before).unwrap());
    let mut stale = after;
    replace(&mut stale, projection().runs().to_vec());
    assert!(decoded.inverse.apply(&stale).is_err());
}

#[test]
fn preserve_input_document_run_and_ancestor_work_budgets_are_enforced() {
    let mut doc = document();
    doc.nodes.clear();
    doc.derived_clocks.clear();
    let mut children = Vec::new();
    for n in 0..17 {
        let owner = format!("stage-{n}");
        let leaf = format!("leaf-{n}");
        children.push(id(&owner));
        doc.nodes.insert(id(&leaf), hold(4096));
        doc.nodes.insert(id(&owner), stage(&leaf, 4096, 2048));
        let runs = (0..4096)
            .map(|i| run(range(i, i + 1), range(0, 1), &leaf))
            .collect();
        doc.derived_clocks.insert(
            id(&owner),
            DerivedRetimeClock {
                extent: ExactRatio::integer(2048),
                child_start: ExactRatio::ZERO,
                child_end: ExactRatio::integer(4096),
                preserve_input: Some(PreserveInputProjection::new(runs).unwrap()),
            },
        );
        doc.nodes
            .insert(id("root"), BeatNode::sequence("Root", children.clone()));
        if n == 15 {
            doc.validate().unwrap();
        }
    }
    rejected(&doc, "document Preserve input run limit");

    let mut doc = document();
    doc.nodes.clear();
    doc.derived_clocks.clear();
    doc.nodes.insert(id("leaf"), hold(4096));
    let mut child = id("leaf");
    for n in 0..250 {
        let parent = id(&format!("group-{n}"));
        doc.nodes
            .insert(parent.clone(), BeatNode::sequence("Group", vec![child]));
        child = parent;
    }
    let mut owner = stage("leaf", 4096, 2048);
    let NodeKind::Retime { child: target, .. } = &mut owner.kind else {
        panic!()
    };
    *target = child;
    doc.nodes.insert(id("stage"), owner);
    doc.nodes
        .insert(id("root"), BeatNode::sequence("Root", vec![id("stage")]));
    doc.derived_clocks.insert(
        id("stage"),
        DerivedRetimeClock {
            extent: ExactRatio::integer(2048),
            child_start: ExactRatio::ZERO,
            child_end: ExactRatio::integer(4096),
            preserve_input: Some(
                PreserveInputProjection::new(
                    (0..4096)
                        .map(|i| run(range(i, i + 1), range(0, 1), "leaf"))
                        .collect(),
                )
                .unwrap(),
            ),
        },
    );
    rejected(&doc, "ownership work limit");
}
