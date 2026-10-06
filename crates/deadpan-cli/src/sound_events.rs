//! Pure root sound placement from measured catalog evidence, shared by the
//! native `,s` / `:sound-*` commands and the headless `sound` command: both
//! derive the same complete `SoundEvent` and commit it as an ordinary
//! `SetSound`, `SetSoundAllowance` or `DeleteSound`. Storage and playback
//! still own transactional and live source admission.

use deadpan_core::{
    AssetId, AudioEdgePolicy, AudioSample, ExactFrameRange, ExactRatio, FrameDuration,
    ProjectFrame, SoundEvent, SoundHoldIssuer, SoundId, SoundOverflowPolicy, SourceAudio,
    SourceAudioMapping,
};

use deadpan_core::ProjectDocument;
use deadpan_plan::RenderPlan;
use deadpan_store::source_registration::SourceQualificationReceipt;

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
    document: &ProjectDocument,
    plan: &RenderPlan,
    id: &SoundId,
    at: ProjectFrame,
) -> Result<PauseTarget, String> {
    if at.0 < 0 || at.0 >= plan.duration().frames() {
        return Err("Place the Edit cursor inside a silent pause first.".into());
    }
    let sample = document
        .presentation_basis()
        .frame_rate
        .audio_boundary(at)
        .map_err(|error| error.to_string())?;
    let next = document
        .presentation_basis()
        .frame_rate
        .audio_boundary(ProjectFrame(at.0 + 1))
        .map_err(|error| error.to_string())?;
    if next <= sample {
        return Err("The Edit cursor frame has no allocated audio sample.".into());
    }
    let query = plan
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
    let name = document
        .nodes()
        .get(&instance.node)
        .map(|node| node.label.as_str())
        .unwrap_or("Pause");
    // The current frame and readable play positions explain the scope. The
    // complete stable issuer, never this display label, chooses the target.
    let mut label = name.to_owned();
    for play in &instance.repeats {
        let owner = document
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
            &document.nodes()[&instance.node].kind
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
    let allowed = document
        .sound_allowances()
        .get(id)
        .is_some_and(|allowances| allowances.contains(&issuer));
    let sound = plan.root_sound(id).map_err(|error| error.to_string())?;
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
    document: &ProjectDocument,
    plan: &RenderPlan,
    receipt: &SourceQualificationReceipt,
    asset: &AssetId,
    at: AudioSample,
) -> Result<SoundEvent, String> {
    let metadata = document
        .assets()
        .get(asset)
        .ok_or("The catalog sound is no longer registered.")?;
    if receipt.snapshot().video().is_some()
        || metadata.video.is_some()
        || metadata.still_image
        || receipt.snapshot().audio().is_none()
    {
        return Err("Sound placement requires an audio-only catalog source.".into());
    }
    if metadata.source_qualification.as_ref() != Some(receipt.id()) {
        return Err("The catalog sound qualification has changed.".into());
    }
    let rate = document.presentation_basis().frame_rate;
    let span = receipt
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
        owner: document.root().clone(),
        label: metadata.label.clone(),
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
    validate_placement(document, plan, &event)?;
    Ok(event)
}

pub fn moved(
    document: &ProjectDocument,
    plan: &RenderPlan,
    id: &SoundId,
    at: AudioSample,
) -> Result<SoundEvent, String> {
    let mut event = movable(document, id)?;
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
    validate_placement(document, plan, &event)?;
    Ok(event)
}

/// Shift an unrouted recipe in its exact project-frame clock. The independent
/// sample offset and source selection phase survive repeated and inverse nudges.
pub fn nudge(
    document: &ProjectDocument,
    plan: &RenderPlan,
    id: &SoundId,
    frames: i64,
) -> Result<SoundEvent, String> {
    let mut event = movable(document, id)?;
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
    validate_placement(document, plan, &event)?;
    Ok(event)
}

/// A bed drop: end the sound abruptly at Edit frame boundary `at`, keeping
/// its onset, source phase and gain. The selection ends exactly at that frame
/// boundary in the sound's own exact frame clock, and the end edge is Hard so
/// no fade softens the cut.
pub fn cut(
    document: &ProjectDocument,
    plan: &RenderPlan,
    id: &SoundId,
    at: ProjectFrame,
) -> Result<SoundEvent, String> {
    if document.sound_routes().contains_key(id) {
        return Err("This sound follows timeline cuts. Cutting a routed sound is not supported yet; no edit was made.".into());
    }
    let event = movable(document, id)?
        .cut_at(at, document.presentation_basis().frame_rate)
        .map_err(|error| error.message)?;
    validate_placement(document, plan, &event)?;
    Ok(event)
}

fn movable(document: &ProjectDocument, id: &SoundId) -> Result<SoundEvent, String> {
    let event = document
        .sounds()
        .get(id)
        .cloned()
        .ok_or("The selected sound no longer exists.")?;
    if document.sound_routes().contains_key(id) {
        return Err("This sound has retained edit cuts. Moving routed sounds is not supported yet; its routing was preserved.".into());
    }
    Ok(event)
}

fn validate_placement(
    document: &ProjectDocument,
    plan: &RenderPlan,
    event: &SoundEvent,
) -> Result<(), String> {
    if event.owner != *document.root() {
        return Err("Native sound placement currently requires the project root owner.".into());
    }
    let selected = event
        .mapping
        .selection_frames_with_offset(
            FrameDuration::ZERO,
            event.offset,
            document.presentation_basis().frame_rate,
        )
        .map_err(|error| error.to_string())?;
    if selected.start.compare_integer(0).is_lt() {
        return Err("Sound onset must be inside Your edit.".into());
    }
    if selected
        .end
        .compare_integer(plan.duration().frames())
        .is_gt()
    {
        return Err(
            "The complete sound extends past the end of Your edit; choose an earlier onset.".into(),
        );
    }
    Ok(())
}

/// Where a sound starts: an Edit frame boundary (as `,s` at the Edit cursor)
/// or an exact 48 kHz sample onset (as `:sound-at`).
#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Onset {
    Frame(i64),
    Sample(i64),
}

