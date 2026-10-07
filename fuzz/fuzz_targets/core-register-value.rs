#![no_main]
//! Register bank rows: Original moments, edited slices and Macro programs.
//! Accepted values validate (programs) and round-trip to an equal value.
use deadpan_core::RegisterValue;
use deadpan_fuzz::refused;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    match serde_json::from_slice::<RegisterValue>(input) {
        Ok(value) => {
            if let RegisterValue::Macro { program } = &value {
                program.validate().expect("accepted program validates");
            }
            let encoded = serde_json::to_vec(&value).expect("accepted register re-encodes");
            let again: RegisterValue =
                serde_json::from_slice(&encoded).expect("accepted register round-trips");
            assert!(again == value, "register round trip changed the value");
        }
        Err(error) => refused(error),
    }
});
