//! Host validation of complete bridge candidate bundles.
//!
//! The worker supplies native footage and provenance. The host retains their
//! declared identities, derives both masters from one immutable native snapshot,
//! and creates immutable provenance binding the request, plan, and verified media.
//! Publication, selected-Ready state, audition, and authored acceptance are separate.

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod qualification;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use qualification::*;

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod conditioning;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use conditioning::*;

mod provider;
pub use provider::SelectedBridgeProvider;

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod bounded_json;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod provenance;
mod strict_json;
