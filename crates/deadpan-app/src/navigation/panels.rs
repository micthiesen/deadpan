//! Fixed keys of the Jobs, Storage and Marks panels and the Keys sheet.
//!
//! These panels have no router of their own: each press is looked up in the
//! action registry's declared chords, so the registry is their key map. The
//! Kestrel audit checks the same lookups against reserved global chords.

use eframe::egui::{Key, Modifiers};

use super::registry::{Mode, mode_chords};

/// The registered action a press reaches in a panel.
fn registered(mode: Mode, key: Key, modifiers: Modifiers) -> Option<&'static str> {
    mode_chords(mode)
        .find(|(_, chord)| chord.key == key && chord.mods.accepts(modifiers))
        .map(|(spec, _)| spec.id)
}

/// The panel action of a registered press, from a typed `(action id, key)`
/// table. Tests prove each table covers exactly its mode's declared chords,
/// so a missing entry cannot hide behind a fallback.
fn lookup<T: Copy>(
    mode: Mode,
    table: &[(&str, Key, T)],
    key: Key,
    modifiers: Modifiers,
) -> Option<T> {
    let id = registered(mode, key, modifiers)?;
    table
        .iter()
        .find(|(owner, candidate, _)| *owner == id && *candidate == key)
        .map(|(_, _, action)| *action)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobsKey {
    Next,
    Previous,
    Cancel,
    Retry,
    Discard,
}

pub(super) const JOBS: &[(&str, Key, JobsKey)] = &[
    ("jobs.select", Key::J, JobsKey::Next),
    ("jobs.select", Key::ArrowDown, JobsKey::Next),
    ("jobs.select", Key::K, JobsKey::Previous),
    ("jobs.select", Key::ArrowUp, JobsKey::Previous),
    ("jobs.cancel", Key::X, JobsKey::Cancel),
    ("jobs.retry", Key::R, JobsKey::Retry),
    ("jobs.discard", Key::D, JobsKey::Discard),
];

/// A Jobs panel press, after `mode_key` has read its typed character.
pub fn jobs_key(key: Key, modifiers: Modifiers) -> Option<JobsKey> {
    lookup(Mode::Jobs, JOBS, key, modifiers)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageKey {
    Preview,
    Remove,
    CleanCaches,
    PortableCopy,
    Refresh,
}

pub(super) const STORAGE: &[(&str, Key, StorageKey)] = &[
    ("storage.preview", Key::P, StorageKey::Preview),
    ("storage.remove", Key::R, StorageKey::Remove),
    ("storage.caches", Key::C, StorageKey::CleanCaches),
    ("storage.copy", Key::S, StorageKey::PortableCopy),
    ("storage.refresh", Key::U, StorageKey::Refresh),
];

/// A Storage panel press, after `mode_key` has read its typed character, as
/// in Jobs. Only an unmodified key acts: Shift+R must not confirm a removal,
/// and Option chords stay with Kestrel.
pub fn storage_key(key: Key, modifiers: Modifiers) -> Option<StorageKey> {
    lookup(Mode::Storage, STORAGE, key, modifiers)
}

pub(super) const MARKS: &[(&str, Key, bool)] = &[
    ("marks.history", Key::O, false),
    ("marks.history", Key::I, true),
];

/// Marks panel jump history: `Some(true)` is forward (Ctrl I).
pub fn marks_key(key: Key, modifiers: Modifiers) -> Option<bool> {
    lookup(Mode::Marks, MARKS, key, modifiers)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HelpKey {
    LineDown,
    LineUp,
    PageDown,
    PageUp,
    Top,
    Bottom,
    Search,
    Close,
}

pub(super) const HELP: &[(&str, Key, HelpKey)] = &[
    ("help.scroll", Key::J, HelpKey::LineDown),
    ("help.scroll", Key::ArrowDown, HelpKey::LineDown),
    ("help.scroll", Key::K, HelpKey::LineUp),
    ("help.scroll", Key::ArrowUp, HelpKey::LineUp),
    ("help.scroll", Key::PageDown, HelpKey::PageDown),
    ("help.scroll", Key::PageUp, HelpKey::PageUp),
    ("help.scroll", Key::Home, HelpKey::Top),
    ("help.scroll", Key::End, HelpKey::Bottom),
    ("help.search", Key::Slash, HelpKey::Search),
    ("help.close", Key::Escape, HelpKey::Close),
];

/// A Keys sheet press while its search field does not own the keyboard.
pub fn help_key(key: Key, modifiers: Modifiers) -> Option<HelpKey> {
    lookup(Mode::Help, HELP, key, modifiers)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn former_jobs(key: Key) -> Option<JobsKey> {
        match key {
            Key::ArrowDown | Key::J => Some(JobsKey::Next),
            Key::ArrowUp | Key::K => Some(JobsKey::Previous),
            Key::X => Some(JobsKey::Cancel),
            Key::R => Some(JobsKey::Retry),
            Key::D => Some(JobsKey::Discard),
            _ => None,
        }
    }

    fn former_storage(key: Key) -> Option<StorageKey> {
        match key {
            Key::P => Some(StorageKey::Preview),
            Key::R => Some(StorageKey::Remove),
            Key::C => Some(StorageKey::CleanCaches),
            Key::S => Some(StorageKey::PortableCopy),
            Key::U => Some(StorageKey::Refresh),
            _ => None,
        }
    }

    fn former_marks(key: Key) -> Option<bool> {
        match key {
            Key::O => Some(false),
            Key::I => Some(true),
            _ => None,
        }
    }

    fn former_help(key: Key) -> Option<HelpKey> {
        match key {
            Key::J | Key::ArrowDown => Some(HelpKey::LineDown),
            Key::K | Key::ArrowUp => Some(HelpKey::LineUp),
            Key::PageDown => Some(HelpKey::PageDown),
            Key::PageUp => Some(HelpKey::PageUp),
            Key::Home => Some(HelpKey::Top),
            Key::End => Some(HelpKey::Bottom),
            Key::Slash => Some(HelpKey::Search),
            Key::Escape => Some(HelpKey::Close),
            _ => None,
        }
    }

    /// The panels behave exactly as their former hand-written matches did,
    /// except that Storage no longer ignores Option.
    #[test]
    fn registry_lookups_preserve_the_former_panel_keys() {
        for key in Key::ALL {
            for modifiers in [
                Modifiers::NONE,
                Modifiers::SHIFT,
                Modifiers::ALT,
                Modifiers::SHIFT | Modifiers::ALT,
                Modifiers::CTRL,
                Modifiers::COMMAND,
            ] {
                let plain = modifiers.is_none();
                let jobs = if plain { former_jobs(*key) } else { None };
                assert_eq!(jobs_key(*key, modifiers), jobs, "{key:?} {modifiers:?}");
                // Formerly `matches_logically(NONE)`, which also admitted
                // Option (Kestrel's Option+P and Option+S) and Shift.
                let storage = if plain { former_storage(*key) } else { None };
                assert_eq!(
                    storage_key(*key, modifiers),
                    storage,
                    "{key:?} {modifiers:?}"
                );
                let marks = if modifiers == Modifiers::CTRL {
                    former_marks(*key)
                } else {
                    None
                };
                assert_eq!(marks_key(*key, modifiers), marks, "{key:?} {modifiers:?}");
                let help = if plain { former_help(*key) } else { None };
                assert_eq!(help_key(*key, modifiers), help, "{key:?} {modifiers:?}");
            }
        }
    }

    fn covers<T>(mode: Mode, table: &[(&str, Key, T)]) {
        let mut declared: Vec<_> = mode_chords(mode)
            .map(|(spec, chord)| (spec.id, chord.key))
            .collect();
        let mut mapped: Vec<_> = table.iter().map(|(id, key, _)| (*id, *key)).collect();
        declared.sort_by_key(|(id, key)| (*id, format!("{key:?}")));
        mapped.sort_by_key(|(id, key)| (*id, format!("{key:?}")));
        assert_eq!(
            declared, mapped,
            "{mode:?} table and registry chords differ"
        );
    }

    /// Each panel's typed table covers exactly its registered chords.
    #[test]
    fn panel_tables_cover_exactly_the_registered_chords() {
        covers(Mode::Jobs, JOBS);
        covers(Mode::Storage, STORAGE);
        covers(Mode::Marks, MARKS);
        covers(Mode::Help, HELP);
    }
}
