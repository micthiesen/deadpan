//! Relative whole-owner gain intent follows one isolated concrete occurrence.
use super::*;
use crate::AudioChange;

#[path = "scoped_mute.rs"]
mod scoped_mute;

fn step(millidecibels: i32) -> SemanticInstruction {
    SemanticInstruction::SetAudio {
        change: AudioChange::Step { millidecibels },
    }
}

fn assert_context(before: &SemanticContext, after: &SemanticContext) {
    let mut expected = before.clone();
    expected.scoped = after.scoped.clone();
    assert_eq!(&expected, after);
}

fn assert_reversible(before: &ProjectDocument, plan: &SemanticPlan) {
    let replay =
        crate::replay_compound::<EditError>(before, plan.request.as_ref().unwrap(), |_| Ok(()))
            .unwrap();
    assert_eq!(replay.document, plan.document);
    assert_eq!(replay.edit.inverse.apply(&plan.document).unwrap(), *before);
    assert_eq!(
        ProjectDocument::from_json(&plan.document.to_json().unwrap()).unwrap(),
        plan.document
    );
}

#[test]
fn scoped_gain_step_isolates_one_nested_play_with_exact_pools_and_one_inverse() {
    let before = fixture();
    let context = entry();
    let target = &context.scoped.as_ref().unwrap().target;
    let treatments = AudioChange::Step {
        millidecibels: 3000,
    }
    .apply(&before.nodes()[&target.node].audio_treatments)
    .unwrap();
    let needs = before
        .scoped_edit_requirements(
            target,
            &crate::ScopedNodeEdit::SetAudioTreatments { treatments },
        )
        .unwrap();
    assert_eq!((needs.nodes, needs.marks), (10, 2));
    assert_eq!(
        before.nodes()[&node("a")]
            .audio_treatments
            .trim()
            .millidecibels(),
        0
    );
    let snapshot = before.to_json().unwrap();
    let mut allocations = Vec::new();
    let plan = resolve(
        &before,
        &context,
        vec![step(3000)],
        &BTreeMap::new(),
        |request| {
            allocations.push(request.clone());
            allocator("gain", request)
        },
    )
    .expect("one explicit nested Play must admit a relative whole-owner gain step");
    assert_eq!(before.to_json().unwrap(), snapshot);
    assert_eq!(
        allocations,
        vec![SemanticAllocationRequest::Isolation {
            step_index: 0,
            nodes: needs.nodes,
            marks: needs.marks,
        }]
    );
    assert_context(&context, &plan.context);
    let mapped = plan.context.scoped.as_ref().unwrap();
    assert!(
        mapped
            .target
            .matches_instance(&plan.document, &mapped.presentation)
            .unwrap()
    );
    assert_ne!(mapped.target.node, node("a"));
    for (owner, authored) in plan.document.nodes() {
        assert_eq!(
            authored.audio_treatments.trim().millidecibels(),
            if owner == &mapped.target.node {
                3000
            } else {
                0
            }
        );
    }
    assert_eq!(
        plan.document.nodes()[&node("a")],
        before.nodes()[&node("a")]
    );
    assert_eq!(
        plan.document.duration().unwrap(),
        before.duration().unwrap()
    );
    assert_reversible(&before, &plan);
}
fn gain(document: &ProjectDocument, context: &SemanticContext) -> i32 {
    document.nodes()[&context.scoped.as_ref().unwrap().target.node]
        .audio_treatments
        .trim()
        .millidecibels()
}

fn at_play(outer: u32, inner: u32) -> SemanticContext {
    let mut context = entry();
    let within = presentation(outer, inner);
    let scoped = context.scoped.as_mut().unwrap();
    scoped.presentation = within.clone();
    scoped.target.repeats = within
        .repeats
        .iter()
        .map(|step| RepeatEditStep {
            repeat: step.node.clone(),
            branch: RepeatEditBranch::Play {
                iteration: step.iteration.clone(),
            },
        })
        .collect();
    context.cursor = ProjectFrame(18 * i64::from(outer) + 2 + 7 * i64::from(inner) + 1);
    context
}

