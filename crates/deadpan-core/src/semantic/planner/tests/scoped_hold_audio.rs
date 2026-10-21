//! Value-only pause sound intent follows the isolated concrete Hold.
use super::*;
use crate::{
    AssetId, AssetRecord, AudioSample, SourceAudio, SourceAudioMapping, SourceQualificationId,
    SourceSpan, SourceTimeBase, SourceTimestamp,
};

fn audio_fixture() -> (ProjectDocument, SourceAudio, SourceNode) {
    let mut document = fixture();
    let asset = AssetId::new("original").unwrap();
    let time_base = SourceTimeBase::new(1, 48_000).unwrap();
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base,
        },
        SourceTimestamp {
            ticks: 96_000,
            time_base,
        },
    )
    .unwrap();
    document.assets.insert(
        asset.clone(),
        AssetRecord {
            label: "Original".into(),
            content_hash: "a".repeat(64),
            audio: Some(span),
            video: None,
            frame_count: None,
            still_image: false,
            source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()),
        },
    );
    document.validate().unwrap();
    let audio = SourceAudio { asset, span };
    let moment = SourceNode {
        duration: FrameDuration::new(30).unwrap(),
        edit_window: Some(
            crate::SourceEditWindow::new(ExactRatio::ZERO, ExactRatio::integer(30)).unwrap(),
        ),
        video: crate::SourceVideo::Blank,
        video_mapping: crate::SourceVideoMapping::FitBeat,
        audio: Some(audio.clone()),
        audio_mapping: SourceAudioMapping::Placement {
            start: ExactRatio::new(-1, 3).unwrap(),
            frames: ExactRatio::integer(60),
        },
        link: crate::LinkRelation::Independent,
        audio_offset: AudioSample(0),
    };
    (document, audio, moment)
}

fn sound(audio: HoldAudio) -> SemanticInstruction {
    SemanticInstruction::SetHoldAudio { audio }
}

fn tone(audio: &SourceAudio) -> SemanticInstruction {
    sound(HoldAudio::RoomTone {
        source: audio.clone(),
    })
}

fn original(audio: &SourceAudio) -> Arc<RegisterValue> {
    Arc::new(RegisterValue::Original {
        revision: revision("base"),
        asset: audio.asset.clone(),
        qualification: SourceQualificationId::new("b".repeat(64)).unwrap(),
        ordinals: 0..30,
    })
}

fn plan(
    document: &ProjectDocument,
    context: &SemanticContext,
    instructions: Vec<SemanticInstruction>,
    bank: &BTreeMap<RegisterName, Arc<RegisterValue>>,
    allocator: impl FnMut(SemanticAllocationRequest) -> Result<SemanticAllocation, EditError>,
    resolver: impl FnMut(&ProjectDocument, &RegisterValue) -> Result<SourceNode, EditError>,
) -> Result<SemanticPlan, EditError> {
    plan_semantic_with_framing(
        document,
        context,
        &program(instructions),
        SemanticRegisterBank {
            entries: bank,
            version: 3,
        },
        revision(&format!("{}-audio-outer", document.revision_id())),
        allocator,
        resolver,
        |_| Err(speech_unavailable()),
        |_, _| panic!("sound or black intent must not sample a picture"),
        None,
        state,
    )
}

