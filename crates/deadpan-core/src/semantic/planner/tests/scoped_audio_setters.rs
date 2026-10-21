//! Literal whole-owner audio setters isolate the invocation's concrete Play.
//!
//! Register as a child of scoped_gain.rs to reuse the nested scoped fixture.
use super::*;

fn db(value: i32) -> crate::GainDb {
    crate::GainDb::new(value).unwrap()
}

fn eq() -> crate::EqualizerRecipe {
    crate::EqualizerRecipe::new(
        [db(6000), db(-3000), db(9000)],
        Some(crate::EqCutoffHz::new(80).unwrap()),
        None,
    )
    .unwrap()
}

fn seeded_setters() -> ProjectDocument {
    let mut before = fixture();
    let local = crate::GainRange::new(ExactRatio::ZERO, ExactRatio::integer(4)).unwrap();
    let envelope = crate::GainEnvelope::new(
        crate::GainClock::OwnerOutput,
        local,
        db(-3000),
        vec![crate::GainSegment::new(local.end(), db(3000), crate::GainCurve::Linear).unwrap()],
    )
    .unwrap();
    let clip = crate::ClipGain::new(
        db(6000),
        false,
        vec![envelope],
        vec![crate::GainRange::new(ExactRatio::ONE, ExactRatio::integer(2)).unwrap()],
    )
    .unwrap();
    // A legal nondefault order makes accidental reconstruction visible.
    before.nodes.get_mut(&node("a")).unwrap().audio_treatments =
        crate::AudioTreatments::with_equalizer_stages(
            vec![
                crate::AudioTreatmentStage::Saturation,
                crate::AudioTreatmentStage::Equalizer,
                crate::AudioTreatmentStage::ClipGain,
                crate::AudioTreatmentStage::StereoBalance,
            ],
            Some(clip),
            Some(crate::Saturation::new(db(9000)).unwrap()),
            Some(crate::StereoBalance::new(-25).unwrap()),
            Some(eq()),
        )
        .unwrap();
    before.validate().unwrap();
    before
}

