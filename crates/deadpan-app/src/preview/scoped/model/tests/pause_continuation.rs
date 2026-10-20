use super::*;
use deadpan_core::{
    Anchor, AnchorIndex, AnchorTarget, AudioTimingId, BoundaryAnchor, OccurrencePauseIdentities,
    SplitIdentities, prepare_occurrence_time,
};

fn insert(
    document: &ProjectDocument,
    target: ScopedNodeTarget,
    instance: InstancePath,
    local: i64,
    name: &str,
) -> deadpan_core::PreparedOccurrenceTime {
    let boundary = AnchorIndex::new(document)
        .unwrap()
        .resolve_target(&AnchorTarget {
            boundary: BoundaryAnchor {
                coordinate: Anchor::Occurrence {
                    instance: instance.clone(),
                    position: ExactRatio::integer(local),
                },
                bias: InsertionBias::Right,
            },
            occurrence: None,
        })
        .unwrap();
    assert_eq!(boundary.exact_frame.denominator(), 1);
    let required = document
        .pause_insertion_scoped(boundary.frame, &target, &instance)
        .unwrap();
    prepare_occurrence_time(
        document,
        &CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: revision(name),
            command: Command::InsertOccurrenceTime {
                at: boundary.frame,
                within: Some(instance),
                target: Some(target),
                hold: recipe(1),
                id: node(&format!("{name}-hold")),
                identities: OccurrencePauseIdentities {
                    isolation: OccurrenceIdentities {
                        nodes: (0..required.isolation_nodes)
                            .map(|n| node(&format!("{name}-node-{n}")))
                            .collect(),
                        marks: (0..required.isolation_marks)
                            .map(|n| MarkId::new(format!("{name}-mark-{n}")).unwrap())
                            .collect(),
                    },
                    split: SplitIdentities {
                        nodes: (0..required.split_nodes)
                            .map(|n| node(&format!("{name}-split-{n}")))
                            .collect(),
                    },
                    container: required
                        .container
                        .then(|| node(&format!("{name}-container"))),
                    gap: required.gap.then(|| node(&format!("{name}-gap"))),
                },
                timing: AudioTimingId {
                    allocation: revision(name),
                    ordinal: 0,
                },
                ai: false,
                all_plays: false,
            },
        },
    )
    .unwrap()
}

fn continuation() -> (State, ProjectDocument, Commit) {
    let document = fixture(
        "outer",
        vec![
            repeat("outer", "inner", 3, None),
            repeat("inner", "body", 2, None),
            sequence("body", &["a", "b"]),
            hold("a", 2),
            hold("b", 4),
            sequence("other", &["c", "d"]),
            hold("c", 4),
            hold("d", 2),
        ],
        [overrides("inner", &[(1, "other")])].into(),
        BTreeMap::new(),
    );
    let mut state = state(document.clone(), "outer", 27);
    state.switch_play_in(3).unwrap();
    state.enter_in().unwrap();
    state.switch_plays_in(&[1, 2]).unwrap();
    assert_eq!(state.selected.node, node("body"));
    stage(state, document, 3)
}

fn stage(state: State, document: ProjectDocument, local: i64) -> (State, ProjectDocument, Commit) {
    stage_named(state, document, local, "pause")
}

fn stage_named(
    state: State,
    document: ProjectDocument,
    local: i64,
    name: &str,
) -> (State, ProjectDocument, Commit) {
    let other = state.also_targets().unwrap();
    let primary_instance = presentation(&state).instance.clone();
    let other_instance = InstancePath {
        node: other[0].node.clone(),
        repeats: other[0]
            .repeats
            .iter()
            .map(|step| {
                let RepeatEditBranch::Play { iteration } = &step.branch else {
                    panic!("Play fixture")
                };
                RepeatInstance {
                    node: step.repeat.clone(),
                    iteration: iteration.clone(),
                }
            })
            .collect(),
    };
    let before = Target {
        session: 17,
        navigation: 0,
        project: document.project_id().clone(),
        revision: document.revision_id().clone(),
        scope: SequenceScope::default(),
        root: node("outer"),
        target: state.selected.clone(),
        presentation: Some(primary_instance.clone()),
        cursor: state.cursor,
        also: other.clone(),
    };
    let first = insert(
        &document,
        state.selected.clone(),
        primary_instance,
        local,
        &format!("{name}-first"),
    );
    let second_target = first.map_target(&document, &other[0]).unwrap();
    let second_instance = first.map_instance(&document, &other_instance).unwrap();
    let second = insert(
        &first.document,
        second_target,
        second_instance,
        local,
        &format!("{name}-second"),
    );
    let target = second.map_target(&first.document, &first.target).unwrap();
    let instance = second
        .map_instance(&first.document, &first.presentation)
        .unwrap();
    let commit = Commit {
        before,
        revision: second.document.revision_id().clone(),
        target,
        presentation: Some(instance),
        also: Some(vec![second.target]),
    };
    (state, second.document, commit)
}

