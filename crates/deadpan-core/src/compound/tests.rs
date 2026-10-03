use super::*;
use crate::*;
use serde_json::json;

fn node(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}
fn revision(name: &str) -> RevisionId {
    RevisionId::new(name).unwrap()
}
fn timing(name: &str) -> AudioTimingId {
    AudioTimingId {
        allocation: revision(name),
        ordinal: 0,
    }
}
fn name(value: char) -> RegisterName {
    RegisterName::new(value).unwrap()
}
fn fixture() -> ProjectDocument {
    ProjectDocument::from_json(&json!({
        "schema_version": DOCUMENT_SCHEMA_VERSION, "project_id":"compound", "revision_id":"base",
        "presentation_basis":{"width":16,"height":16,"frame_rate":{"numerator":30000,"denominator":1001},"color_policy":"sdr_rec709"},
        "basis_state":{"rate_origin":"explicit","geometry_origin":"explicit","primary":null},
        "root":"root", "nodes": {
            "root": BeatNode::sequence("Root",vec![node("held")]),
            "held": BeatNode::hold("Hold",HoldRecipe { duration:FrameDuration::new(7).unwrap(),
                picture_context:None, video:HoldVideo::Background,audio:HoldAudio::Silence })
        }, "assets":{}, "marks":{}, "overrides":{}
    }).to_string()).unwrap()
}
fn leaf(id: &str, command: Command) -> LeafEdit {
    LeafEdit::new(revision(id), command).unwrap()
}
fn request(document: &ProjectDocument, transaction: ResolvedTransaction) -> CommandRequest {
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision("outer"),
        command: Command::Compound { transaction },
    }
}
fn transaction(steps: Vec<ResolvedStep>) -> ResolvedTransaction {
    ResolvedTransaction::new(0, BTreeMap::new(), steps).unwrap()
}

#[test]
fn macro_inputs_are_frozen_values_but_cannot_be_captured_or_pasted() {
    let document = fixture();
    let value = Arc::new(RegisterValue::Macro {
        program: Arc::new(
            SemanticProgram::new(vec![SemanticInstruction::MoveFrames {
                forward: true,
                count: std::num::NonZeroU32::new(1).unwrap(),
            }])
            .unwrap(),
        ),
    });
    let inputs = BTreeMap::from([(name('a'), Some(value.clone()))]);
    let allowed = ResolvedTransaction::new(
        3,
        inputs.clone(),
        vec![ResolvedStep::Edit {
            edit: rename("renamed"),
        }],
    )
    .unwrap();
    let encoded = serde_json::to_string(&allowed).unwrap();
    assert_eq!(
        serde_json::from_str::<ResolvedTransaction>(&encoded).unwrap(),
        allowed
    );
    assert!(apply(&document, &request(&document, allowed)).is_ok());
    for step in [
        ResolvedStep::Yank {
            name: name('a'),
            value,
        },
        ResolvedStep::Paste {
            name: name('a'),
            edit: rename("pasted"),
        },
    ] {
        let rejected = ResolvedTransaction::new(3, inputs.clone(), vec![step]).unwrap();
        let error = apply(&document, &request(&document, rejected)).unwrap_err();
        assert_eq!(error.code, EditErrorCode::InvalidCommand);
        assert!(error.message.contains("a macro cannot"));
    }
}
fn rename(id: &str) -> LeafEdit {
    leaf(
        id,
        Command::Rename {
            node: node("held"),
            label: id.into(),
        },
    )
}
fn apply_leaf(document: &ProjectDocument, leaf: &LeafEdit) -> ProjectDocument {
    apply(document, &leaf.request(document))
        .unwrap()
        .forward
        .apply(document)
        .unwrap()
}
fn slice(document: &ProjectDocument, selected: &str) -> Arc<CapturedEditSlice> {
    Arc::new(
        CapturedEditSlice::capture_selection(
            document,
            &node("root"),
            &SliceCaptureSelection::Child {
                node: node(selected),
            },
            timing("capture"),
        )
        .unwrap(),
    )
}
fn paste(slice: &CapturedEditSlice, id: &str, index: usize) -> LeafEdit {
    let required = slice.identity_requirements().unwrap();
    leaf(
        id,
        Command::SpliceSlice {
            parent: node("root"),
            index,
            slice: slice.clone(),
            timing: timing(id),
            identities: SlicePasteIdentities {
                authored: OccurrenceIdentities {
                    nodes: (0..required.nodes)
                        .map(|n| node(&format!("{id}-n{n}")))
                        .collect(),
                    marks: (0..required.marks)
                        .map(|n| MarkId::new(format!("{id}-m{n}")).unwrap())
                        .collect(),
                },
                aliases: (0..required.aliases)
                    .map(|n| node(&format!("{id}-a{n}")))
                    .collect(),
            },
        },
    )
}
fn repeat_cut_paste() -> (ProjectDocument, CommandRequest) {
    let before = fixture();
    let repeat = leaf(
        "repeat-stage",
        Command::WrapRepeat {
            node: node("held"),
            id: node("repeat"),
            plays: 3,
            gap: None,
            anchor_policy: WrapAnchorPolicy::First,
        },
    );
    let staged = apply_leaf(&before, &repeat);
    let copied = slice(&staged, "repeat");
    let command = request(
        &before,
        transaction(vec![
            ResolvedStep::Edit { edit: repeat },
            ResolvedStep::Cut {
                name: name('a'),
                slice: copied.clone(),
                delete: leaf(
                    "cut-stage",
                    Command::DeleteRipple {
                        node: node("repeat"),
                        timing: timing("cut-stage"),
                    },
                ),
            },
            ResolvedStep::Paste {
                name: name('a'),
                edit: paste(&copied, "paste-stage", 0),
            },
            ResolvedStep::Paste {
                name: RegisterName::unnamed(),
                edit: paste(&copied, "paste-two", 1),
            },
        ]),
    );
    (before, command)
}