fn assert_setter(change: AudioChange) {
    let before = seeded_setters();
    let context = entry();
    let scoped = context.scoped.as_ref().unwrap();
    assert!(
        scoped
            .target
            .matches_instance(&before, &scoped.presentation)
            .unwrap()
    );
    let seed = &before.nodes()[&node("a")].audio_treatments;
    let expected = change.apply(seed).unwrap();
    assert_ne!(&expected, seed, "the witness must change a valid treatment");
    assert_eq!(expected.order(), seed.order());
    let expected_clip = expected.clip_gain().unwrap();
    let seed_clip = seed.clip_gain().unwrap();
    assert_eq!(expected_clip.envelopes(), seed_clip.envelopes());
    assert_eq!(expected_clip.mute_ranges(), seed_clip.mute_ranges());
    assert_eq!(expected_clip.muted(), seed_clip.muted());
    assert_eq!(expected.tail_sends(), seed.tail_sends());
    assert_eq!(expected.wet_sends(), seed.wet_sends());
    match change {
        AudioChange::Trim { gain } => {
            assert_eq!(expected.trim(), gain);
            assert_eq!(expected.saturation(), seed.saturation());
            assert_eq!(expected.balance(), seed.balance());
            assert_eq!(expected.equalizer(), seed.equalizer());
        }
        AudioChange::Saturation { drive } => {
            assert_eq!(expected.saturation().map(crate::Saturation::drive), drive);
            assert_eq!(expected.clip_gain(), seed.clip_gain());
            assert_eq!(expected.balance(), seed.balance());
            assert_eq!(expected.equalizer(), seed.equalizer());
        }
        AudioChange::Balance { balance } => {
            assert_eq!(expected.balance(), balance);
            assert_eq!(expected.clip_gain(), seed.clip_gain());
            assert_eq!(expected.saturation(), seed.saturation());
            assert_eq!(expected.equalizer(), seed.equalizer());
        }
        AudioChange::Equalizer { recipe } => {
            assert_eq!(expected.equalizer(), recipe);
            assert_eq!(expected.clip_gain(), seed.clip_gain());
            assert_eq!(expected.saturation(), seed.saturation());
            assert_eq!(expected.balance(), seed.balance());
        }
        _ => panic!("this witness covers only literal recipe setters"),
    }

    let edit = crate::ScopedNodeEdit::SetAudioTreatments {
        treatments: expected.clone(),
    };
    let needs = before
        .scoped_edit_requirements(&scoped.target, &edit)
        .unwrap();
    assert_eq!((needs.nodes, needs.marks), (10, 2));
    let allocation_request = SemanticAllocationRequest::Isolation {
        step_index: 0,
        nodes: needs.nodes,
        marks: needs.marks,
    };
    let SemanticAllocation::Isolation {
        new_revision,
        identities,
    } = allocator("setter", allocation_request.clone()).unwrap()
    else {
        panic!("the scoped fixture requires exact isolation identities")
    };
    let raw_request = crate::CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision,
        command: Command::EditScoped {
            target: scoped.target.clone(),
            edit,
            identities,
        },
    };
    let snapshot = before.to_json().unwrap();
    // Public backend/pool success is a required prefix: an old-code failure
    // must identify semantic admission, not invalid DSP values or scope.
    let raw = crate::prepare_scoped_edit(&before, &raw_request).unwrap();
    assert_eq!(
        raw.document.nodes()[&raw.target.node].audio_treatments,
        expected
    );
    assert_eq!(raw.document.duration().unwrap(), before.duration().unwrap());
    assert_eq!(raw.document.audio_bindings(), before.audio_bindings());
    assert_eq!(
        raw.transaction.inverse.apply(&raw.document).unwrap(),
        before
    );
    assert_eq!(
        ProjectDocument::from_json(&raw.document.to_json().unwrap()).unwrap(),
        raw.document
    );
    for outer in 0..3 {
        for inner in 0..2 {
            let mapped = raw
                .map_instance(&before, &presentation(outer, inner))
                .unwrap();
            assert_eq!(
                raw.document.nodes()[&mapped.node].audio_treatments,
                if outer == 1 && inner == 1 {
                    &expected
                } else {
                    seed
                }
                .clone()
            );
            let peer = InstancePath {
                node: node("b"),
                ..presentation(outer, inner)
            };
            let mapped_peer = raw.map_instance(&before, &peer).unwrap();
            assert_eq!(
                raw.document.nodes()[&mapped_peer.node].audio_treatments,
                before.nodes()[&node("b")].audio_treatments
            );
        }
    }
    assert_eq!(before.to_json().unwrap(), snapshot);

    let mut allocations = Vec::new();
    let plan = resolve(
        &before,
        &context,
        vec![SemanticInstruction::SetAudio { change }],
        &BTreeMap::new(),
        |request| {
            allocations.push(request.clone());
            allocator("setter", request)
        },
    )
    .expect("an explicit nested Play must admit this whole-owner literal audio setter");
    assert_eq!(allocations, vec![allocation_request]);
    assert_context(&context, &plan.context);
    let mapped = plan.context.scoped.as_ref().unwrap();
    assert_eq!(mapped.target, raw.target);
    assert_eq!(
        mapped.presentation,
        raw.map_instance(&before, &scoped.presentation).unwrap()
    );
    assert!(
        mapped
            .target
            .matches_instance(&plan.document, &mapped.presentation)
            .unwrap()
    );
    assert_eq!(
        plan.document.nodes()[&mapped.target.node].audio_treatments,
        expected
    );
    // Same pool identities make the independently executed backend result an
    // exact authored-content oracle; only the outer Compound revision differs.
    assert_eq!(plan.document.nodes(), raw.document.nodes());
    assert_eq!(plan.document.marks(), raw.document.marks());
    assert_eq!(plan.document.overrides(), raw.document.overrides());
    assert_eq!(plan.document.gap_overrides(), raw.document.gap_overrides());
    assert_eq!(plan.document.audio_bindings(), before.audio_bindings());
    assert_eq!(
        plan.document.duration().unwrap(),
        before.duration().unwrap()
    );
    for (owner, authored) in before.nodes() {
        assert_eq!(
            &plan.document.nodes()[owner],
            authored,
            "shared template {owner}"
        );
    }
    assert!(plan.register_writes.is_empty());
    assert_eq!(before.to_json().unwrap(), snapshot);
    assert_reversible(&before, &plan);
}

