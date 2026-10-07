#![no_main]
//! Frozen audio contexts: refusal or a value that round-trips.
use deadpan_core::FrozenAudioContext;
use deadpan_fuzz::refused;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    let Ok(text) = std::str::from_utf8(input) else { return };
    match FrozenAudioContext::from_json(text) {
        Ok(context) => {
            let encoded = context.to_json().expect("accepted context re-encodes");
            let again = FrozenAudioContext::from_json(&encoded).expect("accepted context round-trips");
            assert!(again == context, "context round trip changed the value");
        }
        Err(error) => refused(error),
    }
});
