use super::*;

fn setter(plays: u32) -> SemanticInstruction {
    SemanticInstruction::SetRepeatPlays {
        plays: NonZeroU32::new(plays).unwrap(),
    }
}

fn repeated(child: &str, plays: u32, gap: Option<i64>) -> BeatNode {
    let mut beat = BeatNode::sequence("Repeat", Vec::new());
    beat.kind = NodeKind::Repeat {
        child: node(child),
        iterations: crate::IterationOrder::new(revision("old-plays"), plays).unwrap(),
        gap: gap.map(|frames| {
            let NodeKind::Hold { recipe } = hold(frames).kind else {
                unreachable!()
            };
            recipe
        }),
        escalation: None,
    };
    beat
}

fn fixture() -> ProjectDocument {
    tree(
        &["prefix", "repeated", "suffix"],
        vec![
            ("prefix", hold(2)),
            ("repeated", repeated("body", 3, Some(1))),
            ("body", hold(3)),
            ("suffix", hold(5)),
        ],
    )
}

fn entry(cursor: i64) -> SemanticContext {
    SemanticContext {
        selected_child: Some(node("repeated")),
        ..context("root", cursor)
    }
}

fn order(document: &ProjectDocument, name: &str) -> crate::IterationOrder {
    let NodeKind::Repeat { iterations, .. } = &document.nodes()[&node(name)].kind else {
        panic!("expected Repeat")
    };
    iterations.clone()
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
    assert!(planned.register_writes.is_empty());
}

#[test]
fn setter_uses_selected_repeat_independently_of_cursor_and_retains_surviving_plays() {
    let document = fixture();
    let original = document.clone();
    let before_order = order(&document, "repeated");
    let planned = plan(&document, entry(18), vec![setter(2)], &BTreeMap::new()).unwrap();
    assert_eq!(document, original);
    assert_eq!(planned.document.duration().unwrap().frames(), 14);
    assert_eq!(planned.context.cursor, ProjectFrame(2));
    assert_eq!(planned.context.selected_child, Some(node("repeated")));
    assert_eq!(planned.context.visual_selection, None);
    assert_eq!(planned.document.nodes().len(), document.nodes().len());
    let after_order = order(&planned.document, "repeated");
    assert_eq!(after_order.len(), 2);
    assert_eq!(after_order.at(0), before_order.at(0));
    assert_eq!(after_order.at(1), before_order.at(1));
    assert_eq!(planned.trace[0].before.cursor, ProjectFrame(18));
    assert_eq!(planned.trace[0].before_revision, revision("base"));
    assert_eq!(planned.trace[0].before_scope, range(0, 18));
    assert_eq!(planned.trace[0].resolved_range, Some(range(2, 13)));
    assert_eq!(
        planned.trace[0].resolved_selection,
        Some(SliceCaptureSelection::Child {
            node: node("repeated")
        })
    );
    assert_eq!(planned.trace[0].instruction, setter(2));
    assert_eq!(planned.trace[0].captured_child_label, None);
    assert_eq!(planned.trace[0].removed_range, None);
    inverse(&document, &planned);
}