#[test]
fn repeat_cut_paste_uses_intermediate_capture_and_one_exact_inverse() {
    let (before, request) = repeat_cut_paste();
    let mut visits = Vec::new();
    let outcome = replay_compound::<EditError>(&before, &request, |step| {
        visits.push((
            step.before.revision_id().clone(),
            step.after.revision_id().clone(),
            step.request.is_some(),
        ));
        if let Some(RegisterValue::Edited { slice }) = step.value {
            assert_eq!(slice.revision_id(), &revision("repeat-stage"));
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(visits.len(), 4);
    assert_eq!(visits[1].0, revision("repeat-stage"));
    assert_eq!(outcome.document.duration().unwrap().frames(), 42);
    assert_eq!(outcome.document.revision_id(), &revision("outer"));
    assert_eq!(
        outcome.edit.inverse.apply(&outcome.document).unwrap(),
        before
    );
    assert_eq!(
        outcome.edit.forward.apply(&before).unwrap(),
        outcome.document
    );
    assert_eq!(apply(&before, &request).unwrap(), outcome.edit);
    assert_eq!(
        outcome.register_writes[&name('a')],
        outcome.register_writes[&RegisterName::unnamed()]
    );
    let families: BTreeSet<_> = outcome
        .document
        .nodes()
        .values()
        .filter_map(|node| {
            if let NodeKind::Repeat { iterations, .. } = &node.kind {
                Some(iterations.at(0).unwrap().allocation.clone())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        families,
        BTreeSet::from([revision("paste-stage"), revision("paste-two")])
    );
    let allocations: BTreeSet<_> = outcome
        .document
        .audio_bindings()
        .timings()
        .keys()
        .map(|id| id.allocation.clone())
        .collect();
    assert!(allocations.contains(&revision("paste-stage")));
    assert!(allocations.contains(&revision("paste-two")));
    assert_eq!(
        serde_json::from_str::<CommandRequest>(&serde_json::to_string(&request).unwrap()).unwrap(),
        request
    );
}

#[test]
fn late_error_leaves_document_and_frozen_values_unchanged() {
    let before = fixture();
    let copied = Arc::new(RegisterValue::Edited {
        slice: slice(&before, "held"),
    });
    let original_copy = copied.clone();
    let tx = ResolvedTransaction::new(
        7,
        BTreeMap::from([(name('a'), Some(copied))]),
        vec![
            ResolvedStep::Edit {
                edit: rename("first"),
            },
            ResolvedStep::Edit {
                edit: leaf(
                    "last",
                    Command::Rename {
                        node: node("absent"),
                        label: "late".into(),
                    },
                ),
            },
        ],
    )
    .unwrap();
    let frozen = tx.clone();
    let original = before.clone();
    assert!(replay_compound::<EditError>(&before, &request(&before, tx), |_| Ok(())).is_err());
    assert_eq!(before, original);
    assert_eq!(
        frozen.inputs()[&name('a')].as_ref().unwrap(),
        &original_copy
    );
}

#[test]
fn capture_must_match_current_stage_and_cut_exact_selection() {
    let before = fixture();
    let old = slice(&before, "held");
    let tx = transaction(vec![
        ResolvedStep::Edit {
            edit: rename("changed"),
        },
        ResolvedStep::Yank {
            name: name('a'),
            value: Arc::new(RegisterValue::Edited { slice: old.clone() }),
        },
    ]);
    assert!(apply(&before, &request(&before, tx)).is_err());
    let tx = transaction(vec![ResolvedStep::Cut {
        name: name('a'),
        slice: old,
        delete: leaf(
            "cut",
            Command::DeleteRipple {
                node: node("root"),
                timing: timing("cut"),
            },
        ),
    }]);
    let error = apply(&before, &request(&before, tx)).unwrap_err();
    assert!(error.to_string().contains("exactly its captured selection"));
}

#[test]
fn paste_requires_exact_selected_payload_and_frozen_absence_is_not_a_value() {
    let before = fixture();
    let copied = slice(&before, "held");
    let tx = ResolvedTransaction::new(
        0,
        BTreeMap::from([(name('a'), None)]),
        vec![ResolvedStep::Paste {
            name: name('a'),
            edit: paste(&copied, "paste", 1),
        }],
    )
    .unwrap();
    assert!(
        apply(&before, &request(&before, tx))
            .unwrap_err()
            .to_string()
            .contains("paste register is absent")
    );
    let different = Arc::new(
        CapturedEditSlice::capture(
            &before,
            &node("root"),
            FrameRange::new(ProjectFrame(0), ProjectFrame(3)).unwrap(),
            timing("partial"),
        )
        .unwrap(),
    );
    let tx = ResolvedTransaction::new(
        0,
        BTreeMap::from([(
            name('a'),
            Some(Arc::new(RegisterValue::Edited { slice: different })),
        )]),
        vec![ResolvedStep::Paste {
            name: name('a'),
            edit: paste(&copied, "paste", 1),
        }],
    )
    .unwrap();
    assert!(
        apply(&before, &request(&before, tx))
            .unwrap_err()
            .to_string()
            .contains("differs from its selected")
    );
}

#[test]
fn yank_only_visits_no_leaf_and_named_write_sets_unnamed() {
    let before = fixture();
    let value = Arc::new(RegisterValue::Edited {
        slice: slice(&before, "held"),
    });
    let tx = transaction(vec![ResolvedStep::Yank {
        name: name('z'),
        value: value.clone(),
    }]);
    let outcome = replay_compound::<EditError>(&before, &request(&before, tx), |visit| {
        assert!(visit.request.is_none());
        assert!(std::ptr::eq(visit.before, visit.after));
        Ok(())
    })
    .unwrap();
    assert!(outcome.edit.forward.nodes.is_empty());
    assert_eq!(outcome.document.nodes(), before.nodes());
    assert_eq!(outcome.register_writes.len(), 2);
    assert_eq!(outcome.register_writes[&name('z')], value);
}

#[test]
fn revision_reuse_and_retired_node_identity_are_rejected() {
    let before = fixture();
    assert!(
        ResolvedTransaction::new(
            0,
            BTreeMap::new(),
            vec![
                ResolvedStep::Edit {
                    edit: rename("same")
                },
                ResolvedStep::Edit {
                    edit: rename("same")
                }
            ]
        )
        .is_err()
    );
    for duplicate in ["base", "outer"] {
        assert!(
            apply(
                &before,
                &request(
                    &before,
                    transaction(vec![ResolvedStep::Edit {
                        edit: rename(duplicate)
                    }])
                )
            )
            .is_err()
        );
    }
    let tx = transaction(vec![
        ResolvedStep::Edit {
            edit: leaf(
                "cut",
                Command::DeleteRipple {
                    node: node("held"),
                    timing: timing("cut"),
                },
            ),
        },
        ResolvedStep::Edit {
            edit: leaf(
                "reuse",
                Command::Insert {
                    parent: node("root"),
                    index: 0,
                    subtree: Subtree {
                        root: node("held"),
                        nodes: BTreeMap::from([(
                            node("held"),
                            before.nodes()[&node("held")].clone(),
                        )]),
                        overrides: BTreeMap::new(),
                        gap_overrides: BTreeMap::new(),
                    },
                },
            ),
        },
    ]);
    assert_eq!(
        apply(&before, &request(&before, tx)).unwrap_err().code,
        EditErrorCode::IdentityConflict
    );
}

#[test]
fn nested_historical_and_oversized_programs_reject_before_execution() {
    let tx = transaction(vec![ResolvedStep::Edit {
        edit: rename("leaf"),
    }]);
    assert!(
        AtomicCommand::new(Command::Compound {
            transaction: tx.clone()
        })
        .is_err()
    );
    assert!(
        AtomicCommand::new(Command::Delete { node: node("held") })
            .unwrap_err()
            .to_string()
            .contains("DeleteRipple")
    );
    let nested = json!({"new_revision":"inner","command":{"command":"compound","transaction":tx}});
    assert!(
        serde_json::from_value::<LeafEdit>(nested)
            .unwrap_err()
            .to_string()
            .contains("nested compound")
    );
    assert!(ResolvedTransaction::new(0, BTreeMap::new(), Vec::new()).is_err());
    let steps = (0..=MAX_COMPOUND_STEPS)
        .map(|i| ResolvedStep::Edit {
            edit: rename(&format!("leaf-{i}")),
        })
        .collect::<Vec<_>>();
    let encoded = json!({"expected_bank_version":0,"inputs":{},"steps":steps});
    assert!(
        serde_json::from_value::<ResolvedTransaction>(encoded)
            .unwrap_err()
            .to_string()
            .contains("1024")
    );
    assert!(ResolvedTransaction::new(0, BTreeMap::new(), steps).is_err());
    let command = Command::Rename {
        node: node("held"),
        label: "x".repeat(MAX_COMPOUND_WIRE_BYTES),
    };
    assert_eq!(
        AtomicCommand::new(command).unwrap_err().code,
        EditErrorCode::LimitExceeded
    );
}

#[test]
fn visitor_rejection_returns_no_completed_transaction() {
    let before = fixture();
    let mut visited = 0;
    let result = replay_compound::<EditError>(
        &before,
        &request(
            &before,
            transaction(vec![
                ResolvedStep::Edit {
                    edit: rename("one"),
                },
                ResolvedStep::Edit {
                    edit: rename("two"),
                },
            ]),
        ),
        |_| {
            visited += 1;
            if visited == 2 {
                Err(super::invalid("host admission failed"))
            } else {
                Ok(())
            }
        },
    );
    assert!(result.is_err());
    assert_eq!(visited, 2);
    assert_eq!(before, fixture());
}

#[test]
fn canonical_size_limit_includes_the_complete_compound_command() {
    let steps = |length| {
        vec![ResolvedStep::Edit {
            edit: leaf(
                "large",
                Command::Rename {
                    node: node("held"),
                    label: "x".repeat(length),
                },
            ),
        }]
    };
    let empty = Command::Compound {
        transaction: transaction(steps(0)),
    };
    let overhead = serde_json::to_vec(&empty).unwrap().len();
    let maximum_label = MAX_COMPOUND_WIRE_BYTES - overhead;
    let command = Command::Compound {
        transaction: transaction(steps(maximum_label)),
    };
    let encoded = serde_json::to_vec(&command).unwrap();
    assert_eq!(encoded.len(), MAX_COMPOUND_WIRE_BYTES);
    assert_eq!(
        serde_json::from_slice::<Command>(&encoded).unwrap(),
        command
    );
    drop(encoded);
    drop(command);
    assert_eq!(
        ResolvedTransaction::new(0, BTreeMap::new(), steps(maximum_label + 1))
            .unwrap_err()
            .code,
        EditErrorCode::LimitExceeded
    );
}

fn set_mark(id: &str, position: i64) -> LeafEdit {
    leaf(
        id,
        Command::SetMark {
            id: MarkId::new("mark-a").unwrap(),
            owner: node("held"),
            label: "A".into(),
            boundary: BoundaryAnchor {
                coordinate: Anchor::Local {
                    node: node("held"),
                    position: ExactRatio::integer(position),
                },
                bias: InsertionBias::Right,
            },
            loss_policy: AnchorLossPolicy::KeepUnresolved,
        },
    )
}
#[test]
fn existing_mark_updates_are_allowed_but_deleted_marks_cannot_be_reallocated() {
    let before = fixture();
    let update = transaction(vec![
        ResolvedStep::Edit {
            edit: set_mark("create", 1),
        },
        ResolvedStep::Edit {
            edit: set_mark("update", 3),
        },
    ]);
    let after = apply(&before, &request(&before, update))
        .unwrap()
        .forward
        .apply(&before)
        .unwrap();
    assert_eq!(after.marks().len(), 1);
    assert_eq!(
        after.marks()[&MarkId::new("mark-a").unwrap()]
            .boundary
            .coordinate,
        Anchor::Local {
            node: node("held"),
            position: ExactRatio::integer(3)
        }
    );
    let reuse = transaction(vec![
        ResolvedStep::Edit {
            edit: set_mark("create", 1),
        },
        ResolvedStep::Edit {
            edit: leaf(
                "retire",
                Command::DeleteMark {
                    id: MarkId::new("mark-a").unwrap(),
                },
            ),
        },
        ResolvedStep::Edit {
            edit: set_mark("reallocate", 3),
        },
    ]);
    assert_eq!(
        apply(&before, &request(&before, reuse)).unwrap_err().code,
        EditErrorCode::IdentityConflict
    );
}

#[test]
fn streaming_wire_rejects_excess_before_reading_its_payload_and_rejects_duplicates_and_depth() {
    // The excess value is deliberately incomplete. A syntax error here would
    // mean the reader entered its payload instead of rejecting its allocation.
    let mut wire = String::from("{\"expected_bank_version\":0,\"inputs\":{},\"steps\":[");
    for i in 0..MAX_COMPOUND_STEPS {
        if i > 0 {
            wire.push(',');
        }
        wire.push_str(
            &serde_json::to_string(&ResolvedStep::Edit {
                edit: rename(&format!("r-{i}")),
            })
            .unwrap(),
        );
    }
    wire.push_str(",{\"unread");
    assert!(
        serde_json::from_str::<ResolvedTransaction>(&wire)
            .unwrap_err()
            .to_string()
            .contains("1024 expanded steps")
    );
    let command = format!("{{\"command\":\"compound\",\"transaction\":{wire}");
    assert!(
        serde_json::from_str::<Command>(&command)
            .unwrap_err()
            .to_string()
            .contains("1024 expanded steps")
    );
    let duplicate = r#"{"command":"rename","node":"held","node":"other","label":"x"}"#;
    assert!(
        serde_json::from_str::<Command>(duplicate)
            .unwrap_err()
            .to_string()
            .contains("duplicate command")
    );
    let deep = format!(
        "{{\"command\":\"rename\",\"unexpected\":{}0{} }}",
        "[".repeat(70),
        "]".repeat(70)
    );
    assert!(
        serde_json::from_str::<Command>(&deep)
            .unwrap_err()
            .to_string()
            .contains("depth limit")
    );
}

fn original_fixture() -> (ProjectDocument, Arc<RegisterValue>, SourceNode) {
    let base = fixture();
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
    let qualification = SourceQualificationId::new("b".repeat(64)).unwrap();
    let asset = AssetId::new("original").unwrap();
    let document = apply_leaf(
        &base,
        &leaf(
            "registered",
            Command::AddAsset {
                id: asset.clone(),
                asset: AssetRecord {
                    label: "Original".into(),
                    content_hash: "a".repeat(64),
                    video: Some(span),
                    audio: None,
                    still_image: false,
                    frame_count: Some(FrameDuration::new(30).unwrap()),
                    source_qualification: Some(qualification.clone()),
                },
            },
        ),
    );
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
fn original_capture_and_paste_bind_qualified_video_and_allow_video_without_audio() {
    let (document, value, source) = original_fixture();
    let paste = |source: SourceNode| {
        leaf(
            "paste-original",
            Command::SpliceSource {
                parent: node("root"),
                index: 1,
                source,
                id: node("placed-original"),
                label: "Original".into(),
                timing: timing("paste-original"),
            },
        )
    };
    let tx = transaction(vec![
        ResolvedStep::Yank {
            name: name('a'),
            value: value.clone(),
        },
        ResolvedStep::Paste {
            name: name('a'),
            edit: paste(source.clone()),
        },
    ]);
    let result = apply(&document, &request(&document, tx))
        .unwrap()
        .forward
        .apply(&document)
        .unwrap();
    assert_eq!(result.duration().unwrap().frames(), 37);
    let mut stale = value.as_ref().clone();
    let RegisterValue::Original {
        revision: captured_revision,
        ..
    } = &mut stale
    else {
        unreachable!()
    };
    *captured_revision = revision("old");
    let tx = transaction(vec![ResolvedStep::Yank {
        name: name('a'),
        value: Arc::new(stale),
    }]);
    assert!(apply(&document, &request(&document, tx)).is_err());
    let mut wrong = source.clone();
    wrong.video = SourceVideo::Blank;
    let tx = ResolvedTransaction::new(
        0,
        BTreeMap::from([(name('a'), Some(value.clone()))]),
        vec![ResolvedStep::Paste {
            name: name('a'),
            edit: paste(wrong),
        }],
    )
    .unwrap();
    assert!(
        apply(&document, &request(&document, tx))
            .unwrap_err()
            .to_string()
            .contains("selected asset")
    );
    let RegisterValue::Original {
        revision,
        asset,
        qualification,
        ..
    } = value.as_ref()
    else {
        unreachable!()
    };
    for ordinals in [2..2, 0..31] {
        let tx = transaction(vec![ResolvedStep::Yank {
            name: name('a'),
            value: Arc::new(RegisterValue::Original {
                revision: revision.clone(),
                asset: asset.clone(),
                qualification: qualification.clone(),
                ordinals,
            }),
        }]);
        assert!(apply(&document, &request(&document, tx)).is_err());
    }
    let tx = transaction(vec![ResolvedStep::Yank {
        name: name('a'),
        value: Arc::new(RegisterValue::Original {
            revision: revision.clone(),
            asset: asset.clone(),
            qualification: SourceQualificationId::new("c".repeat(64)).unwrap(),
            ordinals: 0..30,
        }),
    }]);
    assert!(apply(&document, &request(&document, tx)).is_err());
}
