use super::*;
use SemanticTextObject::{AroundGroup, InnerGroup};

fn selected(parent: &str, cursor: i64, child: &str) -> SemanticContext {
    SemanticContext {
        selected_child: Some(node(child)),
        ..context(parent, cursor)
    }
}
fn object(kind: SemanticTextObject) -> SemanticSelector {
    SemanticSelector::TextObject { object: kind }
}
fn select(kind: SemanticTextObject) -> SemanticInstruction {
    SemanticInstruction::SelectObject { object: kind }
}
fn yank(kind: SemanticTextObject, register: char) -> SemanticInstruction {
    SemanticInstruction::Yank {
        selector: object(kind),
        register: name(register),
    }
}
fn cut_object(kind: SemanticTextObject) -> SemanticInstruction {
    SemanticInstruction::Cut {
        selector: object(kind),
        register: name('a'),
    }
}
fn nested() -> ProjectDocument {
    tree(
        &["prefix", "outer", "suffix"],
        vec![
            ("prefix", hold(2)),
            (
                "outer",
                BeatNode::sequence("Outer", vec![node("lead"), node("inner"), node("after")]),
            ),
            ("lead", hold(3)),
            (
                "inner",
                BeatNode::sequence(
                    "Inner",
                    vec![node("empty-left"), node("body"), node("empty-right")],
                ),
            ),
            ("empty-left", BeatNode::sequence("Left empty", vec![])),
            ("body", hold(4)),
            ("empty-right", BeatNode::sequence("Right empty", vec![])),
            ("after", hold(1)),
            ("suffix", hold(2)),
        ],
    )
}
fn copied(plan: &SemanticPlan, register: char) -> &CapturedEditSlice {
    let RegisterValue::Edited { slice } = plan.register_writes[&name(register)].as_ref() else {
        panic!("Edited register expected")
    };
    slice
}
fn children(document: &ProjectDocument, parent: &str) -> Vec<NodeId> {
    let NodeKind::Sequence { children } = &document.nodes()[&node(parent)].kind else {
        panic!("Sequence expected")
    };
    children.clone()
}
fn assert_inverse(document: &ProjectDocument, planned: &SemanticPlan) {
    let request = planned.request.as_ref().unwrap();
    let replay = crate::replay_compound::<EditError>(document, request, |_| Ok(())).unwrap();
    let Command::Compound { transaction } = &request.command else {
        panic!("Compound expected")
    };
    let mut expected = planned.document.clone();
    if transaction.steps().iter().all(|step| step.edit().is_none()) {
        assert_eq!(planned.document, *document);
        expected.revision_id = request.new_revision.clone();
    }
    assert_eq!(replay.document, expected);
    assert_eq!(replay.register_writes, planned.register_writes);
    assert_eq!(
        replay.edit.inverse.apply(&replay.document).unwrap(),
        *document
    );
}

#[test]
fn objects_choose_explicit_group_then_containing_group_without_cursor_inference() {
    let document = nested();
    for entry in [
        selected("outer", 3, "inner"),
        selected("inner", 6, "body"),
        context("inner", 9),
    ] {
        for kind in [InnerGroup, AroundGroup] {
            let selection = document.resolve_group_object(&entry, kind).unwrap();
            assert_eq!(
                selection,
                SemanticObjectSelection {
                    kind,
                    group: node("inner")
                }
            );
            let target = document
                .resolve_object_selection(&entry.parent, &selection)
                .unwrap();
            assert_eq!(target.range, range(5, 9));
            assert_eq!(
                target.parent,
                node(if kind == InnerGroup { "inner" } else { "outer" })
            );
            assert_eq!(
                target.selection,
                Some(if kind == InnerGroup {
                    SliceCaptureSelection::Children {
                        first: node("empty-left"),
                        last: node("empty-right"),
                    }
                } else {
                    SliceCaptureSelection::Child {
                        node: node("inner"),
                    }
                })
            );
        }
    }
    for entry in [context("root", 6), selected("root", 6, "prefix")] {
        assert!(document.resolve_group_object(&entry, InnerGroup).is_err());
    }
    let chosen = document
        .resolve_group_object(&selected("root", 0, "outer"), AroundGroup)
        .unwrap();
    assert_eq!(chosen.group, node("outer"));
}