#[test]
fn wrap_then_set_then_move_resolves_each_new_staged_repeat_in_one_compound() {
    let document = tree(
        &["a", "existing"],
        vec![
            ("a", hold(3)),
            ("existing", repeated("b", 2, None)),
            ("b", hold(2)),
        ],
    );
    let start = SemanticContext {
        selected_child: Some(node("a")),
        ..context("root", 7)
    };
    let bank = BTreeMap::from([(name('a'), macro_value(vec![setter(4)]))]);
    let planned = plan(
        &document,
        start,
        vec![
            SemanticInstruction::Repeat {
                selector: SemanticSelector::SelectedBeat,
                plays: NonZeroU32::new(2).unwrap(),
                escalation: None,
            },
            call('a', 1),
            SemanticInstruction::MoveBeats {
                forward: true,
                count: NonZeroU32::new(1).unwrap(),
            },
            setter(3),
        ],
        &bank,
    )
    .unwrap();
    let request = planned.request.as_ref().unwrap();
    let Command::Compound { transaction } = &request.command else {
        panic!()
    };
    assert_eq!(transaction.steps().len(), 3);
    assert_eq!(order(&planned.document, "repeat-0").len(), 4);
    assert_eq!(order(&planned.document, "existing").len(), 3);
    assert_eq!(planned.context.selected_child, Some(node("existing")));
    assert_eq!(planned.context.cursor, ProjectFrame(12));
    assert_eq!(planned.document.duration().unwrap().frames(), 18);
    assert_eq!(planned.trace[2].before_revision, revision("leaf-0"));
    assert_eq!(planned.trace[2].resolved_range, Some(range(0, 6)));
    assert_eq!(planned.trace[4].before_revision, revision("leaf-1"));
    assert_eq!(planned.trace[4].resolved_range, Some(range(12, 16)));
    assert_eq!(planned.trace[4].before_scope, range(0, 16));
    let newly_added = order(&planned.document, "repeat-0").at(2).unwrap();
    assert_eq!(newly_added.allocation, revision("leaf-1"));
    assert_eq!(
        order(&planned.document, "existing")
            .at(2)
            .unwrap()
            .allocation,
        revision("leaf-2")
    );
    inverse(&document, &planned);
}

#[test]
fn unchanged_count_still_uses_fresh_leaf_revisions_and_bounded_counted_calls() {
    let document = fixture();
    let bank = BTreeMap::from([
        (name('a'), macro_value(vec![setter(3)])),
        (name('b'), macro_value(vec![call('a', 2)])),
    ]);
    let mut requests = Vec::new();
    let planned = plan_semantic(
        &document,
        &entry(17),
        &program(vec![call('b', 2)]),
        SemanticRegisterBank {
            entries: &bank,
            version: 7,
        },
        revision("outer"),
        |request| {
            requests.push(request);
            allocate(request)
        },
        no_original,
    )
    .unwrap();
    assert_eq!(
        requests,
        (0..4)
            .map(|step_index| SemanticAllocationRequest::SetRepeatPlays { step_index })
            .collect::<Vec<_>>()
    );
    let mut expected = document.clone();
    expected.revision_id = revision("outer");
    assert_eq!(planned.document, expected);
    assert_eq!(planned.context.cursor, ProjectFrame(2));
    let Command::Compound { transaction } = &planned.request.as_ref().unwrap().command else {
        panic!()
    };
    for (index, step) in transaction.steps().iter().enumerate() {
        let edit = step.edit().unwrap();
        let request = edit.request(&document);
        assert_eq!(request.new_revision, revision(&format!("leaf-{index}")));
        assert!(
            matches!(request.command, Command::SetRepeatPlays { ref node, plays: 3, ref timing }
            if node == &self::node("repeated") && timing.allocation == request.new_revision && timing.ordinal == 0)
        );
    }
    inverse(&document, &planned);
}