#[test]
fn scoped_trim_literal_isolates_one_play_and_preserves_peer_treatments_and_timing() {
    assert_setter(AudioChange::Trim { gain: db(-6250) });
}

#[test]
fn scoped_saturation_literal_isolates_one_play_and_preserves_peer_treatments_and_timing() {
    assert_setter(AudioChange::Saturation {
        drive: Some(db(12000)),
    });
}

#[test]
fn scoped_balance_literal_isolates_one_play_and_preserves_peer_treatments_and_timing() {
    assert_setter(AudioChange::Balance {
        balance: Some(crate::StereoBalance::new(40).unwrap()),
    });
}

#[test]
fn scoped_equalizer_literal_isolates_one_play_and_preserves_peer_treatments_and_timing() {
    assert_setter(AudioChange::Equalizer {
        recipe: Some(
            crate::EqualizerRecipe::new(
                [db(-6000), db(3000), db(-9000)],
                Some(crate::EqCutoffHz::new(100).unwrap()),
                Some(crate::EqCutoffHz::new(12000).unwrap()),
            )
            .unwrap(),
        ),
    });
}

fn instruction(change: AudioChange) -> SemanticInstruction {
    SemanticInstruction::SetAudio { change }
}

fn replacement_eq() -> crate::EqualizerRecipe {
    crate::EqualizerRecipe::new(
        [db(-6000), db(3000), db(-9000)],
        Some(crate::EqCutoffHz::new(100).unwrap()),
        Some(crate::EqCutoffHz::new(12000).unwrap()),
    )
    .unwrap()
}

fn setters() -> [AudioChange; 4] {
    [
        AudioChange::Trim { gain: db(-6250) },
        AudioChange::Saturation {
            drive: Some(db(12000)),
        },
        AudioChange::Balance {
            balance: Some(crate::StereoBalance::new(40).unwrap()),
        },
        AudioChange::Equalizer {
            recipe: Some(replacement_eq()),
        },
    ]
}

fn treatments<'a>(
    document: &'a ProjectDocument,
    context: &SemanticContext,
) -> &'a crate::AudioTreatments {
    &document.nodes()[&context.scoped.as_ref().unwrap().target.node].audio_treatments
}

