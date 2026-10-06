use super::*;

fn repeated() -> ProjectDocument {
    tree(
        &["lead", "repeat"],
        vec![
            ("lead", hold(2)),
            ("held", hold(3)),
            (
                "repeat",
                BeatNode {
                    kind: NodeKind::Repeat {
                        child: node("held"),
                        iterations: crate::IterationOrder::new(revision("plays"), 3).unwrap(),
                        gap: None,
                        escalation: None,
                    },
                    ..BeatNode::sequence("Repeat", vec![])
                },
            ),
        ],
    )
}
fn selected(child: &str, cursor: i64) -> SemanticContext {
    SemanticContext {
        selected_child: Some(node(child)),
        ..context("root", cursor)
    }
}
fn replayed(document: &ProjectDocument, plan: &SemanticPlan) -> ProjectDocument {
    let replay =
        crate::replay_compound::<EditError>(document, plan.request.as_ref().unwrap(), |_| Ok(()))
            .unwrap();
    assert_eq!(replay.document, plan.document);
    assert_eq!(
        replay.edit.inverse.apply(&replay.document).unwrap(),
        *document
    );
    replay.document
}

#[test]
fn explode_instruction_keeps_selection_time_and_replays_its_exact_command() {
    let document = repeated();
    let plan = plan(
        &document,
        selected("repeat", 2),
        vec![SemanticInstruction::Explode],
        &BTreeMap::new(),
    )
    .unwrap();
    let result = replayed(&document, &plan);
    assert_eq!(result.duration().unwrap().frames(), 11);
    assert_eq!(plan.context.selected_child, Some(node("repeat")));
    assert_eq!(plan.context.cursor, ProjectFrame(2));
    assert!(matches!(
        &result.nodes()[&node("repeat")].kind,
        NodeKind::Sequence { children } if children.len() == 3
    ));
    // A second explode at the same target refuses: it is no longer a Repeat.
    let mut result = result;
    result.revision_id = revision("exploded");
    let error = super::plan(
        &result,
        selected("repeat", 2),
        vec![SemanticInstruction::Explode],
        &BTreeMap::new(),
    )
    .unwrap_err();
    assert_eq!(error.code, EditErrorCode::WrongNodeKind, "{error}");
    // Without a selected beat, and with a Visual range, it refuses too.
    for context in [
        context("root", 2),
        SemanticContext {
            visual_selection: Some(SemanticVisualSelection::Time {
                anchor: ProjectFrame(2),
                head: ProjectFrame(4),
                extending: false,
            }),
            ..selected("repeat", 2)
        },
    ] {
        assert!(
            super::plan(
                &document,
                context,
                vec![SemanticInstruction::Explode],
                &BTreeMap::new()
            )
            .is_err()
        );
    }
}

#[test]
fn duplicate_instruction_copies_selected_beat_and_visual_range_then_selects_the_copy() {
    let document = tree(&["a", "b"], vec![("a", hold(4)), ("b", hold(3))]);
    let plan = plan(
        &document,
        selected("a", 0),
        vec![SemanticInstruction::Duplicate {
            selector: SemanticSelector::SelectedBeat,
        }],
        &BTreeMap::new(),
    )
    .unwrap();
    let result = replayed(&document, &plan);
    assert_eq!(result.duration().unwrap().frames(), 11);
    assert_eq!(plan.context.selected_child, Some(node("paste-0-0")));
    assert_eq!(plan.context.cursor, ProjectFrame(4));
    assert!(
        plan.register_writes.is_empty(),
        "duplicate leaves registers"
    );
    // A Visual range ending inside b splits it and inserts the copy there.
    let plan = super::plan(
        &document,
        SemanticContext {
            visual_selection: Some(SemanticVisualSelection::Time {
                anchor: ProjectFrame(2),
                head: ProjectFrame(5),
                extending: false,
            }),
            ..context("root", 5)
        },
        vec![SemanticInstruction::Duplicate {
            selector: SemanticSelector::VisualSelection,
        }],
        &BTreeMap::new(),
    )
    .unwrap();
    let result = replayed(&document, &plan);
    assert_eq!(result.duration().unwrap().frames(), 10);
    assert_eq!(plan.context.cursor, ProjectFrame(5));
    assert!(plan.context.visual_selection.is_none());
}
