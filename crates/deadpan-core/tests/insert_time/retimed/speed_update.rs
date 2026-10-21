//! Public-command regression proposal. Expected to fail on the retained old
//! source at SetRetime's exact-clock refusal, after positive controls pass.
use super::*;

#[path = "speed_update/source_crop.rs"]
mod source_crop;

#[path = "speed_update/inner_repeat.rs"]
mod inner_repeat;

fn retain_witness(
    before: &ProjectDocument,
    update: &CommandRequest,
    outcome: &Result<EditTransaction, EditError>,
) {
    let Some(directory) = std::env::var_os("DEADPAN_SPEED_UPDATE_WITNESS_DIR") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    std::fs::create_dir_all(&directory).expect("create explicit witness directory");
    std::fs::write(directory.join("before.json"), before.to_json().unwrap())
        .expect("retain pre-update document");
    std::fs::write(
        directory.join("request.json"),
        serde_json::to_vec_pretty(update).unwrap(),
    )
    .expect("retain public SetRetime request");
    let observation = match outcome {
        Ok(transaction) => json!({ "status": "ok", "duration_delta": transaction.duration_delta }),
        Err(error) => json!({ "status": "error", "error": error }),
    };
    std::fs::write(
        directory.join("observed.json"),
        serde_json::to_vec_pretty(&observation).unwrap(),
    )
    .expect("retain observed command outcome before assertion");
}

fn origin(document: &ProjectDocument, node: &str) -> ExactRatio {
    FrozenAudioLayout::capture(document)
        .unwrap()
        .project(
            &InstancePath {
                node: id(node),
                repeats: vec![],
            },
            ExactRatio::ZERO,
            None,
            MAX_DOCUMENT_NODES,
        )
        .unwrap()
        .origin
}

#[test]
fn unchanged_clocked_stage_keeps_raw_noop_admission() {
    let initial = fixture();
    let before = edit(&initial, pause(&initial, "p", 5, 3, 5));
    let transaction = apply(
        &before,
        &request(
            &before,
            "unchanged-clocked-speed",
            Command::SetRetime {
                node: id("slow"),
                duration: duration(27),
                pitch: PitchPolicy::FollowSpeed,
            },
        ),
    )
    .unwrap();
    let after = transaction.forward.apply(&before).unwrap();
    assert_eq!(transaction.duration_delta, 0);
    assert_eq!(after.nodes(), before.nodes());
    assert_eq!(after.derived_clocks(), before.derived_clocks());
    assert_eq!(after.audio_bindings(), before.audio_bindings());
    assert_eq!(transaction.inverse.apply(&after).unwrap(), before);
}

