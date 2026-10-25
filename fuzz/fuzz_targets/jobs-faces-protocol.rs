#![no_main]
//! Framed worker protocol, both directions; see `deadpan_fuzz::protocol`.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    deadpan_fuzz::protocol::<deadpan_jobs::picture_proposals::PictureProposalProtocol>(
        input,
        |reader| deadpan_jobs::picture_proposals::read_host(reader),
    );
});
