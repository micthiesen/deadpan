//! Source presentation indexes and bounded generated-media conversion.
//!
//! A vetted private helper performs decode, FFV1 encoding, and independent
//! pixel/timing validation. This boundary supplies immutable bytes, bounded
//! resources, cancellation, a hard process deadline, and final content identity.
//! It does not select candidates, publish objects, or change authored state.
//! Source sessions use a separate persistent native decoder on media threads;
//! generated conversion retains its isolated-helper contract.

pub mod audio_index;
pub mod protocol;
pub mod source_index;

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod source_input;

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod audio_session;

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod source_session;

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod source_import_timing;

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod source_qualification;

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod conversion;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use conversion::{
    CanonicalBridge, CanonicalMedia, ConversionError, InputIdentity, canonicalize,
    canonicalize_bridge, sample_bridge,
};
