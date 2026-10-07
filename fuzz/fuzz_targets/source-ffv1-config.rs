#![no_main]
//! FFV1 version 3 configuration records from Matroska CodecPrivate.
use deadpan_source::fuzzing;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    if let Err(error) = fuzzing::ffv1_configuration(input) {
        deadpan_fuzz::refused(error);
    }
});
