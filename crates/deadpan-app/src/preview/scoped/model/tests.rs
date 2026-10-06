use super::*;
use deadpan_core::{
    BeatNode, ColorPolicy, Command, CommandRequest, FrameRange, FrameRate, HoldAudio, HoldRecipe,
    HoldVideo, IterationOrder, MarkId, OccurrenceIdentities, PitchPolicy, PlayOverride,
    PlayOverrides, PresentationBasis, ProjectId, RetimePurpose, RevisionId, ScopedNodeEdit,
    Subtree, apply, prepare_scoped_edit,
};

fn node(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}
fn revision(name: &str) -> RevisionId {
    RevisionId::new(name).unwrap()
}
fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn recipe(value: i64) -> HoldRecipe {
    HoldRecipe {
        picture_context: None,
        duration: frames(value),
        video: HoldVideo::Background,
        audio: HoldAudio::Silence,
    }
}
fn hold(name: &str, value: i64) -> (NodeId, BeatNode) {
    (node(name), BeatNode::hold(name, recipe(value)))
}
fn sequence(name: &str, children: &[&str]) -> (NodeId, BeatNode) {
    (
        node(name),
        BeatNode::sequence(name, children.iter().map(|name| node(name)).collect()),
    )
}
fn repeat(name: &str, child: &str, plays: u32, gap: Option<i64>) -> (NodeId, BeatNode) {
    let mut beat = BeatNode::sequence(name, Vec::new());
    beat.kind = NodeKind::Repeat {
        child: node(child),
        iterations: IterationOrder::new(revision(name), plays).unwrap(),
        gap: gap.map(recipe),
        escalation: None,
    };
    (node(name), beat)
}
fn retime(name: &str, child: &str, start: i64, end: i64, output: i64) -> (NodeId, BeatNode) {
    let mut beat = BeatNode::sequence(name, Vec::new());
    beat.kind = NodeKind::Retime {
        child: node(child),
        duration: frames(output),
        mapping: FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap(),
        pitch: PitchPolicy::Preserve,
        purpose: RetimePurpose::Edit,
    };
    (node(name), beat)
}
fn overrides(name: &str, entries: &[(u32, &str)]) -> (NodeId, PlayOverrides) {
    (
        node(name),
        PlayOverrides::try_from(
            entries
                .iter()
                .map(|(ordinal, root)| PlayOverride {
                    iteration: deadpan_core::IterationId {
                        allocation: revision(name),
                        ordinal: *ordinal,
                    },
                    root: node(root),
                })
                .collect::<Vec<_>>(),
        )
        .unwrap(),
    )
}
fn edited(document: &ProjectDocument, command: Command, name: &str) -> ProjectDocument {
    apply(
        document,
        &CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: revision(name),
            command,
        },
    )
    .unwrap()
    .forward
    .apply(document)
    .unwrap()
}
fn fixture(
    root: &str,
    nodes: Vec<(NodeId, BeatNode)>,
    overrides: BTreeMap<NodeId, PlayOverrides>,
    gap_overrides: BTreeMap<NodeId, PlayOverrides>,
) -> ProjectDocument {
    let empty = ProjectDocument::new(
        ProjectId::new("scoped-navigation").unwrap(),
        revision("empty"),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30, 1).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )
    .unwrap();
    edited(
        &empty,
        Command::Insert {
            parent: node("root"),
            index: 0,
            subtree: Subtree {
                root: node(root),
                nodes: nodes.into_iter().collect(),
                overrides,
                gap_overrides,
            },
        },
        "seed",
    )
}
fn index(document: ProjectDocument) -> Index {
    let plan = Arc::new(RenderPlan::compile(&document).unwrap());
    Index::new(Arc::new(document), plan).unwrap()
}
fn state(document: ProjectDocument, root: &str, cursor: i64) -> State {
    State::new_in(
        17,
        SequenceScope::default(),
        node(root),
        ProjectFrame(cursor),
        index(document),
    )
    .unwrap()
}
fn branch(document: &ProjectDocument, name: &str, position: Option<u32>) -> RepeatEditStep {
    let NodeKind::Repeat { iterations, .. } = &document.nodes()[&node(name)].kind else {
        panic!("expected Repeat fixture");
    };
    RepeatEditStep {
        repeat: node(name),
        branch: position.map_or(RepeatEditBranch::Default, |position| {
            RepeatEditBranch::Play {
                iteration: iterations.at(position).expect("fixture play position"),
            }
        }),
    }
}
fn presentation(state: &State) -> &Presentation {
    state
        .projection
        .presentation
        .as_ref()
        .expect("sampled picture")
}

