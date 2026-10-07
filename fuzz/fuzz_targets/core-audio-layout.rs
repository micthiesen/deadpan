#![no_main]
//! Frozen audio timing layouts: refusal or a value that round-trips.
use deadpan_core::FrozenAudioLayout;
use deadpan_fuzz::refused;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    let Ok(text) = std::str::from_utf8(input) else { return };
    match FrozenAudioLayout::from_json(text) {
        Ok(layout) => {
            let encoded = layout.to_json().expect("accepted layout re-encodes");
            let again = FrozenAudioLayout::from_json(&encoded).expect("accepted layout round-trips");
            assert_eq!(again.to_json().unwrap(), encoded, "layout round trip changed the value");
        }
        Err(error) => refused(error),
    }
});