fn recipe<'a>(document: &'a ProjectDocument, context: &SemanticContext) -> &'a HoldRecipe {
    let node = &context.scoped.as_ref().unwrap().target.node;
    let NodeKind::Hold { recipe } = &document.nodes()[node].kind else {
        panic!()
    };
    recipe
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
fn scoped_hold_audio_first_last_nested_plays_keep_clocks_provider_selection_and_exact_pools() {
    let (before, audio, _) = audio_fixture();
    for outer in [0, 2] {
        for inner in [0, 1] {
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
            context.cursor = ProjectFrame(18 * i64::from(outer) + 2 + 7 * i64::from(inner) + 2);
            let needs = before
                .scoped_edit_requirements(
                    &context.scoped.as_ref().unwrap().target,
                    &crate::ScopedNodeEdit::SetHoldAudio {
                        audio: HoldAudio::RoomTone {
                            source: audio.clone(),
                        },
                    },
                )
                .unwrap();
            let mut allocations = vec![];
            let changed = plan(
                &before,
                &context,
                vec![tone(&audio)],
                &BTreeMap::new(),
                |request| {
                    allocations.push(request.clone());
                    allocator("audio", request)
                },
                no_original,
            )
            .unwrap();
            assert_eq!(
                allocations,
                vec![SemanticAllocationRequest::Isolation {
                    step_index: 0,
                    nodes: needs.nodes,
                    marks: needs.marks
                }]
            );
            assert_eq!(
                changed.document.duration().unwrap(),
                before.duration().unwrap()
            );
            assert_context(&context, &changed.context);
            let mapped = changed.context.scoped.as_ref().unwrap();
            assert!(
                mapped
                    .target
                    .matches_instance(&changed.document, &mapped.presentation)
                    .unwrap()
            );
            assert_ne!(mapped.target.node, node("a"));
            assert_eq!(
                changed.document.nodes()[&node("a")],
                before.nodes()[&node("a")]
            );
            assert_eq!(
                changed.document.marks().len(),
                before.marks().len() + needs.marks
            );
            let mut expected = recipe(&before, &context).clone();
            expected.audio = HoldAudio::RoomTone {
                source: audio.clone(),
            };
            assert_eq!(recipe(&changed.document, &changed.context), &expected);
            assert_reversible(&before, &changed);
            // Once this play owns the Hold, another value change needs no clones.
            let silent = plan(
                &changed.document,
                &changed.context,
                vec![sound(HoldAudio::Silence)],
                &BTreeMap::new(),
                |request| {
                    assert!(matches!(
                        request,
                        SemanticAllocationRequest::Isolation {
                            nodes: 0,
                            marks: 0,
                            ..
                        }
                    ));
                    allocator("silent", request)
                },
                no_original,
            )
            .unwrap();
            assert_eq!(silent.context.scoped, changed.context.scoped);
            assert_context(&context, &silent.context);
            assert_eq!(
                recipe(&silent.document, &silent.context).audio,
                HoldAudio::Silence
            );
            assert_reversible(&changed.document, &silent);
        }
    }
}

#[test]
fn scoped_hold_audio_noop_invalid_authority_and_wrong_pools_do_not_mutate_inputs() {
    let (before, audio, _) = audio_fixture();
    let context = entry();
    let unchanged = plan(
        &before,
        &context,
        vec![sound(HoldAudio::Silence)],
        &BTreeMap::new(),
        |_| panic!("no-op allocated"),
        no_original,
    )
    .unwrap();
    assert!(unchanged.request.is_none());
    assert_eq!(unchanged.document, before);
    assert_eq!(unchanged.context, context);
    let mut mixed = context.clone();
    mixed.scoped.as_mut().unwrap().target.repeats[0].branch = RepeatEditBranch::Default;
    let mut stale = context.clone();
    stale.scoped.as_mut().unwrap().presentation.repeats[1]
        .iteration
        .ordinal = 99;
    let mut group = context.clone();
    group.scoped.as_mut().unwrap().target.node = node("inside");
    group.scoped.as_mut().unwrap().presentation.node = node("inside");
    for invalid in [mixed, stale, group] {
        assert!(
            plan(
                &before,
                &invalid,
                vec![tone(&audio)],
                &BTreeMap::new(),
                |_| panic!("invalid target allocated"),
                no_original
            )
            .is_err()
        );
    }
    for extra in [false, true] {
        let error = plan(
            &before,
            &context,
            vec![tone(&audio)],
            &BTreeMap::new(),
            |request| {
                let mut allocation = allocator("bad", request)?;
                let SemanticAllocation::Isolation { identities, .. } = &mut allocation else {
                    panic!()
                };
                if extra {
                    identities.marks.push(MarkId::new("extra").unwrap());
                } else {
                    identities.nodes.pop().unwrap();
                }
                Ok(allocation)
            },
            no_original,
        )
        .unwrap_err();
        assert!(error.message.contains("exact isolation identity pools"));
    }
    assert_eq!(before, audio_fixture().0);
}

