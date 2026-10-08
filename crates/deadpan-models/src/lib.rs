//! Host validation of complete bridge and extension candidate bundles.
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
mod extension_binding;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use extension_binding::{ExtensionGenerationBinding, SelectedExtensionProvider};
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod extension_qualification;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use extension_qualification::{
    ExtensionQualification, QualifiedExtensionBundle, qualify_extension,
};
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod stored_extension;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use stored_extension::{StoredExtensionProvenance, ValidatedExtensionEvidence};

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod quality;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod quality_input;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use quality::{BridgeQualityReport, FrameObservation, MotionObservation, QualityThresholds};

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod endpoints;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use endpoints::{BridgeEndpointReport, EndpointObservation, EndpointThresholds};

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod extension_motion;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use extension_motion::ExtensionMotionReport;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod extension_endpoints;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use extension_endpoints::{ExtensionEndpointJoin, ExtensionEndpointReport, ExtensionJoinRole};
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod extension_pixels;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use extension_pixels::{ExtensionPixelReport, inspect_extension_pixels};

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod extension_geometry;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use extension_geometry::{ExtensionGeometryChecks, inspect_extension_geometry};

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod geometry;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod landmark_inspection;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use geometry::BridgeGeometryReport;

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod region;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use region::BridgeRegionReport;

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod stored_bridge;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use stored_bridge::{AcceptedBridgeEvidence, StoredBridgeProvenance};

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod bridge_color;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use bridge_color::*;

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod conditioning;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use conditioning::*;

pub mod packs;
mod provider;
pub mod updates;
pub use provider::SelectedBridgeProvider;

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod bounded_json;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod provenance;
mod strict_json;
