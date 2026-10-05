//! New-from-URL keys. Text editing, paste and focused button activation stay
//! native; only plain Enter and Escape act on the import step.

use eframe::egui::{Key, Modifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UrlKey {
    /// The current step's primary action: fetch details, confirm the
    /// download, install the downloader or try again.
    Primary,
    /// Cancel running work, decline, dismiss or leave the field.
    Cancel,
}

/// `control` is a focused native button or checkbox, which keeps its own
/// Enter/Space activation. Composition and held keys never act.
pub fn route_key(
    key: Key,
    modifiers: Modifiers,
    control: bool,
    ime: bool,
    repeat: bool,
) -> Option<UrlKey> {
    if ime || repeat || modifiers != Modifiers::NONE {
        return None;
    }
    match key {
        Key::Escape => Some(UrlKey::Cancel),
        Key::Enter if !control => Some(UrlKey::Primary),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_enter_and_escape_act_and_native_owners_keep_the_rest() {
        assert_eq!(
            route_key(Key::Enter, Modifiers::NONE, false, false, false),
            Some(UrlKey::Primary)
        );
        assert_eq!(
            route_key(Key::Escape, Modifiers::NONE, false, false, false),
            Some(UrlKey::Cancel)
        );
        // A focused button keeps Enter; Escape still cancels the step.
        assert_eq!(
            route_key(Key::Enter, Modifiers::NONE, true, false, false),
            None
        );
        assert_eq!(
            route_key(Key::Escape, Modifiers::NONE, true, false, false),
            Some(UrlKey::Cancel)
        );
        for (key, modifiers) in [
            (Key::V, Modifiers::COMMAND),
            (Key::A, Modifiers::COMMAND),
            (Key::Z, Modifiers::COMMAND),
            (Key::Enter, Modifiers::SHIFT),
            (Key::Enter, Modifiers::COMMAND),
            (Key::Escape, Modifiers::SHIFT),
            (Key::Space, Modifiers::NONE),
            (Key::Tab, Modifiers::NONE),
            (Key::J, Modifiers::NONE),
            (Key::Colon, Modifiers::NONE),
        ] {
            assert_eq!(route_key(key, modifiers, false, false, false), None);
        }
        for key in [Key::Enter, Key::Escape] {
            assert_eq!(
                route_key(key, Modifiers::NONE, false, true, false),
                None,
                "IME owns its confirmation and cancellation"
            );
            assert_eq!(
                route_key(key, Modifiers::NONE, false, false, true),
                None,
                "held keys never repeat an import step"
            );
        }
    }
}
