//! Inner Repeat carriers retain their individual clocks through speed updates.
use super::*;
use std::collections::BTreeMap;

fn audio_span() -> SourceSpan {
    let timestamp = |ticks| SourceTimestamp {
        ticks,
        time_base: SourceTimeBase::new(1, 44_100).unwrap(),
    };
    SourceSpan::new(timestamp(3100), timestamp(3260)).unwrap()
}

fn audible_source(frames: i64) -> BeatNode {
    let mut node = source(frames);
    let NodeKind::Source { source } = &mut node.kind else {
        unreachable!()
    };
    source.audio = Some(SourceAudio {
        asset: AssetId::new("media").unwrap(),
        span: audio_span(),
    });
    source.audio_mapping =
        SourceAudioMapping::natural_rate(audio_span(), FrameRate::new(30_000, 1001).unwrap())
            .unwrap();
    node
}

fn room(frames: i64) -> BeatNode {
    let mut held = recipe(frames);
    held.audio = HoldAudio::RoomTone {
        source: SourceAudio {
            asset: AssetId::new("media").unwrap(),
            span: audio_span(),
        },
    };
    BeatNode::hold("Room tone", held)
}

fn concrete(owner: &str, iteration: &IterationId) -> InstancePath {
    InstancePath {
        node: id(owner),
        repeats: vec![RepeatInstance {
            node: id("repeat"),
            iteration: iteration.clone(),
        }],
    }
}

fn wrapped_fixture() -> ProjectDocument {
    let iterations = IterationOrder::new(revision("repeat-plays"), 3).unwrap();
    let first = iterations.at(0).unwrap();
    let middle = iterations.at(1).unwrap();
    let mut repeat = BeatNode::sequence("Repeat", vec![]);
    repeat.kind = NodeKind::Repeat {
        child: id("default-body"),
        iterations: iterations.clone(),
        gap: Some(recipe(1)),
        escalation: None,
    };
    let mut wire = serde_json::to_value(tree(
        &["a", "group", "b"],
        vec![
            ("a", source(2)),
            (
                "group",
                BeatNode::sequence("Group", vec![id("lead"), id("repeat"), id("tail")]),
            ),
            ("lead", source(1)),
            ("tail", source(1)),
            ("repeat", repeat),
            (
                "default-body",
                BeatNode::sequence("Default", vec![id("default-source"), id("default-room")]),
            ),
            ("default-source", source(2)),
            ("default-room", BeatNode::hold("Room tone", recipe(1))),
            ("b", source(2)),
        ],
    ))
    .unwrap();
    // Initial asset/recipe admission only. Every edit below uses a public
    // Command, with stable iteration IDs and explicit new subtree identities.
    wire["assets"]["media"]["audio"] = serde_json::to_value(audio_span()).unwrap();
    wire["nodes"]["default-source"] = serde_json::to_value(audible_source(2)).unwrap();
    wire["nodes"]["default-room"] = serde_json::to_value(room(1)).unwrap();
    let NodeKind::Hold {
        recipe: default_gap,
    } = room(1).kind
    else {
        unreachable!()
    };
    wire["nodes"]["repeat"]["kind"]["gap"] = serde_json::to_value(default_gap).unwrap();
    let initial = ProjectDocument::from_json(&wire.to_string()).unwrap();
    assert_eq!(initial.durations().unwrap()[&id("repeat")], duration(11));
    let with_play = edit(
        &initial,
        request(
            &initial,
            "owned-middle",
            Command::SetPlayOverride {
                node: id("repeat"),
                iteration: middle.clone(),
                subtree: Subtree {
                    root: id("owned-body"),
                    nodes: BTreeMap::from([
                        (
                            id("owned-body"),
                            BeatNode::sequence("Owned", vec![id("owned-source"), id("owned-room")]),
                        ),
                        (id("owned-source"), audible_source(3)),
                        (id("owned-room"), room(1)),
                    ]),
                    overrides: BTreeMap::new(),
                    gap_overrides: BTreeMap::new(),
                },
            },
        ),
    );
    assert_eq!(with_play.durations().unwrap()[&id("repeat")], duration(12));
    let grouped = edit(
        &with_play,
        request(
            &with_play,
            "owned-first-gap",
            Command::SetGapOverride {
                node: id("repeat"),
                iteration: first.clone(),
                subtree: Subtree {
                    root: id("owned-gap"),
                    nodes: BTreeMap::from([(id("owned-gap"), room(2))]),
                    overrides: BTreeMap::new(),
                    gap_overrides: BTreeMap::new(),
                },
            },
        ),
    );
    let extents = grouped.durations().unwrap();
    assert_eq!(extents[&id("repeat")], duration(13));
    assert_eq!(extents[&id("group")], duration(15));
    let wrapped = edit(
        &grouped,
        request(
            &grouped,
            "wrap-speed",
            Command::WrapRetime {
                node: id("group"),
                id: id("slow"),
                duration: duration(30),
                pitch: PitchPolicy::FollowSpeed,
            },
        ),
    );
    assert!(wrapped.derived_clocks().is_empty());
    assert_eq!(wrapped.duration().unwrap(), duration(34));
    // Positive control: the same existing stage accepts ordinary parameters
    // before an exact pause is installed. It does not mutate our witness base.
    let ordinary = edit(&wrapped, update(&wrapped, "ordinary-repeat-speed", 18));
    assert_eq!(ordinary.duration().unwrap(), duration(22));
    assert!(ordinary.derived_clocks().is_empty());
    wrapped
}

