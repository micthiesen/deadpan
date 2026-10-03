use super::*;

fn selected(parent: &str, cursor: i64, child: &str) -> SemanticContext {
    SemanticContext {
        selected_child: Some(node(child)),
        ..context(parent, cursor)
    }
}
fn yank(register: char) -> SemanticInstruction {
    SemanticInstruction::YankBeat {
        register: name(register),
    }
}
fn paste(register: char, before: bool) -> SemanticInstruction {
    SemanticInstruction::Paste {
        register: name(register),
        before,
    }
}
fn edited(planned: &SemanticPlan, register: char) -> &CapturedEditSlice {
    let RegisterValue::Edited { slice } = planned.register_writes[&name(register)].as_ref() else {
        panic!()
    };
    slice
}
fn children(document: &ProjectDocument, parent: &str) -> Vec<NodeId> {
    let NodeKind::Sequence { children } = &document.nodes()[&node(parent)].kind else {
        panic!()
    };
    children.clone()
}

#[test]
fn yank_uses_explicit_selected_empty_sibling_and_keeps_cursor_and_revision() {
    let document = tree(
        &["first", "second", "held"],
        vec![
            ("first", BeatNode::sequence("First empty", vec![])),
            ("second", BeatNode::sequence("Second empty", vec![])),
            ("held", hold(5)),
        ],
    );
    let entry = selected("root", 4, "second");
    let planned = plan(&document, entry.clone(), vec![yank('a')], &BTreeMap::new()).unwrap();
    assert_eq!(planned.context, entry);
    assert_eq!(planned.selected_child, Some(node("second")));
    assert_eq!(planned.document, document);
    assert_eq!(
        planned.trace[0].captured_child_label.as_deref(),
        Some("Second empty")
    );
    assert_eq!(planned.trace[0].resolved_range, Some(range(0, 0)));
    let slice = edited(&planned, 'a');
    assert_eq!(
        slice.selection(),
        &SliceCaptureSelection::Child {
            node: node("second")
        }
    );
    assert_eq!(slice.revision_id(), document.revision_id());
    slice.validate_capture(&document).unwrap();
    assert_eq!(
        planned.register_writes[&name('a')],
        planned.register_writes[&name('"')]
    );
    let Command::Compound { transaction } = &planned.request.as_ref().unwrap().command else {
        panic!()
    };
    assert_eq!(transaction.steps().len(), 1);
    assert!(transaction.steps()[0].edit().is_none());
    let replayed =
        crate::replay_compound::<EditError>(&document, planned.request.as_ref().unwrap(), |_| {
            Ok(())
        })
        .unwrap();
    assert_eq!(replayed.register_writes, planned.register_writes);
    assert_eq!(
        replayed.edit.inverse.apply(&replayed.document).unwrap(),
        document
    );

    for entry in [
        context("root", 1),
        selected("root", 1, "missing"),
        selected("root", 1, "root"),
    ] {
        let error = plan(&document, entry, vec![yank('a')], &BTreeMap::new()).unwrap_err();
        assert_eq!(error.code, EditErrorCode::SelectionUnavailable);
    }
}

#[test]
fn frame_motion_reselects_even_when_clamped_but_yank_never_infers_selection() {
    let document = tree(
        &["a", "empty", "b"],
        vec![
            ("a", hold(2)),
            ("empty", BeatNode::sequence("Empty", vec![])),
            ("b", hold(3)),
        ],
    );
    for (entry, instruction, expected) in [
        (selected("root", 0, "empty"), motion(false, 1), "a"),
        (selected("root", 5, "empty"), motion(true, 1), "b"),
        (context("root", 1), motion(true, 1), "b"),
    ] {
        let planned = plan(
            &document,
            entry,
            vec![instruction, yank('a')],
            &BTreeMap::new(),
        )
        .unwrap();
        assert_eq!(planned.context.selected_child, Some(node(expected)));
        assert_eq!(
            edited(&planned, 'a').selection(),
            &SliceCaptureSelection::Child {
                node: node(expected)
            }
        );
    }
    let absent = plan(
        &document,
        context("root", 1),
        vec![call('m', 1)],
        &BTreeMap::from([(name('m'), macro_value(vec![yank('a')]))]),
    )
    .unwrap_err();
    assert_eq!(absent.code, EditErrorCode::SelectionUnavailable);
}