#[test]
fn nested_authored_choices_remain_distinct_from_entry_cursor_occurrence() {
    let document = fixture(
        "outer",
        vec![
            repeat("outer", "group", 3, None),
            sequence("group", &["first", "inner"]),
            hold("first", 2),
            repeat("inner", "leaf", 2, None),
            hold("leaf", 2),
        ],
        BTreeMap::new(),
        BTreeMap::new(),
    );
    let mut state = state(document, "outer", 15);
    assert_eq!(state.selected.node, node("group"));
    assert_eq!(
        state.selected.repeats,
        vec![branch(&state.index.document, "outer", None)]
    );
    assert_eq!(
        presentation(&state).instance.repeats[0].iteration.ordinal,
        2
    );
    assert_eq!(presentation(&state).frames, 12..18);
    state.enter_in().unwrap();
    state.step_in(true, u32::MAX).unwrap();
    assert_eq!(state.selected.node, node("inner"));
    assert!(state.enter_in().unwrap());
    assert_eq!(
        state.selected.repeats,
        vec![
            branch(&state.index.document, "outer", None),
            branch(&state.index.document, "inner", None)
        ]
    );
    assert_eq!(presentation(&state).frames, 14..16);
    state.switch_play_in(2).unwrap();
    assert_eq!(
        state.selected.repeats,
        vec![
            branch(&state.index.document, "outer", None),
            branch(&state.index.document, "inner", Some(1))
        ]
    );
    assert_eq!(presentation(&state).frames, 16..18);
    assert!(state.leave_in().unwrap());
    assert_eq!(state.selected.node, node("inner"));
    assert!(state.leave_in().unwrap());
    state.switch_play_in(2).unwrap();
    assert_eq!(
        state.selected.repeats,
        vec![branch(&state.index.document, "outer", Some(1))]
    );
    assert_eq!(presentation(&state).frames, 6..12);
    assert!(!state.leave_in().unwrap());
    assert_eq!(state.scope, SequenceScope::default());
}

#[test]
fn nested_scope_metadata_names_every_branch_and_the_repeat_controls_will_change() {
    let document = fixture(
        "outer",
        vec![
            repeat("outer", "group", 3, None),
            sequence("group", &["first", "inner"]),
            hold("first", 2),
            repeat("inner", "leaf", 2, None),
            hold("leaf", 2),
        ],
        BTreeMap::new(),
        BTreeMap::new(),
    );
    let mut state = state(document, "outer", 6);
    state.switch_play_in(2).unwrap();
    state.enter_in().unwrap();
    let parent_labels = state.breadcrumbs.clone();
    state.select_in(1).unwrap();
    assert!(Arc::ptr_eq(&parent_labels, &state.breadcrumbs));
    assert_eq!(state.selected.node, node("inner"));
    assert_eq!(state.breadcrumbs.as_ref(), &["outer [play 2/3]", "group"]);
    assert_eq!(state.scope_label, "outer · play 2/3");
    assert_eq!(
        state.repeat_choice.as_ref().unwrap().owner_label,
        "outer",
        "selecting Inner does not enter its scope"
    );

    state.enter_in().unwrap();
    assert_eq!(
        state.breadcrumbs.as_ref(),
        &["outer [play 2/3]", "group", "inner [all plays]"]
    );
    assert_eq!(state.scope_label, "outer · play 2/3 › inner · all plays");
    assert_eq!(
        state.repeat_choice,
        Some(RepeatChoice {
            owner_label: "inner".into(),
            label: "all plays".into(),
            one_based: None,
            plays: 2
        })
    );
    let nested_labels = state.breadcrumbs.clone();
    state.switch_play_in(2).unwrap();
    assert_eq!(state.scope_label, "outer · play 2/3 › inner · play 2/2");
    assert_eq!(state.repeat_choice.as_ref().unwrap().one_based, Some(2));
    state.switch_in(RepeatEditBranch::Default).unwrap();
    assert_eq!(state.scope_label, "outer · play 2/3 › inner · all plays");
    assert_eq!(
        state.selected.repeats,
        vec![
            branch(&state.index.document, "outer", Some(1)),
            branch(&state.index.document, "inner", None)
        ]
    );
    state.leave_in().unwrap();
    assert_eq!(state.breadcrumbs.as_ref(), parent_labels.as_ref());
    assert_eq!(state.scope_label, "outer · play 2/3");
    assert_eq!(state.repeat_choice.as_ref().unwrap().owner_label, "outer");
    assert_eq!(
        nested_labels.as_ref(),
        &["outer [play 2/3]", "group", "inner [all plays]"],
        "retained paint metadata stays unchanged"
    );
}

#[test]
fn fully_overridden_default_stays_editable_without_inventing_a_play() {
    let document = fixture(
        "repeat",
        vec![
            repeat("repeat", "default", 2, None),
            hold("default", 3),
            hold("first", 2),
            hold("second", 4),
        ],
        [overrides("repeat", &[(0, "first"), (1, "second")])].into(),
        BTreeMap::new(),
    );
    let mut state = state(document, "repeat", 4);
    assert_eq!(state.selected.node, node("default"));
    assert!(state.projection.presentation.is_none());
    assert!(state.projection.exact.is_none());
    assert_eq!(state.projection.reason.as_deref(), Some(DORMANT_DEFAULT));
    state.selected.validate(&state.index.document).unwrap();
    state.switch_play_in(2).unwrap();
    assert_eq!(state.selected.node, node("second"));
    assert_eq!(presentation(&state).frames, 2..6);
    state.switch_in(RepeatEditBranch::Default).unwrap();
    assert_eq!(state.selected.node, node("default"));
    assert_eq!(state.projection.reason.as_deref(), Some(DORMANT_DEFAULT));
}

