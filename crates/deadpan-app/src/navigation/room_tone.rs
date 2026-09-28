//! Modal shortcuts leave text editing and focused button activation native.

use eframe::egui::{Key, Modifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoomToneKey {
    Apply,
    Play,
    Loop,
    Cancel,
}

pub fn route_key(
    key: Key,
    modifiers: Modifiers,
    field: bool,
    background: bool,
    ime: bool,
    repeat: bool,
) -> Option<RoomToneKey> {
    if ime || repeat {
        return None;
    }
    if modifiers == Modifiers::NONE {
        match key {
            Key::Escape => Some(RoomToneKey::Cancel),
            Key::Enter if field || background => Some(RoomToneKey::Apply),
            Key::Space if background && !field => Some(RoomToneKey::Play),
            _ => None,
        }
    } else if modifiers == Modifiers::SHIFT && key == Key::Space && !field {
        Some(RoomToneKey::Loop)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modal_shortcuts_preserve_field_and_control_ownership_and_composition() {
        assert_eq!(
            route_key(Key::Enter, Modifiers::NONE, true, false, false, false),
            Some(RoomToneKey::Apply)
        );
        assert_eq!(
            route_key(Key::Space, Modifiers::NONE, false, true, false, false),
            Some(RoomToneKey::Play)
        );
        assert_eq!(
            route_key(Key::Space, Modifiers::SHIFT, false, false, false, false),
            Some(RoomToneKey::Loop)
        );
        assert_eq!(
            route_key(Key::Escape, Modifiers::NONE, true, false, false, false),
            Some(RoomToneKey::Cancel)
        );
        for (key, modifiers) in [
            (Key::Space, Modifiers::NONE),
            (Key::Space, Modifiers::SHIFT),
            (Key::H, Modifiers::NONE),
            (Key::Tab, Modifiers::NONE),
            (Key::Z, Modifiers::COMMAND),
        ] {
            assert_eq!(route_key(key, modifiers, true, false, false, false), None);
        }
        for key in [Key::Enter, Key::Space] {
            assert_eq!(
                route_key(key, Modifiers::NONE, false, false, false, false),
                None,
                "native focused control"
            );
        }
        for key in [Key::Enter, Key::Escape, Key::Space] {
            assert_eq!(
                route_key(key, Modifiers::NONE, true, true, true, false),
                None,
                "IME owns input"
            );
            assert_eq!(
                route_key(key, Modifiers::NONE, false, true, false, true),
                None,
                "held activation never repeats"
            );
        }
    }
}
