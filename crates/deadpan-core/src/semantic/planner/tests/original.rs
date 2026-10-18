use super::*;
use crate::{
    Anchor, AnchorLossPolicy, AssetId, AssetRecord, BoundaryAnchor, IndexedSourceFrame,
    InsertionBias, Mark, MarkFragment, MarkLossReason, MarkState, SourceFrameId, SourceFrameIndex,
    SourceMoment, SourceQualificationId, SourceSpan, SourceStream, SourceTimeBase, SourceTimestamp,
    TerminalProvenance,
};

fn fixture() -> (ProjectDocument, Arc<SourceFrameIndex>) {
    let mut document = super::fixture(20);
    let asset = AssetId::new("original").unwrap();
    let time_base = SourceTimeBase::new(1, 30_000).unwrap();
    let index = Arc::new(
        SourceFrameIndex::new(
            asset.clone(),
            time_base,
            [-2002, -1001, 1001, 4004]
                .into_iter()
                .enumerate()
                .map(|(n, pts)| IndexedSourceFrame {
                    identity: SourceFrameId(u64::try_from(n).unwrap()),
                    pts,
                    reported_duration: Some(1001),
                    keyframe: n == 0,
                    seek_from: Some(SourceFrameId(0)),
                    decode_timestamp: None,
                })
                .collect(),
            5005,
            TerminalProvenance::DecodedFrameDuration,
        )
        .unwrap(),
    );
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: -2002,
            time_base,
        },
        SourceTimestamp {
            ticks: 5005,
            time_base,
        },
    )
    .unwrap();
    document.assets.insert(
        asset,
        AssetRecord {
            label: "Original".into(),
            content_hash: "a".repeat(64),
            video: Some(span),
            audio: Some(span),
            still_image: false,
            frame_count: Some(FrameDuration::new(4).unwrap()),
            source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()),
        },
    );
    document.validate().unwrap();
    (document, index)
}

fn entry(ordinal: u64) -> SemanticContext {
    SemanticContext {
        original: Some(SemanticOriginalContext {
            asset: AssetId::new("original").unwrap(),
            qualification: SourceQualificationId::new("b".repeat(64)).unwrap(),
            ordinal: SourceFrameId(ordinal),
        }),
        selected_child: Some(node("held")),
        visual_selection: Some(SemanticVisualSelection::Time {
            anchor: ProjectFrame(8),
            head: ProjectFrame(12),
            extending: false,
        }),
        ..context("root", 3)
    }
}

fn plan_original(
    document: &ProjectDocument,
    context: &SemanticContext,
    instructions: Vec<SemanticInstruction>,
    bank: &BTreeMap<RegisterName, Arc<RegisterValue>>,
    index: Option<Arc<SourceFrameIndex>>,
) -> Result<SemanticPlan, EditError> {
    plan_semantic_with_original(
        document,
        context,
        &program(instructions),
        SemanticRegisterBank {
            entries: bank,
            version: 7,
        },
        revision("outer"),
        allocate,
        no_original,
        |_| Err(speech_unavailable()),
        |_, _| Err(pause_unavailable()),
        index,
    )
}

fn assert_edit_retained(actual: &SemanticContext, expected: &SemanticContext) {
    let mut edit = actual.clone();
    edit.original = expected.original.clone();
    assert_eq!(&edit, expected);
}

fn source_anchor(index: &SourceFrameIndex, ticks: i64) -> Anchor {
    Anchor::Source {
        asset: index.asset().clone(),
        moment: SourceMoment::Timestamp {
            stream: SourceStream::Video,
            timestamp: SourceTimestamp {
                ticks,
                time_base: index.time_base(),
            },
        },
    }
}

fn marked(document: &ProjectDocument, coordinate: Anchor) -> ProjectDocument {
    let mut document = document.clone();
    document.marks.insert(
        crate::native_mark_id('a').unwrap(),
        Mark {
            owner: node("root"),
            label: "a".into(),
            boundary: BoundaryAnchor {
                coordinate,
                bias: InsertionBias::Right,
            },
            loss_policy: AnchorLossPolicy::KeepUnresolved,
            state: MarkState::Bound,
            fragments: vec![],
        },
    );
    document
}