#[test]
fn setter_matches_ordinary_retained_clock_edit_in_nested_scope_with_root_sound() {
    // Asserts the authored reference representation (every reanchor step
    // and complete timing tables). tests/timing_representation.rs proves the
    // compact storage resolves and renders identically.
    crate::with_reference_timing_representation(|| {
        let mut document = tree(
            &["prefix", "group", "suffix"],
            vec![
                ("prefix", hold(2)),
                ("group", BeatNode::sequence("Group", vec![node("repeated")])),
                ("repeated", repeated("body", 3, Some(1))),
                ("body", hold(3)),
                ("suffix", hold(20)),
            ],
        );
        let span = crate::SourceSpan::new(
            crate::SourceTimestamp {
                ticks: 0,
                time_base: crate::SourceTimeBase::new(1, 48_000).unwrap(),
            },
            crate::SourceTimestamp {
                ticks: 4_800,
                time_base: crate::SourceTimeBase::new(1, 48_000).unwrap(),
            },
        )
        .unwrap();
        let asset = crate::AssetId::new("sound").unwrap();
        document.assets.insert(
            asset.clone(),
            crate::AssetRecord {
                label: "Sound".into(),
                content_hash: "a".repeat(64),
                video: None,
                audio: Some(span),
                still_image: false,
                frame_count: None,
                source_qualification: Some(
                    crate::SourceQualificationId::new("b".repeat(64)).unwrap(),
                ),
            },
        );
        let sound = crate::SoundId::new("sound").unwrap();
        document.sounds.insert(
            sound.clone(),
            crate::SoundEvent {
                owner: node("root"),
                label: "Sound".into(),
                source: crate::SourceAudio { asset, span },
                mapping: crate::SourceAudioMapping::natural_rate(
                    span,
                    document.presentation_basis().frame_rate,
                )
                .unwrap(),
                offset: crate::AudioSample(137),
                gain_millidecibels: 0,
                start_edge: crate::AudioEdgePolicy::Automatic,
                end_edge: crate::AudioEdgePolicy::Automatic,
                overflow: crate::SoundOverflowPolicy::Reject,
            },
        );
        document.validate().unwrap();
        for plays in [2, 4] {
            let target = SemanticContext {
                parent: node("group"),
                ..entry(13)
            };
            let planned = plan(&document, target, vec![setter(plays)], &BTreeMap::new()).unwrap();
            let request = CommandRequest {
                project_id: document.project_id().clone(),
                expected_revision: revision("base"),
                new_revision: revision("leaf-0"),
                command: Command::SetRepeatPlays {
                    node: node("repeated"),
                    plays,
                    timing: AudioTimingId {
                        allocation: revision("leaf-0"),
                        ordinal: 0,
                    },
                },
            };
            let mut expected = crate::apply(&document, &request)
                .unwrap()
                .forward
                .apply(&document)
                .unwrap();
            expected.revision_id = revision("outer");
            assert_eq!(planned.document, expected);
            assert_eq!(planned.document.sounds(), document.sounds());
            assert_eq!(planned.document.sound_routes()[&sound].edits.len(), 1);
            assert_eq!(
                planned.document.audio_bindings().bindings()[&node("suffix")]
                    .reanchors
                    .len(),
                1
            );
            assert_eq!(planned.trace[0].before_scope, range(2, 13));
            assert_eq!(planned.context.parent, node("group"));
            inverse(&document, &planned);
        }
    })
}

#[test]
fn setter_refuses_visual_absence_wrong_kind_and_stale_selection_before_allocation() {
    let document = fixture();
    let mut invalid = vec![
        (context("root", 2), EditErrorCode::SelectionUnavailable),
        (
            SemanticContext {
                selected_child: Some(node("suffix")),
                ..context("root", 2)
            },
            EditErrorCode::WrongNodeKind,
        ),
        (
            SemanticContext {
                selected_child: Some(node("body")),
                ..context("root", 2)
            },
            EditErrorCode::SelectionUnavailable,
        ),
    ];
    for (anchor, head, extending) in [(2, 2, true), (2, 7, true), (7, 2, false)] {
        invalid.push((
            SemanticContext {
                visual_selection: Some(SemanticVisualSelection::Time {
                    anchor: ProjectFrame(anchor),
                    head: ProjectFrame(head),
                    extending,
                }),
                ..entry(head)
            },
            EditErrorCode::InvalidCommand,
        ));
    }
    for (context, expected) in invalid {
        let result = plan_semantic(
            &document,
            &context,
            &program(vec![setter(2)]),
            SemanticRegisterBank {
                entries: &BTreeMap::new(),
                version: 0,
            },
            revision("outer"),
            |_| panic!("invalid target must refuse before allocation"),
            no_original,
        );
        assert_eq!(result.unwrap_err().code, expected);
    }
}