#[test]
fn scoped_audio_setters_first_last_and_owned_plays_replace_remove_and_recreate_stages() {
    let off = [
        AudioChange::Trim { gain: db(0) },
        AudioChange::Saturation { drive: None },
        AudioChange::Balance { balance: None },
        AudioChange::Equalizer { recipe: None },
    ];
    for (change, remove) in setters().into_iter().zip(off) {
        for outer in [0, 2] {
            for inner in [0, 1] {
                let before = seeded_setters();
                let context = at_play(outer, inner);
                let seed = &before.nodes()[&node("a")].audio_treatments;
                let first = resolve(
                    &before,
                    &context,
                    vec![instruction(change)],
                    &BTreeMap::new(),
                    |request| allocator("replace-stage", request),
                )
                .unwrap();
                assert_eq!(
                    treatments(&first.document, &first.context),
                    &change.apply(seed).unwrap()
                );
                assert_context(&context, &first.context);
                assert_reversible(&before, &first);
                let mut prior = first;
                for (prefix, value) in [("remove-stage", remove), ("recreate-stage", change)] {
                    let expected = value
                        .apply(treatments(&prior.document, &prior.context))
                        .unwrap();
                    let mut allocations = Vec::new();
                    let next = resolve(
                        &prior.document,
                        &prior.context,
                        vec![instruction(value)],
                        &BTreeMap::new(),
                        |request| {
                            allocations.push(request.clone());
                            allocator(prefix, request)
                        },
                    )
                    .unwrap();
                    assert_eq!(
                        allocations,
                        [SemanticAllocationRequest::Isolation {
                            step_index: 0,
                            nodes: 0,
                            marks: 0,
                        }]
                    );
                    assert_eq!(treatments(&next.document, &next.context), &expected);
                    assert_eq!(next.context, prior.context);
                    assert_eq!(next.document.nodes().len(), prior.document.nodes().len());
                    assert_eq!(next.document.marks(), prior.document.marks());
                    assert_eq!(next.document.overrides(), prior.document.overrides());
                    assert_eq!(
                        next.document.gap_overrides(),
                        prior.document.gap_overrides()
                    );
                    assert_eq!(
                        next.document.audio_bindings(),
                        prior.document.audio_bindings()
                    );
                    assert_eq!(
                        next.document.duration().unwrap(),
                        prior.document.duration().unwrap()
                    );
                    assert_eq!(
                        next.document.nodes()[&node("a")],
                        before.nodes()[&node("a")]
                    );
                    assert_reversible(&prior.document, &next);
                    prior = next;
                }
            }
        }
    }
}

#[test]
fn scoped_audio_setters_create_default_stage_order_on_a_group_without_changing_children() {
    let before = fixture();
    let mut context = entry();
    context.scoped.as_mut().unwrap().target.node = node("inside");
    context.scoped.as_mut().unwrap().presentation.node = node("inside");
    // Deliberately add stages in reverse order. Their normal insertion
    // positions, rather than command order, establish the canonical recipe.
    let changes = [setters()[2], setters()[3], setters()[1], setters()[0]];
    let plan = resolve(
        &before,
        &context,
        changes.into_iter().map(instruction).collect(),
        &BTreeMap::new(),
        |request| allocator("new-stage", request),
    )
    .unwrap();
    let owner = &plan.context.scoped.as_ref().unwrap().target.node;
    assert!(matches!(
        plan.document.nodes()[owner].kind,
        NodeKind::Sequence { .. }
    ));
    assert_eq!(
        treatments(&plan.document, &plan.context).order(),
        [
            crate::AudioTreatmentStage::ClipGain,
            crate::AudioTreatmentStage::Equalizer,
            crate::AudioTreatmentStage::Saturation,
            crate::AudioTreatmentStage::StereoBalance,
        ]
    );
    let mut a = presentation(1, 1);
    let mut b = InstancePath {
        node: node("b"),
        ..a.clone()
    };
    let mut staged = before.clone();
    let Command::Compound { transaction } = &plan.request.as_ref().unwrap().command else {
        panic!()
    };
    for step in transaction.steps() {
        let leaf = step.edit().unwrap();
        let prepared = crate::prepare_scoped_edit(&staged, &leaf.request(&staged)).unwrap();
        a = prepared.map_instance(&staged, &a).unwrap();
        b = prepared.map_instance(&staged, &b).unwrap();
        staged = prepared.document;
    }
    assert_eq!(
        plan.document.nodes()[&a.node].audio_treatments,
        before.nodes()[&node("a")].audio_treatments
    );
    assert_eq!(
        plan.document.nodes()[&b.node].audio_treatments,
        before.nodes()[&node("b")].audio_treatments
    );
    assert_eq!(
        plan.document.nodes()[&node("inside")],
        before.nodes()[&node("inside")]
    );
    assert_context(&context, &plan.context);
    assert_reversible(&before, &plan);
}

