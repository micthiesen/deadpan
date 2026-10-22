//! An unchanged Retime output-owned sound is independent of its child signal.
//!
//! Intended child of insert_time/hold_duration/tests/processing.rs.
use super::*;

fn ratio(numerator: i128, denominator: i128) -> ExactRatio {
    ExactRatio::new(numerator, denominator).unwrap()
}

fn changed(before: &ProjectDocument, command: Command, revision: &str) -> ProjectDocument {
    let request = CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: rev(revision),
        command,
    };
    let transaction = apply(before, &request).unwrap();
    let after = transaction.forward.apply(before).unwrap();
    assert_eq!(transaction.inverse.apply(&after).unwrap(), *before);
    assert_eq!(
        ProjectDocument::from_json(&after.to_json().unwrap()).unwrap(),
        after
    );
    after
}

fn output_sound(previous_journal: bool, grouped: bool) -> (ProjectDocument, SoundId) {
    let mut before = processing(PitchPolicy::Preserve, 1, false);
    if grouped {
        before.nodes.insert(
            id("group"),
            BeatNode::sequence("Group", vec![id("processing"), id("tail")]),
        );
        before.nodes.insert(
            id("root"),
            BeatNode::sequence("Root", vec![id("lead"), id("group")]),
        );
        before.validate().unwrap();
    }
    // Reuse the existing fixture recipe, but admit the attachment through its
    // public command rather than introducing it during the target edit.
    let mut template = before.clone();
    let effect = attach_effect(&mut template, "processing");
    let event = template.beat_sounds[&id("processing")][&effect].clone();
    let attached = changed(
        &before,
        Command::SetBeatSound {
            owner: id("processing"),
            id: effect.clone(),
            event,
        },
        "attach-output-sound",
    );
    assert_eq!(attached.nodes(), before.nodes());
    let document = if previous_journal {
        // Existing supported control: move the entire unchanged stage by
        // growing the preceding Hold. This retains a genuine sound journal.
        changed(
            &attached,
            Command::SetHoldDuration {
                node: id("lead"),
                duration: frames(4),
            },
            "move-unchanged-stage",
        )
    } else {
        attached
    };
    (document, effect)
}

fn output_projection(document: &ProjectDocument) -> FrozenAudioProjection {
    FrozenAudioLayout::capture(document)
        .unwrap()
        .project(
            &instance("processing", &[]),
            ExactRatio::ZERO,
            None,
            MAX_DOCUMENT_NODES,
        )
        .unwrap()
}

