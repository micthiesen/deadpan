//! An explicit whole-owner mute state follows the invocation's chosen Play.
use super::*;

fn mute(muted: bool) -> SemanticInstruction {
    SemanticInstruction::SetAudio {
        change: AudioChange::Mute { muted },
    }
}

fn muted(document: &ProjectDocument, context: &SemanticContext) -> bool {
    document.nodes()[&context.scoped.as_ref().unwrap().target.node]
        .audio_treatments
        .clip_gain()
        .is_some_and(|clip| clip.muted())
}

fn seeded(muted: bool) -> ProjectDocument {
    let mut document = fixture();
    let local = crate::GainRange::new(ExactRatio::ZERO, ExactRatio::integer(4)).unwrap();
    let envelope = crate::GainEnvelope::new(
        crate::GainClock::OwnerOutput,
        local,
        crate::GainDb::new(-3000).unwrap(),
        vec![
            crate::GainSegment::new(
                local.end(),
                crate::GainDb::new(3000).unwrap(),
                crate::GainCurve::Linear,
            )
            .unwrap(),
        ],
    )
    .unwrap();
    let clip = crate::ClipGain::new(
        crate::GainDb::new(6000).unwrap(),
        muted,
        vec![envelope],
        vec![crate::GainRange::new(ExactRatio::ONE, ExactRatio::integer(2)).unwrap()],
    )
    .unwrap();
    document.nodes.get_mut(&node("a")).unwrap().audio_treatments =
        crate::AudioTreatments::default()
            .with_clip_gain(clip)
            .unwrap()
            .with_saturation(Some(
                crate::Saturation::new(crate::GainDb::new(9000).unwrap()).unwrap(),
            ))
            .unwrap();
    document.validate().unwrap();
    document
}

#[test]
fn scoped_mute_true_isolates_one_nested_play_and_preserves_gain_curves_order_with_exact_pools() {
    let before = seeded(false);
    let context = entry();
    let target = &context.scoped.as_ref().unwrap().target;
    let expected = AudioChange::Mute { muted: true }
        .apply(&before.nodes()[&target.node].audio_treatments)
        .unwrap();
    let needs = before
        .scoped_edit_requirements(
            target,
            &crate::ScopedNodeEdit::SetAudioTreatments {
                treatments: expected.clone(),
            },
        )
        .unwrap();
    assert_eq!((needs.nodes, needs.marks), (10, 2));
    assert!(!muted(&before, &context));
    let snapshot = before.to_json().unwrap();
    let mut allocations = Vec::new();
    let changed = resolve(
        &before,
        &context,
        vec![mute(true)],
        &BTreeMap::new(),
        |request| {
            allocations.push(request.clone());
            allocator("mute", request)
        },
    )
    .expect("one explicit nested Play must admit whole-owner mute intent");
    assert_eq!(
        allocations,
        vec![SemanticAllocationRequest::Isolation {
            step_index: 0,
            nodes: needs.nodes,
            marks: needs.marks,
        }]
    );
    assert_context(&context, &changed.context);
    let mapped = changed.context.scoped.as_ref().unwrap();
    assert!(
        mapped
            .target
            .matches_instance(&changed.document, &mapped.presentation)
            .unwrap()
    );
    assert_eq!(
        changed.document.nodes()[&mapped.target.node].audio_treatments,
        expected
    );
    for (owner, authored) in changed.document.nodes() {
        assert_eq!(
            authored
                .audio_treatments
                .clip_gain()
                .is_some_and(|clip| clip.muted()),
            owner == &mapped.target.node
        );
    }
    assert_eq!(
        changed.document.nodes()[&node("a")],
        before.nodes()[&node("a")]
    );
    assert_eq!(
        changed.document.duration().unwrap(),
        before.duration().unwrap()
    );
    assert_eq!(before.to_json().unwrap(), snapshot);
    assert_reversible(&before, &changed);
}
#[test]
fn scoped_mute_first_last_true_false_and_owned_reentry_preserve_other_treatments() {
    for initial in [false, true] {
        let before = seeded(initial);
        for outer in [0, 2] {
            for inner in [0, 1] {
                let context = at_play(outer, inner);
                let first = resolve(
                    &before,
                    &context,
                    vec![mute(!initial)],
                    &BTreeMap::new(),
                    |request| allocator("first-mute", request),
                )
                .unwrap();
                assert_eq!(muted(&first.document, &first.context), !initial);
                assert_context(&context, &first.context);
                let expected = AudioChange::Mute { muted: !initial }
                    .apply(&before.nodes()[&node("a")].audio_treatments)
                    .unwrap();
                assert_eq!(
                    first.document.nodes()[&first.context.scoped.as_ref().unwrap().target.node]
                        .audio_treatments,
                    expected
                );
                let mut allocations = Vec::new();
                let second = resolve(
                    &first.document,
                    &first.context,
                    vec![mute(initial)],
                    &BTreeMap::new(),
                    |request| {
                        allocations.push(request.clone());
                        allocator("owned-mute", request)
                    },
                )
                .unwrap();
                assert_eq!(
                    allocations,
                    vec![SemanticAllocationRequest::Isolation {
                        step_index: 0,
                        nodes: 0,
                        marks: 0
                    }]
                );
                assert_eq!(muted(&second.document, &second.context), initial);
                assert_eq!(
                    second.document.nodes()[&second.context.scoped.as_ref().unwrap().target.node]
                        .audio_treatments,
                    before.nodes()[&node("a")].audio_treatments
                );
                assert_eq!(second.context, first.context);
                assert_eq!(second.document.nodes().len(), first.document.nodes().len());
                assert_eq!(second.document.marks(), first.document.marks());
                assert_eq!(second.document.overrides(), first.document.overrides());
                assert_eq!(
                    second.document.gap_overrides(),
                    first.document.gap_overrides()
                );
                assert_eq!(
                    second.document.nodes()[&node("a")],
                    before.nodes()[&node("a")]
                );
                assert_reversible(&before, &first);
                assert_reversible(&first.document, &second);
            }
        }
    }
}