#[test]
fn set_retime_updates_a_stage_with_retained_exact_pause_clocks() {
    let initial = fixture();
    assert_eq!(
        initial.exact_durations().unwrap()[initial.root()],
        ratio(28, 1)
    );
    // Positive control: the same public parameter update is already supported
    // before the pause gives this ordinary stage exact descendant clocks.
    let ordinary = edit(
        &initial,
        request(
            &initial,
            "ordinary-speed",
            Command::SetRetime {
                node: id("slow"),
                duration: duration(15),
                pitch: PitchPolicy::FollowSpeed,
            },
        ),
    );
    assert!(ordinary.derived_clocks().is_empty());
    assert_eq!(
        ordinary.exact_durations().unwrap()[ordinary.root()],
        ratio(19, 1)
    );

    // Reuse the admitted public pause construction and exact five-ID pool.
    let before = edit(&initial, pause(&initial, "p", 5, 3, 5));
    let before_json = before.to_json().unwrap();
    let old_clocks = before.derived_clocks().clone();
    let clock = &old_clocks[&id("slow")];
    assert_eq!(clock.extent, ratio(27, 1));
    assert_eq!(
        (clock.child_start, clock.child_end),
        (ExactRatio::ZERO, ratio(27, 2))
    );
    assert_eq!(
        before.exact_durations().unwrap()[before.root()],
        ratio(31, 1)
    );
    assert_eq!(old_clocks[&id("p-0")].extent, ratio(3, 2));
    assert_eq!(old_clocks[&id("p-4")].extent, ratio(3, 2));
    assert_eq!(old_clocks[&id("p-1")].extent, ratio(21, 2));
    assert!(before.audio_bindings().bindings().contains_key(&id("p-2")));
    // The rounded recipe is only a cover, not the selected input to use for
    // a speed request. At 7/8 the exact13.5 input rounds to15 output, not16.
    let (_, recipe_window, old_child) = retime(&before, "slow");
    assert_eq!(recipe_window.end(), ProjectFrame(14));
    let selected = clock.child_end.checked_sub(clock.child_start).unwrap();
    assert_eq!(
        selected
            .checked_div(ratio(7, 8))
            .unwrap()
            .round_even()
            .unwrap(),
        15
    );
    assert_eq!(
        ExactRatio::integer(recipe_window.duration().frames())
            .checked_div(ratio(7, 8))
            .unwrap()
            .round_even()
            .unwrap(),
        16
    );

    let update = request(
        &before,
        "updated-clocked-speed",
        Command::SetRetime {
            node: id("slow"),
            duration: duration(15),
            pitch: PitchPolicy::FollowSpeed,
        },
    );
    let outcome = apply(&before, &update);
    // Optional bounded retained evidence is written before the intended old
    // failure, without changing the authored document or relaxing assertions.
    retain_witness(&before, &update, &outcome);
    assert_eq!(before.to_json().unwrap(), before_json);
    let transaction = outcome.unwrap_or_else(|error| panic!(
        "public SetRetime must update the live stage while retaining its exact pause clocks; code={:?}; message={}",
        error.code, error.message,
    ));
    let after = transaction.forward.apply(&before).unwrap();
    assert_eq!(transaction.duration_delta, -12);
    assert_eq!(transaction.inverse.apply(&after).unwrap(), before);
    assert_eq!(
        ProjectDocument::from_json(&after.to_json().unwrap()).unwrap(),
        after
    );
    let updated = &after.derived_clocks()[&id("slow")];
    assert_eq!(updated.extent, ratio(15, 1));
    assert_eq!(
        (updated.child_start, updated.child_end),
        (clock.child_start, clock.child_end)
    );
    assert_eq!(after.exact_durations().unwrap()[after.root()], ratio(19, 1));
    assert_eq!(retime(&after, "slow").2, old_child);
    for (owner, historical) in &old_clocks {
        if owner != &id("slow") {
            assert_eq!(after.derived_clocks().get(owner), Some(historical));
        }
    }
    for owner in ["inner", "p-2"] {
        assert_eq!(
            after.audio_bindings().bindings().get(&id(owner)),
            before.audio_bindings().bindings().get(&id(owner))
        );
    }
    // The old three-frame Hold now scales with its live containing stage. Its
    // authored recipe remains3, its existing adapter remains extent3/2.
    assert_eq!(
        after.nodes()[&id("pause-p")],
        before.nodes()[&id("pause-p")]
    );
    assert_eq!(origin(&after, "p-0"), ratio(2, 1));
    assert_eq!(origin(&after, "p-4"), ratio(11, 3));
    assert_eq!(origin(&after, "p-1"), ratio(16, 3));
    assert_eq!(origin(&after, "b"), ratio(17, 1));
}

fn clocked_with_output(output: i64) -> ProjectDocument {
    let mut wire = serde_json::to_value(fixture()).unwrap();
    wire["nodes"]["slow"]["kind"]["duration"] = json!(output);
    let initial = ProjectDocument::from_json(&wire.to_string()).unwrap();
    edit(&initial, pause(&initial, "p", 5, 3, 5))
}

fn update(document: &ProjectDocument, name: &str, output: i64) -> CommandRequest {
    request(
        document,
        name,
        Command::SetRetime {
            node: id("slow"),
            duration: duration(output),
            pitch: PitchPolicy::FollowSpeed,
        },
    )
}

