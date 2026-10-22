//! Public old-code controls for the next transcript-search selector.
//! Literal ranges prove the existing union mechanics, not query projection or
//! analysis freshness. Only the last assertion asks for the new selector wire.

#[path = "range_set/fixture.rs"]
mod fixture;

use deadpan_core::*;
use fixture::*;
use serde_json::json;
use std::collections::BTreeMap;

fn context() -> SemanticContext {
    SemanticContext {
        original: None,
        scoped: None,
        hold_duration: None,
        shared_hold_duration: None,
        mixed_hold_duration: None,
        selected_hold_duration: None,
        parent: node("root"),
        cursor: ProjectFrame(0),
        selected_child: Some(node("source")),
        visual_selection: Some(SemanticVisualSelection::Time {
            anchor: ProjectFrame(0),
            head: ProjectFrame(1),
            extending: false,
        }),
    }
}

fn offsets(start: i64, end: i64) -> SemanticRangeOffsets {
    SemanticRangeOffsets {
        start: SemanticFrameOffset(start),
        end: SemanticFrameOffset(end),
    }
}

#[test]
fn transcript_search_admission_follows_literal_ranges_cut_capture_and_paste_controls() {
    let before = fixture();
    before.validate().unwrap();
    let frozen = before.to_json().unwrap();
    let ranges = [range(2, 6), range(10, 14), range(18, 22)];
    let input = vec![offsets(18, 22), offsets(2, 6), offsets(10, 14)];
    let selected = before
        .select_semantic_ranges(&context(), input.clone())
        .unwrap();
    assert_eq!(selected.cursor, ProjectFrame(22));
    assert_eq!(
        selected.visual_selection,
        Some(SemanticVisualSelection::Ranges {
            ranges: ranges.to_vec(),
        })
    );

    let capsules = ranges
        .iter()
        .enumerate()
        .map(|(index, selected)| capture(&before, *selected, u32::try_from(index).unwrap()))
        .collect::<Vec<_>>();
    assert_eq!(
        capsules
            .iter()
            .map(|slice| slice.duration().frames())
            .sum::<i64>(),
        12
    );
    assert_eq!(capture(&before, range(2, 22), 0).duration(), frames(20));

    let program = SemanticProgram::new(vec![
        SemanticInstruction::SelectRanges { ranges: input },
        SemanticInstruction::Cut {
            selector: SemanticSelector::VisualSelection,
            register: RegisterName::new('a').unwrap(),
        },
    ])
    .unwrap();
    assert_eq!(
        serde_json::from_value::<SemanticProgram>(serde_json::to_value(&program).unwrap()).unwrap(),
        program
    );
    let bank = BTreeMap::new();
    let plan = plan_semantic(
        &before,
        &context(),
        &program,
        SemanticRegisterBank {
            entries: &bank,
            version: 19,
        },
        revision("search-control-final"),
        |request| {
            Ok(match request {
                SemanticAllocationRequest::Yank { .. } => SemanticAllocation::Yank {
                    capture_revision: revision("range-set-capture"),
                },
                SemanticAllocationRequest::DeleteRange {
                    step_index,
                    required_nodes,
                } => SemanticAllocation::DeleteRange {
                    new_revision: revision(&format!("search-control-cut-{step_index}")),
                    identities: SplitIdentities {
                        nodes: (0..required_nodes)
                            .map(|n| node(&format!("search-control-split-{step_index}-{n}")))
                            .collect(),
                    },
                },
                other => panic!("unexpected allocation: {other:?}"),
            })
        },
        |_, _| panic!("no Original register is used"),
    )
    .unwrap();
    assert_eq!(plan.document.duration().unwrap(), frames(12));
    assert_eq!(plan.context.cursor, ProjectFrame(2));
    assert!(plan.context.visual_selection.is_none());
    assert_eq!(plan.trace[1].resolved_ranges, ranges);
    assert!(plan.trace[1].resolved_range.is_none());
    let RegisterValue::EditedSet { set } =
        plan.register_writes[&RegisterName::new('a').unwrap()].as_ref()
    else {
        panic!("the separated selection must capture one EditedSet");
    };
    assert_eq!(set.components(), capsules);
    assert_eq!(set.duration(), frames(12));
    set.validate_capture(&before).unwrap();
    assert_eq!(
        set.as_ref(),
        &CapturedEditSet::capture_ranges(
            &before,
            &node("root"),
            &ranges,
            timing("range-set-capture", 0),
        )
        .unwrap()
    );
    assert_eq!(
        plan.register_writes[&RegisterName::new('a').unwrap()],
        plan.register_writes[&RegisterName::unnamed()]
    );
    assert!(bank.is_empty());

    let request = plan.request.as_ref().unwrap();
    let Command::Compound { transaction } = &request.command else {
        panic!("one Cut must publish one Compound");
    };
    assert_eq!(transaction.steps().len(), 4);
    assert!(matches!(transaction.steps()[0], ResolvedStep::Yank { .. }));
    for (step, expected) in transaction.steps()[1..].iter().zip(ranges.iter().rev()) {
        let Command::DeleteRange { range, .. } = step.edit().unwrap().command.as_command() else {
            panic!("each member must use an ordinary range deletion");
        };
        assert_eq!(range, expected);
    }
    let transaction = apply(&before, request).unwrap();
    let after = transaction.forward.apply(&before).unwrap();
    assert_eq!(after, plan.document);
    assert_eq!(transaction.inverse.apply(&after).unwrap(), before);

    // Independent ordinary pastes provide an exact Source-window oracle.
    let empty = destination(&before, "search-control-empty");
    let mut oracle = empty.clone();
    for (index, capsule) in capsules.iter().enumerate() {
        oracle = paste(
            &oracle,
            capsule,
            &format!("search-control-separate-{index}"),
            index,
        );
    }
    assert_eq!(check_oracle(&oracle, &before, &ranges).len(), 3);
    let pasted = paste_set(&empty, set, "search-control-set");
    assert_eq!(pasted.children(&node("root")).count(), 1);
    assert_eq!(
        pasted.children(&node("search-control-set-group")).count(),
        3
    );
    assert_eq!(pasted.duration().unwrap(), oracle.duration().unwrap());
    let flat = edit(
        &pasted,
        "search-control-ungroup",
        Command::Ungroup {
            node: node("search-control-set-group"),
        },
    );
    assert_eq!(check_oracle(&flat, &before, &ranges).len(), 3);
    assert_eq!(before.to_json().unwrap(), frozen);

    let mut wire = serde_json::to_value(
        SemanticProgram::new(vec![SemanticInstruction::Yank {
            selector: SemanticSelector::SelectedBeat,
            register: RegisterName::new('a').unwrap(),
        }])
        .unwrap(),
    )
    .unwrap();
    wire["instructions"][0]["selector"] = json!({
        "type": "transcript_search", "query": "hello world",
    });
    let admitted = serde_json::from_value::<SemanticProgram>(wire);
    assert!(
        admitted.is_ok(),
        "transcript_search admission follows all ordinary public controls: {admitted:?}"
    );
}
