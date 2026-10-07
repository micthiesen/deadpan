#![no_main]
//! Framed worker protocol, both directions; see `deadpan_fuzz::protocol`.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    deadpan_fuzz::protocol::<deadpan_jobs::faces::FaceProtocol>(input, |reader| deadpan_jobs::faces::read_host(reader));
});