#[test]
fn scoped_audio_setters_counted_nested_calls_resolve_staged_values_and_trim_then_step() {
    let before = seeded_setters();
    let cycle = vec![
        instruction(setters()[0]),
        step(3000),
        instruction(setters()[1]),
        instruction(setters()[2]),
        instruction(setters()[3]),
        instruction(AudioChange::Trim { gain: db(6000) }),
        instruction(AudioChange::Saturation {
            drive: Some(db(9000)),
        }),
        instruction(AudioChange::Balance {
            balance: Some(crate::StereoBalance::new(-25).unwrap()),
        }),
        instruction(AudioChange::Equalizer { recipe: Some(eq()) }),
    ];
    let bank = BTreeMap::from([
        (name('a'), macro_value(vec![call('b', 2)])),
        (name('b'), macro_value(cycle.clone())),
    ]);
    let frozen_bank = bank.clone();
    let snapshot = before.to_json().unwrap();
    let mut allocations = Vec::new();
    let plan = resolve(&before, &entry(), vec![call('a', 1)], &bank, |request| {
        allocations.push(request.clone());
        allocator("setter-calls", request)
    })
    .unwrap();
    assert_eq!(allocations.len(), 18);
    for (step_index, allocation) in allocations.iter().enumerate() {
        assert_eq!(
            *allocation,
            SemanticAllocationRequest::Isolation {
                step_index,
                nodes: if step_index == 0 { 10 } else { 0 },
                marks: if step_index == 0 { 2 } else { 0 },
            }
        );
    }
    let Command::Compound { transaction } = &plan.request.as_ref().unwrap().command else {
        panic!()
    };
    let mut expected = before.nodes()[&node("a")].audio_treatments.clone();
    for (leaf, intent) in transaction
        .steps()
        .iter()
        .zip(cycle.iter().cycle().take(18))
    {
        let SemanticInstruction::SetAudio { change } = intent else {
            panic!()
        };
        expected = change.apply(&expected).unwrap();
        let Command::EditScoped {
            edit: crate::ScopedNodeEdit::SetAudioTreatments { treatments },
            ..
        } = leaf.edit().unwrap().command.as_command()
        else {
            panic!()
        };
        assert_eq!(treatments, &expected);
    }
    assert_eq!(
        treatments(&plan.document, &plan.context),
        &before.nodes()[&node("a")].audio_treatments
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
    assert_eq!(before.to_json().unwrap(), snapshot);
    assert_context(&entry(), &plan.context);
    assert_reversible(&before, &plan);
    let saved = program(setters().into_iter().map(instruction).collect());
    let wire = serde_json::to_value(&saved).unwrap();
    assert!(!wire.to_string().contains("\"node\""));
    assert_eq!(
        serde_json::from_value::<SemanticProgram>(wire).unwrap(),
        saved
    );
    let repeated = resolve(
        &plan.document,
        &at_play(2, 0),
        saved.instructions().to_vec(),
        &bank,
        |request| allocator("dot-setters", request),
    )
    .unwrap();
    let expected = setters().into_iter().fold(
        before.nodes()[&node("a")].audio_treatments.clone(),
        |current, change| change.apply(&current).unwrap(),
    );
    assert_eq!(treatments(&repeated.document, &repeated.context), &expected);
    assert_eq!(
        treatments(&repeated.document, &plan.context),
        &before.nodes()[&node("a")].audio_treatments
    );
    assert_context(&at_play(2, 0), &repeated.context);
    assert_reversible(&plan.document, &repeated);
}

#[test]
fn scoped_audio_setters_same_values_and_late_noop_or_gain_overflow_refuse_atomically() {
    let before = seeded_setters();
    let snapshot = before.to_json().unwrap();
    let same = [
        AudioChange::Trim { gain: db(6000) },
        AudioChange::Saturation {
            drive: Some(db(9000)),
        },
        AudioChange::Balance {
            balance: Some(crate::StereoBalance::new(-25).unwrap()),
        },
        AudioChange::Equalizer { recipe: Some(eq()) },
    ];
    for (change, same) in setters().into_iter().zip(same) {
        let error = resolve(
            &before,
            &entry(),
            vec![instruction(same)],
            &BTreeMap::new(),
            |_| panic!("unchanged literal setter allocated"),
        )
        .unwrap_err();
        assert_eq!(error.code, EditErrorCode::InvalidCommand);
        assert!(
            error
                .message
                .contains("those audio treatments; no edit was made")
        );
        let bank = BTreeMap::from([(name('a'), macro_value(vec![instruction(change)]))]);
        let frozen = bank.clone();
        let mut allocations = 0;
        let error = resolve(&before, &entry(), vec![call('a', 2)], &bank, |request| {
            allocations += 1;
            allocator("late-setter-noop", request)
        })
        .unwrap_err();
        assert!(
            error
                .message
                .contains("those audio treatments; no edit was made")
        );
        assert_eq!(allocations, 1);
        assert_eq!(bank, frozen);
        assert_eq!(before.to_json().unwrap(), snapshot);
    }
    // Removal on a bare target is the same no-op policy, without isolation.
    for change in [
        AudioChange::Trim { gain: db(0) },
        AudioChange::Saturation { drive: None },
        AudioChange::Balance { balance: None },
        AudioChange::Equalizer { recipe: None },
    ] {
        assert!(
            resolve(
                &fixture(),
                &entry(),
                vec![instruction(change)],
                &BTreeMap::new(),
                |_| panic!("absent stage removal allocated"),
            )
            .unwrap_err()
            .message
            .contains("no edit was made")
        );
    }
    let bank = BTreeMap::from([(name('a'), macro_value(vec![step(3000)]))]);
    let frozen = bank.clone();
    let mut allocations = 0;
    let error = resolve(
        &before,
        &entry(),
        vec![
            instruction(AudioChange::Trim { gain: db(24000) }),
            call('a', 1),
        ],
        &bank,
        |request| {
            allocations += 1;
            allocator("late-setter-overflow", request)
        },
    )
    .unwrap_err();
    assert!(error.message.contains("gain must be between"));
    assert_eq!(allocations, 1);
    assert_eq!(bank, frozen);
    assert_eq!(before.to_json().unwrap(), snapshot);
}

#[test]
fn scoped_audio_setters_reject_invalid_authority_stale_capture_and_wrong_identity_pools() {
    let before = seeded_setters();
    let snapshot = before.to_json().unwrap();
    for change in setters() {
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
                    vec![instruction(change)],
                    &BTreeMap::new(),
                    |_| panic!("invalid setter authority allocated"),
                )
                .is_err(),
                "authority variant {variant}, change {change:?}"
            );
        }
        let first = resolve(
            &before,
            &entry(),
            vec![instruction(change)],
            &BTreeMap::new(),
            |request| allocator("stale-setter", request),
        )
        .unwrap();
        assert!(
            resolve(
                &first.document,
                &entry(),
                vec![instruction(change)],
                &BTreeMap::new(),
                |_| panic!("stale shared-template scope allocated"),
            )
            .is_err()
        );
        for variant in 0..4 {
            let error = resolve(
                &before,
                &entry(),
                vec![instruction(change)],
                &BTreeMap::new(),
                |request| {
                    let mut allocation = allocator("bad-setter-pool", request)?;
                    let SemanticAllocation::Isolation { identities, .. } = &mut allocation else {
                        panic!()
                    };
                    match variant {
                        0 => {
                            identities.nodes.pop();
                        }
                        1 => identities.nodes.push(node("extra-setter-node")),
                        2 => {
                            identities.marks.pop();
                        }
                        3 => identities
                            .marks
                            .push(MarkId::new("extra-setter-mark").unwrap()),
                        _ => unreachable!(),
                    }
                    Ok(allocation)
                },
            )
            .unwrap_err();
            assert!(error.message.contains("exact isolation identity pools"));
        }
    }
    assert_eq!(before.to_json().unwrap(), snapshot);
}

