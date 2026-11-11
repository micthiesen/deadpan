use super::*;

fn deletion(document: &ProjectDocument, first: &str, last: &str) -> CommandRequest {
    request(
        document,
        "forest-cut",
        Command::DeleteChildren {
            parent: id("group"),
            first: id(first),
            last: id(last),
            timing: AudioTimingId {
                allocation: revision("forest-cut"),
                ordinal: 0,
            },
        },
    )
}

fn fixture() -> ProjectDocument {
    tree(
        &["prefix", "group", "tail"],
        vec![
            ("prefix", source(1)),
            (
                "group",
                BeatNode::sequence(
                    "Group",
                    ["left", "a", "middle", "b", "right"].map(id).to_vec(),
                ),
            ),
            ("left", BeatNode::sequence("Left", vec![])),
            ("a", source(2)),
            ("middle", BeatNode::sequence("Middle", vec![])),
            ("b", source(3)),
            ("right", BeatNode::sequence("Right", vec![])),
            ("tail", source(4)),
        ],
    )
}

#[test]
fn equal_time_children_are_distinct_and_reversed_or_foreign_names_refuse() {
    let before = fixture();
    for (first, last) in [
        ("a", "left"),
        ("tail", "tail"),
        ("left", "missing"),
        ("group", "right"),
    ] {
        assert!(
            apply(&before, &deletion(&before, first, last)).is_err(),
            "{first}..={last}"
        );
    }
    let mut wrong = deletion(&before, "left", "right");
    let Command::DeleteChildren { timing, .. } = &mut wrong.command else {
        panic!()
    };
    timing.allocation = revision("wrong");
    assert!(apply(&before, &wrong).is_err());
    assert_eq!(before, fixture());
}

#[test]
fn empty_endpoint_owned_marks_follow_loss_policy_without_time_fallback() {
    let before = fixture();
    let mut wire = json!(before);
    let mark = |owner: &str, host: &str, loss_policy| Mark {
        owner: id(owner),
        label: "Boundary".into(),
        boundary: BoundaryAnchor {
            coordinate: Anchor::Local {
                node: id(host),
                position: ExactRatio::ZERO,
            },
            bias: InsertionBias::Right,
        },
        loss_policy,
        state: MarkState::Bound,
        fragments: vec![],
    };
    wire["marks"] = json!({
        "owned": mark("left", "left", AnchorLossPolicy::DeleteOwned),
        "lost": mark("root", "left", AnchorLossPolicy::KeepUnresolved),
        "survives": mark("right", "right", AnchorLossPolicy::DeleteOwned),
    });
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let after = edit(&before, deletion(&before, "left", "middle"));
    assert!(!after.marks().contains_key(&MarkId::new("owned").unwrap()));
    assert!(matches!(
        after.marks()[&MarkId::new("lost").unwrap()].state,
        MarkState::Unresolved { .. }
    ));
    assert_eq!(
        after.marks()[&MarkId::new("survives").unwrap()],
        before.marks()[&MarkId::new("survives").unwrap()]
    );
}
