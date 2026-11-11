use super::*;

#[path = "retimed/speed_update.rs"]
mod speed_update;

fn ratio(numerator: i128, denominator: i128) -> ExactRatio {
    ExactRatio::new(numerator, denominator).unwrap()
}

/// `slow` plays its 12-frame child over 24 frames: two output frames per
/// child frame, so a pause of `h` frames spans `h/2` child frames.
fn fixture() -> ProjectDocument {
    tree(
        &["a", "slow", "b"],
        vec![
            ("a", source(2)),
            (
                "slow",
                BeatNode {
                    audio_treatments: Default::default(),
                    label: "Slow".into(),
                    framing: None,
                    audio_editorial_edges: Default::default(),
                    audio_edges: Default::default(),
                    kind: NodeKind::Retime {
                        child: id("inner"),
                        duration: duration(24),
                        mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(12)).unwrap(),
                        pitch: PitchPolicy::FollowSpeed,
                        purpose: RetimePurpose::Edit,
                    },
                    cutaways: Vec::new(),
                    captions: Vec::new(),
                },
            ),
            ("inner", source(12)),
            ("b", source(2)),
        ],
    )
}

fn pause(
    document: &ProjectDocument,
    name: &str,
    at: i64,
    frames: i64,
    ids: usize,
) -> CommandRequest {
    request(
        document,
        name,
        Command::InsertTime {
            at: ProjectFrame(at),
            hold: recipe(frames),
            id: id(&format!("pause-{name}")),
            identities: pool(name, ids),
            timing: AudioTimingId {
                allocation: revision(name),
                ordinal: 0,
            },
        },
    )
}

fn retimed_target(document: &ProjectDocument, at: i64) -> RetimedPauseTarget {
    match document.pause_insertion_target(ProjectFrame(at)).unwrap() {
        PauseInsertionTarget::Retimed(target) => target,
        other => panic!("expected a retimed pause target, got {other:?}"),
    }
}

fn retime(document: &ProjectDocument, name: &str) -> (FrameDuration, FrameRange, NodeId) {
    let NodeKind::Retime {
        duration,
        mapping,
        child,
        ..
    } = &document.nodes()[&id(name)].kind
    else {
        panic!("{name} is not a Retime")
    };
    (*duration, *mapping, child.clone())
}

#[test]
fn integral_child_cuts_keep_ordinary_recipes_and_the_live_speed_owner() {
    let before = fixture();
    let target = retimed_target(&before, 6);
    assert_eq!(target.retime, id("slow"));
    assert_eq!(target.navigation_parent, id("root"));
    assert_eq!(target.picture_parent, id("slow"));
    // Left and right windows, the copied Source, a container below the
    // Retime and the speed adapter.
    assert_eq!(target.required_ids, 5);
    // Child frame 2 is whole; a two-frame pause spans one child frame, a
    // three-frame pause 3/2. A cut at frame 5 lands at child frame 3/2.
    assert!(!target.needs_exact_clocks(duration(2)));
    assert!(target.needs_exact_clocks(duration(3)));
    assert!(retimed_target(&before, 5).needs_exact_clocks(duration(2)));

    let after = edit(&before, pause(&before, "p", 6, 2, 5));
    assert!(after.derived_clocks().is_empty());
    assert_eq!(after.duration().unwrap(), duration(30));
    let (output, window, container) = retime(&after, "slow");
    assert_eq!(output, duration(26));
    assert_eq!(
        window,
        FrameRange::new(ProjectFrame(0), ProjectFrame(13)).unwrap()
    );
    assert_eq!(container, id("p-3"));
    let NodeKind::Sequence { children } = &after.nodes()[&container].kind else {
        panic!("container")
    };
    assert_eq!(children, &[id("p-0"), id("p-4"), id("p-1")]);
    let (_, left, inner) = retime(&after, "p-0");
    assert_eq!((left.start().0, left.end().0, inner), (0, 2, id("inner")));
    let (_, right, copy) = retime(&after, "p-1");
    assert_eq!((right.start().0, right.end().0, copy), (2, 12, id("p-2")));
    // The authored pause keeps its project duration below the 2x adapter.
    let (adapted, held, hold) = retime(&after, "p-4");
    assert_eq!(
        (adapted, held.end().0, hold),
        (duration(1), 2, id("pause-p"))
    );
    assert_eq!(after.durations().unwrap()[&id("pause-p")], duration(2));
    // The copied right fragment resumes its own retained clock.
}

