//! Revision-bound limited audition. Media preparation and device control
//! have separate owners; no decoding or authored-state mutation runs on the UI.

mod controller;
mod preparation;
mod sources;
mod target;
mod warming;
mod waveform;
pub use warming::{WARM_HORIZON_FRAMES, WarmRequestError, WarmStatus, WarmTicket, WarmUpdate};

pub use controller::{Diagnostics, Engine, Phase, Preparation, RequestError, StopHandle, Update};
pub use sources::{ContentIdentity, Snapshot, SnapshotError, SourceEntry};
pub use target::{AudioRange, Original, Sound, Target, Window};
pub use waveform::{
    EditWaveformRequest, EditWaveformUpdate, WaveformRequest, WaveformRequestError, WaveformStatus,
    WaveformTicket, WaveformUpdate,
};

#[cfg(test)]
mod tests;
