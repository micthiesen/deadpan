#![no_main]
//! Project documents: typed refusal or a valid value that round-trips.
use deadpan_core::ProjectDocument;
use deadpan_fuzz::refused;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    let Ok(text) = std::str::from_utf8(input) else { return };
    match ProjectDocument::from_json(text) {
        Ok(document) => {
            document.validate().expect("accepted document validates");
            let encoded = document.to_json().expect("accepted document re-encodes");
            let again = ProjectDocument::from_json(&encoded).expect("accepted document round-trips");
            assert!(again == document, "document round trip changed the value");
        }
        Err(error) => refused(error),
    }
});
