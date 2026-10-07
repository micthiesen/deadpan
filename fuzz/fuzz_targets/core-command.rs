#![no_main]
//! Command requests applied by the core: the forward patch applies and
//! validates, and the inverse restores the exact document.
use deadpan_core::{CommandRequest, apply};
use deadpan_fuzz::{DOCUMENTS, refused};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    let request = match serde_json::from_slice::<CommandRequest>(input) {
        Ok(request) => request,
        Err(error) => return refused(error),
    };
    let document = DOCUMENTS
        .iter()
        .find(|document| {
            document.project_id() == &request.project_id
                && document.revision_id() == &request.expected_revision
        })
        .unwrap_or(&DOCUMENTS[0]);
    match apply(document, &request) {
        Ok(transaction) => {
            let after = transaction.forward.apply(document).expect("forward patch applies");
            after.validate().expect("command produced a valid document");
            let restored = transaction.inverse.apply(&after).expect("inverse patch applies");
            assert!(&restored == document, "inverse patch does not restore the exact document");
        }
        Err(error) => refused(error),
    }
});
