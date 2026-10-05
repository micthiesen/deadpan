//! Revision-bound limited audition. Media preparation and device control
//! have separate owners; no decoding or authored-state mutation runs on the UI.

mod controller;
mod preparation;
mod sources;
mod target;
mod waveform;

pub use controller::{Diagnostics, Engine, Phase, RequestError, StopHandle, Update};
pub use sources::{ContentIdentity, Snapshot, SnapshotError, SourceEntry};
pub use target::{AudioRange, Original, Sound, Target, Window};
pub use waveform::{
    EditWaveformRequest, EditWaveformUpdate, WaveformRequest, WaveformRequestError, WaveformStatus,
    WaveformTicket, WaveformUpdate,
};

#[cfg(test)]
mod tests;