fn exact_fixture() -> ProjectDocument {
    let wrapped = wrapped_fixture();
    let NodeKind::Repeat { iterations, .. } = &wrapped.nodes()[&id("repeat")].kind else {
        unreachable!()
    };
    let first = iterations.at(0).unwrap();
    let middle = iterations.at(1).unwrap();
    let last = iterations.at(2).unwrap();
    let target = retimed_target(&wrapped, 3);
    assert_eq!(target.retime, id("slow"));
    assert_eq!(target.required_ids, 4); // left, right, one copied lead, adapter
    let before = edit(
        &wrapped,
        pause(&wrapped, "repeat-pause", 3, 3, target.required_ids),
    );
    let clock = &before.derived_clocks()[&id("slow")];
    assert_eq!(clock.extent, ratio(33, 1));
    assert_eq!(
        (clock.child_start, clock.child_end),
        (ExactRatio::ZERO, ratio(33, 2))
    );
    assert_eq!(
        before.exact_durations().unwrap()[&id("repeat")],
        ratio(13, 1)
    );
    assert_eq!(
        before.exact_durations().unwrap()[before.root()],
        ratio(37, 1)
    );
    assert_eq!(
        before.nodes()[&id("repeat")],
        wrapped.nodes()[&id("repeat")]
    );
    assert_eq!(before.overrides(), wrapped.overrides());
    assert_eq!(before.gap_overrides(), wrapped.gap_overrides());
    assert!(
        !before
            .audio_bindings()
            .bindings()
            .contains_key(&id("repeat"))
    );
    let default = &before.audio_bindings().bindings()[&id("default-source")].lattice;
    assert_eq!(
        default.arguments[0].value,
        AudioRepeatValue::Live {
            repeat: id("repeat")
        }
    );
    assert_eq!(default.births[0].definition_root, id("default-body"));
    let owned = &before.audio_bindings().bindings()[&id("owned-source")].lattice;
    assert_eq!(
        owned.arguments[0].value,
        AudioRepeatValue::Captured {
            iteration: middle.clone()
        }
    );
    let gap = &before.audio_bindings().gap_bindings()[&id("repeat")].lattice;
    assert_eq!(
        gap.gap_after,
        Some(AudioRepeatValue::Live {
            repeat: id("repeat")
        })
    );
    let frozen = FrozenAudioLayout::capture(&before).unwrap();
    let empty_path = InstancePath {
        node: id("default-source"),
        repeats: vec![],
    };
    assert!(
        frozen
            .project(&empty_path, ExactRatio::ZERO, None, MAX_DOCUMENT_NODES)
            .unwrap_err()
            .message
            .contains("omits a Repeat")
    );
    // Concrete phase controls cover both default plays, the owned middle,
    // and an owned gap. The pause shifts each current origin by exactly3,
    // while every raw carrier retains its pre-pause lattice and slope2.
    for (owner, play, old, current) in [
        ("default-source", &first, 4, 7),
        ("default-room", &first, 8, 11),
        ("owned-source", &middle, 14, 17),
        ("owned-room", &middle, 20, 23),
        ("default-source", &last, 24, 27),
        ("default-room", &last, 28, 31),
        ("owned-gap", &first, 10, 13),
    ] {
        let path = concrete(owner, play);
        let placement = frozen
            .project(&path, ExactRatio::ZERO, None, MAX_DOCUMENT_NODES)
            .unwrap();
        assert_eq!(placement.origin, ExactRatio::integer(current));
        assert_eq!(placement.frames_per_local_frame, ratio(2, 1));
        let retained = before
            .audio_bindings()
            .resolve_in(
                &id(owner),
                AudioBindingEnvironment::Occurrence(&path),
                MAX_DOCUMENT_NODES,
            )
            .unwrap();
        assert_eq!(retained.lattice.origin, ExactRatio::integer(old));
        assert_eq!(retained.lattice.frames_per_local_frame, ratio(2, 1));
    }
    let gap_path = InstancePath {
        node: id("repeat"),
        repeats: vec![],
    };
    let gap_current = frozen
        .project(
            &gap_path,
            ExactRatio::ZERO,
            Some(&middle),
            MAX_DOCUMENT_NODES,
        )
        .unwrap();
    assert_eq!(gap_current.origin, ratio(25, 1));
    let gap_old = before
        .audio_bindings()
        .resolve_gap_in(
            &id("repeat"),
            AudioBindingEnvironment::GapOccurrence {
                instance: &gap_path,
                after: &middle,
            },
            MAX_DOCUMENT_NODES,
        )
        .unwrap();
    assert_eq!(gap_old.lattice.origin, ratio(22, 1));
    assert_eq!(gap_old.lattice.frames_per_local_frame, ratio(2, 1));
    before
}