#[test]
fn owned_gap_rows_include_dormant_final_gap_but_never_implicit_recipes() {
    let document = fixture(
        "repeat",
        vec![
            repeat("repeat", "body", 3, Some(1)),
            hold("body", 2),
            hold("first-gap", 3),
            hold("last-gap", 4),
        ],
        BTreeMap::new(),
        [overrides("repeat", &[(0, "first-gap"), (2, "last-gap")])].into(),
    );
    let mut state = state(document, "repeat", 0);
    assert_eq!(state.rows.len(), 1, "All plays has only its literal body");
    state.switch_play_in(1).unwrap();
    assert_eq!(state.rows.len(), 2);
    state.select_in(1).unwrap();
    assert_eq!(state.selected.node, node("first-gap"));
    assert_eq!(presentation(&state).frames, 2..5);
    state.switch_play_in(2).unwrap();
    assert_eq!(
        state.rows.len(),
        1,
        "implicit gap recipe has no authored row"
    );
    state.switch_play_in(3).unwrap();
    state.select_in(1).unwrap();
    state.selected.validate(&state.index.document).unwrap();
    assert_eq!(state.selected.node, node("last-gap"));
    assert!(state.projection.presentation.is_none());
    assert_eq!(state.projection.reason.as_deref(), Some(DORMANT_GAP));
}

#[test]
fn fractional_visibility_uses_real_frame_centers_and_preserves_exact_interval() {
    let document = fixture(
        "retime",
        vec![
            retime("retime", "body", 0, 2, 1),
            sequence("body", &["a", "b"]),
            hold("a", 1),
            hold("b", 1),
        ],
        BTreeMap::new(),
        BTreeMap::new(),
    );
    let mut state = state(document, "retime", 0);
    assert_eq!(state.scope_label, "Retime child");
    assert_eq!(state.breadcrumbs.as_ref(), &["retime"]);
    assert!(state.repeat_choice.is_none());
    assert_eq!(presentation(&state).frames, 0..1);
    state.enter_in().unwrap();
    assert_eq!(
        state.projection.exact,
        Some(ExactRatio::ZERO..ExactRatio::new(1, 2).unwrap())
    );
    assert!(
        state.projection.presentation.is_none(),
        "[0, 1/2) contains no root frame center"
    );
    assert_eq!(state.projection.reason.as_deref(), Some(UNSAMPLED));
    state.select_in(1).unwrap();
    assert_eq!(presentation(&state).frames, 0..1);
    assert_eq!(
        presentation(&state).exact,
        ExactRatio::new(1, 2).unwrap()..ExactRatio::ONE
    );
    assert_eq!(presentation(&state).instance.node, node("b"));
    assert!(
        state
            .index
            .plan
            .picture(ProjectFrame(0))
            .unwrap()
            .framing
            .iter()
            .any(|entry| entry.instance == presentation(&state).instance)
    );
}

#[test]
fn retime_crop_hides_children_without_turning_intrinsic_duration_into_root_time() {
    let document = fixture(
        "retime",
        vec![
            retime("retime", "body", 2, 4, 3),
            sequence("body", &["a", "b"]),
            hold("a", 2),
            hold("b", 2),
        ],
        BTreeMap::new(),
        BTreeMap::new(),
    );
    let mut state = state(document, "retime", 0);
    state.enter_in().unwrap();
    assert_eq!(state.selected.node, node("a"));
    assert!(state.projection.exact.is_none());
    assert!(state.projection.presentation.is_none());
    assert_eq!(state.projection.reason.as_deref(), Some(CROPPED));
    state.select_in(1).unwrap();
    assert_eq!(presentation(&state).frames, 0..3);
    assert_eq!(
        presentation(&state).exact,
        ExactRatio::ZERO..ExactRatio::integer(3)
    );
}

#[test]
fn huge_repeat_uses_sparse_overrides_and_prefers_the_cursor_play() {
    let document = fixture(
        "repeat",
        vec![
            repeat("repeat", "body", 1_000_000_000, None),
            hold("body", 1),
            hold("first", 1),
            hold("second", 1),
        ],
        [overrides("repeat", &[(0, "first"), (1, "second")])].into(),
        BTreeMap::new(),
    );
    let mut state = state(document.clone(), "repeat", 999_999_998);
    assert_eq!(
        presentation(&state).instance.repeats[0].iteration.ordinal,
        999_999_998
    );
    assert_eq!(presentation(&state).frames, 999_999_998..999_999_999);
    assert_eq!(state.index.overridden[&node("repeat")], vec![0, 1]);
    assert_eq!(state.rows.len(), 1);
    state.switch_play_in(1_000_000_000).unwrap();
    assert_eq!(presentation(&state).frames, 999_999_999..1_000_000_000);
    assert!(state.switch_play_in(0).is_err());
    assert!(state.switch_play_in(1_000_000_001).is_err());
    let state = State::new_in(
        17,
        SequenceScope::default(),
        node("repeat"),
        ProjectFrame(0),
        index(document),
    )
    .unwrap();
    assert_eq!(
        presentation(&state).instance.repeats[0].iteration.ordinal,
        2,
        "skip sparse overridden prefix instead of selecting its content as Default"
    );
}

