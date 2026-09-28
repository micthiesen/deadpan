//! Pure root sound placement from the workspace's measured catalog evidence.
//! Storage and playback still own transactional and live source admission.

use deadpan_core::{
    AssetId, AudioEdgePolicy, AudioSample, ExactFrameRange, ExactRatio, FrameDuration, SoundEvent,
    SoundId, SoundOverflowPolicy, SourceAudio, SourceAudioMapping,
};

use super::Workspace;

/// Place the complete measured audio-only catalog span at an exact 48 kHz onset.
/// This performs no filesystem work and never rounds duration to picture frames.
pub fn placement(
    workspace: &Workspace,
    asset: &AssetId,
    at: AudioSample,
) -> Result<SoundEvent, String> {
    let source = workspace
        .sources
        .get(asset)
        .ok_or("The catalog sound is no longer registered.")?;
    let metadata = workspace
        .document
        .assets()
        .get(asset)
        .ok_or("The catalog sound is no longer registered.")?;
    if source.receipt.snapshot().video().is_some()
        || metadata.video.is_some()
        || metadata.still_image
        || source.receipt.snapshot().audio().is_none()
    {
        return Err("Sound placement requires an audio-only catalog source.".into());
    }
    if metadata.source_qualification.as_ref() != Some(source.receipt.id()) {
        return Err("The catalog sound qualification has changed.".into());
    }
    let rate = workspace.document.presentation_basis().frame_rate;
    let span = source
        .receipt
        .snapshot()
        .derive_timing(rate)
        .map_err(|error| error.to_string())?
        .audio
        .ok_or("The catalog sound has no measured audio span.")?
        .span;
    if metadata.audio != Some(span) {
        return Err("The catalog sound differs from its measured source qualification.".into());
    }
    let event = SoundEvent {
        owner: workspace.document.root().clone(),
        label: source.label.clone(),
        source: SourceAudio {
            asset: asset.clone(),
            span,
        },
        mapping: SourceAudioMapping::natural_rate(span, rate).map_err(|error| error.to_string())?,
        offset: at,
        gain_millidecibels: 0,
        start_edge: AudioEdgePolicy::Automatic,
        end_edge: AudioEdgePolicy::Automatic,
        overflow: SoundOverflowPolicy::Reject,
    };
    validate_placement(workspace, &event)?;
    Ok(event)
}

pub(super) fn moved(
    workspace: &Workspace,
    id: &SoundId,
    at: AudioSample,
) -> Result<SoundEvent, String> {
    let mut event = movable(workspace, id)?;
    // The target is the selected onset, even for an unrouted event authored
    // through another client with a fractional placement or selected source.
    // Translate the complete recipe and selection together, retaining phase.
    event.mapping = match event.mapping {
        SourceAudioMapping::FitBeat => {
            return Err("This sound has no natural-rate placement.".into());
        }
        SourceAudioMapping::Duration { frames } => SourceAudioMapping::Duration { frames },
        SourceAudioMapping::Placement { frames, .. } => SourceAudioMapping::Placement {
            start: ExactRatio::ZERO,
            frames,
        },
        SourceAudioMapping::SelectedPlacement {
            start,
            frames,
            selection,
        } => SourceAudioMapping::SelectedPlacement {
            start: start
                .checked_sub(selection.start)
                .map_err(|error| error.to_string())?,
            frames,
            selection: ExactFrameRange {
                start: ExactRatio::ZERO,
                end: selection
                    .end
                    .checked_sub(selection.start)
                    .map_err(|error| error.to_string())?,
            },
        },
    };
    event.offset = at;
    validate_placement(workspace, &event)?;
    Ok(event)
}

/// Shift an unrouted recipe in its exact project-frame clock. The independent
/// sample offset and source selection phase survive repeated and inverse nudges.
pub fn nudge(workspace: &Workspace, id: &SoundId, frames: i64) -> Result<SoundEvent, String> {
    let mut event = movable(workspace, id)?;
    let delta = ExactRatio::integer(frames);
    let shift = |value: ExactRatio| value.checked_add(delta).map_err(|error| error.to_string());
    event.mapping = match event.mapping {
        SourceAudioMapping::FitBeat => {
            return Err("This sound has no natural-rate placement.".into());
        }
        SourceAudioMapping::Duration { frames } => SourceAudioMapping::Placement {
            start: delta,
            frames,
        },
        SourceAudioMapping::Placement { start, frames } => SourceAudioMapping::Placement {
            start: shift(start)?,
            frames,
        },
        SourceAudioMapping::SelectedPlacement {
            start,
            frames,
            selection,
        } => SourceAudioMapping::SelectedPlacement {
            start: shift(start)?,
            frames,
            selection: ExactFrameRange {
                start: shift(selection.start)?,
                end: shift(selection.end)?,
            },
        },
    };
    validate_placement(workspace, &event)?;
    Ok(event)
}

fn movable(workspace: &Workspace, id: &SoundId) -> Result<SoundEvent, String> {
    let event = workspace
        .document
        .sounds()
        .get(id)
        .cloned()
        .ok_or("The selected sound no longer exists.")?;
    if workspace.document.sound_routes().contains_key(id) {
        return Err("This sound has retained edit cuts. Moving routed sounds is not supported yet; its routing was preserved.".into());
    }
    Ok(event)
}

fn validate_placement(workspace: &Workspace, event: &SoundEvent) -> Result<(), String> {
    if event.owner != *workspace.document.root() {
        return Err("Native sound placement currently requires the project root owner.".into());
    }
    let selected = event
        .mapping
        .selection_frames_with_offset(
            FrameDuration::ZERO,
            event.offset,
            workspace.document.presentation_basis().frame_rate,
        )
        .map_err(|error| error.to_string())?;
    if selected.start.compare_integer(0).is_lt() {
        return Err("Sound onset must be inside Your edit.".into());
    }
    if selected
        .end
        .compare_integer(workspace.plan.duration().frames())
        .is_gt()
    {
        return Err(
            "The complete sound extends past the end of Your edit; choose an earlier onset.".into(),
        );
    }
    Ok(())
}
