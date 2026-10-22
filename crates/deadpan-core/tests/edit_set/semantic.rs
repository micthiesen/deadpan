use super::*;
use std::{num::NonZeroU32, sync::Arc};

fn entry() -> SemanticContext {
    SemanticContext {
        original: None,
        scoped: None,
        hold_duration: None,
        shared_hold_duration: None,
        mixed_hold_duration: None,
        selected_hold_duration: None,
        parent: node("root"),
        cursor: ProjectFrame(6),
        selected_child: Some(node("gap")),
        visual_selection: Some(SemanticVisualSelection::Time {
            anchor: ProjectFrame(5),
            head: ProjectFrame(7),
            extending: false,
        }),
    }
}

fn nodes() -> SemanticSelector {
    SemanticSelector::Nodes {
        nodes: vec![node("right-fragment"), node("left-fragment")],
    }
}

fn run(
    document: &ProjectDocument,
    context: &SemanticContext,
    instructions: Vec<SemanticInstruction>,
) -> (
    Result<SemanticPlan, EditError>,
    Vec<SemanticAllocationRequest>,
) {
    let bank = BTreeMap::<RegisterName, Arc<RegisterValue>>::new();
    let program = SemanticProgram::new(instructions).unwrap();
    let mut allocations = Vec::new();
    let result = plan_semantic(
        document,
        context,
        &program,
        SemanticRegisterBank {
            entries: &bank,
            version: 29,
        },
        revision("semantic-union-final"),
        |request| {
            allocations.push(request.clone());
            match request {
                SemanticAllocationRequest::Yank { step_index } => Ok(SemanticAllocation::Yank {
                    capture_revision: revision(&format!("semantic-union-capture-{step_index}")),
                }),
                SemanticAllocationRequest::ParameterEdit { step_index } => {
                    Ok(SemanticAllocation::ParameterEdit {
                        new_revision: revision(&format!("semantic-union-leaf-{step_index}")),
                    })
                }
                SemanticAllocationRequest::PasteEditedSet {
                    step_index,
                    requirements,
                } => {
                    let prefix = format!("semantic-union-{step_index}");
                    Ok(SemanticAllocation::PasteEditedSet {
                        new_revision: revision(&prefix),
                        identities: EditSetPasteIdentities {
                            group: node(&format!("{prefix}-group")),
                            components: requirements
                                .components
                                .iter()
                                .enumerate()
                                .map(|(member, required)| EditSetComponentIdentities {
                                    nodes: (0..required.nodes)
                                        .map(|index| {
                                            node(&format!("{prefix}-member-{member}-node-{index}"))
                                        })
                                        .collect(),
                                    aliases: (0..required.aliases)
                                        .map(|index| {
                                            node(&format!("{prefix}-member-{member}-alias-{index}"))
                                        })
                                        .collect(),
                                })
                                .collect(),
                            marks: (0..requirements.marks)
                                .map(|index| MarkId::new(format!("{prefix}-mark-{index}")).unwrap())
                                .collect(),
                        },
                    })
                }
                other => panic!("unexpected set lifecycle allocation: {other:?}"),
            }
        },
        |_, _| panic!("no Original register resolution"),
    );
    assert!(bank.is_empty());
    (result, allocations)
}

#[test]
fn semantic_union_yank_is_one_canonical_value_and_preserves_unrelated_context() {
    let (_, document) = fixture();
    let context = entry();
    let (result, allocations) = run(
        &document,
        &context,
        vec![SemanticInstruction::Yank {
            selector: nodes(),
            register: RegisterName::new('a').unwrap(),
        }],
    );
    let plan = result.unwrap();
    assert_eq!(
        allocations,
        vec![SemanticAllocationRequest::Yank { step_index: 0 }]
    );
    assert_eq!(plan.document, document);
    assert_eq!(plan.context, context);
    assert_eq!(plan.register_writes.len(), 2);
    assert_eq!(
        plan.register_writes[&RegisterName::new('a').unwrap()],
        plan.register_writes[&RegisterName::unnamed()]
    );
    let RegisterValue::EditedSet { set } =
        plan.register_writes[&RegisterName::new('a').unwrap()].as_ref()
    else {
        panic!("one union register value expected");
    };
    set.validate_capture(&document).unwrap();
    assert_eq!(set.duration(), frames(12));
    assert_eq!(
        set.components()[0].selection(),
        &SliceCaptureSelection::Child {
            node: node("left-fragment")
        }
    );
    assert_eq!(
        set.components()[1].selection(),
        &SliceCaptureSelection::Child {
            node: node("right-fragment")
        }
    );
    assert_eq!(
        plan.trace[0].resolved_ranges,
        vec![range(0, 5), range(7, 14)]
    );
    assert_eq!(
        plan.trace[0]
            .resolved_nodes
            .iter()
            .map(|member| member.node.clone())
            .collect::<Vec<_>>(),
        vec![node("left-fragment"), node("right-fragment")]
    );
    let Command::Compound { transaction } = &plan.request.as_ref().unwrap().command else {
        panic!("bank-only Compound expected");
    };
    assert_eq!(transaction.steps().len(), 1);
    assert!(transaction.steps()[0].edit().is_none());
}