#[test]
fn scoped_audio_setters_continue_through_a_new_pause_and_framing_on_the_mapped_hold() {
    let before = fixture();
    let black = SemanticInstruction::InsertPause {
        length: crate::PauseLength::Frames {
            frames: NonZeroU32::new(2).unwrap(),
        },
        black: true,
    };
    let plan = resolve(
        &before,
        &entry(),
        vec![
            instruction(setters()[0]),
            black,
            instruction(setters()[0]),
            instruction(setters()[1]),
            instruction(setters()[2]),
            instruction(setters()[3]),
            step(3000),
            zoom(2),
        ],
        &BTreeMap::new(),
        super::super::super::allocate,
    )
    .unwrap();
    assert_context(&entry(), &plan.context);
    let target = &plan.context.scoped.as_ref().unwrap().target.node;
    let NodeKind::Hold { recipe } = &plan.document.nodes()[target].kind else {
        panic!()
    };
    assert_eq!(recipe.duration.frames(), 2);
    assert_eq!(treatments(&plan.document, &plan.context).trim(), db(-3250));
    assert_eq!(
        treatments(&plan.document, &plan.context).equalizer(),
        Some(replacement_eq())
    );
    assert_eq!(
        treatments(&plan.document, &plan.context).balance(),
        Some(crate::StereoBalance::new(40).unwrap())
    );
    assert_eq!(
        treatments(&plan.document, &plan.context).saturation(),
        Some(crate::Saturation::new(db(12000)).unwrap())
    );
    assert!(plan.document.nodes()[target].framing.is_some());
    assert_eq!(
        plan.document.nodes()[&node("a")],
        before.nodes()[&node("a")]
    );
    let leaves: Vec<_> = plan
        .trace
        .iter()
        .filter(|row| !matches!(row.instruction, SemanticInstruction::Call { .. }))
        .collect();
    assert_ne!(leaves[0].after.scoped, leaves[1].after.scoped);
    for pair in leaves.windows(2) {
        assert_eq!(pair[0].after.scoped, pair[1].before.scoped);
    }
    for row in &leaves[2..] {
        assert_eq!(row.before.scoped, plan.context.scoped);
        assert_eq!(row.after.scoped, plan.context.scoped);
    }
    assert_reversible(&before, &plan);
}