#[test]
fn bounded_default_search_reports_only_this_representative_without_claiming_global_inactivity() {
    // A quarter-frame default occurs twice before the visible third default.
    // Cursor is in the long override, so the bounded candidates may have no
    // sampled picture even though a later default does.
    let document = fixture(
        "retime",
        vec![
            retime("retime", "repeat", 0, 8, 2),
            repeat("repeat", "body", 5, None),
            hold("body", 1),
            hold("override", 4),
        ],
        [overrides("repeat", &[(0, "override")])].into(),
        BTreeMap::new(),
    );
    let mut state = state(document, "retime", 0);
    state.enter_in().unwrap();
    assert_eq!(state.projection.reason.as_deref(), Some(UNSAMPLED));
    assert!(state.projection.exact.is_some());
    assert!(state.projection.presentation.is_none());
    state.switch_play_in(4).unwrap();
    assert_eq!(presentation(&state).frames, 1..2);
    assert_eq!(
        presentation(&state).exact,
        ExactRatio::new(3, 2).unwrap()..ExactRatio::new(7, 4).unwrap()
    );
}

#[test]
fn row_cache_contains_only_current_level_and_selection_updates_cached_projection() {
    let document = fixture(
        "repeat",
        vec![
            repeat("repeat", "body", 2, None),
            sequence("body", &["a", "empty", "b"]),
            hold("a", 2),
            sequence("empty", &[]),
            hold("b", 3),
        ],
        BTreeMap::new(),
        BTreeMap::new(),
    );
    let mut state = state(document, "repeat", 0);
    state.enter_in().unwrap();
    let old_rows = state.rows.clone();
    assert!(Arc::ptr_eq(&old_rows, &state.rows));
    state.select_in(1).unwrap();
    assert_eq!(state.selected.node, node("empty"));
    assert!(state.rows[1].selected);
    assert!(!state.rows[0].selected);
    assert!(
        old_rows[0].selected,
        "retained paint snapshot cannot be mutated"
    );
    assert!(state.projection.presentation.is_none());
    assert!(!state.enter_in().unwrap());
    state.step_in(true, u32::MAX).unwrap();
    assert_eq!(presentation(&state).frames, 2..5);
    state.step_in(false, u32::MAX).unwrap();
    assert_eq!(presentation(&state).frames, 0..2);
}

#[test]
fn unrelated_revision_rejects_and_exact_receipt_rebases_without_entry_cursor_equality() {
    let document = fixture(
        "repeat",
        vec![repeat("repeat", "body", 2, None), hold("body", 3)],
        BTreeMap::new(),
        BTreeMap::new(),
    );
    let mut state = state(document.clone(), "repeat", 0);
    let after = edited(
        &document,
        Command::Rename {
            node: node("body"),
            label: "Renamed".into(),
        },
        "renamed",
    );
    assert!(!state.matches_identity(17, &after));
    assert!(!state.matches_identity(18, &document));
    let before = Target {
        session: 17,
        project: document.project_id().clone(),
        revision: document.revision_id().clone(),
        scope: SequenceScope::default(),
        root: node("repeat"),
        target: state.selected.clone(),
        presentation: Some(presentation(&state).instance.clone()),
        cursor: ProjectFrame(2),
        also: Vec::new(),
    };
    let commit = Commit {
        before,
        revision: after.revision_id().clone(),
        target: state.selected.clone(),
        presentation: Some(presentation(&state).instance.clone()),
    };
    let mut wrong = commit.clone();
    wrong.before.presentation.as_mut().unwrap().repeats[0]
        .iteration
        .ordinal = 1;
    assert!(!state.reconcile_in(index(after.clone()), &wrong).unwrap());
    assert_eq!(state.index.document.revision_id(), document.revision_id());
    assert!(state.reconcile_in(index(after.clone()), &commit).unwrap());
    assert!(state.matches_identity(17, &after));
    assert_eq!(state.cursor, ProjectFrame(2));
    assert_eq!(state.rows[0].label, "Renamed");
    assert_eq!(presentation(&state).instance, commit.presentation.unwrap());
}

#[test]
fn terminal_cursor_uses_a_visible_sample_instead_of_unsampled_first_plays() {
    let document = fixture(
        "retime",
        vec![
            retime("retime", "repeat", 0, 100, 1),
            repeat("repeat", "body", 100, None),
            hold("body", 1),
        ],
        BTreeMap::new(),
        BTreeMap::new(),
    );
    let mut state = state(document, "retime", 1);
    state.enter_in().unwrap();
    assert_eq!(
        state.selected.repeats,
        vec![branch(&state.index.document, "repeat", None)]
    );
    assert_eq!(presentation(&state).frames, 0..1);
    assert_eq!(
        presentation(&state).instance.repeats[0].iteration.ordinal,
        50
    );
    assert_eq!(
        presentation(&state).exact,
        ExactRatio::new(1, 2).unwrap()..ExactRatio::new(51, 100).unwrap()
    );
}

