//! Independent authored sound recipes in their owner's output clock.
//!
//! This first persisted vocabulary admits the root bus only. Structural edits
//! outside ordinary root-clock ripple edits remain explicitly rejected.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    AudioEdgePolicy, AudioSample, Command, DocumentError, DocumentErrorCode, EditError,
    EditErrorCode, FrameDuration, NodeId, OccurrenceEdit, ProjectDocument, SourceAudio,
    SourceAudioMapping,
};

pub const MAX_DOCUMENT_SOUNDS: usize = 64;
pub const MIN_SOUND_GAIN_MILLIDECIBELS: i32 = -96_000;
pub const MAX_SOUND_GAIN_MILLIDECIBELS: i32 = 24_000;

/// Overflow never silently trims the source or lengthens picture time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoundOverflowPolicy {
    Reject,
}

/// A complete source recipe plus an exact audible selection. Source phase is
/// defined by `mapping`; `offset` independently shifts it on the 48 kHz clock.
/// A qualification identity is necessary evidence, not host media admission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SoundEvent {
    pub owner: NodeId,
    pub label: String,
    pub source: SourceAudio,
    pub mapping: SourceAudioMapping,
    pub offset: AudioSample,
    pub gain_millidecibels: i32,
    pub start_edge: AudioEdgePolicy,
    pub end_edge: AudioEdgePolicy,
    pub overflow: SoundOverflowPolicy,
}

pub(crate) fn validate(
    document: &ProjectDocument,
    durations: &BTreeMap<NodeId, FrameDuration>,
) -> Result<(), DocumentError> {
    if document.sounds.len() > MAX_DOCUMENT_SOUNDS {
        return Err(DocumentError::new(
            DocumentErrorCode::LimitExceeded,
            "document exceeds 64 live sound events",
        ));
    }
    let rate = document.presentation_basis.frame_rate;
    for (id, event) in &document.sounds {
        if event.owner != document.root {
            return Err(invalid(
                "sound events currently require the root Sequence owner",
            ));
        }
        crate::document::validate_label(&event.label)?;
        if !(MIN_SOUND_GAIN_MILLIDECIBELS..=MAX_SOUND_GAIN_MILLIDECIBELS)
            .contains(&event.gain_millidecibels)
        {
            return Err(invalid(
                "sound gain must be between -96000 and 24000 milli dB",
            ));
        }
        let asset = document.assets.get(&event.source.asset).ok_or_else(|| {
            DocumentError::new(
                DocumentErrorCode::MissingAsset,
                "sound source asset is absent",
            )
        })?;
        if asset.source_qualification.is_none() {
            return Err(invalid("sound events require a qualified source asset"));
        }
        if !asset
            .audio
            .is_some_and(|span| span.contains_span(event.source.span))
        {
            return Err(invalid("sound selection exceeds its qualified audio span"));
        }
        let extent = document
            .sound_routes
            .get(id)
            .map_or(durations[&event.owner], |route| route.recipe_extent);
        let natural = SourceAudioMapping::natural_rate(event.source.span, rate)?;
        if event.mapping == SourceAudioMapping::FitBeat
            || event.mapping.duration_frames(extent)? != natural.duration_frames(extent)?
        {
            return Err(invalid(
                "sound events require an explicit exact natural-rate mapping",
            ));
        }
        let interval = event
            .mapping
            .selection_frames_with_offset(extent, event.offset, rate)?;
        if interval.start.compare_integer(0).is_lt()
            || interval
                .end
                .checked_sub(interval.start)?
                .compare_integer(0)
                .is_le()
            || interval.end.compare_integer(extent.frames()).is_gt()
        {
            return Err(invalid(
                "sound selected interval with offset must fit entirely inside its owner",
            ));
        }
    }
    crate::sound_routing::validate(document, durations[&document.root])?;
    Ok(())
}

fn invalid(message: &str) -> DocumentError {
    DocumentError::new(DocumentErrorCode::SourceRangeInvalid, message)
}

