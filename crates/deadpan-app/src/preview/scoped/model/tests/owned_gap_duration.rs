//! Old-code positive witness for native mixed owned-gap Hold duration.
//!
//! Install as preview/scoped/model/tests/owned_gap_duration.rs. Silent
//! Background Holds need no media qualification: there is no Source or asset
//! lookup in this structural/hint witness.

use super::*;
use deadpan_core::{
    Anchor, AnchorIndex, AnchorTarget, BoundaryAnchor, ExactRatio, InsertionBias, InstancePath,
    OccurrenceEdit, RepeatInstance, ScopedNodeTarget, apply_with_result,
    prepare_occurrence_hold_duration,
};

fn occurrence_start(document: &ProjectDocument, instance: &InstancePath) -> ProjectFrame {
    let boundary = AnchorIndex::new(document)
        .unwrap()
        .resolve_target(&AnchorTarget {
            boundary: BoundaryAnchor {
                coordinate: Anchor::Occurrence {
                    instance: instance.clone(),
                    position: ExactRatio::ZERO,
                },
                bias: InsertionBias::Right,
            },
            occurrence: None,
        })
        .unwrap();
    assert_eq!(boundary.exact_frame, ExactRatio::integer(boundary.frame.0));
    boundary.frame
}

fn actual_play(
    document: &ProjectDocument,
    owner: &str,
    position: u32,
) -> deadpan_core::IterationId {
    let NodeKind::Repeat { iterations, .. } = &document.nodes()[&node(owner)].kind else {
        panic!("expected Repeat fixture");
    };
    iterations.at(position).unwrap()
}