#[test]
fn scoped_audio_setters_preserve_existing_mute_automation_and_nonempty_sends() {
    let mut before = seeded_setters();
    let mut seed = before.nodes()[&node("a")].audio_treatments.clone();
    for change in [
        AudioChange::Mute { muted: true },
        AudioChange::WetSend {
            effect: crate::TailEffect::Reverb,
            level: Some(db(-9000)),
        },
        AudioChange::TailSend {
            effect: crate::TailEffect::Delay,
            level: Some(db(-6000)),
        },
    ] {
        seed = change.apply(&seed).unwrap();
    }
    before.nodes.get_mut(&node("a")).unwrap().audio_treatments = seed.clone();
    before.validate().unwrap();
    for change in setters() {
        let plan = resolve(
            &before,
            &entry(),
            vec![instruction(change)],
            &BTreeMap::new(),
            |request| allocator("retained-sends", request),
        )
        .unwrap();
        let actual = treatments(&plan.document, &plan.context);
        assert_eq!(actual, &change.apply(&seed).unwrap());
        assert_eq!(actual.wet_sends(), seed.wet_sends());
        assert_eq!(actual.tail_sends(), seed.tail_sends());
        assert!(actual.clip_gain().unwrap().muted());
        assert_eq!(
            actual.clip_gain().unwrap().envelopes(),
            seed.clip_gain().unwrap().envelopes()
        );
        assert_eq!(
            actual.clip_gain().unwrap().mute_ranges(),
            seed.clip_gain().unwrap().mute_ranges()
        );
        assert_eq!(plan.document.nodes()[&node("a")].audio_treatments, seed);
        assert_context(&entry(), &plan.context);
        assert_reversible(&before, &plan);
    }
}
