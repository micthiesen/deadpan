//! Public old-code controls, extended with positive range-union assertions.
//! The baseline source and executed binary remain frozen in qualification scratch.
//! This candidate passed the initial core smoke run; metadata is synthetic.

#[path = "range_set/fixture.rs"]
mod fixture;

use deadpan_core::*;
use fixture::*;
use serde_json::json;
use std::{collections::BTreeMap, num::NonZeroU32};

fn context(cursor: i64) -> SemanticContext {
    SemanticContext {
        original: None,
        scoped: None,
        hold_duration: None,
        shared_hold_duration: None,
        mixed_hold_duration: None,
        selected_hold_duration: None,
        parent: node("root"),
        cursor: ProjectFrame(cursor),
        selected_child: Some(node("source")),
        visual_selection: Some(SemanticVisualSelection::Time {
            anchor: ProjectFrame(0),
            head: ProjectFrame(1),
            extending: false,
        }),
    }
}

#[test]
fn ranges_selector_admission_follows_same_source_capsule_and_separate_splice_controls() {
    let before = fixture();
    before.validate().unwrap();
    assert_eq!(before.duration().unwrap(), frames(24));
    let frozen = before.to_json().unwrap();
    let ranges = [range(2, 6), range(10, 14), range(18, 22)];
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
    assert_eq!(range(2, 22).duration(), frames(20));
    assert!(
        capsules
            .iter()
            .all(|slice| captured_marks(slice).is_empty())
    );

    // Each range keeps the complete same physical Source recipe and selects
    // only its exact output window. A union cannot reject shared authored IDs
    // or mutate/trim the original recipe merely because two windows reuse it.
    for capsule in &capsules {
        let wire = serde_json::to_value(capsule).unwrap();
        assert_eq!(
            wire["nodes"],
            serde_json::to_value(&capsules[0]).unwrap()["nodes"]
        );
        assert_eq!(
            wire["assets"],
            serde_json::to_value(before.assets()).unwrap()
        );
    }

    // Existing semantic relative-range Yank agrees byte for byte with each
    // direct public capture. The unrelated Visual [0,1) cannot supply a hull.
    let bank = BTreeMap::new();
    for (index, selected) in ranges.iter().enumerate() {
        let entry = context(selected.start().0);
        let program = SemanticProgram::new(vec![SemanticInstruction::Yank {
            selector: SemanticSelector::Motion {
                motion: SemanticMotion::Frames {
                    forward: true,
                    count: NonZeroU32::new(4).unwrap(),
                },
            },
            register: RegisterName::new('a').unwrap(),
        }])
        .unwrap();
        let plan = plan_semantic(
            &before,
            &entry,
            &program,
            SemanticRegisterBank {
                entries: &bank,
                version: 17,
            },
            revision("range-control-final"),
            |request| {
                assert_eq!(request, SemanticAllocationRequest::Yank { step_index: 0 });
                Ok(SemanticAllocation::Yank {
                    capture_revision: revision("range-set-capture"),
                })
            },
            |_, _| panic!("no Original register is used"),
        )
        .unwrap();
        let RegisterValue::Edited { slice } =
            plan.register_writes[&RegisterName::new('a').unwrap()].as_ref()
        else {
            panic!("ordinary single-range capture remains an Edited value");
        };
        // This standalone semantic instruction always allocates ordinal zero.
        let expected = capture(&before, *selected, 0);
        assert_eq!(slice.as_ref(), &expected);
        assert_eq!(plan.trace[0].resolved_range, Some(ranges[index]));
        assert_eq!(plan.document, before);
        assert_eq!(plan.context, entry);
        assert_eq!(plan.register_writes.len(), 2);
    }
    assert!(bank.is_empty());

    // Three ordinary public splices form an independent structural oracle:
    // output [0,4) -> Source [2,6), [4,8) -> [10,14), [8,12) -> [18,22).
    let mut oracle = destination(&before, "range-oracle-empty");
    for (index, capsule) in capsules.iter().enumerate() {
        oracle = paste(&oracle, capsule, &format!("range-oracle-{index}"), index);
    }
    let owners = check_oracle(&oracle, &before, &ranges);
    assert_eq!(owners.len(), 3);
    assert!(oracle.marks().is_empty());
    assert_eq!(before.to_json().unwrap(), frozen);

    // The actual public union constructor admits repeated identical Source
    // recipes and retains each distinct selected window.
    let union = CapturedEditSet::capture_ranges(
        &before,
        &node("root"),
        &ranges,
        timing("range-set-capture", 0),
    )
    .unwrap();
    union.validate_capture(&before).unwrap();
    assert_eq!(union.components(), capsules);
    assert_eq!(union.duration(), frames(12));
    let candidate = json!({ "scope": "root", "components": capsules, "marks": {} });
    assert_eq!(
        CapturedEditSet::from_json(&candidate.to_string()).unwrap(),
        union
    );
    assert_eq!(before.to_json().unwrap(), frozen);

    // The frozen old-code copy fails here after all independent controls.
    let mut wire = serde_json::to_value(
        SemanticProgram::new(vec![SemanticInstruction::Yank {
            selector: SemanticSelector::SelectedBeat,
            register: RegisterName::new('a').unwrap(),
        }])
        .unwrap(),
    )
    .unwrap();
    wire["instructions"][0]["selector"] = json!({
        "type": "ranges", "ranges": [{"start":18,"end":22},{"start":2,"end":6},{"start":10,"end":14}],
    });
    let admitted = serde_json::from_value::<SemanticProgram>(wire);
    assert!(
        admitted.is_ok(),
        "strict Ranges admission follows all existing public controls: {admitted:?}"
    );
}