#[test]
fn pause_receipt_keeps_each_queried_owners_independent_descent_and_later_edits() {
    for deeper in [false, true] {
        let mut other = repeat(
            "other-inner",
            if deeper { "extra" } else { "short-leaf" },
            2,
            None,
        );
        let NodeKind::Repeat { iterations, .. } = &mut other.1.kind else {
            unreachable!()
        };
        *iterations = IterationOrder::new(revision("inner"), 2).unwrap();
        let mut nodes = vec![
            repeat("outer", "inner", 2, None),
            repeat("inner", "leaf", 2, None),
            hold("leaf", 4),
            other,
            hold("short-leaf", 2),
        ];
        if deeper {
            nodes.push(repeat("extra", "short-leaf", 2, None));
        }
        let document = fixture(
            "outer",
            nodes,
            [overrides("outer", &[(1, "other-inner")])].into(),
            BTreeMap::new(),
        );
        let mut selected = state(document.clone(), "outer", 3);
        selected.switch_plays_in(&[1, 2]).unwrap();
        let (mut selected, after, commit) = stage(selected, document, 3);
        assert_eq!(
            commit.target.repeats[1].branch,
            branch(&after, "inner", Some(0)).branch
        );
        let other = &commit.also.as_ref().unwrap()[0];
        if deeper {
            assert_eq!(other.repeats.len(), 3);
        } else {
            assert_eq!(
                other.repeats[1].branch,
                branch(&after, "other-inner", Some(1)).branch
            );
        }
        assert!(
            selected
                .reconcile_in(index(after.clone()), &commit)
                .unwrap()
        );
        assert_eq!(
            selected.also_targets().unwrap(),
            commit.also.clone().unwrap()
        );
        assert_eq!(
            selected.scope_label,
            "2 captured pauses · shown: outer · play 1/2 › inner · play 1/2"
        );

        let before = Target {
            session: 17,
            navigation: 0,
            project: after.project_id().clone(),
            revision: after.revision_id().clone(),
            scope: SequenceScope::default(),
            root: node("outer"),
            target: selected.selected.clone(),
            presentation: Some(presentation(&selected).instance.clone()),
            cursor: ProjectFrame(3),
            also: selected.also_targets().unwrap(),
        };
        let edits = std::iter::once(before.target.clone())
            .chain(before.also.clone())
            .map(|target| deadpan_core::ScopedTargetEdit {
                target,
                edit: ScopedNodeEdit::Rename {
                    label: "Captured pause".into(),
                },
            })
            .collect::<Vec<_>>();
        assert!(
            after
                .scoped_many_requirements(&edits)
                .unwrap()
                .iter()
                .all(|required| required.nodes == 0 && required.marks == 0)
        );
        let prepared = prepare_scoped_edit(
            &after,
            &CommandRequest {
                project_id: after.project_id().clone(),
                expected_revision: after.revision_id().clone(),
                new_revision: revision("value-edit"),
                command: Command::EditScopedMany {
                    edits,
                    identities: vec![
                        OccurrenceIdentities {
                            nodes: Vec::new(),
                            marks: Vec::new()
                        };
                        2
                    ],
                },
            },
        )
        .unwrap();
        let also = before
            .also
            .iter()
            .map(|target| prepared.map_target(&after, target).unwrap())
            .collect();
        let commit = Commit {
            presentation: Some(
                prepared
                    .map_instance(&after, before.presentation.as_ref().unwrap())
                    .unwrap(),
            ),
            before,
            revision: prepared.document.revision_id().clone(),
            target: prepared.target,
            also: Some(also),
        };
        assert!(
            selected
                .reconcile_in(index(prepared.document.clone()), &commit)
                .unwrap()
        );
        assert_eq!(selected.also_targets().unwrap(), commit.also.unwrap());
        assert!(
            selected
                .scope_label
                .starts_with("2 captured pauses · shown:")
        );
        // A second pause keeps the reducer-proven heterogeneous target set.
        selected.cursor = ProjectFrame(3);
        let (mut selected, after, commit) = stage_named(selected, prepared.document, 0, "again");
        assert!(selected.reconcile_in(index(after), &commit).unwrap());
        assert_eq!(selected.also_targets().unwrap(), commit.also.unwrap());
        assert!(
            selected
                .scope_label
                .starts_with("2 captured pauses · shown:")
        );
    }
}