fn assert_fixed_output_resize(length: i64, previous_journal: bool, grouped: bool) {
    let (before, effect) = output_sound(previous_journal, grouped);
    before.validate().unwrap();
    let snapshot = before.to_json().unwrap();
    let event = before.beat_sounds[&id("processing")][&effect].clone();
    let stage = before.nodes()[&id("processing")].clone();
    let origin = if previous_journal { 4 } else { 3 };
    assert_eq!(
        before.presentation_basis().frame_rate,
        FrameRate::new(30_000, 1001).unwrap()
    );
    assert_eq!(event.source.span, span(3100, 3200)); // 100 native 44.1-kHz samples.
    assert_eq!(event.offset, AudioSample(37));
    assert_eq!(
        event.mapping,
        SourceAudioMapping::natural_rate(event.source.span, before.presentation_basis().frame_rate)
            .unwrap()
    );
    assert_eq!(event.overflow, SoundOverflowPolicy::Reject);
    assert_eq!(event.start_edge, AudioEdgePolicy::Hard);
    assert_eq!(event.end_edge, AudioEdgePolicy::Hard);
    let NodeKind::Retime {
        child,
        duration,
        mapping,
        pitch,
        purpose,
    } = &stage.kind
    else {
        panic!("fixture must have a real processing stage")
    };
    assert_eq!(child, &id("body"));
    assert_eq!(*duration, frames(4));
    assert_eq!(
        *mapping,
        FrameRange::new(ProjectFrame(1), ProjectFrame(8)).unwrap()
    );
    assert_eq!(*pitch, PitchPolicy::Preserve);
    assert_eq!(*purpose, RetimePurpose::Edit);
    assert_eq!(before.node_duration(&id("body")).unwrap(), frames(9));
    let projection = output_projection(&before);
    assert_eq!(projection.origin, ExactRatio::integer(origin));
    assert_eq!(projection.frames_per_local_frame, ExactRatio::ONE);
    assert_eq!(projection.local_extent, ExactRatio::integer(4));
    assert_eq!(projection.local_duration, frames(4));
    assert_eq!(projection.instance, instance("processing", &[]));
    // No silent Hold is introduced by this fixture. Its independent event
    // signal, not its changing Original+RoomTone bus, is the PCM invariant.
    for owner in ["lead", "hold"] {
        assert!(matches!(
            &before.nodes()[&id(owner)].kind,
            NodeKind::Hold { recipe }
                if matches!(recipe.audio, HoldAudio::RoomTone { .. })
        ));
    }
    assert!(before.sound_allowances().is_empty());
    let selection = event
        .mapping
        .selection_frames_with_offset(
            frames(4),
            event.offset,
            before.presentation_basis().frame_rate,
        )
        .unwrap();
    assert_eq!(selection.start, ratio(185, 8008));
    assert_eq!(
        selection.end,
        ratio(185, 8008).checked_add(ratio(10000, 147147)).unwrap()
    );
    assert!(selection.start.compare_integer(0).is_gt());
    assert!(selection.end.compare_integer(4).is_lt());
    // Absolute root allocation is derived from the unchanged owner transform;
    // the offset stays an independent mix-sample offset, never native samples.
    let samples_per_frame = ratio(8008, 5);
    let start = projection
        .origin
        .checked_add(selection.start)
        .unwrap()
        .checked_mul(samples_per_frame)
        .unwrap()
        .round_even()
        .unwrap();
    let end = projection
        .origin
        .checked_add(selection.end)
        .unwrap()
        .checked_mul(samples_per_frame)
        .unwrap()
        .round_even()
        .unwrap();
    assert_eq!(
        (start, end),
        if previous_journal {
            (6443, 6552)
        } else {
            (4842, 4951)
        }
    );
    if previous_journal {
        let journal = &before.audio_bindings().sound_clocks()[&id("processing")][&effect];
        let scope = id(if grouped { "group" } else { "processing" });
        assert_eq!(journal.scope(), &scope);
        assert_eq!(journal.clocks().len(), 1);
        let clock = &journal.clocks()[0];
        assert_eq!(clock.scope(), &scope);
        assert_eq!(clock.owner(), &id("processing"));
        assert_eq!(clock.timing().allocation, rev("move-unchanged-stage"));
        assert_eq!(clock.timing().ordinal, 0);
    } else {
        assert!(before.audio_bindings().sound_clocks().is_empty());
    }

    // Unchanged-edit control comes first. Presence, 44.1-kHz mapping, offset,
    // existing journals and the ordinary command are independently admitted.
    let noop = changed(
        &before,
        Command::SetHoldDuration {
            node: id("hold"),
            duration: frames(2),
        },
        "unchanged-output-sound",
    );
    assert_eq!(noop.nodes(), before.nodes());
    assert_eq!(noop.beat_sounds, before.beat_sounds);
    assert_eq!(noop.audio_bindings(), before.audio_bindings());
    assert_eq!(output_projection(&noop), projection);
    assert_eq!(before.to_json().unwrap(), snapshot);

    // The exact same interior edit without that one attachment already works.
    // Deletion uses its public lifecycle and prunes only its sound journal.
    let no_event = changed(
        &before,
        Command::DeleteBeatSound {
            owner: id("processing"),
            id: effect.clone(),
        },
        "remove-output-sound-control",
    );
    assert!(no_event.beat_sounds.is_empty());
    let ordinary = changed(
        &no_event,
        Command::SetHoldDuration {
            node: id("hold"),
            duration: frames(length),
        },
        "resize-without-output-sound",
    );
    assert_eq!(ordinary.nodes()[&id("processing")], stage);
    assert_eq!(
        ordinary.node_duration(&id("body")).unwrap(),
        frames(7 + length)
    );
    assert_eq!(ordinary.duration().unwrap(), before.duration().unwrap());
    assert_eq!(output_projection(&ordinary).origin, projection.origin);
    assert_eq!(
        output_projection(&ordinary).frames_per_local_frame,
        projection.frames_per_local_frame
    );
    assert_eq!(
        output_projection(&ordinary).local_extent,
        projection.local_extent
    );
    // Complete Original-processing subtree identity genuinely changed.
    // The existing general correspondence must remain strict; only an
    // independent owner's output dependency proof may use the narrower fact.
    assert_eq!(
        FrozenAudioLayout::capture(&no_event)
            .unwrap()
            .sound_clock_correspondence(
                &FrozenAudioLayout::capture(&ordinary).unwrap(),
                &id("processing"),
                &id("processing"),
                MAX_DOCUMENT_NODES,
            )
            .unwrap_err()
            .message,
        "sound clock processing subtree changed"
    );

    let request = CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: rev("resize-with-output-sound"),
        command: Command::SetHoldDuration {
            node: id("hold"),
            duration: frames(length),
        },
    };
    // Old code must fail HERE, after every valid factory/control assertion.
    let transaction = apply(&before, &request).expect(
        "an unchanged stage-output-owned sound must not depend on its changed child signal",
    );
    let after = transaction.forward.apply(&before).unwrap();
    assert_eq!(after.nodes()[&id("processing")], stage);
    assert_eq!(after.beat_sounds, before.beat_sounds);
    if grouped && previous_journal {
        // This valid older capture includes the surrounding Sequence group.
        // The output-only proof narrows its dependency without inventing a new
        // clock or changing the historical owner, timing identity, or layout.
        let old = &before.audio_bindings().sound_clocks()[&id("processing")][&effect];
        let new = &after.audio_bindings().sound_clocks()[&id("processing")][&effect];
        assert_eq!(new.scope(), &id("processing"));
        assert_eq!(new.clocks().len(), old.clocks().len());
        for (old, new) in old.clocks().iter().zip(new.clocks()) {
            assert_eq!(new.scope(), &id("processing"));
            assert_eq!(new.owner(), old.owner());
            assert_eq!(new.timing(), old.timing());
            assert_eq!(new.repeats(), old.repeats());
            assert_eq!(
                after.audio_bindings().timings()[new.timing()],
                before.audio_bindings().timings()[old.timing()]
            );
        }
    } else {
        assert_eq!(
            after.audio_bindings().sound_clocks(),
            before.audio_bindings().sound_clocks()
        );
    }
    let current_projection = output_projection(&after);
    assert_eq!(current_projection.origin, projection.origin);
    assert_eq!(
        current_projection.frames_per_local_frame,
        projection.frames_per_local_frame
    );
    assert_eq!(current_projection.local_extent, projection.local_extent);
    assert_eq!(
        after.node_duration(&id("body")).unwrap(),
        frames(7 + length)
    );
    assert_eq!(after.duration().unwrap(), before.duration().unwrap());
    assert_eq!(after.marks(), before.marks());
    assert_eq!(after.nodes()[&id("lead")], before.nodes()[&id("lead")]);
    assert_eq!(after.nodes()[&id("tail")], before.nodes()[&id("tail")]);
    after.validate().unwrap();
    assert_eq!(transaction.inverse.apply(&after).unwrap(), before);
    assert_eq!(
        ProjectDocument::from_json(&after.to_json().unwrap()).unwrap(),
        after
    );
    assert_eq!(before.to_json().unwrap(), snapshot);
}