#[test]
fn original_vfr_marks_replay_intent_at_each_invocation_and_retain_edit_state() {
    let (document, index) = fixture();
    let body = vec![
        SemanticInstruction::SetMark { letter: 'A' },
        motion(true, 3),
        SemanticInstruction::JumpMark { letter: 'A' },
    ];
    let wire = serde_json::to_string(&program(body.clone())).unwrap();
    assert!(!wire.contains("ordinal") && !wire.contains("timestamp") && !wire.contains("-2002"));
    for (ordinal, ticks) in [(0, -2002), (2, 1001)] {
        let entry = entry(ordinal);
        let plan = plan_original(
            &document,
            &entry,
            body.clone(),
            &BTreeMap::new(),
            Some(index.clone()),
        )
        .unwrap();
        assert_eq!(plan.context, entry);
        let mark = &plan.document.marks()[&crate::native_mark_id('A').unwrap()];
        assert_eq!(mark.owner, node("root"));
        assert_eq!(mark.boundary.coordinate, source_anchor(&index, ticks));
        assert_eq!(mark.boundary.bias, InsertionBias::Right);
        assert!(
            !plan
                .document
                .marks()
                .contains_key(&crate::native_mark_id('a').unwrap())
        );
        for trace in &plan.trace {
            assert_edit_retained(&trace.before, &entry);
            assert_edit_retained(&trace.after, &entry);
            assert_eq!(trace.before_scope, range(0, 20));
        }
        let replay =
            crate::replay_compound::<EditError>(&document, plan.request.as_ref().unwrap(), |_| {
                Ok(())
            })
            .unwrap();
        assert_eq!(replay.document, plan.document);
        assert_eq!(
            replay.edit.inverse.apply(&replay.document).unwrap(),
            document
        );
    }
}

#[test]
fn original_navigation_clamps_to_measured_terminal_without_creating_history() {
    let (document, index) = fixture();
    let entry = entry(1);
    let navigation = plan_original(
        &document,
        &entry,
        vec![
            motion(true, u32::MAX),
            motion(false, u32::MAX),
            SemanticInstruction::MoveScope { end: true },
        ],
        &BTreeMap::new(),
        Some(index.clone()),
    )
    .unwrap();
    assert!(navigation.request.is_none());
    assert_eq!(navigation.document, document);
    assert_eq!(
        navigation.context.original.as_ref().unwrap().ordinal,
        SourceFrameId(4)
    );
    assert_edit_retained(&navigation.context, &entry);
    let marked = plan_original(
        &document,
        &entry,
        vec![
            SemanticInstruction::MoveScope { end: true },
            SemanticInstruction::SetMark { letter: 'a' },
            SemanticInstruction::MoveScope { end: false },
            SemanticInstruction::JumpMark { letter: 'a' },
        ],
        &BTreeMap::new(),
        Some(index.clone()),
    )
    .unwrap();
    let mark = &marked.document.marks()[&crate::native_mark_id('a').unwrap()];
    assert_eq!(mark.boundary.coordinate, source_anchor(&index, 5005));
    assert_eq!(mark.boundary.bias, InsertionBias::Left);
    assert_eq!(marked.context.original.unwrap().ordinal, SourceFrameId(4));
}

