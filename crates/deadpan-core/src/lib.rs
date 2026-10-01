//! Foundational exact-time arithmetic for Deadpan.
//!
//! Project frames, mix samples, and source timestamps are different coordinates.
//! Convert timeline boundaries from their common origin instead of accumulating
//! separately rounded durations. See specification §4 for the timing contract.
//!
//! Authored documents and semantic editing transactions are pure data. Hosts own
//! persistence, identity generation, media decoding, and external jobs.

mod anchor;
mod audio_binding;
mod audio_binding_lifecycle;
pub use audio_binding_lifecycle::capture_unbound_audio_bindings;
mod audio_context;
mod audio_edges;
mod audio_gain;
mod audio_lineage;
mod audio_mapping;
mod audio_reference;
mod basis;
mod command;
mod document;
mod edit_slice;
mod exact;
mod framing;
mod gap_override;
mod generated;
mod insert_time;
mod legacy_asset;
mod legacy_audio_binding_v20;
mod legacy_audio_binding_v21;
mod legacy_audio_binding_v22;
mod legacy_audio_binding_v35;
mod legacy_audio_binding_v36;
mod legacy_audio_mapping_v19;
mod legacy_audio_mapping_v35;
mod legacy_framing_v37;
mod legacy_hold_v18;
mod legacy_mark;
mod legacy_mark_v13;
mod legacy_sound_routes;
mod legacy_source_mapping;
pub mod legacy_v1;
pub mod legacy_v10;
pub mod legacy_v11;
pub mod legacy_v12;
pub mod legacy_v13;
pub mod legacy_v14;
pub mod legacy_v15;
pub mod legacy_v16;
pub mod legacy_v17;
pub mod legacy_v18;
pub mod legacy_v19;
pub mod legacy_v2;
pub mod legacy_v20;
pub mod legacy_v21;
pub mod legacy_v22;
pub mod legacy_v23;
pub mod legacy_v24;
pub mod legacy_v25;
pub mod legacy_v26;
pub mod legacy_v27;
pub mod legacy_v28;
pub mod legacy_v29;
pub mod legacy_v3;
pub mod legacy_v30;
pub mod legacy_v31;
pub mod legacy_v32;
pub mod legacy_v4;
pub mod legacy_v5;
pub mod legacy_v6;
pub mod legacy_v7;
pub mod legacy_v8;
pub mod legacy_v9;
mod legacy_video_mapping_v34;
mod marks;
mod move_range;
mod occurrence;
mod occurrence_edit;
mod picture_context;
mod repeat_layout;
mod sound_allowance;
mod sound_events;
mod sound_route;
mod sound_routing;
mod source_edit;
mod source_edit_window;
mod source_index;
mod source_mapping;
mod source_roll;
mod source_slip;
mod source_trim;
mod source_trim_geometry;
mod split;
mod time;
mod video_mapping;

pub use anchor::*;
pub use audio_binding::*;
pub use audio_context::*;
pub use audio_edges::*;
pub use audio_gain::*;
pub use audio_lineage::AudioLineageId;
pub use audio_mapping::SourceAudioMapping;
pub use audio_reference::*;
pub use basis::*;
pub use command::*;
pub use document::*;
pub use edit_slice::{
    CapturedEditSlice, SliceCaptureSelection, SliceIdentityRequirements, SlicePasteIdentities,
};
pub use exact::ExactRatio;
pub use framing::*;
pub use generated::*;
pub use insert_time::{
    InsertTimeSplit, InsertTimeTarget, SequenceRangeEdit, SourceReplacement, SourceSpliceInterior,
};
pub use marks::*;
pub use move_range::{MoveRangeDestination, SequenceRangeMove};
pub use occurrence::*;
pub use occurrence_edit::{OccurrenceEdit, OccurrenceIdentities};
pub use picture_context::*;
pub use repeat_layout::*;
pub use sound_allowance::*;
pub use sound_events::*;
pub use sound_route::*;
pub use sound_routing::*;
pub use source_edit_window::SourceEditWindow;
pub use source_index::*;
pub use source_roll::{
    SourceRollLimit, SourceRollResolution, SourceRollSide, SourceRollSideResolution,
};
pub use source_slip::{SourceSlipClamp, SourceSlipResolution};
pub use source_trim::{
    SourceTrimClamp, SourceTrimEdge, SourceTrimLimit, SourceTrimMode, SourceTrimResolution,
};
pub use source_trim_geometry::*;
pub use split::SplitIdentities;
pub use video_mapping::SourceVideoMapping;

pub use time::{
    AudioSample, FrameDuration, FrameRange, FrameRate, MIX_SAMPLE_RATE, ProjectFrame,
    SourceTimeBase, SourceTimestamp, TimeError, repeat_duration,
};