/// Exhaustive classification keeps future commands from silently bypassing the
/// missing interval transforms, including edits hidden by occurrence isolation.
pub(crate) fn validate_command(
    document: &ProjectDocument,
    command: &Command,
) -> Result<(), EditError> {
    if document.sounds.is_empty()
        || preserves_sound_clocks(command)
        || matches!(
            command,
            Command::InsertTime { .. }
                | Command::SpliceSource { .. }
                | Command::SpliceSlice { .. }
                | Command::SpliceSliceAt { .. }
                | Command::ReplaceSlice { .. }
                | Command::SpliceSourceAt { .. }
                | Command::ReplaceSource { .. }
                | Command::DeleteRipple { .. }
                | Command::DeleteRange { .. }
                | Command::MoveRange { .. }
                | Command::Delete { .. }
        )
        || matches!(command, Command::Split { node, .. } if node != document.root())
    {
        return Ok(());
    }
    Err(EditError::new(
        EditErrorCode::InvalidCommand,
        "this structural edit cannot yet preserve authored sound intervals and sample phase; remove the sound events before editing time",
    ))
}

fn preserves_sound_clocks(command: &Command) -> bool {
    match command {
        Command::SetSound { .. }
        | Command::ReplaceSound { .. }
        | Command::DeleteSound { .. }
        | Command::SetSoundAllowance { .. }
        | Command::SetSourceVideoMapping { .. }
        | Command::SetSourceAudioMapping { .. }
        | Command::SetHoldAudio { .. }
        | Command::SetHoldProvider { .. }
        | Command::SetHoldPictureContext { .. }
        | Command::AcceptGeneratedHold { .. }
        | Command::RevertGeneratedHold { .. }
        | Command::Rename { .. }
        | Command::SetAudioEdge { .. }
        | Command::SetFraming { .. }
        | Command::SetAudioTreatments { .. }
        | Command::AddAsset { .. }
        | Command::SetCanvas { .. }
        | Command::AdoptPrimaryGeometry { .. }
        | Command::SetMark { .. }
        | Command::DeleteMark { .. } => true,
        Command::ImportSource {
            insertion, primary, ..
        } => insertion.is_none() && primary.is_none(),
        Command::EditOccurrence { edit, .. } => match edit {
            OccurrenceEdit::SetSourceVideoMapping { .. }
            | OccurrenceEdit::SetSourceAudioMapping { .. }
            | OccurrenceEdit::SetHoldAudio { .. }
            | OccurrenceEdit::SetHoldProvider { .. }
            | OccurrenceEdit::SetHoldPictureContext { .. }
            | OccurrenceEdit::AcceptGeneratedHold { .. }
            | OccurrenceEdit::RevertGeneratedHold
            | OccurrenceEdit::Rename { .. }
            | OccurrenceEdit::SetAudioEdge { .. }
            | OccurrenceEdit::SetAudioTreatments { .. }
            | OccurrenceEdit::SetFraming { .. } => true,
            OccurrenceEdit::Split { .. }
            | OccurrenceEdit::Insert { .. }
            | OccurrenceEdit::Delete
            | OccurrenceEdit::Group { .. }
            | OccurrenceEdit::Ungroup
            | OccurrenceEdit::WrapRepeat { .. }
            | OccurrenceEdit::SetRepeat { .. }
            | OccurrenceEdit::WrapRetime { .. }
            | OccurrenceEdit::SetRetime { .. }
            | OccurrenceEdit::InsertPlays { .. }
            | OccurrenceEdit::MovePlays { .. }
            | OccurrenceEdit::SetHoldDuration { .. }
            | OccurrenceEdit::SetPlayOverride { .. }
            | OccurrenceEdit::ClearPlayOverride { .. }
            | OccurrenceEdit::SetGapOverride { .. }
            | OccurrenceEdit::IsolateGap { .. }
            | OccurrenceEdit::ClearGapOverride { .. } => false,
        },
        Command::InsertTime { .. }
        | Command::SpliceSource { .. }
        | Command::SpliceSlice { .. }
        | Command::SpliceSliceAt { .. }
        | Command::ReplaceSlice { .. }
        | Command::SpliceSourceAt { .. }
        | Command::ReplaceSource { .. }
        | Command::DeleteRipple { .. }
        | Command::DeleteRange { .. }
        | Command::MoveRange { .. }
        | Command::Split { .. }
        | Command::Insert { .. }
        | Command::Delete { .. }
        | Command::Move { .. }
        | Command::Group { .. }
        | Command::Ungroup { .. }
        | Command::WrapRepeat { .. }
        | Command::SetRepeat { .. }
        | Command::WrapRetime { .. }
        | Command::SetRetime { .. }
        | Command::InsertPlays { .. }
        | Command::MovePlays { .. }
        | Command::SetHoldDuration { .. }
        | Command::SetPlayOverride { .. }
        | Command::ClearPlayOverride { .. }
        | Command::SetGapOverride { .. }
        | Command::IsolateGap { .. }
        | Command::ClearGapOverride { .. } => false,
    }
}