/// One native placed-sound action, with the context the GUI takes from its
/// focus (Edit cursor, selected sound) given explicitly.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SoundEdit {
    /// `,s` / `:sound-place`: the complete catalog sound at `at`.
    Place {
        asset: AssetId,
        at: Onset,
        /// The new sound's identity; omitted for a fresh one.
        #[serde(default)]
        id: Option<SoundId>,
    },
    /// `:sound-at`: move an uncut sound's onset, keeping its phase.
    Move { id: SoundId, at: Onset },
    /// `h` / `l`: shift an uncut sound by exact project frames.
    Nudge { id: SoundId, frames: i64 },
    /// `:sound-gain` (absolute or a step) and `:sound-edges`.
    Set {
        id: SoundId,
        #[serde(default)]
        gain_millidecibels: Option<i32>,
        #[serde(default)]
        gain_step_millidecibels: Option<i32>,
        #[serde(default)]
        edges: Option<AudioEdgePolicy>,
    },
    /// `:sound-cut`: end the sound at this Edit frame with a hard edge.
    Cut { id: SoundId, at: i64 },
    /// `:sound-allow` / `:sound-silence` in the one silent pause at this
    /// Edit frame.
    Allowance { id: SoundId, at: i64, allowed: bool },
    /// `:sound-delete`.
    Delete { id: SoundId },
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SoundRequest {
    pub protocol: u32,
    pub expected_revision: deadpan_core::RevisionId,
    #[serde(default)]
    pub new_revision: Option<deadpan_core::RevisionId>,
    pub edit: SoundEdit,
}

fn refused(message: impl ToString) -> crate::live_project::LiveError {
    crate::live_project::LiveError::new("SoundEditRefused", message)
}

/// Derive the exact core command the GUI would commit for `edit` against
/// `document` at its own revision, plus the sound it addresses.
pub fn command(
    store: &deadpan_store::ProjectStore,
    document: &ProjectDocument,
    edit: &SoundEdit,
) -> Result<(deadpan_core::Command, SoundId), crate::live_project::LiveError> {
    use deadpan_core::Command;
    let plan = RenderPlan::compile(document).map_err(refused)?;
    let rate = document.presentation_basis().frame_rate;
    let onset = |at: Onset| -> Result<AudioSample, crate::live_project::LiveError> {
        match at {
            Onset::Sample(sample) => Ok(AudioSample(sample)),
            Onset::Frame(frame) => rate.audio_boundary(ProjectFrame(frame)).map_err(refused),
        }
    };
    let set = |id: &SoundId, event: SoundEvent| {
        (
            Command::SetSound {
                id: id.clone(),
                event,
            },
            id.clone(),
        )
    };
    Ok(match edit {
        SoundEdit::Place { asset, at, id } => {
            let qualification = document
                .assets()
                .get(asset)
                .and_then(|record| record.source_qualification.clone())
                .ok_or_else(|| refused("The catalog sound is no longer registered."))?;
            let receipt = store
                .source_qualification(&qualification)
                .map_err(crate::live_project::LiveError::store)?;
            let event =
                placement(document, &plan, &receipt, asset, onset(*at)?).map_err(refused)?;
            let id = match id {
                Some(id) => id.clone(),
                None => SoundId::new(uuid::Uuid::new_v4().to_string()).map_err(refused)?,
            };
            if document.sounds().contains_key(&id) {
                return Err(refused("A placed sound already has this identity."));
            }
            set(&id, event)
        }
        SoundEdit::Move { id, at } => set(
            id,
            moved(document, &plan, id, onset(*at)?).map_err(refused)?,
        ),
        SoundEdit::Nudge { id, frames } => {
            set(id, nudge(document, &plan, id, *frames).map_err(refused)?)
        }
        SoundEdit::Set {
            id,
            gain_millidecibels,
            gain_step_millidecibels,
            edges,
        } => {
            let mut event = document
                .sounds()
                .get(id)
                .cloned()
                .ok_or_else(|| refused("The selected sound no longer exists."))?;
            match (gain_millidecibels, gain_step_millidecibels) {
                (Some(_), Some(_)) => {
                    return Err(refused("Give either a gain or a gain step, not both."));
                }
                (Some(gain), None) => event.gain_millidecibels = *gain,
                (None, Some(step)) => {
                    event.gain_millidecibels = event
                        .gain_millidecibels
                        .checked_add(*step)
                        .ok_or_else(|| refused("Sound gain exceeds the supported range."))?;
                }
                (None, None) if edges.is_none() => {
                    return Err(refused("Give a gain, a gain step or edges to change."));
                }
                (None, None) => {}
            }
            if let Some(edge) = edges {
                event.start_edge = *edge;
                event.end_edge = *edge;
            }
            set(id, event)
        }
        SoundEdit::Cut { id, at } => set(
            id,
            cut(document, &plan, id, ProjectFrame(*at)).map_err(refused)?,
        ),
        SoundEdit::Allowance { id, at, allowed } => {
            if !document.sounds().contains_key(id) {
                return Err(refused("The selected sound no longer exists."));
            }
            let target = pause_target(document, &plan, id, ProjectFrame(*at)).map_err(refused)?;
            target.validate_change(*allowed).map_err(refused)?;
            (
                Command::SetSoundAllowance {
                    sound: id.clone(),
                    issuer: target.issuer,
                    allowed: *allowed,
                },
                id.clone(),
            )
        }
        SoundEdit::Delete { id } => {
            if !document.sounds().contains_key(id) {
                return Err(refused("The selected sound no longer exists."));
            }
            (Command::DeleteSound { id: id.clone() }, id.clone())
        }
    })
}

