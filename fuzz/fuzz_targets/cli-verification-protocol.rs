#![no_main]
//! Framed worker protocol, both directions; see `deadpan_fuzz::protocol`.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    deadpan_fuzz::protocol::<deadpan_cli::encoded_render::verification::protocol::VerificationProtocol>(input, |reader| deadpan_cli::encoded_render::verification::protocol::read_host(reader));
});