#[test]
fn scoped_room_tone_snapshots_original_and_copies_exact_measured_samples_before_allocation() {
    let (before, audio, moment) = audio_fixture();
    let bank = BTreeMap::from([(name('t'), original(&audio))]);
    let instruction = SemanticInstruction::SetRoomTone {
        register: name('t'),
    };
    let mut resolves = 0;
    let changed = plan(
        &before,
        &entry(),
        vec![instruction.clone()],
        &bank,
        |request| allocator("room", request),
        |staged, value| {
            assert_eq!(staged, &before);
            assert_eq!(value, bank[&name('t')].as_ref());
            resolves += 1;
            Ok(moment.clone())
        },
    )
    .unwrap();
    assert_eq!(resolves, 1);
    let HoldAudio::RoomTone { source } = &recipe(&changed.document, &changed.context).audio else {
        panic!()
    };
    assert_eq!(source.asset, audio.asset);
    assert_eq!(source.span.start().ticks, 534);
    assert_eq!(source.span.end().ticks, 48_533);
    let Command::Compound { transaction } = &changed.request.as_ref().unwrap().command else {
        panic!()
    };
    assert_eq!(
        transaction.inputs()[&name('t')],
        Some(bank[&name('t')].clone())
    );
    assert_eq!(transaction.expected_bank_version(), 3);
    assert_context(&entry(), &changed.context);
    assert_reversible(&before, &changed);
    for invalid_bank in [
        BTreeMap::new(),
        BTreeMap::from([(name('t'), macro_value(vec![zoom(2)]))]),
    ] {
        assert!(
            plan(
                &before,
                &entry(),
                vec![instruction.clone()],
                &invalid_bank,
                |_| panic!("bad register allocated"),
                |_, _| panic!("bad register resolved")
            )
            .is_err()
        );
    }
    for bad_mapping in [
        SourceAudioMapping::FitBeat,
        SourceAudioMapping::Placement {
            start: ExactRatio::integer(100),
            frames: ExactRatio::ONE,
        },
    ] {
        let mut invalid = moment.clone();
        invalid.audio_mapping = bad_mapping;
        assert!(
            plan(
                &before,
                &entry(),
                vec![instruction.clone()],
                &bank,
                |_| panic!("unmeasured span allocated"),
                |_, _| Ok(invalid.clone())
            )
            .is_err()
        );
    }
    let mut invalid = moment;
    invalid.audio_offset = AudioSample(1);
    assert!(
        plan(
            &before,
            &entry(),
            vec![instruction],
            &bank,
            |_| panic!("offset copy allocated"),
            |_, _| Ok(invalid.clone())
        )
        .is_err()
    );
}

#[test]
fn scoped_hold_audio_named_calls_follow_staged_hold_through_zoom_pause_and_late_refusal() {
    let (before, audio, moment) = audio_fixture();
    let bank = BTreeMap::from([
        (name('t'), original(&audio)),
        (name('a'), macro_value(vec![call('b', 1)])),
        (
            name('b'),
            macro_value(vec![
                SemanticInstruction::SetRoomTone {
                    register: name('t'),
                },
                sound(HoldAudio::Silence),
                zoom(2),
            ]),
        ),
    ]);
    let frozen_bank = bank.clone();
    let black = SemanticInstruction::InsertPause {
        length: crate::PauseLength::Frames {
            frames: NonZeroU32::new(2).unwrap(),
        },
        black: true,
    };
    let instructions = vec![call('a', 1), black, tone(&audio)];
    let changed = plan(
        &before,
        &entry(),
        instructions.clone(),
        &bank,
        super::super::allocate,
        |_, _| Ok(moment.clone()),
    )
    .unwrap();
    assert_eq!(changed.document.duration().unwrap().frames(), 56);
    assert_context(&entry(), &changed.context);
    assert_eq!(
        recipe(&changed.document, &changed.context)
            .duration
            .frames(),
        2
    );
    assert_eq!(
        recipe(&changed.document, &changed.context).audio,
        HoldAudio::RoomTone {
            source: audio.clone()
        }
    );
    let leaves: Vec<_> = changed
        .trace
        .iter()
        .filter(|row| !matches!(row.instruction, SemanticInstruction::Call { .. }))
        .collect();
    for pair in leaves.windows(2) {
        assert_eq!(pair[0].after.scoped, pair[1].before.scoped);
    }
    assert_ne!(
        leaves[2].after.scoped, changed.context.scoped,
        "the final audio edit must follow the newly inserted pause"
    );
    assert_reversible(&before, &changed);
    let mut late = instructions;
    late.push(motion(true, 1));
    let mut allocated = 0;
    assert!(
        plan(
            &before,
            &entry(),
            late,
            &bank,
            |request| {
                allocated += 1;
                super::super::allocate(request)
            },
            |_, _| Ok(moment.clone())
        )
        .is_err()
    );
    assert!(allocated >= 4, "failure must happen after staged edits");
    assert_eq!(bank, frozen_bank);
    assert_eq!(before, audio_fixture().0);
}

