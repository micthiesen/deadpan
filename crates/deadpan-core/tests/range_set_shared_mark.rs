//! Public multiplicity control: one unresolved logical mark, two copies of its
//! same physical Source owner. No authored JSON mutation or private mark APIs.
//! The frozen baseline proves separate-paste multiplicity. This candidate also
//! checks the actual union and passed the initial core smoke run.

#[path = "range_set/fixture.rs"]
mod fixture;

use deadpan_core::*;
use fixture::*;
use std::collections::BTreeSet;

#[test]
fn repeated_same_source_capsules_need_two_distinct_destination_bindings_of_one_logical_mark() {
    let before = unresolved_fixture();
    let frozen = before.to_json().unwrap();
    let mark_id = MarkId::new("one-logical-cue").unwrap();
    let original = &before.marks()[&mark_id];
    assert_eq!(before.marks().len(), 1);
    assert_eq!(original.binding_count(), 1);
    assert!(matches!(original.state, MarkState::Unresolved { .. }));
    let ranges = [range(2, 6), range(18, 22)];
    let left = capture(&before, ranges[0], 0);
    let right = capture(&before, ranges[1], 1);
    for capsule in [&left, &right] {
        let marks = captured_marks(capsule);
        assert_eq!(marks.len(), 1);
        assert_eq!(marks[&mark_id], *original);
        assert_eq!(capsule.identity_requirements().unwrap().marks, 1);
    }
    assert_eq!(captured_marks(&left), captured_marks(&right));

    // A hull is independently wrong: it includes all 12 unselected frames
    // between these 4f windows and retains only one physical Source recipe.
    let hull = capture(&before, range(2, 22), 0);
    assert_eq!(hull.duration(), frames(20));
    assert_eq!(captured_marks(&hull)[&mark_id].binding_count(), 1);

    let empty = destination(&before, "range-mark-oracle-empty");
    let once = paste(&empty, &left, "range-mark-left", 0);
    let oracle = paste(&once, &right, "range-mark-right", 1);
    let owners = check_oracle(&oracle, &before, &ranges);
    assert_eq!(owners.len(), 2);
    assert_eq!(oracle.duration().unwrap(), frames(8));

    // Separate ordinary pastes truthfully produce two logical MarkIds. This
    // establishes physical multiplicity only; it is deliberately insufficient
    // for a future single-register union, which must keep one logical mark
    // with TWO bindings after component-specific owner renaming.
    assert_eq!(oracle.marks().len(), 2);
    let copied = oracle.marks().values().collect::<Vec<_>>();
    assert!(copied.iter().all(|mark| mark.binding_count() == 1));
    let copied_owners = copied
        .iter()
        .map(|mark| mark.owner.clone())
        .collect::<BTreeSet<_>>();
    assert_eq!(copied_owners, owners);
    assert_eq!(copied_owners.len(), 2);
    for mark in copied {
        assert_eq!(mark.label, original.label);
        assert_eq!(mark.loss_policy, original.loss_policy);
        assert_eq!(mark.boundary.bias, original.boundary.bias);
        assert_eq!(mark.boundary.coordinate, original.boundary.coordinate);
        assert_eq!(mark.state, original.state);
        assert!(matches!(
            oracle.nodes()[&mark.owner].kind,
            NodeKind::Source { .. }
        ));
    }
    // Deduplicating equal source-side bindings before each component has a
    // destination owner would collapse this two-owner result incorrectly.
    let source_bindings = left.to_json().unwrap();
    assert_eq!(
        CapturedEditSlice::from_json(&source_bindings).unwrap(),
        left
    );
    let union = CapturedEditSet::capture_ranges(
        &before,
        &node("root"),
        &ranges,
        timing("range-set-capture", 0),
    )
    .unwrap();
    union.validate_capture(&before).unwrap();
    assert_eq!(union.identity_requirements().unwrap().marks, 1);
    let pasted = paste_set(&empty, &union, "range-shared-union");
    let pasted = edit(
        &pasted,
        "range-shared-ungroup",
        Command::Ungroup {
            node: node("range-shared-union-group"),
        },
    );
    let owners = check_oracle(&pasted, &before, &ranges);
    assert_eq!(pasted.marks().len(), 1);
    let mark = pasted.marks().values().next().unwrap();
    assert_eq!(mark.binding_count(), 2);
    assert_eq!(
        mark.bindings()
            .map(|binding| binding.owner)
            .collect::<BTreeSet<_>>(),
        owners
    );
    for binding in mark.bindings() {
        assert_eq!(binding.coordinate, original.boundary.coordinate);
        assert_eq!(binding.state, original.state);
    }
    assert_eq!(mark.label, original.label);
    assert_eq!(mark.loss_policy, original.loss_policy);
    assert_eq!(mark.boundary.bias, original.boundary.bias);
    assert_eq!(before.to_json().unwrap(), frozen);
}
