use super::*;

fn committed(document: &ProjectDocument, command: crate::Command, label: &str) -> ProjectDocument {
    let request = crate::CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision(label),
        command,
    };
    crate::apply(document, &request)
        .unwrap()
        .forward
        .apply(document)
        .unwrap()
}

fn clocked() -> ProjectDocument {
    let before = tree(
        &["lead", "inner", "tail"],
        vec![("lead", hold(2)), ("inner", hold(12)), ("tail", hold(2))],
    );
    let wrapped = committed(
        &before,
        crate::Command::WrapRetime {
            node: node("inner"),
            id: node("slow"),
            duration: FrameDuration::new(24).unwrap(),
            pitch: PitchPolicy::FollowSpeed,
        },
        "semantic-clock-wrap",
    );
    let crate::PauseInsertionTarget::Retimed(target) =
        wrapped.pause_insertion_target(ProjectFrame(5)).unwrap()
    else {
        panic!("public retimed pause target")
    };
    let NodeKind::Hold { recipe } = hold(3).kind else {
        unreachable!()
    };
    let before = committed(
        &wrapped,
        crate::Command::InsertTime {
            at: ProjectFrame(5),
            hold: recipe,
            id: node("inserted-pause"),
            identities: crate::SplitIdentities {
                nodes: (0..target.required_ids)
                    .map(|index| node(&format!("clock-split-{index}")))
                    .collect(),
            },
            timing: crate::AudioTimingId {
                allocation: revision("semantic-clock-pause"),
                ordinal: 0,
            },
        },
        "semantic-clock-pause",
    );
    let clock = &before.derived_clocks()[&node("slow")];
    assert_eq!(clock.extent, ExactRatio::integer(27));
    assert_eq!(clock.child_end, ExactRatio::new(27, 2).unwrap());
    before
}

fn tape(numerator: i128, denominator: i128) -> SemanticInstruction {
    SemanticInstruction::Retime {
        speed: ExactRatio::new(numerator, denominator).unwrap(),
        pitch: PitchPolicy::FollowSpeed,
        wrap: false,
    }
}

#[test]
fn clocked_speed_semantics_use_exact_input_and_current_trace() {
    let before = clocked();
    let planned = plan(
        &before,
        selected("slow", 4),
        vec![tape(7, 8)],
        &BTreeMap::new(),
    )
    .unwrap();
    let after = &planned.document;
    assert_eq!(after.duration().unwrap().frames(), 19);
    let clock = &after.derived_clocks()[&node("slow")];
    assert_eq!(clock.extent, ExactRatio::integer(15));
    assert_eq!(clock.child_end, ExactRatio::new(27, 2).unwrap());
    assert_eq!(planned.context.selected_child, Some(node("slow")));
    assert_eq!(planned.context.cursor, ProjectFrame(2));
    assert_eq!(planned.trace[0].resolved_range, Some(range(2, 17)));
    replays_and_reverts(&before, &planned);
}

#[test]
fn clocked_speed_named_call_keeps_exact_input_for_the_next_staged_update() {
    let before = clocked();
    let bank = BTreeMap::from([(name('a'), macro_value(vec![tape(7, 8)]))]);
    let saved = bank.clone();
    let planned = plan(
        &before,
        selected("slow", 4),
        vec![call('a', 1), tape(3, 2)],
        &bank,
    )
    .unwrap();
    assert_eq!(planned.document.duration().unwrap().frames(), 13);
    assert_eq!(
        planned.document.derived_clocks()[&node("slow")].extent,
        ExactRatio::integer(9)
    );
    assert_eq!(planned.context.selected_child, Some(node("slow")));
    assert_eq!(planned.context.cursor, ProjectFrame(2));
    assert_eq!(
        planned.trace.last().unwrap().resolved_range,
        Some(range(2, 11))
    );
    assert_eq!(bank, saved);
    replays_and_reverts(&before, &planned);
}

#[test]
fn clocked_speed_semantic_noop_pitch_and_late_failures_leave_inputs_unchanged() {
    let before = clocked();
    let json = before.to_json().unwrap();
    let bank = BTreeMap::from([(name('a'), macro_value(vec![tape(7, 8)]))]);
    let saved = bank.clone();
    for instructions in [
        vec![tape(1, 2)],
        vec![call('a', 1), tape(7, 8)],
        vec![tape(7, 8), SemanticInstruction::Pitch { semitones: 3 }],
        vec![tape(7, 8), SemanticInstruction::Pitch { semitones: 0 }],
    ] {
        assert!(plan(&before, selected("slow", 4), instructions, &bank).is_err());
        assert_eq!(before.to_json().unwrap(), json);
        assert_eq!(bank, saved);
    }
}