// Fixture-only graph surgery can remove the last reference to an immutable
// timing table. Keep every table still named by a binding or sound journal.
fn prune_unreferenced_fixture_timings(wire: &mut serde_json::Value) {
    fn references(value: &serde_json::Value, used: &mut std::collections::BTreeSet<AudioTimingId>) {
        match value {
            serde_json::Value::Object(fields) => {
                for (key, value) in fields {
                    if key == "timing" {
                        used.insert(serde_json::from_value(value.clone()).unwrap());
                    } else {
                        references(value, used);
                    }
                }
            }
            serde_json::Value::Array(values) => {
                for value in values {
                    references(value, used);
                }
            }
            _ => {}
        }
    }
    let state = wire["audio_bindings"].as_object_mut().unwrap();
    let mut used = std::collections::BTreeSet::new();
    for (key, value) in state.iter() {
        if key != "timings" {
            references(value, &mut used);
        }
    }
    state
        .get_mut("timings")
        .unwrap()
        .as_array_mut()
        .unwrap()
        .retain(|record| {
            used.contains(&serde_json::from_value::<AudioTimingId>(record["id"].clone()).unwrap())
        });
}

#[test]
fn speed_update_queries_use_exact_input_and_requested_pitch_and_keep_raw_nested_noop() {
    let before = clocked_with_output(24);
    assert_eq!(
        before
            .retime_update_input(&id("slow"), PitchPolicy::FollowSpeed)
            .unwrap(),
        ratio(27, 2)
    );
    for pitch in [PitchPolicy::Preserve, PitchPolicy::Shift { semitones: 3 }] {
        assert!(
            before
                .retime_update_input(&id("slow"), pitch)
                .unwrap_err()
                .message
                .contains("tape pitch")
        );
        let changed = request(
            &before,
            "unsupported-pitch",
            Command::SetRetime {
                node: id("slow"),
                duration: duration(15),
                pitch,
            },
        );
        let snapshot = before.to_json().unwrap();
        assert!(
            apply(&before, &changed)
                .unwrap_err()
                .message
                .contains("tape pitch")
        );
        assert_eq!(before.to_json().unwrap(), snapshot);
    }
    assert!(
        before
            .validate_retime_update(&id("slow"), duration(0), PitchPolicy::FollowSpeed)
            .is_err()
    );
    // The pause adapter owns extent3/2 but its unchanged integral recipe is2.
    // Its raw no-op remains admitted before fractional/ancestor restrictions.
    let nested = request(
        &before,
        "nested-unchanged",
        Command::SetRetime {
            node: id("p-4"),
            duration: duration(2),
            pitch: PitchPolicy::FollowSpeed,
        },
    );
    let unchanged = edit(&before, nested);
    assert_eq!(unchanged.nodes(), before.nodes());
    assert_eq!(unchanged.derived_clocks(), before.derived_clocks());
    assert_eq!(unchanged.audio_bindings(), before.audio_bindings());
    assert!(
        before
            .retime_update_input(&id("p-4"), PitchPolicy::FollowSpeed)
            .is_err()
    );
    let partition = request(
        &before,
        "partition",
        Command::SetRetime {
            node: id("p-0"),
            duration: duration(12),
            pitch: PitchPolicy::FollowSpeed,
        },
    );
    assert_eq!(
        apply(&before, &partition).unwrap_err().code,
        EditErrorCode::WrongNodeKind
    );
}