#[test]
fn scoped_mute_counted_nested_calls_apply_explicit_booleans_to_each_staged_target() {
    let before = seeded(false);
    let bank = BTreeMap::from([
        (name('a'), macro_value(vec![call('b', 2)])),
        (
            name('b'),
            macro_value(vec![mute(true), step(3000), mute(false), step(-3000)]),
        ),
    ]);
    let frozen_bank = bank.clone();
    let mut allocations = Vec::new();
    let changed = resolve(&before, &entry(), vec![call('a', 1)], &bank, |request| {
        allocations.push(request.clone());
        allocator("mute-calls", request)
    })
    .unwrap();
    assert_eq!(allocations.len(), 8);
    for (index, request) in allocations.iter().enumerate() {
        assert_eq!(
            *request,
            SemanticAllocationRequest::Isolation {
                step_index: index,
                nodes: if index == 0 { 10 } else { 0 },
                marks: if index == 0 { 2 } else { 0 },
            }
        );
    }
    assert_context(&entry(), &changed.context);
    assert!(!muted(&changed.document, &changed.context));
    assert_eq!(
        changed.document.nodes()[&changed.context.scoped.as_ref().unwrap().target.node]
            .audio_treatments,
        before.nodes()[&node("a")].audio_treatments
    );
    let Command::Compound { transaction } = &changed.request.as_ref().unwrap().command else {
        panic!()
    };
    let states: Vec<_> = transaction
        .steps()
        .iter()
        .map(|step| {
            let Command::EditScoped {
                edit: crate::ScopedNodeEdit::SetAudioTreatments { treatments },
                ..
            } = step.edit().unwrap().command.as_command()
            else {
                panic!()
            };
            (
                treatments.clip_gain().unwrap().muted(),
                treatments.trim().millidecibels(),
            )
        })
        .collect();
    assert_eq!(
        states,
        [
            (true, 6000),
            (true, 9000),
            (false, 9000),
            (false, 6000),
            (true, 6000),
            (true, 9000),
            (false, 9000),
            (false, 6000)
        ]
    );
    assert_eq!(
        transaction.inputs()[&name('a')],
        Some(bank[&name('a')].clone())
    );
    assert_eq!(
        transaction.inputs()[&name('b')],
        Some(bank[&name('b')].clone())
    );
    assert_eq!(bank, frozen_bank);
    assert_reversible(&before, &changed);
    let intent = program(vec![mute(true)]);
    let json = serde_json::to_value(&intent).unwrap();
    let instruction = &json["instructions"][0];
    assert_eq!(instruction["change"]["muted"], true);
    assert!(!json.to_string().contains("\"node\""));
    let first = resolve(
        &before,
        &entry(),
        intent.instructions().to_vec(),
        &bank,
        |request| allocator("recorded-mute", request),
    )
    .unwrap();
    let other = at_play(2, 0);
    let repeated = resolve(
        &first.document,
        &other,
        intent.instructions().to_vec(),
        &bank,
        |request| allocator("dot-mute", request),
    )
    .unwrap();
    assert!(muted(&repeated.document, &repeated.context));
    assert!(muted(&repeated.document, &first.context));
    assert_context(&other, &repeated.context);
    assert_reversible(&first.document, &repeated);
}

#[test]
fn scoped_mute_same_state_and_counted_late_noop_refuse_atomically_before_further_allocation() {
    for state in [false, true] {
        let before = seeded(state);
        let snapshot = before.to_json().unwrap();
        let error = resolve(
            &before,
            &entry(),
            vec![mute(state)],
            &BTreeMap::new(),
            |_| panic!("same mute state allocated"),
        )
        .unwrap_err();
        assert_eq!(error.code, EditErrorCode::InvalidCommand);
        assert!(error.message.contains("that mute state; no edit was made"));
        let bank = BTreeMap::from([(name('a'), macro_value(vec![mute(!state)]))]);
        let frozen_bank = bank.clone();
        let mut allocations = 0;
        let error = resolve(&before, &entry(), vec![call('a', 2)], &bank, |request| {
            allocations += 1;
            allocator("late-mute", request)
        })
        .unwrap_err();
        assert_eq!(error.code, EditErrorCode::InvalidCommand);
        assert!(error.message.contains("that mute state; no edit was made"));
        assert_eq!(allocations, 1);
        assert_eq!(before.to_json().unwrap(), snapshot);
        assert_eq!(bank, frozen_bank);
    }
}

