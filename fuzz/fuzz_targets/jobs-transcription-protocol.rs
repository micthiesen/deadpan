#![no_main]
//! Framed worker protocol, both directions; see `deadpan_fuzz::protocol`.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    deadpan_fuzz::protocol::<deadpan_jobs::transcription::TranscriptionProtocol>(input, |reader| deadpan_jobs::transcription::read_host(reader));
});
