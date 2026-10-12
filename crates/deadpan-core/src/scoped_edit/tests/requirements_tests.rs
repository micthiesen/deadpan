use super::*;

const COLLISION: &str = "scoped-preflight-0-0";

fn edits() -> Vec<ScopedTargetEdit> {
    vec![
        ScopedTargetEdit {
            target: target(Some(1), Some(0)),
            edit: rename("first play"),
        },
        ScopedTargetEdit {
            target: target(Some(1), Some(1)),
            edit: rename("second play"),
        },
        // The last target must see both earlier remappings and the first value.
        ScopedTargetEdit {
            target: target(Some(1), Some(0)),
            edit: rename("first play"),
        },
    ]
}

fn check(document: ProjectDocument, has_mark: bool) {
    document.validate().unwrap();
    let before = document.to_json().unwrap();
    let expected: Vec<_> = [(10, 2, false), (3, 1, false), (0, 0, true)]
        .into_iter()
        .map(|(nodes, marks, unchanged)| ScopedEditRequirements {
            nodes,
            marks: if has_mark { marks } else { 0 },
            unchanged,
        })
        .collect();
    // Real caller-owned identities already work. The allocation query must
    // admit exactly the same edit independently of legal authored names.
    let request = CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision("collision-edit"),
        command: Command::EditScopedMany {
            edits: edits(),
            identities: expected
                .iter()
                .enumerate()
                .map(|(i, needs)| OccurrenceIdentities {
                    nodes: (0..needs.nodes)
                        .map(|n| node(&format!("real-{i}-{n}")))
                        .collect(),
                    marks: (0..needs.marks)
                        .map(|n| mark(&format!("real-{i}-{n}")))
                        .collect(),
                })
                .collect(),
        },
    };
    let (transaction, after) = apply_with_result(&document, &request).unwrap();
    assert_eq!(after.nodes().len(), document.nodes().len() + 13);
    assert_eq!(after.duration().unwrap(), document.duration().unwrap());
    let proof = derive_scoped_isolation(&document, &request, &after).unwrap();
    for (inner, label) in [(0, "first play"), (1, "second play")] {
        let selected = proof.map_forward(&target(Some(1), Some(inner))).unwrap();
        assert_eq!(after.nodes()[&selected.node].label, label);
        for outer in [0, 2] {
            let unaffected = target(Some(outer), Some(inner));
            assert_eq!(proof.map_forward(&unaffected).unwrap(), unaffected);
            assert_eq!(after.nodes()[&unaffected.node].label, "a");
        }
    }
    assert_eq!(transaction.inverse.apply(&after).unwrap(), document);
    assert_eq!(
        document.scoped_many_requirements(&edits()).unwrap(),
        expected
    );
    assert_eq!(
        document.scoped_many_requirements(&edits()).unwrap(),
        expected
    );
    assert_eq!(document.to_json().unwrap(), before);
}

#[test]
fn allocation_query_avoids_live_node_names() {
    let mut document = fixture();
    let NodeKind::Sequence { children } = &mut document.nodes.get_mut(&node("root")).unwrap().kind
    else {
        panic!()
    };
    children.push(node(COLLISION));
    document
        .nodes
        .insert(node(COLLISION), BeatNode::hold("existing", hold(1)));
    check(document, false);
}

#[test]
fn allocation_query_avoids_live_lineage_origins() {
    let mut document = fixture();
    document.audio_lineage.insert(
        node("a"),
        AudioLineageId {
            allocation: revision("historical"),
            origin: node(COLLISION),
        },
    );
    check(document, false);
}

#[test]
fn allocation_query_avoids_retained_timing_node_names() {
    let mut document = fixture();
    // A historical root no longer needs a live node with that identity.
    let root = document.nodes.remove(&node("root")).unwrap();
    document.root = node(COLLISION);
    document.nodes.insert(node(COLLISION), root);
    document.audio_bindings = capture_unbound_audio_bindings(
        &document,
        AudioTimingId {
            allocation: revision("capture"),
            ordinal: 0,
        },
    )
    .unwrap();
    let root = document.nodes.remove(&node(COLLISION)).unwrap();
    document.root = node("root");
    document.nodes.insert(node("root"), root);
    assert!(!document.nodes.contains_key(&node(COLLISION)));
    check(document, false);
}

#[test]
fn allocation_query_avoids_retained_timing_lineage_origins() {
    let mut document = fixture();
    document.audio_lineage.insert(
        node("a"),
        AudioLineageId {
            allocation: revision("historical"),
            origin: node(COLLISION),
        },
    );
    document.audio_bindings = capture_unbound_audio_bindings(
        &document,
        AudioTimingId {
            allocation: revision("capture"),
            ordinal: 0,
        },
    )
    .unwrap();
    document.audio_lineage.clear();
    check(document, false);
}

#[test]
fn allocation_query_avoids_mark_names() {
    let mut document = fixture();
    add_owned_mark(
        &mut document,
        COLLISION,
        Anchor::Local {
            node: node("a"),
            position: ExactRatio::ONE,
        },
        MarkState::Bound,
    );
    check(document, true);
}

#[test]
fn allocation_query_skips_dense_names_at_each_stage() {
    let mut document = fixture();
    for index in 0..2 {
        for ordinal in 0..64 {
            let id = node(&format!("scoped-preflight-{index}-{ordinal}"));
            let NodeKind::Sequence { children } =
                &mut document.nodes.get_mut(&node("root")).unwrap().kind
            else {
                panic!()
            };
            children.push(id.clone());
            document
                .nodes
                .insert(id, BeatNode::hold("existing", hold(1)));
        }
    }
    add_owned_mark(
        &mut document,
        "scoped-preflight-1-64",
        Anchor::Local {
            node: node("a"),
            position: ExactRatio::ONE,
        },
        MarkState::Bound,
    );
    check(document, true);
}

#[test]
fn allocation_query_keeps_large_repeat_orders_compact() {
    let mut document = fixture();
    let NodeKind::Repeat { iterations, .. } =
        &mut document.nodes.get_mut(&node("outer")).unwrap().kind
    else {
        panic!()
    };
    *iterations = IterationOrder::new(revision("outer"), u32::MAX).unwrap();
    check(document, false);
}

fn reject_private_target(target: ScopedNodeTarget) {
    let document = fixture();
    let before = document.to_json().unwrap();
    let expected = target.validate(&document).unwrap_err().code;
    let edits = vec![
        edits().remove(0),
        ScopedTargetEdit {
            target,
            edit: rename("must remain missing"),
        },
    ];
    assert_eq!(
        document.scoped_many_requirements(&edits).unwrap_err().code,
        expected
    );
    assert_eq!(document.to_json().unwrap(), before);
}

#[test]
fn allocation_query_does_not_resolve_private_node_names() {
    // These would name the cloned body and lead after the first target's
    // isolation. Neither is a declared node in the input document.
    for ordinal in 0..2 {
        reject_private_target(ScopedNodeTarget {
            node: node(&format!("scoped-preflight-0-{ordinal}")),
            repeats: vec![RepeatEditStep {
                repeat: node("outer"),
                branch: RepeatEditBranch::Play {
                    iteration: play("outer", 1),
                },
            }],
        });
    }
}

#[test]
fn allocation_query_does_not_resolve_private_repeat_names() {
    let mut selected = target(Some(1), Some(1));
    // The live leaf exists, but this Repeat ancestry is absent. An earlier
    // private clone must not accidentally make that malformed path valid.
    selected.repeats[1].repeat = node("scoped-preflight-0-2");
    reject_private_target(selected);
}