#[test]
fn fixed_retime_output_owned_sound_grow_ignores_changed_child_dependency() {
    assert_fixed_output_resize(3, false, false);
}

#[test]
fn fixed_retime_output_owned_sound_shrink_ignores_changed_child_dependency() {
    assert_fixed_output_resize(1, false, false);
}

#[test]
fn fixed_retime_output_owned_sound_grow_preserves_existing_output_journal() {
    assert_fixed_output_resize(3, true, false);
}

#[test]
fn fixed_retime_output_owned_sound_shrink_preserves_existing_output_journal() {
    assert_fixed_output_resize(1, true, false);
}

#[test]
fn fixed_retime_output_owned_sound_under_sequence_group_grow_ignores_child_dependency() {
    assert_fixed_output_resize(3, false, true);
}

#[test]
fn fixed_retime_output_owned_sound_under_sequence_group_shrink_narrows_existing_journal() {
    assert_fixed_output_resize(1, true, true);
}

#[test]
fn fixed_retime_descendant_owned_sound_keeps_changed_input_refusal() {
    let base = processing(PitchPolicy::Preserve, 1, false);
    let mut template = base.clone();
    let effect = attach_effect(&mut template, "after");
    let before = changed(
        &base,
        Command::SetBeatSound {
            owner: id("after"),
            id: effect.clone(),
            event: template.beat_sounds[&id("after")][&effect].clone(),
        },
        "attach-input-sound",
    );
    assert!(
        before
            .children(&id("body"))
            .any(|owner| owner == &id("after"))
    );
    assert_ne!(id("after"), id("processing"));
    let noop = changed(
        &before,
        Command::SetHoldDuration {
            node: id("hold"),
            duration: frames(2),
        },
        "unchanged-input-sound",
    );
    assert_eq!(noop.beat_sounds, before.beat_sounds);
    let snapshot = before.to_json().unwrap();
    for length in [1, 3] {
        let request = CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: rev("still-refused-input-sound"),
            command: Command::SetHoldDuration {
                node: id("hold"),
                duration: frames(length),
            },
        };
        let error = apply(&before, &request).unwrap_err();
        assert_eq!(error.code, EditErrorCode::InvalidCommand);
        assert_eq!(error.message, "sound clock processing subtree changed");
        assert_eq!(before.to_json().unwrap(), snapshot);
    }
}

