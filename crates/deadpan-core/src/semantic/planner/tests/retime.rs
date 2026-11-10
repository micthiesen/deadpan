//! Speed, pitch and pause-duration instructions, and the ranged forms of
//! gain steps, cutaways and captions over a Visual range inside one beat.

use super::*;
use crate::{AudioChange, ExactRatio, GainDb, PauseLength, PitchPolicy, SemanticVisualSelection};

mod exact_clock;

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
fn a_waveform_selection_mutes_or_steps_exact_samples_of_the_selected_beat() {
    let document = tree(&["a", "b"], vec![("a", hold(3)), ("b", hold(6))]);
    let rate = document.presentation_basis().frame_rate;
    // Beat b starts at Edit frame 3; select 7 mix samples that straddle no
    // frame boundary, starting 101 samples after frame 4 begins.
    let first = rate.audio_boundary(ProjectFrame(4)).unwrap().0 + 101;
    let local =
        crate::GainRange::from_mix_samples(rate, ProjectFrame(3), first, first + 7).unwrap();
    let per_sample = ExactRatio::new(
        i128::from(rate.numerator()),
        i128::from(rate.denominator()) * 48_000,
    )
    .unwrap();
    assert_eq!(
        local.end().checked_sub(local.start()).unwrap(),
        ExactRatio::integer(7).checked_mul(per_sample).unwrap(),
        "seven samples long, not a whole frame"
    );
    assert!(local.start().compare_integer(1).is_gt() && local.end().compare_integer(2).is_lt());
    let mute = SemanticInstruction::SetAudio {
        change: AudioChange::SampleRangeMute {
            start: local.start(),
            end: local.end(),
        },
    };
    let planned = plan(
        &document,
        selected("b", 3),
        vec![mute.clone()],
        &BTreeMap::new(),
    )
    .unwrap();
    let clip = planned.document.nodes()[&node("b")]
        .audio_treatments
        .clip_gain()
        .unwrap()
        .clone();
    assert_eq!(clip.mute_ranges(), &[local]);
    let sample_at = |n: i64| {
        ExactRatio::integer(n)
            .checked_mul(per_sample)
            .unwrap()
            .checked_sub(ExactRatio::integer(3))
            .unwrap()
    };
    // Exactly samples [first, first + 7) are muted, by their own positions.
    for n in first - 2..first + 9 {
        assert_eq!(
            clip.evaluate(sample_at(n)).unwrap().muted,
            (first..first + 7).contains(&n),
            "sample {n}"
        );
    }
    assert_eq!(
        planned.trace[0].resolved_selection,
        Some(SliceCaptureSelection::Child { node: node("b") })
    );
    replays_and_reverts(&document, &planned);
    // Muting the same range again is no edit.
    assert!(
        plan(
            &document,
            selected("b", 3),
            vec![mute.clone(), mute.clone()],
            &BTreeMap::new()
        )
        .is_err()
    );
    // The selection must be inside the beat, needs a selected beat and
    // refuses a frame Visual range.
    let outside =
        crate::GainRange::from_mix_samples(rate, ProjectFrame(3), first, first + 48_000).unwrap();
    let too_long = SemanticInstruction::SetAudio {
        change: AudioChange::SampleRangeMute {
            start: outside.start(),
            end: outside.end(),
        },
    };
    assert!(
        plan(
            &document,
            selected("b", 3),
            vec![too_long],
            &BTreeMap::new()
        )
        .is_err()
    );
    assert!(
        plan(
            &document,
            context("root", 3),
            vec![mute.clone()],
            &BTreeMap::new()
        )
        .is_err()
    );
    assert!(plan(&document, ranged(4, 6), vec![mute], &BTreeMap::new()).is_err());
    // A step adds one constant envelope over the same exact samples; a
    // second press adjusts it and the opposite step removes it.
    let step = |millidecibels| SemanticInstruction::SetAudio {
        change: AudioChange::SampleRangeStep {
            start: local.start(),
            end: local.end(),
            millidecibels,
        },
    };
    let planned = plan(
        &document,
        selected("b", 3),
        vec![step(-6_000), step(-6_000)],
        &BTreeMap::new(),
    )
    .unwrap();
    let clip = planned.document.nodes()[&node("b")]
        .audio_treatments
        .clip_gain()
        .unwrap()
        .clone();
    assert_eq!(clip.range_step(local), Some(GainDb::new(-12_000).unwrap()));
    for n in first - 2..first + 9 {
        let expected = if (first..first + 7).contains(&n) {
            -12_000
        } else {
            0
        };
        assert_eq!(
            clip.evaluate(sample_at(n)).unwrap().millidecibels,
            ExactRatio::integer(expected),
            "sample {n}"
        );
    }
    replays_and_reverts(&document, &planned);
    let planned = plan(
        &document,
        selected("b", 3),
        vec![step(3_000), step(-3_000), mute_and_wire_round_trip(local)],
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(
        planned.document.nodes()[&node("b")]
            .audio_treatments
            .clip_gain()
            .unwrap()
            .envelopes(),
        &[],
        "a step back to 0 dB removes its envelope"
    );
}

/// A sample-range instruction survives its JSON wire exactly.
fn mute_and_wire_round_trip(local: crate::GainRange) -> SemanticInstruction {
    let instruction = SemanticInstruction::SetAudio {
        change: AudioChange::SampleRangeMute {
            start: local.start(),
            end: local.end(),
        },
    };
    let wire = serde_json::to_string(&instruction).unwrap();
    assert!(wire.contains("sample_range_mute"), "{wire}");
    let back: SemanticInstruction = serde_json::from_str(&wire).unwrap();
    assert_eq!(back, instruction);
    back
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

#[test]
fn a_ramp_splits_a_beat_into_equal_parts_each_at_its_interpolated_speed() {
    let document = tree(&["a", "b"], vec![("a", hold(3)), ("b", hold(12))]);
    // Three 4-frame parts at 4/3, 2 and 8/3 times: 3, 2 and 1.5 → 2 frames.
    let planned = plan(
        &document,
        selected("b", 3),
        vec![SemanticInstruction::Ramp {
            from: ExactRatio::ONE,
            to: ExactRatio::integer(3),
            steps: Some(3),
            pitch: PitchPolicy::Preserve,
        }],
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(planned.document.duration().unwrap().frames(), 3 + 3 + 2 + 2);
    let NodeKind::Sequence { children } = &planned.document.nodes()[&node("root")].kind else {
        panic!("root stays a Sequence")
    };
    assert_eq!(children.len(), 4);
    let durations = planned.document.durations().unwrap();
    let parts: Vec<_> = children[1..]
        .iter()
        .map(|id| match &planned.document.nodes()[id].kind {
            NodeKind::Retime {
                duration, mapping, ..
            } => (duration.frames(), mapping.end().0 - mapping.start().0),
            other => panic!("each part is a speed change: {other:?}"),
        })
        .collect();
    assert_eq!(parts, [(3, 4), (2, 4), (2, 4)]);
    assert_eq!(durations[&children[1]].frames(), 3);
    replays_and_reverts(&document, &planned);

    for (from, to, steps, message) in [(1, 3, 1, "2 to 32 steps"), (0, 3, 2, "positive")] {
        let error = crate::SemanticProgram::new(vec![SemanticInstruction::Ramp {
            from: ExactRatio::integer(from),
            to: ExactRatio::integer(to),
            steps: Some(steps),
            pitch: PitchPolicy::Preserve,
        }])
        .unwrap_err();
        assert!(error.message.contains(message), "{error:?}");
    }
    let error = plan(
        &document,
        selected("a", 0),
        vec![SemanticInstruction::Ramp {
            from: ExactRatio::ONE,
            to: ExactRatio::integer(2),
            steps: Some(4),
            pitch: PitchPolicy::Preserve,
        }],
        &BTreeMap::new(),
    )
    .unwrap_err();
    assert!(
        error.message.contains("needs at least 4 frames"),
        "{error:?}"
    );
}

/// An Original beat of `frames` frames, with its picture-only asset.
fn original_tree(frames: i64) -> ProjectDocument {
    let mut document = tree(&["a", "b"], vec![("a", hold(3)), ("b", hold(frames))]);
    let base = crate::SourceTimeBase::new(1, 30).unwrap();
    let span = crate::SourceSpan::new(
        crate::SourceTimestamp {
            ticks: 0,
            time_base: base,
        },
        crate::SourceTimestamp {
            ticks: frames,
            time_base: base,
        },
    )
    .unwrap();
    let asset = crate::AssetId::new("original").unwrap();
    document.assets.insert(
        asset.clone(),
        crate::AssetRecord {
            source_qualification: None,
            label: "Original".into(),
            content_hash: "a".repeat(64),
            video: Some(span),
            audio: None,
            still_image: false,
            frame_count: Some(FrameDuration::new(frames).unwrap()),
        },
    );
    document.nodes.insert(
        node("b"),
        BeatNode {
            label: "Original".into(),
            kind: NodeKind::Source {
                source: crate::SourceNode {
                    duration: FrameDuration::new(frames).unwrap(),
                    edit_window: None,
                    video: crate::SourceVideo::Stream { asset, span },
                    video_mapping: crate::SourceVideoMapping::FitBeat,
                    audio: None,
                    audio_mapping: crate::SourceAudioMapping::FitBeat,
                    link: crate::LinkRelation::Independent,
                    audio_offset: crate::AudioSample(0),
                },
            },
            ..hold(1)
        },
    );
    document.validate().unwrap();
    document
}

fn continuous(from: (i128, i128), to: (i128, i128), pitch: PitchPolicy) -> SemanticInstruction {
    SemanticInstruction::Ramp {
        from: ExactRatio::new(from.0, from.1).unwrap(),
        to: ExactRatio::new(to.0, to.1).unwrap(),
        steps: None,
        pitch,
    }
}

#[test]
fn a_continuous_ramp_is_one_retime_on_the_exact_quadratic_clock() {
    let document = original_tree(12);
    // Mean speed (1 + 3)/2 = 2 over 12 frames: 6 frames, one Retime.
    let planned = plan(
        &document,
        selected("b", 3),
        vec![continuous((1, 1), (3, 1), PitchPolicy::FollowSpeed)],
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(planned.document.duration().unwrap().frames(), 3 + 6);
    let NodeKind::Sequence { children } = &planned.document.nodes()[&node("root")].kind else {
        panic!("root stays a Sequence")
    };
    let NodeKind::Retime {
        child,
        duration,
        mapping,
        pitch: PitchPolicy::TapeRamp { ramp },
        ..
    } = &planned.document.nodes()[&children[1]].kind
    else {
        panic!("the beat is wrapped in one ramp")
    };
    assert_eq!(child, &node("b"));
    assert_eq!(
        (duration.frames(), mapping.start().0, mapping.end().0),
        (6, 0, 12)
    );
    assert_eq!(ramp.weights().unwrap(), (1, 3));
    let (first, last) = ramp
        .endpoint_rates(ExactRatio::integer(6), ExactRatio::integer(12))
        .unwrap();
    assert_eq!((first, last), (ExactRatio::ONE, ExactRatio::integer(3)));
    replays_and_reverts(&document, &planned);
    // The serialized program carries no steps and round-trips.
    let wire =
        serde_json::to_string(&continuous((1, 1), (3, 1), PitchPolicy::FollowSpeed)).unwrap();
    assert!(!wire.contains("steps"), "{wire}");
    assert_eq!(
        serde_json::from_str::<SemanticInstruction>(&wire).unwrap(),
        continuous((1, 1), (3, 1), PitchPolicy::FollowSpeed)
    );

    // pitch=preserve plans the same clock with the stretcher's policy, and
    // within its narrower 1/8..8 range; a fixed shift has no ramp form.
    let preserved = plan(
        &document,
        selected("b", 3),
        vec![continuous((1, 1), (3, 1), PitchPolicy::Preserve)],
        &BTreeMap::new(),
    )
    .unwrap();
    let NodeKind::Retime {
        duration: preserved_duration,
        pitch: PitchPolicy::PreserveRamp {
            ramp: preserved_ramp,
        },
        ..
    } = &preserved.document.nodes()[&children[1]].kind
    else {
        panic!("pitch=preserve wraps the beat in one pitch-preserving ramp")
    };
    assert_eq!((preserved_duration.frames(), preserved_ramp), (6, ramp));
    replays_and_reverts(&document, &preserved);
    let error = plan(
        &document,
        selected("b", 3),
        vec![continuous((1, 12), (1, 1), PitchPolicy::Preserve)],
        &BTreeMap::new(),
    )
    .unwrap_err();
    assert!(error.message.contains("1/8x to 8x"), "{error:?}");
    assert!(
        plan(
            &document,
            selected("b", 3),
            vec![continuous((1, 12), (1, 1), PitchPolicy::FollowSpeed)],
            &BTreeMap::new(),
        )
        .is_ok()
    );
    let error = plan(
        &document,
        selected("b", 3),
        vec![continuous(
            (1, 1),
            (3, 1),
            PitchPolicy::Shift { semitones: 3 },
        )],
        &BTreeMap::new(),
    )
    .unwrap_err();
    assert!(error.message.contains("pitch=preserve"), "{error:?}");
    // Unsupported contents refuse.
    let error = plan(
        &document,
        selected("a", 0),
        vec![continuous((1, 1), (3, 1), PitchPolicy::FollowSpeed)],
        &BTreeMap::new(),
    )
    .unwrap_err();
    assert!(
        error.message.contains("exactly one Original beat"),
        "{error:?}"
    );
    let error =
        crate::SemanticProgram::new(vec![continuous((1, 1), (1, 1), PitchPolicy::FollowSpeed)])
            .unwrap_err();
    assert!(error.message.contains("two different speeds"), "{error:?}");
    // Speeds outside 1/16..16 refuse before any edit.
    let error = plan(
        &document,
        selected("b", 3),
        vec![continuous((1, 32), (1, 1), PitchPolicy::FollowSpeed)],
        &BTreeMap::new(),
    )
    .unwrap_err();
    assert!(error.message.contains("1/16x to 16x"), "{error:?}");

    // Inserting a pause inside the ramp refuses: the document would no
    // longer hold exactly one Original beat inside it.
    let ramped = planned.document.clone();
    let NodeKind::Hold { recipe } = hold(2).kind else {
        unreachable!()
    };
    let inside = crate::apply(
        &ramped,
        &crate::CommandRequest {
            project_id: ramped.project_id().clone(),
            expected_revision: ramped.revision_id().clone(),
            new_revision: revision("ramp-pause"),
            command: crate::Command::InsertTime {
                at: ProjectFrame(5),
                hold: recipe,
                id: node("ramp-pause"),
                identities: crate::SplitIdentities {
                    nodes: (0..8)
                        .map(|index| node(&format!("ramp-split-{index}")))
                        .collect(),
                },
                timing: crate::AudioTimingId {
                    allocation: revision("ramp-pause"),
                    ordinal: 0,
                },
            },
        },
    );
    assert!(inside.is_err());

    // Splitting the ramp never re-cuts its clock: each side is a unity
    // Partition over a complete copy of the ramp, selecting [0, 2) and [2, 6)
    // of its output, so both halves read the same exact quadratic. Undo
    // restores the single ramp.
    let ramp_id = children[1].clone();
    let request = crate::CommandRequest {
        project_id: ramped.project_id().clone(),
        expected_revision: ramped.revision_id().clone(),
        new_revision: revision("ramp-split"),
        command: crate::Command::Split {
            node: ramp_id.clone(),
            at: FrameDuration::new(2).unwrap(),
            identities: crate::SplitIdentities {
                nodes: (0..8)
                    .map(|index| node(&format!("ramp-split-{index}")))
                    .collect(),
            },
        },
    };
    let applied = crate::apply(&ramped, &request).unwrap();
    let split = applied.forward.apply(&ramped).unwrap();
    split.validate().unwrap();
    assert_eq!(split.duration().unwrap(), ramped.duration().unwrap());
    let NodeKind::Sequence { children: parts } = &split.nodes()[&node("root")].kind else {
        panic!("root stays a Sequence")
    };
    let mut selections = Vec::new();
    for part in &parts[1..] {
        let NodeKind::Retime {
            child,
            mapping,
            purpose: crate::RetimePurpose::Partition,
            ..
        } = &split.nodes()[part].kind
        else {
            panic!("each side is a Partition")
        };
        let NodeKind::Retime {
            duration,
            mapping: input,
            pitch: PitchPolicy::TapeRamp { ramp: copied },
            ..
        } = &split.nodes()[child].kind
        else {
            panic!("each Partition holds the complete ramp")
        };
        assert_eq!(
            (duration.frames(), input.start().0, input.end().0),
            (6, 0, 12)
        );
        assert_eq!(copied, ramp);
        selections.push((mapping.start().0, mapping.end().0));
    }
    assert_eq!(selections, [(0, 2), (2, 6)]);
    assert_eq!(applied.inverse.apply(&split).unwrap(), ramped);
}
