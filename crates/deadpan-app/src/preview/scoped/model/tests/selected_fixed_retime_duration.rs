//! Public many-target isolation and raw duration controls for fixed windows.
//! Silent structural evidence only; this does not qualify audible phase.

use super::*;
use deadpan_core::{LeafEdit, ResolvedStep, ResolvedTransaction, ScopedTargetEdit};

fn select_both_inner_plays(document: ProjectDocument) -> State {
    let mut selected = state(document, "outer", 16);
    selected.switch_play_in(2).unwrap();
    assert!(selected.enter_in().unwrap()); // Outer second Play -> Inner
    selected.switch_plays_in(&[1, 2]).unwrap();
    assert!(selected.enter_in().unwrap()); // selected Inner Plays -> fixed Stage
    assert!(selected.enter_in().unwrap()); // fixed Stage -> Body Sequence
    selected.select_in(1).unwrap(); // Held in both selected Inner Plays
    selected
}

fn sampled_identity(
    plan: &deadpan_plan::RenderPlan,
    instance: &InstancePath,
    start: ExactRatio,
    end: ExactRatio,
) {
    let half = ExactRatio::new(1, 2).unwrap();
    let first = start.checked_sub(half).unwrap().ceil().unwrap();
    let terminal = end.checked_sub(half).unwrap().ceil().unwrap();
    assert!(first < terminal);
    for frame in [first, terminal.checked_sub(1).unwrap()] {
        assert!(
            plan.picture(ProjectFrame(i64::try_from(frame).unwrap()))
                .unwrap()
                .framing
                .iter()
                .any(|layer| &layer.instance == instance),
            "Hold identity at frame {frame}"
        );
    }
}