#[test]
fn fixed_retime_output_proof_maps_only_owner_and_explicit_repeat_ancestry() {
    let mut before = processing(PitchPolicy::Preserve, 1, false);
    before.marks.clear();
    let mut repeat = BeatNode::sequence("Repeat", vec![]);
    repeat.kind = NodeKind::Repeat {
        child: id("processing"),
        iterations: IterationOrder::new(rev("output-plays"), 2).unwrap(),
        gap: None,
        escalation: None,
    };
    before.nodes.insert(id("repeat"), repeat);
    before.nodes.insert(
        id("root"),
        BeatNode::sequence("Root", vec![id("lead"), id("repeat"), id("tail")]),
    );
    before.validate().unwrap();
    let after = resized(&before, "hold", 3);
    let old = FrozenAudioLayout::capture(&before).unwrap();
    let new = FrozenAudioLayout::capture(&after).unwrap();
    let repeats = SoundClockRepeatMap::new(vec![SoundClockRepeatStep::Shared {
        live_repeat: id("repeat"),
        historical_repeat: id("repeat"),
    }])
    .unwrap();
    let proof = old
        .sound_owner_clock_correspondence_with_repeats(
            &new,
            &id("processing"),
            &id("processing"),
            (&id("processing"), &id("processing")),
            &repeats,
            MAX_DOCUMENT_NODES,
        )
        .unwrap();
    assert_eq!(
        proof
            .node_pairs()
            .map(|(live, old)| (live.clone(), old.clone()))
            .collect::<Vec<_>>(),
        vec![
            (id("processing"), id("processing")),
            (id("repeat"), id("repeat"))
        ]
    );
    for ordinal in [0, 1] {
        let owner = InstancePath {
            node: id("processing"),
            repeats: vec![RepeatInstance {
                node: id("repeat"),
                iteration: IterationId {
                    allocation: rev("output-plays"),
                    ordinal,
                },
            }],
        };
        assert_eq!(proof.remap_instance(&owner).unwrap(), owner);
        let child = InstancePath {
            node: id("after"),
            ..owner
        };
        assert!(proof.historical_node(&child.node).is_none());
        assert!(proof.remap_instance(&child).is_err());
    }
    assert_eq!(
        old.sound_clock_correspondence_with_repeats(
            &new,
            &id("processing"),
            &id("processing"),
            &repeats,
            MAX_DOCUMENT_NODES,
        )
        .unwrap_err()
        .message,
        "sound clock processing subtree changed"
    );
    // Supplying a child as the event owner cannot reuse the output proof.
    assert!(
        old.sound_owner_clock_correspondence_with_repeats(
            &new,
            &id("processing"),
            &id("processing"),
            (&id("after"), &id("after")),
            &repeats,
            MAX_DOCUMENT_NODES,
        )
        .is_err()
    );
}

