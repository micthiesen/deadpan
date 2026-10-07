#![no_main]
//! Closed MP4/Matroska video container grammar checked before FFmpeg, plus
//! the public MP4 inspection that must agree with admission.
use std::io::Write as _;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use deadpan_source::{DecodeControl, DecodeLimits, fuzzing, inspect_mp4};
use libfuzzer_sys::fuzz_target;

static CANCELLED: AtomicBool = AtomicBool::new(false);

fuzz_target!(|input: &[u8]| {
    if input.is_empty() {
        return;
    }
    let mut file = tempfile::tempfile().expect("snapshot");
    file.write_all(input).expect("write snapshot");
    let control = || DecodeControl { timeout: Duration::from_secs(10), cancelled: &CANCELLED };
    let admitted = fuzzing::admit(&file, false, control());
    if let Err(error) = &admitted {
        deadpan_fuzz::refused(error);
    }
    if input.get(4..8) == Some(b"ftyp") {
        let inspected = inspect_mp4(&file, DecodeLimits::default(), control());
        if admitted.is_ok() {
            if let Err(error) = inspected {
                panic!("admitted MP4 failed inspection: {error}");
            }
        }
    }
});
