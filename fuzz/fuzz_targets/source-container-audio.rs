#![no_main]
//! Closed MP4/Matroska/WAVE first-audio container grammar before FFmpeg.
use std::io::Write as _;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use deadpan_source::{DecodeControl, fuzzing};
use libfuzzer_sys::fuzz_target;

static CANCELLED: AtomicBool = AtomicBool::new(false);

fuzz_target!(|input: &[u8]| {
    if input.is_empty() {
        return;
    }
    let mut file = tempfile::tempfile().expect("snapshot");
    file.write_all(input).expect("write snapshot");
    let control = DecodeControl { timeout: Duration::from_secs(10), cancelled: &CANCELLED };
    if let Err(error) = fuzzing::admit(&file, true, control) {
        deadpan_fuzz::refused(error);
    }
});