#[test]
fn nested_play_isolation_receipt_rebuilds_only_the_mapped_authored_path() {
    let document = fixture(
        "outer",
        vec![
            repeat("outer", "group", 3, None),
            sequence("group", &["first", "inner"]),
            hold("first", 2),
            repeat("inner", "leaf", 2, None),
            hold("leaf", 1),
        ],
        BTreeMap::new(),
        BTreeMap::new(),
    );
    let mut state = state(document.clone(), "outer", 6);
    state.switch_play_in(2).unwrap();
    state.enter_in().unwrap();
    state.select_in(1).unwrap();
    state.enter_in().unwrap();
    let before = Target {
        session: 17,
        project: document.project_id().clone(),
        revision: document.revision_id().clone(),
        scope: SequenceScope::default(),
        root: node("outer"),
        target: state.selected.clone(),
        presentation: Some(presentation(&state).instance.clone()),
        cursor: ProjectFrame(6),
        also: Vec::new(),
    };
    let edit = ScopedNodeEdit::Rename {
        label: "Isolated leaf".into(),
    };
    let required = document
        .scoped_edit_requirements(&before.target, &edit)
        .unwrap();
    let prepared = prepare_scoped_edit(
        &document,
        &CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: revision("isolated"),
            command: Command::EditScoped {
                target: before.target.clone(),
                edit,
                identities: OccurrenceIdentities {
                    nodes: (0..required.nodes)
                        .map(|index| node(&format!("isolated-node-{index}")))
                        .collect(),
                    marks: (0..required.marks)
                        .map(|index| MarkId::new(format!("isolated-mark-{index}")).unwrap())
                        .collect(),
                },
            },
        },
    )
    .unwrap();
    let mapped = prepared
        .map_instance(&document, before.presentation.as_ref().unwrap())
        .unwrap();
    let commit = Commit {
        before,
        revision: prepared.document.revision_id().clone(),
        target: prepared.target.clone(),
        presentation: Some(mapped.clone()),
    };
    assert_ne!(commit.target.node, node("leaf"));
    assert_ne!(commit.target.repeats[1].repeat, node("inner"));
    assert!(
        state
            .reconcile_in(index(prepared.document), &commit)
            .unwrap()
    );
    assert_eq!(state.selected, commit.target);
    assert_eq!(presentation(&state).instance, mapped);
    assert_eq!(presentation(&state).frames, 6..7);
    assert_eq!(state.rows[state.selected()].label, "Isolated leaf");
    assert_eq!(
        state.breadcrumbs.as_ref(),
        &["outer [play 2/3]", "group", "inner [all plays]"]
    );
    assert_eq!(state.scope_label, "outer · play 2/3 › inner · all plays");
    assert_eq!(state.repeat_choice.as_ref().unwrap().owner_label, "inner");
    assert_eq!(state.index.document.nodes()[&node("leaf")].label, "leaf");
    assert_eq!(state.scope, SequenceScope::default());
    assert!(state.leave_in().unwrap());
    assert_eq!(state.selected.node, commit.target.repeats[1].repeat);
    assert_eq!(
        state.selected.repeats,
        vec![branch(&state.index.document, "outer", Some(1))]
    );
}

#[test]
fn maximum_valid_document_depth_includes_root_plus_256_edges() {
    let groups = MAX_DOCUMENT_DEPTH - 2;
    let mut nodes = vec![retime("retime", "group-0", 0, 1, 1), hold("leaf", 1)];
    for index in 0..groups {
        let child = if index + 1 == groups {
            "leaf".into()
        } else {
            format!("group-{}", index + 1)
        };
        nodes.push(sequence(&format!("group-{index}"), &[&child]));
    }
    let mut state = state(
        fixture("retime", nodes, BTreeMap::new(), BTreeMap::new()),
        "retime",
        0,
    );
    for _ in 0..groups {
        assert!(state.enter_in().unwrap());
    }
    assert_eq!(state.selected.node, node("leaf"));
    assert_eq!(
        state.index.path(&node("leaf")).unwrap().len(),
        MAX_DOCUMENT_DEPTH + 1
    );
    assert_eq!(presentation(&state).frames, 0..1);
}

#[test]
fn fragmented_iteration_runs_map_sparse_identities_to_authored_positions() {
    let runs = (0..64).rev().map(|index| serde_json::json!({ "allocation": format!("allocation-{index}"), "first": 17, "count": 1 })).collect::<Vec<_>>();
    let mut repeated = repeat("repeat", "body", 64, None);
    let NodeKind::Repeat { iterations, .. } = &mut repeated.1.kind else {
        unreachable!()
    };
    *iterations = serde_json::from_value(serde_json::json!({ "runs": runs })).unwrap();
    let mut nodes = vec![repeated, hold("body", 1)];
    let mut entries = Vec::new();
    for index in 0..64 {
        let root = format!("override-{index}");
        nodes.push(hold(&root, 1));
        entries.push(PlayOverride {
            iteration: deadpan_core::IterationId {
                allocation: revision(&format!("allocation-{index}")),
                ordinal: 17,
            },
            root: node(&root),
        });
    }
    let mut state = state(
        fixture(
            "repeat",
            nodes,
            [(node("repeat"), PlayOverrides::try_from(entries).unwrap())].into(),
            BTreeMap::new(),
        ),
        "repeat",
        0,
    );
    assert_eq!(
        state.index.overridden[&node("repeat")],
        (0..64).collect::<Vec<_>>()
    );
    assert_eq!(state.index.overridden_runs[&node("repeat")], vec![0..64]);
    assert_eq!(state.projection.reason.as_deref(), Some(DORMANT_DEFAULT));
    state.switch_play_in(1).unwrap();
    assert_eq!(state.selected.node, node("override-63"));
    assert_eq!(
        state.repeat_choice.as_ref().unwrap().one_based,
        Some(1),
        "show current position, not stable ordinal 17"
    );
    assert_eq!(state.scope_label, "repeat · play 1/64");
    assert_eq!(presentation(&state).frames, 0..1);
    state.switch_play_in(64).unwrap();
    assert_eq!(state.selected.node, node("override-0"));
    assert_eq!(state.scope_label, "repeat · play 64/64");
    assert_eq!(presentation(&state).frames, 63..64);
}