#[test]
fn native_selected_fixed_retime_hold_duration_offers_entry_after_public_many_control() {
    let before = nested_fixed_retime_fixture();
    before.validate().unwrap();
    assert_eq!(before.duration().unwrap().frames(), 42);
    assert!(before.overrides().is_empty());
    assert!(before.gap_overrides().is_empty());
    assert!(before.sounds().is_empty() && before.beat_sounds().is_empty());

    let before_selected = select_both_inner_plays(before.clone());
    let primary = before_selected.selected.clone();
    let also = before_selected.also_targets().unwrap();
    assert_eq!(also.len(), 1);
    let targets = [primary.clone(), also[0].clone()];
    assert_eq!(targets[0].node, node("held"));
    assert_eq!(targets[1].node, node("held"));
    assert_ne!(targets[0], targets[1]);
    for (inner, target) in targets.iter().enumerate() {
        assert_eq!(target.repeats.len(), 2);
        assert_eq!(target.repeats[0], branch(&before, "outer", Some(1)));
        assert_eq!(
            target.repeats[1],
            branch(&before, "inner", Some(u32::try_from(inner).unwrap()))
        );
        target.validate(&before).unwrap();
    }
    let held = [
        nested_instance(&before, "held", 1, 0),
        nested_instance(&before, "held", 1, 1),
    ];
    for (target, instance) in targets.iter().zip(&held) {
        assert!(target.matches_instance(&before, instance).unwrap());
    }
    assert_eq!(before_selected.cursor, ProjectFrame(16));
    assert_eq!(presentation(&before_selected).instance, held[0]);
    assert_eq!(presentation(&before_selected).frames, 16..18);
    assert_eq!(
        presentation(&before_selected).exact,
        ExactRatio::new(33, 2).unwrap()..ExactRatio::integer(18)
    );

    // The public many-target Rename stages real isolation in request order.
    // Both targets address the same shared Hold initially, but different Plays.
    let edits = targets
        .iter()
        .enumerate()
        .map(|(index, target)| ScopedTargetEdit {
            target: target.clone(),
            edit: ScopedNodeEdit::Rename {
                label: format!("Selected fixed window isolation control {index}"),
            },
        })
        .collect::<Vec<_>>();
    let requirements = before.scoped_many_requirements(&edits).unwrap();
    assert_eq!(requirements.len(), 2);
    assert_eq!(
        requirements
            .iter()
            .map(|need| (need.nodes, need.marks, need.unchanged))
            .collect::<Vec<_>>(),
        vec![(11, 0, false), (5, 0, false)]
    );
    let identities = requirements
        .iter()
        .enumerate()
        .map(|(member, need)| OccurrenceIdentities {
            nodes: (0..need.nodes)
                .map(|index| node(&format!("selected-fixed-control-{member}-{index}")))
                .collect(),
            marks: Vec::new(),
        })
        .collect();
    let isolation_request = CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: revision("selected-fixed-isolation-control"),
        command: Command::EditScopedMany { edits, identities },
    };
    let prepared = prepare_scoped_edit(&before, &isolation_request).unwrap();
    assert_eq!(
        prepared.transaction,
        apply(&before, &isolation_request).unwrap()
    );
    assert_eq!(prepared.transaction.duration_delta, 0);
    assert_eq!(prepared.isolation().steps().len(), 3);
    let isolated = &prepared.document;
    assert_eq!(isolated.nodes().len(), before.nodes().len() + 16);
    assert_eq!(isolated.duration().unwrap().frames(), 42);
    let mapped_targets = targets
        .iter()
        .map(|target| prepared.map_target(&before, target).unwrap())
        .collect::<Vec<_>>();
    let mapped_held = held
        .iter()
        .map(|instance| prepared.map_instance(&before, instance).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(prepared.target, mapped_targets[0]);
    assert_ne!(mapped_targets[0].node, mapped_targets[1].node);
    for (target, instance) in mapped_targets.iter().zip(&mapped_held) {
        assert!(target.matches_instance(isolated, instance).unwrap());
        assert_eq!(target.node, instance.node);
        assert_ne!(instance.node, node("held"));
    }

    // Apply ordinary raw duration reducers only after executed proof mapping.
    // This control does not invoke the selected duration policy under test.
    let resize_request = CommandRequest {
        project_id: isolated.project_id().clone(),
        expected_revision: isolated.revision_id().clone(),
        new_revision: revision("selected-fixed-duration-control"),
        command: Command::Compound {
            transaction: ResolvedTransaction::new(
                0,
                BTreeMap::new(),
                mapped_targets
                    .iter()
                    .enumerate()
                    .map(|(index, target)| ResolvedStep::Edit {
                        edit: LeafEdit::new(
                            revision(&format!("selected-fixed-duration-member-{index}")),
                            Command::SetHoldDuration {
                                node: target.node.clone(),
                                duration: frames(4),
                            },
                        )
                        .unwrap(),
                    })
                    .collect(),
            )
            .unwrap(),
        },
    };
    let resized = apply(isolated, &resize_request).unwrap();
    assert_eq!(resized.duration_delta, 0);
    let after = resized.forward.apply(isolated).unwrap();
    after.validate().unwrap();
    assert_eq!(after.duration().unwrap().frames(), 42);
    assert_eq!(after.nodes().len(), isolated.nodes().len());
    assert_eq!(after.overrides(), isolated.overrides());
    assert_eq!(after.gap_overrides(), isolated.gap_overrides());
    for target in &mapped_targets {
        assert!(matches!(&after.nodes()[&target.node].kind,
            NodeKind::Hold { recipe } if recipe.duration == frames(4)));
    }

    let before_plan = deadpan_plan::RenderPlan::compile(&before).unwrap();
    let after_plan = deadpan_plan::RenderPlan::compile(&after).unwrap();
    for (inner, (old_start, old_end, new_end, sampled)) in [
        (
            ExactRatio::new(33, 2).unwrap(),
            ExactRatio::integer(18),
            ExactRatio::new(39, 2).unwrap(),
            16..19,
        ),
        (
            ExactRatio::new(45, 2).unwrap(),
            ExactRatio::integer(24),
            ExactRatio::new(51, 2).unwrap(),
            22..25,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let inner_play = u32::try_from(inner).unwrap();
        let original_stage = nested_instance(&before, "fixed-stage", 1, inner_play);
        let stage_instance = prepared.map_instance(&before, &original_stage).unwrap();
        let speech_instance = prepared
            .map_instance(&before, &nested_instance(&before, "speech", 1, inner_play))
            .unwrap();
        let mut body_target = targets[inner].clone();
        body_target.node = node("fixed-body");
        let mapped_body = prepared.map_target(&before, &body_target).unwrap().node;
        assert_eq!(exact_nested_boundary(&before, &held[inner], 0), old_start);
        assert_eq!(exact_nested_boundary(&before, &held[inner], 2), old_end);
        assert_eq!(
            exact_nested_boundary(&after, &mapped_held[inner], 0),
            old_start
        );
        assert_eq!(
            exact_nested_boundary(&after, &mapped_held[inner], 4),
            new_end
        );
        assert_eq!(exact_nested_boundary(&after, &speech_instance, 0), new_end);
        assert_eq!(
            exact_nested_boundary(&after, &stage_instance, 0),
            ExactRatio::integer(if inner == 0 { 15 } else { 21 })
        );
        let mut expected_stage = before.nodes()[&node("fixed-stage")].clone();
        let NodeKind::Retime {
            child,
            duration,
            mapping,
            pitch,
            purpose,
        } = &mut expected_stage.kind
        else {
            panic!("fixed Edit Retime");
        };
        assert_eq!(*duration, frames(6));
        assert_eq!(
            *mapping,
            FrameRange::new(ProjectFrame(0), ProjectFrame(8)).unwrap()
        );
        assert_eq!(*pitch, PitchPolicy::FollowSpeed);
        assert_eq!(*purpose, RetimePurpose::Edit);
        *child = mapped_body;
        assert_eq!(after.nodes()[&stage_instance.node], expected_stage);
        sampled_identity(&before_plan, &held[inner], old_start, old_end);
        sampled_identity(&after_plan, &mapped_held[inner], old_start, new_end);
        let half = ExactRatio::new(1, 2).unwrap();
        assert_eq!(
            u64::try_from(old_start.checked_sub(half).unwrap().ceil().unwrap()).unwrap()
                ..u64::try_from(new_end.checked_sub(half).unwrap().ceil().unwrap()).unwrap(),
            sampled
        );
    }

    // All four other outer/inner occurrences keep the complete Hold recipe,
    // exact boundaries, and actual sampled picture identity.
    let mut unchanged_uses = 0;
    for outer in [0, 2] {
        for inner in 0..2 {
            unchanged_uses += 1;
            let original = nested_instance(&before, "held", outer, inner);
            let mapped = prepared.map_instance(&before, &original).unwrap();
            assert_eq!(after.nodes()[&mapped.node], before.nodes()[&original.node]);
            let start = exact_nested_boundary(&before, &original, 0);
            let end = exact_nested_boundary(&before, &original, 2);
            assert_eq!(exact_nested_boundary(&after, &mapped, 0), start);
            assert_eq!(exact_nested_boundary(&after, &mapped, 2), end);
            sampled_identity(&before_plan, &original, start, end);
            sampled_identity(&after_plan, &mapped, start, end);
        }
    }
    assert_eq!(unchanged_uses, 4);
    // Every picture outside the selected outer family remains identical,
    // including the complete lead/tail and both other outer families.
    for frame in (0..15).chain(27..42) {
        let mut expected = before_plan.picture(ProjectFrame(frame)).unwrap();
        let actual = after_plan.picture(ProjectFrame(frame)).unwrap();
        // Revisions and diagnostic traversal work are not picture semantics.
        expected.revision_id = actual.revision_id.clone();
        expected.lookup = actual.lookup;
        assert_eq!(expected, actual);
    }
    for (id, beat) in before.nodes() {
        assert_eq!(&after.nodes()[id], beat);
    }
    let isolated_selected = select_both_inner_plays(isolated.clone());
    assert_eq!(isolated_selected.selected, mapped_targets[0]);
    assert_eq!(
        isolated_selected.also_targets().unwrap(),
        mapped_targets[1..]
    );
    assert_eq!(presentation(&isolated_selected).frames, 16..18);
    let after_selected = select_both_inner_plays(after.clone());
    assert_eq!(after_selected.selected, mapped_targets[0]);
    assert_eq!(after_selected.also_targets().unwrap(), mapped_targets[1..]);
    assert_eq!(after_selected.cursor, ProjectFrame(16));
    assert_eq!(presentation(&after_selected).instance, mapped_held[0]);
    assert_eq!(presentation(&after_selected).frames, 16..19);
    assert_eq!(
        presentation(&after_selected).exact,
        ExactRatio::new(33, 2).unwrap()..ExactRatio::new(39, 2).unwrap()
    );

    assert_eq!(resized.inverse.apply(&after).unwrap(), *isolated);
    assert_eq!(
        prepared.transaction.inverse.apply(isolated).unwrap(),
        before
    );
    for document in [&before, isolated, &after] {
        assert_eq!(
            ProjectDocument::from_json(&document.to_json().unwrap()).unwrap(),
            *document
        );
    }
    assert_eq!(*before_selected.index.document, before);
    assert_eq!(before_selected.selected, primary);
    assert_eq!(before_selected.also_targets().unwrap(), also);
    assert_eq!(presentation(&before_selected).instance, held[0]);

    // Sole intended old-code failure, after every independent public control.
    assert!(
        before_selected.hold_duration_available(),
        "Both selected Inner Plays in Outer second Play must offer Change duration inside their fixed Edit Retime windows"
    );
}

fn select_both_outer_plays(document: ProjectDocument, inner_play: bool) -> State {
    let mut selected = state(document, "outer", 22);
    selected.switch_plays_in(&[1, 2]).unwrap();
    assert!(selected.enter_in().unwrap()); // selected Outer Plays -> Inner
    if inner_play {
        selected.switch_play_in(2).unwrap();
    } else {
        // An explicit redundant All plays choice must preserve the outer set.
        selected.switch_in(RepeatEditBranch::Default).unwrap();
    }
    assert!(selected.enter_in().unwrap()); // Inner choice -> fixed Stage
    assert!(selected.enter_in().unwrap()); // fixed Stage -> Body
    selected.select_in(1).unwrap();
    selected
}

#[test]
fn native_selected_fixed_retime_outer_axis_survives_inner_scalar_scope_choices() {
    let document = nested_fixed_retime_fixture();
    for inner_play in [false, true] {
        let selected = select_both_outer_plays(document.clone(), inner_play);
        let also = selected.also_targets().unwrap();
        assert_eq!(also.len(), 1);
        for (ordinal, target) in [&selected.selected, &also[0]].into_iter().enumerate() {
            assert_eq!(target.node, node("held"));
            assert_eq!(target.repeats.len(), 2);
            assert_eq!(
                target.repeats[0],
                branch(&document, "outer", Some(u32::try_from(ordinal).unwrap()))
            );
            assert_eq!(
                target.repeats[1],
                branch(&document, "inner", inner_play.then_some(1))
            );
            target.validate(&document).unwrap();
        }
        assert_eq!(
            selected.edit_scope_hint(),
            "Changes apply to the selected plays."
        );
        assert!(selected.hold_duration_available());
        let inner = u32::from(inner_play);
        assert_eq!(
            presentation(&selected).instance,
            nested_instance(&document, "held", 0, inner)
        );
        assert_eq!(
            presentation(&selected).frames,
            if inner_play { 10..12 } else { 4..6 }
        );
        // The model retains its entry cursor; the native caller follows the
        // independently projected presentation checked above.
        assert_eq!(selected.cursor, ProjectFrame(22));
        assert_eq!(*selected.index.document, document);
        assert_eq!(
            selected
                .levels
                .iter()
                .filter(|level| !level.also.is_empty())
                .count(),
            1
        );
        assert_eq!(
            selected
                .levels
                .iter()
                .find(|level| !level.also.is_empty())
                .unwrap()
                .owner
                .node,
            node("outer")
        );
    }
}

#[test]
fn native_selected_fixed_retime_new_inner_axis_replaces_outer_selection() {
    let document = nested_fixed_retime_fixture();
    let mut selected = state(document.clone(), "outer", 22);
    selected.switch_plays_in(&[1, 2]).unwrap();
    assert!(selected.enter_in().unwrap());
    assert_eq!(selected.also_targets().unwrap().len(), 1);
    selected.switch_plays_in(&[1, 2]).unwrap();
    assert!(selected.enter_in().unwrap());
    assert!(selected.enter_in().unwrap());
    selected.select_in(1).unwrap();
    let also = selected.also_targets().unwrap();
    assert_eq!(
        also.len(),
        1,
        "the new axis has two members, not a Cartesian product"
    );
    for (ordinal, target) in [&selected.selected, &also[0]].into_iter().enumerate() {
        assert_eq!(target.repeats[0], branch(&document, "outer", Some(0)));
        assert_eq!(
            target.repeats[1],
            branch(&document, "inner", Some(u32::try_from(ordinal).unwrap()))
        );
    }
    assert_eq!(
        selected
            .levels
            .iter()
            .filter(|level| !level.also.is_empty())
            .count(),
        1
    );
    assert!(
        selected
            .levels
            .iter()
            .find(|level| level.owner.node == node("outer"))
            .unwrap()
            .also
            .is_empty()
    );
    assert_eq!(
        selected
            .levels
            .iter()
            .find(|level| !level.also.is_empty())
            .unwrap()
            .owner
            .node,
        node("inner")
    );
    assert_eq!(
        selected.edit_scope_hint(),
        "Changes apply to the selected plays."
    );
    assert!(selected.hold_duration_available());
    assert_eq!(
        presentation(&selected).instance,
        nested_instance(&document, "held", 0, 0)
    );
    assert_eq!(*selected.index.document, document);
}

#[test]
fn native_selected_fixed_retime_scalar_scope_revokes_exact_mapped_receipt() {
    let document = nested_fixed_retime_fixture();
    let mut selected = select_both_outer_plays(document.clone(), true);
    let also = selected.also_targets().unwrap();
    let shown = presentation(&selected).instance.clone();
    let before = Target {
        session: selected.session,
        navigation: 0,
        project: document.project_id().clone(),
        revision: document.revision_id().clone(),
        scope: selected.scope.clone(),
        root: selected.root.clone(),
        target: selected.selected.clone(),
        presentation: Some(shown.clone()),
        cursor: selected.cursor,
        also: also.clone(),
    };
    let edits = [&selected.selected, &also[0]]
        .into_iter()
        .enumerate()
        .map(|(member, target)| ScopedTargetEdit {
            target: target.clone(),
            edit: ScopedNodeEdit::Rename {
                label: format!("Outer selected fixed receipt {member}"),
            },
        })
        .collect::<Vec<_>>();
    let identities = document
        .scoped_many_requirements(&edits)
        .unwrap()
        .iter()
        .enumerate()
        .map(|(member, need)| {
            assert_eq!(need.marks, 0);
            OccurrenceIdentities {
                nodes: (0..need.nodes)
                    .map(|index| node(&format!("outer-selected-fixed-receipt-{member}-{index}")))
                    .collect(),
                marks: Vec::new(),
            }
        })
        .collect();
    let request = CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision("outer-selected-fixed-receipt"),
        command: Command::EditScopedMany { edits, identities },
    };
    let prepared = prepare_scoped_edit(&document, &request).unwrap();
    let commit = Commit {
        before,
        revision: prepared.document.revision_id().clone(),
        target: prepared.map_target(&document, &selected.selected).unwrap(),
        presentation: Some(prepared.map_instance(&document, &shown).unwrap()),
        also: Some(vec![prepared.map_target(&document, &also[0]).unwrap()]),
    };
    assert!(
        selected
            .reconcile_in(index(prepared.document.clone()), &commit)
            .unwrap()
    );
    assert_eq!(selected.selected, commit.target);
    assert_eq!(
        selected.also_targets().unwrap(),
        *commit.also.as_ref().unwrap()
    );
    assert!(selected.mapped_also.is_some());
    assert!(selected.hold_duration_available());
    selected.switch_in(RepeatEditBranch::Default).unwrap();
    assert!(selected.mapped_also.is_none());
    assert!(selected.also_targets().unwrap().is_empty());
    assert!(selected.levels.iter().all(|level| level.also.is_empty()));
    assert_ne!(
        selected.edit_scope_hint(),
        "Changes apply to the selected plays."
    );
    assert_eq!(
        *selected.index.document, prepared.document,
        "scope changes do not edit authored state"
    );
}

