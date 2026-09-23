use deadpan_core::*;
use proptest::prelude::*;
use std::collections::BTreeMap;

fn id(s: &str) -> NodeId {
    NodeId::new(s).unwrap()
}
fn rev(s: &str) -> RevisionId {
    RevisionId::new(s).unwrap()
}
fn dur(n: i64) -> FrameDuration {
    FrameDuration::new(n).unwrap()
}
fn recipe(n: i64) -> HoldRecipe {
    HoldRecipe {
        duration: dur(n),
        video: HoldVideo::Background,
        audio: HoldAudio::Silence,
    }
}
fn subtree(name: &str, n: i64) -> Subtree {
    Subtree {
        root: id(name),
        nodes: BTreeMap::from([(id(name), BeatNode::hold(name, recipe(n)))]),
        overrides: BTreeMap::new(),
    }
}
fn request(document: &ProjectDocument, command: Command) -> CommandRequest {
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: rev(&format!("{}x", document.revision_id())),
        command,
    }
}
fn edit(document: &ProjectDocument, command: Command) -> ProjectDocument {
    let transaction = apply(document, &request(document, command)).unwrap();
    let next = transaction.forward.apply(document).unwrap();
    assert_eq!(transaction.inverse.apply(&next).unwrap(), *document);
    assert_eq!(
        ProjectDocument::from_json(&next.to_json().unwrap()).unwrap(),
        next
    );
    next
}
fn repeated(plays: u32, base: i64, gap: i64) -> ProjectDocument {
    let empty = ProjectDocument::new(
        ProjectId::new("project").unwrap(),
        rev("initial"),
        PresentationBasis {
            width: 1920,
            height: 1080,
            frame_rate: FrameRate::new(30000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let inserted = edit(
        &empty,
        Command::Insert {
            parent: id("root"),
            index: 0,
            subtree: subtree("base", base),
        },
    );
    edit(
        &inserted,
        Command::WrapRepeat {
            node: id("base"),
            id: id("repeat"),
            plays,
            gap: (gap > 0).then(|| recipe(gap)),
            anchor_policy: WrapAnchorPolicy::First,
        },
    )
}
fn iteration(document: &ProjectDocument, index: u32) -> IterationId {
    let NodeKind::Repeat { iterations, .. } = &document.nodes()[&id("repeat")].kind else {
        panic!()
    };
    iterations.at(index).unwrap()
}
fn path(document: &ProjectDocument, node: &str, index: u32) -> InstancePath {
    InstancePath {
        node: id(node),
        repeats: vec![RepeatInstance {
            node: id("repeat"),
            iteration: iteration(document, index),
        }],
    }
}
fn target(instance: InstancePath, position: i64) -> AnchorTarget {
    AnchorTarget {
        boundary: BoundaryAnchor {
            coordinate: Anchor::Occurrence {
                instance,
                position: ExactRatio::integer(position),
            },
            bias: InsertionBias::Right,
        },
        occurrence: None,
    }
}
fn mark(document: &ProjectDocument, name: &str, coordinate: Anchor) -> ProjectDocument {
    edit(
        document,
        Command::SetMark {
            id: MarkId::new(name).unwrap(),
            owner: id("root"),
            label: name.into(),
            boundary: BoundaryAnchor {
                coordinate,
                bias: InsertionBias::Right,
            },
            loss_policy: AnchorLossPolicy::KeepUnresolved,
        },
    )
}
fn local(node: &str, position: i64) -> Anchor {
    Anchor::Local {
        node: id(node),
        position: ExactRatio::integer(position),
    }
}
fn mark_position(document: &ProjectDocument, name: &str) -> ExactRatio {
    let Anchor::Local { position, .. } = document.marks()[&MarkId::new(name).unwrap()]
        .boundary
        .coordinate
    else {
        panic!()
    };
    position
}

#[test]
fn one_play_override_changes_only_its_content_and_exact_neighbor_offsets() {
    let document = repeated(3, 4, 2);
    let document = mark(&document, "last", local("repeat", 14));
    let document = mark(&document, "gap", local("repeat", 11));
    let document = mark(&document, "replaced", local("repeat", 8));
    let document = mark(
        &document,
        "occurrence",
        Anchor::Occurrence {
            instance: path(&document, "base", 1),
            position: ExactRatio::integer(2),
        },
    );
    let second = iteration(&document, 1);
    let overridden = edit(
        &document,
        Command::SetPlayOverride {
            node: id("repeat"),
            iteration: second.clone(),
            subtree: subtree("custom", 7),
        },
    );
    assert_eq!(document.duration().unwrap(), dur(16));
    assert_eq!(overridden.duration().unwrap(), dur(19));
    assert_eq!(mark_position(&overridden, "last"), ExactRatio::integer(17));
    assert_eq!(mark_position(&overridden, "gap"), ExactRatio::integer(14));
    for name in ["replaced", "occurrence"] {
        assert!(matches!(
            overridden.marks()[&MarkId::new(name).unwrap()].state,
            MarkState::Unresolved { .. }
        ));
    }
    let index = AnchorIndex::new(&overridden).unwrap();
    for (node, play, expected) in [("base", 0, 2), ("custom", 1, 8), ("base", 2, 17)] {
        let instance = path(&overridden, node, play);
        instance.validate(&overridden).unwrap();
        assert_eq!(
            index
                .resolve_target(&target(instance, 2))
                .unwrap()
                .exact_frame,
            ExactRatio::integer(expected)
        );
    }
    for (node, play) in [("base", 1), ("custom", 0), ("custom", 2)] {
        let instance = path(&overridden, node, play);
        assert!(instance.validate(&overridden).is_err());
        assert_eq!(
            index.resolve_target(&target(instance, 2)).unwrap_err().code,
            AnchorErrorCode::OccurrenceInvalid
        );
    }
    let changed = edit(
        &overridden,
        Command::SetHoldDuration {
            node: id("base"),
            duration: dur(5),
        },
    );
    assert_eq!(changed.duration().unwrap(), dur(21));
    assert_eq!(changed.node_duration(&id("custom")).unwrap(), dur(7));
    let changed = edit(
        &changed,
        Command::SetHoldDuration {
            node: id("custom"),
            duration: dur(8),
        },
    );
    assert_eq!(changed.duration().unwrap(), dur(22));
    assert_eq!(
        changed.overrides()[&id("repeat")].get(&second),
        Some(&id("custom"))
    );
}

#[test]
fn reorder_insert_shrink_and_regrow_preserve_only_surviving_overrides() {
    let document = repeated(4, 3, 1);
    let selected = iteration(&document, 2);
    let document = edit(
        &document,
        Command::SetPlayOverride {
            node: id("repeat"),
            iteration: selected.clone(),
            subtree: subtree("custom", 8),
        },
    );
    let document = mark(
        &document,
        "inside",
        Anchor::Occurrence {
            instance: path(&document, "custom", 2),
            position: ExactRatio::integer(2),
        },
    );
    let moved = edit(
        &document,
        Command::MovePlays {
            node: id("repeat"),
            start: 2,
            end: 3,
            destination: 0,
        },
    );
    assert_eq!(iteration(&moved, 0), selected);
    assert_eq!(
        AnchorIndex::new(&moved)
            .unwrap()
            .resolve_target(&target(path(&moved, "custom", 0), 2))
            .unwrap()
            .frame,
        ProjectFrame(2)
    );
    let inserted = edit(
        &moved,
        Command::InsertPlays {
            node: id("repeat"),
            index: 0,
            count: 2,
        },
    );
    assert_eq!(iteration(&inserted, 2), selected);
    assert_eq!(inserted.overrides()[&id("repeat")].len(), 1);
    let removed = edit(
        &inserted,
        Command::SetRepeat {
            node: id("repeat"),
            plays: 2,
            gap: Some(recipe(1)),
        },
    );
    assert!(removed.overrides().is_empty());
    assert!(!removed.nodes().contains_key(&id("custom")));
    assert!(matches!(
        removed.marks()[&MarkId::new("inside").unwrap()].state,
        MarkState::Unresolved { .. }
    ));
    let grown = edit(
        &removed,
        Command::SetRepeat {
            node: id("repeat"),
            plays: 4,
            gap: Some(recipe(1)),
        },
    );
    assert!(grown.overrides().is_empty());
    assert_ne!(iteration(&grown, 2), selected);
    assert_eq!(grown.duration().unwrap(), dur(15));
}

#[test]
fn clear_replace_and_delete_remove_owned_subtrees_without_reattaching_marks() {
    let document = repeated(2, 4, 0);
    let selected = iteration(&document, 1);
    let document = edit(
        &document,
        Command::SetPlayOverride {
            node: id("repeat"),
            iteration: selected.clone(),
            subtree: subtree("custom", 7),
        },
    );
    let document = mark(&document, "inside", local("custom", 3));
    let before = document.clone();
    assert!(
        apply(
            &document,
            &request(
                &document,
                Command::SetPlayOverride {
                    node: id("repeat"),
                    iteration: selected.clone(),
                    subtree: subtree("custom", 8)
                }
            )
        )
        .is_err()
    );
    assert_eq!(document, before);
    let replacement = edit(
        &document,
        Command::SetPlayOverride {
            node: id("repeat"),
            iteration: selected.clone(),
            subtree: subtree("replacement", 2),
        },
    );
    assert!(!replacement.nodes().contains_key(&id("custom")));
    assert!(matches!(
        replacement.marks()[&MarkId::new("inside").unwrap()].state,
        MarkState::Unresolved { .. }
    ));
    let cleared = edit(
        &replacement,
        Command::ClearPlayOverride {
            node: id("repeat"),
            iteration: selected,
        },
    );
    assert_eq!(cleared.duration().unwrap(), dur(8));
    assert!(cleared.overrides().is_empty());
    assert!(!cleared.nodes().contains_key(&id("replacement")));
    let deleted = edit(&document, Command::Delete { node: id("repeat") });
    assert_eq!(deleted.nodes().len(), 1);
    assert!(deleted.overrides().is_empty());
}

#[test]
fn nested_override_insertion_remaps_ids_and_wrapping_preserves_actual_ancestry() {
    let document = repeated(2, 4, 0);
    let outer = iteration(&document, 1);
    let foreign = IterationOrder::new(rev("foreign"), 3).unwrap();
    let inner_selected = foreign.at(1).unwrap();
    let fragment = Subtree {
        root: id("inner"),
        nodes: BTreeMap::from([
            (
                id("inner"),
                BeatNode {
                    audio_edges: Default::default(),
                    label: "Inner".into(),
                    kind: NodeKind::Repeat {
                        child: id("inner-base"),
                        iterations: foreign,
                        gap: None,
                    },
                },
            ),
            (id("inner-base"), BeatNode::hold("Default", recipe(2))),
            (id("inner-custom"), BeatNode::hold("Changed", recipe(5))),
        ]),
        overrides: BTreeMap::from([(
            id("inner"),
            PlayOverrides::try_from(vec![PlayOverride {
                iteration: inner_selected,
                root: id("inner-custom"),
            }])
            .unwrap(),
        )]),
    };
    let document = edit(
        &document,
        Command::SetPlayOverride {
            node: id("repeat"),
            iteration: outer.clone(),
            subtree: fragment,
        },
    );
    assert_eq!(document.duration().unwrap(), dur(13));
    let (inner_id, _) = document.overrides()[&id("inner")].iter().next().unwrap();
    assert_eq!(&inner_id.allocation, document.revision_id());
    let instance = InstancePath {
        node: id("inner-custom"),
        repeats: vec![
            RepeatInstance {
                node: id("repeat"),
                iteration: outer.clone(),
            },
            RepeatInstance {
                node: id("inner"),
                iteration: inner_id.clone(),
            },
        ],
    };
    instance.validate(&document).unwrap();
    assert_eq!(
        AnchorIndex::new(&document)
            .unwrap()
            .resolve_target(&target(instance.clone(), 1))
            .unwrap()
            .frame,
        ProjectFrame(7)
    );
    let document = mark(
        &document,
        "nested",
        Anchor::Occurrence {
            instance,
            position: ExactRatio::integer(1),
        },
    );
    let wrapped = edit(
        &document,
        Command::WrapRepeat {
            node: id("inner"),
            id: id("wrapper"),
            plays: 2,
            gap: None,
            anchor_policy: WrapAnchorPolicy::First,
        },
    );
    assert_eq!(
        wrapped.overrides()[&id("repeat")].get(&outer),
        Some(&id("wrapper"))
    );
    let Anchor::Occurrence { instance, .. } = &wrapped.marks()[&MarkId::new("nested").unwrap()]
        .boundary
        .coordinate
    else {
        panic!()
    };
    assert_eq!(
        instance
            .repeats
            .iter()
            .map(|part| part.node.as_str())
            .collect::<Vec<_>>(),
        ["repeat", "wrapper", "inner"]
    );
    instance.validate(&wrapped).unwrap();
    let removed = edit(
        &wrapped,
        Command::ClearPlayOverride {
            node: id("repeat"),
            iteration: outer,
        },
    );
    assert_eq!(removed.nodes().len(), 3);
    assert!(removed.overrides().is_empty());
}

#[test]
fn invalid_override_graphs_and_wire_shapes_fail_without_mutation() {
    let document = repeated(2, 4, 0);
    let identity = iteration(&document, 0);
    let make = |root: &str, entries: serde_json::Value| {
        let mut value = serde_json::to_value(&document).unwrap();
        value["overrides"] = serde_json::json!({root:entries});
        ProjectDocument::from_json(&value.to_string())
    };
    assert!(make("repeat", serde_json::json!([])).is_err());
    assert!(
        make(
            "repeat",
            serde_json::json!([{"iteration":identity,"root":"repeat"}])
        )
        .is_err()
    );
    assert!(
        make(
            "base",
            serde_json::json!([{"iteration":identity,"root":"base"}])
        )
        .is_err()
    );
    assert!(
        make(
            "repeat",
            serde_json::json!([{"iteration":identity,"root":"base"}])
        )
        .is_err()
    );
    assert!(
        make(
            "repeat",
            serde_json::json!([{"iteration":identity,"root":"absent"}])
        )
        .is_err()
    );
    assert!(make("repeat",serde_json::json!([{"iteration":identity,"root":"base"},{"iteration":identity,"root":"base"}])).is_err());
    let stale = IterationId {
        allocation: rev("missing"),
        ordinal: 0,
    };
    assert!(
        apply(
            &document,
            &request(
                &document,
                Command::SetPlayOverride {
                    node: id("repeat"),
                    iteration: stale,
                    subtree: subtree("new", 2)
                }
            )
        )
        .is_err()
    );
    let mut unknown = serde_json::json!([{"iteration":identity,"root":"new","hidden":true}]);
    assert!(serde_json::from_value::<PlayOverrides>(unknown.take()).is_err());
}

#[test]
fn override_duration_overflow_and_empty_content_reject_the_whole_edit() {
    let document = repeated(2, 4, 0);
    let before = document.to_json().unwrap();
    let selected = iteration(&document, 1);
    let overflow = apply(
        &document,
        &request(
            &document,
            Command::SetPlayOverride {
                node: id("repeat"),
                iteration: selected.clone(),
                subtree: subtree("too-long", i64::MAX),
            },
        ),
    )
    .unwrap_err();
    assert_eq!(overflow.code, EditErrorCode::TimingOverflow);
    let empty = Subtree {
        root: id("empty"),
        nodes: BTreeMap::from([(id("empty"), BeatNode::sequence("Empty", vec![]))]),
        overrides: BTreeMap::new(),
    };
    assert!(
        apply(
            &document,
            &request(
                &document,
                Command::SetPlayOverride {
                    node: id("repeat"),
                    iteration: selected,
                    subtree: empty,
                }
            )
        )
        .is_err()
    );
    assert_eq!(document.to_json().unwrap(), before);
}

#[test]
fn billion_play_sparse_layout_and_unused_huge_default_stay_bounded() {
    let document = repeated(u32::MAX, 1, 0);
    let last = iteration(&document, u32::MAX - 1);
    let document = edit(
        &document,
        Command::SetPlayOverride {
            node: id("repeat"),
            iteration: last.clone(),
            subtree: subtree("last", 3),
        },
    );
    assert_eq!(document.duration().unwrap(), dur(i64::from(u32::MAX) + 2));
    let NodeKind::Repeat {
        iterations, child, ..
    } = &document.nodes()[&id("repeat")].kind
    else {
        panic!()
    };
    let layout = RepeatLayout::compile(
        iterations,
        child,
        document.overrides().get(&id("repeat")),
        FrameDuration::ZERO,
        &document.durations().unwrap(),
    )
    .unwrap();
    assert_eq!(layout.segment_count(), 2);
    assert_eq!(layout.play(&last).unwrap().start, i64::from(u32::MAX) - 1);
    assert_eq!(
        layout
            .locate(
                ExactRatio::integer(i64::from(u32::MAX)),
                InsertionBias::Right
            )
            .unwrap()
            .play
            .child,
        id("last")
    );
    let high: IterationOrder = serde_json::from_value(
        serde_json::json!({"runs":[{"allocation":"high","first":u32::MAX,"count":1}]}),
    )
    .unwrap();
    let override_map = PlayOverrides::try_from(vec![PlayOverride {
        iteration: high.at(0).unwrap(),
        root: id("small"),
    }])
    .unwrap();
    let durations = BTreeMap::from([(id("huge"), dur(i64::MAX)), (id("small"), dur(1))]);
    let layout = RepeatLayout::compile(
        &high,
        &id("huge"),
        Some(&override_map),
        dur(i64::MAX),
        &durations,
    )
    .unwrap();
    assert_eq!(layout.duration(), dur(1));
    assert_eq!(
        layout
            .locate(ExactRatio::integer(1), InsertionBias::Left)
            .unwrap()
            .position,
        ExactRatio::integer(1)
    );
}

proptest! {
    #[test]
    fn sparse_layout_matches_expanded_durations_and_boundary_sides(plays in 1u32..30, base in 1i64..10, gap in 0i64..5, choices in proptest::collection::vec((0u32..30,1i64..12),0..20)) {
        let order=IterationOrder::new(rev("all"),plays).unwrap();
        let mut expanded=vec![base;plays as usize];
        let mut entries=BTreeMap::new();
        let mut durations=BTreeMap::from([(id("base"),dur(base))]);
        for (index,frames) in choices {
            let index=index%plays;
            let node=id(&format!("play-{index}"));
            expanded[index as usize]=frames;
            durations.insert(node.clone(),dur(frames));
            entries.insert(order.at(index).unwrap(),node);
        }
        let overrides=PlayOverrides::try_from(entries.into_iter().map(|(iteration,root)|PlayOverride {iteration,root}).collect::<Vec<_>>()).unwrap();
        let layout=RepeatLayout::compile(&order,&id("base"),Some(&overrides),dur(gap),&durations).unwrap();
        let expected=expanded.iter().sum::<i64>()+i64::from(plays-1)*gap;
        prop_assert_eq!(layout.duration(),dur(expected));
        prop_assert!(layout.segment_count() <= 1+2*overrides.len());
        let mut start=0;
        for (index,frames) in expanded.iter().enumerate() {
            let identity=order.at(index as u32).unwrap();
            prop_assert_eq!(layout.play(&identity).unwrap().start,start);
            for offset in 0..*frames {
                let selected=layout.locate(ExactRatio::new(i128::from(start+offset)*2+1,2).unwrap(),InsertionBias::Right).unwrap();
                prop_assert_eq!(selected.play.iteration.clone(),identity.clone());
                prop_assert!(!selected.in_gap);
                prop_assert_eq!(selected.position,ExactRatio::new(i128::from(offset)*2+1,2).unwrap());
            }
            let boundary=start+frames;
            prop_assert!(!layout.locate(ExactRatio::integer(boundary),InsertionBias::Left).unwrap().in_gap);
            if index+1<expanded.len() {
                let after=layout.locate(ExactRatio::integer(boundary),InsertionBias::Right).unwrap();
                prop_assert_eq!(after.in_gap,gap>0);
                if gap>0 {prop_assert_eq!(after.play.iteration.clone(),identity.clone());}
                let end_gap=layout.locate(ExactRatio::integer(boundary+gap),InsertionBias::Right).unwrap();
                prop_assert_eq!(end_gap.play.index,index as u32+1);
            }
            start=boundary+gap;
        }
    }
}
