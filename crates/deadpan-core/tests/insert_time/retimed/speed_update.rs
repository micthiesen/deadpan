//! Public-command regression proposal. Expected to fail on the retained old
//! source at SetRetime's exact-clock refusal, after positive controls pass.
use super::*;

#[path = "speed_update/source_crop.rs"]
mod source_crop;

#[path = "speed_update/inner_repeat.rs"]
mod inner_repeat;

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
    assert_eq!(after.exact_durations().unwrap()[after.root()], ratio(5, 1));
    assert_eq!(transaction.inverse.apply(&after).unwrap(), before);
}
