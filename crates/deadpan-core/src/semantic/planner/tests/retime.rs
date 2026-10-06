//! Speed, pitch and pause-duration instructions, and the ranged forms of
//! gain steps, cutaways and captions over a Visual range inside one beat.

use super::*;
use crate::{AudioChange, ExactRatio, GainDb, PauseLength, PitchPolicy, SemanticVisualSelection};

fn selected(child: &str, cursor: i64) -> SemanticContext {
    SemanticContext {
        selected_child: Some(node(child)),
        ..context("root", cursor)
    }
}

fn ranged(anchor: i64, head: i64) -> SemanticContext {
    SemanticContext {
        selected_child: Some(node("b")),
        visual_selection: Some(SemanticVisualSelection::Time {
            anchor: ProjectFrame(anchor),
            head: ProjectFrame(head),
            extending: false,
        }),
        ..context("root", anchor)
    }
}

fn speed(numerator: i128, denominator: i128) -> SemanticInstruction {
    SemanticInstruction::Retime {
        speed: ExactRatio::new(numerator, denominator).unwrap(),
        pitch: PitchPolicy::Preserve,
        wrap: false,
    }
}

fn replays_and_reverts(document: &ProjectDocument, planned: &SemanticPlan) {
    let replay = crate::replay_compound::<EditError>(
        document,
        planned.request.as_ref().unwrap(),
        |_| Ok(()),
    )
    .unwrap();
    assert_eq!(replay.document, planned.document);
    assert_eq!(
        &replay.edit.inverse.apply(&replay.document).unwrap(),
        document
    );
}