#[test]
fn maximum_depth_repeat_projection_uses_bounded_heap_continuations() {
    let repeats = MAX_DOCUMENT_DEPTH - 1;
    let mut nodes = vec![hold("leaf", 1)];
    for index in 0..repeats {
        let child = if index + 1 == repeats {
            "leaf".into()
        } else {
            format!("repeat-{}", index + 1)
        };
        nodes.push(repeat(&format!("repeat-{index}"), &child, 1, None));
    }
    let mut state = state(
        fixture("repeat-0", nodes, BTreeMap::new(), BTreeMap::new()),
        "repeat-0",
        0,
    );
    for _ in 1..repeats {
        assert!(state.enter_in().unwrap());
    }
    assert_eq!(state.selected.node, node("leaf"));
    assert_eq!(state.selected.repeats.len(), repeats);
    assert!(
        state
            .selected
            .repeats
            .iter()
            .all(|step| step.branch == RepeatEditBranch::Default)
    );
    assert_eq!(
        state.index.path(&node("leaf")).unwrap().len(),
        MAX_DOCUMENT_DEPTH + 1
    );
    assert_eq!(presentation(&state).frames, 0..1);
    assert_eq!(presentation(&state).instance.repeats.len(), repeats);
    assert!(
        state
            .selected
            .matches_instance(&state.index.document, &presentation(&state).instance)
            .unwrap()
    );
}

fn mark_navigation_fixture() -> ProjectDocument {
    fixture(
        "outer",
        vec![
            repeat("outer", "group", 3, None),
            sequence("group", &["first", "inner"]),
            hold("first", 2),
            repeat("inner", "leaf", 2, None),
            hold("leaf", 2),
        ],
        BTreeMap::new(),
        BTreeMap::new(),
    )
}

fn set_navigation_mark() -> Command {
    Command::SetMark {
        id: MarkId::new("native-mark-a").unwrap(),
        owner: node("root"),
        label: "a".into(),
        boundary: deadpan_core::BoundaryAnchor {
            coordinate: deadpan_core::Anchor::Occurrence {
                instance: InstancePath {
                    node: node("root"),
                    repeats: Vec::new(),
                },
                position: ExactRatio::ZERO,
            },
            bias: InsertionBias::Right,
        },
        loss_policy: deadpan_core::AnchorLossPolicy::KeepUnresolved,
    }
}

fn mark_transition(
    before: &ProjectDocument,
    command: Command,
    name: &str,
) -> (
    Arc<ProjectDocument>,
    Arc<RenderPlan>,
    crate::project::marks::Saved,
) {
    let document = Arc::new(edited(before, command, name));
    let plan = Arc::new(RenderPlan::compile(&document).unwrap());
    let saved = crate::project::marks::Saved {
        id: crate::project::marks::Id {
            ticket: 41,
            session: 17,
            project: before.project_id().clone(),
            revision: before.revision_id().clone(),
        },
        letter: 'a',
        revision: document.revision_id().clone(),
        refresh_error: None,
    };
    (document, plan, saved)
}

fn assert_same_inspector_navigation(before: &State, after: &State) {
    assert_eq!(after.session, before.session);
    assert_eq!(after.scope, before.scope);
    assert_eq!(after.root, before.root);
    assert_eq!(after.selected, before.selected);
    assert_eq!(after.cursor, before.cursor);
    assert_eq!(after.preferred, before.preferred);
    assert_eq!(after.levels.len(), before.levels.len());
    for (after, before) in after.levels.iter().zip(&before.levels) {
        assert_eq!(after.owner, before.owner);
        assert_eq!(after.branch, before.branch);
        assert_eq!(after.selected, before.selected);
    }
    assert_eq!(
        after.projection.presentation,
        before.projection.presentation
    );
    assert_eq!(after.projection.exact, before.projection.exact);
    assert_eq!(after.projection.reason, before.projection.reason);
    assert!(Arc::ptr_eq(&after.rows, &before.rows));
    assert!(Arc::ptr_eq(&after.breadcrumbs, &before.breadcrumbs));
    assert_eq!(after.scope_label, before.scope_label);
    assert_eq!(after.repeat_choice, before.repeat_choice);
    assert_eq!(after.index.parents, before.index.parents);
    assert_eq!(after.index.sequence_offsets, before.index.sequence_offsets);
    assert_eq!(after.index.durations, before.index.durations);
    assert_eq!(after.index.overridden, before.index.overridden);
    assert_eq!(after.index.overridden_runs, before.index.overridden_runs);
}