#[test]
fn set_retime_updates_exact_stage_with_inner_repeat_default_owned_plays_and_gaps() {
    let before = exact_fixture();
    let old_bindings = before.audio_bindings().clone();
    let old_clocks = before.derived_clocks().clone();
    let snapshot = before.to_json().unwrap();
    let command = update(&before, "inner-repeat-speed", 18);
    let outcome = apply(&before, &command);
    retain_witness(&before, &command, &outcome);
    assert_eq!(before.to_json().unwrap(), snapshot);
    // The archived old-code witness refuses here after all preceding controls.
    let transaction =
        outcome.expect("an exact stage with this inner Repeat must retain editable tape speed");
    before
        .validate_retime_update(&id("slow"), duration(18), PitchPolicy::FollowSpeed)
        .unwrap();
    assert_eq!(
        before
            .retime_update_input(&id("slow"), PitchPolicy::FollowSpeed)
            .unwrap(),
        ratio(33, 2)
    );
    let after = transaction.forward.apply(&before).unwrap();
    assert_eq!(transaction.duration_delta, -15);
    assert_eq!(after.exact_durations().unwrap()[after.root()], ratio(22, 1));
    for owner in [
        "default-source",
        "default-room",
        "owned-source",
        "owned-room",
        "owned-gap",
    ] {
        assert_eq!(
            after.audio_bindings().bindings()[&id(owner)],
            old_bindings.bindings()[&id(owner)]
        );
    }
    assert_eq!(
        after.audio_bindings().gap_bindings()[&id("repeat")],
        old_bindings.gap_bindings()[&id("repeat")]
    );
    for (owner, old) in old_clocks {
        if owner != id("slow") {
            assert_eq!(after.derived_clocks().get(&owner), Some(&old));
        }
    }
    assert_eq!(after.nodes()[&id("repeat")], before.nodes()[&id("repeat")]);
    assert_eq!(after.overrides(), before.overrides());
    assert_eq!(after.gap_overrides(), before.gap_overrides());
    assert_eq!(
        ProjectDocument::from_json(&after.to_json().unwrap()).unwrap(),
        after
    );
    assert_eq!(transaction.inverse.apply(&after).unwrap(), before);
}