#[test]
fn fractional_child_cuts_author_exact_clocks_and_accept_later_pauses() {
    let before = fixture();
    let first = edit(&before, pause(&before, "p", 5, 3, 5));
    let clocks = first.derived_clocks();
    let clock = |name: &str| {
        let clock = &clocks[&id(name)];
        (clock.extent, clock.child_start, clock.child_end)
    };
    assert_eq!(clock("p-0"), (ratio(3, 2), ExactRatio::ZERO, ratio(3, 2)));
    assert_eq!(clock("p-1"), (ratio(21, 2), ratio(3, 2), ratio(12, 1)));
    assert_eq!(clock("p-4"), (ratio(3, 2), ExactRatio::ZERO, ratio(3, 1)));
    assert_eq!(
        clock("slow"),
        (ratio(27, 1), ExactRatio::ZERO, ratio(27, 2))
    );
    let exact = first.exact_durations().unwrap();
    assert_eq!(exact[&id("root")], ratio(31, 1));
    assert_eq!(exact[&id("pause-p")], ratio(3, 1));
    // Ordinary integral queries still refuse the document; exact ones remain.
    assert!(first.durations().is_err());
    // Integral recipes above the speed change still total the exact length.
    assert_eq!(
        first.recipe_durations_shared().unwrap()[first.root()],
        duration(31)
    );

    // A second pause lands inside the right fragment, which is refined in
    // place: one new window, its copied Source and the adapter.
    let target = retimed_target(&first, 22);
    assert_eq!(target.required_ids, 3);
    let second = edit(&first, pause(&first, "q", 22, 2, 3));
    let exact = second.exact_durations().unwrap();
    assert_eq!(exact[&id("root")], ratio(33, 1));
    let clocks = second.derived_clocks();
    assert_eq!(
        (clocks[&id("p-1")].child_start, clocks[&id("p-1")].child_end),
        (ratio(3, 2), ratio(17, 2))
    );
    let (_, right, _) = retime(&second, "q-0");
    assert_eq!(
        (clocks[&id("q-0")].child_start, clocks[&id("q-0")].child_end),
        (ratio(17, 2), ratio(12, 1))
    );
    assert_eq!(right.end().0, 12);
    // The 2-frame pause spans exactly one child frame: an ordinary adapter.
    assert!(!clocks.contains_key(&id("q-2")));
    assert_eq!(retime(&second, "q-2").0, duration(1));

    // Outside the speed change, the ordinary path moves it whole.
    let outside = edit(&second, pause(&second, "r", 0, 1, 3));
    assert_eq!(outside.derived_clocks(), second.derived_clocks());
}

#[test]
fn a_root_seam_before_a_plain_speed_change_inserts_a_pause() {
    let before = fixture();
    let after = edit(&before, insertion(&before, "seam", 2, 4));
    assert_eq!(after.duration().unwrap(), duration(32));
}

#[test]
fn seams_inside_the_speed_change_need_only_the_adapter() {
    let mut document = fixture();
    // Give the speed change a two-child group with a seam at child frame 4.
    let mut wire = serde_json::to_value(&document).unwrap();
    wire["nodes"]["slow"]["kind"]["child"] = json!("group");
    wire["nodes"]["group"] =
        serde_json::to_value(BeatNode::sequence("Group", vec![id("inner"), id("tail")])).unwrap();
    wire["nodes"]["inner"] = serde_json::to_value(source(4)).unwrap();
    wire["nodes"]["tail"] = serde_json::to_value(source(8)).unwrap();
    document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let target = retimed_target(&document, 10);
    assert_eq!(
        (target.required_ids, target.picture_parent.clone()),
        (1, id("group"))
    );
    let after = edit(&document, pause(&document, "s", 10, 1, 1));
    let NodeKind::Sequence { children } = &after.nodes()[&id("group")].kind else {
        panic!("group")
    };
    assert_eq!(children, &[id("inner"), id("s-0"), id("tail")]);
    assert_eq!(after.derived_clocks()[&id("s-0")].extent, ratio(1, 2));
    assert_eq!(after.exact_durations().unwrap()[&id("root")], ratio(29, 1));
}

