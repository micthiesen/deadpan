#![no_main]
//! Stored measured audio indexes.
use deadpan_fuzz::refused;
use deadpan_media::audio_index::AudioIndexSnapshot;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    match AudioIndexSnapshot::from_json(input) {
        Ok(snapshot) => {
            let encoded = snapshot.to_json().expect("accepted index re-encodes");
            let again = AudioIndexSnapshot::from_json(&encoded).expect("accepted index round-trips");
            assert!(again == snapshot, "index round trip changed the value");
        }
        Err(error) => refused(error),
    }
});