#[test]
fn exact_mark_transition_preserves_current_nested_navigation_and_repeated_receipt_is_noop() {
    let document = mark_navigation_fixture();
    let mut state = state(document.clone(), "outer", 8);
    let entry_selection = state.selected.clone();
    let (marked, marked_plan, saved) =
        mark_transition(&document, set_navigation_mark(), "mark-saved");

    // The mark's revision was captured before this later inspector navigation.
    state.switch_play_in(2).unwrap();
    state.enter_in().unwrap();
    state.select_in(1).unwrap();
    state.enter_in().unwrap();
    state.switch_play_in(2).unwrap();
    state.preferred = Some(presentation(&state).instance.clone());
    assert_ne!(state.selected, entry_selection);
    let current = state.clone();
    assert!(state.rebase_mark_in(17, &marked, &marked_plan, &saved));
    assert_same_inspector_navigation(&current, &state);
    assert!(state.matches_identity(17, &marked));
    assert!(Arc::ptr_eq(&state.index.document, &marked));
    assert!(Arc::ptr_eq(&state.index.plan, &marked_plan));
    assert_eq!(
        current.index.document.revision_id(),
        document.revision_id(),
        "copy-on-write cannot change a retained pre-receipt snapshot"
    );
    assert_eq!(state.index.document.marks().len(), 1);
    state.selected.validate(&marked).unwrap();

    let rebased_index = state.index.clone();
    assert!(state.rebase_mark_in(17, &marked, &marked_plan, &saved));
    assert!(
        Arc::ptr_eq(&rebased_index, &state.index),
        "repeated valid Saved is a true no-op"
    );
    assert_same_inspector_navigation(&current, &state);

    let (unmarked, unmarked_plan, deleted) = mark_transition(
        &marked,
        Command::DeleteMark {
            id: MarkId::new("native-mark-a").unwrap(),
        },
        "mark-deleted",
    );
    state.leave_in().unwrap();
    let before_delete = state.clone();
    assert!(state.rebase_mark_in(17, &unmarked, &unmarked_plan, &deleted));
    assert_same_inspector_navigation(&before_delete, &state);
    assert!(state.index.document.marks().is_empty());

    let before_sticky = state.clone();
    assert!(!state.rebase_mark_in(17, &unmarked, &unmarked_plan, &saved));
    assert!(!state.rebase_mark_in(17, &marked, &marked_plan, &saved));
    assert!(Arc::ptr_eq(&state.index, &before_sticky.index));
    assert_same_inspector_navigation(&before_sticky, &state);
}

#[test]
fn mark_rebase_rejects_every_identity_mismatch_without_touching_navigation() {
    let document = mark_navigation_fixture();
    let original = state(document.clone(), "outer", 8);
    let (marked, marked_plan, saved) =
        mark_transition(&document, set_navigation_mark(), "mark-saved");
    let mut wrong_old = saved.clone();
    wrong_old.id.revision = revision("wrong-old");
    let mut wrong_new = saved.clone();
    wrong_new.revision = revision("wrong-new");
    let mut wrong_session = saved.clone();
    wrong_session.id.session += 1;
    let mut wrong_project = saved.clone();
    wrong_project.id.project = ProjectId::new("other-project").unwrap();
    for bad in [wrong_old, wrong_new, wrong_session, wrong_project] {
        let mut candidate = original.clone();
        assert!(!candidate.rebase_mark_in(17, &marked, &marked_plan, &bad));
        assert!(Arc::ptr_eq(&candidate.index, &original.index));
        assert_same_inspector_navigation(&original, &candidate);
    }

    let mut candidate = original.clone();
    assert!(!candidate.rebase_mark_in(18, &marked, &marked_plan, &saved));
    assert!(!candidate.rebase_mark_in(17, &original.index.document, &original.index.plan, &saved));
    let mut foreign_json = serde_json::to_value(marked.as_ref()).unwrap();
    foreign_json["project_id"] = serde_json::json!("other-project");
    let foreign = Arc::new(ProjectDocument::from_json(&foreign_json.to_string()).unwrap());
    let foreign_plan = Arc::new(RenderPlan::compile(&foreign).unwrap());
    assert!(!candidate.rebase_mark_in(17, &foreign, &foreign_plan, &saved));
    assert!(Arc::ptr_eq(&candidate.index, &original.index));
    assert_same_inspector_navigation(&original, &candidate);

    let mut foreign_session = original.clone();
    foreign_session.session = 18;
    let retained = foreign_session.clone();
    assert!(!foreign_session.rebase_mark_in(17, &marked, &marked_plan, &saved));
    assert_same_inspector_navigation(&retained, &foreign_session);
    let mut foreign_state = state(foreign.as_ref().clone(), "outer", 8);
    let retained = foreign_state.clone();
    assert!(!foreign_state.rebase_mark_in(17, &marked, &marked_plan, &saved));
    assert_same_inspector_navigation(&retained, &foreign_state);

    let unrelated = edited(
        &document,
        Command::Rename {
            node: node("first"),
            label: "Changed outside mark save".into(),
        },
        "unrelated",
    );
    let mut stale_state = state(unrelated, "outer", 8);
    let retained = stale_state.clone();
    assert!(!stale_state.rebase_mark_in(17, &marked, &marked_plan, &saved));
    assert!(Arc::ptr_eq(&stale_state.index, &retained.index));
    assert_same_inspector_navigation(&retained, &stale_state);
}

