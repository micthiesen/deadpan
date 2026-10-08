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
mod audio_binding_patch;
pub use audio_binding_lifecycle::capture_unbound_audio_bindings;
#[doc(hidden)]
pub use audio_binding_lifecycle::with_reference_timing_representation;
mod audio_context;
mod audio_edges;
mod audio_gain;
mod audio_lineage;
mod audio_mapping;
mod audio_reference;
mod basis;
mod beat_attachments;
mod boundary_replacement;
mod caption;
mod command;
mod command_work;
#[cfg(any(test, feature = "test-support"))]
pub use command_work::{
    binding_wire_check_for_tests, diff_for_tests, provisional_captures_for_tests,
    with_reference_command_work,
};
mod compound;
mod cutaway;
mod document;
mod duplicate;
mod edit_slice;
mod exact;
mod explode;
mod framing;
mod gap_override;
mod generated;
mod group_selection;
mod hdr;
mod id_hash;
mod insert_time;
#[cfg(test)]
mod legacy_audio_binding_v20;
mod legacy_audio_binding_v21;
#[cfg(test)]
mod legacy_audio_binding_v22;
mod legacy_audio_binding_v35;
#[cfg(test)]
mod legacy_audio_binding_v36;
mod legacy_audio_mapping_v19;
mod legacy_audio_mapping_v35;
mod marks;
mod move_range;
mod occurrence;
mod occurrence_edit;
mod output_color;
mod picture_context;
mod register;
mod repeat_escalation;
mod repeat_layout;
mod repeat_selection;
mod scoped_edit;
mod semantic;
mod sound_allowance;
mod sound_clock;
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
mod source_trim_edit;
mod source_trim_geometry;
mod split;
mod target;
mod time;
mod video_mapping;

pub use anchor::*;
pub use audio_binding::*;
pub use audio_binding_patch::{AudioBindingPatch, AudioTimingChange};
pub use audio_context::*;
pub use audio_edges::*;
pub use audio_gain::*;
pub use audio_lineage::AudioLineageId;
pub use audio_mapping::SourceAudioMapping;
pub use audio_reference::*;
pub use basis::*;
pub use boundary_replacement::{BoundaryReplacement, BoundaryReplacementEdit};
pub use caption::*;
pub use command::*;
pub use compound::*;
pub use cutaway::*;
pub use document::*;
pub use duplicate::DuplicateRequirements;
pub use edit_slice::{
    CapturedEditSlice, SequenceChildrenPlan, SliceAttachments, SliceCaptureSelection,
    SliceIdentityRequirements, SlicePasteIdentities,
};
pub use exact::ExactRatio;
pub use explode::ExplodeRequirements;
pub use framing::*;
pub use generated::*;
pub use group_selection::{GroupSelectionIdentities, GroupSelectionPlan, validate_group_label};
pub use hdr::*;
pub use insert_time::{
    InsertTimeSplit, InsertTimeTarget, SequenceRangeEdit, SourceReplacement, SourceSpliceInterior,
};
pub use marks::*;
pub use move_range::{MoveRangeDestination, SequenceRangeMove};
pub use occurrence::*;
pub use occurrence_edit::{OccurrenceEdit, OccurrenceIdentities};
pub use output_color::*;
pub use picture_context::*;
pub use register::{RegisterName, RegisterValue};
pub use repeat_escalation::*;
pub use repeat_layout::*;
pub use repeat_selection::{RepeatGapHold, RepeatSelectionIdentities, RepeatSelectionPlan};
pub use scoped_edit::{
    MAX_SCOPED_TARGETS, PreparedScopedEdit, RepeatEditBranch, RepeatEditStep,
    ScopedEditRequirements, ScopedIsolationRecord, ScopedIsolationStep, ScopedNodeEdit,
    ScopedNodeTarget, ScopedTargetEdit, ValidatedScopedIsolation, derive_scoped_isolation,
    prepare_scoped_edit,
};
pub use semantic::*;
pub use sound_allowance::*;
pub use sound_clock::{
    MAX_SOUND_CLOCK_BYTES, MAX_SOUND_CLOCKS, SoundClockJournal, SoundClockReference,
    SoundClockRepeatMap, SoundClockRepeatStep,
};
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
pub use source_trim_edit::{
    SourceTrimCapture, SourceTrimEditResolution, SourceTrimEmptyMove, SourceTrimFinalOwner,
    SourceTrimReanchorGroup, SourceTrimResources, SourceTrimResultIdentities,
    SourceTrimRightDisposition, SourceTrimSplit,
};
pub use source_trim_geometry::*;
pub use split::SplitIdentities;
pub use target::*;
pub use video_mapping::SourceVideoMapping;

pub use time::{
    AudioSample, FrameDuration, FrameRange, FrameRate, MIX_SAMPLE_RATE, ProjectFrame,
    SourceTimeBase, SourceTimestamp, TimeError, repeat_duration,
};
