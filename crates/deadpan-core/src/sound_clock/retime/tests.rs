use super::*;
use crate::*;
use std::sync::Arc;

fn id(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}
fn revision(name: &str) -> RevisionId {
    RevisionId::new(name).unwrap()
}
fn sound(name: &str) -> SoundId {
    SoundId::new(name).unwrap()
}
fn frames(count: i64) -> FrameDuration {
    FrameDuration::new(count).unwrap()
}
fn ratio(a: i128, b: i128) -> ExactRatio {
    ExactRatio::new(a, b).unwrap()
}
fn hold(count: i64) -> BeatNode {
    BeatNode::hold(
        "Hold",
        HoldRecipe {
            duration: frames(count),
            video: HoldVideo::Background,
            picture_context: None,
            audio: HoldAudio::Silence,
        },
    )
}
fn fixture() -> ProjectDocument {
    let mut document = ProjectDocument::new(
        ProjectId::new("retime-proof").unwrap(),
        revision("before"),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    document.nodes = BTreeMap::from([
        (
            id("root"),
            BeatNode::sequence("Root", vec![id("lead"), id("top"), id("suffix")]),
        ),
        (id("lead"), hold(2)),
        (
            id("top"),
            BeatNode::sequence("Top", vec![id("owner"), id("tail")]),
        ),
        (id("owner"), hold(12)),
        (id("tail"), hold(2)),
        (id("suffix"), hold(2)),
    ])
    .into();
    let base = SourceTimeBase::new(1, 44_100).unwrap();
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: 10,
            time_base: base,
        },
        SourceTimestamp {
            ticks: 1010,
            time_base: base,
        },
    )
    .unwrap();
    document.assets.insert(
        AssetId::new("sound").unwrap(),
        AssetRecord {
            label: "Sound".into(),
            content_hash: "a".repeat(64),
            video: None,
            audio: Some(span),
            still_image: false,
            frame_count: None,
            source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()),
        },
    );
    let event = BeatSound {
        label: "Independent".into(),
        source: SourceAudio {
            asset: AssetId::new("sound").unwrap(),
            span,
        },
        mapping: SourceAudioMapping::natural_rate(span, document.presentation_basis.frame_rate)
            .unwrap(),
        offset: AudioSample(37),
        treatments: AudioTreatments::default(),
        start_edge: AudioEdgePolicy::Hard,
        end_edge: AudioEdgePolicy::Hard,
        overflow: SoundOverflowPolicy::Reject,
    };
    for owner in ["root", "top", "owner", "tail", "suffix"] {
        document
            .beat_sounds
            .insert(id(owner), BTreeMap::from([(sound(owner), event.clone())]));
    }
    document.validate().unwrap();
    document
}
fn wrap_command() -> Command {
    Command::WrapRetime {
        node: id("owner"),
        id: id("stage"),
        duration: frames(9),
        pitch: PitchPolicy::FollowSpeed,
    }
}
/// Produce only the structural intermediate the future transport must verify.
/// No command refusal is disabled and this is not installed into a project.
fn candidate(before: &ProjectDocument, command: &Command) -> ProjectDocument {
    let mut after = before.clone();
    after.revision_id = revision("candidate");
    match command {
        Command::WrapRetime {
            node,
            id,
            duration,
            pitch,
        } => {
            let mapping = FrameRange::new(
                ProjectFrame(0),
                ProjectFrame(before.recipe_structural_durations().unwrap()[node].frames()),
            )
            .unwrap();
            let parent = before.parent_of(node).unwrap();
            crate::command::replace_child(&mut after, &parent, node, id.clone()).unwrap();
            let mut wrapper = BeatNode::sequence("Retime", vec![]);
            wrapper.kind = NodeKind::Retime {
                child: node.clone(),
                duration: *duration,
                mapping,
                pitch: *pitch,
                purpose: RetimePurpose::Edit,
            };
            after.nodes.insert(id.clone(), wrapper);
        }
        Command::SetRetime {
            node,
            duration,
            pitch,
        } => {
            let NodeKind::Retime {
                duration: output,
                pitch: policy,
                ..
            } = &mut after.nodes.get_mut(node).unwrap().kind
            else {
                unreachable!()
            };
            *output = *duration;
            *policy = *pitch;
            if let Some(clock) = after.derived_clocks.get_mut(node) {
                clock.extent = ExactRatio::integer(duration.frames());
            }
        }
        _ => unreachable!(),
    }
    after
}
fn admitted(document: ProjectDocument) -> ValidatedDocument {
    ValidatedDocument::new(Arc::new(document)).unwrap()
}

