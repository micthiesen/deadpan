//! Foundational exact-time arithmetic for Deadpan.
//!
//! Project frames, mix samples, and source timestamps are different coordinates.
//! Convert timeline boundaries from their common origin instead of accumulating
//! separately rounded durations. See specification §4 for the timing contract.
//!
//! This crate currently implements timing primitives, not the document model or
//! editing operations.

mod time;

pub use time::{
    AudioSample, FrameDuration, FrameRange, FrameRate, MIX_SAMPLE_RATE, ProjectFrame,
    SourceTimeBase, SourceTimestamp, TimeError, repeat_duration,
};
