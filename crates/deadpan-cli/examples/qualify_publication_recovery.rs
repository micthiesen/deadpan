//! Real process-crash qualification using an existing retained render checkpoint.
//! Usage: qualify_publication_recovery PACKAGE JOB_ID ENCODING_ATTEMPT WORKER
//!        NEW_OUTPUT_DIRECTORY
//! The caller supplies a fresh schema-40 package clone under /tmp. This program
//! migrates it and creates operational verification/publication records. It
//! never encodes another movie. All publication crash points are between real
//! host API calls, not inside rename, fsync, SQLite, or a simulated power loss.

#[cfg(target_os = "macos")]
#[path = "qualify_publication_recovery/harness.rs"]
mod harness;

#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    harness::run()
}

#[cfg(not(target_os = "macos"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    Err("publication crash qualification requires macOS APFS".into())
}
