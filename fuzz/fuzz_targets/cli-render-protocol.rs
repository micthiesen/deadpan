#![no_main]
//! Framed worker protocol, both directions; see `deadpan_fuzz::protocol`.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    deadpan_fuzz::protocol::<deadpan_cli::render_worker::protocol::RenderProtocol>(input, |reader| deadpan_cli::render_worker::protocol::read_host_message(reader));
});
