use super::*;

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
    assert!(after.audio_bindings().bindings().contains_key(&id("p-2")));
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

    // Outside the speed change, exact clocks still refuse new pauses.
    let message = apply(&second, &pause(&second, "r", 0, 1, 3))
        .unwrap_err()
        .message;
    assert!(message.contains("FollowSpeed"), "{message}");
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
fn preserve_speed_changes_and_attached_marks_still_refuse() {
    let mut wire = serde_json::to_value(fixture()).unwrap();
    wire["nodes"]["slow"]["kind"]["pitch"] = json!("preserve");
    let preserve = ProjectDocument::from_json(&wire.to_string()).unwrap();
    assert!(apply(&preserve, &pause(&preserve, "p", 6, 2, 5)).is_err());
    assert!(!matches!(
        preserve.pause_insertion_target(ProjectFrame(6)),
        Ok(PauseInsertionTarget::Retimed(_))
    ));
}