#[test]
fn setter_checks_allocation_variant_freshness_and_late_failure_atomically() {
    let document = fixture();
    let before = document.clone();
    for (allocation, expected) in [
        (
            SemanticAllocation::Yank {
                capture_revision: revision("capture"),
            },
            EditErrorCode::InvalidCommand,
        ),
        (
            SemanticAllocation::SetRepeatPlays {
                new_revision: revision("outer"),
            },
            EditErrorCode::IdentityConflict,
        ),
        (
            SemanticAllocation::SetRepeatPlays {
                new_revision: revision("base"),
            },
            EditErrorCode::IdentityConflict,
        ),
        (
            SemanticAllocation::SetRepeatPlays {
                new_revision: revision("old-plays"),
            },
            EditErrorCode::IdentityConflict,
        ),
    ] {
        let error = plan_semantic(
            &document,
            &entry(3),
            &program(vec![setter(2)]),
            SemanticRegisterBank {
                entries: &BTreeMap::new(),
                version: 0,
            },
            revision("outer"),
            |_| Ok(allocation.clone()),
            no_original,
        )
        .unwrap_err();
        assert_eq!(error.code, expected);
    }
    let reused = plan_semantic(
        &document,
        &entry(3),
        &program(vec![setter(2), setter(4)]),
        SemanticRegisterBank {
            entries: &BTreeMap::new(),
            version: 0,
        },
        revision("outer"),
        |_| {
            Ok(SemanticAllocation::SetRepeatPlays {
                new_revision: revision("duplicate"),
            })
        },
        no_original,
    )
    .unwrap_err();
    assert_eq!(reused.code, EditErrorCode::IdentityConflict);
    let bank = BTreeMap::from([(name('a'), macro_value(vec![setter(2), call('z', 1)]))]);
    let before_bank = bank.clone();
    assert!(plan(&document, entry(3), vec![call('a', 1)], &bank).is_err());
    assert_eq!(document, before);
    assert_eq!(bank, before_bank);
}

#[test]
fn setter_wire_and_counted_step_limits_refuse_without_expanding_work() {
    let instruction = setter(u32::MAX);
    let wire = serde_json::to_string(&instruction).unwrap();
    assert_eq!(
        serde_json::from_str::<SemanticInstruction>(&wire).unwrap(),
        instruction
    );
    for wire in [
        r#"{"type":"set_repeat_plays","plays":0}"#,
        r#"{"type":"set_repeat_plays","plays":-1}"#,
        r#"{"type":"set_repeat_plays","plays":4294967296}"#,
        r#"{"type":"set_repeat_plays","plays":2,"node":"repeated"}"#,
        r#"{"type":"set_repeat_plays","plays":2,"selector":{"type":"selected_beat"}}"#,
    ] {
        assert!(serde_json::from_str::<SemanticInstruction>(wire).is_err());
    }
    let document = fixture();
    let bank = BTreeMap::from([(name('a'), macro_value(vec![setter(3)]))]);
    for count in [1025, u32::MAX] {
        let result = plan_semantic(
            &document,
            &entry(3),
            &program(vec![call('a', count)]),
            SemanticRegisterBank {
                entries: &bank,
                version: 0,
            },
            revision("outer"),
            |_| panic!("count must refuse before first allocation"),
            no_original,
        );
        assert_eq!(result.unwrap_err().code, EditErrorCode::LimitExceeded);
    }
}

