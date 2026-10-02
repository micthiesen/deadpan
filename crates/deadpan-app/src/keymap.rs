//! Startup admission of an optional personal keymap. No map is installed partly.

use crate::keymap_file::KeymapFile;
use crate::navigation::Bindings;

pub struct Startup {
    pub bindings: Bindings,
    pub status: String,
    pub failed: bool,
}

impl Startup {
    /// Harnesses and the native lifecycle smoke test never read personal settings.
    pub fn shipped() -> Self {
        Self {
            bindings: Bindings::default(),
            status: "Shipped editor keys · Logical keys".into(),
            failed: false,
        }
    }

    pub fn from_file(file: KeymapFile) -> Self {
        let location = file.path.as_ref().map_or_else(
            || "the user Application Support directory".into(),
            |path| path.display().to_string(),
        );
        let admitted = file.contents.and_then(|contents| {
            contents
                .map(|bytes| Bindings::from_json(&bytes))
                .transpose()
        });
        match admitted {
            Ok(Some(bindings)) => Self {
                status: format!(
                    "Custom editor keys · {} · {location} · Restart Deadpan after changing the file.",
                    bindings.key_mode_label()
                ),
                bindings,
                failed: false,
            },
            Ok(None) => Self {
                status: format!(
                    "Shipped editor keys · Logical keys · Optional settings: {location}"
                ),
                ..Self::shipped()
            },
            Err(error) => Self {
                status: format!(
                    "Keymap error at {location}: {error}. All shipped editor keys remain active. Fix the file and restart Deadpan."
                ),
                failed: true,
                ..Self::shipped()
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::navigation::{Action, BindingId};
    use eframe::egui::{Key, Modifiers};

    fn input(contents: Result<Option<Vec<u8>>, String>) -> KeymapFile {
        KeymapFile {
            path: Some("/private/keymaps/Deadpan/keymap.json".into()),
            contents,
        }
    }

    #[test]
    fn missing_file_keeps_shipped_keys_and_reports_the_native_location() {
        let setup = Startup::from_file(input(Ok(None)));
        assert!(!setup.failed);
        assert!(
            setup
                .status
                .contains("/private/keymaps/Deadpan/keymap.json")
        );
        assert_eq!(setup.bindings.key_label(BindingId::Hold), ",h");
    }

    #[test]
    fn a_late_invalid_override_cannot_partly_install_an_earlier_valid_one() {
        let mut setup = Startup::from_file(input(Ok(Some(
            br#"{"version":1,"key_mode":"logical","bindings":[
                {"action":"frame.previous","keys":[["a"]]},
                {"action":"does.not.exist","keys":[["b"]]}
            ]}"#
            .to_vec(),
        ))));
        assert!(setup.failed);
        assert!(
            setup
                .status
                .contains("All shipped editor keys remain active")
        );
        assert_eq!(
            setup.bindings.key(Key::H, Modifiers::NONE, false, false),
            Some(Action::Step {
                forward: false,
                count: 1
            })
        );
        assert_eq!(
            setup.bindings.key(Key::A, Modifiers::NONE, false, false),
            None
        );
    }

    #[test]
    fn read_and_location_failures_remain_distinct_and_keep_defaults() {
        let read = Startup::from_file(input(Err("cannot read descriptor".into())));
        assert!(read.failed);
        assert!(read.status.contains("cannot read descriptor"));
        assert!(read.status.contains("/private/keymaps/Deadpan/keymap.json"));
        let location = Startup::from_file(KeymapFile {
            path: None,
            contents: Err("macOS returned no location".into()),
        });
        assert!(location.failed);
        assert!(location.status.contains("macOS returned no location"));
        assert_eq!(location.bindings.key_label(BindingId::Command), ":");
    }
}