#[test]
fn wrap_proves_exact_input_boundary_and_fresh_owner_cohorts_without_admitting_command() {
    let before = admitted(fixture());
    let command = wrap_command();
    let after = candidate(before.document(), &command);
    let proof = SoundRetimeTransition::derive(&before, &after, &command, MAX_AUDIO_BINDING_ENTRIES)
        .unwrap();
    assert!(proof.old_stage().is_none());
    assert!(!proof.is_no_op());
    assert!(std::ptr::eq(proof.before(), &before));
    assert!(std::ptr::eq(proof.after(), &after));
    let stage = proof.new_stage();
    assert_eq!(stage.stage(), &id("stage"));
    assert_eq!(stage.child(), &id("owner"));
    assert_eq!(stage.recipe_duration(), frames(9));
    assert_eq!(stage.pitch(), PitchPolicy::FollowSpeed);
    assert_eq!(stage.input_at(ExactRatio::ZERO).unwrap(), ExactRatio::ZERO);
    assert_eq!(
        stage.input_at(ratio(555, 32_032)).unwrap(),
        ratio(185, 8_008)
    );
    assert_eq!(
        stage.input_at(ExactRatio::integer(9)).unwrap(),
        ExactRatio::integer(12)
    );
    assert!(stage.input_at(ExactRatio::integer(10)).is_err());
    let kinds: BTreeMap<_, _> = proof
        .cohorts()
        .iter()
        .map(|cohort| (cohort.owner().as_str(), cohort.kind()))
        .collect();
    assert_eq!(kinds["owner"], SoundRetimeCohortKind::ChildInput);
    assert_eq!(kinds["top"], SoundRetimeCohortKind::AncestorOutput);
    assert_eq!(kinds["root"], SoundRetimeCohortKind::AncestorOutput);
    assert_eq!(kinds["tail"], SoundRetimeCohortKind::PlacementOnly);
    let owner = proof
        .cohorts()
        .iter()
        .find(|c| c.owner() == &id("owner"))
        .unwrap();
    assert_eq!(
        owner.after_ancestors(),
        &[&id("stage"), &id("top"), &id("root")]
    );
    assert!(owner.journal().is_none());
    let top = proof
        .cohorts()
        .iter()
        .find(|c| c.owner() == &id("top"))
        .unwrap();
    assert_eq!(top.before_extent(), ExactRatio::integer(14));
    assert_eq!(top.after_extent(), ExactRatio::integer(11));
    assert_eq!(
        owner.event(),
        &before.beat_sounds()[&id("owner")][&sound("owner")]
    );
    // The existing public path is still closed, even when this geometry proves.
    let request = CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: revision("refused"),
        command,
    };
    assert!(apply(before.document(), &request).is_err());
}

