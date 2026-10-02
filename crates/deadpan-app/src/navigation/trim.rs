//! Trim input describes one complete draft; inspection never changes its values.

use deadpan_core::{SourceTrimControl, SourceTrimIntent, SourceTrimPolicy};
use eframe::egui::{Key, Modifiers};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrimInput {
    pub control: SourceTrimControl,
    pub intent: SourceTrimIntent,
}

impl Default for TrimInput {
    fn default() -> Self {
        Self {
            control: SourceTrimControl::In,
            intent: SourceTrimIntent::default(),
        }
    }
}

fn usage() -> String {
    "Use :trim or :trim edge=out delta=-3f mode=ripple. Supply edge, delta and mode exactly once; edges are in/out/slip/roll and modes are ripple/overwrite.".into()
}

/// The amount field and command share the existing exact whole-frame grammar.
pub fn parse_frames(input: &str) -> Result<i64, String> {
    super::slip::parse_frames(input).map_err(|_| usage())
}

pub fn parse<'a>(words: impl Iterator<Item = &'a str>) -> Result<TrimInput, String> {
    let mut control = None;
    let mut delta = None;
    let mut policy = None;
    for word in words {
        let (name, value) = word.split_once('=').ok_or_else(usage)?;
        match name {
            "edge" if control.is_none() => {
                control = Some(match value {
                    "in" => SourceTrimControl::In,
                    "out" => SourceTrimControl::Out,
                    "slip" => SourceTrimControl::Slip,
                    "roll" => SourceTrimControl::Roll,
                    _ => return Err(usage()),
                });
            }
            "delta" if delta.is_none() => delta = Some(parse_frames(value)?),
            "mode" if policy.is_none() => {
                policy = Some(match value {
                    "ripple" => SourceTrimPolicy::Ripple,
                    "overwrite" => SourceTrimPolicy::Overwrite,
                    _ => return Err(usage()),
                });
            }
            _ => return Err(usage()),
        }
    }
    if control.is_none() && delta.is_none() && policy.is_none() {
        return Ok(TrimInput::default());
    }
    let control = control.ok_or_else(usage)?;
    let delta = delta.ok_or_else(usage)?;
    let mut intent = SourceTrimIntent {
        policy: policy.ok_or_else(usage)?,
        ..Default::default()
    };
    match control {
        SourceTrimControl::In => intent.in_frames = delta,
        SourceTrimControl::Out => intent.out_frames = delta,
        SourceTrimControl::Slip => intent.slip_frames = delta,
        SourceTrimControl::Roll => intent.roll_frames = delta,
    }
    Ok(TrimInput { control, intent })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrimKey {
    Cycle { reverse: bool },
    Nudge(i64),
    TogglePolicy,
    Compare,
    In,
    Out,
    Play,
    Loop,
    FocusAmount,
    Apply,
    Cancel,
}

/// Cycle inspection only. The UI replays this and subsequent nudges in event
/// order, associating each nudge with the control active at that event.
pub fn cycle_control(control: SourceTrimControl, reverse: bool) -> SourceTrimControl {
    use SourceTrimControl::{In, Out, Roll, Slip};
    match (control, reverse) {
        (In, false) | (Slip, true) => Out,
        (Out, false) | (Roll, true) => Slip,
        (Slip, false) | (In, true) => Roll,
        (Roll, false) | (Out, true) => In,
    }
}

pub fn route_key(
    key: Key,
    modifiers: Modifiers,
    field: bool,
    background: bool,
    ime: bool,
    repeat: bool,
) -> Option<TrimKey> {
    // Check both Command flags explicitly. egui's convenience Shift predicate
    // assumes mac_cmd and command are normalized together.
    if ime || modifiers.alt || modifiers.ctrl || modifiers.mac_cmd || modifiers.command {
        return None;
    }
    // Escape cancels from native controls too, unless composition owns it.
    if key == Key::Escape && modifiers.is_none() && !repeat {
        return Some(TrimKey::Cancel);
    }
    // Native fields and buttons retain Tab traversal and activation. The
    // amount field's Enter accepts text in the UI; it cannot also Apply.
    if field || !background {
        return None;
    }
    let step = if modifiers.shift_only() { 10 } else { 1 };
    match key {
        Key::H => return Some(TrimKey::Nudge(-step)),
        Key::L => return Some(TrimKey::Nudge(step)),
        _ => {}
    }
    if repeat {
        return None;
    }
    if key == Key::Tab {
        return Some(TrimKey::Cycle {
            reverse: modifiers.shift_only(),
        });
    }
    if key == Key::Space && modifiers.shift_only() {
        return Some(TrimKey::Loop);
    }
    if !modifiers.is_none() {
        return None;
    }
    Some(match key {
        Key::R => TrimKey::TogglePolicy,
        Key::B => TrimKey::Compare,
        Key::I => TrimKey::In,
        Key::O => TrimKey::Out,
        Key::Space => TrimKey::Play,
        Key::E => TrimKey::FocusAmount,
        Key::Enter => TrimKey::Apply,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::navigation::{Action, Bindings, command, command::Entry};

    const KEYS: [Key; 11] = [
        Key::Tab,
        Key::H,
        Key::L,
        Key::R,
        Key::B,
        Key::I,
        Key::O,
        Key::Space,
        Key::E,
        Key::Enter,
        Key::Escape,
    ];

    #[test]
    fn bare_command_opens_zero_in_ripple_and_slip_command_stays_separate() {
        for input in ["trim", ":trim", "  :TrIm  "] {
            assert_eq!(command::parse(input), Ok(Entry::Trim(TrimInput::default())));
        }
        assert_eq!(TrimInput::default().control, SourceTrimControl::In);
        assert_eq!(
            TrimInput::default().intent,
            SourceTrimIntent {
                in_frames: 0,
                out_frames: 0,
                slip_frames: 0,
                roll_frames: 0,
                policy: SourceTrimPolicy::Ripple,
            }
        );
        assert_eq!(command::parse(":slip -3f"), Ok(Entry::Slip(-3)));
    }

    #[test]
    fn complete_command_sets_only_its_selected_amount_and_keeps_exact_extremes() {
        for (edge, control) in [
            ("in", SourceTrimControl::In),
            ("out", SourceTrimControl::Out),
            ("slip", SourceTrimControl::Slip),
            ("roll", SourceTrimControl::Roll),
        ] {
            for (mode, policy) in [
                ("ripple", SourceTrimPolicy::Ripple),
                ("overwrite", SourceTrimPolicy::Overwrite),
            ] {
                for (amount, value) in [
                    ("+5f", 5),
                    ("-3f", -3),
                    ("0f", 0),
                    ("7f", 7),
                    ("-9223372036854775808f", i64::MIN),
                    ("9223372036854775807f", i64::MAX),
                ] {
                    let mut intent = SourceTrimIntent {
                        policy,
                        ..Default::default()
                    };
                    match control {
                        SourceTrimControl::In => intent.in_frames = value,
                        SourceTrimControl::Out => intent.out_frames = value,
                        SourceTrimControl::Slip => intent.slip_frames = value,
                        SourceTrimControl::Roll => intent.roll_frames = value,
                    }
                    let expected = Ok(Entry::Trim(TrimInput { control, intent }));
                    assert_eq!(
                        command::parse(&format!(":trim edge={edge} delta={amount} mode={mode}")),
                        expected
                    );
                    assert_eq!(
                        command::parse(&format!(":trim mode={mode} delta={amount} edge={edge}")),
                        expected
                    );
                }
            }
        }
    }

    #[test]
    fn incomplete_duplicate_unknown_or_inexact_command_never_opens_a_draft() {
        for input in [
            "trim edge=out",
            "trim delta=-3f",
            "trim mode=ripple",
            "trim edge=out delta=-3f",
            "trim edge=out mode=ripple",
            "trim delta=-3f mode=ripple",
            "trim out -3f ripple",
            "trim edge=out delta=-3f mode=ripple edge=in",
            "trim edge=out delta=-3f mode=ripple delta=2f",
            "trim edge=out delta=-3f mode=ripple mode=overwrite",
            "trim edge=out delta=-3f mode=ripple extra=1",
            "trim edge=out delta=-3f mode=ripple extra",
            "trim edge=unknown delta=-3f mode=ripple",
            "trim edge=OUT delta=-3f mode=ripple",
            "trim edge=out delta=-3f mode=unknown",
            "trim edge=out delta=-3f mode=Ripple",
            "trim edge=out delta= mode=ripple",
            "trim edge=out delta=+f mode=ripple",
            "trim edge=out delta=--1f mode=ripple",
            "trim edge=out delta=1.5f mode=ripple",
            "trim edge=out delta=1s mode=ripple",
            "trim edge=out delta=1 mode=ripple",
            "trim edge=out delta=１f mode=ripple",
            "trim edge=out delta=1F mode=ripple",
            "trim edge=out delta=1f=2f mode=ripple",
            "trim edge=out delta=9223372036854775808f mode=ripple",
            "trim edge=out delta=-9223372036854775809f mode=ripple",
        ] {
            assert!(command::parse(input).is_err(), "{input}");
        }
    }

    #[test]
    fn background_keys_route_exact_actions_and_only_movement_repeats() {
        for (key, modifiers, expected) in [
            (Key::Tab, Modifiers::NONE, TrimKey::Cycle { reverse: false }),
            (Key::Tab, Modifiers::SHIFT, TrimKey::Cycle { reverse: true }),
            (Key::R, Modifiers::NONE, TrimKey::TogglePolicy),
            (Key::B, Modifiers::NONE, TrimKey::Compare),
            (Key::I, Modifiers::NONE, TrimKey::In),
            (Key::O, Modifiers::NONE, TrimKey::Out),
            (Key::Space, Modifiers::NONE, TrimKey::Play),
            (Key::Space, Modifiers::SHIFT, TrimKey::Loop),
            (Key::E, Modifiers::NONE, TrimKey::FocusAmount),
            (Key::Enter, Modifiers::NONE, TrimKey::Apply),
            (Key::Escape, Modifiers::NONE, TrimKey::Cancel),
        ] {
            assert_eq!(
                route_key(key, modifiers, false, true, false, false),
                Some(expected)
            );
            assert_eq!(route_key(key, modifiers, false, true, false, true), None);
        }
        for repeat in [false, true] {
            for (key, modifiers, expected) in [
                (Key::H, Modifiers::NONE, -1),
                (Key::L, Modifiers::NONE, 1),
                (Key::H, Modifiers::SHIFT, -10),
                (Key::L, Modifiers::SHIFT, 10),
            ] {
                assert_eq!(
                    route_key(key, modifiers, false, true, false, repeat),
                    Some(TrimKey::Nudge(expected))
                );
            }
        }
        for key in [
            Key::R,
            Key::B,
            Key::I,
            Key::O,
            Key::E,
            Key::Enter,
            Key::Escape,
        ] {
            assert_eq!(
                route_key(key, Modifiers::SHIFT, false, true, false, false),
                None
            );
        }
        for key in [Key::ArrowLeft, Key::ArrowRight, Key::Num1, Key::V] {
            assert_eq!(
                route_key(key, Modifiers::NONE, false, true, false, false),
                None
            );
        }
    }

    #[test]
    fn native_tab_activation_and_composition_keep_ownership() {
        for key in KEYS {
            for modifiers in [Modifiers::NONE, Modifiers::SHIFT] {
                for repeat in [false, true] {
                    for (field, background) in [(true, true), (true, false), (false, false)] {
                        let expected = (key == Key::Escape && modifiers.is_none() && !repeat)
                            .then_some(TrimKey::Cancel);
                        assert_eq!(
                            route_key(key, modifiers, field, background, false, repeat),
                            expected
                        );
                        assert_eq!(
                            route_key(key, modifiers, field, background, true, repeat),
                            None
                        );
                    }
                    assert_eq!(route_key(key, modifiers, false, true, true, repeat), None);
                }
            }
        }
    }

    #[test]
    fn every_command_control_or_option_combination_stays_unclaimed() {
        for mask in 0_u8..32 {
            let modifiers = Modifiers {
                alt: mask & 1 != 0,
                ctrl: mask & 2 != 0,
                shift: mask & 4 != 0,
                mac_cmd: mask & 8 != 0,
                command: mask & 16 != 0,
            };
            if !modifiers.alt && !modifiers.ctrl && !modifiers.mac_cmd && !modifiers.command {
                continue;
            }
            for key in KEYS {
                for repeat in [false, true] {
                    assert_eq!(route_key(key, modifiers, false, true, false, repeat), None);
                }
            }
        }
    }

    #[test]
    fn cycling_wraps_both_directions_without_changing_authored_values() {
        let mut input = TrimInput {
            control: SourceTrimControl::In,
            intent: SourceTrimIntent {
                in_frames: -3,
                out_frames: 2,
                slip_frames: 1,
                roll_frames: -2,
                policy: SourceTrimPolicy::Overwrite,
            },
        };
        let accepted = input.intent;
        for control in [
            SourceTrimControl::Out,
            SourceTrimControl::Slip,
            SourceTrimControl::Roll,
            SourceTrimControl::In,
        ] {
            input.control = cycle_control(input.control, false);
            assert_eq!(input.control, control);
            assert_eq!(input.intent, accepted);
        }
        for control in [
            SourceTrimControl::Roll,
            SourceTrimControl::Slip,
            SourceTrimControl::Out,
            SourceTrimControl::In,
        ] {
            input.control = cycle_control(input.control, true);
            assert_eq!(input.control, control);
            assert_eq!(input.intent, accepted);
        }
    }

    #[test]
    fn mixed_tab_nudge_policy_and_junction_events_remain_ordered() {
        // This checks router output, not the UI's separate event replay/gates.
        let routed: Vec<_> = [
            (Key::L, Modifiers::SHIFT),
            (Key::Tab, Modifiers::NONE),
            (Key::H, Modifiers::NONE),
            (Key::R, Modifiers::NONE),
            (Key::Tab, Modifiers::SHIFT),
            (Key::I, Modifiers::NONE),
            (Key::O, Modifiers::NONE),
            (Key::L, Modifiers::NONE),
        ]
        .into_iter()
        .filter_map(|(key, modifiers)| route_key(key, modifiers, false, true, false, false))
        .collect();
        assert_eq!(
            routed,
            vec![
                TrimKey::Nudge(10),
                TrimKey::Cycle { reverse: false },
                TrimKey::Nudge(-1),
                TrimKey::TogglePolicy,
                TrimKey::Cycle { reverse: true },
                TrimKey::In,
                TrimKey::Out,
                TrimKey::Nudge(1),
            ]
        );
    }

    #[test]
    fn comma_v_opens_once_with_a_visible_hint_and_leaves_other_comma_keys_intact() {
        for (key, expected) in [
            (Key::V, Action::Trim),
            (Key::I, Action::Insert),
            (Key::S, Action::Sound(super::super::SoundAction::Place)),
            (
                Key::F,
                Action::Framing(super::super::FramingAction::EnterCamera),
            ),
        ] {
            let mut bindings = Bindings::default();
            assert_eq!(
                bindings.key(Key::Comma, Modifiers::NONE, false, false),
                Some(Action::OfferInsert)
            );
            assert!(bindings.pending_hint().unwrap().contains("v Trim"));
            assert!(!bindings.allows_key_repeat(Key::Comma, Modifiers::NONE));
            assert!(!bindings.allows_key_repeat(key, Modifiers::NONE));
            assert_eq!(
                bindings.key(key, Modifiers::NONE, false, false),
                Some(expected)
            );
            assert!(bindings.pending().is_empty());
        }
        let mut bindings = Bindings::default();
        assert_eq!(
            bindings.key(Key::V, Modifiers::NONE, false, false),
            Some(Action::VisualMoment)
        );
    }

    #[test]
    fn counted_or_native_owned_comma_v_clears_without_trim_activation() {
        for count in [
            vec![Key::Num0],
            vec![Key::Num1],
            vec![Key::Num3],
            vec![Key::Num9; 11],
        ] {
            let mut bindings = Bindings::default();
            for key in count {
                bindings.key(key, Modifiers::NONE, false, false);
            }
            bindings.key(Key::Comma, Modifiers::NONE, false, false);
            assert!(matches!(
                bindings.key(Key::V, Modifiers::NONE, false, false),
                Some(Action::Invalid(_))
            ));
            assert!(bindings.pending().is_empty());
        }
        for (modifiers, field, ime) in [
            (Modifiers::NONE, true, false),
            (Modifiers::NONE, false, true),
            (Modifiers::NONE, true, true),
            (Modifiers::SHIFT, false, false),
            (Modifiers::ALT, false, false),
            (Modifiers::CTRL, false, false),
            (Modifiers::COMMAND, false, false),
            (Modifiers::MAC_CMD, false, false),
        ] {
            let mut bindings = Bindings::default();
            bindings.key(Key::Comma, Modifiers::NONE, false, false);
            assert_eq!(bindings.key(Key::V, modifiers, field, ime), None);
            assert!(bindings.pending().is_empty());
        }
        // Logical comma may need Shift/Option on a non-US layout; its following
        // V remains a plain uncounted letter, using the same existing prefix.
        for modifiers in [
            Modifiers::SHIFT,
            Modifiers::ALT,
            Modifiers::SHIFT | Modifiers::ALT,
        ] {
            let mut bindings = Bindings::default();
            bindings.key(Key::Comma, modifiers, false, false);
            assert_eq!(
                bindings.key(Key::V, Modifiers::NONE, false, false),
                Some(Action::Trim)
            );
        }
    }
}
