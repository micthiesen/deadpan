//! Shared helpers for the coverage-guided targets. A target panics only on an
//! invariant violation (an accepted value that does not round-trip, an
//! untyped refusal, a reader that admits without consuming); libFuzzer also
//! reports crashes, aborts, timeouts and memory-limit violations itself.
//! Input formats match the `deadpan-chaos` regression targets of the same name
//! so their seeds are this corpus. See docs/ADVERSARIAL.md.

use std::path::PathBuf;
use std::sync::LazyLock;

use deadpan_chaos::Outcome;
use deadpan_core::{DOCUMENT_SCHEMA_VERSION, ProjectDocument};
use deadpan_jobs::process::WorkerProtocol;

pub use deadpan_chaos::select;

/// Fails the run on an invariant violation reported by a target body.
pub fn check(outcome: Outcome) {
    if let Err(violation) = outcome {
        panic!("invariant violated: {violation}");
    }
}

/// A typed refusal must say something.
pub fn refused(error: impl std::fmt::Display) {
    let message = error.to_string();
    assert!(!message.trim().is_empty(), "untyped empty refusal");
}

/// The store's current-schema document fixtures, as the core adversarial
/// regression loads them.
pub static DOCUMENTS: LazyLock<Vec<ProjectDocument>> = LazyLock::new(|| {
    let directory =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../crates/deadpan-store/tests/fixtures");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&directory)
        .expect("store fixtures")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("current-") && name.ends_with(".json"))
        })
        .collect();
    paths.sort();
    let documents: Vec<ProjectDocument> = paths
        .iter()
        .map(|path| {
            let mut wire: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
            wire["schema_version"] = DOCUMENT_SCHEMA_VERSION.into();
            ProjectDocument::from_json(&wire.to_string()).unwrap()
        })
        .collect();
    assert!(!documents.is_empty(), "document fixtures missing");
    documents
});

/// A worker-side reader of host requests.
pub type HostReader<P> = fn(&mut &[u8]) -> Result<Option<<P as WorkerProtocol>::Request>, String>;

/// One framed worker protocol, both directions. Selector 0: host requests as
/// the worker reads them; every admitted request must re-encode and read back.
/// Selector 1: worker responses as the host reads them. Selector 2: one host
/// request that, when it is a valid initial request, classifies the
/// following worker responses against its attempt identity.
pub fn protocol<P: WorkerProtocol>(input: &[u8], read_host: HostReader<P>) {
    check(deadpan_chaos::protocol_stream(
        input, read_host,
        |writer, request| P::write_request(writer, request),
        |reader| P::read_response(reader),
        |request| P::from_request(request).map_err(|error| error.to_string()),
        |protocol, response| protocol.classify(response).map(|_| ()),
    ));
}