#[test]
fn only_silent_freeze_or_black_pauses_and_unmarked_projects_are_admitted() {
    let before = fixture();
    let mut request = pause(&before, "tone", 6, 2, 5);
    let Command::InsertTime { hold, .. } = &mut request.command else {
        unreachable!()
    };
    hold.audio = HoldAudio::Tone {
        frequency_hz: 1000,
        level: GainDb::new(-12_000).unwrap(),
    };
    assert!(
        apply(&before, &request)
            .unwrap_err()
            .message
            .contains("silent")
    );
    let mut wire = serde_json::to_value(&before).unwrap();
    wire["marks"] = json!({ "mark": super::mark(1, InsertionBias::Right, true) });
    let marked = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let message = marked
        .pause_insertion_target(ProjectFrame(6))
        .unwrap_err()
        .message;
    assert!(message.contains("marks"), "{message}");
}

#[test]
fn pitch_preserving_speed_changes_use_the_ordinary_split_path() {
    let mut wire = serde_json::to_value(fixture()).unwrap();
    wire["nodes"]["slow"]["kind"]["pitch"] = json!("preserve");
    let preserve = ProjectDocument::from_json(&wire.to_string()).unwrap();
    assert!(matches!(
        preserve.pause_insertion_target(ProjectFrame(6)),
        Ok(PauseInsertionTarget::Ordinary(_))
    ));
}

#[test]
fn ordinary_edits_move_copy_and_keep_clocked_speed_changes_but_refuse_inside_them() {
    let before = fixture();
    let clocked = edit(&before, pause(&before, "p", 5, 3, 5));
    let total = |document: &ProjectDocument| document.exact_durations().unwrap()[document.root()];
    assert_eq!(total(&clocked), ratio(31, 1));

    // Root seams before and after the speed change move it whole.
    assert!(matches!(
        clocked.pause_insertion_target(ProjectFrame(2)).unwrap(),
        PauseInsertionTarget::Ordinary(_)
    ));
    let early = edit(&clocked, insertion(&clocked, "early", 2, 4));
    assert_eq!(total(&early), ratio(35, 1));
    assert_eq!(early.derived_clocks(), clocked.derived_clocks());
    let late = edit(&early, insertion(&early, "late", 33, 1));
    assert_eq!(total(&late), ratio(36, 1));

    // A Hold outside the speed change keeps ordinary editing.
    let longer = edit(
        &late,
        request(
            &late,
            "longer",
            Command::SetHoldDuration {
                node: id("pause-early"),
                duration: duration(6),
            },
        ),
    );
    assert_eq!(total(&longer), ratio(38, 1));

    // Inside it, a pause duration propagates exactly through the adapter and
    // the speed change: 4 frames span 2 child frames, so both become whole.
    let lengthened = edit(
        &longer,
        request(
            &longer,
            "inside",
            Command::SetHoldDuration {
                node: id("pause-p"),
                duration: duration(4),
            },
        ),
    );
    assert_eq!(total(&lengthened), ratio(39, 1));
    let clocks = lengthened.derived_clocks();
    assert!(!clocks.contains_key(&id("p-4")));
    assert_eq!(retime(&lengthened, "p-4").0, duration(2));
    // The speed change is whole again, so it keeps an ordinary recipe.
    assert!(!clocks.contains_key(&id("slow")));
    let (output, window, _) = retime(&lengthened, "slow");
    assert_eq!((output, window.end().0), (duration(28), 14));
    // Attributes outside timing change freely inside it: a label and a
    // Camera pose on the pause, keeping every clock.
    let renamed = edit(
        &lengthened,
        request(
            &lengthened,
            "renamed",
            Command::Rename {
                node: id("pause-p"),
                label: "Long stare".into(),
            },
        ),
    );
    let framed = edit(
        &renamed,
        request(
            &renamed,
            "framed",
            Command::SetFraming {
                node: id("pause-p"),
                framing: Some(Framing {
                    clock: Default::default(),
                    value: FramingValue::Static {
                        pose: FramingPose::new(ratio(1, 3), ratio(1, 2), ratio(2, 1)).unwrap(),
                    },
                }),
            },
        ),
    );
    assert_eq!(framed.derived_clocks(), lengthened.derived_clocks());
    assert_eq!(total(&framed), ratio(39, 1));
    // Other changes inside it would read integral recipes, and refuse.
    let inside = apply(
        &longer,
        &request(
            &longer,
            "inside-split",
            Command::Split {
                node: id("p-1"),
                at: duration(2),
                identities: pool("inside-split", 8),
            },
        ),
    )
    .unwrap_err();
    assert!(
        inside.message.contains("speed change"),
        "{}",
        inside.message
    );

    // Splitting the speed change at a whole project frame copies its clocked
    // content exactly beneath two transparent windows.
    let split = edit(
        &longer,
        request(
            &longer,
            "cut",
            Command::Split {
                node: id("slow"),
                at: duration(10),
                identities: pool("cut", 16),
            },
        ),
    );
    assert_eq!(total(&split), total(&longer));
    assert_eq!(
        split.derived_clocks().len(),
        2 * longer.derived_clocks().len()
    );
    // The copy keeps exact geometry: a later pause in the right copy works.
    let NodeKind::Sequence { children } = &split.nodes()[split.root()].kind else {
        panic!("root")
    };
    assert_eq!(children.len(), 6);
    let again = edit(&split, pause(&split, "again", 26, 2, 8));
    assert_eq!(total(&again), ratio(40, 1));
}

