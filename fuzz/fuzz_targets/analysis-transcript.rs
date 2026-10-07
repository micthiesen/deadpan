#![no_main]
//! Recognizer segments (selector 0) and stored transcripts (selector 1).
use deadpan_analysis::{AnalysedAudio, RawSegment, Transcript};
use deadpan_fuzz::{refused, select};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    let (selector, body) = select(input);
    let audio = AnalysedAudio { origin: -1_024, sample_rate: 48_000, duration_cs: 740 };
    let transcript = if selector % 2 == 0 {
        match serde_json::from_slice::<Vec<RawSegment>>(body) {
            Ok(segments) => match Transcript::from_segments(audio, &segments) {
                Ok(transcript) => transcript,
                Err(error) => return refused(error),
            },
            Err(error) => return refused(error),
        }
    } else {
        match serde_json::from_slice::<Transcript>(body) {
            Ok(transcript) => transcript,
            Err(error) => return refused(error),
        }
    };
    let encoded = serde_json::to_vec(&transcript).expect("transcript re-encodes");
    let again: Transcript = serde_json::from_slice(&encoded).expect("accepted transcript revalidates");
    assert!(again == transcript, "transcript round trip changed the value");
});