#[test]
fn clamped_motion_selects_last_empty_child_at_terminal_and_all_empty_boundaries() {
    for (document, cursor) in [
        (
            tree(
                &["held", "first", "last"],
                vec![
                    ("held", hold(2)),
                    ("first", BeatNode::sequence("First empty", vec![])),
                    ("last", BeatNode::sequence("Last empty", vec![])),
                ],
            ),
            2,
        ),
        (
            tree(
                &["first", "last"],
                vec![
                    ("first", BeatNode::sequence("First empty", vec![])),
                    ("last", BeatNode::sequence("Last empty", vec![])),
                ],
            ),
            0,
        ),
    ] {
        let planned = plan(
            &document,
            selected("root", cursor, "first"),
            vec![motion(true, 1), yank('a')],
            &BTreeMap::new(),
        )
        .unwrap();
        assert_eq!(planned.context.cursor, ProjectFrame(cursor));
        assert_eq!(planned.context.selected_child, Some(node("last")));
        assert_eq!(planned.document, document);
        assert_eq!(
            edited(&planned, 'a').selection(),
            &SliceCaptureSelection::Child { node: node("last") }
        );
        assert_eq!(planned.trace[1].resolved_range, Some(range(cursor, cursor)));
        assert_eq!(
            planned.trace[1].captured_child_label.as_deref(),
            Some("Last empty")
        );
    }
}

#[test]
fn paste_uses_selected_beat_instead_of_cursor_and_keeps_empty_import_selected() {
    let document = tree(
        &["a", "first", "second", "b"],
        vec![
            ("a", hold(2)),
            ("first", BeatNode::sequence("First", vec![])),
            ("second", BeatNode::sequence("Second", vec![])),
            ("b", hold(3)),
        ],
    );
    for before in [true, false] {
        let planned = plan(
            &document,
            selected("root", 4, "second"),
            vec![yank('a'), paste('a', before)],
            &BTreeMap::new(),
        )
        .unwrap();
        let rows = children(&planned.document, "root");
        let index = if before { 2 } else { 3 };
        assert_eq!(rows[index], node("paste-1-0"));
        assert_eq!(planned.context.selected_child, Some(rows[index].clone()));
        assert_eq!(planned.selected_child, planned.context.selected_child);
        assert_eq!(planned.context.cursor, ProjectFrame(2));
        assert_eq!(planned.trace[1].resolved_range, Some(range(2, 2)));
        assert_eq!(
            planned.document.duration().unwrap(),
            document.duration().unwrap()
        );
        assert_eq!(planned.document.audio_bindings(), document.audio_bindings());
        let replayed = crate::replay_compound::<EditError>(
            &document,
            planned.request.as_ref().unwrap(),
            |_| Ok(()),
        )
        .unwrap();
        assert_eq!(replayed.document, planned.document);
        assert_eq!(
            replayed.edit.inverse.apply(&replayed.document).unwrap(),
            document
        );
    }
}

#[test]
fn empty_sequence_accepts_paste_without_selection_but_nonempty_sequence_requires_one() {
    let source = fixture(3);
    let copied = Arc::new(RegisterValue::Edited {
        slice: Arc::new(
            CapturedEditSlice::capture_selection(
                &source,
                source.root(),
                &SliceCaptureSelection::Child { node: node("held") },
                AudioTimingId {
                    allocation: revision("prior-copy"),
                    ordinal: 0,
                },
            )
            .unwrap(),
        ),
    });
    let bank = BTreeMap::from([(name('a'), copied)]);
    let empty = tree(&[], vec![]);
    let planned = plan(&empty, context("root", 0), vec![paste('a', false)], &bank).unwrap();
    assert_eq!(planned.context.cursor, ProjectFrame(0));
    assert_eq!(planned.context.selected_child, Some(node("paste-0-0")));
    assert_eq!(planned.document.duration().unwrap().frames(), 3);
    let error = plan(&source, context("root", 0), vec![paste('a', true)], &bank).unwrap_err();
    assert_eq!(error.code, EditErrorCode::SelectionUnavailable);
}