#[test]
fn fixed_retime_output_proof_retains_stage_controls_and_budget_guards() {
    let before = processing(PitchPolicy::Preserve, 1, false);
    let old = FrozenAudioLayout::capture(&before).unwrap();
    for changed in [
        rate("body", 5, 1, 8, PitchPolicy::Preserve),
        rate("body", 4, 0, 8, PitchPolicy::Preserve),
        rate("body", 4, 1, 8, PitchPolicy::FollowSpeed),
    ] {
        let mut candidate = before.clone();
        candidate.nodes.insert(id("processing"), changed);
        candidate.validate().unwrap();
        let new = FrozenAudioLayout::capture(&candidate).unwrap();
        assert_eq!(
            old.sound_owner_clock_correspondence_with_repeats(
                &new,
                &id("processing"),
                &id("processing"),
                (&id("processing"), &id("processing")),
                &SoundClockRepeatMap::default(),
                MAX_DOCUMENT_NODES,
            )
            .unwrap_err()
            .message,
            "sound clock processing subtree changed"
        );
    }
    for budget in [0, 1, MAX_DOCUMENT_NODES + 1] {
        assert!(
            old.sound_owner_clock_correspondence_with_repeats(
                &old,
                &id("processing"),
                &id("processing"),
                (&id("processing"), &id("processing")),
                &SoundClockRepeatMap::default(),
                budget,
            )
            .is_err()
        );
    }
}

#[test]
fn fixed_retime_output_proof_cannot_omit_an_enclosing_processor() {
    let mut before = processing(PitchPolicy::Preserve, 1, false);
    before.nodes.insert(
        id("outer"),
        rate("processing", 4, 0, 4, PitchPolicy::Preserve),
    );
    before.nodes.insert(
        id("root"),
        BeatNode::sequence("Root", vec![id("lead"), id("outer"), id("tail")]),
    );
    before.validate().unwrap();
    let after = resized(&before, "hold", 3);
    let old = FrozenAudioLayout::capture(&before).unwrap();
    let new = FrozenAudioLayout::capture(&after).unwrap();
    assert_eq!(
        old.sound_owner_clock_correspondence_with_repeats(
            &new,
            &id("processing"),
            &id("processing"),
            (&id("processing"), &id("processing")),
            &SoundClockRepeatMap::default(),
            MAX_DOCUMENT_NODES,
        )
        .unwrap_err()
        .message,
        "sound clock scope omits an enclosing processor"
    );
    assert!(
        old.sound_owner_clock_correspondence_with_repeats(
            &new,
            &id("outer"),
            &id("outer"),
            (&id("processing"), &id("processing")),
            &SoundClockRepeatMap::default(),
            MAX_DOCUMENT_NODES,
        )
        .is_err()
    );
}

#[test]
fn fixed_retime_output_owned_prior_journal_survives_later_stage_movement() {
    let (before, effect) = output_sound(true, true);
    let resized = changed(
        &before,
        Command::SetHoldDuration {
            node: id("hold"),
            duration: frames(3),
        },
        "change-input-before-later-move",
    );
    let moved = changed(
        &resized,
        Command::SetHoldDuration {
            node: id("lead"),
            duration: frames(5),
        },
        "move-after-input-edit",
    );
    let old = &before.audio_bindings().sound_clocks()[&id("processing")][&effect];
    let new = &moved.audio_bindings().sound_clocks()[&id("processing")][&effect];
    assert_eq!(new.scope(), &id("processing"));
    assert_eq!(new.clocks().len(), 2);
    assert_eq!(new.clocks()[0].timing(), old.clocks()[0].timing());
    assert_eq!(new.clocks()[0].scope(), &id("processing"));
    assert_eq!(
        new.clocks()[1].timing().allocation,
        rev("move-after-input-edit")
    );
    assert_eq!(output_projection(&moved).origin, ExactRatio::integer(5));
    assert_eq!(moved.beat_sounds, before.beat_sounds);
    assert!(moved.derived_clocks.is_empty());
}

