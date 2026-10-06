//! Multi-target and partial-range occurrence edits (plays 2-3 only).
use std::collections::BTreeMap;

use deadpan_core::*;

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn silent(duration: i64) -> HoldRecipe {
    HoldRecipe {
        picture_context: None,
        duration: FrameDuration::new(duration).unwrap(),
        video: HoldVideo::Background,
        audio: HoldAudio::Silence,
    }
}
fn request(document: &ProjectDocument, name: &str, command: Command) -> CommandRequest {
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new(name).unwrap(),
        command,
    }
}
fn edit(document: &ProjectDocument, name: &str, command: Command) -> ProjectDocument {
    let request = request(document, name, command);
    let request: CommandRequest =
        serde_json::from_value(serde_json::to_value(&request).unwrap()).unwrap();
    let (transaction, result) = apply_with_result(document, &request).unwrap();
    assert_eq!(transaction.forward.apply(document).unwrap(), result);
    assert_eq!(transaction.inverse.apply(&result).unwrap(), *document);
    if matches!(request.command, Command::EditScopedMany { .. }) {
        assert_eq!(transaction.duration_delta, 0);
        assert_eq!(result.duration().unwrap(), document.duration().unwrap());
    }
    result
}

/// `outer(inner(group[h1, h2]) x 3) x 2`, all shared definitions.
fn nested() -> ProjectDocument {
    let document = ProjectDocument::new(
        ProjectId::new("scoped-many").unwrap(),
        RevisionId::new("r0").unwrap(),
        PresentationBasis {
            width: 64,
            height: 36,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let document = edit(
        &document,
        "r1",
        Command::Insert {
            parent: id("root"),
            index: 0,
            subtree: Subtree {
                root: id("group"),
                nodes: BTreeMap::from([
                    (
                        id("group"),
                        BeatNode::sequence("Group", vec![id("h1"), id("h2")]),
                    ),
                    (id("h1"), BeatNode::hold("h1", silent(4))),
                    (id("h2"), BeatNode::hold("h2", silent(2))),
                ]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
    );
    let document = edit(
        &document,
        "r2",
        Command::WrapRepeat {
            node: id("group"),
            id: id("inner"),
            plays: 3,
            gap: None,
            anchor_policy: WrapAnchorPolicy::First,
        },
    );
    edit(
        &document,
        "r3",
        Command::WrapRepeat {
            node: id("inner"),
            id: id("outer"),
            plays: 2,
            gap: None,
            anchor_policy: WrapAnchorPolicy::First,
        },
    )
}

fn play(document: &ProjectDocument, repeat: &str, index: u32) -> RepeatEditBranch {
    let NodeKind::Repeat { iterations, .. } = &document.nodes()[&id(repeat)].kind else {
        panic!()
    };
    RepeatEditBranch::Play {
        iteration: iterations.at(index).unwrap(),
    }
}

fn target(document: &ProjectDocument, outer: Option<u32>, inner: u32) -> ScopedNodeTarget {
    ScopedNodeTarget {
        node: id("h1"),
        repeats: vec![
            RepeatEditStep {
                repeat: id("outer"),
                branch: outer.map_or(RepeatEditBranch::Default, |index| {
                    play(document, "outer", index)
                }),
            },
            RepeatEditStep {
                repeat: id("inner"),
                branch: play(document, "inner", inner),
            },
        ],
    }
}

fn gain(millidecibels: i32, range: Option<(i64, i64)>) -> ScopedNodeEdit {
    let clip = ClipGain::default();
    let clip = match range {
        None => clip.with_trim(GainDb::new(millidecibels).unwrap()),
        Some((start, end)) => clip
            .adjust_range(
                GainRange::new(ExactRatio::integer(start), ExactRatio::integer(end)).unwrap(),
                millidecibels,
            )
            .unwrap(),
    };
    ScopedNodeEdit::SetAudioTreatments {
        treatments: AudioTreatments::from_clip_gain(clip),
    }
}

fn many(document: &ProjectDocument, name: &str, edits: Vec<ScopedTargetEdit>) -> ProjectDocument {
    let needs = document.scoped_many_requirements(&edits).unwrap();
    let identities = needs
        .iter()
        .enumerate()
        .map(|(index, needs)| OccurrenceIdentities {
            nodes: (0..needs.nodes)
                .map(|n| id(&format!("{name}-{index}-{n}")))
                .collect(),
            marks: (0..needs.marks)
                .map(|n| MarkId::new(format!("{name}-{index}-{n}")).unwrap())
                .collect(),
        })
        .collect();
    edit(
        document,
        name,
        Command::EditScopedMany { edits, identities },
    )
}

fn effective_h1(document: &ProjectDocument, outer: u32, inner: u32) -> &BeatNode {
    let NodeKind::Repeat {
        iterations, child, ..
    } = &document.nodes()[&id("outer")].kind
    else {
        panic!()
    };
    let outer_child = document
        .overrides()
        .get(&id("outer"))
        .and_then(|entries| entries.get(&iterations.at(outer).unwrap()))
        .unwrap_or(child);
    let NodeKind::Repeat {
        iterations, child, ..
    } = &document.nodes()[outer_child].kind
    else {
        panic!()
    };
    let group = document
        .overrides()
        .get(outer_child)
        .and_then(|entries| entries.get(&iterations.at(inner).unwrap()))
        .unwrap_or(child);
    let h1 = document.children(group).next().unwrap();
    &document.nodes()[h1]
}

#[test]
fn plays_two_and_three_change_together_and_others_stay_shared() {
    let document = nested();
    let edits: Vec<_> = [1, 2]
        .map(|inner| ScopedTargetEdit {
            target: target(&document, None, inner),
            edit: gain(-6000, None),
        })
        .into();
    let needs = document.scoped_many_requirements(&edits).unwrap();
    assert_eq!(
        needs.iter().map(|needs| needs.nodes).collect::<Vec<_>>(),
        [3, 3]
    );
    let result = many(&document, "two-three", edits);
    let trim = |outer, inner| {
        effective_h1(&result, outer, inner)
            .audio_treatments
            .clip_gain()
            .map(|clip| clip.trim().millidecibels())
    };
    // Default outer branch: both outer plays share inner plays 2 and 3.
    for outer in 0..2 {
        assert_eq!(trim(outer, 0), None);
        assert_eq!(trim(outer, 1), Some(-6000));
        assert_eq!(trim(outer, 2), Some(-6000));
    }
}

#[test]
fn a_later_target_follows_the_outer_play_an_earlier_target_isolated() {
    let document = nested();
    let edits: Vec<_> = [1, 2]
        .map(|inner| ScopedTargetEdit {
            target: target(&document, Some(1), inner),
            edit: gain(3000, None),
        })
        .into();
    let needs = document.scoped_many_requirements(&edits).unwrap();
    // The first isolates outer play 2 (inner + group + 2 holds) and then its
    // inner play 2; the second reuses that outer copy.
    assert_eq!(needs[0].nodes, 7);
    assert_eq!(needs[1].nodes, 3);
    let result = many(&document, "nested", edits);
    let trim = |outer, inner| {
        effective_h1(&result, outer, inner)
            .audio_treatments
            .clip_gain()
            .map(|clip| clip.trim().millidecibels())
    };
    assert_eq!([trim(0, 1), trim(0, 2)], [None, None]);
    assert_eq!(
        [trim(1, 0), trim(1, 1), trim(1, 2)],
        [None, Some(3000), Some(3000)]
    );
}

#[test]
fn a_partial_range_inside_one_play_keeps_the_rest_of_that_play() {
    let document = nested();
    let result = many(
        &document,
        "range",
        vec![ScopedTargetEdit {
            target: target(&document, None, 1),
            edit: gain(-12000, Some((1, 3))),
        }],
    );
    let clip = effective_h1(&result, 0, 1)
        .audio_treatments
        .clip_gain()
        .unwrap()
        .clone();
    let at = |frame: i64| {
        clip.evaluate(ExactRatio::new(i128::from(frame) * 2 + 1, 2).unwrap())
            .unwrap()
            .millidecibels
    };
    assert_eq!(at(0), ExactRatio::ZERO);
    assert_eq!(at(1), ExactRatio::integer(-12000));
    assert_eq!(at(2), ExactRatio::integer(-12000));
    assert_eq!(at(3), ExactRatio::ZERO);
    assert!(effective_h1(&result, 0, 0).audio_treatments.is_empty());
}

#[test]
fn unchanged_targets_are_skipped_and_an_all_unchanged_edit_refuses() {
    let document = nested();
    let once = many(
        &document,
        "once",
        vec![ScopedTargetEdit {
            target: target(&document, None, 1),
            edit: gain(-6000, None),
        }],
    );
    let edits: Vec<_> = [1, 2]
        .map(|inner| ScopedTargetEdit {
            target: target(&document, None, inner),
            edit: gain(-6000, None),
        })
        .into();
    // The isolated play keeps its owned branch; its target now names it.
    let mut edits = edits;
    let NodeKind::Repeat { iterations, .. } = &once.nodes()[&id("inner")].kind else {
        panic!()
    };
    let owned = &once.overrides()[&id("inner")]
        .get(&iterations.at(1).unwrap())
        .unwrap()
        .clone();
    edits[0].target.node = once.children(owned).next().unwrap().clone();
    let needs = once.scoped_many_requirements(&edits).unwrap();
    assert!(needs[0].unchanged && !needs[1].unchanged);
    let twice = many(&once, "twice", edits.clone());
    assert_ne!(twice, once);
    let error = apply(
        &twice,
        &request(
            &twice,
            "none",
            Command::EditScopedMany {
                edits: vec![edits[0].clone()],
                identities: vec![OccurrenceIdentities::default()],
            },
        ),
    )
    .unwrap_err();
    assert!(error.message.contains("does not change"), "{error}");
}
