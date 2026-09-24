use eframe::egui::{Key, Modifiers};

pub mod camera;
pub mod command;
pub mod duration;
pub use camera::route_camera_key;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BeatEdit {
    Split,
    InsertHold(duration::DurationInput),
    Repeat(u32),
    WrapRepeat(u32),
    Delete,
    HoldDuration(deadpan_core::FrameDuration),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Pane {
    Sources,
    #[default]
    Viewer,
    Sequence,
    Inspector,
}

impl Pane {
    pub fn visible(self, inspector: bool) -> Self {
        if self == Self::Inspector && !inspector {
            Self::Viewer
        } else {
            self
        }
    }

    pub fn cycle_visible(self, reverse: bool, inspector: bool) -> Self {
        if !inspector {
            return if self == Self::Inspector {
                Self::Viewer
            } else {
                self.cycle(reverse)
            };
        }
        match (self, reverse) {
            (Self::Sources, false) | (Self::Inspector, true) => Self::Viewer,
            (Self::Viewer, false) | (Self::Sequence, true) => Self::Inspector,
            (Self::Inspector, false) | (Self::Sources, true) => Self::Sequence,
            _ => Self::Sources,
        }
    }

    pub fn cycle(self, reverse: bool) -> Self {
        match (self, reverse) {
            (Self::Sources, false) | (Self::Sequence, true) => Self::Viewer,
            (Self::Viewer, false) | (Self::Sources, true) => Self::Sequence,
            _ => Self::Sources,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    New,
    Open,
    Import,
    Insert,
    Undo,
    Redo,
    Playback,
    Step { forward: bool, count: u32 },
    Beat { forward: bool, count: u32 },
    First,
    Last,
    Pane { reverse: bool },
    Search,
    Command,
    Help,
    Escape,
    OfferInsert,
    Edit(BeatEdit),
    Framing(FramingAction),
    Invalid(&'static str),
}

/// Framing actions owned by the selected edited beat.
///
/// Camera opens a cancellable draft. PunchIn and Creep are ordinary typed
/// edits and must be committed by the project service as one history step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FramingAction {
    EnterCamera,
    PunchIn,
    Creep,
}

/// The implemented navigation vocabulary. Prefixes have no timing dependency.
const MOTIONS: &[(Key, bool)] = &[
    (Key::H, false),
    (Key::ArrowLeft, false),
    (Key::L, true),
    (Key::ArrowRight, true),
];
const DIGITS: &[(Key, u32)] = &[
    (Key::Num0, 0),
    (Key::Num1, 1),
    (Key::Num2, 2),
    (Key::Num3, 3),
    (Key::Num4, 4),
    (Key::Num5, 5),
    (Key::Num6, 6),
    (Key::Num7, 7),
    (Key::Num8, 8),
    (Key::Num9, 9),
];

#[derive(Default)]
pub struct Bindings {
    count: Option<u32>,
    count_overflow: bool,
    g: bool,
    comma: bool,
    operator: Option<Key>,
}

impl Bindings {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn pending(&self) -> String {
        format!(
            "{}{}",
            if self.count_overflow {
                "count overflow".into()
            } else {
                self.count.map_or_else(String::new, |n| n.to_string())
            },
            match self.operator {
                Some(Key::R) => "r",
                Some(Key::D) => "d",
                _ if self.comma => ",",
                _ if self.g => "g",
                _ => "",
            }
        )
    }

    pub fn pending_hint(&self) -> Option<&'static str> {
        if self.count_overflow {
            return Some("Count is too large. Esc clears it.");
        }
        match self.operator {
            Some(Key::R) => Some("r completes the Repeat · Esc cancels"),
            Some(Key::D) => Some("d cuts this whole beat · Esc cancels"),
            _ if self.comma => Some("h pause · f Camera · z punch in · c creep · Esc cancels"),
            _ if self.g => Some("g goes to the start · Esc cancels"),
            _ if self.count.is_some() => {
                Some("Then h/l to move, rr to repeat, or ,h to pause · Esc cancels")
            }
            _ => None,
        }
    }

    pub fn key(&mut self, key: Key, modifiers: Modifiers, text: bool, ime: bool) -> Option<Action> {
        if ime {
            self.clear();
            return None;
        }
        // egui also sets `command` for Control on non-macOS platforms. Check
        // this explicit Control binding before the platform command shortcuts.
        if key == Key::R && modifiers.matches_exact(Modifiers::CTRL) && !modifiers.mac_cmd {
            self.clear();
            return (!text).then_some(Action::Redo);
        }
        if (modifiers.command || modifiers.mac_cmd)
            && !modifiers.alt
            && !(modifiers.ctrl && modifiers.mac_cmd)
        {
            self.clear();
            return match (key, modifiers.shift) {
                (Key::N, false) => Some(Action::New),
                (Key::O, false) => Some(Action::Open),
                (Key::I, false) => Some(Action::Import),
                (Key::Enter, false) if !text => Some(Action::Insert),
                (Key::Z, false) if !text => Some(Action::Undo),
                (Key::Z, true) if !text => Some(Action::Redo),
                _ => None,
            };
        }
        if text {
            self.clear();
            return None;
        }
        if key == Key::Escape && modifiers == Modifiers::NONE {
            self.clear();
            return Some(Action::Escape);
        }
        if key == Key::Tab
            && !modifiers.alt
            && !modifiers.ctrl
            && !modifiers.command
            && !modifiers.mac_cmd
        {
            self.clear();
            return Some(Action::Pane {
                reverse: modifiers.shift,
            });
        }
        // These are logical symbols: layouts may need Shift or Option to type
        // them. Never infer a colon from the physical US semicolon position.
        if !modifiers.ctrl && !modifiers.command && !modifiers.mac_cmd {
            if key == Key::Comma && !self.g && !self.comma && self.operator.is_none() {
                self.comma = true;
                return Some(Action::OfferInsert);
            }
            if matches!(key, Key::Colon | Key::Slash | Key::Questionmark) {
                self.clear();
                return Some(match key {
                    Key::Colon => Action::Command,
                    Key::Questionmark => Action::Help,
                    _ => Action::Search,
                });
            }
            if !self.g
                && let Some((_, digit)) = DIGITS.iter().find(|(bound, _)| *bound == key)
            {
                if self.operator.is_some() || self.comma {
                    self.clear();
                    return Some(Action::Invalid(
                        "Put one count before the operator, for example 3rr or 3,h.",
                    ));
                }
                if let Some(count) = self
                    .count
                    .unwrap_or(0)
                    .checked_mul(10)
                    .and_then(|n| n.checked_add(*digit))
                {
                    self.count = Some(count);
                } else {
                    self.count_overflow = true;
                }
                return None;
            }
        }
        if key == Key::G && modifiers == Modifiers::SHIFT {
            self.clear();
            return Some(Action::Last);
        }
        if modifiers != Modifiers::NONE {
            self.clear();
            return None;
        }
        if key == Key::Space {
            self.clear();
            return Some(Action::Playback);
        }
        if self.count_overflow {
            self.clear();
            return Some(Action::Invalid(
                "Count exceeds 4294967295; no edit was made.",
            ));
        }
        if self.comma {
            let action = match key {
                Key::H => Action::Edit(BeatEdit::InsertHold(
                    duration::DurationInput::half_seconds(self.count.unwrap_or(1)),
                )),
                Key::F | Key::Z | Key::C if self.count.is_none() => Action::Framing(match key {
                    Key::F => FramingAction::EnterCamera,
                    Key::Z => FramingAction::PunchIn,
                    Key::C => FramingAction::Creep,
                    _ => unreachable!("matched framing key"),
                }),
                Key::F | Key::Z | Key::C => {
                    Action::Invalid("Counts apply only to ,h. Use ,f, ,z, or ,c without a count.")
                }
                _ => Action::Invalid(
                    "After comma, use h for a pause, f for Camera, z to punch in, or c to creep.",
                ),
            };
            self.clear();
            return Some(action);
        }
        if let Some(operator) = self.operator {
            let action = if key != operator {
                Action::Invalid(
                    "Only rr (repeat root beat) and dd (delete root beat) are available. Range and text-object operators are not ready.",
                )
            } else if self.count == Some(0) {
                Action::Invalid("An edit count must be positive; no edit was made.")
            } else if operator == Key::R {
                Action::Edit(BeatEdit::WrapRepeat(self.count.unwrap_or(2)))
            } else if self.count.is_some_and(|count| count != 1) {
                Action::Invalid("dd deletes one root beat. Counted deletion is not available.")
            } else {
                Action::Edit(BeatEdit::Delete)
            };
            self.clear();
            return Some(action);
        }
        if !self.g && matches!(key, Key::R | Key::D) {
            self.operator = Some(key);
            return Some(Action::OfferInsert);
        }
        if key == Key::G && !self.g {
            self.g = true;
            return None;
        }
        let count = self.count.unwrap_or(1).max(1);
        let action = if self.g {
            (key == Key::G).then_some(Action::First)
        } else if let Some((_, forward)) = MOTIONS.iter().find(|(bound, _)| *bound == key) {
            Some(Action::Step {
                forward: *forward,
                count,
            })
        } else {
            match key {
                Key::J | Key::ArrowDown => Some(Action::Beat {
                    forward: true,
                    count,
                }),
                Key::K | Key::ArrowUp => Some(Action::Beat {
                    forward: false,
                    count,
                }),
                Key::Home => Some(Action::First),
                Key::End => Some(Action::Last),
                Key::U => Some(Action::Undo),
                Key::S if self.count.is_none() => Some(Action::Edit(BeatEdit::Split)),
                Key::S => Some(Action::Invalid(
                    "Split uses the current boundary. Move with a count first, for example 12l then s.",
                )),
                _ => None,
            }
        };
        self.clear();
        action
    }
}

/// A held key may navigate, but cannot finish an operator or repeat an edit.
pub fn allows_key_repeat(key: Key, modifiers: Modifiers) -> bool {
    modifiers == Modifiers::NONE
        && matches!(
            key,
            Key::H
                | Key::J
                | Key::K
                | Key::L
                | Key::ArrowLeft
                | Key::ArrowRight
                | Key::ArrowUp
                | Key::ArrowDown
        )
}

pub fn inspector_parameter_key(
    key: Key,
    modifiers: Modifiers,
    pane: Pane,
    text: bool,
    ime: bool,
) -> bool {
    key == Key::Enter && modifiers == Modifiers::NONE && pane == Pane::Inspector && !text && !ime
}

pub fn boundary_step(current: u64, length: u64, forward: bool, count: u32) -> u64 {
    if forward {
        current.saturating_add(u64::from(count)).min(length)
    } else {
        current.saturating_sub(u64::from(count)).min(length)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextAction {
    Open,
    Leave,
}

pub fn text_action(
    key: Key,
    modifiers: Modifiers,
    text_focused: bool,
    ime_active: bool,
) -> Option<TextAction> {
    if !text_focused || ime_active || modifiers != Modifiers::NONE {
        return None;
    }
    match key {
        Key::Enter => Some(TextAction::Open),
        Key::Escape => Some(TextAction::Leave),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn space_is_one_normal_mode_toggle_and_clears_pending_edits() {
        use super::*;
        for prefix in [None, Some(Key::R), Some(Key::Comma), Some(Key::G)] {
            let mut bindings = Bindings::default();
            if let Some(key) = prefix {
                bindings.key(key, Modifiers::NONE, false, false);
            }
            assert_eq!(
                bindings.key(Key::Space, Modifiers::NONE, false, false),
                Some(Action::Playback)
            );
            assert!(bindings.pending().is_empty());
        }
        for (text, ime) in [(true, false), (false, true), (true, true)] {
            assert_eq!(
                Bindings::default().key(Key::Space, Modifiers::NONE, text, ime),
                None
            );
        }
        for modifiers in [
            Modifiers::SHIFT,
            Modifiers::ALT,
            Modifiers::CTRL,
            Modifiers::COMMAND,
            Modifiers::MAC_CMD,
        ] {
            assert_eq!(
                Bindings::default().key(Key::Space, modifiers, false, false),
                None
            );
        }
        assert!(!allows_key_repeat(Key::Space, Modifiers::NONE));
    }
    use super::*;

    #[test]
    fn pause_leader_preserves_counts_and_cannot_fire_from_text_or_repeat() {
        let mut bindings = Bindings::default();
        assert_eq!(bindings.key(Key::Num3, Modifiers::NONE, false, false), None);
        assert_eq!(
            bindings.key(Key::Comma, Modifiers::NONE, false, false),
            Some(Action::OfferInsert)
        );
        assert_eq!(bindings.pending(), "3,");
        assert!(bindings.pending_hint().unwrap().contains("Camera"));
        assert_eq!(
            bindings.key(Key::H, Modifiers::NONE, false, false),
            Some(Action::Edit(BeatEdit::InsertHold(
                duration::DurationInput::half_seconds(3)
            )))
        );
        assert!(bindings.pending().is_empty());
        assert!(!allows_key_repeat(Key::Comma, Modifiers::NONE));
        // h may repeat as a motion, but the pause prefix is consumed once.
        assert_eq!(
            bindings.key(Key::H, Modifiers::NONE, false, false),
            Some(Action::Step {
                forward: false,
                count: 1
            })
        );
        for (text, ime) in [(true, false), (false, true)] {
            bindings.key(Key::Comma, Modifiers::NONE, false, false);
            assert_eq!(bindings.key(Key::H, Modifiers::NONE, text, ime), None);
            assert!(bindings.pending().is_empty());
        }
        bindings.key(Key::Comma, Modifiers::NONE, false, false);
        assert_eq!(
            bindings.key(Key::Escape, Modifiers::NONE, false, false),
            Some(Action::Escape)
        );
        assert!(bindings.pending().is_empty());
        bindings.key(Key::Comma, Modifiers::NONE, false, false);
        assert!(matches!(
            bindings.key(Key::Num2, Modifiers::NONE, false, false),
            Some(Action::Invalid(_))
        ));
        bindings.key(Key::Comma, Modifiers::NONE, false, false);
        assert!(matches!(
            bindings.key(Key::A, Modifiers::NONE, false, false),
            Some(Action::Invalid(_))
        ));
        assert!(bindings.pending().is_empty());
    }

    #[test]
    fn logical_comma_accepts_layout_modifiers_but_never_command_or_control() {
        for modifiers in [
            Modifiers::SHIFT,
            Modifiers::ALT,
            Modifiers::ALT | Modifiers::SHIFT,
        ] {
            let mut bindings = Bindings::default();
            bindings.key(Key::Num3, Modifiers::NONE, false, false);
            assert_eq!(
                bindings.key(Key::Comma, modifiers, false, false),
                Some(Action::OfferInsert)
            );
            assert_eq!(bindings.pending(), "3,");
            assert_eq!(
                bindings.key(Key::H, Modifiers::NONE, false, false),
                Some(Action::Edit(BeatEdit::InsertHold(
                    duration::DurationInput::half_seconds(3)
                )))
            );
        }
        for modifiers in [Modifiers::CTRL, Modifiers::COMMAND, Modifiers::MAC_CMD] {
            let mut bindings = Bindings::default();
            assert_eq!(bindings.key(Key::Comma, modifiers, false, false), None);
            assert!(bindings.pending().is_empty());
        }
    }

    #[test]
    fn comma_f_z_c_are_direct_unrepeatable_camera_actions() {
        for (key, action) in [
            (Key::F, Action::Framing(FramingAction::EnterCamera)),
            (Key::Z, Action::Framing(FramingAction::PunchIn)),
            (Key::C, Action::Framing(FramingAction::Creep)),
        ] {
            let mut bindings = Bindings::default();
            assert_eq!(
                bindings.key(Key::Comma, Modifiers::NONE, false, false),
                Some(Action::OfferInsert)
            );
            assert_eq!(bindings.pending(), ",");
            assert_eq!(
                bindings.key(key, Modifiers::NONE, false, false),
                Some(action)
            );
            assert!(bindings.pending().is_empty());
            assert!(!allows_key_repeat(key, Modifiers::NONE));
        }

        for (key, modifiers) in [
            (Key::F, Modifiers::NONE),
            (Key::Z, Modifiers::NONE),
            (Key::C, Modifiers::NONE),
        ] {
            let mut bindings = Bindings::default();
            bindings.key(Key::Num2, Modifiers::NONE, false, false);
            bindings.key(Key::Comma, Modifiers::NONE, false, false);
            assert!(matches!(
                bindings.key(key, modifiers, false, false),
                Some(Action::Invalid(_))
            ));
            assert!(bindings.pending().is_empty());
        }
    }

    fn keys(bindings: &mut Bindings, keys: &[Key]) -> Option<Action> {
        keys.iter().fold(None, |_, key| {
            bindings.key(*key, Modifiers::NONE, false, false)
        })
    }

    #[test]
    fn split_uses_a_single_unmodified_key_and_respects_native_text_entry() {
        let mut bindings = Bindings::default();
        assert_eq!(
            keys(&mut bindings, &[Key::S]),
            Some(Action::Edit(BeatEdit::Split))
        );
        assert!(!allows_key_repeat(Key::S, Modifiers::NONE));
        assert!(matches!(
            keys(&mut bindings, &[Key::Num3, Key::S]),
            Some(Action::Invalid(_))
        ));
        assert!(bindings.pending().is_empty());
        for (modifiers, text, ime) in [
            (Modifiers::NONE, true, false),
            (Modifiers::NONE, false, true),
            (Modifiers::SHIFT, false, false),
            (Modifiers::ALT, false, false),
            (Modifiers::COMMAND, false, false),
        ] {
            assert_eq!(bindings.key(Key::S, modifiers, text, ime), None);
        }
        assert!(matches!(
            keys(&mut bindings, &[Key::R, Key::S]),
            Some(Action::Invalid(_))
        ));
        assert!(bindings.pending().is_empty());
    }

    #[test]
    fn repeat_and_delete_operators_keep_counts_and_wait_without_a_timeout() {
        for (prefix, pending, result) in [
            (vec![Key::R], "r", BeatEdit::WrapRepeat(2)),
            (vec![Key::Num1, Key::R], "1r", BeatEdit::WrapRepeat(1)),
            (vec![Key::Num3, Key::R], "3r", BeatEdit::WrapRepeat(3)),
            (vec![Key::D], "d", BeatEdit::Delete),
            (vec![Key::Num1, Key::D], "1d", BeatEdit::Delete),
        ] {
            let mut bindings = Bindings::default();
            assert_eq!(keys(&mut bindings, &prefix), Some(Action::OfferInsert));
            for _ in 0..1_000 {
                assert_eq!(bindings.pending(), pending);
            }
            assert_eq!(
                keys(&mut bindings, &[*prefix.last().unwrap()]),
                Some(Action::Edit(result))
            );
            assert!(bindings.pending().is_empty());
        }
    }

    #[test]
    fn invalid_operator_counts_and_unimplemented_selectors_never_commit() {
        for sequence in [
            vec![Key::Num0, Key::R, Key::R],
            vec![Key::Num0, Key::D, Key::D],
            vec![Key::Num2, Key::D, Key::D],
            vec![Key::Num3, Key::R, Key::Num2],
            vec![Key::R, Key::I],
            vec![Key::D, Key::W],
            vec![Key::R, Key::D],
        ] {
            let mut bindings = Bindings::default();
            assert!(
                matches!(keys(&mut bindings, &sequence), Some(Action::Invalid(_))),
                "{sequence:?}"
            );
            assert!(bindings.pending().is_empty());
        }
        let mut bindings = Bindings::default();
        keys(&mut bindings, &[Key::Num9; 30]);
        assert!(matches!(
            keys(&mut bindings, &[Key::R]),
            Some(Action::Invalid(_))
        ));
    }

    #[test]
    fn pending_edits_cancel_for_text_ime_context_reset_and_escape() {
        for operator in [Key::R, Key::D] {
            for (text, ime) in [(true, false), (false, true), (true, true)] {
                let mut bindings = Bindings::default();
                keys(&mut bindings, &[Key::Num3, operator]);
                assert!(bindings.key(operator, Modifiers::NONE, text, ime).is_none());
                assert!(bindings.pending().is_empty());
            }
            let mut bindings = Bindings::default();
            keys(&mut bindings, &[operator]);
            assert_eq!(keys(&mut bindings, &[Key::Escape]), Some(Action::Escape));
            keys(&mut bindings, &[operator]);
            bindings.clear();
            assert_eq!(keys(&mut bindings, &[operator]), Some(Action::OfferInsert));
        }
    }

    #[test]
    fn held_keys_cannot_finish_an_operator_or_repeat_an_edit() {
        for key in [Key::R, Key::D, Key::U, Key::Enter, Key::Num3, Key::G] {
            assert!(!allows_key_repeat(key, Modifiers::NONE));
        }
        assert!(!allows_key_repeat(Key::R, Modifiers::CTRL));
        assert!(!allows_key_repeat(Key::Z, Modifiers::COMMAND));
        for key in [Key::H, Key::J, Key::K, Key::L, Key::ArrowDown] {
            assert!(allows_key_repeat(key, Modifiers::NONE));
            assert!(!allows_key_repeat(key, Modifiers::ALT));
        }
        let mut bindings = Bindings::default();
        keys(&mut bindings, &[Key::R]);
        assert!(!allows_key_repeat(Key::R, Modifiers::NONE));
        assert_eq!(bindings.pending(), "r");
        assert_eq!(
            keys(&mut bindings, &[Key::R]),
            Some(Action::Edit(BeatEdit::WrapRepeat(2)))
        );
    }

    #[test]
    fn enter_and_escape_belong_to_composition_until_it_finishes() {
        for key in [Key::Enter, Key::Escape] {
            assert!(text_action(key, Modifiers::NONE, true, true).is_none());
            assert!(text_action(key, Modifiers::NONE, false, false).is_none());
            assert!(text_action(key, Modifiers::COMMAND, true, false).is_none());
        }
        assert_eq!(
            text_action(Key::Enter, Modifiers::NONE, true, false),
            Some(TextAction::Open)
        );
        assert_eq!(
            text_action(Key::Escape, Modifiers::NONE, true, false),
            Some(TextAction::Leave)
        );
        assert!(text_action(Key::Tab, Modifiers::NONE, true, false).is_none());
    }

    #[test]
    fn frame_and_beat_bindings_use_one_accumulated_count() {
        for (key, forward, beat) in [
            (Key::H, false, false),
            (Key::ArrowLeft, false, false),
            (Key::L, true, false),
            (Key::ArrowRight, true, false),
            (Key::J, true, true),
            (Key::ArrowDown, true, true),
            (Key::K, false, true),
            (Key::ArrowUp, false, true),
        ] {
            let mut bindings = Bindings::default();
            for digit in [Key::Num1, Key::Num2] {
                assert!(bindings.key(digit, Modifiers::NONE, false, false).is_none());
            }
            assert_eq!(bindings.pending(), "12");
            let expected = if beat {
                Action::Beat { forward, count: 12 }
            } else {
                Action::Step { forward, count: 12 }
            };
            assert_eq!(
                bindings.key(key, Modifiers::NONE, false, false),
                Some(expected)
            );
            assert!(bindings.pending().is_empty());
            assert_eq!(
                bindings.key(Key::L, Modifiers::NONE, false, false),
                Some(Action::Step {
                    forward: true,
                    count: 1
                })
            );
        }
    }

    #[test]
    fn counts_reject_overflow_and_zero_never_creates_a_zero_distance_motion() {
        let mut bindings = Bindings::default();
        for _ in 0..100 {
            bindings.key(Key::Num9, Modifiers::NONE, false, false);
        }
        assert!(bindings.pending().contains("overflow"));
        assert!(matches!(
            bindings.key(Key::H, Modifiers::NONE, false, false),
            Some(Action::Invalid(_))
        ));
        assert!(bindings.pending().is_empty());
        bindings.key(Key::Num0, Modifiers::NONE, false, false);
        assert_eq!(
            bindings.key(Key::L, Modifiers::NONE, false, false),
            Some(Action::Step {
                forward: true,
                count: 1
            })
        );
    }

    #[test]
    fn g_prefix_survives_idle_observation_without_a_clock_or_timeout() {
        let mut bindings = Bindings::default();
        bindings.key(Key::Num2, Modifiers::NONE, false, false);
        assert!(
            bindings
                .key(Key::G, Modifiers::NONE, false, false)
                .is_none()
        );
        // Rendering the status during arbitrarily many idle updates does not
        // consume the prefix. Binding state intentionally has no clock input.
        for _ in 0..10_000 {
            assert_eq!(bindings.pending(), "2g");
        }
        assert_eq!(
            bindings.key(Key::G, Modifiers::NONE, false, false),
            Some(Action::First)
        );
        assert!(bindings.pending().is_empty());
    }

    #[test]
    fn invalid_prefixes_escape_and_explicit_reset_discard_pending_input() {
        let mut bindings = Bindings::default();
        for invalid in [Key::H, Key::Num2, Key::A] {
            bindings.key(Key::G, Modifiers::NONE, false, false);
            assert!(
                bindings
                    .key(invalid, Modifiers::NONE, false, false)
                    .is_none()
            );
            assert!(bindings.pending().is_empty());
        }
        bindings.key(Key::Num3, Modifiers::NONE, false, false);
        bindings.key(Key::G, Modifiers::NONE, false, false);
        assert_eq!(
            bindings.key(Key::Escape, Modifiers::NONE, false, false),
            Some(Action::Escape)
        );
        assert!(bindings.pending().is_empty());
        bindings.key(Key::Num9, Modifiers::NONE, false, false);
        bindings.clear();
        assert_eq!(
            bindings.key(Key::J, Modifiers::NONE, false, false),
            Some(Action::Beat {
                forward: true,
                count: 1
            })
        );
    }

    #[test]
    fn endpoint_search_history_and_pane_actions_clear_counts() {
        for (key, modifiers, expected) in [
            (Key::Home, Modifiers::NONE, Action::First),
            (Key::End, Modifiers::NONE, Action::Last),
            (Key::G, Modifiers::SHIFT, Action::Last),
            (Key::Slash, Modifiers::NONE, Action::Search),
            (Key::Colon, Modifiers::NONE, Action::Command),
            (Key::U, Modifiers::NONE, Action::Undo),
            (Key::Tab, Modifiers::NONE, Action::Pane { reverse: false }),
            (Key::Tab, Modifiers::SHIFT, Action::Pane { reverse: true }),
        ] {
            let mut bindings = Bindings::default();
            bindings.key(Key::Num4, Modifiers::NONE, false, false);
            assert_eq!(bindings.key(key, modifiers, false, false), Some(expected));
            assert!(bindings.pending().is_empty());
        }
    }

    #[test]
    fn logical_symbols_and_digits_do_not_assume_a_us_layout() {
        for modifiers in [
            Modifiers::NONE,
            Modifiers::SHIFT,
            Modifiers::ALT,
            Modifiers::ALT | Modifiers::SHIFT,
        ] {
            let mut bindings = Bindings::default();
            assert_eq!(
                bindings.key(Key::Colon, modifiers, false, false),
                Some(Action::Command)
            );
            assert_eq!(
                bindings.key(Key::Slash, modifiers, false, false),
                Some(Action::Search)
            );
            bindings.key(Key::Num3, modifiers, false, false);
            assert_eq!(
                bindings.key(Key::L, Modifiers::NONE, false, false),
                Some(Action::Step {
                    forward: true,
                    count: 3
                })
            );
            assert!(
                bindings
                    .key(Key::Semicolon, modifiers, false, false)
                    .is_none()
            );
        }
    }

    #[test]
    fn native_command_shortcuts_preserve_text_editing_history() {
        for command in [
            Modifiers::COMMAND,
            Modifiers::MAC_CMD,
            Modifiers::MAC_CMD | Modifiers::COMMAND,
            Modifiers::CTRL | Modifiers::COMMAND,
        ] {
            let mut bindings = Bindings::default();
            for text in [false, true] {
                for (key, expected) in [
                    (Key::N, Action::New),
                    (Key::O, Action::Open),
                    (Key::I, Action::Import),
                ] {
                    assert_eq!(bindings.key(key, command, text, false), Some(expected));
                }
            }
            for (key, modifiers, expected) in [
                (Key::Enter, command, Action::Insert),
                (Key::Z, command, Action::Undo),
                (Key::Z, command | Modifiers::SHIFT, Action::Redo),
            ] {
                assert_eq!(bindings.key(key, modifiers, false, false), Some(expected));
                assert!(bindings.key(key, modifiers, true, false).is_none());
            }
            for key in [Key::A, Key::C, Key::V, Key::X] {
                assert!(bindings.key(key, command, true, false).is_none());
            }
        }
    }

    #[test]
    fn control_r_is_redo_only_outside_text_and_composition() {
        for modifiers in [Modifiers::CTRL, Modifiers::CTRL | Modifiers::COMMAND] {
            let mut bindings = Bindings::default();
            bindings.key(Key::Num4, Modifiers::NONE, false, false);
            assert_eq!(
                bindings.key(Key::R, modifiers, false, false),
                Some(Action::Redo)
            );
            assert!(bindings.pending().is_empty());
            assert!(bindings.key(Key::R, modifiers, true, false).is_none());
            assert!(bindings.key(Key::R, modifiers, false, true).is_none());
        }
    }

    #[test]
    fn text_and_ime_discard_pending_normal_mode_bindings() {
        let normal_keys = [
            Key::H,
            Key::L,
            Key::J,
            Key::K,
            Key::ArrowLeft,
            Key::ArrowRight,
            Key::ArrowUp,
            Key::ArrowDown,
            Key::Home,
            Key::End,
            Key::G,
            Key::U,
            Key::R,
            Key::D,
            Key::Slash,
            Key::Colon,
            Key::Tab,
            Key::Escape,
            Key::Num3,
        ];
        for (text, ime) in [(true, false), (false, true), (true, true)] {
            for key in normal_keys {
                let mut bindings = Bindings::default();
                bindings.key(Key::Num3, Modifiers::NONE, false, false);
                bindings.key(Key::G, Modifiers::NONE, false, false);
                assert!(bindings.key(key, Modifiers::NONE, text, ime).is_none());
                assert!(bindings.pending().is_empty());
            }
        }
        for key in [Key::N, Key::O, Key::I, Key::Z, Key::Enter] {
            let mut bindings = Bindings::default();
            assert!(bindings.key(key, Modifiers::COMMAND, false, true).is_none());
        }
    }

    #[test]
    fn modified_navigation_and_reserved_shortcuts_do_not_edit() {
        for key in [
            Key::H,
            Key::J,
            Key::K,
            Key::L,
            Key::ArrowLeft,
            Key::ArrowRight,
            Key::Home,
            Key::End,
        ] {
            for modifiers in [
                Modifiers::SHIFT,
                Modifiers::ALT,
                Modifiers::CTRL,
                Modifiers::COMMAND,
                Modifiers::MAC_CMD,
            ] {
                let mut bindings = Bindings::default();
                bindings.key(Key::Num3, Modifiers::NONE, false, false);
                assert!(bindings.key(key, modifiers, false, false).is_none());
                assert!(bindings.pending().is_empty());
            }
        }
        for modifiers in [
            Modifiers::COMMAND | Modifiers::ALT,
            Modifiers::MAC_CMD | Modifiers::COMMAND | Modifiers::CTRL,
        ] {
            for key in [Key::N, Key::O, Key::I, Key::Z, Key::R, Key::Tab, Key::Colon] {
                assert!(
                    Bindings::default()
                        .key(key, modifiers, false, false)
                        .is_none()
                );
            }
        }
    }

    #[test]
    fn boundaries_include_the_end_and_saturate_without_wrapping() {
        assert_eq!(boundary_step(0, 0, true, 1), 0);
        assert_eq!(boundary_step(0, 1, true, 1), 1);
        assert_eq!(boundary_step(0, 120, false, 20), 0);
        assert_eq!(boundary_step(119, 120, true, 1), 120);
        assert_eq!(boundary_step(120, 120, true, 1), 120);
        assert_eq!(boundary_step(120, 120, false, 1), 119);
        assert_eq!(boundary_step(19, 120, true, 12), 31);
        assert_eq!(boundary_step(19, 120, false, 12), 7);
        assert_eq!(boundary_step(u64::MAX - 1, u64::MAX, true, 2), u64::MAX);
        assert_eq!(boundary_step(u64::MAX, 120, true, u32::MAX), 120);
    }

    #[test]
    fn pane_order_cycles_both_directions_and_reverses_exactly() {
        assert_eq!(Pane::default(), Pane::Viewer);
        assert_eq!(Pane::Sources.cycle(false), Pane::Viewer);
        assert_eq!(Pane::Viewer.cycle(false), Pane::Sequence);
        assert_eq!(Pane::Sequence.cycle(false), Pane::Sources);
        for pane in [Pane::Sources, Pane::Viewer, Pane::Sequence] {
            assert_eq!(pane.cycle(false).cycle(true), pane);
            assert_eq!(pane.cycle(true).cycle(false), pane);
            assert_eq!(pane.cycle(false).cycle(false).cycle(false), pane);
        }
    }

    #[test]
    fn inspector_participates_only_while_visible() {
        let visible = [Pane::Sources, Pane::Viewer, Pane::Inspector, Pane::Sequence];
        for (index, pane) in visible.into_iter().enumerate() {
            assert_eq!(
                pane.cycle_visible(false, true),
                visible[(index + 1) % visible.len()]
            );
            assert_eq!(
                pane.cycle_visible(false, true).cycle_visible(true, true),
                pane
            );
        }
        for pane in [Pane::Sources, Pane::Viewer, Pane::Sequence] {
            for reverse in [false, true] {
                assert_ne!(pane.cycle_visible(reverse, false), Pane::Inspector);
                assert_eq!(pane.cycle_visible(reverse, false), pane.cycle(reverse));
            }
        }
        assert_eq!(Pane::Inspector.cycle_visible(false, false), Pane::Viewer);
        assert_eq!(Pane::Inspector.visible(false), Pane::Viewer);
        assert_eq!(Pane::Inspector.visible(true), Pane::Inspector);
        assert_eq!(Pane::Sequence.visible(false), Pane::Sequence);
    }

    #[test]
    fn inspector_enter_is_distinct_from_native_text_and_composition() {
        assert!(inspector_parameter_key(
            Key::Enter,
            Modifiers::NONE,
            Pane::Inspector,
            false,
            false
        ));
        for (pane, text, ime) in [
            (Pane::Viewer, false, false),
            (Pane::Sequence, false, false),
            (Pane::Inspector, true, false),
            (Pane::Inspector, false, true),
        ] {
            assert!(!inspector_parameter_key(
                Key::Enter,
                Modifiers::NONE,
                pane,
                text,
                ime
            ));
        }
        assert!(!inspector_parameter_key(
            Key::Enter,
            Modifiers::COMMAND,
            Pane::Inspector,
            false,
            false
        ));
        assert!(!inspector_parameter_key(
            Key::Escape,
            Modifiers::NONE,
            Pane::Inspector,
            false,
            false
        ));
    }

    #[test]
    fn logical_question_mark_opens_help_and_remains_text_during_editing() {
        for modifiers in [Modifiers::NONE, Modifiers::SHIFT, Modifiers::ALT] {
            let mut bindings = Bindings::default();
            assert_eq!(
                bindings.key(Key::Questionmark, modifiers, false, false),
                Some(Action::Help)
            );
            assert_eq!(
                bindings.key(Key::Questionmark, modifiers, true, false),
                None
            );
            assert_eq!(
                bindings.key(Key::Questionmark, modifiers, false, true),
                None
            );
            assert!(bindings.pending().is_empty());
        }
    }
}