#[test]
fn speed_update_checks_original_lattice_across_repeated_updates_and_exact_rate_limits() {
    let before = clocked_with_output(100);
    assert_eq!(before.derived_clocks()[&id("slow")].extent, ratio(103, 1));
    for (middle, rejected) in [(30, 1), (5120, 10_000)] {
        let intermediate = edit(&before, update(&before, "middle", middle));
        assert_eq!(
            intermediate.audio_bindings().bindings()[&id("inner")],
            before.audio_bindings().bindings()[&id("inner")]
        );
        let snapshot = intermediate.to_json().unwrap();
        let error = intermediate
            .validate_retime_update(&id("slow"), duration(rejected), PitchPolicy::FollowSpeed)
            .unwrap_err();
        assert!(
            error.message.contains("retained audio transfer"),
            "{error:?}"
        );
        assert!(
            apply(&intermediate, &update(&intermediate, "too-far", rejected))
                .unwrap_err()
                .message
                .contains("1/64..64")
        );
        assert_eq!(intermediate.to_json().unwrap(), snapshot);
    }
    // Initial61 plus the3-frame pause gives64; the original Source lattice
    // therefore reaches both inclusive transfer endpoints exactly.
    let endpoints = clocked_with_output(61);
    for output in [1, 4096] {
        assert!(
            endpoints
                .validate_retime_update(&id("slow"), duration(output), PitchPolicy::FollowSpeed)
                .is_ok()
        );
        let after = edit(&endpoints, update(&endpoints, "endpoint", output));
        assert_eq!(
            after.exact_durations().unwrap()[&id("slow")],
            ratio(i128::from(output), 1)
        );
    }
    assert!(
        endpoints
            .validate_retime_update(&id("slow"), duration(4097), PitchPolicy::FollowSpeed)
            .is_err()
    );
    let just_fast = clocked_with_output(62);
    assert!(
        just_fast
            .validate_retime_update(&id("slow"), duration(1), PitchPolicy::FollowSpeed)
            .is_err()
    );
}

#[test]
fn speed_update_captures_missing_inner_entries_but_reanchors_existing_moved_suffix() {
    let original = clocked_with_output(24);
    let mut wire = serde_json::to_value(&original).unwrap();
    wire["audio_bindings"]["bindings"]
        .as_object_mut()
        .unwrap()
        .remove("p-2");
    wire["audio_bindings"]["bindings"]["b"]["resume"] = serde_json::to_value(AudioResume {
        local_boundary: ratio(1, 2),
        phase: AudioLocalPhase {
            constant: ratio(1, 3),
            terms: vec![],
        },
    })
    .unwrap();
    prune_unreferenced_fixture_timings(&mut wire);
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    assert!(
        before
            .validate_retime_update(&id("slow"), duration(15), PitchPolicy::FollowSpeed)
            .is_ok()
    );
    let after = edit(&before, update(&before, "partial-speed", 15));
    assert_eq!(
        after.audio_bindings().bindings()[&id("inner")],
        before.audio_bindings().bindings()[&id("inner")]
    );
    let new = &after.audio_bindings().bindings()[&id("p-2")];
    assert_eq!(
        new.lattice.reference.timing.allocation,
        revision("partial-speed")
    );
    assert_eq!(new.reanchors.len(), 1);
    let resolved = after
        .audio_bindings()
        .resolve(
            &id("p-2"),
            &InstancePath {
                node: id("p-2"),
                repeats: vec![],
            },
            MAX_DOCUMENT_NODES,
        )
        .unwrap();
    assert_eq!(resolved.lattice.origin, ratio(5, 1));
    assert_eq!(resolved.resume.unwrap().local_boundary, ratio(3, 2));
    assert_eq!(
        after.audio_bindings().bindings()[&id("b")].reanchors.len(),
        before.audio_bindings().bindings()[&id("b")].reanchors.len() + 1
    );
    assert_eq!(
        after.audio_bindings().bindings()[&id("b")].resume,
        before.audio_bindings().bindings()[&id("b")].resume
    );
}