#[test]
fn pause_receipt_retains_queried_repeat_owner_after_concrete_descent() {
    let document = fixture(
        "outer",
        vec![
            repeat("outer", "inner", 3, None),
            repeat("inner", "leaf", 2, None),
            hold("leaf", 120),
        ],
        BTreeMap::new(),
        BTreeMap::new(),
    );
    let mut state = state(document.clone(), "outer", 3);
    state.switch_plays_in(&[1, 2]).unwrap();
    assert_eq!(state.selected.node, node("inner"));
    assert_eq!(state.selected.repeats.len(), 1);
    let (mut state, after, commit) = stage(state, document, 3);
    assert_eq!(commit.target.repeats.len(), 2);
    assert_eq!(commit.also.as_ref().unwrap()[0].repeats.len(), 2);
    assert!(state.reconcile_in(index(after), &commit).unwrap());
    assert_eq!(state.also_targets().unwrap(), commit.also.unwrap());
    assert_eq!(state.scope_label, "outer · plays 1, 2/3 › inner · play 1/2");
    assert_eq!(presentation(&state).instance, commit.presentation.unwrap());
}

#[test]
fn pause_receipt_keeps_cloned_repeat_and_exact_nonpositional_counterparts() {
    let (mut state, after, commit) = continuation();
    assert_ne!(commit.target.repeats[1].repeat, node("inner"));
    assert_ne!(
        crate::project::scoped::retarget(
            &after,
            &commit.target,
            1,
            match &commit.also.as_ref().unwrap()[0].repeats[1].branch {
                RepeatEditBranch::Play { iteration } => iteration,
                RepeatEditBranch::Default => panic!("Play fixture"),
            }
        )
        .ok(),
        Some(commit.also.as_ref().unwrap()[0].clone()),
        "different child slots cannot reconstruct the inserted Hold"
    );
    assert!(state.reconcile_in(index(after), &commit).unwrap());
    assert_eq!(state.selected, commit.target);
    assert_eq!(state.also_targets().unwrap(), commit.also.clone().unwrap());
    assert_eq!(state.scope_label, "outer · play 3/3 › inner · plays 1, 2/2");
    assert_eq!(state.repeat_choice.as_ref().unwrap().label, "plays 1, 2/2");
    assert_eq!(
        presentation(&state).instance,
        commit.presentation.clone().unwrap()
    );
    // Re-selecting even this same row revokes direct receipt continuation.
    state.select_in(state.selected()).unwrap();
    assert!(state.also_targets().unwrap().is_empty());
    assert_eq!(state.repeat_choice.as_ref().unwrap().label, "play 1/2");
}

#[test]
fn pause_receipt_rejects_changed_captured_set_or_incomplete_mapped_set() {
    let (state, after, commit) = continuation();
    for corrupt in [0, 1, 2] {
        let mut wrong = commit.clone();
        match corrupt {
            0 => wrong.before.also.clear(),
            1 => wrong.also.as_mut().unwrap().clear(),
            _ => wrong.also = Some(vec![wrong.target.clone()]),
        }
        let mut actual = state.clone();
        assert!(!actual.reconcile_in(index(after.clone()), &wrong).unwrap());
        assert_eq!(
            actual.index.document.revision_id(),
            state.index.document.revision_id()
        );
        assert_eq!(actual.selected, state.selected);
    }
}

#[test]
fn pause_receipt_navigation_drops_counterparts_and_cannot_restore_them_by_returning() {
    for navigation in [0, 1, 2] {
        let (mut state, after, commit) = continuation();
        assert!(state.reconcile_in(index(after), &commit).unwrap());
        match navigation {
            0 => {
                state.step_in(true, 1).unwrap();
                state.step_in(false, 1).unwrap();
            }
            1 => {
                assert!(state.leave_in().unwrap());
                assert!(state.enter_in().unwrap());
            }
            _ => state.switch_play_in(2).unwrap(),
        }
        assert!(state.mapped_also.is_none());
        assert!(state.also_targets().unwrap().is_empty());
    }
}
