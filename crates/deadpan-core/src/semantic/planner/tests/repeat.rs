use super::*;

fn repeat(selector: SemanticSelector, plays: u32) -> SemanticInstruction {
    SemanticInstruction::Repeat {
        selector,
        plays: NonZeroU32::new(plays).unwrap(),
    }
}
fn inverse(document: &ProjectDocument, planned: &SemanticPlan) {
    let replay = crate::replay_compound::<EditError>(
        document,
        planned.request.as_ref().unwrap(),
        |_| Ok(()),
    )
    .unwrap();
    assert_eq!(replay.document, planned.document);
    assert_eq!(
        replay.edit.inverse.apply(&replay.document).unwrap(),
        *document
    );
    assert!(replay.register_writes.is_empty());
}

#[test]
fn selected_beat_repeat_is_independent_of_cursor_and_wraps_instead_of_setting() {
    let document = tree(&["a", "b"], vec![("a", hold(3)), ("b", hold(4))]);
    let entry = SemanticContext {
        selected_child: Some(node("a")),
        ..context("root", 6)
    };
    let planned = plan(
        &document,
        entry,
        vec![
            repeat(SemanticSelector::SelectedBeat, 3),
            repeat(SemanticSelector::SelectedBeat, 2),
        ],
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(planned.document.duration().unwrap().frames(), 22);
    assert_eq!(planned.context.cursor, ProjectFrame(0));
    assert_eq!(planned.context.selected_child, Some(node("repeat-1")));
    assert_eq!(
        planned.trace[0].resolved_selection,
        Some(SliceCaptureSelection::Child { node: node("a") })
    );
    assert!(
        matches!(&planned.document.nodes()[&node("repeat-0")].kind, NodeKind::Repeat { child, iterations, .. } if child == &node("a") && iterations.len() == 3)
    );
    assert!(
        matches!(&planned.document.nodes()[&node("repeat-1")].kind, NodeKind::Repeat { child, iterations, .. } if child == &node("repeat-0") && iterations.len() == 2)
    );
    inverse(&document, &planned);
}

#[test]
fn oriented_visual_and_motion_ranges_repeat_exact_staged_contents() {
    let document = fixture(10);
    for (anchor, head, extending) in [(2, 7, true), (7, 2, true), (7, 2, false)] {
        let entry = SemanticContext {
            visual_selection: Some(SemanticVisualSelection::Time {
                anchor: ProjectFrame(anchor),
                head: ProjectFrame(head),
                extending,
            }),
            ..context("root", head)
        };
        let planned = plan(
            &document,
            entry,
            vec![
                repeat(SemanticSelector::VisualSelection, 3),
                motion(true, 15),
                repeat(
                    SemanticSelector::Motion {
                        motion: SemanticMotion::Frames {
                            forward: false,
                            count: NonZeroU32::new(2).unwrap(),
                        },
                    },
                    2,
                ),
            ],
            &BTreeMap::new(),
        )
        .unwrap();
        assert_eq!(planned.trace[0].resolved_range, Some(range(2, 7)));
        assert_eq!(planned.trace[2].resolved_range, Some(range(15, 17)));
        assert_eq!(planned.context.visual_selection, None);
        assert_eq!(planned.context.cursor, ProjectFrame(15));
        assert_eq!(planned.document.duration().unwrap().frames(), 22);
        inverse(&document, &planned);
    }
}

#[test]
fn counted_calls_keep_requested_repeat_plays_and_motion_clamping() {
    let document = fixture(2);
    let instruction = repeat(
        SemanticSelector::Motion {
            motion: SemanticMotion::Frames {
                forward: true,
                count: NonZeroU32::new(100).unwrap(),
            },
        },
        3,
    );
    let bank = BTreeMap::from([(name('a'), macro_value(vec![instruction.clone()]))]);
    let planned = plan(&document, context("root", 0), vec![call('a', 2)], &bank).unwrap();
    assert_eq!(planned.document.duration().unwrap().frames(), 18);
    assert_eq!(planned.trace[1].resolved_range, Some(range(0, 2)));
    assert_eq!(planned.trace[2].resolved_range, Some(range(0, 6)));
    assert_eq!(planned.trace[1].instruction, instruction);
    assert_eq!(planned.trace[2].instruction, instruction);
    inverse(&document, &planned);
}

#[test]
fn repeat_failures_do_not_publish_staged_edits_and_allocations_are_checked() {
    let document = fixture(6);
    let original = document.clone();
    assert!(
        plan(
            &document,
            context("root", 0),
            vec![repeat(SemanticSelector::SelectedBeat, 2)],
            &BTreeMap::new()
        )
        .is_err()
    );
    let mut entry = context("root", 0);
    entry.selected_child = Some(node("held"));
    assert!(
        plan(
            &document,
            entry.clone(),
            vec![repeat(SemanticSelector::SelectedBeat, 2), call('z', 1)],
            &BTreeMap::new()
        )
        .is_err()
    );
    let result = plan_semantic(
        &document,
        &entry,
        &program(vec![repeat(SemanticSelector::SelectedBeat, 2)]),
        SemanticRegisterBank {
            entries: &BTreeMap::new(),
            version: 0,
        },
        revision("outer"),
        |_| {
            Ok(SemanticAllocation::Repeat {
                new_revision: revision("leaf"),
                identities: RepeatSelectionIdentities {
                    repeat: node("held"),
                    group: None,
                    split: SplitIdentities::default(),
                },
            })
        },
        no_original,
    );
    assert_eq!(result.unwrap_err().code, EditErrorCode::IdentityConflict);
    for wire in [
        r#"{"type":"repeat","selector":{"type":"selected_beat"},"plays":0}"#,
        r#"{"type":"repeat","selector":{"type":"selected_beat"},"plays":2,"count":4}"#,
    ] {
        assert!(serde_json::from_str::<SemanticInstruction>(wire).is_err());
    }
    assert_eq!(document, original);
}