#[test]
fn speed_update_refuses_nested_repeat_without_blocking_unrelated_ordinary_stage() {
    let original = clocked_with_output(24);
    let mut wire = serde_json::to_value(original).unwrap();
    wire["audio_bindings"] = serde_json::to_value(AudioBindingState::default()).unwrap();
    wire["audio_lineage"] = json!({});
    wire["nodes"]["p-0"]["kind"]["child"] = json!("inner-repeat");
    let mut repeated = BeatNode::sequence("Repeated", vec![]);
    repeated.kind = NodeKind::Repeat {
        child: id("nested-repeat"),
        iterations: IterationOrder::new(revision("repeated"), 1_000_000).unwrap(),
        gap: None,
        escalation: None,
    };
    wire["nodes"]["inner-repeat"] = serde_json::to_value(repeated).unwrap();
    let mut nested = BeatNode::sequence("Nested", vec![]);
    nested.kind = NodeKind::Repeat {
        child: id("inner"),
        iterations: IterationOrder::new(revision("nested"), 2).unwrap(),
        gap: None,
        escalation: None,
    };
    wire["nodes"]["nested-repeat"] = serde_json::to_value(nested).unwrap();
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let snapshot = before.to_json().unwrap();
    assert!(
        before
            .retime_update_input(&id("slow"), PitchPolicy::FollowSpeed)
            .unwrap_err()
            .message
            .contains("nested Repeat families")
    );
    assert!(
        apply(&before, &update(&before, "repeat-refused", 15))
            .unwrap_err()
            .message
            .contains("nested Repeat families")
    );
    assert_eq!(before.to_json().unwrap(), snapshot);
    assert_eq!(
        edit(&before, update(&before, "repeat-noop", 27)).audio_bindings(),
        before.audio_bindings()
    );
    let wrapped = edit(
        &before,
        request(
            &before,
            "ordinary-wrap",
            Command::WrapRetime {
                node: id("b"),
                id: id("plain"),
                duration: duration(2),
                pitch: PitchPolicy::FollowSpeed,
            },
        ),
    );
    assert_eq!(
        wrapped
            .retime_update_input(&id("plain"), PitchPolicy::FollowSpeed)
            .unwrap(),
        ratio(2, 1)
    );
    let after = edit(
        &wrapped,
        request(
            &wrapped,
            "ordinary-update",
            Command::SetRetime {
                node: id("plain"),
                duration: duration(1),
                pitch: PitchPolicy::FollowSpeed,
            },
        ),
    );
    assert_eq!(after.derived_clocks(), wrapped.derived_clocks());
}

#[test]
fn speed_update_rate_preflight_skips_fully_clipped_retained_carrier() {
    let original = clocked_with_output(100);
    let mut historical_wire = serde_json::to_value(&original).unwrap();
    historical_wire["audio_bindings"] = serde_json::to_value(AudioBindingState::default()).unwrap();
    historical_wire["nodes"]["slow"]["kind"]["duration"] = json!(10_000);
    historical_wire["derived_clocks"]["slow"]["extent"] =
        serde_json::to_value(ratio(10_000, 1)).unwrap();
    let historical = ProjectDocument::from_json(&historical_wire.to_string()).unwrap();
    let captured = capture_unbound_audio_bindings(
        &historical,
        AudioTimingId {
            allocation: revision("hidden-history"),
            ordinal: 0,
        },
    )
    .unwrap();
    let captured_wire = serde_json::to_value(captured).unwrap();
    let mut wire = serde_json::to_value(original).unwrap();
    wire["audio_bindings"]["timings"]
        .as_array_mut()
        .unwrap()
        .extend(captured_wire["timings"].as_array().unwrap().iter().cloned());
    wire["audio_bindings"]["bindings"]["p-2"] = captured_wire["bindings"]["p-2"].clone();
    // Select only the left9/25-frame Partition. The older high-scale right
    // carrier survives but has no current allocation and must not cap speed.
    wire["nodes"]["slow"]["kind"]["mapping"]["end"] = json!(1);
    wire["derived_clocks"]["slow"]["child_end"] = serde_json::to_value(ratio(9, 25)).unwrap();
    prune_unreferenced_fixture_timings(&mut wire);
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    assert!(
        before
            .validate_retime_update(&id("slow"), duration(1), PitchPolicy::FollowSpeed)
            .is_ok()
    );
    let after = edit(&before, update(&before, "hidden-speed", 1));
    assert_eq!(
        after.audio_bindings().bindings()[&id("p-2")],
        before.audio_bindings().bindings()[&id("p-2")]
    );
    assert_eq!(after.exact_durations().unwrap()[after.root()], ratio(5, 1));
}