#[test]
fn scoped_gain_first_last_nested_plays_reuse_owned_branches_without_allocating_more_nodes() {
    let before = fixture();
    for outer in [0, 2] {
        for inner in [0, 1] {
            let context = at_play(outer, inner);
            let first = resolve(
                &before,
                &context,
                vec![step(3000)],
                &BTreeMap::new(),
                |request| allocator("first", request),
            )
            .unwrap();
            assert_eq!(gain(&first.document, &first.context), 3000);
            assert_context(&context, &first.context);
            let mut requests = Vec::new();
            let second = resolve(
                &first.document,
                &first.context,
                vec![step(-3000)],
                &BTreeMap::new(),
                |request| {
                    requests.push(request.clone());
                    allocator("second", request)
                },
            )
            .unwrap();
            assert_eq!(
                requests,
                vec![SemanticAllocationRequest::Isolation {
                    step_index: 0,
                    nodes: 0,
                    marks: 0
                }]
            );
            assert_eq!(gain(&second.document, &second.context), 0);
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

#[test]
fn scoped_gain_counted_nested_calls_resolve_each_staged_treatment_and_keep_saved_intent_relative() {
    let mut before = fixture();
    let seed = crate::AudioTreatments::default()
        .with_trim(crate::GainDb::new(-6000).unwrap())
        .unwrap()
        .with_saturation(Some(
            crate::Saturation::new(crate::GainDb::new(9000).unwrap()).unwrap(),
        ))
        .unwrap();
    let seed = AudioChange::Mute { muted: true }.apply(&seed).unwrap();
    before.nodes.get_mut(&node("a")).unwrap().audio_treatments = seed.clone();
    before.validate().unwrap();
    let bank = BTreeMap::from([
        (name('a'), macro_value(vec![call('b', 2)])),
        (name('b'), macro_value(vec![step(3000)])),
    ]);
    let frozen_bank = bank.clone();
    let mut requests = Vec::new();
    let changed = resolve(
        &before,
        &entry(),
        vec![step(-3000), call('a', 2), zoom(2)],
        &bank,
        |request| {
            requests.push(request.clone());
            allocator("calls", request)
        },
    )
    .unwrap();
    assert_eq!(requests.len(), 6);
    for (index, request) in requests.iter().enumerate() {
        assert_eq!(
            *request,
            SemanticAllocationRequest::Isolation {
                step_index: index,
                nodes: if index == 0 { 10 } else { 0 },
                marks: if index == 0 { 2 } else { 0 },
            }
        );
    }
    assert_eq!(gain(&changed.document, &changed.context), 3000);
    let final_treatments = &changed.document.nodes()
        [&changed.context.scoped.as_ref().unwrap().target.node]
        .audio_treatments;
    assert_eq!(
        *final_treatments,
        AudioChange::Step {
            millidecibels: 9000
        }
        .apply(&seed)
        .unwrap()
    );
    assert_context(&entry(), &changed.context);
    let Command::Compound { transaction } = &changed.request.as_ref().unwrap().command else {
        panic!()
    };
    let authored_gains: Vec<_> = transaction
        .steps()
        .iter()
        .filter_map(|step| match step.edit()?.command.as_command() {
            Command::EditScoped {
                edit: crate::ScopedNodeEdit::SetAudioTreatments { treatments },
                ..
            } => Some(treatments.trim().millidecibels()),
            _ => None,
        })
        .collect();
    assert_eq!(authored_gains, [-9000, -6000, -3000, 0, 3000]);
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
    // Dot's saved program is the relative Step, without any captured owner or
    // final trim. Reinvocation on a different current play starts from -6dB.
    let intent = program(vec![step(3000)]);
    let json = serde_json::to_string(&intent).unwrap();
    assert!(!json.contains("\"node\""));
    assert!(!json.contains("\"repeats\""));
    let other_context = at_play(2, 0);
    let repeated = resolve(
        &before,
        &other_context,
        intent.instructions().to_vec(),
        &bank,
        |request| allocator("dot", request),
    )
    .unwrap();
    assert_eq!(gain(&repeated.document, &repeated.context), -3000);
    assert_context(&other_context, &repeated.context);
}

#[test]
fn scoped_gain_follows_pause_continuation_and_preserves_whole_composite_owner_effects() {
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
        vec![step(3000), black, step(-3000), zoom(2)],
        &BTreeMap::new(),
        super::super::allocate,
    )
    .unwrap();
    let mapped = changed.context.scoped.as_ref().unwrap();
    assert_eq!(gain(&changed.document, &changed.context), -3000);
    let NodeKind::Hold { recipe } = &changed.document.nodes()[&mapped.target.node].kind else {
        panic!()
    };
    assert_eq!(recipe.duration.frames(), 2);
    assert!(
        changed.document.nodes()[&mapped.target.node]
            .framing
            .is_some()
    );
    assert_context(&entry(), &changed.context);
    assert_reversible(&before, &changed);
    let mut composite = entry();
    composite.scoped.as_mut().unwrap().target.node = node("inside");
    composite.scoped.as_mut().unwrap().presentation.node = node("inside");
    let changed = resolve(
        &before,
        &composite,
        vec![step(3000)],
        &BTreeMap::new(),
        |request| allocator("group", request),
    )
    .unwrap();
    let target = &changed.context.scoped.as_ref().unwrap().target.node;
    assert!(matches!(
        changed.document.nodes()[target].kind,
        NodeKind::Sequence { .. }
    ));
    assert_eq!(gain(&changed.document, &changed.context), 3000);
    assert_eq!(
        changed.document.nodes()[&node("inside")]
            .audio_treatments
            .trim()
            .millidecibels(),
        0
    );
    assert_context(&composite, &changed.context);
    assert_reversible(&before, &changed);
}

#[test]
fn scoped_gain_bounds_and_late_failure_keep_document_and_bank_atomic() {
    for (start, delta, end) in [(21_000, 3000, 24_000), (-93_000, -3000, -96_000)] {
        let mut before = fixture();
        before.nodes.get_mut(&node("a")).unwrap().audio_treatments =
            crate::AudioTreatments::default()
                .with_trim(crate::GainDb::new(start).unwrap())
                .unwrap();
        before.validate().unwrap();
        let changed = resolve(
            &before,
            &entry(),
            vec![step(delta)],
            &BTreeMap::new(),
            |request| allocator("endpoint", request),
        )
        .unwrap();
        assert_eq!(gain(&changed.document, &changed.context), end);
        let error = resolve(
            &changed.document,
            &changed.context,
            vec![step(delta)],
            &BTreeMap::new(),
            |_| panic!("invalid gain allocated"),
        )
        .unwrap_err();
        assert!(error.message.contains("gain must be between"));
        let bank = BTreeMap::from([(name('a'), macro_value(vec![step(delta)]))]);
        let frozen_bank = bank.clone();
        let snapshot = before.to_json().unwrap();
        let mut allocations = 0;
        let error = resolve(
            &before,
            &entry(),
            vec![step(delta), call('a', 1)],
            &bank,
            |request| {
                allocations += 1;
                allocator("late", request)
            },
        )
        .unwrap_err();
        assert!(error.message.contains("gain must be between"));
        assert_eq!(allocations, 1);
        assert_eq!(before.to_json().unwrap(), snapshot);
        assert_eq!(bank, frozen_bank);
        assert_reversible(&before, &changed);
    }
}

#[test]
fn scoped_gain_rejects_other_audio_changes_noop_and_malformed_authority_without_fallback() {
    let before = fixture();
    let snapshot = before.to_json().unwrap();
    for change in [
        AudioChange::Trim {
            gain: crate::GainDb::new(3000).unwrap(),
        },
        AudioChange::RangeStep {
            millidecibels: 3000,
        },
        AudioChange::Saturation {
            drive: Some(crate::GainDb::new(3000).unwrap()),
        },
        AudioChange::Balance {
            balance: Some(crate::StereoBalance::new(50).unwrap()),
        },
        AudioChange::Equalizer { recipe: None },
        AudioChange::WetSend {
            effect: crate::TailEffect::Reverb,
            level: Some(crate::GainDb::UNITY),
        },
        AudioChange::TailSend {
            effect: crate::TailEffect::Delay,
            level: Some(crate::GainDb::UNITY),
        },
    ] {
        let forbidden = SemanticInstruction::SetAudio { change };
        let error = resolve(
            &before,
            &entry(),
            vec![forbidden.clone()],
            &BTreeMap::new(),
            |_| panic!("unsupported audio change allocated"),
        )
        .unwrap_err();
        assert!(error.message.contains("only Zoom"));
        let bank = BTreeMap::from([(name('a'), macro_value(vec![forbidden]))]);
        let frozen_bank = bank.clone();
        let mut allocations = 0;
        assert!(
            resolve(
                &before,
                &entry(),
                vec![step(3000), call('a', 1)],
                &bank,
                |request| {
                    allocations += 1;
                    allocator("late-kind", request)
                }
            )
            .is_err()
        );
        assert_eq!(allocations, 1);
        assert_eq!(bank, frozen_bank);
    }
    assert!(
        resolve(
            &before,
            &entry(),
            vec![step(0)],
            &BTreeMap::new(),
            |_| panic!("zero step allocated")
        )
        .unwrap_err()
        .message
        .contains("no edit was made")
    );
    for variant in 0..8 {
        let mut context = entry();
        let scoped = context.scoped.as_mut().unwrap();
        match variant {
            0 => scoped.target.repeats[0].branch = RepeatEditBranch::Default,
            1 => {
                scoped.target.repeats.pop();
            }
            2 => {
                scoped.target.repeats[1].branch = RepeatEditBranch::Play {
                    iteration: iteration("inner", 99),
                }
            }
            3 => scoped.presentation.repeats[0].iteration = iteration("outer", 0),
            4 => scoped.root = node("inner"),
            5 => {
                context.visual_selection = Some(SemanticVisualSelection::Time {
                    anchor: ProjectFrame(28),
                    head: ProjectFrame(29),
                    extending: false,
                })
            }
            6 => {
                context.original = Some(SemanticOriginalContext {
                    asset: crate::AssetId::new("absent").unwrap(),
                    qualification: crate::SourceQualificationId::new("a".repeat(64)).unwrap(),
                    ordinal: crate::SourceFrameId(0),
                })
            }
            7 => {
                scoped.target.node = node("missing");
                scoped.presentation.node = node("missing");
            }
            _ => unreachable!(),
        }
        assert!(
            resolve(
                &before,
                &context,
                vec![step(3000)],
                &BTreeMap::new(),
                |_| panic!("invalid authority allocated")
            )
            .is_err(),
            "variant {variant}"
        );
    }
    let changed = resolve(
        &before,
        &entry(),
        vec![step(3000)],
        &BTreeMap::new(),
        |request| allocator("stale", request),
    )
    .unwrap();
    assert!(
        resolve(
            &changed.document,
            &entry(),
            vec![step(3000)],
            &BTreeMap::new(),
            |_| panic!("stale template allocated")
        )
        .is_err()
    );
    for variant in 0..4 {
        let error = resolve(
            &before,
            &entry(),
            vec![step(3000)],
            &BTreeMap::new(),
            |request| {
                let mut allocated = allocator("bad-pool", request)?;
                let SemanticAllocation::Isolation { identities, .. } = &mut allocated else {
                    panic!()
                };
                match variant {
                    0 => {
                        identities.nodes.pop();
                    }
                    1 => identities.nodes.push(node("extra-node")),
                    2 => {
                        identities.marks.pop();
                    }
                    3 => identities.marks.push(MarkId::new("extra-mark").unwrap()),
                    _ => unreachable!(),
                }
                Ok(allocated)
            },
        )
        .unwrap_err();
        assert!(error.message.contains("exact isolation identity pools"));
    }
    assert_eq!(before.to_json().unwrap(), snapshot);
}