/// `sound <project> --json <request> [--dry-run]`: derive the event the GUI
/// derives from its focus, then commit it through the ordinary `command`
/// path (closed project or the open app's live endpoint).
pub fn run(arguments: &[&str]) -> Result<(), crate::CliError> {
    let usage = || {
        crate::CliError::Usage("usage: sound <project.deadpan> --json <request.json> [--dry-run] | sound --write-sting <new.wav>".into())
    };
    let (package, path, dry_run) = match arguments {
        ["--write-sting", path] => return write_sting(std::path::Path::new(path)),
        [package, "--json", path] => (*package, *path, false),
        [package, "--json", path, "--dry-run"] => (*package, *path, true),
        _ => return Err(usage()),
    };
    let request: SoundRequest =
        serde_json::from_str(&crate::read_request(std::path::Path::new(path))?)?;
    if request.protocol != 1 {
        return Err(crate::CliError::Protocol(request.protocol));
    }
    let package = std::path::Path::new(package);
    let (command, id, project) = {
        let store =
            deadpan_store::ProjectStore::open(package, deadpan_store::AccessMode::ReadOnly)?;
        let document = store.snapshot()?;
        if document.revision_id() != &request.expected_revision {
            return Err(deadpan_store::StoreError::RevisionConflict {
                expected: request.expected_revision.as_str().into(),
                current: document.revision_id().as_str().into(),
            }
            .into());
        }
        let (command, id) = command(&store, &document, &request.edit)?;
        (command, id, document.project_id().clone())
    };
    let new_revision = match request.new_revision {
        Some(revision) => revision,
        None => crate::new_revision()?,
    };
    let mut output = crate::live_project::dispatch_short(
        package,
        Some(project.clone()),
        crate::live_project::ShortOperation::Edit {
            request: Box::new(deadpan_core::CommandRequest {
                project_id: project,
                expected_revision: request.expected_revision,
                new_revision,
                command,
            }),
            dry_run,
        },
    )?;
    if let Some(object) = output.as_object_mut() {
        object.insert("sound_id".into(), serde_json::json!(id));
    }
    crate::write_json(&output)
}

/// `sound --write-sting <file>`: the exact bytes `:sting` adds to the catalog,
/// synthesized here, for `project retain-original` and `register-source`
/// with `audio_only` streams. An existing different file is never replaced.
fn write_sting(path: &std::path::Path) -> Result<(), crate::CliError> {
    use std::io::Write;
    let bytes = deadpan_audio::triumphant_sting_wav();
    let written = match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(mut file) => {
            file.write_all(&bytes)?;
            file.sync_all()?;
            true
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            if std::fs::read(path)? != bytes {
                return Err(crate::CliError::Usage(format!(
                    "{} exists with other contents; choose a new file",
                    path.display()
                )));
            }
            false
        }
        Err(error) => return Err(error.into()),
    };
    crate::write_json(&serde_json::json!({
        "protocol": 1,
        "path": path,
        "written": written,
        "label": deadpan_audio::STING_LABEL,
        "version": deadpan_audio::STING_VERSION,
        "bytes": bytes.len(),
        "sample_rate": deadpan_audio::STING_SAMPLE_RATE,
        "frames": deadpan_audio::STING_FRAMES,
    }))
}