#[test]
fn speed_update_rate_preflight_skips_positive_interval_with_no_new_mix_samples() {
    let mut wire = serde_json::to_value(clocked_with_output(100_000)).unwrap();
    wire["assets"]["media"]["audio"] = serde_json::to_value(source_span()).unwrap();
    wire["nodes"]["inner"]["kind"]["source"]["audio"] = serde_json::to_value(SourceAudio {
        asset: AssetId::new("media").unwrap(),
        span: source_span(),
    })
    .unwrap();
    // Keep the right carrier's historical binding, but make its current
    // policy silent so only the tiny, audible left fragment caps the rate.
    wire["nodes"]["p-2"] =
        serde_json::to_value(BeatNode::hold("Silent remainder", recipe(12))).unwrap();
    wire["audio_lineage"] = json!({});
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let frozen = FrozenAudioLayout::capture(&before).unwrap();
    let (_, allocation) = frozen
        .project_scoped_with_allocation(
            frozen.root(),
            &InstancePath {
                node: id("inner"),
                repeats: vec![],
            },
            None,
            MAX_DOCUMENT_NODES,
        )
        .unwrap();
    let allocation = allocation.expect("positive continuous left allocation");
    assert!(
        allocation
            .start
            .checked_sub(allocation.end)
            .unwrap()
            .compare_integer(0)
            .is_lt()
    );
    // The left fragment occupies the first3 output frames before the update.
    // Compressing the100003-frame stage to1 puts both endpoints at sample3203.
    let first = ratio(2, 1).checked_div(ratio(5, 8008)).unwrap();
    let last = ratio(2, 1)
        .checked_add(ratio(3, 100_003))
        .unwrap()
        .checked_div(ratio(5, 8008))
        .unwrap();
    assert_eq!(first.round_even().unwrap(), 3203);
    assert_eq!(last.round_even().unwrap(), 3203);
    before
        .validate_retime_update(&id("slow"), duration(1), PitchPolicy::FollowSpeed)
        .unwrap();
    let transaction = apply(&before, &update(&before, "zero-sample-speed", 1)).unwrap();
    let after = transaction.forward.apply(&before).unwrap();
    for owner in ["inner", "p-2"] {
        assert_eq!(
            after.audio_bindings().bindings()[&id(owner)],
            before.audio_bindings().bindings()[&id(owner)]
        );
    }
    assert_eq!(after.exact_durations().unwrap()[after.root()], ratio(5, 1));
    assert_eq!(transaction.inverse.apply(&after).unwrap(), before);
    // At1000 output frames that same fragment receives samples, but the
    // historical transfer100003/1000 is still above64 and must refuse.
    let snapshot = before.to_json().unwrap();
    for result in [
        before.validate_retime_update(&id("slow"), duration(1000), PitchPolicy::FollowSpeed),
        apply(&before, &update(&before, "nonempty-transfer-refused", 1000)).map(|_| ()),
    ] {
        assert!(
            result
                .unwrap_err()
                .message
                .contains("retained audio transfer")
        );
    }
    assert_eq!(before.to_json().unwrap(), snapshot);
}

fn empty_old_carrier_history(expansion: i64) -> ProjectDocument {
    let mut tone = recipe(1);
    tone.audio = HoldAudio::RoomTone {
        source: SourceAudio {
            asset: AssetId::new("media").unwrap(),
            span: source_span(),
        },
    };
    let mut stage = BeatNode::sequence("Fast", vec![]);
    stage.kind = NodeKind::Retime {
        child: id("input"),
        duration: duration(1),
        mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(100_001)).unwrap(),
        pitch: PitchPolicy::FollowSpeed,
        purpose: RetimePurpose::Edit,
    };
    let mut initial_wire = serde_json::to_value(tree(
        &["a", "slow", "b"],
        vec![
            ("a", source(2)),
            ("slow", stage),
            (
                "input",
                BeatNode::sequence("Input", vec![id("tiny-tone"), id("silent-rest")]),
            ),
            ("tiny-tone", BeatNode::hold("Tiny tone", recipe(1))),
            ("silent-rest", BeatNode::hold("Silence", recipe(100_000))),
            ("b", source(2)),
        ],
    ))
    .unwrap();
    initial_wire["assets"]["media"]["audio"] = serde_json::to_value(source_span()).unwrap();
    // Install the audio-bearing recipe only after the fixture asset has audio.
    initial_wire["nodes"]["tiny-tone"] =
        serde_json::to_value(BeatNode::hold("Tiny tone", tone)).unwrap();
    let initial = ProjectDocument::from_json(&initial_wire.to_string()).unwrap();
    // All changes after initial admission use public commands. The unrelated
    // insertion captures the tone's complete1/100001-frame old raw carrier.
    let captured = edit(&initial, pause(&initial, "capture-old", 0, 1, 0));
    assert!(
        captured
            .audio_bindings()
            .bindings()
            .contains_key(&id("tiny-tone"))
    );
    let expanded = edit(
        &captured,
        update(&captured, "ordinary-expansion", expansion),
    );
    assert!(expanded.derived_clocks().is_empty());
    assert_eq!(origin(&expanded, "slow"), ratio(3, 1));
    // At root4 the cut is just inside the silent remainder, requiring an
    // exact fractional Partition. The tone itself stays a full physical Hold.
    let required = retimed_target(&expanded, 4).required_ids;
    edit(&expanded, pause(&expanded, "exact", 4, 3, required))
}

