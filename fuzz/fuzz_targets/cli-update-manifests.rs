#![no_main]
//! Signed update envelopes (selector 0: parse, then verify against the
//! compiled keys for its kind), downloader payloads (1) and model-pack
//! payloads (2). Signatures cannot be forged here, so the payload parsers are
//! also reached directly; accepted payloads round-trip.
use deadpan_cli::youtube::updates::DownloaderManifest;
use deadpan_fuzz::{refused, select};
use deadpan_models::packs::updates::PackUpdate;
use deadpan_models::updates::SignedManifest;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    let (selector, body) = select(input);
    match selector % 3 {
        0 => match SignedManifest::parse(body) {
            Ok(envelope) => {
                if let Err(error) = envelope.verify(envelope.kind) {
                    refused(error);
                }
            }
            Err(error) => refused(error),
        },
        1 => {
            let Ok(text) = std::str::from_utf8(body) else { return };
            match DownloaderManifest::parse(text) {
                Ok(manifest) => {
                    let again = DownloaderManifest::parse(&serde_json::to_string(&manifest).unwrap())
                        .expect("accepted downloader manifest re-parses");
                    assert!(again == manifest, "downloader manifest round trip changed the value");
                }
                Err(error) => refused(error),
            }
        }
        _ => {
            let Ok(text) = std::str::from_utf8(body) else { return };
            match PackUpdate::parse(text) {
                Ok(update) => {
                    let again = PackUpdate::parse(&serde_json::to_string(&update).unwrap())
                        .expect("accepted pack update re-parses");
                    assert!(again == update, "pack update round trip changed the value");
                }
                Err(error) => refused(error),
            }
        }
    }
});
