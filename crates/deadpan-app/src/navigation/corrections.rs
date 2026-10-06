//! Keys of the transcript and pause correction sheet. The sheet is modal: its
//! letters never reach the editor. Text entry and IME composition stay
//! native; only Enter and Escape act while the word field is focused.

use eframe::egui::{Key, Modifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorrectionKey {
    /// Select the previous or next word or pause.
    Previous,
    Next,
    /// Choose the start or end edge of the selected item for moving.
    StartEdge,
    EndEdge,
    /// Move the chosen edge 10 ms earlier or later.
    Nudge {
        later: bool,
    },
    /// Move the chosen edge to the previous or next measured edge.
    Snap {
        later: bool,
    },
    /// Edit the selected word's text (Enter or c).
    EditText,
    /// Join the selected word with the next (J).
    Join,
    /// Remove the selected word or pause (x only: Delete and Backspace are
    /// text-editing keys and never delete here).
    Remove,
    /// Discard unreadable corrections, or drop those that no longer apply
    /// (Shift+D).
    Discard,
    /// Add a pause after the selected word (p).
    AddPause,
    /// Apply the edited text or moved edge (Enter).
    Apply,
    Undo,
    Redo,
    /// Discard the edge or text draft, else close (Escape).
    Cancel,
}

/// `field`: the word text field is focused or a text draft is open (even
/// when a click moved focus away, keys go to the draft, never to actions). `editing_edge`: an edge is
/// chosen, so arrows and h/l move it. `pending_text`: a text draft exists.
pub fn route_key(
    key: Key,
    modifiers: Modifiers,
    field: bool,
    editing_edge: bool,
    ime: bool,
    repeat: bool,
) -> Option<CorrectionKey> {
    if ime {
        return None;
    }
    if field {
        return match (key, modifiers) {
            (Key::Enter, Modifiers::NONE) if !repeat => Some(CorrectionKey::Apply),
            (Key::Escape, Modifiers::NONE) if !repeat => Some(CorrectionKey::Cancel),
            _ => None,
        };
    }
    let shift = modifiers == Modifiers::SHIFT;
    let plain = modifiers == Modifiers::NONE;
    // The app's shared ⌘ test: a macOS press also carries `mac_cmd`.
    let command = super::native_command(modifiers);
    // Held arrows and h/l repeat; everything else acts once per press.
    let movement = match key {
        Key::ArrowLeft | Key::H if plain => Some(if editing_edge {
            CorrectionKey::Nudge { later: false }
        } else {
            CorrectionKey::Previous
        }),
        Key::ArrowRight | Key::L if plain => Some(if editing_edge {
            CorrectionKey::Nudge { later: true }
        } else {
            CorrectionKey::Next
        }),
        Key::ArrowLeft | Key::H if shift && editing_edge => {
            Some(CorrectionKey::Snap { later: false })
        }
        Key::ArrowRight | Key::L if shift && editing_edge => {
            Some(CorrectionKey::Snap { later: true })
        }
        _ => None,
    };
    if movement.is_some() {
        return movement;
    }
    if repeat {
        return None;
    }
    match key {
        Key::B if plain => Some(CorrectionKey::StartEdge),
        Key::E if plain => Some(CorrectionKey::EndEdge),
        Key::Enter if plain => Some(if editing_edge {
            CorrectionKey::Apply
        } else {
            CorrectionKey::EditText
        }),
        Key::C if plain => Some(CorrectionKey::EditText),
        Key::J if shift => Some(CorrectionKey::Join),
        Key::X if plain => Some(CorrectionKey::Remove),
        Key::D if shift => Some(CorrectionKey::Discard),
        Key::P if plain => Some(CorrectionKey::AddPause),
        Key::U if plain => Some(CorrectionKey::Undo),
        Key::U if shift => Some(CorrectionKey::Redo),
        Key::R if command && !modifiers.shift => Some(CorrectionKey::Redo),
        Key::Z if command && !modifiers.shift => Some(CorrectionKey::Undo),
        Key::Z if command => Some(CorrectionKey::Redo),
        Key::Escape if plain => Some(CorrectionKey::Cancel),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_select_edit_and_move_edges_without_reaching_the_editor() {
        let route = |key, modifiers, edge| route_key(key, modifiers, false, edge, false, false);
        assert_eq!(
            route(Key::L, Modifiers::NONE, false),
            Some(CorrectionKey::Next)
        );
        assert_eq!(
            route(Key::ArrowLeft, Modifiers::NONE, false),
            Some(CorrectionKey::Previous)
        );
        assert_eq!(
            route(Key::B, Modifiers::NONE, false),
            Some(CorrectionKey::StartEdge)
        );
        assert_eq!(
            route(Key::L, Modifiers::NONE, true),
            Some(CorrectionKey::Nudge { later: true })
        );
        assert_eq!(
            route(Key::H, Modifiers::SHIFT, true),
            Some(CorrectionKey::Snap { later: false })
        );
        assert_eq!(route(Key::H, Modifiers::SHIFT, false), None);
        assert_eq!(
            route(Key::Enter, Modifiers::NONE, true),
            Some(CorrectionKey::Apply)
        );
        assert_eq!(
            route(Key::Enter, Modifiers::NONE, false),
            Some(CorrectionKey::EditText)
        );
        assert_eq!(
            route(Key::J, Modifiers::SHIFT, false),
            Some(CorrectionKey::Join)
        );
        assert_eq!(route(Key::J, Modifiers::NONE, false), None);
        assert_eq!(
            route(Key::X, Modifiers::NONE, false),
            Some(CorrectionKey::Remove)
        );
        assert_eq!(
            route(Key::U, Modifiers::NONE, false),
            Some(CorrectionKey::Undo)
        );
        assert_eq!(
            route(Key::U, Modifiers::SHIFT, false),
            Some(CorrectionKey::Redo)
        );
        assert_eq!(
            route(Key::R, Modifiers::COMMAND, false),
            Some(CorrectionKey::Redo)
        );
        assert_eq!(
            route(Key::P, Modifiers::NONE, false),
            Some(CorrectionKey::AddPause)
        );
        assert_eq!(
            route(Key::Escape, Modifiers::NONE, true),
            Some(CorrectionKey::Cancel)
        );
    }

    #[test]
    fn the_text_field_and_composition_keep_their_input() {
        for key in [Key::H, Key::L, Key::X, Key::U, Key::Space, Key::Backspace] {
            assert_eq!(
                route_key(key, Modifiers::NONE, true, false, false, false),
                None
            );
        }
        assert_eq!(
            route_key(Key::Enter, Modifiers::NONE, true, false, false, false),
            Some(CorrectionKey::Apply)
        );
        assert_eq!(
            route_key(Key::Escape, Modifiers::NONE, true, true, false, false),
            Some(CorrectionKey::Cancel)
        );
        for key in [Key::Enter, Key::Escape, Key::L] {
            assert_eq!(
                route_key(key, Modifiers::NONE, false, false, true, false),
                None
            );
        }
    }

    #[test]
    fn held_movement_repeats_but_actions_do_not() {
        assert_eq!(
            route_key(Key::L, Modifiers::NONE, false, true, false, true),
            Some(CorrectionKey::Nudge { later: true })
        );
        for key in [Key::X, Key::Enter, Key::U, Key::P] {
            assert_eq!(
                route_key(key, Modifiers::NONE, false, false, false, true),
                None
            );
        }
    }

    #[test]
    fn command_shift_z_redoes_with_and_without_the_macos_command_flag() {
        let route = |modifiers| route_key(Key::Z, modifiers, false, false, false, false);
        for command in [Modifiers::COMMAND, Modifiers::MAC_CMD | Modifiers::COMMAND] {
            assert_eq!(route(command), Some(CorrectionKey::Undo), "{command:?}");
            assert_eq!(
                route(command | Modifiers::SHIFT),
                Some(CorrectionKey::Redo),
                "{command:?}"
            );
            assert_eq!(
                route_key(Key::R, command, false, false, false, false),
                Some(CorrectionKey::Redo)
            );
            assert_eq!(route(command | Modifiers::ALT | Modifiers::SHIFT), None);
        }
    }
}
