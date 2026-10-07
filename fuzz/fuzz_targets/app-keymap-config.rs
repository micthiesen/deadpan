#![no_main]
//! The production keymap parser, trie compilation and native action router.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    if let Err(error) = deadpan_app::fuzz_keymap(input) {
        deadpan_fuzz::refused(error);
    }
});
