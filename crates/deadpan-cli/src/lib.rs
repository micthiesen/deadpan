//! Headless application boundary, shared with the native host.

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod audio;
mod doctor;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod originals;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod source_registration;

use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::ExitCode;

use deadpan_core::{
    ColorPolicy, Command, CommandRequest, FrameRate, NodeId, PresentationBasis, ProjectDocument,
    ProjectId, RevisionId,
};
use deadpan_store::{AccessMode, ProjectStore, StoreError};
use serde::{Deserialize, Serialize};

const HELP: &str = "Deadpan headless commands:
  doctor
  project create <project.deadpan> [--fps <N/D> --size <WIDTHxHEIGHT>]
  project validate <project.deadpan>
  project dump <project.deadpan> --json
  project undo <project.deadpan> --expected <revision> [--dry-run]
  project redo <project.deadpan> --expected <revision> [--dry-run]
  project checkpoint <project.deadpan>
  project migrate <project.deadpan>
  project retain-original <project.deadpan> <absolute-source> [--linked]
  project register-source <project.deadpan> --request-json <request.json> [--dry-run]
  project adopt-primary-geometry <project.deadpan> --request-json <request.json> [--dry-run]
  project originals <project.deadpan> [--after <blake3-digest>]
  project verify-original <project.deadpan> <blake3-digest>
  project relink-original <project.deadpan> <blake3-digest> <absolute-source> --expected-version <N>
  inspect-plan <project.deadpan> [--frame <N>]
  inspect-plan <project.deadpan> --audio-samples <START> <END>
  inspect-audio <project.deadpan> --samples <START> <END> [--time-mapped | --edge-faded]
  inspect-audio-domain <project.deadpan> --at <PROBE> --samples <START> <END>
  inspect-audio-definition <project.deadpan> (--node <ID> | --repeat-default <ID>) --samples <START> <END> [--revision <ID>]
  resolve-selection <project.deadpan> --json <selection.json>
  command <project.deadpan> --json <request.json> [--dry-run]

