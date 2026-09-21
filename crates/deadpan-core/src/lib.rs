//! Foundational exact-time arithmetic for Deadpan.
//!
//! Project frames, mix samples, and source timestamps are different coordinates.
//! Convert timeline boundaries from their common origin instead of accumulating
//! separately rounded durations. See specification §4 for the timing contract.
//!
//! Authored documents and semantic editing transactions are pure data. Hosts own
//! persistence, identity generation, media decoding, and external jobs.

mod anchor;
mod command;
mod document;
mod exact;
pub mod legacy_v1;
mod occurrence;
mod source_index;
mod time;

pub use anchor::*;
pub use command::*;
pub use document::*;
pub use exact::ExactRatio;
pub use occurrence::*;
pub use source_index::*;

pub use time::{
    AudioSample, FrameDuration, FrameRange, FrameRate, MIX_SAMPLE_RATE, ProjectFrame,
    SourceTimeBase, SourceTimestamp, TimeError, repeat_duration,
};