#[test]
fn original_admission_requires_current_qualification_and_exact_measured_index() {
    let (document, index) = fixture();
    let entry = entry(0);
    let instructions = vec![motion(true, 1)];
    assert!(
        plan(
            &document,
            entry.clone(),
            instructions.clone(),
            &BTreeMap::new()
        )
        .is_err()
    );
    assert!(
        plan_original(
            &document,
            &entry,
            instructions.clone(),
            &BTreeMap::new(),
            None
        )
        .is_err()
    );
    for mutation in 0..5 {
        let mut entry = entry.clone();
        let mut document = document.clone();
        match mutation {
            0 => {
                entry.original.as_mut().unwrap().qualification =
                    SourceQualificationId::new("c".repeat(64)).unwrap()
            }
            1 => entry.original.as_mut().unwrap().ordinal = SourceFrameId(5),
            2 => entry.original.as_mut().unwrap().asset = AssetId::new("absent").unwrap(),
            3 => {
                document.assets.get_mut(index.asset()).unwrap().frame_count =
                    Some(FrameDuration::new(5).unwrap())
            }
            _ => {
                document
                    .assets
                    .get_mut(index.asset())
                    .unwrap()
                    .source_qualification = None
            }
        }
        assert!(
            plan_original(
                &document,
                &entry,
                instructions.clone(),
                &BTreeMap::new(),
                Some(index.clone())
            )
            .is_err(),
            "mutation {mutation}"
        );
    }
    for mutation in 0..4 {
        let other = Arc::new(
            SourceFrameIndex::new(
                if mutation == 0 {
                    AssetId::new("other").unwrap()
                } else {
                    index.asset().clone()
                },
                if mutation == 1 {
                    SourceTimeBase::new(1, 1000).unwrap()
                } else {
                    index.time_base()
                },
                index
                    .frames()
                    .iter()
                    .map(|frame| {
                        let mut frame = frame.clone();
                        if mutation == 2 {
                            frame.pts += 1;
                        }
                        frame
                    })
                    .collect(),
                if mutation == 3 { 5006 } else { 5005 },
                index.terminal_provenance(),
            )
            .unwrap(),
        );
        assert!(
            plan_original(
                &document,
                &entry,
                instructions.clone(),
                &BTreeMap::new(),
                Some(other)
            )
            .is_err(),
            "index mutation {mutation}"
        );
    }
    let original = entry.original.unwrap();
    let mut wire = serde_json::to_value(&original).unwrap();
    assert_eq!(
        serde_json::from_value::<SemanticOriginalContext>(wire.clone()).unwrap(),
        original
    );
    wire["cursor"] = serde_json::json!(0);
    assert!(serde_json::from_value::<SemanticOriginalContext>(wire).is_err());
}

#[test]
fn original_jump_refuses_wrong_clock_ambiguous_unresolved_and_named_bindings() {
    let (document, index) = fixture();
    let valid = marked(&document, source_anchor(&index, -1001));
    let mut cases = vec![
        marked(
            &document,
            Anchor::Sequence {
                frame: ProjectFrame(3),
            },
        ),
        marked(&document, source_anchor(&index, 0)),
    ];
    let mut wrong_asset = valid.clone();
    wrong_asset.assets.insert(
        AssetId::new("other").unwrap(),
        document.assets()[index.asset()].clone(),
    );
    if let Anchor::Source { asset, .. } = &mut wrong_asset
        .marks
        .get_mut(&crate::native_mark_id('a').unwrap())
        .unwrap()
        .boundary
        .coordinate
    {
        *asset = AssetId::new("other").unwrap();
    }
    cases.push(wrong_asset);
    for mutation in 0..6 {
        let mut document = valid.clone();
        let mark = document
            .marks
            .get_mut(&crate::native_mark_id('a').unwrap())
            .unwrap();
        match mutation {
            0 => {
                if let Anchor::Source {
                    moment: SourceMoment::Timestamp { stream, .. },
                    ..
                } = &mut mark.boundary.coordinate
                {
                    *stream = SourceStream::Audio;
                }
            }
            1 => {
                if let Anchor::Source {
                    moment: SourceMoment::Timestamp { timestamp, .. },
                    ..
                } = &mut mark.boundary.coordinate
                {
                    timestamp.time_base = SourceTimeBase::new(1, 60_000).unwrap();
                }
            }
            2 => {
                mark.state = MarkState::Unresolved {
                    reason: MarkLossReason::SourceUnavailable,
                }
            }
            3 => mark.label = "named".into(),
            4 | 5 => mark.fragments.push(MarkFragment {
                owner: node("root"),
                coordinate: if mutation == 4 {
                    source_anchor(&index, 4004)
                } else {
                    Anchor::Sequence {
                        frame: ProjectFrame(3),
                    }
                },
                state: MarkState::Bound,
            }),
            _ => unreachable!(),
        }
        cases.push(document);
    }
    for (n, document) in cases.iter().enumerate() {
        document.validate().unwrap();
        assert!(
            plan_original(
                document,
                &entry(2),
                vec![SemanticInstruction::JumpMark { letter: 'a' }],
                &BTreeMap::new(),
                Some(index.clone())
            )
            .is_err(),
            "jump mutation {n}"
        );
    }
    let planned = plan_original(
        &valid,
        &entry(2),
        vec![SemanticInstruction::JumpMark { letter: 'a' }],
        &BTreeMap::new(),
        Some(index.clone()),
    )
    .unwrap();
    assert_eq!(planned.context.original.unwrap().ordinal, SourceFrameId(1));
    // The invocation clock is fixed: an Edit invocation cannot jump to Original.
    assert!(
        plan(
            &valid,
            context("root", 3),
            vec![SemanticInstruction::JumpMark { letter: 'a' }],
            &BTreeMap::new()
        )
        .is_err()
    );
    let mut conflict = valid;
    conflict
        .marks
        .get_mut(&crate::native_mark_id('a').unwrap())
        .unwrap()
        .label = "named".into();
    for original in [false, true] {
        let context = if original {
            entry(1)
        } else {
            context("root", 3)
        };
        for instruction in [
            SemanticInstruction::SetMark { letter: 'a' },
            SemanticInstruction::JumpMark { letter: 'a' },
        ] {
            assert!(
                plan_original(
                    &conflict,
                    &context,
                    vec![instruction],
                    &BTreeMap::new(),
                    Some(index.clone())
                )
                .is_err()
            );
        }
    }
}

