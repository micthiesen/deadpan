use eframe::egui::{Key, Modifiers};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Pane {
    Sources,
    #[default]
    Viewer,
    Sequence,
}

impl Pane {
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
    Step { forward: bool, count: u32 },
    Beat { forward: bool, count: u32 },
    First,
    Last,
    Pane { reverse: bool },
    Search,
    Command,
    Escape,
    OfferInsert,
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
    g: bool,
}

impl Bindings {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn pending(&self) -> String {
        format!(
            "{}{}",
            self.count.map_or_else(String::new, |n| n.to_string()),
            if self.g { "g" } else { "" }
        )
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
            if key == Key::Colon || key == Key::Slash {
                self.clear();
                return Some(if key == Key::Colon {
                    Action::Command
                } else {
                    Action::Search
                });
            }
            if !self.g
                && let Some((_, digit)) = DIGITS.iter().find(|(bound, _)| *bound == key)
            {
                self.count = Some(
                    self.count
                        .unwrap_or(0)
                        .saturating_mul(10)
                        .saturating_add(*digit)
                        .min(1_000_000),
                );
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
                Key::R | Key::D => Some(Action::OfferInsert),
                _ => None,
            }
        };
        self.clear();
        action
    }
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
    use super::*;

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
    fn counts_saturate_and_zero_never_creates_a_zero_distance_motion() {
        let mut bindings = Bindings::default();
        for _ in 0..100 {
            bindings.key(Key::Num9, Modifiers::NONE, false, false);
        }
        assert_eq!(bindings.pending(), "1000000");
        assert_eq!(
            bindings.key(Key::H, Modifiers::NONE, false, false),
            Some(Action::Step {
                forward: false,
                count: 1_000_000
            })
        );
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
        for invalid in [Key::H, Key::Num2, Key::Space] {
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
            (Key::R, Modifiers::NONE, Action::OfferInsert),
            (Key::D, Modifiers::NONE, Action::OfferInsert),
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
}
