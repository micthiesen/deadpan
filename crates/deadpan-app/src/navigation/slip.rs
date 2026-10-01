//! Slip keys leave text, composition, buttons and global chords to their owners.

use eframe::egui::{Key, Modifiers};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlipKey {
    Apply,
    Cancel,
    Compare,
    First,
    Last,
    Nudge(i64),
    Inspect(i64),
}

pub fn parse_frames(input: &str) -> Result<i64, String> {
    let invalid =
        || "Use :slip +5f or :slip -3f with one signed whole project-frame amount.".to_owned();
    let number = input.strip_suffix('f').ok_or_else(invalid)?;
    let digits = number.strip_prefix(['+', '-']).unwrap_or(number);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(invalid());
    }
    number.parse::<i64>().map_err(|_| invalid())
}

pub fn route_key(
    key: Key,
    modifiers: Modifiers,
    field: bool,
    background: bool,
    ime: bool,
    repeat: bool,
) -> Option<SlipKey> {
    if ime || (!modifiers.is_none() && !modifiers.shift_only()) {
        return None;
    }
    if key == Key::Escape && modifiers.is_none() && !repeat {
        return Some(SlipKey::Cancel);
    }
    if field || !background {
        return None;
    }
    let step = if modifiers.shift_only() { 10 } else { 1 };
    match key {
        Key::H => Some(SlipKey::Nudge(-step)),
        Key::L => Some(SlipKey::Nudge(step)),
        Key::ArrowLeft => Some(SlipKey::Inspect(-step)),
        Key::ArrowRight => Some(SlipKey::Inspect(step)),
        _ if repeat || !modifiers.is_none() => None,
        Key::Enter => Some(SlipKey::Apply),
        Key::B => Some(SlipKey::Compare),
        Key::I => Some(SlipKey::First),
        Key::O => Some(SlipKey::Last),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_frames_are_exact_and_include_both_integer_extremes() {
        for (input, expected) in [
            ("+5f", 5),
            ("-3f", -3),
            ("0f", 0),
            ("5f", 5),
            ("-9223372036854775808f", i64::MIN),
            ("9223372036854775807f", i64::MAX),
        ] {
            assert_eq!(parse_frames(input), Ok(expected));
        }
        for input in [
            "",
            "+f",
            "-f",
            "--1f",
            "1.5f",
            "1s",
            "1",
            "１f",
            "1F",
            " 1f",
            "1f extra",
            "9223372036854775808f",
            "-9223372036854775809f",
        ] {
            assert!(parse_frames(input).is_err(), "{input}");
        }
    }

    #[test]
    fn repeat_nudges_and_native_focus_do_not_repeat_apply_or_take_global_keys() {
        assert_eq!(
            route_key(Key::L, Modifiers::SHIFT, false, true, false, true),
            Some(SlipKey::Nudge(10))
        );
        assert_eq!(
            route_key(Key::H, Modifiers::NONE, false, true, false, true),
            Some(SlipKey::Nudge(-1))
        );
        assert_eq!(
            route_key(Key::ArrowLeft, Modifiers::SHIFT, false, true, false, false),
            Some(SlipKey::Inspect(-10))
        );
        for key in [Key::Enter, Key::B, Key::I, Key::O] {
            assert!(route_key(key, Modifiers::NONE, false, true, false, true).is_none());
        }
        for key in [
            Key::H,
            Key::L,
            Key::Enter,
            Key::B,
            Key::I,
            Key::O,
            Key::ArrowLeft,
        ] {
            assert!(route_key(key, Modifiers::NONE, true, true, false, false).is_none());
            assert!(route_key(key, Modifiers::NONE, false, false, false, false).is_none());
            assert!(route_key(key, Modifiers::NONE, false, true, true, false).is_none());
            for modifiers in [Modifiers::COMMAND, Modifiers::CTRL, Modifiers::ALT] {
                assert!(route_key(key, modifiers, false, true, false, false).is_none());
            }
        }
        assert_eq!(
            route_key(Key::Escape, Modifiers::NONE, true, false, false, false),
            Some(SlipKey::Cancel)
        );
        assert!(route_key(Key::Escape, Modifiers::NONE, true, false, true, false).is_none());
    }
}
