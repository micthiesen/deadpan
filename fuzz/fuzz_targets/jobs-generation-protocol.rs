#![no_main]
//! Framed worker protocol, both directions; see `deadpan_fuzz::protocol`.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    deadpan_fuzz::protocol::<deadpan_jobs::supervisor::GenerationProtocol>(input, |reader| deadpan_jobs::read_host_message(reader).map_err(|error| error.to_string()));
});
