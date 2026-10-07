#![no_main]
//! History patch wire (`DocumentPatch`, the store's forward/inverse rows)
//! applied to every fixture document: a refusal or a valid document.
use deadpan_core::DocumentPatch;
use deadpan_fuzz::{DOCUMENTS, refused};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    let patch = match serde_json::from_slice::<DocumentPatch>(input) {
        Ok(patch) => patch,
        Err(error) => return refused(error),
    };
    for document in DOCUMENTS.iter() {
        match patch.apply(document) {
            Ok(after) => after.validate().expect("patch produced a valid document"),
            Err(error) => refused(error),
        }
    }
});