#[test]
fn inner_repeat_speed_update_captures_missing_node_and_default_gap_clocks_once() {
    let bound = exact_fixture();
    let mut wire = serde_json::to_value(&bound).unwrap();
    for owner in ["default-source", "owned-source"] {
        assert!(
            wire["audio_bindings"]["bindings"]
                .as_object_mut()
                .unwrap()
                .remove(owner)
                .is_some()
        );
    }
    assert!(
        wire["audio_bindings"]["gap_bindings"]
            .as_object_mut()
            .unwrap()
            .remove("repeat")
            .is_some()
    );
    prune_unreferenced_fixture_timings(&mut wire);
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let old = before.audio_bindings().clone();
    let timing = AudioTimingId {
        allocation: revision("fresh-repeat-speed"),
        ordinal: 0,
    };
    let captured = capture_unbound_audio_bindings(&before, timing.clone()).unwrap();
    let old_layout = FrozenAudioLayout::capture(&before).unwrap();
    let window = ExactFrameRange::new(ExactRatio::ZERO, ratio(37, 1)).unwrap();
    let outcome = apply(&before, &update(&before, "fresh-repeat-speed", 18)).unwrap();
    let after = outcome.forward.apply(&before).unwrap();
    for owner in ["default-source", "owned-source"] {
        let added = &after.audio_bindings().bindings()[&id(owner)];
        assert_eq!(added.lattice, captured.bindings()[&id(owner)].lattice);
        assert_eq!(added.lattice.reference.timing, timing);
        assert_eq!(added.resume, None);
        // A new repeated carrier retains the pre-edit allocation-entry step,
        // just like an ordinary missing carrier. It must not sample the new
        // stage geometry or install another sampling lattice.
        assert_eq!(added.reanchors.len(), 1);
        assert_eq!(
            added.reanchors[0].anchor,
            AudioReanchorAnchor::AllocationEntry
        );
        assert_eq!(added.reanchors[0].placement, added.lattice);
        assert_eq!(added.reanchors[0].window, Some(window));
    }
    let gap = &after.audio_bindings().gap_bindings()[&id("repeat")];
    assert_eq!(gap.lattice, captured.gap_bindings()[&id("repeat")].lattice);
    assert_eq!(gap.lattice.reference.timing, timing);
    assert_eq!(gap.resume, None);
    assert_eq!(gap.reanchors.len(), 1);
    assert_eq!(
        gap.reanchors[0].anchor,
        AudioReanchorAnchor::AllocationEntry
    );
    assert_eq!(gap.reanchors[0].placement, gap.lattice);
    assert_eq!(gap.reanchors[0].window, Some(window));
    assert_eq!(after.audio_bindings().timings()[&timing], old_layout);
    let added_timings: Vec<_> = after
        .audio_bindings()
        .timings()
        .keys()
        .filter(|key| !old.timings().contains_key(*key))
        .cloned()
        .collect();
    assert_eq!(added_timings, vec![timing]);
    let NodeKind::Repeat { iterations, .. } = &before.nodes()[&id("repeat")].kind else {
        unreachable!()
    };
    let first = iterations.at(0).unwrap();
    let middle = iterations.at(1).unwrap();
    let last = iterations.at(2).unwrap();
    let current_layout = FrozenAudioLayout::capture(&after).unwrap();
    for (owner, iteration, origin, sample, new_origin) in [
        ("default-source", &first, 7, 11_211, ratio(52, 11)),
        ("owned-source", &middle, 17, 27_227, ratio(112, 11)),
        ("default-source", &last, 27, 43_243, ratio(172, 11)),
    ] {
        let path = concrete(owner, iteration);
        let expected = captured
            .resolve_in(
                &id(owner),
                AudioBindingEnvironment::Occurrence(&path),
                MAX_DOCUMENT_NODES,
            )
            .unwrap();
        let retained = after
            .audio_bindings()
            .resolve_in(
                &id(owner),
                AudioBindingEnvironment::Occurrence(&path),
                MAX_DOCUMENT_NODES,
            )
            .unwrap();
        assert_eq!(retained.lattice, expected.lattice);
        assert_eq!(retained.lattice.origin, ExactRatio::integer(origin));
        assert_eq!(retained.lattice.frames_per_local_frame, ratio(2, 1));
        assert_eq!(
            retained.lattice.sample_boundary(ExactRatio::ZERO).unwrap(),
            sample
        );
        let resume = retained.resume.unwrap();
        assert_eq!(resume.local_boundary, ExactRatio::ZERO);
        assert_eq!(resume.reference_local_delta, ExactRatio::ZERO);
        let current = current_layout
            .project(&path, ExactRatio::ZERO, None, MAX_DOCUMENT_NODES)
            .unwrap();
        assert_eq!(current.origin, new_origin);
        assert_eq!(current.frames_per_local_frame, ratio(12, 11));
    }
    let gap_path = InstancePath {
        node: id("repeat"),
        repeats: vec![],
    };
    let retained = after
        .audio_bindings()
        .resolve_gap_in(
            &id("repeat"),
            AudioBindingEnvironment::GapOccurrence {
                instance: &gap_path,
                after: &middle,
            },
            MAX_DOCUMENT_NODES,
        )
        .unwrap();
    let expected = captured
        .resolve_gap_in(
            &id("repeat"),
            AudioBindingEnvironment::GapOccurrence {
                instance: &gap_path,
                after: &middle,
            },
            MAX_DOCUMENT_NODES,
        )
        .unwrap();
    assert_eq!(retained.lattice, expected.lattice);
    assert_eq!(retained.lattice.origin, ratio(25, 1));
    assert_eq!(
        retained.lattice.sample_boundary(ExactRatio::ZERO).unwrap(),
        40_040
    );
    let resume = retained.resume.unwrap();
    assert_eq!(resume.local_boundary, ExactRatio::ZERO);
    assert_eq!(resume.reference_local_delta, ExactRatio::ZERO);
    let current = current_layout
        .project(
            &gap_path,
            ExactRatio::ZERO,
            Some(&middle),
            MAX_DOCUMENT_NODES,
        )
        .unwrap();
    assert_eq!(current.origin, ratio(160, 11));
    assert_eq!(current.frames_per_local_frame, ratio(12, 11));
    for (owner, binding) in old.bindings() {
        if owner != &id("b") {
            assert_eq!(after.audio_bindings().bindings().get(owner), Some(binding));
        }
    }
    assert!(
        !after
            .audio_bindings()
            .bindings()
            .contains_key(&id("repeat"))
    );
    assert_eq!(outcome.inverse.apply(&after).unwrap(), before);
    assert_eq!(
        ProjectDocument::from_json(&after.to_json().unwrap()).unwrap(),
        after
    );
}

