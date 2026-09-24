//! Revision-bound pre-master audition. Media preparation and device control
//! have separate owners; no decoding or authored-state mutation runs on the UI.

mod controller;
mod preparation;
mod sources;

pub use controller::{Engine, Phase, RequestError, StopHandle, Update};
pub use sources::{Snapshot, SourceEntry};

#[cfg(test)]
mod tests;