#[test]
fn stage_output_bypasses_updated_processor_and_no_op_keeps_exact_boundary() {
    let command = wrap_command();
    let base = candidate(&fixture(), &command);
    let mut base = base;
    let event = base.beat_sounds[&id("owner")][&sound("owner")].clone();
    base.beat_sounds
        .insert(id("stage"), BTreeMap::from([(sound("output"), event)]));
    let before = admitted(base);
    for pitch in [PitchPolicy::Preserve, PitchPolicy::Shift { semitones: 3 }] {
        let command = Command::SetRetime {
            node: id("stage"),
            duration: frames(6),
            pitch,
        };
        let after = candidate(before.document(), &command);
        let proof =
            SoundRetimeTransition::derive(&before, &after, &command, MAX_AUDIO_BINDING_ENTRIES)
                .unwrap();
        assert_eq!(proof.old_stage().unwrap().extent(), ExactRatio::integer(9));
        assert_eq!(proof.new_stage().extent(), ExactRatio::integer(6));
        assert_eq!(
            proof.new_stage().selection(),
            proof.old_stage().unwrap().selection()
        );
        assert_eq!(
            proof
                .cohorts()
                .iter()
                .find(|c| c.owner() == &id("stage"))
                .unwrap()
                .kind(),
            SoundRetimeCohortKind::StageOutput
        );
        assert_eq!(
            proof
                .cohorts()
                .iter()
                .find(|c| c.owner() == &id("owner"))
                .unwrap()
                .kind(),
            SoundRetimeCohortKind::ChildInput
        );
    }
    let command = Command::SetRetime {
        node: id("stage"),
        duration: frames(9),
        pitch: PitchPolicy::FollowSpeed,
    };
    let after = candidate(before.document(), &command);
    let proof = SoundRetimeTransition::derive(&before, &after, &command, MAX_AUDIO_BINDING_ENTRIES)
        .unwrap();
    assert!(proof.is_no_op());
    assert_eq!(proof.old_stage(), Some(proof.new_stage()));
}

#[test]
fn prior_journal_inventory_is_complete_borrowed_and_cannot_be_replaced() {
    let mut before = fixture();
    before.beat_sounds.retain(|owner, _| owner == &id("owner"));
    let mut layouts = Vec::new();
    let mut references = Vec::new();
    for ordinal in 0..2 {
        let timing = AudioTimingId {
            allocation: revision("history"),
            ordinal,
        };
        layouts.push(AudioTimingRecord {
            id: timing.clone(),
            layout: FrozenAudioLayout::capture(&before).unwrap(),
        });
        references.push(SoundClockReference::new(timing, id("top"), id("owner")));
    }
    let journal = SoundClockJournal::new(id("top"), references).unwrap();
    before.audio_bindings = AudioBindingState::new_with_sound_clocks(
        layouts,
        BTreeMap::from([(id("owner"), BTreeMap::from([(sound("owner"), journal)]))]),
    )
    .unwrap();
    let before = admitted(before);
    let command = wrap_command();
    let after = candidate(before.document(), &command);
    let proof = SoundRetimeTransition::derive(&before, &after, &command, MAX_AUDIO_BINDING_ENTRIES)
        .unwrap();
    let cohort = &proof.cohorts()[0];
    let journal = cohort.journal().unwrap();
    assert_eq!(journal.clocks().len(), 2);
    assert!(std::ptr::eq(
        journal,
        &before.audio_bindings().sound_clocks()[&id("owner")][&sound("owner")]
    ));
    // Old unchanged-subtree validation is still truthfully unavailable.
    assert!(after.validate().is_err());
    let mut forged = after.clone();
    forged.audio_bindings.sound_clocks.clear();
    assert!(
        SoundRetimeTransition::derive(&before, &forged, &command, MAX_AUDIO_BINDING_ENTRIES)
            .is_err()
    );
    let mut forged = after.clone();
    forged.audio_bindings.timings.clear();
    assert!(
        SoundRetimeTransition::derive(&before, &forged, &command, MAX_AUDIO_BINDING_ENTRIES)
            .is_err()
    );
}

