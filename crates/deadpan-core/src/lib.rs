//! Foundational exact-time arithmetic for Deadpan.
//!
//! Project frames, mix samples, and source timestamps are different coordinates.
//! Convert timeline boundaries from their common origin instead of accumulating
//! separately rounded durations. See specification §4 for the timing contract.
//!
//! Authored documents and semantic editing transactions are pure data. Hosts own
//! persistence, identity generation, media decoding, and external jobs.

mod anchor;
mod audio_edges;
mod audio_mapping;
mod audio_reference;
mod basis;
mod command;
mod document;
mod exact;
mod generated;
mod legacy_asset;
mod legacy_mark;
mod legacy_mark_v13;
mod legacy_source_mapping;
pub mod legacy_v1;
pub mod legacy_v10;
pub mod legacy_v11;
pub mod legacy_v12;
pub mod legacy_v13;
pub mod legacy_v2;
pub mod legacy_v3;
pub mod legacy_v4;
pub mod legacy_v5;
pub mod legacy_v6;
pub mod legacy_v7;
pub mod legacy_v8;
pub mod legacy_v9;
mod marks;
mod occurrence;
mod occurrence_edit;
mod repeat_layout;
mod source_index;
mod source_mapping;
mod split;
mod time;
mod video_mapping;

pub use anchor::*;
pub use audio_edges::*;
pub use audio_mapping::SourceAudioMapping;
pub use audio_reference::*;
pub use basis::*;
pub use command::*;
pub use document::*;
pub use exact::ExactRatio;
pub use generated::*;
pub use marks::*;
pub use occurrence::*;
pub use occurrence_edit::{OccurrenceEdit, OccurrenceIdentities};
pub use repeat_layout::*;
pub use source_index::*;
pub use split::SplitIdentities;
pub use video_mapping::SourceVideoMapping;

pub use time::{
    AudioSample, FrameDuration, FrameRange, FrameRate, MIX_SAMPLE_RATE, ProjectFrame,
    SourceTimeBase, SourceTimestamp, TimeError, repeat_duration,
};
