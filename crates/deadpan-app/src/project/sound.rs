//! Pure root sound placement from the workspace's measured catalog evidence.
//! Storage and playback still own transactional and live source admission.

use deadpan_core::{
    AssetId, AudioEdgePolicy, AudioSample, ExactFrameRange, ExactRatio, FrameDuration,
    ProjectFrame, SoundEvent, SoundHoldIssuer, SoundId, SoundOverflowPolicy, SourceAudio,
    SourceAudioMapping,
};

use super::Workspace;

/// One exact current root occurrence, resolved with bounded indexed queries.
/// Retained selection support is independent of the Hold's permission policy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PauseTarget {
    pub at: ProjectFrame,
    pub issuer: SoundHoldIssuer,
    pub label: String,
    pub allowed: bool,
    pub selected_support: bool,
}

impl PauseTarget {
    pub fn validate_change(&self, allowed: bool) -> Result<(), String> {
        if allowed && !self.selected_support {
            return Err("This sound has no retained selection in this pause within the Edit frame. An allowance cannot fill a timing gap.".into());
        }
        if self.allowed == allowed {
            return Err(if allowed {
                "This sound is already allowed in this pause."
            } else {
                "This sound is already silenced by this pause."
            }
            .into());
        }
        Ok(())
    }
}

pub fn pause_target(
    workspace: &Workspace,
    id: &SoundId,
    at: ProjectFrame,
) -> Result<PauseTarget, String> {
    if at.0 < 0 || at.0 >= workspace.plan.duration().frames() {
        return Err("Place the Edit cursor inside a silent pause first.".into());
    }
    let sample = workspace
        .document
        .presentation_basis()
        .frame_rate
        .audio_boundary(at)
        .map_err(|error| error.to_string())?;
    let next = workspace
        .document
        .presentation_basis()
        .frame_rate
        .audio_boundary(ProjectFrame(at.0 + 1))
        .map_err(|error| error.to_string())?;
    if next <= sample {
        return Err("The Edit cursor frame has no allocated audio sample.".into());
    }
    let query = workspace
        .plan
        .audio_hold_policy(
            sample..next,
            deadpan_plan::AudioQueryLimits {
                maximum_spans: 8,
                maximum_work: 4096,
            },
        )
        .map_err(|error| {
            format!(
                "The pause target could not be resolved within the inspector query limit: {error}"
            )
        })?;
    let Some(rule) = query.rules.first() else {
        return Err("No single silent pause is allocated in the Edit frame.".into());
    };
    let issuer = rule
        .issuer
        .sound_issuer()
        .ok_or("This pause has no concrete root occurrence to allow.")?;
    for rule in &query.rules[1..] {
        if rule.issuer.sound_issuer().as_ref() != Some(&issuer) {
            return Err("More than one silent pause occurs in this Edit frame. Choose a frame with one identified pause.".into());
        }
    }
    let instance = issuer.instance();
    let name = workspace
        .document
        .nodes()
        .get(&instance.node)
        .map(|node| node.label.as_str())
        .unwrap_or("Pause");
    // The current frame and readable play positions explain the scope. The
    // complete stable issuer, never this display label, chooses the target.
    let mut label = name.to_owned();
    for play in &instance.repeats {
        let owner = workspace
            .document
            .nodes()
            .get(&play.node)
            .ok_or("The pause's enclosing Repeat is unavailable.")?;
        let deadpan_core::NodeKind::Repeat { iterations, .. } = &owner.kind else {
            return Err("The pause's enclosing Repeat changed.".into());
        };
        let position = iterations
            .position(&play.iteration)
            .ok_or("The pause's enclosing play is unavailable.")?;
        label.push_str(&format!(
            " · {}: play {} of {}",
            owner.label,
            position + 1,
            iterations.len()
        ));
    }
    if let SoundHoldIssuer::RepeatGap { gap_after, .. } = &issuer {
        let deadpan_core::NodeKind::Repeat { iterations, .. } =
            &workspace.document.nodes()[&instance.node].kind
        else {
            return Err("The pause's Repeat changed.".into());
        };
        let position = iterations
            .position(gap_after)
            .ok_or("The pause's preceding play is unavailable.")?;
        label.push_str(&format!(
            " · pause after play {} of {}",
            position + 1,
            iterations.len()
        ));
    }
    let allowed = workspace
        .document
        .sound_allowances()
        .get(id)
        .is_some_and(|allowances| allowances.contains(&issuer));
    let sound = workspace
        .plan
        .root_sound(id)
        .map_err(|error| error.to_string())?;
    let selected_support = query
        .rules
        .iter()
        .try_fold(false, |overlap, rule| {
            sound
                .selects_range(rule.samples.clone())
                .map(|selected| overlap || selected)
        })
        .map_err(|error| error.to_string())?;
    Ok(PauseTarget {
        at,
        issuer,
        label,
        allowed,
        selected_support,
    })
}

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

/// A bed drop: end the sound abruptly at Edit frame boundary `at`, keeping
/// its onset, source phase and gain. The selection ends exactly at that frame
/// boundary in the sound's own exact frame clock, and the end edge is Hard so
/// no fade softens the cut.
pub fn cut(workspace: &Workspace, id: &SoundId, at: ProjectFrame) -> Result<SoundEvent, String> {
    if workspace.document.sound_routes().contains_key(id) {
        return Err("This sound follows timeline cuts. Cutting a routed sound is not supported yet; no edit was made.".into());
    }
    let event = movable(workspace, id)?
        .cut_at(at, workspace.document.presentation_basis().frame_rate)
        .map_err(|error| error.message)?;
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
