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
                    audio_edges: Default::default(),
                    kind: NodeKind::Repeat {
                        child: id("inner"),
                        iterations: IterationOrder::new(revision("plays"), plays).unwrap(),
                        gap: Some(recipe(1)),
                    },
                },
            ),
            ("inner", source(2)),
        ],
    )
}

#[test]
fn source_and_hold_interiors_keep_pre_split_lattices_and_distinct_current_placements() {
    for lead in [source(2), BeatNode::hold("Hold", recipe(2))] {
        let before = composite_suffix(lead, 3);
        let after = edit(&before, insertion(&before, "insert", 1, 1));
        assert_eq!(
            after.duration().unwrap().frames(),
            before.duration().unwrap().frames() + 1
        );
        assert_eq!(children(&after).len(), 4);
        assert_eq!(children(&after)[1], id("pause-insert"));
        assert_eq!(children(&after)[3], id("repeat"));
        let right = owner(&after, &children(&after)[2]);
        let left = owner(&after, &children(&after)[0]);
        let old_clock = AudioTimingId {
            allocation: revision("insert"),
            ordinal: 0,
        };
        let current_clock = AudioTimingId {
            allocation: revision("insert"),
            ordinal: 1,
        };
        let bindings = after.audio_bindings();
        assert_eq!(bindings.timings().len(), 2);
        assert_eq!(
            bindings.bindings()[&left].lattice,
            bindings.bindings()[&right].lattice
        );
        assert_eq!(
            bindings.bindings()[&right].lattice.reference.timing,
            old_clock
        );
        assert!(bindings.bindings()[&left].reanchors.is_empty());
        let right_binding = &bindings.bindings()[&right];
        assert_eq!(right_binding.reanchors.len(), 1);
        assert_eq!(
            right_binding.reanchors[0].placement.reference.timing,
            current_clock
        );
        assert_eq!(
            right_binding.reanchors[0].placement.reference.physical,
            right
        );
        assert_eq!(
            reference_at_anchor(&after, &right),
            ExactRatio::integer(1602)
        );
        assert_eq!(
            resolved(&after, &right).resume.unwrap().local_boundary,
            ExactRatio::ONE
        );
        // No alias from the split graph may replace the immutable old graph.
        assert!(!bindings.timings()[&old_clock].nodes().contains_key(&right));
        assert!(
            bindings.timings()[&current_clock]
                .nodes()
                .contains_key(&right)
        );
        assert_eq!(bindings.bindings()[&id("inner")].reanchors.len(), 1);
        assert_eq!(bindings.gap_bindings()[&id("repeat")].reanchors.len(), 1);
    }
}

#[test]
fn interior_cut_before_billion_play_suffix_stays_compact_and_reversible() {
    let before = composite_suffix(source(2), 1_000_000_000);
    let after = edit(&before, insertion(&before, "insert", 1, 1));
    assert_eq!(after.nodes().len(), before.nodes().len() + 4);
    let NodeKind::Repeat { iterations, .. } = &after.nodes()[&id("repeat")].kind else {
        panic!()
    };
    assert_eq!(iterations.segment_count(), 1);
    assert_eq!(after.nodes()[&id("repeat")], before.nodes()[&id("repeat")]);
    let value = after
        .audio_bindings()
        .resolve(
            &id("inner"),
            &InstancePath {
                node: id("inner"),
                repeats: vec![RepeatInstance {
                    node: id("repeat"),
                    iteration: iterations.at(iterations.len() - 1).unwrap(),
                }],
            },
            MAX_AUDIO_BINDING_ENTRIES,
        )
        .unwrap();
    assert!(value.work < 100);
    assert_eq!(value.resume.unwrap().local_boundary, ExactRatio::ZERO);
    assert_eq!(after.audio_bindings().timings().len(), 2);
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