#[test]
fn speed_wraps_a_beat_then_updates_its_own_retime_from_the_exact_input() {
    let document = tree(&["a", "b"], vec![("a", hold(3)), ("b", hold(5))]);
    // Twice as fast: 5 / 2 = 2.5 rounds to even, 2 frames.
    let planned = plan(
        &document,
        selected("b", 3),
        vec![speed(2, 1)],
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(planned.document.duration().unwrap().frames(), 5);
    assert_eq!(planned.context.selected_child, Some(node("retime-0")));
    assert_eq!(planned.context.cursor, ProjectFrame(3));
    assert_eq!(planned.trace[0].resolved_range, Some(range(3, 5)));
    let NodeKind::Retime {
        child,
        duration,
        pitch,
        ..
    } = &planned.document.nodes()[&node("retime-0")].kind
    else {
        panic!("a new Retime wraps the beat")
    };
    assert_eq!(
        (child, duration.frames(), *pitch),
        (&node("b"), 2, PitchPolicy::Preserve)
    );
    replays_and_reverts(&document, &planned);

    // A second speed applies to the Retime's own 5-frame input, not to its
    // already shortened output; the same speed again is refused.
    let planned = plan(
        &document,
        selected("b", 3),
        vec![speed(2, 1), speed(1, 2)],
        &BTreeMap::new(),
    )
    .unwrap();
    let NodeKind::Retime { duration, .. } = &planned.document.nodes()[&node("retime-0")].kind
    else {
        panic!("still the same Retime")
    };
    assert_eq!(duration.frames(), 10);
    assert_eq!(planned.document.duration().unwrap().frames(), 13);
    assert!(
        plan(
            &document,
            selected("b", 3),
            vec![speed(2, 1), speed(2, 1)],
            &BTreeMap::new()
        )
        .is_err()
    );
    // wrap=true nests a second Retime around the first.
    let planned = plan(
        &document,
        selected("b", 3),
        vec![
            speed(2, 1),
            SemanticInstruction::Retime {
                speed: ExactRatio::new(1, 2).unwrap(),
                pitch: PitchPolicy::FollowSpeed,
                wrap: true,
            },
        ],
        &BTreeMap::new(),
    )
    .unwrap();
    assert!(matches!(
        &planned.document.nodes()[&node("retime-1")].kind,
        NodeKind::Retime { child, duration, pitch: PitchPolicy::FollowSpeed, .. }
            if child == &node("retime-0") && duration.frames() == 4
    ));
    // A speed that resolves to no frames, zero speed and a Visual range refuse.
    assert!(
        plan(
            &document,
            selected("b", 3),
            vec![speed(100, 1)],
            &BTreeMap::new()
        )
        .is_err()
    );
    assert!(
        SemanticProgram::new(vec![SemanticInstruction::Retime {
            speed: ExactRatio::ZERO,
            pitch: PitchPolicy::Preserve,
            wrap: false,
        }])
        .is_err()
    );
    assert!(plan(&document, ranged(3, 5), vec![speed(2, 1)], &BTreeMap::new()).is_err());
}

#[test]
fn pitch_keeps_the_speed_or_wraps_at_unity_and_zero_restores_preservation() {
    let document = tree(&["a", "b"], vec![("a", hold(3)), ("b", hold(6))]);
    let pitch = |semitones| SemanticInstruction::Pitch { semitones };
    let planned = plan(
        &document,
        selected("b", 3),
        vec![pitch(3)],
        &BTreeMap::new(),
    )
    .unwrap();
    assert!(matches!(
        &planned.document.nodes()[&node("retime-0")].kind,
        NodeKind::Retime { duration, pitch: PitchPolicy::Shift { semitones: 3 }, .. }
            if duration.frames() == 6
    ));
    replays_and_reverts(&document, &planned);
    // On a 2x Retime the shift keeps its exact speed; zero removes it.
    let planned = plan(
        &document,
        selected("b", 3),
        vec![speed(2, 1), pitch(-5), pitch(0)],
        &BTreeMap::new(),
    )
    .unwrap();
    assert!(matches!(
        &planned.document.nodes()[&node("retime-0")].kind,
        NodeKind::Retime { duration, pitch: PitchPolicy::Preserve, .. } if duration.frames() == 3
    ));
    // Nothing to remove on a plain beat; out-of-range shifts never plan.
    assert!(
        plan(
            &document,
            selected("b", 3),
            vec![pitch(0)],
            &BTreeMap::new()
        )
        .is_err()
    );
    assert!(SemanticProgram::new(vec![pitch(25)]).is_err());
}

#[test]
fn a_pause_duration_changes_only_the_selected_hold() {
    let document = tree(&["a", "b"], vec![("a", hold(3)), ("b", hold(6))]);
    let length = |frames| SemanticInstruction::SetHoldDuration {
        length: PauseLength::Frames {
            frames: NonZeroU32::new(frames).unwrap(),
        },
    };
    let planned = plan(
        &document,
        selected("b", 4),
        vec![length(9)],
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(planned.document.duration().unwrap().frames(), 12);
    assert_eq!(planned.context.cursor, ProjectFrame(3));
    assert_eq!(planned.trace[0].resolved_range, Some(range(3, 12)));
    replays_and_reverts(&document, &planned);
    assert!(
        plan(
            &document,
            selected("b", 4),
            vec![length(6)],
            &BTreeMap::new()
        )
        .is_err()
    );
    let wrapped = plan(
        &document,
        selected("b", 4),
        vec![speed(2, 1), length(4)],
        &BTreeMap::new(),
    );
    assert!(wrapped.is_err(), "a Retime is not a pause");
}

#[test]
fn gain_steps_over_a_visual_range_adjust_one_envelope_of_that_beat() {
    let document = tree(&["a", "b"], vec![("a", hold(3)), ("b", hold(6))]);
    let step = |millidecibels| SemanticInstruction::SetAudio {
        change: AudioChange::RangeStep { millidecibels },
    };
    // A whole-beat step refuses a Visual range, and a range step needs one,
    // so `.` never turns one into the other.
    let whole = SemanticInstruction::SetAudio {
        change: AudioChange::Step {
            millidecibels: 3_000,
        },
    };
    assert!(plan(&document, ranged(4, 6), vec![whole], &BTreeMap::new()).is_err());
    assert!(
        plan(
            &document,
            selected("b", 3),
            vec![step(3_000)],
            &BTreeMap::new()
        )
        .is_err()
    );
    // Edit [4, 6) is the beat's own frames [1, 3).
    let planned = plan(
        &document,
        ranged(4, 6),
        vec![step(3_000), step(3_000)],
        &BTreeMap::new(),
    )
    .unwrap();
    let clip = planned.document.nodes()[&node("b")]
        .audio_treatments
        .clip_gain()
        .unwrap()
        .clone();
    assert_eq!(clip.trim(), GainDb::UNITY, "the whole beat keeps its trim");
    assert_eq!(
        clip.envelopes().len(),
        1,
        "a second press adjusts the same range"
    );
    let local = crate::GainRange::new(ExactRatio::integer(1), ExactRatio::integer(3)).unwrap();
    assert_eq!(clip.range_step(local), Some(GainDb::new(6_000).unwrap()));
    let evaluated = |frame| {
        clip.evaluate(ExactRatio::integer(frame))
            .unwrap()
            .millidecibels
    };
    assert_eq!(evaluated(0), ExactRatio::ZERO);
    assert_eq!(evaluated(1), ExactRatio::integer(6_000));
    assert_eq!(evaluated(3), ExactRatio::ZERO, "the range is half-open");
    assert_eq!(planned.trace[0].resolved_range, Some(range(4, 6)));
    assert!(
        planned.context.visual_selection.is_some(),
        "the range stays selected for another press"
    );
    replays_and_reverts(&document, &planned);
    // Back to 0 dB removes the envelope.
    let planned = plan(
        &document,
        ranged(4, 6),
        vec![step(3_000), step(-3_000)],
        &BTreeMap::new(),
    )
    .unwrap();
    assert!(
        planned.document.nodes()[&node("b")]
            .audio_treatments
            .is_empty(),
        "the inert clip-gain stage is removed"
    );
    // A range across beats, and setting (not stepping) gain over a range, refuse.
    assert!(plan(&document, ranged(2, 5), vec![step(3_000)], &BTreeMap::new()).is_err());
    let trim = SemanticInstruction::SetAudio {
        change: AudioChange::Trim {
            gain: GainDb::new(-6_000).unwrap(),
        },
    };
    assert!(plan(&document, ranged(4, 6), vec![trim], &BTreeMap::new()).is_err());
}

#[test]
fn a_caption_over_a_visual_range_lands_on_that_range_of_its_beat() {
    let document = tree(&["a", "b"], vec![("a", hold(3)), ("b", hold(6))]);
    let caption = |delay: Option<u32>| SemanticInstruction::SetCaption {
        text: "Wait".into(),
        placement: crate::CaptionPlacement::Bottom,
        delay: delay
            .and_then(NonZeroU32::new)
            .map(|frames| PauseLength::Frames { frames }),
        reveal: None,
    };
    let planned = plan(
        &document,
        ranged(4, 7),
        vec![caption(None)],
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(
        planned.document.nodes()[&node("b")].captions[0].range,
        range(1, 4)
    );
    assert_eq!(planned.trace[0].resolved_range, Some(range(4, 7)));
    replays_and_reverts(&document, &planned);
    // A range already places the caption, so a delay refuses.
    assert!(
        plan(
            &document,
            ranged(4, 7),
            vec![caption(Some(1))],
            &BTreeMap::new()
        )
        .is_err()
    );
    assert!(
        plan(
            &document,
            ranged(2, 7),
            vec![caption(None)],
            &BTreeMap::new()
        )
        .is_err()
    );
}

#[test]
fn sound_edges_set_a_beats_ends_a_repeats_play_seams_or_its_gap_edges() {
    use crate::{AudioBoundaryKind as Kind, AudioEdgePolicy, EdgeSide};
    let document = tree(&["a", "b"], vec![("a", hold(3)), ("b", hold(5))]);
    let edges = |side, policy| SemanticInstruction::SetAudioEdges { side, policy };
    let planned = plan(
        &document,
        selected("b", 3),
        vec![edges(EdgeSide::Both, AudioEdgePolicy::Hard)],
        &BTreeMap::new(),
    )
    .unwrap();
    let b = planned.document.nodes()[&node("b")].audio_edges;
    assert_eq!(
        (b.get(Kind::NodeStart), b.get(Kind::NodeEnd)),
        (AudioEdgePolicy::Hard, AudioEdgePolicy::Hard)
    );
    assert_eq!(
        planned.document.duration(),
        document.duration(),
        "no time changes"
    );
    replays_and_reverts(&document, &planned);
    // Restating a policy, and play seams or gaps on a plain beat, refuse.
    for refused in [
        edges(EdgeSide::Start, AudioEdgePolicy::Automatic),
        edges(EdgeSide::Plays, AudioEdgePolicy::Hard),
        edges(EdgeSide::Gaps, AudioEdgePolicy::Hard),
    ] {
        assert!(plan(&document, selected("b", 3), vec![refused], &BTreeMap::new()).is_err());
    }
    // On a Repeat, play seams are its child's ends; gaps are its own.
    let wrapped = plan(
        &document,
        selected("b", 3),
        vec![
            SemanticInstruction::Repeat {
                selector: SemanticSelector::SelectedBeat,
                plays: NonZeroU32::new(3).unwrap(),
                escalation: None,
            },
            edges(EdgeSide::Plays, AudioEdgePolicy::Hard),
            edges(EdgeSide::Gaps, AudioEdgePolicy::Hard),
        ],
        &BTreeMap::new(),
    )
    .unwrap();
    let repeat = wrapped.context.selected_child.clone().unwrap();
    let NodeKind::Repeat { child, .. } = &wrapped.document.nodes()[&repeat].kind else {
        panic!("a Repeat")
    };
    let seams = wrapped.document.nodes()[child].audio_edges;
    assert_eq!(seams.get(Kind::NodeStart), AudioEdgePolicy::Hard);
    assert_eq!(seams.get(Kind::NodeEnd), AudioEdgePolicy::Hard);
    let own = wrapped.document.nodes()[&repeat].audio_edges;
    assert_eq!(own.get(Kind::RepeatGapStart), AudioEdgePolicy::Hard);
    assert_eq!(own.get(Kind::NodeStart), AudioEdgePolicy::Automatic);
    replays_and_reverts(&document, &wrapped);
}

#[test]
fn a_split_fragments_play_seams_are_marked_before_their_policy_applies() {
    use crate::{AudioBoundaryKind as Kind, AudioEdgePolicy, EdgeSide};
    let fragment = BeatNode {
        label: "Fragment".into(),
        kind: NodeKind::Retime {
            child: node("held"),
            duration: FrameDuration::new(4).unwrap(),
            mapping: range(2, 6),
            pitch: PitchPolicy::Preserve,
            purpose: crate::RetimePurpose::Partition,
        },
        ..hold(1)
    };
    let document = tree(
        &["a", "p"],
        vec![("a", hold(3)), ("held", hold(8)), ("p", fragment)],
    );
    let edges = |side, policy| SemanticInstruction::SetAudioEdges { side, policy };
    let wrapped = plan(
        &document,
        selected("p", 3),
        vec![
            SemanticInstruction::Repeat {
                selector: SemanticSelector::SelectedBeat,
                plays: NonZeroU32::new(2).unwrap(),
                escalation: None,
            },
            edges(EdgeSide::Plays, AudioEdgePolicy::Hard),
        ],
        &BTreeMap::new(),
    )
    .unwrap();
    let fragment = &wrapped.document.nodes()[&node("p")];
    assert_eq!(
        fragment.audio_editorial_edges,
        crate::AudioEditorialEdges {
            start: true,
            end: true
        }
    );
    assert_eq!(
        fragment.audio_edges.get(Kind::NodeStart),
        AudioEdgePolicy::Hard
    );
    replays_and_reverts(&document, &wrapped);
    // `auto` on the unmarked fragment marks its seams for the automatic fade.
    let faded = plan(
        &document,
        selected("p", 3),
        vec![edges(EdgeSide::Both, AudioEdgePolicy::Automatic)],
        &BTreeMap::new(),
    )
    .unwrap();
    assert!(
        faded.document.nodes()[&node("p")]
            .audio_editorial_edges
            .start
    );
    replays_and_reverts(&document, &faded);
}

#[test]
fn mute_and_audio_lag_change_one_beat_without_time() {
    let mut document = tree(&["a", "b"], vec![("a", hold(3)), ("b", hold(5))]);
    let mute = |muted| SemanticInstruction::SetAudio {
        change: AudioChange::Mute { muted },
    };
    let muted = plan(
        &document,
        selected("b", 3),
        vec![mute(true)],
        &BTreeMap::new(),
    )
    .unwrap();
    assert!(
        muted.document.nodes()[&node("b")]
            .audio_treatments
            .clip_gain()
            .is_some_and(|clip| clip.muted())
    );
    replays_and_reverts(&document, &muted);
    assert!(plan(&document, ranged(4, 6), vec![mute(true)], &BTreeMap::new()).is_err());
    // A Source with sound takes an exact sample offset; a pause refuses.
    let asset = crate::AssetId::new("original").unwrap();
    let time_base = crate::SourceTimeBase::new(1, 48_000).unwrap();
    let span = crate::SourceSpan::new(
        crate::SourceTimestamp {
            ticks: 0,
            time_base,
        },
        crate::SourceTimestamp {
            ticks: 96_000,
            time_base,
        },
    )
    .unwrap();
    document.assets.insert(
        asset.clone(),
        crate::AssetRecord {
            label: "Original".into(),
            content_hash: "a".repeat(64),
            audio: Some(span),
            video: None,
            frame_count: None,
            still_image: false,
            source_qualification: Some(crate::SourceQualificationId::new("b".repeat(64)).unwrap()),
        },
    );
    document.nodes.insert(
        node("s"),
        BeatNode {
            label: "Sound".into(),
            kind: NodeKind::Source {
                source: SourceNode {
                    duration: FrameDuration::new(5).unwrap(),
                    edit_window: None,
                    video: crate::SourceVideo::Blank,
                    video_mapping: crate::SourceVideoMapping::FitBeat,
                    audio: Some(crate::SourceAudio {
                        asset: asset.clone(),
                        span,
                    }),
                    audio_mapping: crate::SourceAudioMapping::FitBeat,
                    link: crate::LinkRelation::Independent,
                    audio_offset: crate::AudioSample(0),
                },
            },
            ..hold(1)
        },
    );
    if let Some(root) = document.nodes.get_mut(&node("root")) {
        root.kind = NodeKind::Sequence {
            children: vec![node("a"), node("s")],
        };
    }
    document.nodes.remove(&node("b"));
    document.validate().unwrap();
    let lag = SemanticInstruction::SetAudioLag {
        offset: crate::AudioSample(3_840),
    };
    let lagged = plan(
        &document,
        selected("s", 3),
        vec![lag.clone()],
        &BTreeMap::new(),
    )
    .unwrap();
    assert!(matches!(
        &lagged.document.nodes()[&node("s")].kind,
        NodeKind::Source { source } if source.audio_offset == crate::AudioSample(3_840)
    ));
    assert_eq!(lagged.document.duration(), document.duration());
    replays_and_reverts(&document, &lagged);
    assert!(
        plan(
            &document,
            selected("s", 3),
            vec![lag.clone(), lag.clone()],
            &BTreeMap::new()
        )
        .is_err()
    );
    assert!(plan(&document, selected("a", 0), vec![lag], &BTreeMap::new()).is_err());
}

fn fragment_of(child: &str, start: i64, end: i64) -> BeatNode {
    BeatNode {
        label: "Fragment".into(),
        kind: NodeKind::Retime {
            child: node(child),
            duration: FrameDuration::new(end - start).unwrap(),
            mapping: range(start, end),
            pitch: PitchPolicy::Preserve,
            purpose: crate::RetimePurpose::Partition,
        },
        ..hold(1)
    }
}

#[test]
fn a_split_seam_has_no_edge_while_a_cut_between_fragments_marks_both_sides() {
    use crate::{AudioBoundaryKind as Kind, AudioEdgePolicy, EdgeSide};
    let edges = |side, policy| SemanticInstruction::SetAudioEdges { side, policy };
    // [0, 4) and [4, 8) of one Hold: a pure Split, continuous sound.
    // Split fragments hold full copies of the same context.
    let split = tree(
        &["left", "right"],
        vec![
            ("held", hold(8)),
            ("copy", hold(8)),
            ("left", fragment_of("held", 0, 4)),
            ("right", fragment_of("copy", 4, 8)),
        ],
    );
    for (child, side) in [
        ("left", EdgeSide::End),
        ("right", EdgeSide::Start),
        ("left", EdgeSide::Both),
    ] {
        for policy in [AudioEdgePolicy::Automatic, AudioEdgePolicy::Hard] {
            let error = plan(
                &split,
                selected(child, 0),
                vec![edges(side, policy)],
                &BTreeMap::new(),
            )
            .unwrap_err();
            assert!(
                error.message.contains("continues the same sound"),
                "{error:?}"
            );
        }
    }
    // The outer ends are real edges: the left fragment's start is marked.
    let outer = plan(
        &split,
        selected("left", 0),
        vec![edges(EdgeSide::Start, AudioEdgePolicy::Automatic)],
        &BTreeMap::new(),
    )
    .unwrap();
    assert!(
        outer.document.nodes()[&node("left")]
            .audio_editorial_edges
            .start
    );
    replays_and_reverts(&split, &outer);
    // [0, 3) then [5, 8): two frames were cut, so the seam is an edit. Both
    // incident sides are marked, as Trim marks them, and Hard applies to the
    // selected side.
    let cut = tree(
        &["left", "right"],
        vec![
            ("held", hold(8)),
            ("copy", hold(8)),
            ("left", fragment_of("held", 0, 3)),
            ("right", fragment_of("copy", 5, 8)),
        ],
    );
    let hard = plan(
        &cut,
        selected("left", 0),
        vec![edges(EdgeSide::End, AudioEdgePolicy::Hard)],
        &BTreeMap::new(),
    )
    .unwrap();
    let (left, right) = (
        &hard.document.nodes()[&node("left")],
        &hard.document.nodes()[&node("right")],
    );
    assert!(left.audio_editorial_edges.end && !left.audio_editorial_edges.start);
    assert!(right.audio_editorial_edges.start && !right.audio_editorial_edges.end);
    assert_eq!(left.audio_edges.get(Kind::NodeEnd), AudioEdgePolicy::Hard);
    assert_eq!(
        right.audio_edges.get(Kind::NodeStart),
        AudioEdgePolicy::Automatic
    );
    replays_and_reverts(&cut, &hard);
    // Undo and redo of the compound restore and reapply every mark.
    let request = hard.request.as_ref().unwrap();
    let applied = crate::apply(&cut, request).unwrap();
    let redone = applied.forward.apply(&cut).unwrap();
    let undone = applied.inverse.apply(&redone).unwrap();
    assert_eq!(undone, cut);
    assert_eq!(applied.forward.apply(&undone).unwrap(), redone);
}
