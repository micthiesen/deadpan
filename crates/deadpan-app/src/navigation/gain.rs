//! Gain sheet shortcuts preserve native text, buttons and composition.

use eframe::egui::{Key, Modifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GainKey {
    Apply,
    Play,
    Cancel,
}

pub fn route_key(
    key: Key,
    modifiers: Modifiers,
    field: bool,
    background: bool,
    ime: bool,
    repeat: bool,
) -> Option<GainKey> {
    if ime || repeat || modifiers != Modifiers::NONE {
        return None;
    }
    match key {
        Key::Escape => Some(GainKey::Cancel),
        Key::Enter if background && !field => Some(GainKey::Apply),
        Key::Space if background && !field => Some(GainKey::Play),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gain_keys_preserve_buffered_fields_buttons_composition_and_repeat() {
        assert_eq!(
            route_key(Key::Enter, Modifiers::NONE, false, true, false, false),
            Some(GainKey::Apply)
        );
        assert_eq!(
            route_key(Key::Space, Modifiers::NONE, false, true, false, false),
            Some(GainKey::Play)
        );
        assert_eq!(
            route_key(Key::Escape, Modifiers::NONE, true, false, false, false),
            Some(GainKey::Cancel)
        );
        for key in [
            Key::Enter,
            Key::Space,
            Key::Plus,
            Key::Minus,
            Key::Tab,
            Key::H,
        ] {
            assert_eq!(
                route_key(key, Modifiers::NONE, true, false, false, false),
                None
            );
            assert_eq!(
                route_key(key, Modifiers::NONE, false, false, false, false),
                None
            );
        }
        for key in [Key::Enter, Key::Space, Key::Escape] {
            assert_eq!(
                route_key(key, Modifiers::NONE, false, true, true, false),
                None
            );
            assert_eq!(
                route_key(key, Modifiers::NONE, false, true, false, true),
                None
            );
            assert_eq!(
                route_key(key, Modifiers::COMMAND, false, true, false, false),
                None
            );
        }
    }
}
