//! Explicit stream qualification of retained originals for the headless host.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use deadpan_media::audio_session::{AudioSession, AudioSessionLimits};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_media::source_input::VerifiedSourceInput;
use deadpan_media::source_qualification::DecodedSourceQualification;
use deadpan_media::source_session::{SourceSession, SourceSessionLimits};
use deadpan_store::original_media::OriginalMediaLimits;
use deadpan_store::source_registration::{PrimaryGeometryAdoption, SourceRegistration};
use deadpan_store::{AccessMode, ProjectStore, StoreError};
use serde::Deserialize;

use crate::live_project::preparation::{self, PreparationCommand, SourceStreams as Streams};
use crate::{CliError, read_request, write_json};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RegistrationEnvelope {
    protocol: u32,
    registration: SourceRegistration,
    streams: Streams,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GeometryEnvelope {
    protocol: u32,
    adoption: PrimaryGeometryAdoption,
}

pub(super) fn adopt_geometry(
    package: &Path,
    request: &Path,
    dry_run: bool,
) -> Result<(), CliError> {
    let envelope: GeometryEnvelope = serde_json::from_str(&read_request(request)?)?;
    if envelope.protocol != 1 {
        return Err(CliError::Protocol(envelope.protocol));
    }
    write_json(&crate::live_project::dispatch_short(
        package,
        None,
        crate::live_project::ShortOperation::AdoptPrimaryGeometry {
            adoption: envelope.adoption,
            dry_run,
        },
    )?)
}

pub(super) fn run(package: &Path, request: &Path, dry_run: bool) -> Result<(), CliError> {
    let envelope: RegistrationEnvelope = serde_json::from_str(&read_request(request)?)?;
    if envelope.protocol != 1 {
        return Err(CliError::Protocol(envelope.protocol));
    }
    register(package, envelope.registration, envelope.streams, dry_run)
}

/// `project insert-original <project> --parent <id> --index <N> --expected
/// <revision> [--dry-run]`: reuse the project's whole ready Original as a new
/// beat, exactly as `,i` / `:insert` does. The registration the app derives
/// from its session (same content, asset, label and qualified streams, a
/// fresh node) is derived here from the stored qualification, then admitted
/// through `register-source`'s path, which reuses the existing qualification.
pub(super) fn run_insert_original(arguments: &[&str]) -> Result<(), CliError> {
    let usage = || {
        CliError::Usage(
            "usage: project insert-original <project.deadpan> --parent <node-id> --index <N> --expected <revision> [--dry-run]"
                .into(),
        )
    };
    let (package, parent, index, expected, dry_run) = match arguments {
        [
            package,
            "--parent",
            parent,
            "--index",
            index,
            "--expected",
            expected,
        ] => (package, parent, index, expected, false),
        [
            package,
            "--parent",
            parent,
            "--index",
            index,
            "--expected",
            expected,
            "--dry-run",
        ] => (package, parent, index, expected, true),
        _ => return Err(usage()),
    };
    let package = Path::new(package);
    let index: usize = index.parse().map_err(|_| usage())?;
    let expected = deadpan_core::RevisionId::new(*expected)?;
    let (registration, streams) = {
        let store = ProjectStore::open(package, AccessMode::ReadOnly)?;
        let document = store.snapshot()?;
        let Some(deadpan_store::single_source::SingleSourceState::Ready {
            asset,
            qualification,
            ..
        }) = store.single_source_state()?
        else {
            return Err(crate::live_project::LiveError::new(
                "OriginalUnavailable",
                "The project has no ready Original to reuse",
            )
            .into());
        };
        let record = document.assets().get(&asset).ok_or_else(|| {
            crate::live_project::LiveError::new(
                "OriginalUnavailable",
                "The Original is not registered in the current revision",
            )
        })?;
        let receipt = store.source_qualification(&qualification)?;
        let streams = match receipt.snapshot().audio() {
            Some(audio) => Streams::VideoAndAudio {
                audio_stream: audio.stream().stream_index,
            },
            None => Streams::VideoOnly {},
        };
        let registration = SourceRegistration {
            expected_revision: expected,
            new_revision: crate::new_revision()?,
            original: receipt.original().content().clone(),
            new_asset_id: asset,
            label: record.label.clone(),
            insertion: Some(deadpan_store::source_registration::SourceInsertionRequest {
                parent: deadpan_core::NodeId::new(*parent)?,
                index,
                node: deadpan_core::NodeId::new(uuid::Uuid::new_v4().to_string())?,
                label: record.label.clone(),
                purpose: deadpan_store::source_registration::SourceInsertionPurpose::Primary,
            }),
        };
        (registration, streams)
    };
    register(package, registration, streams, dry_run)
}

fn register(
    package: &Path,
    input: SourceRegistration,
    streams: Streams,
    dry_run: bool,
) -> Result<(), CliError> {
    let mut store = match ProjectStore::open(
        package,
        if dry_run {
            AccessMode::ReadOnly
        } else {
            AccessMode::ReadWrite
        },
    ) {
        Ok(store) => store,
        Err(StoreError::AlreadyOpen) if !dry_run => {
            return preparation::run(
                package,
                PreparationCommand::Register {
                    registration: input,
                    streams,
                },
            );
        }
        Err(error) => return Err(error.into()),
    };
    store.set_generation_context_resolver(std::sync::Arc::new(
        crate::generation_context::BoundaryContextResolver::default(),
    ));
    let current = store.snapshot()?;
    if current.revision_id() != &input.expected_revision {
        return Err(StoreError::RevisionConflict {
            expected: input.expected_revision.as_str().into(),
            current: current.revision_id().as_str().into(),
        }
        .into());
    }
    let cancelled = AtomicBool::new(false);
    let limits = OriginalMediaLimits::default();
    let mut original = store.snapshot_original(&input.original, limits, &cancelled)?;
    let identity = SourceContentIdentity::new(
        original.record().sha256(),
        original.record().object().byte_length(),
    )
    .map_err(|error| StoreError::SourceQualification(error.into()))?;
    let video_limits = SourceSessionLimits::default();
    let audio_limits = AudioSessionLimits::default();
    let maximum_bytes = video_limits
        .decode
        .max_input_bytes
        .min(audio_limits.decode.max_input_bytes);
    let bytes = VerifiedSourceInput::copy_verified(
        &mut original,
        identity,
        maximum_bytes,
        video_limits.opening_timeout,
        &cancelled,
    )?;
    let video = match streams {
        Streams::VideoOnly {} | Streams::VideoAndAudio { .. } => Some(SourceSession::open_input(
            bytes.clone(),
            input.new_asset_id.clone(),
            video_limits,
            &cancelled,
        )?),
        Streams::AudioOnly { .. } => None,
    };
    let audio = match streams {
        Streams::VideoAndAudio {
            audio_stream: stream,
        }
        | Streams::AudioOnly { stream, .. } => Some(AudioSession::open_input(
            bytes,
            stream,
            audio_limits,
            &cancelled,
        )?),
        Streams::VideoOnly {} => None,
    };
    let decoded = DecodedSourceQualification::for_registration(
        video.as_ref(),
        audio.as_ref(),
        streams.interpretation(),
    )
    .map_err(StoreError::from)?;
    if dry_run {
        let preview = store.preview_source_registration(&input, &decoded, limits, &cancelled)?;
        write_json(&serde_json::json!({"protocol":1,"committed":false,"preview":preview}))
    } else {
        let outcome = store.register_source(&input, &decoded, None, limits, &cancelled)?;
        write_json(
            &serde_json::json!({"protocol":1,"committed":outcome.commit.is_some(),"outcome":outcome}),
        )
    }
}
