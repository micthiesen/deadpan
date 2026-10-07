#![no_main]
//! Stored measured picture indexes.
use deadpan_fuzz::refused;
use deadpan_media::source_index::SourceIndexSnapshot;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    match SourceIndexSnapshot::from_json(input) {
        Ok(snapshot) => {
            let encoded = snapshot.to_json().expect("accepted index re-encodes");
            let again = SourceIndexSnapshot::from_json(&encoded).expect("accepted index round-trips");
            assert!(again == snapshot, "index round trip changed the value");
        }
        Err(error) => refused(error),
    }
});
