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

use crate::{CliError, read_request, write_json};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RegistrationEnvelope {
    protocol: u32,
    registration: SourceRegistration,
    streams: Streams,
}

/// Selecting video alone is deliberate; no failed audio decode falls back to it.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum Streams {
    VideoOnly {},
    VideoAndAudio { audio_stream: u32 },
    AudioOnly { stream: u32 },
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
    let mut store = ProjectStore::open(
        package,
        if dry_run {
            AccessMode::ReadOnly
        } else {
            AccessMode::ReadWrite
        },
    )?;
    if dry_run {
        let edit = store.preview_primary_geometry(&envelope.adoption)?;
        write_json(&serde_json::json!({"protocol":1,"committed":false,"edit":edit}))
    } else {
        let outcome = store.adopt_primary_geometry(&envelope.adoption, None)?;
        write_json(&serde_json::json!({"protocol":1,"committed":true,"outcome":outcome}))
    }
}

pub(super) fn run(package: &Path, request: &Path, dry_run: bool) -> Result<(), CliError> {
    let envelope: RegistrationEnvelope = serde_json::from_str(&read_request(request)?)?;
    if envelope.protocol != 1 {
        return Err(CliError::Protocol(envelope.protocol));
    }
    let input = envelope.registration;
    let mut store = ProjectStore::open(
        package,
        if dry_run {
            AccessMode::ReadOnly
        } else {
            AccessMode::ReadWrite
        },
    )?;
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
    let video = match envelope.streams {
        Streams::VideoOnly {} | Streams::VideoAndAudio { .. } => Some(SourceSession::open_input(
            bytes.clone(),
            input.new_asset_id.clone(),
            video_limits,
            &cancelled,
        )?),
        Streams::AudioOnly { .. } => None,
    };
    let audio = match envelope.streams {
        Streams::VideoAndAudio {
            audio_stream: stream,
        }
        | Streams::AudioOnly { stream } => Some(AudioSession::open_input(
            bytes,
            stream,
            audio_limits,
            &cancelled,
        )?),
        Streams::VideoOnly {} => None,
    };
    let decoded = DecodedSourceQualification::from_sessions(video.as_ref(), audio.as_ref())
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
