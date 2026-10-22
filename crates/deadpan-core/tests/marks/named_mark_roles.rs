//! Named-range composition with existing one-child role operators.
use super::*;

// Draft append block for crates/deadpan-core/tests/marks/named_mark_selectors.rs.
// Focus: composing a named Edit range with existing single-child role consumers.
fn role_named_fixture(two_sources: bool) -> ProjectDocument {
    let asset_id = AssetId::new("named-role-original").unwrap();
    let qualified = edit(
        &empty(),
        Command::AddAsset {
            id: asset_id.clone(),
            asset: AssetRecord {
                source_qualification: Some(SourceQualificationId::new("c".repeat(64)).unwrap()),
                label: "Synthetic linked A/V Original".into(),
                content_hash: "d".repeat(64),
                video: Some(source_span(0, 32)),
                audio: Some(source_span(0, 32)),
                still_image: false,
                frame_count: Some(duration(32)),
            },
        },
    )
    .0;
    let linked_source = |label: &str, start: i64| {
        let mut beat = source_beat(label, start);
        let NodeKind::Source { source } = &mut beat.kind else {
            unreachable!()
        };
        if let SourceVideo::Stream { asset, .. } = &mut source.video {
            *asset = asset_id.clone();
        }
        source.audio = Some(SourceAudio {
            asset: asset_id.clone(),
            span: source_span(start, start + 8),
        });
        source.audio_mapping = SourceAudioMapping::Duration {
            frames: ExactRatio::integer(8),
        };
        source.link = LinkRelation::Linked;
        beat
    };
    let mut nodes = BTreeMap::from([
        (
            node("group"),
            BeatNode::sequence(
                "Group",
                if two_sources {
                    vec![node("first"), node("second")]
                } else {
                    vec![node("first")]
                },
            ),
        ),
        (node("first"), linked_source("First source", 0)),
    ]);
    if two_sources {
        nodes.insert(node("second"), linked_source("Second source", 16));
    }
    let inserted = edit(
        &qualified,
        Command::Insert {
            parent: node("root"),
            index: 0,
            subtree: Subtree {
                root: node("group"),
                nodes,
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
    )
    .0;
    let with_start = put(
        &inserted,
        "role-range-start",
        "first",
        local("first", 2),
        InsertionBias::Right,
        AnchorLossPolicy::KeepUnresolved,
    );
    let end_owner = if two_sources { "second" } else { "first" };
    let end_local = if two_sources { 4 } else { 6 };
    put(
        &with_start,
        "role-range-end",
        end_owner,
        local(end_owner, end_local),
        InsertionBias::Left,
        AnchorLossPolicy::KeepUnresolved,
    )
}

fn role_named_select_instruction() -> SemanticInstruction {
    SemanticInstruction::SelectMarkRange {
        start: named("role-range-start"),
        end: named("role-range-end"),
    }
}

fn role_named_context() -> SemanticContext {
    SemanticContext {
        original: None,
        scoped: None,
        hold_duration: None,
        shared_hold_duration: None,
        mixed_hold_duration: None,
        selected_hold_duration: None,
        parent: node("group"),
        cursor: ProjectFrame(0),
        selected_child: Some(node("first")),
        visual_selection: None,
    }
}

fn plan_role_named(
    document: &ProjectDocument,
    program: SemanticProgram,
    tag: &str,
) -> Result<(SemanticPlan, Vec<SemanticAllocationRequest>), EditError> {
    let bank = BTreeMap::new();
    let mut allocations = Vec::new();
    let planned = plan_semantic(
        document,
        &role_named_context(),
        &program,
        SemanticRegisterBank {
            entries: &bank,
            version: 11,
        },
        RevisionId::new(format!("role-named-{tag}-final")).unwrap(),
        |request| {
            allocations.push(request.clone());
            let revision = |step: usize, kind: &str| {
                RevisionId::new(format!("role-named-{tag}-{kind}-{step}")).unwrap()
            };
            Ok(match request {
                SemanticAllocationRequest::ParameterEdit { step_index } => {
                    SemanticAllocation::ParameterEdit {
                        new_revision: revision(step_index, "edit"),
                    }
                }
                SemanticAllocationRequest::Sound { step_index } => SemanticAllocation::Sound {
                    new_revision: revision(step_index, "sound"),
                    id: SoundId::new(format!("role-named-{tag}-sound-{step_index}")).unwrap(),
                },
                other => panic!("unexpected role named-range allocation: {other:?}"),
            })
        },
        |_, _| panic!("role named-range Edit selectors never resolve Original registers"),
    )?;
    Ok((planned, allocations))
}

#[test]
fn named_mark_range_composes_with_single_child_role_edits_and_refuses_cross_child_ranges() {
    let before = role_named_fixture(false);
    before.validate().unwrap();
    assert_eq!(before.duration().unwrap(), duration(8));
    let program =
        |operation| SemanticProgram::new(vec![role_named_select_instruction(), operation]).unwrap();

    // DeleteRole consumes the selected mark range as a one-child Visual time
    // range. It creates a mute interval only; authored picture/time mapping,
    // duration, marks and the register bank remain intact.
    let delete = SemanticInstruction::DeleteRole {
        role: MediaRole::Audio,
    };
    let (deleted, allocations) =
        plan_role_named(&before, program(delete.clone()), "delete-audio").unwrap();
    assert_eq!(
        allocations,
        vec![SemanticAllocationRequest::ParameterEdit { step_index: 0 }]
    );
    assert_eq!(
        deleted.trace[0].instruction,
        role_named_select_instruction()
    );
    assert_eq!(deleted.trace[0].resolved_range, Some(frame_range(2, 6)));
    assert_eq!(deleted.trace[1].instruction, delete);
    assert_eq!(deleted.trace[1].resolved_range, Some(frame_range(2, 6)));
    assert_eq!(
        deleted.document.duration().unwrap(),
        before.duration().unwrap()
    );
    assert_eq!(deleted.document.marks(), before.marks());
    assert_eq!(deleted.document.sounds(), before.sounds());
    let source_before = match &before.nodes()[&node("first")].kind {
        NodeKind::Source { source } => source,
        _ => unreachable!(),
    };
    let source_after_delete = match &deleted.document.nodes()[&node("first")].kind {
        NodeKind::Source { source } => source,
        _ => unreachable!(),
    };
    assert_eq!(
        source_after_delete.audio_mapping,
        source_before.audio_mapping
    );
    assert_eq!(source_after_delete.link, LinkRelation::Linked);
    assert_eq!(
        deleted.document.nodes()[&node("first")]
            .audio_treatments
            .clip_gain()
            .unwrap()
            .mute_ranges(),
        &[GainRange::new(ExactRatio::integer(2), ExactRatio::integer(6)).unwrap()]
    );
    assert!(deleted.register_writes.is_empty());
    let delete_request = deleted.request.as_ref().unwrap();
    let delete_replay = replay_compound::<EditError>(&before, delete_request, |_| Ok(())).unwrap();
    assert_eq!(delete_replay.document, deleted.document);
    assert!(delete_replay.register_writes.is_empty());
    assert_eq!(delete_replay.edit.duration_delta, 0);
    assert_eq!(
        delete_replay
            .edit
            .inverse
            .apply(&delete_replay.document)
            .unwrap(),
        before
    );

    // RoleRepeat is non-ripple too. Two total plays fit only after trimming the
    // second play at the Source end, with an exact repeated-sound mapping.
    let repeat = SemanticInstruction::RoleRepeat {
        role: MediaRole::Audio,
        plays: std::num::NonZeroU32::new(2).unwrap(),
        trim: true,
        extend: false,
    };
    let (repeated, allocations) =
        plan_role_named(&before, program(repeat.clone()), "repeat-audio").unwrap();
    assert_eq!(
        allocations,
        vec![
            SemanticAllocationRequest::ParameterEdit { step_index: 0 },
            SemanticAllocationRequest::Sound { step_index: 1 },
        ]
    );
    assert_eq!(
        repeated.trace[0].instruction,
        role_named_select_instruction()
    );
    assert_eq!(repeated.trace[0].resolved_range, Some(frame_range(2, 6)));
    assert_eq!(repeated.trace[1].instruction, repeat);
    assert_eq!(repeated.trace[1].resolved_range, Some(frame_range(2, 8)));
    assert_eq!(
        repeated.document.duration().unwrap(),
        before.duration().unwrap()
    );
    assert_eq!(repeated.document.marks(), before.marks());
    assert_eq!(repeated.document.sounds().len(), 1);
    let source_after_repeat = match &repeated.document.nodes()[&node("first")].kind {
        NodeKind::Source { source } => source,
        _ => unreachable!(),
    };
    assert_eq!(
        source_after_repeat.audio_mapping,
        source_before.audio_mapping
    );
    assert_eq!(
        repeated.document.nodes()[&node("first")]
            .audio_treatments
            .clip_gain()
            .unwrap()
            .mute_ranges(),
        &[GainRange::new(ExactRatio::integer(6), ExactRatio::integer(8)).unwrap()]
    );
    let event = repeated.document.sounds().values().next().unwrap();
    assert_eq!(event.owner, node("root"));
    assert_eq!(
        event.mapping,
        SourceAudioMapping::SelectedPlacement {
            start: ExactRatio::integer(4),
            frames: ExactRatio::integer(8),
            selection: ExactFrameRange::new(ExactRatio::integer(6), ExactRatio::integer(8))
                .unwrap(),
        }
    );
    assert!(repeated.register_writes.is_empty());
    let repeat_request = repeated.request.as_ref().unwrap();
    let repeat_replay = replay_compound::<EditError>(&before, repeat_request, |_| Ok(())).unwrap();
    assert_eq!(repeat_replay.document, repeated.document);
    assert!(repeat_replay.register_writes.is_empty());
    assert_eq!(repeat_replay.edit.duration_delta, 0);
    assert_eq!(
        repeat_replay
            .edit
            .inverse
            .apply(&repeat_replay.document)
            .unwrap(),
        before
    );

    // Both role operations refuse a named range spanning two direct children
    // before requesting any identity/revision allocation or publishing a step.
    let cross_child = role_named_fixture(true);
    cross_child.validate().unwrap();
    for operation in [
        SemanticInstruction::DeleteRole {
            role: MediaRole::Audio,
        },
        SemanticInstruction::RoleRepeat {
            role: MediaRole::Audio,
            plays: std::num::NonZeroU32::new(2).unwrap(),
            trim: true,
            extend: false,
        },
    ] {
        let snapshot = cross_child.to_json().unwrap();
        let bank = BTreeMap::new();
        let mut allocations = Vec::new();
        let error = plan_semantic(
            &cross_child,
            &role_named_context(),
            &program(operation),
            SemanticRegisterBank {
                entries: &bank,
                version: 11,
            },
            RevisionId::new("role-named-cross-child-final").unwrap(),
            |request| {
                allocations.push(request.clone());
                allocate_named(request, "cross-child")
            },
            |_, _| panic!("role named-range Edit selectors never resolve Original registers"),
        )
        .unwrap_err();
        assert_eq!(error.code, EditErrorCode::SelectionUnavailable);
        assert!(error.message.contains("one beat"));
        assert!(allocations.is_empty());
        assert!(bank.is_empty());
        assert_eq!(cross_child.to_json().unwrap(), snapshot);
    }
}