#[test]
fn counted_nested_body_freezes_macro_while_yanks_capture_each_staged_paste() {
    let document = tree(
        &["prefix", "group", "tail"],
        vec![
            ("prefix", hold(2)),
            ("group", BeatNode::sequence("Group", vec![node("held")])),
            ("held", hold(3)),
            ("tail", hold(4)),
        ],
    );
    let bank = BTreeMap::from([
        (name('a'), macro_value(vec![yank('a'), paste('a', false)])),
        (name('b'), macro_value(vec![call('a', 2)])),
    ]);
    let entry = selected("group", 5, "held");
    let planned = plan(&document, entry.clone(), vec![call('b', 1)], &bank).unwrap();
    assert_eq!(planned.document.duration().unwrap().frames(), 15);
    assert_eq!(planned.context.cursor, ProjectFrame(8));
    assert_eq!(planned.context.selected_child, Some(node("paste-3-0")));
    assert_eq!(
        planned.document.nodes()[&node("prefix")],
        document.nodes()[&node("prefix")]
    );
    assert_eq!(
        planned.document.nodes()[&node("tail")],
        document.nodes()[&node("tail")]
    );
    assert_eq!(edited(&planned, 'a').revision_id(), &revision("leaf-1"));
    assert_eq!(planned.trace[4].before_revision, revision("leaf-1"));
    assert_eq!(planned.trace[4].before_scope, range(2, 8));
    let replayed =
        crate::replay_compound::<EditError>(&document, planned.request.as_ref().unwrap(), |_| {
            Ok(())
        })
        .unwrap();
    assert_eq!(replayed.document, planned.document);
    assert_eq!(
        replayed.edit.inverse.apply(&replayed.document).unwrap(),
        document
    );
    let original = document.clone();
    let original_bank = bank.clone();
    let error = plan(&document, entry, vec![call('b', 1), call('a', 1)], &bank).unwrap_err();
    assert!(error.message.contains("copied content"));
    assert_eq!(document, original);
    assert_eq!(bank, original_bank);
}

#[test]
fn yank_and_paste_require_matching_exact_fresh_allocations() {
    let document = fixture(3);
    for failure in 0..5 {
        let error = plan_semantic(
            &document,
            &selected("root", 0, "held"),
            &program(vec![yank('a'), paste('a', false), paste('a', false)]),
            SemanticRegisterBank {
                entries: &BTreeMap::new(),
                version: 0,
            },
            revision("outer"),
            |request| {
                let mut allocation = allocate(request)?;
                match (&mut allocation, failure) {
                    (SemanticAllocation::Yank { capture_revision }, 0) => {
                        *capture_revision = revision("outer")
                    }
                    (SemanticAllocation::Yank { .. }, 1) => {
                        return Ok(SemanticAllocation::PasteOriginal {
                            new_revision: revision("wrong"),
                            node: node("wrong"),
                            split_identities: SplitIdentities::default(),
                        });
                    }
                    (SemanticAllocation::PasteEdited { identities, .. }, 2) => {
                        identities.aliases.push(node("extra"));
                    }
                    (SemanticAllocation::PasteEdited { identities, .. }, 3) => {
                        identities.authored.nodes[0] = node("held")
                    }
                    (SemanticAllocation::PasteEdited { new_revision, .. }, 4) => {
                        *new_revision = revision("same-paste")
                    }
                    _ => {}
                }
                Ok(allocation)
            },
            no_original,
        )
        .unwrap_err();
        assert!(matches!(
            error.code,
            EditErrorCode::IdentityConflict | EditErrorCode::InvalidCommand
        ));
    }
    for instructions in [vec![paste('m', false)], vec![yank('a'), paste('z', false)]] {
        let original = document.clone();
        let error = plan(
            &document,
            selected("root", 0, "held"),
            instructions,
            &BTreeMap::from([(name('m'), macro_value(vec![motion(true, 1)]))]),
        )
        .unwrap_err();
        assert_eq!(error.code, EditErrorCode::InvalidCommand);
        assert_eq!(document, original);
    }
}

#[test]
fn yank_only_calls_count_toward_resolved_steps_before_allocation() {
    let document = fixture(3);
    let error = plan_semantic(
        &document,
        &selected("root", 0, "held"),
        &program(vec![call('a', 1025)]),
        SemanticRegisterBank {
            entries: &BTreeMap::from([(name('a'), macro_value(vec![yank('b')]))]),
            version: 0,
        },
        revision("outer"),
        |_| panic!("count must fail before allocation"),
        no_original,
    )
    .unwrap_err();
    assert_eq!(error.code, EditErrorCode::LimitExceeded);
}

#[test]
fn new_content_instructions_are_closed_and_round_trip() {
    let value = program(vec![yank('a'), paste('a', true), paste('"', false)]);
    assert_eq!(
        serde_json::from_str::<SemanticProgram>(&serde_json::to_string(&value).unwrap()).unwrap(),
        value
    );
    for invalid in [
        r#"{"instructions":[{"type":"yank_beat","register":"a","node":"fixed"}]}"#,
        r#"{"instructions":[{"type":"paste","register":"a","before":true,"index":0}]}"#,
        r#"{"instructions":[{"type":"paste","register":"a"}]}"#,
    ] {
        assert!(serde_json::from_str::<SemanticProgram>(invalid).is_err());
    }
}