#[test]
fn inner_repeat_speed_update_checks_born_point_clock_beside_surviving_root_clock() {
    let wrapped = wrapped_fixture();
    let captured = edit(&wrapped, pause(&wrapped, "pre-birth-capture", 0, 1, 0));
    let born = edit(
        &captured,
        request(
            &captured,
            "born-middle-play",
            Command::InsertPlays {
                node: id("repeat"),
                index: 1,
                count: 1,
            },
        ),
    );
    let target = retimed_target(&born, 4);
    let before = edit(
        &born,
        pause(&born, "post-birth-pause", 4, 3, target.required_ids),
    );
    assert_eq!(before.derived_clocks()[&id("slow")].extent, ratio(33, 1));
    let NodeKind::Repeat { iterations, .. } = &before.nodes()[&id("repeat")].kind else {
        unreachable!()
    };
    let first = iterations.at(0).unwrap();
    let born = iterations.at(1).unwrap();
    assert_eq!(born.allocation, revision("born-middle-play"));
    let resolve = |iteration: &IterationId| {
        before
            .audio_bindings()
            .resolve_in(
                &id("default-source"),
                AudioBindingEnvironment::Occurrence(&concrete("default-source", iteration)),
                MAX_DOCUMENT_NODES,
            )
            .unwrap()
            .lattice
    };
    let old_clock = resolve(&first);
    let born_clock = resolve(&born);
    assert_eq!(old_clock.clock, AudioClockRoot::ProjectRootRoundEven);
    assert_eq!(
        old_clock.local_frames_per_sample().unwrap(),
        ratio(5, 16016)
    );
    assert_eq!(
        born_clock.clock,
        AudioClockRoot::DefinitionPointCeil {
            root: id("default-body")
        }
    );
    assert_eq!(
        born_clock.local_frames_per_sample().unwrap(),
        ratio(5, 8008)
    );
    let gap_path = InstancePath {
        node: id("repeat"),
        repeats: vec![],
    };
    let born_gap = before
        .audio_bindings()
        .resolve_gap_in(
            &id("repeat"),
            AudioBindingEnvironment::GapOccurrence {
                instance: &gap_path,
                after: &born,
            },
            MAX_DOCUMENT_NODES,
        )
        .unwrap()
        .lattice;
    assert_eq!(
        born_gap.clock,
        AudioClockRoot::GapDefinitionPointCeil {
            repeat: id("repeat")
        }
    );
    assert_eq!(born_gap.local_frames_per_sample().unwrap(), ratio(5, 8008));
    let old_middle = iterations.at(2).unwrap();
    let old_gap = before
        .audio_bindings()
        .resolve_gap_in(
            &id("repeat"),
            AudioBindingEnvironment::GapOccurrence {
                instance: &gap_path,
                after: &old_middle,
            },
            MAX_DOCUMENT_NODES,
        )
        .unwrap()
        .lattice;
    assert_eq!(old_gap.clock, AudioClockRoot::ProjectRootRoundEven);
    assert_eq!(old_gap.local_frames_per_sample().unwrap(), ratio(5, 16016));
    // At1500 output frames the first surviving class transfers at11/500,
    // inside the inclusive bound; the live born class needs11/1000, below1/64.
    assert!(
        ratio(11, 500)
            .checked_mul(ratio(64, 1))
            .unwrap()
            .compare_integer(1)
            .is_gt()
    );
    assert!(
        ratio(11, 1000)
            .checked_mul(ratio(64, 1))
            .unwrap()
            .compare_integer(1)
            .is_lt()
    );
    let snapshot = before.to_json().unwrap();
    for result in [
        before.validate_retime_update(&id("slow"), duration(1500), PitchPolicy::FollowSpeed),
        apply(&before, &update(&before, "born-rate-refused", 1500)).map(|_| ()),
    ] {
        let error = result.unwrap_err();
        assert_eq!(error.code, EditErrorCode::InvalidCommand);
        assert!(error.message.contains("repeated audio clock"), "{error:?}");
    }
    assert_eq!(before.to_json().unwrap(), snapshot);
    let outcome = apply(&before, &update(&before, "born-rate-safe", 18)).unwrap();
    let after = outcome.forward.apply(&before).unwrap();
    assert_eq!(
        after.audio_bindings().bindings()[&id("default-source")],
        before.audio_bindings().bindings()[&id("default-source")]
    );
    assert_eq!(outcome.inverse.apply(&after).unwrap(), before);
}