#[test]
fn original_nested_counted_calls_keep_proofs_limits_and_atomic_failure() {
    let (document, index) = fixture();
    let entry = entry(0);
    let bank = BTreeMap::from([
        (name('a'), macro_value(vec![call('b', 2)])),
        (name('b'), macro_value(vec![motion(true, 1)])),
    ]);
    let planned = plan_original(
        &document,
        &entry,
        vec![call('a', 2)],
        &bank,
        Some(index.clone()),
    )
    .unwrap();
    assert!(planned.request.is_none());
    assert_eq!(
        planned.context.original.as_ref().unwrap().ordinal,
        SourceFrameId(4)
    );
    assert_edit_retained(&planned.context, &entry);
    assert_eq!(planned.trace.len(), 7);
    let marked = plan_original(
        &document,
        &entry,
        vec![call('a', 1), SemanticInstruction::SetMark { letter: 'a' }],
        &bank,
        Some(index.clone()),
    )
    .unwrap();
    let Command::Compound { transaction } = &marked.request.as_ref().unwrap().command else {
        panic!("Compound required")
    };
    assert_eq!(transaction.inputs().len(), 2);
    let before = document.clone();
    let bank_before = bank.clone();
    for instruction in [
        cut(1, 'z'),
        SemanticInstruction::BeginSelection,
        SemanticInstruction::MoveBeats {
            forward: true,
            count: NonZeroU32::new(1).unwrap(),
        },
    ] {
        assert!(
            plan_original(
                &document,
                &entry,
                vec![
                    SemanticInstruction::SetMark { letter: 'a' },
                    call('a', 1),
                    instruction
                ],
                &bank,
                Some(index.clone())
            )
            .is_err()
        );
        assert_eq!(document, before);
        assert_eq!(bank, bank_before);
    }
    let recursive = BTreeMap::from([(name('a'), macro_value(vec![call('a', 1)]))]);
    assert!(
        plan_original(
            &document,
            &entry,
            vec![call('a', 1)],
            &recursive,
            Some(index.clone())
        )
        .unwrap_err()
        .message
        .contains("recursive")
    );
    let deep: BTreeMap<_, _> = ('a'..='q')
        .map(|letter| {
            let instruction = if letter == 'q' {
                motion(true, 1)
            } else {
                call(char::from_u32(u32::from(letter) + 1).unwrap(), 1)
            };
            (name(letter), macro_value(vec![instruction]))
        })
        .collect();
    assert_eq!(
        plan_original(
            &document,
            &entry,
            vec![call('a', 1)],
            &deep,
            Some(index.clone())
        )
        .unwrap_err()
        .code,
        EditErrorCode::LimitExceeded
    );
    let fuel = BTreeMap::from([(name('a'), macro_value(vec![motion(true, 1)]))]);
    assert_eq!(
        plan_original(&document, &entry, vec![call('a', 4096)], &fuel, Some(index))
            .unwrap_err()
            .code,
        EditErrorCode::LimitExceeded
    );
}