#[test]
fn huge_count_stays_compact_and_regrowth_does_not_reuse_retired_play_ids() {
    let document = tree(
        &["repeated"],
        vec![("repeated", repeated("body", 1, None)), ("body", hold(1))],
    );
    let original = order(&document, "repeated").at(0).unwrap();
    let planned = plan(
        &document,
        entry(1),
        vec![setter(u32::MAX), setter(1), setter(2)],
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(planned.document.nodes().len(), document.nodes().len());
    assert_eq!(planned.document.duration().unwrap().frames(), 2);
    let order = order(&planned.document, "repeated");
    assert_eq!(order.len(), 2);
    assert_eq!(order.at(0), Some(original));
    assert_eq!(order.at(1).unwrap().allocation, revision("leaf-2"));
    assert_eq!(order.segment_count(), 2);
    assert_eq!(
        planned.trace[1].resolved_range,
        Some(range(0, i64::from(u32::MAX)))
    );
    assert_eq!(planned.context.cursor, ProjectFrame(0));
    inverse(&document, &planned);
}

#[test]
fn escalation_is_one_reversible_parameter_change_checked_against_play_count() {
    let document = fixture();
    let request = |command| CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision("escalated"),
        command,
    };
    let escalation = crate::RepeatEscalation {
        gain_step: crate::GainDb::new(8_000).unwrap(),
        zoom: None,
    };
    let edit = crate::apply(
        &document,
        &request(Command::SetRepeatEscalation {
            node: node("repeated"),
            escalation: Some(escalation),
        }),
    )
    .unwrap();
    let escalated = edit.forward.apply(&document).unwrap();
    let NodeKind::Repeat {
        escalation: stored, ..
    } = &escalated.nodes()[&node("repeated")].kind
    else {
        panic!("expected Repeat")
    };
    assert_eq!(*stored, Some(escalation));
    assert_eq!(
        escalated.duration(),
        document.duration(),
        "timing unchanged"
    );
    assert_eq!(edit.duration_delta, 0);
    let mut restored = edit.inverse.apply(&escalated).unwrap();
    restored.revision_id = document.revision_id().clone();
    assert_eq!(restored, document);

    // +8 dB per play reaches +16 dB on the third play; a fifth would exceed +24.
    let grow = |plays| CommandRequest {
        project_id: escalated.project_id().clone(),
        expected_revision: escalated.revision_id().clone(),
        new_revision: revision("grown"),
        command: Command::SetRepeatPlays {
            node: node("repeated"),
            plays,
            timing: AudioTimingId {
                allocation: revision("grown"),
                ordinal: 0,
            },
        },
    };
    assert!(crate::apply(&escalated, &grow(4)).is_ok());
    let refused = crate::apply(&escalated, &grow(5)).unwrap_err();
    assert!(refused.message.contains("escalation"), "{refused:?}");

    let wrong = crate::apply(
        &document,
        &request(Command::SetRepeatEscalation {
            node: node("prefix"),
            escalation: Some(escalation),
        }),
    )
    .unwrap_err();
    assert_eq!(wrong.code, EditErrorCode::WrongNodeKind);
}

#[test]
fn cutaways_belong_only_to_source_and_hold_beats() {
    let document = fixture();
    let base = crate::SourceTimeBase::new(1, 30).unwrap();
    let stamp = |ticks| crate::SourceTimestamp {
        ticks,
        time_base: base,
    };
    let span = crate::SourceSpan::new(stamp(0), stamp(30)).unwrap();
    let mut wire = serde_json::to_value(&document).unwrap();
    wire["assets"]["clip"] = serde_json::to_value(crate::AssetRecord {
        label: "Clip".into(),
        content_hash: "a".repeat(64),
        video: Some(span),
        audio: None,
        still_image: false,
        frame_count: Some(crate::FrameDuration::new(30).unwrap()),
        source_qualification: None,
    })
    .unwrap();
    let cutaway = serde_json::to_value(vec![crate::Cutaway {
        range: range(0, 2),
        asset: crate::AssetId::new("clip").unwrap(),
        selection: crate::ExactSourceSpan::from(
            crate::SourceSpan::new(stamp(0), stamp(2)).unwrap(),
        ),
        fit: crate::CutawayFit::Hold,
        removed: false,
    }])
    .unwrap();
    let with = |node: &str| {
        let mut wire = wire.clone();
        wire["nodes"][node]["cutaways"] = cutaway.clone();
        ProjectDocument::from_json(&wire.to_string())
    };
    with("prefix").expect("a Hold hosts a cutaway");
    let refused = with("repeated").unwrap_err();
    assert!(refused.message.contains("Source or Hold"), "{refused:?}");
}