#[test]
fn inner_capture_retains_empty_endpoints_and_excludes_parent_effects_while_around_owns_them() {
    let mut document = nested();
    document
        .nodes
        .get_mut(&node("inner"))
        .unwrap()
        .audio_treatments = crate::AudioTreatments::from_clip_gain(
        crate::ClipGain::new(crate::GainDb::new(-3000).unwrap(), false, vec![], vec![]).unwrap(),
    );
    document.validate().unwrap();
    let entry = selected("outer", 3, "inner");
    let planned = plan(
        &document,
        entry.clone(),
        vec![yank(InnerGroup, 'a'), yank(AroundGroup, 'b')],
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(planned.context, entry);
    assert_eq!(planned.document, document);
    let inside = serde_json::to_value(copied(&planned, 'a')).unwrap();
    let around = serde_json::to_value(copied(&planned, 'b')).unwrap();
    assert!(inside["nodes"].get("inner").is_none());
    assert_eq!(inside["parts"].as_array().unwrap().len(), 3);
    assert_eq!(inside["parts"][0]["root"], "empty-left");
    assert_eq!(inside["parts"][2]["root"], "empty-right");
    assert_eq!(
        around["nodes"]["inner"],
        serde_json::to_value(&document.nodes()[&node("inner")]).unwrap()
    );
    for (index, parent, scope, labels, bounds) in [
        (
            0,
            "inner",
            vec![node("outer"), node("inner")],
            vec!["Outer", "Inner"],
            range(5, 9),
        ),
        (1, "outer", vec![node("outer")], vec!["Outer"], range(2, 10)),
    ] {
        let trace = &planned.trace[index];
        let capture = trace.capture.as_ref().unwrap();
        assert_eq!(trace.before_revision, revision("base"));
        assert_eq!(trace.resolved_parent, Some(node(parent)));
        assert_eq!(capture.parent, node(parent));
        assert_eq!(capture.bounds, bounds);
        assert_eq!(capture.scope, scope);
        assert_eq!(capture.scope_labels, labels);
        assert_eq!(
            capture.timing,
            *copied(&planned, if index == 0 { 'a' } else { 'b' }).capture_timing()
        );
    }
    assert_ne!(
        planned.trace[0].capture.as_ref().unwrap().timing,
        planned.trace[1].capture.as_ref().unwrap().timing
    );
    assert_eq!(
        planned.trace[1].captured_child_label.as_deref(),
        Some("Inner")
    );
    copied(&planned, 'a').validate_capture(&document).unwrap();
    copied(&planned, 'b').validate_capture(&document).unwrap();
    assert_inverse(&document, &planned);
}

#[test]
fn extending_object_converts_to_time_but_finished_object_survives_independent_motion() {
    let document = nested();
    let entry = selected("outer", 3, "inner");
    let extending = plan(
        &document,
        entry.clone(),
        vec![select(InnerGroup), motion(false, 2)],
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(extending.trace[0].after.cursor, ProjectFrame(9));
    assert_eq!(
        extending.trace[0].after.selected_child,
        entry.selected_child
    );
    assert_eq!(
        extending.context.visual_selection,
        Some(SemanticVisualSelection::Time {
            anchor: ProjectFrame(5),
            head: ProjectFrame(7),
            extending: true,
        })
    );
    assert!(extending.request.is_none());
    let finished = plan(
        &document,
        entry,
        vec![
            select(InnerGroup),
            SemanticInstruction::FinishSelection,
            SemanticInstruction::MoveScope { end: true },
            SemanticInstruction::YankSelection {
                register: name('a'),
            },
        ],
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(finished.context.cursor, ProjectFrame(10));
    assert_eq!(finished.context.selected_child, Some(node("after")));
    assert_eq!(
        finished.context.visual_selection,
        Some(SemanticVisualSelection::Object {
            selection: SemanticObjectSelection {
                kind: InnerGroup,
                group: node("inner")
            },
            extending: false,
        })
    );
    assert_eq!(copied(&finished, 'a').parent(), &node("inner"));
    assert_eq!(copied(&finished, 'a').range(), range(5, 9));
    assert_inverse(&document, &finished);
}

#[test]
fn inner_cut_preserves_inside_or_outside_navigation_and_exact_empty_siblings() {
    let document = nested();
    for entry in [selected("outer", 3, "inner"), selected("inner", 6, "body")] {
        let planned = plan(
            &document,
            entry.clone(),
            vec![cut_object(InnerGroup)],
            &BTreeMap::new(),
        )
        .unwrap();
        assert_eq!(children(&planned.document, "inner"), Vec::<NodeId>::new());
        for removed in ["empty-left", "body", "empty-right"] {
            assert!(!planned.document.nodes().contains_key(&node(removed)));
        }
        assert_eq!(planned.context.parent, entry.parent);
        assert_eq!(planned.context.cursor, ProjectFrame(5));
        assert_eq!(
            planned.context.selected_child,
            (entry.parent == node("outer")).then(|| node("inner"))
        );
        assert_eq!(planned.context.visual_selection, None);
        assert_eq!(planned.trace[0].resolved_parent, Some(node("inner")));
        assert_inverse(&document, &planned);
    }
}

#[test]
fn around_cut_continues_in_outer_scope_and_counted_calls_use_that_new_scope() {
    let document = nested();
    let bank = BTreeMap::from([(name('m'), macro_value(vec![cut_object(AroundGroup)]))]);
    let planned = plan(
        &document,
        selected("inner", 6, "body"),
        vec![call('m', 2)],
        &bank,
    )
    .unwrap();
    assert_eq!(planned.trace[1].after.parent, node("outer"));
    assert_eq!(planned.trace[1].after.selected_child, Some(node("after")));
    assert_eq!(planned.trace[2].before.parent, node("outer"));
    assert_eq!(planned.trace[2].resolved_parent, Some(node("root")));
    assert_eq!(
        planned.trace[2].capture.as_ref().unwrap().scope,
        Vec::<NodeId>::new()
    );
    assert_eq!(planned.trace[2].before_revision, revision("leaf-0"));
    assert_eq!(planned.context, selected("root", 2, "suffix"));
    assert_eq!(
        children(&planned.document, "root"),
        vec![node("prefix"), node("suffix")]
    );
    assert_inverse(&document, &planned);
}

#[test]
fn around_empty_child_cut_chooses_literal_following_empty_sibling() {
    let document = tree(
        &["first", "second", "held"],
        vec![
            ("first", BeatNode::sequence("First", vec![])),
            ("second", BeatNode::sequence("Second", vec![])),
            ("held", hold(4)),
        ],
    );
    let planned = plan(
        &document,
        context("first", 0),
        vec![cut_object(AroundGroup)],
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(planned.context, selected("root", 0, "second"));
    assert_eq!(copied(&planned, 'a').duration().frames(), 0);
    assert_inverse(&document, &planned);
}

#[test]
fn group_and_repeat_objects_select_the_correct_surviving_navigation_owner() {
    let document = nested();
    for kind in [InnerGroup, AroundGroup] {
        for inside in [false, true] {
            for repeat in [false, true] {
                let entry = if inside {
                    selected("inner", 6, "body")
                } else {
                    selected("outer", 3, "inner")
                };
                let instruction = if repeat {
                    SemanticInstruction::Repeat {
                        selector: object(kind),
                        plays: NonZeroU32::new(2).unwrap(),
                        escalation: None,
                    }
                } else {
                    SemanticInstruction::Group {
                        selector: object(kind),
                        label: "Wrapped".into(),
                    }
                };
                let planned = plan(&document, entry, vec![instruction], &BTreeMap::new()).unwrap();
                let expected_parent = if inside && kind == InnerGroup {
                    "inner"
                } else {
                    "outer"
                };
                let expected_selected = if !inside && kind == InnerGroup {
                    "inner"
                } else if repeat {
                    "repeat-0"
                } else {
                    "group-0"
                };
                assert_eq!(
                    planned.context,
                    selected(expected_parent, 5, expected_selected)
                );
                assert_eq!(
                    planned.trace[0].resolved_parent,
                    Some(node(if kind == InnerGroup { "inner" } else { "outer" }))
                );
                assert_inverse(&document, &planned);
            }
        }
    }
}

#[test]
fn empty_contents_are_explicit_and_all_empty_children_remain_real_objects() {
    for children_exist in [false, true] {
        let document = if children_exist {
            tree(
                &["group"],
                vec![
                    (
                        "group",
                        BeatNode::sequence("Group", vec![node("a"), node("b")]),
                    ),
                    ("a", BeatNode::sequence("A", vec![])),
                    ("b", BeatNode::sequence("B", vec![])),
                ],
            )
        } else {
            tree(
                &["group"],
                vec![("group", BeatNode::sequence("Group", vec![]))],
            )
        };
        let entry = selected("root", 0, "group");
        let selected_plan = plan(
            &document,
            entry.clone(),
            vec![select(InnerGroup), SemanticInstruction::FinishSelection],
            &BTreeMap::new(),
        )
        .unwrap();
        assert!(selected_plan.request.is_none());
        assert!(matches!(
            selected_plan.context.visual_selection,
            Some(SemanticVisualSelection::Object {
                extending: false,
                ..
            })
        ));
        for instruction in [
            yank(InnerGroup, 'a'),
            cut_object(InnerGroup),
            SemanticInstruction::Group {
                selector: object(InnerGroup),
                label: "Wrapped".into(),
            },
        ] {
            let result = plan(
                &document,
                entry.clone(),
                vec![instruction],
                &BTreeMap::new(),
            );
            if children_exist {
                assert_inverse(&document, &result.unwrap());
            } else {
                assert_eq!(
                    result.unwrap_err().code,
                    EditErrorCode::SelectionUnavailable
                );
            }
        }
        assert!(
            plan(
                &document,
                entry,
                vec![SemanticInstruction::Repeat {
                    selector: object(InnerGroup),
                    plays: NonZeroU32::new(2).unwrap(),
                    escalation: None,
                }],
                &BTreeMap::new()
            )
            .is_err()
        );
    }
}

#[test]
fn staged_inner_yank_and_object_paste_replace_exact_children_in_one_transaction() {
    let document = nested();
    for before in [false, true] {
        let planned = plan(
            &document,
            selected("outer", 3, "inner"),
            vec![
                yank(InnerGroup, 'a'),
                select(InnerGroup),
                SemanticInstruction::Paste {
                    register: name('a'),
                    before,
                },
                yank(InnerGroup, 'b'),
            ],
            &BTreeMap::new(),
        )
        .unwrap();
        assert_eq!(planned.context, selected("outer", 5, "inner"));
        assert_eq!(
            planned.document.duration().unwrap(),
            document.duration().unwrap()
        );
        for old in ["empty-left", "body", "empty-right"] {
            assert!(!planned.document.nodes().contains_key(&node(old)));
        }
        assert_eq!(
            children(&planned.document, "inner"),
            vec![node("paste-1-0")]
        );
        assert_eq!(planned.trace[2].removed_range, Some(range(5, 9)));
        assert_eq!(planned.trace[2].resolved_parent, Some(node("inner")));
        assert_eq!(copied(&planned, 'b').revision_id(), &revision("leaf-1"));
        assert_eq!(
            planned.trace[3].capture.as_ref().unwrap().scope_labels,
            vec!["Outer", "Inner"]
        );
        assert_inverse(&document, &planned);
    }
}

#[test]
fn empty_inner_object_paste_inserts_at_zero_and_around_replacement_leaves_deleted_scope() {
    let document = tree(
        &["group", "held"],
        vec![
            ("group", BeatNode::sequence("Group", vec![])),
            ("held", hold(3)),
        ],
    );
    let copied = CapturedEditSlice::capture_selection(
        &document,
        &node("root"),
        &SliceCaptureSelection::Child { node: node("held") },
        AudioTimingId {
            allocation: revision("copy"),
            ordinal: 0,
        },
    )
    .unwrap();
    let bank = BTreeMap::from([(
        name('a'),
        Arc::new(RegisterValue::Edited {
            slice: Arc::new(copied),
        }),
    )]);
    for inside in [false, true] {
        for kind in [InnerGroup, AroundGroup] {
            let entry = if inside {
                context("group", 0)
            } else {
                selected("root", 2, "group")
            };
            let planned = plan(
                &document,
                entry,
                vec![
                    select(kind),
                    SemanticInstruction::ReplaceSelection {
                        register: name('a'),
                    },
                ],
                &bank,
            )
            .unwrap();
            let parent = if inside && kind == InnerGroup {
                "group"
            } else {
                "root"
            };
            let selected = if !inside && kind == InnerGroup {
                "group"
            } else {
                "paste-0-0"
            };
            assert_eq!(planned.context, self::selected(parent, 0, selected));
            assert_eq!(planned.trace[1].removed_range, Some(range(0, 0)));
            assert_eq!(planned.trace[1].resolved_range, Some(range(0, 3)));
            assert_eq!(
                planned.document.nodes().contains_key(&node("group")),
                kind == InnerGroup
            );
            assert_inverse(&document, &planned);
        }
    }
}

#[test]
fn original_object_replacement_uses_effective_parent_and_resolves_each_staged_document() {
    let (asset_document, original, source) = super::content::original_fixture();
    let mut document = nested();
    document.assets = asset_document.assets;
    document.validate().unwrap();
    let bank = BTreeMap::from([(name('a'), original)]);
    let mut observed = Vec::new();
    let planned = plan_semantic(
        &document,
        &selected("inner", 6, "body"),
        &program(vec![
            select(InnerGroup),
            SemanticInstruction::ReplaceSelection {
                register: name('a'),
            },
            select(AroundGroup),
            SemanticInstruction::Paste {
                register: name('a'),
                before: false,
            },
        ]),
        SemanticRegisterBank {
            entries: &bank,
            version: 1,
        },
        revision("outer-revision"),
        allocate,
        |staged, _| {
            observed.push(staged.revision_id().clone());
            Ok(source.clone())
        },
    )
    .unwrap();
    assert_eq!(observed, vec![revision("base"), revision("leaf-0")]);
    assert_eq!(planned.context, selected("outer", 5, "original-1"));
    assert!(!planned.document.nodes().contains_key(&node("inner")));
    assert_eq!(planned.trace[1].removed_range, Some(range(5, 9)));
    assert_eq!(planned.trace[3].removed_range, Some(range(5, 35)));
    assert_inverse(&document, &planned);
}

#[test]
fn original_object_replacement_is_admitted_at_the_document_node_limit() {
    let (asset_document, original, source) = super::content::original_fixture();
    let mut document = ProjectDocument::new(
        ProjectId::new("macro").unwrap(),
        revision("base"),
        crate::basis::default_basis(),
        node("root"),
    )
    .unwrap();
    let mut children = Vec::with_capacity(crate::MAX_DOCUMENT_NODES - 1);
    for index in 0..(crate::MAX_DOCUMENT_NODES - 1) {
        let id = if index == 0 {
            node("group")
        } else {
            node(&format!("fill-{index}"))
        };
        children.push(id.clone());
        document
            .nodes
            .insert(id, BeatNode::sequence("Empty", vec![]));
    }
    document
        .nodes
        .insert(node("root"), BeatNode::sequence("Root", children));
    document.assets = asset_document.assets;
    document.validate().unwrap();
    assert_eq!(document.nodes().len(), crate::MAX_DOCUMENT_NODES);

    let bank = BTreeMap::from([(name('a'), original)]);
    let planned = plan_semantic(
        &document,
        &selected("root", 0, "group"),
        &program(vec![
            select(AroundGroup),
            SemanticInstruction::ReplaceSelection {
                register: name('a'),
            },
        ]),
        SemanticRegisterBank {
            entries: &bank,
            version: 1,
        },
        revision("outer-revision"),
        allocate,
        |_, _| Ok(source.clone()),
    )
    .unwrap();

    assert_eq!(planned.document.nodes().len(), crate::MAX_DOCUMENT_NODES);
    assert!(!planned.document.nodes().contains_key(&node("group")));
    assert!(planned.document.nodes().contains_key(&node("original-0")));
    assert_eq!(planned.trace[1].removed_range, Some(range(0, 0)));
}

#[test]
fn stale_foreign_and_nonordinary_objects_refuse_without_fallback_or_allocation() {
    let document = nested();
    for group in ["root", "missing", "prefix", "inner", "body"] {
        let entry = SemanticContext {
            visual_selection: Some(SemanticVisualSelection::Object {
                selection: SemanticObjectSelection {
                    kind: InnerGroup,
                    group: node(group),
                },
                extending: false,
            }),
            ..selected("root", 6, "outer")
        };
        assert!(
            plan_semantic(
                &document,
                &entry,
                &program(vec![SemanticInstruction::YankSelection {
                    register: name('a')
                }]),
                SemanticRegisterBank {
                    entries: &BTreeMap::new(),
                    version: 0
                },
                revision("outer-revision"),
                |_| panic!("invalid target cannot allocate"),
                no_original
            )
            .is_err()
        );
    }
    let mut under_repeat = document.clone();
    under_repeat.nodes.get_mut(&node("outer")).unwrap().kind = NodeKind::Repeat {
        child: node("inner"),
        iterations: crate::IterationOrder::new(revision("plays"), 2).unwrap(),
        gap: None,
        escalation: None,
    };
    under_repeat.nodes.remove(&node("lead"));
    under_repeat.nodes.remove(&node("after"));
    under_repeat.validate().unwrap();
    assert!(
        under_repeat
            .resolve_group_object(&context("inner", 3), AroundGroup)
            .is_err()
    );
    let entry = document
        .select_semantic_object(&selected("outer", 3, "inner"), InnerGroup)
        .unwrap();
    let mut mismatched = entry;
    mismatched.cursor = ProjectFrame(8);
    assert!(
        plan(
            &document,
            mismatched,
            vec![SemanticInstruction::FinishSelection],
            &BTreeMap::new()
        )
        .is_err()
    );
}

#[test]
fn late_object_failure_and_wrong_replacement_identity_pool_leave_inputs_unchanged() {
    let document = nested();
    let snapshot = document.clone();
    assert!(
        plan(
            &document,
            selected("inner", 6, "body"),
            vec![
                cut_object(AroundGroup),
                select(InnerGroup),
                SemanticInstruction::Paste {
                    register: name('z'),
                    before: true
                }
            ],
            &BTreeMap::new()
        )
        .is_err()
    );
    assert_eq!(document, snapshot);
    let error = plan_semantic(
        &document,
        &selected("outer", 3, "inner"),
        &program(vec![
            yank(InnerGroup, 'a'),
            select(InnerGroup),
            SemanticInstruction::ReplaceSelection {
                register: name('a'),
            },
        ]),
        SemanticRegisterBank {
            entries: &BTreeMap::new(),
            version: 0,
        },
        revision("outer-revision"),
        |request| {
            if let SemanticAllocationRequest::PasteEdited {
                required_split_ids, ..
            } = request
            {
                assert_eq!(required_split_ids, 0);
            }
            let mut allocation = allocate(request)?;
            if let SemanticAllocation::PasteEdited {
                split_identities, ..
            } = &mut allocation
            {
                split_identities.nodes.push(node("unexpected"));
            }
            Ok(allocation)
        },
        no_original,
    )
    .unwrap_err();
    assert_eq!(error.code, EditErrorCode::InvalidCommand);
    assert_eq!(document, snapshot);
}

#[test]
fn object_and_time_wire_is_tagged_closed_and_keeps_programs_relative() {
    let body = program(vec![select(InnerGroup), yank(AroundGroup, 'a')]);
    let wire = serde_json::to_string(&body).unwrap();
    assert_eq!(
        serde_json::from_str::<SemanticProgram>(&wire).unwrap(),
        body
    );
    assert!(!wire.contains("group_id"));
    for invalid in [
        r#"{"instructions":[{"type":"select_object","object":{"type":"inner_group","node":"fixed"}}]}"#,
        r#"{"instructions":[{"type":"select_object","object":{"type":"around_group"},"group":"fixed"}]}"#,
        r#"{"instructions":[{"type":"yank","register":"a","selector":{"type":"text_object","object":{"type":"around_group"},"range":[0,1]}}]}"#,
        r#"{"instructions":[{"type":"select_object","object":{"type":"inner_beat"}}]}"#,
    ] {
        assert!(
            serde_json::from_str::<SemanticProgram>(invalid).is_err(),
            "{invalid}"
        );
    }
    for selection in [
        SemanticVisualSelection::Time {
            anchor: ProjectFrame(1),
            head: ProjectFrame(2),
            extending: false,
        },
        SemanticVisualSelection::Object {
            selection: SemanticObjectSelection {
                kind: InnerGroup,
                group: node("inner"),
            },
            extending: true,
        },
    ] {
        let value = serde_json::to_value(&selection).unwrap();
        assert!(value.get("type").is_some());
        assert_eq!(
            serde_json::from_value::<SemanticVisualSelection>(value.clone()).unwrap(),
            selection
        );
        let mut corrupted = value;
        corrupted["range"] = serde_json::json!([1, 2]);
        assert!(serde_json::from_value::<SemanticVisualSelection>(corrupted).is_err());
    }
    for invalid in [
        r#"{"anchor":1,"head":2,"extending":true}"#,
        r#"{"type":"object","selection":{"kind":{"type":"inner_group"},"group":"inner","parent":"forged"},"extending":false}"#,
        r#"{"type":"object","selection":{"kind":{"type":"around_group","extra":true},"group":"inner"},"extending":true}"#,
    ] {
        assert!(
            serde_json::from_str::<SemanticVisualSelection>(invalid).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn zero_duration_edited_object_replacement_removes_old_contents_but_time_paste_remains_seam() {
    let document = nested();
    let slice = Arc::new(
        CapturedEditSlice::capture_selection(
            &document,
            &node("inner"),
            &SliceCaptureSelection::Child {
                node: node("empty-left"),
            },
            AudioTimingId {
                allocation: revision("empty-copy"),
                ordinal: 0,
            },
        )
        .unwrap(),
    );
    let bank = BTreeMap::from([(name('a'), Arc::new(RegisterValue::Edited { slice }))]);
    let replaced = plan(
        &document,
        selected("outer", 3, "inner"),
        vec![
            select(InnerGroup),
            SemanticInstruction::ReplaceSelection {
                register: name('a'),
            },
        ],
        &bank,
    )
    .unwrap();
    assert_eq!(replaced.context, selected("outer", 5, "inner"));
    assert_eq!(replaced.document.duration().unwrap().frames(), 8);
    assert_eq!(replaced.trace[1].removed_range, Some(range(5, 9)));
    assert_eq!(replaced.trace[1].resolved_range, Some(range(5, 5)));
    assert_eq!(
        children(&replaced.document, "inner"),
        vec![node("paste-0-0")]
    );
    assert!(!replaced.document.nodes().contains_key(&node("body")));
    assert_inverse(&document, &replaced);
    let entry = SemanticContext {
        visual_selection: Some(SemanticVisualSelection::Time {
            anchor: ProjectFrame(5),
            head: ProjectFrame(9),
            extending: false,
        }),
        ..selected("outer", 3, "inner")
    };
    let pasted = plan(
        &document,
        entry,
        vec![SemanticInstruction::Paste {
            register: name('a'),
            before: true,
        }],
        &bank,
    )
    .unwrap();
    assert_eq!(
        pasted.document.duration().unwrap(),
        document.duration().unwrap()
    );
    assert!(pasted.document.nodes().contains_key(&node("body")));
    assert_eq!(
        children(&pasted.document, "outer"),
        vec![
            node("lead"),
            node("paste-0-0"),
            node("inner"),
            node("after")
        ]
    );
    assert_eq!(pasted.trace[0].removed_range, None);
    assert_eq!(pasted.context, selected("outer", 5, "paste-0-0"));
    assert_inverse(&document, &pasted);
}

#[test]
fn legacy_captures_have_exact_provenance_and_select_object_calls_obey_fuel() {
    let document = nested();
    let planned = plan(
        &document,
        selected("inner", 6, "body"),
        vec![
            SemanticInstruction::YankBeat {
                register: name('a'),
            },
            cut(1, 'b'),
        ],
        &BTreeMap::new(),
    )
    .unwrap();
    for (index, register) in [(0, 'a'), (1, 'b')] {
        let trace = &planned.trace[index];
        let capture = trace.capture.as_ref().unwrap();
        assert_eq!(trace.resolved_parent, Some(node("inner")));
        assert_eq!(capture.parent, node("inner"));
        assert_eq!(capture.bounds, range(5, 9));
        assert_eq!(capture.scope, vec![node("outer"), node("inner")]);
        assert_eq!(capture.timing, *copied(&planned, register).capture_timing());
    }
    assert_inverse(&document, &planned);
    let bank = BTreeMap::from([(name('m'), macro_value(vec![select(InnerGroup)]))]);
    let error = plan_semantic(
        &document,
        &selected("outer", 3, "inner"),
        &program(vec![call('m', 4096)]),
        SemanticRegisterBank {
            entries: &bank,
            version: 0,
        },
        revision("outer-revision"),
        |_| panic!("object selection cannot allocate"),
        no_original,
    )
    .unwrap_err();
    assert_eq!(error.code, EditErrorCode::LimitExceeded);
}