#[test]
fn grouping_and_deleting_a_clocked_speed_change_keep_or_drop_its_clocks() {
    let before = fixture();
    let clocked = edit(&before, pause(&before, "p", 5, 3, 5));
    let grouped = edit(
        &clocked,
        request(
            &clocked,
            "group",
            Command::Group {
                parent: id("root"),
                start: 0,
                end: 2,
                id: id("group"),
                label: "Group".into(),
            },
        ),
    );
    assert_eq!(grouped.derived_clocks(), clocked.derived_clocks());
    assert_eq!(
        grouped.exact_durations().unwrap()[&id("group")],
        ratio(29, 1)
    );
    let deleted = edit(
        &clocked,
        request(
            &clocked,
            "drop",
            Command::DeleteRipple {
                node: id("slow"),
                timing: AudioTimingId {
                    allocation: revision("drop"),
                    ordinal: 0,
                },
            },
        ),
    );
    assert!(deleted.derived_clocks().is_empty());
    assert_eq!(deleted.duration().unwrap(), duration(4));
}

fn preserve_fixture(framed: bool) -> ProjectDocument {
    let mut wire = serde_json::to_value(fixture()).unwrap();
    wire["nodes"]["slow"]["kind"]["pitch"] = json!("preserve");
    if framed {
        wire["nodes"]["slow"]["framing"] = serde_json::to_value(Framing {
            clock: Default::default(),
            value: FramingValue::Static {
                pose: FramingPose::new(ratio(1, 2), ratio(1, 2), ratio(3, 2)).unwrap(),
            },
        })
        .unwrap();
    }
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

#[test]
fn pitch_preserving_speed_changes_split_into_full_contexts_around_the_pause() {
    let before = preserve_fixture(false);
    let target = before.insert_time_target(ProjectFrame(5)).unwrap();
    assert_eq!(target.parent, id("root"));
    assert_eq!(target.lift, None);
    let split = target.split.clone().unwrap();
    assert_eq!((split.target, split.at), (id("slow"), duration(3)));
    let after = edit(&before, pause(&before, "p", 5, 3, split.required_ids));
    assert!(after.derived_clocks().is_empty());
    assert_eq!(after.duration().unwrap(), duration(31));
    let NodeKind::Sequence { children } = &after.nodes()[after.root()].kind else {
        panic!("root")
    };
    assert_eq!(children[2], id("pause-p"));
    // Both halves window a complete copy of the processing stage.
    let (_, left, inner) = retime(&after, &children[1].to_string());
    assert_eq!((left.start().0, left.end().0, inner), (0, 3, id("slow")));
    let (_, right, copy) = retime(&after, &children[3].to_string());
    assert_eq!((right.start().0, right.end().0), (3, 24));
    let speed = |document: &ProjectDocument, node: &NodeId| {
        let NodeKind::Retime {
            duration,
            mapping,
            pitch,
            ..
        } = &document.nodes()[node].kind
        else {
            panic!("speed change")
        };
        (*duration, *mapping, *pitch)
    };
    assert_eq!(speed(&after, &copy), speed(&before, &id("slow")));
}

#[test]
fn a_framed_pitch_preserving_speed_change_lifts_its_framing_over_the_pause() {
    let before = preserve_fixture(true);
    let target = before.insert_time_target(ProjectFrame(5)).unwrap();
    assert_eq!(target.lift, Some(id("slow")));
    // Picture capture stops below the lifted speed change.
    assert_eq!(
        before
            .pause_insertion_target(ProjectFrame(5))
            .unwrap()
            .picture_parent()
            .node,
        id("slow")
    );
    let required = target.split.unwrap().required_ids;
    let after = edit(&before, pause(&before, "p", 5, 3, required));
    let group = id(&format!("p-{}", required - 1));
    let NodeKind::Sequence { children } = &after.nodes()[&group].kind else {
        panic!("lifted group")
    };
    assert_eq!(children.len(), 3);
    assert_eq!(children[1], id("pause-p"));
    assert_eq!(
        after.nodes()[&group].framing,
        before.nodes()[&id("slow")].framing
    );
    assert!(after.nodes()[&id("slow")].framing.is_none());
    assert_eq!(after.duration().unwrap(), duration(31));
}

#[test]
fn moving_framing_on_a_group_inside_a_pitch_preserving_speed_change_refuses() {
    let mut wire = serde_json::to_value(preserve_fixture(false)).unwrap();
    wire["nodes"]["slow"]["kind"]["child"] = json!("group");
    let mut group = BeatNode::sequence("Group", vec![id("inner")]);
    group.framing = Some(Framing {
        clock: Default::default(),
        value: FramingValue::Envelope {
            envelope: FramingEnvelope {
                initial: FramingPose::identity(),
                segments: vec![FramingSegment {
                    end: ratio(1, 1),
                    pose: FramingPose::new(ratio(1, 2), ratio(1, 2), ratio(2, 1)).unwrap(),
                    curve: FramingCurve::Linear,
                }],
            },
        },
    });
    wire["nodes"]["group"] = serde_json::to_value(group).unwrap();
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let message = document
        .insert_time_target(ProjectFrame(5))
        .unwrap_err()
        .message;
    assert!(message.contains("moving framing"), "{message}");
}

fn nested_fixture() -> ProjectDocument {
    let mut wire = serde_json::to_value(fixture()).unwrap();
    wire["nodes"]["slow"]["kind"]["child"] = json!("nested");
    wire["nodes"]["slow"]["kind"]["duration"] = json!(36);
    wire["nodes"]["slow"]["kind"]["mapping"] =
        serde_json::to_value(FrameRange::new(ProjectFrame(0), ProjectFrame(18)).unwrap()).unwrap();
    let mut nested = fixture().nodes()[&id("slow")].clone();
    let NodeKind::Retime {
        duration: frames, ..
    } = &mut nested.kind
    else {
        unreachable!()
    };
    *frames = duration(18);
    wire["nodes"]["nested"] = serde_json::to_value(nested).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

#[test]
fn nested_follow_speed_pauses_keep_each_live_clock_and_report_intermediate_fractions() {
    let before = nested_fixture();
    let target = retimed_target(&before, 8);
    assert_eq!(target.slope, ratio(1, 3));
    assert_eq!(target.picture_parent, id("nested"));
    assert_eq!(target.navigation_parent, id("root"));
    // The leaf cut/span are whole, but the inner stage's output grows by 3/2.
    assert!(target.needs_exact_clocks(duration(3)));
    assert!(!target.needs_exact_clocks(duration(6)));
    let after = edit(
        &before,
        pause(&before, "nested-first", 8, 3, target.required_ids),
    );
    assert_eq!(after.exact_durations().unwrap()[after.root()], ratio(43, 1));
    let outer = &after.derived_clocks()[&id("slow")];
    let inner = &after.derived_clocks()[&id("nested")];
    assert_eq!(
        (outer.extent, outer.child_end),
        (ratio(39, 1), ratio(39, 2))
    );
    assert_eq!(
        (inner.extent, inner.child_end),
        (ratio(39, 2), ratio(13, 1))
    );
    assert_eq!(
        outer.child_end.checked_div(outer.extent).unwrap(),
        ratio(1, 2)
    );
    assert_eq!(
        inner.child_end.checked_div(inner.extent).unwrap(),
        ratio(2, 3)
    );
    // A new pause inside the first pause descends through its speed adapter.
    let target = retimed_target(&after, 9);
    let second = edit(
        &after,
        pause(&after, "nested-second", 9, 2, target.required_ids),
    );
    assert_eq!(
        second.exact_durations().unwrap()[second.root()],
        ratio(45, 1)
    );
    assert_eq!(
        second.exact_durations().unwrap()[&id("pause-nested-second")],
        ratio(2, 1)
    );
    assert_eq!(second.derived_clocks()[&id("slow")].extent, ratio(41, 1));
}

#[test]
fn nested_follow_speed_pauses_refuse_processing_stages_and_insufficient_identities_atomically() {
    let before = nested_fixture();
    let target = retimed_target(&before, 8);
    let snapshot = before.clone();
    assert!(
        apply(
            &before,
            &pause(&before, "short-pool", 8, 3, target.required_ids - 1)
        )
        .is_err()
    );
    assert_eq!(before, snapshot);
}

/// `slow` (tape speed, 36 frames over 18) holds a pitch-preserving `nested`
/// stage (18 frames over 12). A pause splits `nested` inside `slow`'s child.
fn follow_speed_over_processing(pitch: serde_json::Value) -> ProjectDocument {
    let mut wire = serde_json::to_value(nested_fixture()).unwrap();
    wire["nodes"]["nested"]["kind"]["pitch"] = pitch;
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

#[test]
fn a_processing_stage_inside_follow_speed_splits_into_full_contexts_at_whole_child_frames() {
    for pitch in [json!("preserve"), json!({"shift": {"semitones": 3}})] {
        let before = follow_speed_over_processing(pitch);
        let target = retimed_target(&before, 8);
        assert_eq!(target.retime, id("slow"));
        assert_eq!(target.picture_parent, id("slow"));
        assert!(!target.needs_exact_clocks(duration(2)));
        assert!(target.needs_exact_clocks(duration(3)));
        let snapshot = before.clone();
        // Root frame 8 is child frame 3 of `slow`: a whole output frame of
        // `nested`. Two pause frames fill exactly one child frame.
        let after = edit(&before, pause(&before, "whole", 8, 2, target.required_ids));
        assert_eq!(before, snapshot);
        assert!(after.derived_clocks().is_empty());
        assert_eq!(after.duration().unwrap(), duration(42));
        let (extent, mapping, container) = retime(&after, "slow");
        assert_eq!((extent, mapping.end().0), (duration(38), 19));
        let NodeKind::Sequence { children } = &after.nodes()[&container].kind else {
            panic!("pause container")
        };
        assert_eq!(children.len(), 3);
        let (_, left, inner) = retime(&after, &children[0].to_string());
        assert_eq!((left.start().0, left.end().0, inner), (0, 3, id("nested")));
        let (_, right, copy) = retime(&after, &children[2].to_string());
        assert_eq!((right.start().0, right.end().0), (3, 18));
        // The right half keeps a complete copy of the processing stage.
        let speed = |document: &ProjectDocument, node: &NodeId| {
            let NodeKind::Retime {
                duration,
                mapping,
                pitch,
                ..
            } = &document.nodes()[node].kind
            else {
                panic!("speed change")
            };
            (*duration, *mapping, *pitch)
        };
        assert_eq!(speed(&after, &copy), speed(&before, &id("nested")));
        assert_eq!(speed(&after, &id("nested")), speed(&before, &id("nested")));
        // The pause keeps its authored frames through the tape-speed adapter.
        let (adapter_extent, adapter_map, hold) = retime(&after, &children[1].to_string());
        assert_eq!((adapter_extent, adapter_map.end().0), (duration(1), 2));
        assert_eq!(hold, id("pause-whole"));

        // A fractional split or child span would need a derived clock, which
        // cannot coexist with processing: refuse atomically and readably.
        for (at, frames) in [(8, 3), (9, 2)] {
            let target = retimed_target(&before, at);
            let error = apply(
                &before,
                &pause(&before, "fractional", at, frames, target.required_ids),
            )
            .unwrap_err();
            assert!(
                error.message.contains("fractional frame"),
                "{}",
                error.message
            );
            assert_eq!(before, snapshot);
        }
    }
    // A treated inner stage cannot lift its effects out of the tape-speed
    // child without changing their clock.
    let mut wire = serde_json::to_value(follow_speed_over_processing(json!("preserve"))).unwrap();
    wire["nodes"]["nested"]["framing"] = serde_json::to_value(Framing {
        clock: Default::default(),
        value: FramingValue::Static {
            pose: FramingPose::new(ratio(1, 2), ratio(1, 2), ratio(3, 2)).unwrap(),
        },
    })
    .unwrap();
    let treated = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let error = treated.pause_insertion_target(ProjectFrame(8)).unwrap_err();
    assert!(
        error.message.contains("framing, gain or edges"),
        "{}",
        error.message
    );
}

/// `slow` (`outer` pitch, 36 frames over 18) holds `nested` (`inner` pitch,
/// 18 frames over 12): two nested pitch-processing stages.
fn nested_processing(outer: serde_json::Value, inner: serde_json::Value) -> ProjectDocument {
    let mut wire = serde_json::to_value(nested_fixture()).unwrap();
    wire["nodes"]["slow"]["kind"]["pitch"] = outer;
    wire["nodes"]["nested"]["kind"]["pitch"] = inner;
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

#[test]
fn nested_processing_stages_split_the_outermost_and_keep_every_inner_stage_whole() {
    let shift = json!({"shift": {"semitones": 3}});
    for (outer, inner) in [
        (json!("preserve"), json!("preserve")),
        (shift.clone(), json!("preserve")),
        (json!("preserve"), shift.clone()),
        (json!("preserve"), json!("follow_speed")),
    ] {
        let before = nested_processing(outer, inner);
        let speed = |document: &ProjectDocument, node: &NodeId| {
            let NodeKind::Retime {
                duration,
                mapping,
                pitch,
                ..
            } = &document.nodes()[node].kind
            else {
                panic!("speed change")
            };
            (*duration, *mapping, *pitch)
        };
        // Root frame 8 is output frame 6 of the outer stage. The inner
        // boundary, 6/2 = 3 child frames in, is never cut: both copies of
        // the outer stage carry the inner stage whole.
        let target = before.insert_time_target(ProjectFrame(8)).unwrap();
        assert_eq!(
            (target.parent.clone(), target.lift.clone()),
            (id("root"), None)
        );
        let split = target.split.unwrap();
        assert_eq!((split.target, split.at), (id("slow"), duration(6)));
        let after = edit(&before, pause(&before, "first", 8, 3, split.required_ids));
        assert!(after.derived_clocks().is_empty());
        assert_eq!(after.duration().unwrap(), duration(43));
        let root = children(&after).to_vec();
        assert_eq!(root.len(), 5);
        assert_eq!(
            (&root[0], &root[2], &root[4]),
            (&id("a"), &id("pause-first"), &id("b"))
        );
        let (_, left, stage) = retime(&after, &root[1].to_string());
        assert_eq!((left.start().0, left.end().0, &stage), (0, 6, &id("slow")));
        let (_, right, copy) = retime(&after, &root[3].to_string());
        assert_eq!((right.start().0, right.end().0), (6, 36));
        assert_eq!(speed(&after, &copy), speed(&before, &id("slow")));
        assert_eq!(speed(&after, &stage), speed(&before, &id("slow")));
        let inner_copy = retime(&after, &copy.to_string()).2;
        assert_ne!(inner_copy, id("nested"));
        assert_eq!(speed(&after, &inner_copy), speed(&before, &id("nested")));
        assert_eq!(speed(&after, &id("nested")), speed(&before, &id("nested")));

        // A second pause in the right half refines its window; each half
        // still windows a complete outer stage.
        let target = after.insert_time_target(ProjectFrame(16)).unwrap();
        let split = target.split.unwrap();
        assert_eq!((split.target, split.at), (root[3].clone(), duration(5)));
        let second = edit(&after, pause(&after, "second", 16, 2, split.required_ids));
        assert_eq!(second.duration().unwrap(), duration(45));
        let root = children(&second).to_vec();
        assert_eq!(root.len(), 7);
        assert_eq!(root[4], id("pause-second"));
        let (_, middle, _) = retime(&second, &root[3].to_string());
        let (_, last, last_copy) = retime(&second, &root[5].to_string());
        assert_eq!((middle.start().0, middle.end().0), (6, 11));
        assert_eq!((last.start().0, last.end().0), (11, 36));
        assert_eq!(speed(&second, &last_copy), speed(&before, &id("slow")));
    }
}

#[test]
fn moving_framing_on_a_nested_speed_change_inside_a_processing_stage_refuses() {
    let mut wire =
        serde_json::to_value(nested_processing(json!("preserve"), json!("preserve"))).unwrap();
    wire["nodes"]["nested"]["framing"] = serde_json::to_value(Framing {
        clock: Default::default(),
        value: FramingValue::Envelope {
            envelope: FramingEnvelope {
                initial: FramingPose::identity(),
                segments: vec![FramingSegment {
                    end: ratio(1, 1),
                    pose: FramingPose::new(ratio(1, 2), ratio(1, 2), ratio(2, 1)).unwrap(),
                    curve: FramingCurve::Linear,
                }],
            },
        },
    })
    .unwrap();
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let snapshot = document.clone();
    let message = document
        .insert_time_target(ProjectFrame(8))
        .unwrap_err()
        .message;
    assert!(message.contains("moving framing"), "{message}");
    assert!(apply(&document, &pause(&document, "moving", 8, 3, 20)).is_err());
    assert_eq!(document, snapshot);
}

#[test]
fn nested_follow_speed_pauses_keep_nonzero_partition_windows_live() {
    let mut wire = serde_json::to_value(nested_fixture()).unwrap();
    wire["nodes"]["slow"]["kind"]["child"] = json!("cropped");
    wire["nodes"]["slow"]["kind"]["duration"] = json!(24);
    wire["nodes"]["slow"]["kind"]["mapping"] =
        serde_json::to_value(FrameRange::new(ProjectFrame(0), ProjectFrame(12)).unwrap()).unwrap();
    let mut cropped = nested_fixture().nodes()[&id("nested")].clone();
    let NodeKind::Retime {
        child,
        duration: frames,
        mapping,
        purpose,
        ..
    } = &mut cropped.kind
    else {
        unreachable!()
    };
    *child = id("nested");
    *frames = duration(12);
    *mapping = FrameRange::new(ProjectFrame(3), ProjectFrame(15)).unwrap();
    *purpose = RetimePurpose::Partition;
    wire["nodes"]["cropped"] = serde_json::to_value(cropped).unwrap();
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let target = retimed_target(&before, 8);
    assert_eq!(target.slope, ratio(1, 3));
    assert_eq!(target.picture_parent, id("nested"));
    let after = edit(
        &before,
        pause(&before, "cropped-pause", 8, 3, target.required_ids),
    );
    assert_eq!(after.exact_durations().unwrap()[after.root()], ratio(31, 1));
    let window = &after.derived_clocks()[&id("cropped")];
    assert_eq!(
        (window.extent, window.child_start, window.child_end),
        (ratio(27, 2), ratio(3, 1), ratio(33, 2))
    );
    let nested = &after.derived_clocks()[&id("nested")];
    assert_eq!(
        (nested.extent, nested.child_start, nested.child_end),
        (ratio(39, 2), ratio(0, 1), ratio(13, 1))
    );
    // The site is child frame (3 + 6/2) * 2/3 = 4, not frame 2.
    let container = retime(&after, "nested").2;
    let left = after.children(&container).next().unwrap();
    let NodeKind::Retime { mapping, .. } = &after.nodes()[left].kind else {
        panic!("left partition")
    };
    assert_eq!(mapping.end(), ProjectFrame(4));
}