pub(super) fn original_fixture() -> (ProjectDocument, Arc<RegisterValue>, SourceNode) {
    use crate::{
        AssetId, AssetRecord, AudioSample, LinkRelation, SourceAudioMapping, SourceQualificationId,
        SourceSpan, SourceTimeBase, SourceTimestamp, SourceVideo, SourceVideoMapping,
    };
    let mut document = fixture(4);
    let asset = AssetId::new("original").unwrap();
    let qualification = SourceQualificationId::new("b".repeat(64)).unwrap();
    let time_base = SourceTimeBase::new(1, 30).unwrap();
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base,
        },
        SourceTimestamp {
            ticks: 30,
            time_base,
        },
    )
    .unwrap();
    document.assets.insert(
        asset.clone(),
        AssetRecord {
            label: "Original".into(),
            content_hash: "a".repeat(64),
            video: Some(span),
            audio: None,
            still_image: false,
            frame_count: Some(FrameDuration::new(30).unwrap()),
            source_qualification: Some(qualification.clone()),
        },
    );
    document.validate().unwrap();
    let value = Arc::new(RegisterValue::Original {
        revision: document.revision_id().clone(),
        asset: asset.clone(),
        qualification,
        ordinals: 0..30,
    });
    let source = SourceNode {
        duration: FrameDuration::new(30).unwrap(),
        edit_window: None,
        video: SourceVideo::Stream { asset, span },
        video_mapping: SourceVideoMapping::FitBeat,
        audio: None,
        audio_mapping: SourceAudioMapping::FitBeat,
        link: LinkRelation::Independent,
        audio_offset: AudioSample(0),
    };
    (document, value, source)
}

#[test]
fn original_resolver_observes_each_staged_document_and_core_checks_register_type_binding() {
    let (document, value, source) = original_fixture();
    let mut observed = Vec::new();
    let bank = BTreeMap::from([(name('a'), value.clone())]);
    let planned = plan_semantic(
        &document,
        &selected("root", 0, "held"),
        &program(vec![paste('a', false), paste('a', false), yank('b')]),
        SemanticRegisterBank {
            entries: &bank,
            version: 7,
        },
        revision("outer"),
        allocate,
        |staged, selected| {
            assert_eq!(selected, value.as_ref());
            observed.push((
                staged.revision_id().clone(),
                staged.duration().unwrap().frames(),
            ));
            Ok(source.clone())
        },
    )
    .unwrap();
    assert_eq!(
        observed,
        vec![(revision("base"), 4), (revision("leaf-0"), 34)]
    );
    assert_eq!(planned.document.duration().unwrap().frames(), 64);
    assert_eq!(planned.context.cursor, ProjectFrame(34));
    assert_eq!(planned.context.selected_child, Some(node("original-1")));
    assert_eq!(edited(&planned, 'b').revision_id(), &revision("leaf-1"));
    assert_eq!(
        planned.trace[2].captured_child_label.as_deref(),
        Some("Original [0..30)")
    );
    let replayed =
        crate::replay_compound::<EditError>(&document, planned.request.as_ref().unwrap(), |_| {
            Ok(())
        })
        .unwrap();
    assert_eq!(replayed.document, planned.document);
    assert_eq!(
        replayed.edit.inverse.apply(&replayed.document).unwrap(),
        document
    );

    let error = plan_semantic(
        &document,
        &selected("root", 0, "held"),
        &program(vec![yank('b'), paste('a', false)]),
        SemanticRegisterBank {
            entries: &bank,
            version: 7,
        },
        revision("outer"),
        allocate,
        |_, _| {
            let mut invalid = source.clone();
            invalid.video = crate::SourceVideo::Blank;
            Ok(invalid)
        },
    )
    .unwrap_err();
    assert_eq!(error.code, EditErrorCode::InvalidCommand);
    assert!(error.message.contains("selected asset"));
    assert_eq!(document.duration().unwrap().frames(), 4);
    assert_eq!(bank[&name('a')], value);
}

#[test]
fn original_resolution_failure_after_an_edit_is_atomic() {
    let (document, value, source) = original_fixture();
    let bank = BTreeMap::from([(name('a'), value)]);
    let before = document.clone();
    let mut calls = 0;
    let error = plan_semantic(
        &document,
        &selected("root", 0, "held"),
        &program(vec![paste('a', false), paste('a', false)]),
        SemanticRegisterBank {
            entries: &bank,
            version: 7,
        },
        revision("outer"),
        allocate,
        |_, _| {
            calls += 1;
            if calls == 2 {
                return Err(invalid("qualified Original resolution failed"));
            }
            Ok(source.clone())
        },
    )
    .unwrap_err();
    assert_eq!(calls, 2);
    assert_eq!(error.message, "qualified Original resolution failed");
    assert_eq!(document, before);
}