#[test]
fn native_selected_fixed_retime_heterogeneous_counterpart_refusal_preserves_navigation() {
    let inner = repeat("inner", "fixed-stage", 2, None);
    let mut other = repeat("other-inner", "other-stage", 2, None);
    let NodeKind::Repeat {
        iterations: source_order,
        ..
    } = &inner.1.kind
    else {
        unreachable!();
    };
    let NodeKind::Repeat {
        iterations: other_order,
        ..
    } = &mut other.1.kind
    else {
        unreachable!();
    };
    // Clone-stable Play IDs ensure the failure comes from the owned contents,
    // rather than an unrelated nested Repeat allocation.
    *other_order = source_order.clone();
    let other_override = PlayOverrides::try_from(vec![PlayOverride {
        iteration: source_order.at(1).unwrap(),
        root: node("other-leaf"),
    }])
    .unwrap();
    let document = fixture(
        "outer",
        vec![
            repeat("outer", "inner", 3, None),
            inner,
            retime("fixed-stage", "fixed-body", 0, 8, 6),
            sequence("fixed-body", &["before", "held", "speech"]),
            hold("before", 2),
            hold("held", 2),
            hold("speech", 6),
            other,
            retime("other-stage", "other-body", 0, 8, 6),
            sequence(
                "other-body",
                &["other-before", "other-held", "other-speech"],
            ),
            hold("other-before", 2),
            hold("other-held", 2),
            hold("other-speech", 6),
            hold("other-leaf", 6),
        ],
        [
            overrides("outer", &[(1, "other-inner")]),
            (node("other-inner"), other_override),
        ]
        .into(),
        BTreeMap::new(),
    );
    document.validate().unwrap();
    let mut selected = state(document.clone(), "outer", 0);
    selected.switch_plays_in(&[1, 2]).unwrap();
    assert!(selected.enter_in().unwrap());
    assert_eq!(selected.selected.node, node("fixed-stage"));
    let old_also = selected.also_targets().unwrap();
    assert_eq!(old_also.len(), 1);
    assert_eq!(old_also[0].node, node("other-stage"));
    selected.selected.validate(&document).unwrap();
    old_also[0].validate(&document).unwrap();

    let before = selected.clone();
    let error = selected.switch_play_in(2).unwrap_err();
    assert!(!error.is_empty());
    let levels = |value: &State| {
        value
            .levels
            .iter()
            .map(|level| {
                (
                    level.owner.clone(),
                    level.branch.clone(),
                    level.selected,
                    level.also.clone(),
                )
            })
            .collect::<Vec<_>>()
    };
    let rows = |value: &State| {
        value
            .rows
            .iter()
            .map(|row| {
                (
                    row.target.clone(),
                    row.label.clone(),
                    row.kind,
                    row.selected,
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(selected.session, before.session);
    assert_eq!(selected.scope, before.scope);
    assert_eq!(selected.root, before.root);
    assert!(Arc::ptr_eq(&selected.index, &before.index));
    assert_eq!(levels(&selected), levels(&before));
    assert_eq!(selected.selected, before.selected);
    assert_eq!(selected.mapped_also, before.mapped_also);
    assert_eq!(selected.cursor, before.cursor);
    assert_eq!(selected.preferred, before.preferred);
    assert_eq!(rows(&selected), rows(&before));
    assert_eq!(
        selected.projection.presentation,
        before.projection.presentation
    );
    assert_eq!(selected.projection.exact, before.projection.exact);
    assert_eq!(selected.projection.reason, before.projection.reason);
    assert_eq!(selected.breadcrumbs, before.breadcrumbs);
    assert_eq!(selected.scope_label, before.scope_label);
    assert_eq!(selected.repeat_choice, before.repeat_choice);
    assert_eq!(selected.also_targets().unwrap(), old_also);
    selected.selected.validate(&document).unwrap();
    old_also[0].validate(&document).unwrap();

    // Replacing the axis clears the incompatible outer set before validating
    // the newly selected inner Plays, which remain compatible in outer Play 1.
    selected.switch_plays_in(&[1, 2]).unwrap();
    let inner_also = selected.also_targets().unwrap();
    assert_eq!(inner_also.len(), 1);
    assert_eq!(
        selected.selected.repeats[0],
        branch(&document, "outer", Some(0))
    );
    assert_eq!(
        inner_also[0].repeats[0],
        branch(&document, "outer", Some(0))
    );
    assert_eq!(
        selected.selected.repeats[1],
        branch(&document, "inner", Some(0))
    );
    assert_eq!(
        inner_also[0].repeats[1],
        branch(&document, "inner", Some(1))
    );
    assert_eq!(
        selected
            .levels
            .iter()
            .filter(|level| !level.also.is_empty())
            .count(),
        1
    );
    assert_eq!(*selected.index.document, document);
}
