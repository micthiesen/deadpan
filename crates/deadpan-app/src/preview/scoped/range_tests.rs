use super::*;

#[test]
fn scoped_visual_range_keeps_framing_navigation_and_explicit_history_routing() {
    for action in [
        Action::VisualMoment,
        Action::Step {
            forward: true,
            count: 4,
        },
        Action::First,
        Action::Last,
        Action::Escape,
        Action::Undo,
        Action::Redo,
        Action::Framing(navigation::FramingAction::PunchIn),
        Action::Framing(navigation::FramingAction::Creep),
        Action::RepeatLast,
        Action::MacroRecord('a'),
        Action::MacroExecute {
            register: 'a',
            count: 1,
        },
    ] {
        assert!(scoped_range_action_allowed(&action, false), "{action:?}");
    }
    assert!(scoped_range_action_allowed(&Action::EnterGroup, false));
    assert!(!scoped_range_action_allowed(&Action::EnterGroup, true));
}

#[test]
fn scoped_visual_range_does_not_reuse_whole_owner_parameter_routes() {
    for action in [
        Action::GainStep(3000),
        Action::Mute,
        Action::Edit(BeatEdit::InsertHold(
            navigation::duration::DurationInput::half_seconds(1),
        )),
        Action::Group,
        Action::DeleteSelection,
        Action::CopyMoment,
        Action::Duplicate,
    ] {
        assert!(!scoped_range_action_allowed(&action, false), "{action:?}");
    }
    for text in [
        "hold 2f",
        "gain +3dB",
        "saturate 3dB",
        "eq",
        "caption hello",
        "reverse 2f",
    ] {
        let command = navigation::command::parse(text).unwrap();
        assert!(!scoped_range_command_allowed(&command, false), "{text}");
    }
    for text in [
        "zoom 2",
        "creep from=1 to=2",
        "scope play 2",
        "undo",
        "models",
    ] {
        let command = navigation::command::parse(text).unwrap();
        assert!(scoped_range_command_allowed(&command, false), "{text}");
    }
}

#[test]
fn scoped_visual_range_preserves_explicit_sound_command_routing() {
    for text in [
        "sound-gain +3",
        "sound-edges hard",
        "sound-at 48000",
        "sound-delete",
        "sound-allow",
        "sound-silence",
        "sound-cut",
        "sound-mute on",
        "sound-relink",
        "sound-channels stereo",
    ] {
        let command = navigation::command::parse(text).unwrap();
        assert!(scoped_range_command_allowed(&command, false), "{text}");
    }
    // Picture gain remains a separate command even with Sounds focused.
    assert!(!scoped_range_command_allowed(&Entry::GainMute, false));
}

#[test]
fn scoped_saved_follow_summary_reports_recipe_scale_window_and_clock() {
    use deadpan_core::{
        ExactRatio, FollowScaleRange, FollowScaleTransition, Framing, FramingClock, FramingPose,
        FramingValue, TargetId,
    };
    let ratio = |n, d| ExactRatio::new(n, d).unwrap();
    let mut framing = Framing {
        clock: FramingClock::OwnerOutput,
        value: FramingValue::Follow {
            target: TargetId::new("saved-target").unwrap(),
            scale: ratio(11, 10),
            fallback: FramingPose::new(ratio(1, 1), ratio(1, 2), ratio(1, 2)).unwrap(),
            scale_transition: Some(FollowScaleTransition {
                factor: ratio(2, 1),
                curve: navigation::zoom::ZoomCurve::Smoothstep,
                range: Some(FollowScaleRange {
                    start: ratio(1, 6),
                    end: ratio(2, 3),
                    from_factor: ratio(15, 11),
                }),
            }),
        },
    };
    let label = |_: &TargetId| "Moving subject".to_owned();
    assert_eq!(
        scoped_framing_fields(Some(&framing), &label),
        vec![
            ("Follows", "Moving subject".into()),
            ("Scale before", "1.100×".into()),
            ("Creep", "1.500× → 2.200× (smoothstep)".into()),
            ("Window", "16.667%–66.667% of framing duration".into()),
            ("Framing clock", "Owner output".into()),
        ]
    );
    framing.clock = FramingClock::RetainedOutput {
        offset: ratio(-3, 2),
        duration: ratio(121, 2),
    };
    let retained = scoped_framing_fields(Some(&framing), &label);
    assert_eq!(
        &retained[4..],
        [
            ("Framing clock", "Retained output".into()),
            ("Framing duration", "121/2 f".into()),
            ("Clock offset", "-3/2 f".into()),
        ]
    );
    assert_eq!(
        scoped_framing_fields(None, &label),
        [("Framing", "full picture, no framing".into())]
    );
}
