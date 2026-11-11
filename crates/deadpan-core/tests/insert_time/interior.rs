use super::*;

fn composite_suffix(lead: BeatNode, plays: u32) -> ProjectDocument {
    tree(
        &["original", "repeat"],
        vec![
            ("original", lead),
            (
                "repeat",
                BeatNode {
                    audio_treatments: Default::default(),
                    label: "Repeated Original".into(),
                    framing: None,
                    audio_editorial_edges: Default::default(),
                    audio_edges: Default::default(),
                    kind: NodeKind::Repeat {
                        child: id("inner"),
                        iterations: IterationOrder::new(revision("plays"), plays).unwrap(),
                        gap: Some(recipe(1)),
                        escalation: None,
                    },
                    cutaways: Vec::new(),
                    captions: Vec::new(),
                },
            ),
            ("inner", source(2)),
        ],
    )
}

#[test]
fn interior_split_and_pause_shift_each_mark_once_without_losing_fragment_identity() {
    let before = composite_suffix(source(2), 3);
    let mut within = mark(0, InsertionBias::Right, false);
    within.boundary.coordinate = Anchor::Local {
        node: id("original"),
        position: ExactRatio::new(3, 2).unwrap(),
    };
    let mut wire = serde_json::to_value(&before).unwrap();
    wire["marks"] = serde_json::to_value(BTreeMap::from([
        ("left", mark(1, InsertionBias::Left, false)),
        ("right", mark(1, InsertionBias::Right, false)),
        ("later", mark(3, InsertionBias::Right, false)),
        ("pin", mark(3, InsertionBias::Right, true)),
        ("within", within),
    ]))
    .unwrap();
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let after = edit(&before, insertion(&before, "insert", 1, 1));
    assert_eq!(after.marks().len(), before.marks().len());
    for (key, position) in [("left", 1), ("right", 2), ("later", 4), ("pin", 3)] {
        assert_eq!(
            mark_position(&after, key),
            ExactRatio::integer(position),
            "{key}"
        );
    }
    assert_eq!(
        mark_position(&after, "within"),
        ExactRatio::new(5, 2).unwrap()
    );
    let again = edit(&after, insertion(&after, "again", 2, 1));
    assert_eq!(
        mark_position(&again, "within"),
        ExactRatio::new(7, 2).unwrap()
    );
}

#[test]
fn interior_identity_and_timing_exhaustion_leave_the_input_unchanged() {
    let before = composite_suffix(source(2), 3);
    let saved = before.to_json().unwrap();
    let mut missing = insertion(&before, "missing", 1, 1);
    let Command::InsertTime { identities, .. } = &mut missing.command else {
        panic!()
    };
    identities.nodes.truncate(2);
    assert_eq!(
        apply(&before, &missing).unwrap_err().code,
        EditErrorCode::InvalidCommand
    );
    let mut exhausted = insertion(&before, "exhausted", 1, 1);
    let Command::InsertTime { timing, .. } = &mut exhausted.command else {
        panic!()
    };
    timing.ordinal = u32::MAX;
    assert_eq!(
        apply(&before, &exhausted).unwrap_err().code,
        EditErrorCode::LimitExceeded
    );
    assert_eq!(before.to_json().unwrap(), saved);
    // A legacy-admitted seam needs only the caller's one timing identity.
    if let Command::InsertTime { at, .. } = &mut exhausted.command {
        *at = ProjectFrame(2);
    }
    edit(&before, exhausted);
}