#[test]
fn fixed_retime_full_scope_proofs_remain_shared_between_child_owned_sounds() {
    let mut before = processing(PitchPolicy::Preserve, 1, false);
    let mut template = before.clone();
    let effect = attach_effect(&mut template, "after");
    let event = template.beat_sounds[&id("after")][&effect].clone();
    before.nodes.remove(&id("processing"));
    before.nodes.insert(
        id("group"),
        BeatNode::sequence("Group", vec![id("body"), id("tail")]),
    );
    before.nodes.insert(
        id("root"),
        BeatNode::sequence("Root", vec![id("lead"), id("group")]),
    );
    before.beat_sounds.insert(
        id("group"),
        std::collections::BTreeMap::from([(SoundId::new("group-effect").unwrap(), event.clone())]),
    );
    let mut children = vec![id("before"), id("hold"), id("after")];
    // A full proof is cheap once but would exceed its fixed work limit if
    // recomputed for every child owner. This also exercises persisted-history
    // validation and restoration on the second root-ripple command.
    let owner_count = 63;
    let filler_count = 2000;
    assert!(owner_count * filler_count > MAX_DOCUMENT_NODES);
    for index in 0..(owner_count + filler_count) {
        let owner = id(&format!("clock-child-{index}"));
        let mut silent = hold(1);
        let NodeKind::Hold { recipe } = &mut silent.kind else {
            unreachable!()
        };
        recipe.audio = HoldAudio::Silence;
        before.nodes.insert(owner.clone(), silent);
        children.push(owner.clone());
        if index < owner_count {
            before.beat_sounds.insert(
                owner,
                std::collections::BTreeMap::from([(
                    SoundId::new(format!("child-effect-{index}")).unwrap(),
                    event.clone(),
                )]),
            );
        }
    }
    before
        .nodes
        .insert(id("body"), BeatNode::sequence("Body", children));
    before.validate().unwrap();
    let first = changed(
        &before,
        Command::SetHoldDuration {
            node: id("lead"),
            duration: frames(4),
        },
        "first-shared-sound-scope",
    );
    let second = changed(
        &first,
        Command::SetHoldDuration {
            node: id("lead"),
            duration: frames(5),
        },
        "second-shared-sound-scope",
    );
    assert_eq!(second.beat_sounds, before.beat_sounds);
    assert_eq!(
        second.audio_bindings().sound_clocks().len(),
        owner_count + 1
    );
    for journals in second.audio_bindings().sound_clocks().values() {
        assert!(journals.values().all(|journal| journal.clocks().len() == 2));
    }
}

#[test]
fn fixed_retime_output_owned_sound_outside_hold_move_keeps_group_scoped_capture() {
    let (original, effect) = output_sound(false, true);
    let (moved, moved_effect) = output_sound(true, true);
    assert_eq!(effect, moved_effect);
    let journal = &moved.audio_bindings().sound_clocks()[&id("processing")][&effect];
    assert_eq!(journal.scope(), &id("group"));
    assert_eq!(journal.clocks().len(), 1);
    let clock = &journal.clocks()[0];
    assert_eq!(clock.scope(), &id("group"));
    assert_eq!(clock.owner(), &id("processing"));
    assert_eq!(
        moved.audio_bindings().timings()[clock.timing()],
        FrozenAudioLayout::capture(&original).unwrap()
    );
    let noop = changed(
        &moved,
        Command::SetHoldDuration {
            node: id("hold"),
            duration: frames(2),
        },
        "unchanged-group-journal",
    );
    assert_eq!(noop.audio_bindings(), moved.audio_bindings());
}