#[test]
fn semantic_union_cut_then_seam_paste_is_one_atomic_reversible_shared_mark_lifecycle() {
    let (_, document) = fixture();
    let (result, allocations) = run(
        &document,
        &entry(),
        vec![
            SemanticInstruction::Cut {
                selector: nodes(),
                register: RegisterName::new('a').unwrap(),
            },
            SemanticInstruction::Paste {
                register: RegisterName::new('a').unwrap(),
                before: false,
            },
        ],
    );
    let plan = result.unwrap();
    assert_eq!(allocations.len(), 4);
    assert_eq!(
        allocations[0],
        SemanticAllocationRequest::Yank { step_index: 0 }
    );
    assert_eq!(
        allocations[1],
        SemanticAllocationRequest::ParameterEdit { step_index: 1 }
    );
    assert_eq!(
        allocations[2],
        SemanticAllocationRequest::ParameterEdit { step_index: 2 }
    );
    assert!(matches!(
        allocations[3],
        SemanticAllocationRequest::PasteEditedSet { step_index: 3, .. }
    ));
    assert_eq!(plan.document.duration().unwrap(), frames(14));
    assert_eq!(
        plan.document.nodes()[&node("gap")],
        document.nodes()[&node("gap")]
    );
    assert_eq!(
        plan.document
            .children(&node("root"))
            .cloned()
            .collect::<Vec<_>>(),
        vec![node("gap"), node("semantic-union-3-group")]
    );
    let copied = &plan.document.marks()[&MarkId::new("semantic-union-3-mark-0").unwrap()];
    assert_eq!(copied.binding_count(), 2);
    assert_eq!(copied.label, stored(&document).label);
    let request = plan.request.as_ref().unwrap();
    let Command::Compound { transaction } = &request.command else {
        panic!("one Compound expected");
    };
    assert_eq!(transaction.steps().len(), 4);
    let deleted = transaction
        .steps()
        .iter()
        .filter_map(|step| step.edit())
        .filter_map(|edit| match edit.command.as_command() {
            Command::DeleteRipple { node, .. } => Some(node.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(deleted, vec![node("right-fragment"), node("left-fragment")]);
    let replay = replay_compound::<EditError>(&document, request, |_| Ok(())).unwrap();
    assert_eq!(replay.document, plan.document);
    assert_eq!(replay.register_writes, plan.register_writes);
    assert_eq!(
        replay.edit.inverse.apply(&replay.document).unwrap(),
        document
    );
    assert_eq!(replay.edit.duration_delta, 0);
}

#[test]
fn invalid_or_unsupported_semantic_set_consumers_fail_before_allocations() {
    let (_, document) = fixture();
    for instruction in [
        SemanticInstruction::Yank {
            selector: SemanticSelector::Nodes {
                nodes: vec![node("missing")],
            },
            register: RegisterName::new('a').unwrap(),
        },
        SemanticInstruction::Repeat {
            selector: nodes(),
            plays: NonZeroU32::new(2).unwrap(),
            escalation: None,
        },
        SemanticInstruction::Group {
            selector: nodes(),
            label: "Unsupported set group".into(),
        },
        SemanticInstruction::Duplicate { selector: nodes() },
    ] {
        let (result, allocations) = run(&document, &entry(), vec![instruction]);
        assert!(result.is_err());
        assert!(allocations.is_empty());
    }
    let mut stale = entry();
    stale.visual_selection = Some(SemanticVisualSelection::Nodes {
        nodes: vec![node("missing")],
    });
    let (result, allocations) = run(
        &document,
        &stale,
        vec![SemanticInstruction::Yank {
            selector: nodes(),
            register: RegisterName::new('a').unwrap(),
        }],
    );
    assert!(result.is_err());
    assert!(allocations.is_empty());
    for ids in [
        vec![node("left-fragment"), node("left-fragment")],
        (0..65)
            .map(|index| node(&format!("over-limit-{index}")))
            .collect(),
    ] {
        assert!(
            SemanticProgram::new(vec![SemanticInstruction::Yank {
                selector: SemanticSelector::Nodes { nodes: ids },
                register: RegisterName::new('a').unwrap()
            }])
            .is_err()
        );
    }
}
