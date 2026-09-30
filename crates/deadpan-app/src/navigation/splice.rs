//! Place-slice keys share native focus and composition ownership with widgets.

use eframe::egui::{Key, Modifiers};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpliceKey {
    Apply,
    Cancel,
    Play,
    Loop,
    Compare,
    Replace,
    In,
    Out,
    Destination,
    Picture,
    Step(bool),
    Boundary(bool),
    Count(u32),
}

pub fn route_key(
    key: Key,
    modifiers: Modifiers,
    field: bool,
    background: bool,
    ime: bool,
    repeat: bool,
) -> Option<SpliceKey> {
    if ime || field {
        return None;
    }
    if key == Key::Escape && modifiers.is_none() && !repeat {
        return Some(SpliceKey::Cancel);
    }
    if !background {
        return None;
    }
    if key == Key::Space && modifiers.shift_only() && !repeat {
        return Some(SpliceKey::Loop);
    }
    if !modifiers.is_none() {
        return None;
    }
    if matches!(key, Key::H | Key::ArrowLeft | Key::L | Key::ArrowRight) {
        return Some(SpliceKey::Step(matches!(key, Key::L | Key::ArrowRight)));
    }
    if repeat {
        return None;
    }
    Some(match key {
        Key::Enter => SpliceKey::Apply,
        Key::Space => SpliceKey::Play,
        Key::B => SpliceKey::Compare,
        Key::R => SpliceKey::Replace,
        Key::I => SpliceKey::In,
        Key::O => SpliceKey::Out,
        Key::D => SpliceKey::Destination,
        Key::F => SpliceKey::Picture,
        Key::J => SpliceKey::Boundary(true),
        Key::K => SpliceKey::Boundary(false),
        Key::Num0 => SpliceKey::Count(0),
        Key::Num1 => SpliceKey::Count(1),
        Key::Num2 => SpliceKey::Count(2),
        Key::Num3 => SpliceKey::Count(3),
        Key::Num4 => SpliceKey::Count(4),
        Key::Num5 => SpliceKey::Count(5),
        Key::Num6 => SpliceKey::Count(6),
        Key::Num7 => SpliceKey::Count(7),
        Key::Num8 => SpliceKey::Count(8),
        Key::Num9 => SpliceKey::Count(9),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slice_shortcuts_preserve_native_buttons_composition_and_global_chords() {
        for key in [
            Key::Enter,
            Key::Space,
            Key::I,
            Key::O,
            Key::D,
            Key::B,
            Key::R,
            Key::H,
            Key::L,
        ] {
            assert_eq!(
                route_key(key, Modifiers::NONE, true, true, false, false),
                None
            );
            assert_eq!(
                route_key(key, Modifiers::NONE, false, false, false, false),
                None
            );
            assert_eq!(
                route_key(key, Modifiers::NONE, false, true, true, false),
                None
            );
            assert_eq!(
                route_key(key, Modifiers::COMMAND, false, true, false, false),
                None
            );
        }
        assert_eq!(
            route_key(Key::Space, Modifiers::SHIFT, false, true, false, false),
            Some(SpliceKey::Loop)
        );
        assert_eq!(
            route_key(Key::H, Modifiers::NONE, false, true, false, true),
            Some(SpliceKey::Step(false))
        );
        assert_eq!(
            route_key(Key::Enter, Modifiers::NONE, false, true, false, true),
            None
        );
        assert_eq!(
            route_key(Key::R, Modifiers::NONE, false, true, false, true),
            None
        );
        assert_eq!(
            route_key(Key::R, Modifiers::NONE, false, true, false, false),
            Some(SpliceKey::Replace)
        );
        assert_eq!(
            route_key(Key::Escape, Modifiers::NONE, false, false, false, false),
            Some(SpliceKey::Cancel)
        );
    }
}