#[test]
fn exact_effective_input_is_not_reconstructed_from_integral_recipe_counts() {
    // Existing sound admission rejects derived clocks. Exercise the exact
    // geometry through supported commands without claiming sound transport.
    let mut base = fixture();
    let retained_sound = base.beat_sounds[&id("owner")].clone();
    base.beat_sounds.clear();
    let request = CommandRequest {
        project_id: base.project_id().clone(),
        expected_revision: base.revision_id().clone(),
        new_revision: revision("wrapped"),
        command: wrap_command(),
    };
    let (_, base) = apply_with_result(&base, &request).unwrap();
    let at = ProjectFrame(3);
    let PauseInsertionTarget::Retimed(target) = base.pause_insertion_target(at).unwrap() else {
        panic!("pause must enter the authored speed change")
    };
    let request = CommandRequest {
        project_id: base.project_id().clone(),
        expected_revision: base.revision_id().clone(),
        new_revision: revision("paused"),
        command: Command::InsertTime {
            at,
            hold: match hold(2).kind {
                NodeKind::Hold { recipe } => recipe,
                _ => unreachable!(),
            },
            id: id("pause"),
            identities: SplitIdentities {
                nodes: (0..target.required_ids)
                    .map(|index| id(&format!("pause-split-{index}")))
                    .collect(),
            },
            timing: AudioTimingId {
                allocation: revision("paused"),
                ordinal: 0,
            },
        },
    };
    let (_, base) = apply_with_result(&base, &request).unwrap();
    let clock = base.derived_clocks()[&id("stage")].clone();
    assert_eq!(clock.extent, ratio(11, 1));
    assert_eq!(clock.child_end, ratio(44, 3));
    let mut unsupported = base.clone();
    unsupported.beat_sounds.insert(id("owner"), retained_sound);
    assert!(ValidatedDocument::new(Arc::new(unsupported)).is_err());
    let before = admitted(base);
    let command = Command::SetRetime {
        node: id("stage"),
        duration: frames(5),
        pitch: PitchPolicy::FollowSpeed,
    };
    let after = candidate(before.document(), &command);
    let proof = SoundRetimeTransition::derive(&before, &after, &command, MAX_AUDIO_BINDING_ENTRIES)
        .unwrap();
    assert_eq!(proof.old_stage().unwrap().extent(), clock.extent);
    assert_eq!(proof.new_stage().selection().start, clock.child_start);
    assert_eq!(
        proof.new_stage().input_at(ExactRatio::integer(5)).unwrap(),
        clock.child_end
    );
    let mut forged = after.clone();
    forged
        .derived_clocks
        .get_mut(&id("stage"))
        .unwrap()
        .child_end = ExactRatio::integer(15);
    assert!(
        SoundRetimeTransition::derive(&before, &forged, &command, MAX_AUDIO_BINDING_ENTRIES)
            .is_err()
    );
}

#[test]
fn forged_stage_child_pitch_source_and_sibling_changes_never_gain_a_proof() {
    let before = admitted(fixture());
    let command = wrap_command();
    let after = candidate(before.document(), &command);
    let mut cases = Vec::new();
    let mut wrong_child = after.clone();
    if let NodeKind::Retime { child, .. } =
        &mut wrong_child.nodes.get_mut(&id("stage")).unwrap().kind
    {
        *child = id("tail");
    }
    cases.push(wrong_child);
    let mut wrong_policy = after.clone();
    if let NodeKind::Retime { pitch, .. } =
        &mut wrong_policy.nodes.get_mut(&id("stage")).unwrap().kind
    {
        *pitch = PitchPolicy::Preserve;
    }
    cases.push(wrong_policy);
    let mut changed_sibling = after.clone();
    changed_sibling.nodes.get_mut(&id("tail")).unwrap().label = "changed".into();
    cases.push(changed_sibling);
    let mut changed_source = after.clone();
    changed_source
        .assets
        .get_mut(&AssetId::new("sound").unwrap())
        .unwrap()
        .content_hash = "c".repeat(64);
    cases.push(changed_source);
    for candidate in cases {
        assert!(
            SoundRetimeTransition::derive(&before, &candidate, &command, MAX_AUDIO_BINDING_ENTRIES)
                .is_err()
        );
    }
    assert!(
        SoundRetimeTransition::derive(
            &before,
            &after,
            &Command::Rename {
                node: id("owner"),
                label: "Retime".into(),
            },
            MAX_AUDIO_BINDING_ENTRIES
        )
        .is_err()
    );
}

#[test]
fn proof_charges_bounded_comparison_and_path_work_before_acceptance() {
    let before = admitted(fixture());
    let command = wrap_command();
    let after = candidate(before.document(), &command);
    let full = SoundRetimeTransition::derive(&before, &after, &command, MAX_AUDIO_BINDING_ENTRIES)
        .unwrap();
    assert!(full.work() > before.nodes().len() + after.nodes().len());
    assert_eq!(
        SoundRetimeTransition::derive(&before, &after, &command, full.work())
            .unwrap()
            .work(),
        full.work()
    );
    assert!(SoundRetimeTransition::derive(&before, &after, &command, full.work() - 1).is_err());
    assert!(SoundRetimeTransition::derive(&before, &after, &command, 0).is_err());
    assert!(
        SoundRetimeTransition::derive(&before, &after, &command, MAX_AUDIO_BINDING_ENTRIES + 1)
            .is_err()
    );
}