#[test]
fn native_mixed_owned_gap_hold_duration_offers_entry_after_public_controls() {
    let document = fixture(
        "gap-outer",
        vec![
            repeat("gap-outer", "gap-inner", 2, None),
            repeat("gap-inner", "gap-body", 2, Some(1)),
            hold("gap-body", 2),
            hold("owned-gap", 3),
        ],
        BTreeMap::new(),
        [overrides("gap-inner", &[(0, "owned-gap")])].into(),
    );
    document.validate().unwrap();
    assert_eq!(document.duration().unwrap().frames(), 14);
    let NodeKind::Repeat { gap, .. } = &document.nodes()[&node("gap-inner")].kind else {
        panic!();
    };
    assert_eq!(gap.as_ref().unwrap().duration, frames(1));
    let first = actual_play(&document, "gap-inner", 0);
    let last = actual_play(&document, "gap-inner", 1);
    assert_ne!(first, last);
    assert_eq!(
        document.gap_overrides()[&node("gap-inner")].get(&first),
        Some(&node("owned-gap"))
    );
    assert!(
        document.gap_overrides()[&node("gap-inner")]
            .get(&last)
            .is_none()
    );

    let mut mixed = state(document.clone(), "gap-outer", 2);
    assert!(mixed.enter_in().unwrap());
    mixed.switch_play_in(1).unwrap();
    mixed.select_in(1).unwrap();
    assert_eq!(mixed.selected.node, node("owned-gap"));
    assert_eq!(
        mixed.selected.repeats,
        vec![
            branch(&document, "gap-outer", None),
            branch(&document, "gap-inner", Some(0)),
        ]
    );
    mixed.selected.validate(&document).unwrap();
    let shown = presentation(&mixed).instance.clone();
    assert!(mixed.selected.matches_instance(&document, &shown).unwrap());
    assert_eq!(shown.node, node("owned-gap"));
    assert_eq!(shown.repeats[1].iteration, first);
    assert_eq!(presentation(&mixed).frames, 2..5);
    assert_eq!(occurrence_start(&document, &shown), ProjectFrame(2));

    // Public all-Play positive control. Only the shared outer body needs
    // isolation: inner Repeat + default body Hold + first owned gap Hold.
    let concrete = InstancePath {
        node: node("owned-gap"),
        repeats: vec![
            RepeatInstance {
                node: node("gap-outer"),
                iteration: actual_play(&document, "gap-outer", 0),
            },
            RepeatInstance {
                node: node("gap-inner"),
                iteration: first.clone(),
            },
        ],
    };
    let all_play_target = ScopedNodeTarget {
        node: concrete.node.clone(),
        repeats: vec![
            branch(&document, "gap-outer", Some(0)),
            branch(&document, "gap-inner", Some(0)),
        ],
    };
    assert!(
        all_play_target
            .matches_instance(&document, &concrete)
            .unwrap()
    );
    let needs = document
        .occurrence_hold_duration_requirements(&concrete, frames(4))
        .unwrap();
    assert_eq!((needs.nodes, needs.marks), (3, 0));
    let request = CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision("owned-gap-one-control"),
        command: Command::EditOccurrence {
            instance: concrete.clone(),
            edit: OccurrenceEdit::SetHoldDuration {
                duration: frames(4),
            },
            identities: OccurrenceIdentities {
                nodes: (0..needs.nodes)
                    .map(|index| node(&format!("owned-gap-one-control-{index}")))
                    .collect(),
                marks: Vec::new(),
            },
        },
    };
    let prepared = prepare_occurrence_hold_duration(&document, &request).unwrap();
    assert_eq!(prepared.transaction.duration_delta, 1);
    assert_eq!(prepared.document.duration().unwrap().frames(), 15);
    assert_eq!(prepared.boundary.as_ref().unwrap().frame, ProjectFrame(2));
    assert_eq!(prepared.presentation.node, prepared.target.node);
    assert_ne!(prepared.target.node, node("owned-gap"));
    assert!(matches!(
        &prepared.document.nodes()[&prepared.target.node].kind,
        NodeKind::Hold { recipe } if recipe.duration == frames(4)
    ));
    // Independent outer play retains the original gap and body definitions.
    assert_eq!(
        prepared.document.nodes()[&node("owned-gap")],
        document.nodes()[&node("owned-gap")]
    );
    assert_eq!(
        prepared.document.nodes()[&node("gap-inner")],
        document.nodes()[&node("gap-inner")]
    );
    let second_outer = InstancePath {
        node: node("owned-gap"),
        repeats: vec![
            RepeatInstance {
                node: node("gap-outer"),
                iteration: actual_play(&document, "gap-outer", 1),
            },
            RepeatInstance {
                node: node("gap-inner"),
                iteration: first.clone(),
            },
        ],
    };
    assert_eq!(
        occurrence_start(&prepared.document, &second_outer),
        ProjectFrame(10)
    );
    assert_eq!(
        prepared
            .transaction
            .inverse
            .apply(&prepared.document)
            .unwrap(),
        document
    );
    assert_eq!(
        ProjectDocument::from_json(&prepared.document.to_json().unwrap()).unwrap(),
        prepared.document
    );

    // Independent raw definition control proves that one owned gap definition
    // can change in both default outer uses without replacing the default gap.
    let raw = CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision("owned-gap-definition-control"),
        command: Command::SetHoldDuration {
            node: node("owned-gap"),
            duration: frames(4),
        },
    };
    let (transaction, after) = apply_with_result(&document, &raw).unwrap();
    assert_eq!(transaction.duration_delta, 2);
    assert_eq!(after.duration().unwrap().frames(), 16);
    assert_eq!(occurrence_start(&after, &shown), ProjectFrame(2));
    let NodeKind::Repeat { gap, .. } = &after.nodes()[&node("gap-inner")].kind else {
        panic!();
    };
    assert_eq!(gap.as_ref().unwrap().duration, frames(1));
    assert_eq!(after.gap_overrides(), document.gap_overrides());
    assert_eq!(transaction.inverse.apply(&after).unwrap(), document);
    assert_eq!(
        ProjectDocument::from_json(&after.to_json().unwrap()).unwrap(),
        after
    );

    // Final-play ownership is valid but must never supply a fabricated project
    // presentation. Its definition remains editable through the raw command.
    let dormant_document = fixture(
        "gap-outer",
        vec![
            repeat("gap-outer", "gap-inner", 2, None),
            repeat("gap-inner", "gap-body", 2, Some(1)),
            hold("gap-body", 2),
            hold("owned-gap", 3),
            hold("final-gap", 5),
        ],
        BTreeMap::new(),
        [overrides(
            "gap-inner",
            &[(0, "owned-gap"), (1, "final-gap")],
        )]
        .into(),
    );
    assert_eq!(dormant_document.duration().unwrap().frames(), 14);
    let dormant = ScopedNodeTarget {
        node: node("final-gap"),
        repeats: vec![
            branch(&dormant_document, "gap-outer", None),
            branch(&dormant_document, "gap-inner", Some(1)),
        ],
    };
    let dormant_instance = InstancePath {
        node: node("final-gap"),
        repeats: vec![
            RepeatInstance {
                node: node("gap-outer"),
                iteration: actual_play(&dormant_document, "gap-outer", 0),
            },
            RepeatInstance {
                node: node("gap-inner"),
                iteration: last,
            },
        ],
    };
    assert!(
        dormant
            .matches_instance(&dormant_document, &dormant_instance)
            .unwrap()
    );
    let mut dormant_state = state(dormant_document.clone(), "gap-outer", 2);
    assert!(dormant_state.enter_in().unwrap());
    dormant_state.switch_play_in(2).unwrap();
    dormant_state.select_in(1).unwrap();
    assert_eq!(dormant_state.selected, dormant);
    assert!(dormant_state.projection.presentation.is_none());
    assert_eq!(
        dormant_state.projection.reason.as_deref(),
        Some(DORMANT_GAP)
    );
    assert_eq!(
        dormant_state.hold_parameter_in().as_deref(),
        Some("hold-duration 5f")
    );
    let mut all_play_dormant = state(dormant_document.clone(), "gap-outer", 2);
    all_play_dormant.switch_play_in(1).unwrap();
    assert!(all_play_dormant.enter_in().unwrap());
    all_play_dormant.switch_play_in(2).unwrap();
    all_play_dormant.select_in(1).unwrap();
    assert_eq!(all_play_dormant.selected.node, node("final-gap"));
    assert_eq!(
        all_play_dormant.projection.reason.as_deref(),
        Some(DORMANT_GAP)
    );
    assert!(!all_play_dormant.hold_duration_available());
    assert!(
        AnchorIndex::new(&dormant_document)
            .unwrap()
            .resolve_target(&AnchorTarget {
                boundary: BoundaryAnchor {
                    coordinate: Anchor::Occurrence {
                        instance: dormant_instance,
                        position: ExactRatio::ZERO,
                    },
                    bias: InsertionBias::Right,
                },
                occurrence: None,
            })
            .is_err()
    );
    let (dormant_transaction, dormant_after) = apply_with_result(
        &dormant_document,
        &CommandRequest {
            project_id: dormant_document.project_id().clone(),
            expected_revision: dormant_document.revision_id().clone(),
            new_revision: revision("owned-gap-dormant-control"),
            command: Command::SetHoldDuration {
                node: node("final-gap"),
                duration: frames(6),
            },
        },
    )
    .unwrap();
    assert_eq!(dormant_transaction.duration_delta, 0);
    assert_eq!(dormant_after.duration().unwrap().frames(), 14);
    assert_eq!(
        dormant_after.audio_bindings(),
        dormant_document.audio_bindings()
    );
    assert_eq!(
        dormant_transaction.inverse.apply(&dormant_after).unwrap(),
        dormant_document
    );

    let mut all_play = state(document.clone(), "gap-outer", 2);
    all_play.switch_play_in(1).unwrap();
    assert!(all_play.enter_in().unwrap());
    all_play.switch_play_in(1).unwrap();
    all_play.select_in(1).unwrap();
    assert_eq!(all_play.selected.node, node("owned-gap"));
    assert_eq!(
        all_play.hold_parameter_in().as_deref(),
        Some("hold-duration 3f")
    );
    assert_eq!(*mixed.index.document, document);
    assert_eq!(mixed.selected.node, node("owned-gap"));

    // Intended OLD failure: only this mixed native availability gate differs.
    assert_eq!(
        mixed.hold_parameter_in().as_deref(),
        Some("hold-duration 3f")
    );
}