#[test]
fn scoped_hold_audio_retime_accepts_value_edit_and_retires_only_selected_silent_issuer() {
    let (mut before, audio, _) = audio_fixture();
    let sound_id = crate::SoundId::new("external").unwrap();
    let mut sound_source = audio.clone();
    sound_source.span = SourceSpan::new(
        sound_source.span.start(),
        SourceTimestamp {
            ticks: 48_000,
            time_base: sound_source.span.end().time_base,
        },
    )
    .unwrap();
    before.sounds.insert(
        sound_id.clone(),
        crate::SoundEvent {
            owner: node("root"),
            label: "external".into(),
            source: sound_source.clone(),
            mapping: SourceAudioMapping::natural_rate(
                sound_source.span,
                before.presentation_basis().frame_rate,
            )
            .unwrap(),
            offset: AudioSample(137),
            treatments: crate::AudioTreatments::default(),
            start_edge: crate::AudioEdgePolicy::Automatic,
            end_edge: crate::AudioEdgePolicy::Automatic,
            overflow: crate::SoundOverflowPolicy::Reject,
        },
    );
    before.sound_allowances.insert(
        sound_id.clone(),
        crate::SoundHoldAllowances::try_from(
            (0..3)
                .flat_map(|a| {
                    (0..2).map(move |b| crate::SoundHoldIssuer::Node {
                        instance: presentation(a, b),
                    })
                })
                .collect::<Vec<_>>(),
        )
        .unwrap(),
    );
    before.validate().unwrap();
    let changed = plan(
        &before,
        &entry(),
        vec![tone(&audio)],
        &BTreeMap::new(),
        |request| allocator("allowed", request),
        no_original,
    )
    .unwrap();
    assert_eq!(changed.document.sounds(), before.sounds());
    assert_eq!(changed.document.sound_routes(), before.sound_routes());
    assert_eq!(changed.document.sound_allowances()[&sound_id].len(), 5);
    let selected = &changed.context.scoped.as_ref().unwrap().target.node;
    for issuer in changed.document.sound_allowances()[&sound_id].iter() {
        assert_ne!(&issuer.instance().node, selected);
    }
    let silent = plan(
        &changed.document,
        &changed.context,
        vec![sound(HoldAudio::Silence)],
        &BTreeMap::new(),
        |request| allocator("again", request),
        no_original,
    )
    .unwrap();
    assert_eq!(
        silent.document.sound_allowances(),
        changed.document.sound_allowances()
    );
    assert_reversible(&before, &changed);
    // A value edit does not apply duration's temporal-policy exclusions. This
    // fractional Retime also carries existing captured clocks through isolation.
    let mut retimed = audio_fixture().0;
    let mut wrapper = BeatNode::sequence("rate", vec![]);
    wrapper.kind = NodeKind::Retime {
        child: node("outer"),
        duration: FrameDuration::new(17).unwrap(),
        mapping: range(1, 53),
        pitch: crate::PitchPolicy::Preserve,
        purpose: crate::RetimePurpose::Edit,
    };
    retimed.nodes.insert(node("rate"), wrapper);
    retimed
        .nodes
        .insert(node("root"), BeatNode::sequence("Root", vec![node("rate")]));
    retimed.audio_bindings = crate::capture_unbound_audio_bindings(
        &retimed,
        crate::AudioTimingId {
            allocation: revision("clock"),
            ordinal: 0,
        },
    )
    .unwrap();
    retimed.validate().unwrap();
    let mut context = entry();
    context.selected_child = Some(node("rate"));
    context.scoped.as_mut().unwrap().root = node("rate");
    context.cursor = ProjectFrame(8);
    let changed = plan(
        &retimed,
        &context,
        vec![tone(&audio)],
        &BTreeMap::new(),
        |request| allocator("rate", request),
        no_original,
    )
    .unwrap();
    assert_context(&context, &changed.context);
    assert_eq!(changed.document.duration().unwrap().frames(), 17);
    assert_eq!(
        changed.document.audio_bindings().timings,
        retimed.audio_bindings().timings
    );
    assert_reversible(&retimed, &changed);
}
