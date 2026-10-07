//! Development access to the native host's production input parser.
//!
//! The navigation grammar shares action types with the host modules. Compile
//! those same modules for fuzzing, without the executable entrypoint; parsing
//! a keymap performs no native UI, filesystem or service work.
#![cfg(feature = "fuzzing")]

mod dialogs;
mod gag_presets;
mod gain;
mod jobs;
mod keymap;
mod keymap_file;
mod library;
#[cfg(target_os = "macos")]
mod menu;
mod model_packs;
mod navigation;
mod presentation;
mod preview;
mod project;
mod recovery;
mod transport;
#[cfg(feature = "ui-harness")]
mod ui_harness;
mod worker;
mod youtube;

/// Parse and exercise an installed production keymap without a window.
pub fn fuzz_keymap(input: &[u8]) -> Result<(), String> {
    use eframe::egui::{Key, Modifiers};
    let (mut bindings, warnings) = navigation::Bindings::from_json_reporting(input)?;
    assert!(warnings.len() <= 8, "keymap diagnostic bound");
    for key in [Key::H, Key::L, Key::A, Key::Escape] {
        let _ = bindings.route_event(
            key,
            Some(key),
            Modifiers::NONE,
            false,
            false,
            false,
            true,
            navigation::EditSelection::None,
        );
        let _ = bindings.pending_next_keys();
    }
    Ok(())
}