#[test]
fn speed_update_preserves_empty_old_raw_carrier_when_new_allocation_has_samples() {
    let before = empty_old_carrier_history(100_000);
    assert_eq!(
        before.derived_clocks()[&id("slow")].extent,
        ratio(100_003, 1)
    );
    let path = InstancePath {
        node: id("tiny-tone"),
        repeats: vec![],
    };
    let frozen = FrozenAudioLayout::capture(&before).unwrap();
    let (current, raw_support) = frozen
        .project_scoped_with_support(frozen.root(), &path, None, MAX_DOCUMENT_NODES)
        .unwrap();
    // Meaningful support ignores transparent Partition allocation. This is
    // the entire raw recipe, not merely its currently visible fragment.
    assert_eq!(raw_support, ExactRatio::ZERO..ExactRatio::ONE);
    let bound = before
        .audio_bindings()
        .resolve_in(
            &id("tiny-tone"),
            AudioBindingEnvironment::Occurrence(&path),
            MAX_DOCUMENT_NODES,
        )
        .unwrap();
    assert_eq!(
        bound.lattice.sample_boundary(raw_support.start).unwrap(),
        bound.lattice.sample_boundary(raw_support.end).unwrap()
    );
    let (_, visible) = frozen
        .project_scoped_with_allocation(frozen.root(), &path, None, MAX_DOCUMENT_NODES)
        .unwrap();
    assert_eq!(visible, Some(raw_support));
    let factor = ratio(1000, 100_003);
    let new_end = current
        .origin
        .checked_add(current.frames_per_local_frame.checked_mul(factor).unwrap())
        .unwrap();
    let first_sample = current
        .origin
        .checked_div(ratio(5, 8008))
        .unwrap()
        .round_even()
        .unwrap();
    let last_sample = new_end
        .checked_div(ratio(5, 8008))
        .unwrap()
        .round_even()
        .unwrap();
    assert!(last_sample > first_sample);
    let transfer = ratio(5, 8008)
        .checked_div(current.frames_per_local_frame)
        .unwrap()
        .checked_div(factor)
        .unwrap()
        .checked_div(bound.lattice.local_frames_per_sample().unwrap())
        .unwrap();
    assert_eq!(transfer, ratio(100_003, 100_000 * 1000));
    assert!(
        transfer
            .checked_mul(ratio(64, 1))
            .unwrap()
            .compare_integer(1)
            .is_lt()
    );
    let snapshot = before.to_json().unwrap();
    let command = update(&before, "empty-old-carrier-speed", 1000);
    let outcome = apply(&before, &command);
    retain_witness(&before, &command, &outcome);
    assert_eq!(before.to_json().unwrap(), snapshot);
    // Expected pre-fix failure: retained-transfer preflight rejects a carrier
    // whose complete old raw domain has no samples to transfer.
    before
        .validate_retime_update(&id("slow"), duration(1000), PitchPolicy::FollowSpeed)
        .unwrap();
    let transaction = outcome.unwrap();
    let after = transaction.forward.apply(&before).unwrap();
    assert_eq!(
        after.audio_bindings().bindings()[&id("tiny-tone")],
        before.audio_bindings().bindings()[&id("tiny-tone")]
    );
    assert_eq!(transaction.inverse.apply(&after).unwrap(), before);
}