#[test]
fn play_steps_walk_all_plays_then_each_play_of_the_nearest_repeat() {
    let document = fixture(
        "outer",
        vec![
            repeat("outer", "group", 3, Some(1)),
            sequence("group", &["first", "inner"]),
            hold("first", 2),
            repeat("inner", "leaf", 2, None),
            hold("leaf", 2),
            hold("owned-gap", 3),
        ],
        BTreeMap::new(),
        [overrides("outer", &[(1, "owned-gap")])].into(),
    );
    let mut state = state(document, "outer", 0);
    let outer = |state: &State, position: Option<u32>| {
        vec![branch(&state.index.document, "outer", position)]
    };
    // Backward from All plays stays put; forward walks 1..3 and clamps.
    assert!(!state.step_play_in(false, 1).unwrap());
    assert!(state.step_play_in(true, 1).unwrap());
    assert_eq!(state.selected.repeats, outer(&state, Some(0)));
    assert!(state.step_play_in(true, 1).unwrap());
    // Play 2 owns its gap branch, reachable with j.
    assert_eq!(state.rows.len(), 2);
    state.step_in(true, 1).unwrap();
    assert_eq!(state.selected.node, node("owned-gap"));
    assert!(state.step_play_in(true, 5).unwrap());
    assert_eq!(state.selected.repeats, outer(&state, Some(2)));
    assert!(!state.step_play_in(true, 1).unwrap());
    // A count steps back several plays; past play 1 is All plays again.
    assert!(state.step_play_in(false, 2).unwrap());
    assert_eq!(state.selected.repeats, outer(&state, Some(0)));
    assert!(state.step_play_in(false, 1).unwrap());
    assert_eq!(state.selected.repeats, outer(&state, None));
    // Inside the nested group, ]r steps the inner Repeat, not the outer one.
    state.step_play_in(true, 2).unwrap();
    state.select_in(0).unwrap();
    assert!(state.enter_in().unwrap());
    state.step_in(true, 1).unwrap();
    assert_eq!(state.selected.node, node("inner"));
    assert!(state.enter_in().unwrap());
    assert!(state.step_play_in(true, 2).unwrap());
    assert_eq!(
        state.selected.repeats,
        vec![
            branch(&state.index.document, "outer", Some(1)),
            branch(&state.index.document, "inner", Some(1))
        ]
    );
    // Backspace keeps the outer play choice.
    assert!(state.leave_in().unwrap());
    assert!(state.leave_in().unwrap());
    assert_eq!(state.selected.repeats, outer(&state, Some(1)));
}

#[test]
fn play_steps_refuse_contents_without_a_repeat() {
    let document = fixture(
        "speed",
        vec![retime("speed", "body", 0, 4, 8), hold("body", 4)],
        BTreeMap::new(),
        BTreeMap::new(),
    );
    let mut state = state(document, "speed", 0);
    assert!(state.step_play_in(true, 1).is_err());
}

#[test]
fn several_plays_browse_the_first_and_address_each_selected_play() {
    let document = fixture(
        "outer",
        vec![
            repeat("outer", "group", 3, None),
            sequence("group", &["first", "inner"]),
            hold("first", 2),
            repeat("inner", "leaf", 2, None),
            hold("leaf", 2),
        ],
        BTreeMap::new(),
        BTreeMap::new(),
    );
    let mut state = state(document.clone(), "outer", 6);
    assert!(state.switch_plays_in(&[2]).is_err());
    assert!(state.switch_plays_in(&[2, 9]).is_err());
    state.switch_plays_in(&[3, 2, 3]).unwrap();
    assert_eq!(state.scope_label, "outer · plays 2, 3/3");
    assert_eq!(
        state.selected.repeats,
        [branch(&document, "outer", Some(1))]
    );
    state.enter_in().unwrap();
    state.select_in(1).unwrap();
    state.enter_in().unwrap();
    // Nested contents keep the outer choice; the further play mirrors it.
    let also = state.also_targets().unwrap();
    assert_eq!(also.len(), 1);
    assert_eq!(also[0].node, state.selected.node);
    assert_eq!(
        also[0].repeats,
        [
            branch(&document, "outer", Some(2)),
            branch(&document, "inner", None)
        ]
    );
    assert_eq!(
        state.scope_label,
        "outer · plays 2, 3/3 › inner · all plays"
    );
    // Any single scope choice ends the multi-play selection.
    state.switch_play_in(1).unwrap();
    assert!(state.also_targets().unwrap().is_empty());
    assert_eq!(state.scope_label, "outer · play 2/3 › inner · play 1/2");
}
