//! `s`/`:split` as a recorded instruction.

use super::*;

fn selected(child: &str, cursor: i64) -> SemanticContext {
    SemanticContext {
        selected_child: Some(node(child)),
        ..context("root", cursor)
    }
}

#[test]
fn a_recorded_split_cuts_at_the_cursor_and_selects_the_right_fragment() {
    let document = tree(&["a", "b"], vec![("a", hold(3)), ("b", hold(6))]);
    let forward = SemanticInstruction::MoveFrames {
        forward: true,
        count: NonZeroU32::new(2).unwrap(),
    };
    let planned = plan(
        &document,
        selected("b", 5),
        vec![
            SemanticInstruction::Split,
            forward,
            SemanticInstruction::Split,
        ],
        &BTreeMap::new(),
    )
    .unwrap();
    // Two cuts at 5 and 7 leave three pieces of b; time is unchanged.
    assert_eq!(planned.document.duration().unwrap().frames(), 9);
    let NodeKind::Sequence { children } = &planned.document.nodes()[&node("root")].kind else {
        panic!()
    };
    assert_eq!(children.len(), 4);
    assert_eq!(planned.context.cursor, ProjectFrame(7));
    assert_eq!(planned.context.selected_child.as_ref(), Some(&children[3]));
    assert_eq!(planned.trace[0].resolved_range, Some(range(5, 9)));
    assert_eq!(planned.trace[2].resolved_range, Some(range(7, 9)));
    let replay =
        crate::replay_compound::<EditError>(&document, planned.request.as_ref().unwrap(), |_| {
            Ok(())
        })
        .unwrap();
    assert_eq!(replay.document, planned.document);
    assert_eq!(
        replay.edit.inverse.apply(&replay.document).unwrap(),
        document
    );
    // A boundary, a missing selection and a Visual range refuse.
    for context in [
        selected("b", 3),
        context("root", 5),
        SemanticContext {
            visual_selection: Some(crate::SemanticVisualSelection::Time {
                anchor: ProjectFrame(4),
                head: ProjectFrame(6),
                extending: false,
            }),
            ..selected("b", 5)
        },
    ] {
        assert!(
            plan(
                &document,
                context,
                vec![SemanticInstruction::Split],
                &BTreeMap::new()
            )
            .is_err()
        );
    }
}

#[test]
fn recorded_marks_save_the_native_letter_and_jump_back_to_it() {
    let document = tree(&["a", "b"], vec![("a", hold(3)), ("b", hold(6))]);
    let forward = SemanticInstruction::MoveFrames {
        forward: true,
        count: NonZeroU32::new(3).unwrap(),
    };
    let planned = plan(
        &document,
        selected("b", 4),
        vec![
            SemanticInstruction::SetMark { letter: 'q' },
            forward,
            SemanticInstruction::JumpMark { letter: 'q' },
        ],
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(planned.context.cursor, ProjectFrame(4));
    assert_eq!(planned.context.selected_child, Some(node("b")));
    let mark = &planned.document.marks()[&crate::semantic::native_mark_id('q').unwrap()];
    assert_eq!(mark.label, "q");
    let replay =
        crate::replay_compound::<EditError>(&document, planned.request.as_ref().unwrap(), |_| {
            Ok(())
        })
        .unwrap();
    assert_eq!(replay.document, planned.document);
    assert_eq!(
        replay.edit.inverse.apply(&replay.document).unwrap(),
        document
    );
    // Unset marks and non-letters refuse.
    assert!(
        plan(
            &document,
            selected("b", 4),
            vec![SemanticInstruction::JumpMark { letter: 'z' }],
            &BTreeMap::new()
        )
        .is_err()
    );
    assert!(SemanticProgram::new(vec![SemanticInstruction::SetMark { letter: '1' }]).is_err());
}

#[test]
fn a_recorded_sound_placement_puts_the_catalog_sound_at_each_cursor() {
    let mut document = tree(&["a", "b"], vec![("a", hold(30)), ("b", hold(30))]);
    let time_base = crate::SourceTimeBase::new(1, 48_000).unwrap();
    let span = crate::SourceSpan::new(
        crate::SourceTimestamp {
            ticks: 0,
            time_base,
        },
        crate::SourceTimestamp {
            ticks: 9_600,
            time_base,
        },
    )
    .unwrap();
    let asset = crate::AssetId::new("boing").unwrap();
    document.assets.insert(
        asset.clone(),
        crate::AssetRecord {
            label: "Boing".into(),
            content_hash: "a".repeat(64),
            video: None,
            audio: Some(span),
            still_image: false,
            frame_count: None,
            source_qualification: Some(crate::SourceQualificationId::new("b".repeat(64)).unwrap()),
        },
    );
    document.validate().unwrap();
    let place = SemanticInstruction::PlaceSound {
        asset: asset.clone(),
    };
    let forward = SemanticInstruction::MoveFrames {
        forward: true,
        count: NonZeroU32::new(20).unwrap(),
    };
    let planned = plan(
        &document,
        context("root", 5),
        vec![place.clone(), forward, place.clone()],
        &BTreeMap::new(),
    )
    .unwrap();
    let rate = document.presentation_basis().frame_rate;
    let mut onsets: Vec<_> = planned
        .document
        .sounds()
        .values()
        .map(|event| event.offset)
        .collect();
    onsets.sort();
    assert_eq!(
        onsets,
        [
            rate.audio_boundary(ProjectFrame(5)).unwrap(),
            rate.audio_boundary(ProjectFrame(25)).unwrap()
        ]
    );
    assert!(
        planned
            .document
            .sounds()
            .values()
            .all(|event| event.source.asset == asset && event.owner == node("root"))
    );
    // Too close to the end for the whole sound: refused.
    assert!(
        plan(
            &document,
            context("root", 59),
            vec![place],
            &BTreeMap::new()
        )
        .is_err()
    );
}
