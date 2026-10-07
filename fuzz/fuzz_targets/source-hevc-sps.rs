#![no_main]
//! HEVC SPS geometry parser: cropped size never exceeds coded size.
use deadpan_source::fuzzing;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    match fuzzing::hevc_sps(input) {
        Ok((coded, cropped)) => {
            assert!(cropped[0] <= coded[0] && cropped[1] <= coded[1], "cropped exceeds coded");
        }
        Err(error) => deadpan_fuzz::refused(error),
    }
});