#[test]
fn native_outer_play_owned_gap_can_reach_inner_all_play_hold() {
    let document = fixture(
        "outer",
        vec![
            repeat("outer", "default-body", 2, Some(1)),
            hold("default-body", 2),
            sequence("outer-gap", &["inner"]),
            repeat("inner", "inner-body", 2, None),
            hold("inner-body", 2),
        ],
        BTreeMap::new(),
        [overrides("outer", &[(0, "outer-gap")])].into(),
    );
    let mut state = state(document.clone(), "outer", 2);
    state.switch_play_in(1).unwrap();
    assert_eq!(state.rows.len(), 2);
    state.select_in(1).unwrap();
    assert_eq!(state.selected.node, node("outer-gap"));
    assert!(state.enter_in().unwrap());
    assert_eq!(state.selected.node, node("inner"));
    assert!(state.enter_in().unwrap());
    assert_eq!(state.selected.node, node("inner-body"));
    assert_eq!(
        state.selected.repeats,
        vec![
            branch(&document, "outer", Some(0)),
            branch(&document, "inner", None),
        ]
    );
    assert_eq!(
        state.hold_parameter_in().as_deref(),
        Some("hold-duration 2f")
    );
}

#[test]
fn selected_live_and_final_owned_gaps_keep_distinct_play_authority() {
    let document = fixture(
        "outer",
        vec![
            repeat("outer", "inner", 2, None),
            repeat("inner", "body", 3, Some(1)),
            hold("body", 2),
            hold("gap-zero", 3),
            hold("gap-one", 4),
            hold("gap-final", 5),
        ],
        BTreeMap::new(),
        [overrides(
            "inner",
            &[(0, "gap-zero"), (1, "gap-one"), (2, "gap-final")],
        )]
        .into(),
    );

    let mut state = state(document, "outer", 2);
    assert!(state.enter_in().unwrap());
    state.switch_plays_in(&[1, 2]).unwrap();
    state.select_in(1).unwrap();
    assert_eq!(state.selected.node, node("gap-zero"));
    assert_eq!(state.also_targets().unwrap()[0].node, node("gap-one"));
    assert!(state.hold_duration_available());
    assert_eq!(
        state.hold_parameter_in().as_deref(),
        Some("hold-duration 3f")
    );

    // The primary remains live while the selected counterpart is the final,
    // dormant owned gap. Its definition is still an explicit target.
    state.switch_plays_in(&[2, 3]).unwrap();
    state.select_in(1).unwrap();
    assert_eq!(state.selected.node, node("gap-one"));
    assert_eq!(state.also_targets().unwrap()[0].node, node("gap-final"));
    assert!(state.hold_duration_available());

    // Selecting the final gap alone retains its duration command despite the
    // absence of a fabricated project frame.
    state.switch_play_in(3).unwrap();
    state.select_in(1).unwrap();
    assert_eq!(state.selected.node, node("gap-final"));
    assert!(state.projection.presentation.is_none());
    assert_eq!(state.projection.reason.as_deref(), Some(DORMANT_GAP));
    assert!(state.hold_duration_available());
    assert_eq!(
        state.hold_parameter_in().as_deref(),
        Some("hold-duration 5f")
    );
}
