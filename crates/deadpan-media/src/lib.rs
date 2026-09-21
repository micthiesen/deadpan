//! Generated media conversion without linking codecs into the application.
//!
//! A vetted private helper performs decode, FFV1 encoding, and independent
//! pixel/timing validation. This boundary supplies immutable bytes, bounded
//! resources, cancellation, a hard process deadline, and final content identity.
//! It does not select candidates, publish objects, or change authored state.

pub mod protocol;

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod conversion;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use conversion::{
    CanonicalBridge, CanonicalMedia, ConversionError, InputIdentity, canonicalize,
    canonicalize_bridge, sample_bridge,
};
