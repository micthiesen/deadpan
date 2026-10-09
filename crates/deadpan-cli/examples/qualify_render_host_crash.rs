//! Kill a real RenderWorkflow host while its encoder or verifier has emitted
//! intermediate progress. Requires the explicit qualification feature.
//!
//! Usage: qualify_render_host_crash SCRATCH_PACKAGE NEW_EVIDENCE_DIRECTORY
//! The caller supplies a current-format package under /tmp with at least eight
//! frames. Operational render records are added; authored history is preserved.
//! Only the first 120 frames are rendered. Evidence and failures are retained.
//! Each case retains a byte-verified executable copy and an adjacent configuration
//! binding only its initial attempt. Helper runtime arguments and environment
//! overrides remain empty, including during the checkpoint retry.

#[cfg(target_os = "macos")]
#[path = "qualify_render_host_crash/harness.rs"]
mod harness;

#[cfg(target_os = "macos")]
fn main() -> std::process::ExitCode {
    match harness::run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("render host-crash qualification: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn main() -> std::process::ExitCode {
    eprintln!("render host-crash qualification requires macOS");
    std::process::ExitCode::FAILURE
}