#[test]
fn speed_update_captures_empty_unbound_old_carrier_when_new_allocation_has_samples() {
    let history = empty_old_carrier_history(2);
    let mut wire = serde_json::to_value(history).unwrap();
    wire["audio_bindings"]["bindings"]
        .as_object_mut()
        .unwrap()
        .remove("tiny-tone")
        .expect("history captured tone binding");
    prune_unreferenced_fixture_timings(&mut wire);
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    assert_eq!(before.derived_clocks()[&id("slow")].extent, ratio(5, 1));
    let path = InstancePath {
        node: id("tiny-tone"),
        repeats: vec![],
    };
    let frozen = FrozenAudioLayout::capture(&before).unwrap();
    let (current, support) = frozen
        .project_scoped_with_support(frozen.root(), &path, None, MAX_DOCUMENT_NODES)
        .unwrap();
    assert_eq!(support, ExactRatio::ZERO..ExactRatio::ONE);
    assert_eq!(current.frames_per_local_frame, ratio(2, 100_001));
    let old_sample = |local: ExactRatio| {
        current
            .origin
            .checked_add(local.checked_mul(current.frames_per_local_frame).unwrap())
            .unwrap()
            .checked_div(ratio(5, 8008))
            .unwrap()
            .round_even()
            .unwrap()
    };
    assert_eq!(old_sample(support.start), old_sample(support.end));
    let prospective_capture = capture_unbound_audio_bindings(
        &before,
        AudioTimingId {
            allocation: revision("fresh-empty-probe"),
            ordinal: 0,
        },
    )
    .unwrap();
    let prospective = prospective_capture
        .resolve_in(
            &id("tiny-tone"),
            AudioBindingEnvironment::Occurrence(&path),
            MAX_DOCUMENT_NODES,
        )
        .unwrap();
    assert_eq!(prospective.lattice.origin, current.origin);
    assert_eq!(
        prospective.lattice.frames_per_local_frame,
        current.frames_per_local_frame
    );
    assert_eq!(
        prospective.lattice.sample_boundary(support.start).unwrap(),
        prospective.lattice.sample_boundary(support.end).unwrap()
    );
    let candidate_end = current
        .origin
        .checked_add(
            current
                .frames_per_local_frame
                .checked_mul(ratio(200, 1))
                .unwrap(),
        )
        .unwrap()
        .checked_div(ratio(5, 8008))
        .unwrap()
        .round_even()
        .unwrap();
    assert!(candidate_end > old_sample(support.start));
    // Commit captures the missing carrier on its current clock before the
    // speed update. Its transfer5/1000 is below1/64, but its entire old raw
    // recipe has zero samples and the renderer returns silence without DSP.
    assert!(
        ratio(1, 200)
            .checked_mul(ratio(64, 1))
            .unwrap()
            .compare_integer(1)
            .is_lt()
    );
    let snapshot = before.to_json().unwrap();
    let command = update(&before, "empty-fresh-carrier-speed", 1000);
    let outcome = apply(&before, &command);
    retain_witness(&before, &command, &outcome);
    assert_eq!(before.to_json().unwrap(), snapshot);
    before
        .validate_retime_update(&id("slow"), duration(1000), PitchPolicy::FollowSpeed)
        .unwrap();
    let transaction = outcome.unwrap();
    let after = transaction.forward.apply(&before).unwrap();
    let captured = after
        .audio_bindings()
        .resolve_in(
            &id("tiny-tone"),
            AudioBindingEnvironment::Occurrence(&path),
            MAX_DOCUMENT_NODES,
        )
        .unwrap();
    assert_eq!(
        captured.lattice.sample_boundary(support.start).unwrap(),
        captured.lattice.sample_boundary(support.end).unwrap()
    );
    for (owner, binding) in before.audio_bindings().bindings() {
        if !before.children(&id("root")).any(|child| child == owner) {
            assert_eq!(after.audio_bindings().bindings()[owner], *binding);
        }
    }
    assert_eq!(transaction.inverse.apply(&after).unwrap(), before);
}
