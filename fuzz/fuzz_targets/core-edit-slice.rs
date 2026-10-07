#![no_main]
//! Captured edited slices (register contents and clipboard captures).
use deadpan_core::CapturedEditSlice;
use deadpan_fuzz::refused;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    let Ok(text) = std::str::from_utf8(input) else { return };
    match CapturedEditSlice::from_json(text) {
        Ok(slice) => {
            let encoded = slice.to_json().expect("accepted slice re-encodes");
            let again = CapturedEditSlice::from_json(&encoded).expect("accepted slice round-trips");
            assert_eq!(again.to_json().unwrap(), encoded, "slice round trip changed the value");
        }
        Err(error) => refused(error),
    }
});