#[test]
fn timing_identity_skips_only_the_same_complete_immutable_allocation() {
    let mut document = fixture();
    document.beat_sounds.clear();
    document
        .nodes
        .retain(|node, _| node == &id("root") || node == &id("owner"));
    document
        .nodes
        .insert(id("root"), BeatNode::sequence("Root", vec![id("owner")]));
    let timing = AudioTimingId {
        allocation: revision("retained"),
        ordinal: 0,
    };
    let layout = FrozenAudioLayout::capture(&document).unwrap();
    let old = BTreeMap::from([(timing.clone(), layout.clone())]);
    let shared = BTreeMap::from([(timing.clone(), layout)]);
    let mut shared_budget = Budget::new(MAX_AUDIO_BINDING_ENTRIES).unwrap();
    assert!(same_timings(&old, &shared, &mut shared_budget).unwrap());

    let detached =
        FrozenAudioLayout::from_json(&serde_json::to_string(&old[&timing]).unwrap()).unwrap();
    assert!(!std::ptr::eq(&*old[&timing], &*detached));
    let detached = BTreeMap::from([(timing.clone(), detached)]);
    let mut bounded = Budget::new(shared_budget.used).unwrap();
    assert!(same_timings(&old, &detached, &mut bounded).is_err());
    let mut full = Budget::new(MAX_AUDIO_BINDING_ENTRIES).unwrap();
    assert!(same_timings(&old, &detached, &mut full).unwrap());
    assert!(full.used > shared_budget.used);

    // Same keys, node identities and topology do not establish equality of
    // an independently allocated historical body.
    document.nodes.insert(id("owner"), hold(13));
    let changed = BTreeMap::from([(timing, FrozenAudioLayout::capture(&document).unwrap())]);
    let mut full = Budget::new(MAX_AUDIO_BINDING_ENTRIES).unwrap();
    assert!(!same_timings(&old, &changed, &mut full).unwrap());
}

#[test]
fn default_and_owned_repeat_branches_keep_their_physical_boundary_without_play_expansion() {
    let mut base = fixture();
    let iterations = IterationOrder::new(revision("many-plays"), 100_000).unwrap();
    let mut repeated = BeatNode::sequence("Repeat", vec![]);
    repeated.kind = NodeKind::Repeat {
        child: id("owner"),
        iterations: iterations.clone(),
        gap: None,
        escalation: None,
    };
    base.nodes.insert(id("plays"), repeated);
    if let NodeKind::Sequence { children } = &mut base.nodes.get_mut(&id("top")).unwrap().kind {
        children[0] = id("plays");
    }
    base.nodes.insert(id("owned-play"), hold(6));
    base.overrides
        .entry(id("plays"))
        .or_default()
        .insert(iterations.at(1).unwrap(), id("owned-play"));
    let event = base.beat_sounds[&id("owner")][&sound("owner")].clone();
    base.beat_sounds
        .insert(id("owned-play"), BTreeMap::from([(sound("owned"), event)]));
    let before = admitted(base);
    let command = wrap_command();
    let after = candidate(before.document(), &command);
    let proof = SoundRetimeTransition::derive(&before, &after, &command, MAX_AUDIO_BINDING_ENTRIES)
        .unwrap();
    let default = proof
        .cohorts()
        .iter()
        .find(|c| c.owner() == &id("owner"))
        .unwrap();
    let owned = proof
        .cohorts()
        .iter()
        .find(|c| c.owner() == &id("owned-play"))
        .unwrap();
    assert_eq!(default.kind(), SoundRetimeCohortKind::ChildInput);
    assert_eq!(owned.kind(), SoundRetimeCohortKind::PlacementOnly);
    assert_eq!(default.after_ancestors()[..2], [&id("stage"), &id("plays")]);
    assert_eq!(owned.after_ancestors()[0], &id("plays"));
    assert_eq!(proof.cohorts().len(), 6);
    assert!(proof.work() < MAX_AUDIO_BINDING_ENTRIES);
}