#[test]
fn scoped_mute_gain_pause_and_zoom_continue_on_the_new_pause_without_retargeting() {
    let before = fixture();
    let black = SemanticInstruction::InsertPause {
        length: crate::PauseLength::Frames {
            frames: NonZeroU32::new(2).unwrap(),
        },
        black: true,
    };
    let changed = resolve(
        &before,
        &entry(),
        vec![
            mute(true),
            step(3000),
            black,
            mute(true),
            mute(false),
            zoom(2),
        ],
        &BTreeMap::new(),
        super::super::super::allocate,
    )
    .unwrap();
    assert_context(&entry(), &changed.context);
    let target = &changed.context.scoped.as_ref().unwrap().target.node;
    let NodeKind::Hold { recipe } = &changed.document.nodes()[target].kind else {
        panic!()
    };
    assert_eq!(recipe.duration.frames(), 2);
    assert!(!muted(&changed.document, &changed.context));
    assert!(changed.document.nodes()[target].framing.is_some());
    assert_eq!(
        changed.document.nodes()[&node("a")],
        before.nodes()[&node("a")]
    );
    let leaves: Vec<_> = changed
        .trace
        .iter()
        .filter(|row| !matches!(row.instruction, SemanticInstruction::Call { .. }))
        .collect();
    assert_eq!(leaves[0].after.scoped, leaves[1].before.scoped);
    assert_eq!(leaves[2].after.scoped, leaves[3].before.scoped);
    assert_eq!(leaves[3].after.scoped, leaves[4].before.scoped);
    assert_ne!(leaves[1].after.scoped, leaves[3].after.scoped);
    assert_reversible(&before, &changed);
    // A whole owner may be an inner Sequence, not only a physical leaf.
    let mut group_context = entry();
    group_context.scoped.as_mut().unwrap().target.node = node("inside");
    group_context.scoped.as_mut().unwrap().presentation.node = node("inside");
    let group = resolve(
        &before,
        &group_context,
        vec![mute(true)],
        &BTreeMap::new(),
        |request| allocator("mute-group", request),
    )
    .unwrap();
    let owner = &group.context.scoped.as_ref().unwrap().target.node;
    assert!(matches!(
        group.document.nodes()[owner].kind,
        NodeKind::Sequence { .. }
    ));
    assert!(muted(&group.document, &group.context));
    assert_eq!(
        group.document.nodes()[&node("inside")],
        before.nodes()[&node("inside")]
    );
    assert_context(&group_context, &group.context);
    assert_reversible(&before, &group);
}

#[test]
fn scoped_mute_stale_mixed_visual_authority_and_wrong_pools_refuse_without_fallback() {
    let before = seeded(false);
    let snapshot = before.to_json().unwrap();
    for variant in 0..6 {
        let mut context = entry();
        let scoped = context.scoped.as_mut().unwrap();
        match variant {
            0 => scoped.target.repeats[0].branch = RepeatEditBranch::Default,
            1 => {
                scoped.target.repeats.pop();
            }
            2 => scoped.presentation.repeats[1].iteration = iteration("inner", 99),
            3 => scoped.presentation.node = node("b"),
            4 => {
                context.visual_selection = Some(SemanticVisualSelection::Time {
                    anchor: ProjectFrame(28),
                    head: ProjectFrame(29),
                    extending: false,
                })
            }
            5 => scoped.root = node("inner"),
            _ => unreachable!(),
        }
        assert!(
            resolve(
                &before,
                &context,
                vec![mute(true)],
                &BTreeMap::new(),
                |_| panic!("invalid mute authority allocated")
            )
            .is_err()
        );
    }
    let changed = resolve(
        &before,
        &entry(),
        vec![mute(true)],
        &BTreeMap::new(),
        |request| allocator("stale-mute", request),
    )
    .unwrap();
    assert!(
        resolve(
            &changed.document,
            &entry(),
            vec![mute(false)],
            &BTreeMap::new(),
            |_| panic!("stale mute template allocated")
        )
        .is_err()
    );
    for variant in 0..4 {
        let error = resolve(
            &before,
            &entry(),
            vec![mute(true)],
            &BTreeMap::new(),
            |request| {
                let mut allocation = allocator("bad-mute-pool", request)?;
                let SemanticAllocation::Isolation { identities, .. } = &mut allocation else {
                    panic!()
                };
                match variant {
                    0 => {
                        identities.nodes.pop();
                    }
                    1 => identities.nodes.push(node("extra-mute-node")),
                    2 => {
                        identities.marks.pop();
                    }
                    3 => identities
                        .marks
                        .push(MarkId::new("extra-mute-mark").unwrap()),
                    _ => unreachable!(),
                }
                Ok(allocation)
            },
        )
        .unwrap_err();
        assert!(error.message.contains("exact isolation identity pools"));
    }
    assert_eq!(before.to_json().unwrap(), snapshot);
}
