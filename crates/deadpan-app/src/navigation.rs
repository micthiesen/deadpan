use eframe::egui::{Key, Modifiers};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    Previous,
    Next,
    First,
    Last,
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

pub fn motion(
    key: Key,
    modifiers: Modifiers,
    text_focused: bool,
    ime_active: bool,
) -> Option<Motion> {
    if text_focused || ime_active || modifiers != Modifiers::NONE {
        return None;
    }
    match key {
        Key::ArrowLeft => Some(Motion::Previous),
        Key::ArrowRight => Some(Motion::Next),
        Key::Home => Some(Motion::First),
        Key::End => Some(Motion::Last),
        _ => None,
    }
}

pub fn destination(current: u64, frame_count: u64, motion: Motion) -> Option<u64> {
    let last = frame_count.checked_sub(1)?;
    let next = match motion {
        Motion::Previous => current.saturating_sub(1),
        Motion::Next => current.saturating_add(1).min(last),
        Motion::First => 0,
        Motion::Last => last,
    };
    (next != current).then_some(next)
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
    fn text_composition_and_native_modified_keys_never_step_source() {
        for key in [Key::ArrowLeft, Key::ArrowRight, Key::Home, Key::End] {
            assert!(motion(key, Modifiers::NONE, true, false).is_none());
            assert!(motion(key, Modifiers::NONE, false, true).is_none());
            for modifiers in [
                Modifiers::SHIFT,
                Modifiers::ALT,
                Modifiers::CTRL,
                Modifiers::COMMAND,
                Modifiers::MAC_CMD,
            ] {
                assert!(motion(key, modifiers, false, false).is_none());
            }
        }
        assert!(motion(Key::Tab, Modifiers::NONE, false, false).is_none());
        assert!(motion(Key::Space, Modifiers::NONE, false, false).is_none());
        assert_eq!(
            motion(Key::ArrowRight, Modifiers::NONE, false, false),
            Some(Motion::Next)
        );
    }

    #[test]
    fn navigation_bounds_original_frame_ordinals_without_wrapping() {
        assert_eq!(destination(0, 0, Motion::Next), None);
        assert_eq!(destination(0, 1, Motion::Next), None);
        assert_eq!(destination(0, 120, Motion::Previous), None);
        assert_eq!(destination(119, 120, Motion::Next), None);
        assert_eq!(destination(19, 120, Motion::Next), Some(20));
        assert_eq!(destination(19, 120, Motion::Previous), Some(18));
        assert_eq!(destination(19, 120, Motion::First), Some(0));
        assert_eq!(destination(19, 120, Motion::Last), Some(119));
        assert_eq!(destination(u64::MAX - 1, u64::MAX, Motion::Next), None);
    }
}
