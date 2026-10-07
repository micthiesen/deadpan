#![no_main]
//! Model pack manifests (receipts and catalogs on disk).
use deadpan_fuzz::refused;
use deadpan_models::packs::PackManifest;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    match serde_json::from_slice::<PackManifest>(input) {
        Ok(manifest) => {
            if let Err(error) = manifest.validate() {
                refused(error);
            }
        }
        Err(error) => refused(error),
    }
});