Creation defaults to a provisional 1920x1080, 30 fps presentation basis.
Document dumps are inspection output; SQLite remains authoritative.
Original retention preserves complete bytes; stream qualification and authored import remain separate.
Audio inspection returns at most 256 stereo source samples before effects and mastering.
Domain inspection reads raw physical context; signed START/END use its captured root grid.
Definition inspection reads a local-zero point grid, not final timeline allocation.";

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("{0}")]
    Usage(String),
    #[error("Unsupported command protocol {0}; expected 1")]
    Protocol(u32),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Document(#[from] deadpan_core::DocumentError),
    #[error(transparent)]
    Timing(#[from] deadpan_core::TimeError),
    #[error(transparent)]
    Plan(#[from] deadpan_plan::PlanError),
    #[error(transparent)]
    Anchor(#[from] deadpan_core::AnchorError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[error(transparent)]
    SourceInput(#[from] deadpan_media::source_input::SourceInputError),
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[error(transparent)]
    SourceVideo(#[from] deadpan_media::source_session::SourceSessionError),
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[error(transparent)]
    SourceAudio(#[from] deadpan_media::audio_session::AudioSessionError),
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[error(transparent)]
    ProjectAudio(#[from] audio::ProjectAudioError),
}

impl CliError {
    fn code(&self) -> &str {
        match self {
            Self::Usage(_) | Self::Timing(_) | Self::Json(_) => "InvalidInput",
            Self::Protocol(_) => "ProtocolUnsupported",
            Self::Document(_) => "ProjectInvalid",
            Self::Plan(deadpan_plan::PlanError::FrameOutOfRange { .. }) => "FrameOutOfRange",
            Self::Plan(deadpan_plan::PlanError::AudioRangeOutOfRange) => "AudioRangeOutOfRange",
            Self::Plan(deadpan_plan::PlanError::AudioQueryLimit(_)) => "AudioQueryLimit",
            Self::Plan(deadpan_plan::PlanError::InvalidAudioDefinitionSelector(_)) => {
                "AudioDefinitionUnavailable"
            }
            Self::Plan(deadpan_plan::PlanError::Time(_)) => "TimingOverflow",
            Self::Plan(_) => "PlanInvalid",
            Self::Anchor(error) => error.code(),
            Self::Io(_) => "IoFailure",
            Self::Store(error) => error.code(),
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            Self::SourceInput(_) => "SourceSnapshotFailed",
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            Self::SourceVideo(_) => "SourceVideoDecodeFailed",
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            Self::SourceAudio(_) => "SourceAudioDecodeFailed",
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            Self::ProjectAudio(error) => match error {
                audio::ProjectAudioError::Store(error) => error.code(),
                audio::ProjectAudioError::Sequence(deadpan_audio::SequenceAudioError::Range) => {
                    "AudioRangeOutOfRange"
                }
                audio::ProjectAudioError::Stage(deadpan_audio::StageAudioError::Range)
                | audio::ProjectAudioError::Stage(deadpan_audio::StageAudioError::Plan(
                    deadpan_plan::PlanError::AudioRangeOutOfRange,
                ))
                | audio::ProjectAudioError::Plan(deadpan_plan::PlanError::AudioRangeOutOfRange) => {
                    "AudioRangeOutOfRange"
                }
                audio::ProjectAudioError::Stage(deadpan_audio::StageAudioError::ForeignDomain) => {
                    "AudioDomainUnavailable"
                }
                audio::ProjectAudioError::Stage(
                    deadpan_audio::StageAudioError::ForeignDefinition,
                )
                | audio::ProjectAudioError::Plan(
                    deadpan_plan::PlanError::InvalidAudioDefinitionSelector(_),
                )
                | audio::ProjectAudioError::Stage(deadpan_audio::StageAudioError::Plan(
                    deadpan_plan::PlanError::InvalidAudioDefinitionSelector(_),
                )) => "AudioDefinitionUnavailable",
                audio::ProjectAudioError::Plan(deadpan_plan::PlanError::AudioQueryLimit(_))
                | audio::ProjectAudioError::Stage(deadpan_audio::StageAudioError::Plan(
                    deadpan_plan::PlanError::AudioQueryLimit(_),
                )) => "AudioQueryLimit",
                audio::ProjectAudioError::Stage(
                    deadpan_audio::StageAudioError::Limit(_)
                    | deadpan_audio::StageAudioError::InvalidLimits,
                ) => "AudioPreparationLimit",
                audio::ProjectAudioError::Stage(deadpan_audio::StageAudioError::Timeout) => {
                    "AudioPreparationTimeout"
                }
                audio::ProjectAudioError::Stage(deadpan_audio::StageAudioError::Unsupported(_)) => {
                    "AudioOperationUnsupported"
                }
                audio::ProjectAudioError::Stage(deadpan_audio::StageAudioError::Preparation(
                    error,
                )) => match error {
                    deadpan_audio::PreparationError::UnsupportedLayout => "AudioLayoutUnsupported",
                    deadpan_audio::PreparationError::SourceUnavailable(_) => {
                        "SourceAudioUnavailable"
                    }
                    deadpan_audio::PreparationError::IndexMismatch => "SourceAudioIndexMismatch",
                    deadpan_audio::PreparationError::Cancelled => "Cancelled",
                    deadpan_audio::PreparationError::Time(_) => "TimingOverflow",
                    _ => "AudioInspectionFailed",
                },
                audio::ProjectAudioError::Stage(error @ deadpan_audio::StageAudioError::Dsp(_)) => {
                    if error.is_cancelled() {
                        "Cancelled"
                    } else {
                        "AudioProcessingFailed"
                    }
                }
                audio::ProjectAudioError::Stage(deadpan_audio::StageAudioError::Time(_)) => {
                    "TimingOverflow"
                }
                audio::ProjectAudioError::Sequence(
                    deadpan_audio::SequenceAudioError::Unsupported { .. },
                ) => "AudioOperationUnsupported",
                audio::ProjectAudioError::Sequence(
                    deadpan_audio::SequenceAudioError::Preparation(
                        deadpan_audio::PreparationError::UnsupportedLayout,
                    ),
                ) => "AudioLayoutUnsupported",
                audio::ProjectAudioError::Sequence(
                    deadpan_audio::SequenceAudioError::Preparation(
                        deadpan_audio::PreparationError::SourceUnavailable(_),
                    ),
                ) => "SourceAudioUnavailable",
                audio::ProjectAudioError::Sequence(
                    deadpan_audio::SequenceAudioError::Preparation(
                        deadpan_audio::PreparationError::IndexMismatch,
                    ),
                ) => "SourceAudioIndexMismatch",
                audio::ProjectAudioError::Sequence(
                    deadpan_audio::SequenceAudioError::Preparation(
                        deadpan_audio::PreparationError::Cancelled,
                    ),
                ) => "Cancelled",
                audio::ProjectAudioError::Sequence(deadpan_audio::SequenceAudioError::Time(_)) => {
                    "TimingOverflow"
                }
                _ => "AudioInspectionFailed",
            },
        }
    }
    fn current_revision(&self) -> Option<&str> {
        match self {
            Self::Store(StoreError::RevisionConflict { current, .. }) => Some(current),
            Self::Store(StoreError::Edit(error)) => {
                error.current_revision.as_ref().map(RevisionId::as_str)
            }
            Self::Anchor(error) => error.current_revision.as_ref().map(RevisionId::as_str),
            _ => None,
        }
    }
    fn recovery_backup(&self) -> Option<&Path> {
        match self {
            Self::Store(StoreError::MigrationFailed { backup, .. }) => Some(backup),
            _ => None,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CommandEnvelope {
    protocol: u32,
    project_id: ProjectId,
    expected_revision: RevisionId,
    #[serde(default)]
    new_revision: Option<RevisionId>,
    command: Command,
    #[serde(default)]
    dry_run: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SelectionEnvelope {
    protocol: u32,
    request: deadpan_core::SelectionRequest,
}

/// Shared process entrypoint for the CLI and `deadpan-app --headless`.
pub fn entry(arguments: impl IntoIterator<Item = String>) -> ExitCode {
    let arguments: Vec<String> = arguments.into_iter().collect();
    match run(&arguments) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let report = serde_json::json!({ "schema_version": 1, "error": {
                "code": error.code(), "message": error.to_string(), "current_revision": error.current_revision(), "recovery_backup": error.recovery_backup()
            }});
            let _ = writeln!(io::stderr().lock(), "{report}");
            ExitCode::FAILURE
        }
    }
}

fn run(arguments: &[String]) -> Result<(), CliError> {
    let arguments: Vec<&str> = arguments.iter().map(String::as_str).collect();
    match arguments.as_slice() {
        [] | ["--help"] | ["-h"] => {
            println!("{HELP}");
            Ok(())
        }
        ["doctor"] => write_json(&doctor::report()?),
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        [
            "inspect-audio-domain",
            path,
            "--at",
            probe,
            "--samples",
            start,
            end,
        ] => {
            let probe = probe
                .parse::<i64>()
                .map_err(|_| CliError::Usage("invalid audio domain probe".into()))?;
            let start = start
                .parse::<i64>()
                .map_err(|_| CliError::Usage("invalid audio sample start".into()))?;
            let end = end
                .parse::<i64>()
                .map_err(|_| CliError::Usage("invalid audio sample end".into()))?;
            let frames = end
                .checked_sub(start)
                .and_then(|value| u32::try_from(value).ok())
                .filter(|frames| *frames > 0 && *frames <= deadpan_audio::MAX_OUTPUT_FRAMES)
                .ok_or(audio::ProjectAudioError::Stage(
                    deadpan_audio::StageAudioError::Range,
                ))?;
            let mut session = audio::ProjectAudioSession::open(Path::new(path))?;
            let block = session.read_domain(
                deadpan_core::AudioSample(probe),
                deadpan_core::AudioSample(start),
                frames,
                &std::sync::atomic::AtomicBool::new(false),
            )?;
            write_json(&serde_json::json!({ "protocol": 1, "audio": block }))
        }
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        [
            "inspect-audio-definition",
            path,
            selector,
            id,
            "--samples",
            start,
            end,
            retained @ ..,
        ] if matches!(*selector, "--node" | "--repeat-default") => {
            let revision = match retained {
                [] => None,
                ["--revision", id] => Some(RevisionId::new(*id)?),
                _ => return Err(CliError::Usage("expected optional --revision <ID>".into())),
            };
            let start = start
                .parse::<i64>()
                .map_err(|_| CliError::Usage("invalid definition sample start".into()))?;
            let end = end
                .parse::<i64>()
                .map_err(|_| CliError::Usage("invalid definition sample end".into()))?;
            let frames = end
                .checked_sub(start)
                .and_then(|count| u32::try_from(count).ok())
                .filter(|count| {
                    start >= 0 && *count > 0 && *count <= deadpan_audio::MAX_OUTPUT_FRAMES
                })
                .ok_or(audio::ProjectAudioError::Stage(
                    deadpan_audio::StageAudioError::Range,
                ))?;
            let id = deadpan_core::NodeId::new(*id)?;
            let selector = if *selector == "--node" {
                deadpan_plan::AudioDefinitionSelector::Node { node: id }
            } else {
                deadpan_plan::AudioDefinitionSelector::RepeatDefault { repeat: id }
            };
            let mut session = match revision {
                Some(revision) => {
                    audio::ProjectAudioSession::open_revision(Path::new(path), &revision)?
                }
                None => audio::ProjectAudioSession::open(Path::new(path))?,
            };
            let block = session.read_definition(
                selector,
                deadpan_plan::SignalSample(start),
                frames,
                &std::sync::atomic::AtomicBool::new(false),
            )?;
            write_json(&serde_json::json!({ "protocol": 1, "audio": block }))
        }
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        ["inspect-audio", path, "--samples", start, end]
        | [
            "inspect-audio",
            path,
            "--samples",
            start,
            end,
            "--time-mapped",
        ]
        | [
            "inspect-audio",
            path,
            "--samples",
            start,
            end,
            "--edge-faded",
        ] => {
            let start = start
                .parse::<i64>()
                .map_err(|_| CliError::Usage("invalid audio sample start".into()))?;
            let end = end
                .parse::<i64>()
                .map_err(|_| CliError::Usage("invalid audio sample end".into()))?;
            let frames = end
                .checked_sub(start)
                .and_then(|value| u32::try_from(value).ok())
                .ok_or(audio::ProjectAudioError::Sequence(
                    deadpan_audio::SequenceAudioError::Range,
                ))?;
            let mut session = audio::ProjectAudioSession::open(Path::new(path))?;
            let cancelled = std::sync::atomic::AtomicBool::new(false);
            let block = if arguments.last() == Some(&"--edge-faded") {
                serde_json::to_value(session.read_edge_faded(
                    deadpan_core::AudioSample(start),
                    frames,
                    &cancelled,
                )?)?
            } else if arguments.last() == Some(&"--time-mapped") {
                serde_json::to_value(session.read_time_mapped(
                    deadpan_core::AudioSample(start),
                    frames,
                    &cancelled,
                )?)?
            } else {
                serde_json::to_value(session.read(
                    deadpan_core::AudioSample(start),
                    frames,
                    &cancelled,
                )?)?
            };
            write_json(&serde_json::json!({ "protocol": 1, "audio": block }))
        }
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        [
            "project",
            action @ ("retain-original" | "originals" | "verify-original" | "relink-original"),
            rest @ ..,
        ] => originals::run(action, rest),
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        [
            "project",
            "register-source",
            path,
            "--request-json",
            request,
        ] => source_registration::run(Path::new(path), Path::new(request), false),
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        [
            "project",
            "register-source",
            path,
            "--request-json",
            request,
            "--dry-run",
        ] => source_registration::run(Path::new(path), Path::new(request), true),
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        [
            "project",
            "adopt-primary-geometry",
            path,
            "--request-json",
            request,
        ] => source_registration::adopt_geometry(Path::new(path), Path::new(request), false),
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        [
            "project",
            "adopt-primary-geometry",
            path,
            "--request-json",
            request,
            "--dry-run",
        ] => source_registration::adopt_geometry(Path::new(path), Path::new(request), true),
        ["project", "create", path] => {
            let document = ProjectDocument::new_automatic(
                ProjectId::new(uuid::Uuid::new_v4().to_string())?,
                new_revision()?,
                NodeId::new(uuid::Uuid::new_v4().to_string())?,
            )?;
            let store = ProjectStore::create(Path::new(path), &document)?;
            summary(&store.snapshot()?)
        }
        ["project", "create", path, "--fps", fps, "--size", size] => {
            let (numerator, denominator) = pair(fps, '/')?;
            let (width, height) = pair(size, 'x')?;
            let document = ProjectDocument::new(
                ProjectId::new(uuid::Uuid::new_v4().to_string())?,
                new_revision()?,
                PresentationBasis {
                    width,
                    height,
                    frame_rate: FrameRate::new(numerator, denominator)?,
                    color_policy: ColorPolicy::SdrRec709,
                },
                NodeId::new(uuid::Uuid::new_v4().to_string())?,
            )?;
            let store = ProjectStore::create(Path::new(path), &document)?;
            summary(&store.snapshot()?)
        }
        ["project", "validate", path] => {
            let store = ProjectStore::open(Path::new(path), AccessMode::ReadOnly)?;
            store.validate()?;
            summary(&store.snapshot()?)
        }
        ["project", "dump", path, "--json"] => {
            let document = ProjectStore::open(Path::new(path), AccessMode::ReadOnly)?.snapshot()?;
            io::stdout()
                .lock()
                .write_all(document.to_json()?.as_bytes())?;
            Ok(())
        }
        [
            "project",
            action @ ("undo" | "redo"),
            path,
            "--expected",
            expected,
        ] => history(Path::new(path), action, expected, false),
        [
            "project",
            action @ ("undo" | "redo"),
            path,
            "--expected",
            expected,
            "--dry-run",
        ] => history(Path::new(path), action, expected, true),
        ["project", "checkpoint", path] => {
            let store = ProjectStore::open(Path::new(path), AccessMode::ReadWrite)?;
            write_json(
                &serde_json::json!({ "protocol": 1, "database_checkpoint": store.checkpoint()? }),
            )
        }
        ["project", "migrate", path] => write_json(
            &serde_json::json!({ "protocol": 1, "migration": ProjectStore::migrate(Path::new(path))? }),
        ),
        ["inspect-plan", path] => {
            let document = ProjectStore::open(Path::new(path), AccessMode::ReadOnly)?.snapshot()?;
            let plan = deadpan_plan::RenderPlan::compile(&document)?;
            write_json(&serde_json::json!({ "protocol": 1, "plan": plan.inspect() }))
        }
        ["inspect-plan", path, "--audio-samples", start, end] => {
            let parse = |value: &str| {
                value
                    .parse::<i64>()
                    .map(deadpan_core::AudioSample)
                    .map_err(|_| {
                        CliError::Usage(
                            "Audio boundaries must be signed 48 kHz sample integers".into(),
                        )
                    })
            };
            let samples = parse(start)?..parse(end)?;
            let document = ProjectStore::open(Path::new(path), AccessMode::ReadOnly)?.snapshot()?;
            let plan = deadpan_plan::RenderPlan::compile(&document)?;
            write_json(&serde_json::json!({
                "protocol": 1,
                "audio": plan.audio(samples, deadpan_plan::AudioQueryLimits::default())?,
            }))
        }
        ["inspect-plan", path, "--frame", frame] => {
            let frame = frame
                .parse::<i64>()
                .map_err(|_| CliError::Usage("Frame must be a signed integer".into()))?;
            let document = ProjectStore::open(Path::new(path), AccessMode::ReadOnly)?.snapshot()?;
            let plan = deadpan_plan::RenderPlan::compile(&document)?;
            write_json(
                &serde_json::json!({ "protocol": 1, "sample": plan.picture(deadpan_core::ProjectFrame(frame))? }),
            )
        }
        ["resolve-selection", path, "--json", request] => {
            let envelope: SelectionEnvelope =
                serde_json::from_str(&read_request(Path::new(request))?)?;
            if envelope.protocol != 1 {
                return Err(CliError::Protocol(envelope.protocol));
            }
            let document = ProjectStore::open(Path::new(path), AccessMode::ReadOnly)?.snapshot()?;
            let index = deadpan_core::AnchorIndex::new(&document)?;
            write_json(
                &serde_json::json!({ "protocol": 1, "resolved": index.resolve(&envelope.request)? }),
            )
        }
        ["command", path, "--json", request] => command(Path::new(path), Path::new(request), false),
        ["command", path, "--json", request, "--dry-run"] => {
            command(Path::new(path), Path::new(request), true)
        }
        _ => Err(CliError::Usage(format!(
            "Unknown command or invalid arguments.\n{HELP}"
        ))),
    }
}

fn new_revision() -> Result<RevisionId, deadpan_core::DocumentError> {
    RevisionId::new(uuid::Uuid::new_v4().to_string())
}

fn history(package: &Path, action: &str, expected: &str, preview: bool) -> Result<(), CliError> {
    let mut store = ProjectStore::open(
        package,
        if preview {
            AccessMode::ReadOnly
        } else {
            AccessMode::ReadWrite
        },
    )?;
    let expected = RevisionId::new(expected)?;
    let next = new_revision()?;
    let outcome = match (action, preview) {
        ("undo", true) => store.preview_undo(&expected, next)?,
        ("undo", false) => store.undo(&expected, next)?,
        (_, true) => store.preview_redo(&expected, next)?,
        (_, false) => store.redo(&expected, next)?,
    };
    write_json(&serde_json::json!({ "protocol": 1, "committed": !preview, "outcome": outcome }))
}

fn pair(value: &str, separator: char) -> Result<(u32, u32), CliError> {
    let invalid = || {
        CliError::Usage(format!(
            "Expected two positive integers separated by '{separator}'"
        ))
    };
    let (first, second) = value.split_once(separator).ok_or_else(invalid)?;
    let first: u32 = first.parse().map_err(|_| invalid())?;
    let second: u32 = second.parse().map_err(|_| invalid())?;
    if first == 0 || second == 0 {
        return Err(invalid());
    }
    Ok((first, second))
}

fn read_request(request: &Path) -> Result<String, CliError> {
    let mut json = String::new();
    File::open(request)?
        .take(deadpan_core::MAX_DOCUMENT_JSON_BYTES as u64 + 1)
        .read_to_string(&mut json)?;
    if json.len() > deadpan_core::MAX_DOCUMENT_JSON_BYTES {
        return Err(CliError::Usage(
            "JSON request exceeds the 64 MiB limit".into(),
        ));
    }
    Ok(json)
}

fn command(package: &Path, request: &Path, dry_run: bool) -> Result<(), CliError> {
    let envelope: CommandEnvelope = serde_json::from_str(&read_request(request)?)?;
    if envelope.protocol != 1 {
        return Err(CliError::Protocol(envelope.protocol));
    }
    let preview = dry_run || envelope.dry_run;
    let request = CommandRequest {
        project_id: envelope.project_id,
        expected_revision: envelope.expected_revision,
        new_revision: envelope.new_revision.map_or_else(new_revision, Ok)?,
        command: envelope.command,
    };
    let mut store = ProjectStore::open(
        package,
        if preview {
            AccessMode::ReadOnly
        } else {
            AccessMode::ReadWrite
        },
    )?;
    if preview {
        write_json(
            &serde_json::json!({ "protocol": 1, "committed": false, "edit": store.preview(&request)? }),
        )
    } else {
        write_json(
            &serde_json::json!({ "protocol": 1, "committed": true, "outcome": store.commit(&request)? }),
        )
    }
}

fn summary(document: &ProjectDocument) -> Result<(), CliError> {
    write_json(&serde_json::json!({
        "protocol": 1, "valid": true, "project_id": document.project_id(), "revision_id": document.revision_id(),
        "root": document.root(), "duration_frames": document.duration()?.frames(), "node_count": document.nodes().len(), "mark_count": document.marks().len(),
        "presentation_basis": document.presentation_basis(), "basis_state": document.basis_state()
    }))
}

fn write_json(value: &impl Serialize) -> Result<(), CliError> {
    let mut output = io::stdout().lock();
    serde_json::to_writer_pretty(&mut output, value)?;
    writeln!(output)?;
    Ok(())
}
