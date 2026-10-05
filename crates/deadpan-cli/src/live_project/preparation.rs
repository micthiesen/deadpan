//! Bounded admission and worker preparation for one live project owner.
//!
//! Wire commands and observations carry no admission capability. The private
//! work/result wrappers bind actual store handles to the complete captured
//! command; only the owning writer can publish their result.

use std::path::{Component, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use deadpan_core::{AssetId, ProjectId, RevisionId, SourceQualificationId};
use deadpan_media::audio_index::AudioLayoutInterpretation;
use deadpan_media::audio_session::{AudioSession, AudioSessionLimits};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_media::source_input::VerifiedSourceInput;
use deadpan_media::source_qualification::DecodedSourceQualification;
use deadpan_media::source_session::{SourceSession, SourceSessionLimits};
use deadpan_store::checkpoint::{CheckpointHandle, CheckpointLimits, PreparedCheckpoint};
use deadpan_store::original_media::{
    LinkedOriginal, OriginalContentId, OriginalImportHandle, OriginalMediaError,
    OriginalMediaLimits, OriginalMediaRecord, OriginalOwnership, PreparedOriginalRelink,
    PreparedOriginalRetention, PreparedOriginalSnapshot,
};
use deadpan_store::source_registration::{PreparedSourceRegistration, SourceRegistration};
use deadpan_store::{ProjectStore, StoreError};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use super::LiveError;

mod client;
pub(crate) use client::run;
#[cfg(test)]
mod tests;

pub const MAX_COMMAND_BYTES: usize = 64 * 1024;
const MAX_PATH_BYTES: usize = 16 * 1024;
const MAX_BOOKMARK_BYTES: usize = 16 * 1024;
const MAX_LABEL_BYTES: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PreparationCommand {
    Retain {
        path: PathBuf,
        ownership: PreparationOwnership,
    },
    Relink {
        content: OriginalContentId,
        expected_version: u64,
        location: LinkedOriginal,
    },
    Register {
        registration: SourceRegistration,
        streams: SourceStreams,
    },
    Checkpoint {},
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "ownership", rename_all = "snake_case", deny_unknown_fields)]
pub enum PreparationOwnership {
    Managed {},
    Linked { bookmark: Option<Vec<u8>> },
}
impl From<OriginalOwnership> for PreparationOwnership {
    fn from(ownership: OriginalOwnership) -> Self {
        match ownership {
            OriginalOwnership::Managed => Self::Managed {},
            OriginalOwnership::Linked { bookmark } => Self::Linked { bookmark },
        }
    }
}
impl PreparationOwnership {
    fn original(&self) -> OriginalOwnership {
        match self {
            Self::Managed {} => OriginalOwnership::Managed,
            Self::Linked { bookmark } => OriginalOwnership::Linked {
                bookmark: bookmark.clone(),
            },
        }
    }
}

/// Exact stream selection, shared with the closed-project CLI envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SourceStreams {
    VideoOnly {},
    VideoAndAudio {
        audio_stream: u32,
    },
    /// `interpretation` is the explicit speaker reading (`mono` or
    /// `stereo_left_right`) of channels that declare no layout. Such a sound
    /// is refused without it; a declared layout needs and keeps none.
    AudioOnly {
        stream: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        interpretation: Option<AudioLayoutInterpretation>,
    },
}

impl SourceStreams {
    pub fn interpretation(self) -> Option<AudioLayoutInterpretation> {
        match self {
            Self::AudioOnly { interpretation, .. } => interpretation,
            Self::VideoOnly {} | Self::VideoAndAudio { .. } => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparationTarget {
    pub operation_id: Uuid,
    pub cancellation_token: Uuid,
}
impl PreparationTarget {
    pub fn fresh() -> Self {
        Self {
            operation_id: Uuid::new_v4(),
            cancellation_token: Uuid::new_v4(),
        }
    }
    pub fn validate(&self) -> Result<(), LiveError> {
        if self.operation_id.is_nil() || self.cancellation_token.is_nil() {
            return Err(LiveError::new(
                "HostProtocolInvalid",
                "Preparation identities must be nonzero UUIDs",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PreparationReceipt {
    Retained {
        content: OriginalContentId,
        location_version: u64,
    },
    Relinked {
        content: OriginalContentId,
        location_version: u64,
    },
    Registered {
        asset: AssetId,
        qualification: SourceQualificationId,
    },
    Checkpoint {
        path: PathBuf,
        project_id: ProjectId,
        revision_id: RevisionId,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum PreparationState {
    Preparing {},
    AwaitingCommit {},
    Cancelling {},
    Completed {
        output: Value,
        receipt: PreparationReceipt,
        committed_revision: Option<RevisionId>,
        inventory_changed: bool,
        completion_error: Option<LiveError>,
        refresh_error: Option<String>,
    },
    Failed {
        error: LiveError,
    },
    Cancelled {},
}
impl PreparationState {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed { .. } | Self::Failed { .. } | Self::Cancelled {}
        )
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparationStatus {
    pub target: PreparationTarget,
    pub state: PreparationState,
}
impl PreparationStatus {
    pub fn is_terminal(&self) -> bool {
        self.state.is_terminal()
    }
}

pub struct PreparationCommit {
    pub output: Value,
    pub receipt: PreparationReceipt,
    pub committed_revision: Option<RevisionId>,
    pub inventory_changed: bool,
    pub completion_error: Option<LiveError>,
}

pub struct PreparationWork {
    command: PreparationCommand,
    work: Work,
}
enum Work {
    Original {
        handle: OriginalImportHandle,
        record: Option<OriginalMediaRecord>,
    },
    Checkpoint(CheckpointHandle),
}
pub struct PreparedOperation {
    command: PreparationCommand,
    prepared: Prepared,
}
enum Prepared {
    Retained(PreparedOriginalRetention),
    Relinked(PreparedOriginalRelink),
    Registered(Box<PreparedSourceRegistration>),
    Checkpoint(PreparedCheckpoint),
}

impl PreparationCommand {
    pub fn validate(&self) -> Result<(), LiveError> {
        if serde_json::to_vec(self).map_err(LiveError::json)?.len() > MAX_COMMAND_BYTES {
            return Err(LiveError::new(
                "HostPreparationLimit",
                "Preparation command exceeds 64 KiB",
            ));
        }
        match self {
            Self::Retain { path, ownership } => {
                validate_path(path)?;
                if let PreparationOwnership::Linked { bookmark } = ownership {
                    validate_bookmark(bookmark.as_deref())?;
                }
            }
            Self::Relink {
                expected_version,
                location,
                ..
            } => {
                validate_path(location.path())?;
                validate_bookmark(location.bookmark())?;
                if *expected_version == 0 || *expected_version > i64::MAX as u64 {
                    return Err(LiveError::new(
                        "HostProtocolInvalid",
                        "Location version must be a positive database integer",
                    ));
                }
            }
            Self::Register { registration, .. } => {
                validate_label(&registration.label)?;
                if let Some(insertion) = &registration.insertion {
                    validate_label(&insertion.label)?;
                }
            }
            Self::Checkpoint {} => {}
        }
        Ok(())
    }
}
fn validate_path(path: &std::path::Path) -> Result<(), LiveError> {
    if !path.is_absolute()
        || path.as_os_str().as_encoded_bytes().len() > MAX_PATH_BYTES
        || path.as_os_str().as_encoded_bytes().contains(&0)
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(LiveError::new(
            "HostProtocolInvalid",
            "Preparation path must be bounded, absolute and contain no parent traversal",
        ));
    }
    Ok(())
}
fn validate_bookmark(bookmark: Option<&[u8]>) -> Result<(), LiveError> {
    if bookmark.is_some_and(|bytes| bytes.len() > MAX_BOOKMARK_BYTES) {
        return Err(LiveError::new(
            "HostPreparationLimit",
            "Bookmark exceeds 16 KiB",
        ));
    }
    Ok(())
}
fn validate_label(label: &str) -> Result<(), LiveError> {
    if label.is_empty() || label.len() > MAX_LABEL_BYTES || label.contains('\0') {
        return Err(LiveError::new(
            "HostProtocolInvalid",
            "Source label is empty or exceeds its limit",
        ));
    }
    Ok(())
}
/// Largest original both default decoders admit.
pub(crate) fn original_byte_limit() -> u64 {
    SourceSessionLimits::default()
        .decode
        .max_input_bytes
        .min(AudioSessionLimits::default().decode.max_input_bytes)
}
pub(crate) fn original_limits() -> OriginalMediaLimits {
    OriginalMediaLimits::new(original_byte_limit(), Duration::from_secs(300))
        .expect("bounded decoder defaults")
}
fn record(
    store: &ProjectStore,
    content: &OriginalContentId,
) -> Result<OriginalMediaRecord, LiveError> {
    store
        .original_record(content)
        .map_err(LiveError::store)?
        .ok_or_else(|| LiveError::store(OriginalMediaError::MissingRecord.into()))
}

/// Short writer-thread admission. No file hashing, decoder or database backup.
pub fn admit(
    store: &mut ProjectStore,
    command: &PreparationCommand,
) -> Result<PreparationWork, LiveError> {
    command.validate()?;
    let work = match command {
        PreparationCommand::Checkpoint {} => Work::Checkpoint(
            store
                .checkpoint_handle()
                .map_err(|error| LiveError::new(error.code(), error))?,
        ),
        command => {
            let handle = store.original_import_handle().map_err(LiveError::store)?;
            let record = match command {
                PreparationCommand::Relink {
                    content,
                    expected_version,
                    ..
                } => {
                    let record = record(store, content)?;
                    if record.version() != *expected_version {
                        return Err(LiveError::store(
                            OriginalMediaError::VersionConflict {
                                current: record.version(),
                            }
                            .into(),
                        ));
                    }
                    Some(record)
                }
                PreparationCommand::Register { registration, .. } => {
                    let current = store.snapshot().map_err(LiveError::store)?;
                    if current.revision_id() != &registration.expected_revision {
                        return Err(LiveError::store(StoreError::RevisionConflict {
                            expected: registration.expected_revision.as_str().into(),
                            current: current.revision_id().as_str().into(),
                        }));
                    }
                    Some(record(store, &registration.original)?)
                }
                _ => None,
            };
            Work::Original { handle, record }
        }
    };
    Ok(PreparationWork {
        command: command.clone(),
        work,
    })
}

/// Heavy worker work; contains neither a store nor a writable DB connection.
pub fn prepare(
    work: PreparationWork,
    cancelled: &AtomicBool,
) -> Result<PreparedOperation, LiveError> {
    if cancelled.load(Ordering::Acquire) {
        return Err(LiveError::new(
            "PreparationCancelled",
            "Preparation cancelled before work",
        ));
    }
    let prepared = match (&work.command, work.work) {
        (PreparationCommand::Retain { path, ownership }, Work::Original { handle, .. }) => {
            Prepared::Retained(
                handle
                    .prepare_retention(path, ownership.original(), original_limits(), cancelled)
                    .map_err(LiveError::store)?,
            )
        }
        (
            PreparationCommand::Relink {
                expected_version,
                location,
                ..
            },
            Work::Original {
                handle,
                record: Some(record),
            },
        ) => Prepared::Relinked(
            handle
                .prepare_relink(
                    &record,
                    *expected_version,
                    location.clone(),
                    original_limits(),
                    cancelled,
                )
                .map_err(LiveError::store)?,
        ),
        (
            PreparationCommand::Register {
                registration,
                streams,
            },
            Work::Original {
                handle,
                record: Some(record),
            },
        ) => Prepared::Registered(Box::new(qualify(
            handle,
            &record,
            registration,
            *streams,
            cancelled,
        )?)),
        (PreparationCommand::Checkpoint {}, Work::Checkpoint(handle)) => Prepared::Checkpoint(
            handle
                .prepare(CheckpointLimits::default(), cancelled)
                .map_err(|error| LiveError::new(error.code(), error))?,
        ),
        _ => {
            return Err(LiveError::new(
                "HostProtocolInvalid",
                "Preparation work does not match its captured command",
            ));
        }
    };
    Ok(PreparedOperation {
        command: work.command,
        prepared,
    })
}

fn qualify(
    handle: OriginalImportHandle,
    record: &OriginalMediaRecord,
    registration: &SourceRegistration,
    streams: SourceStreams,
    cancelled: &AtomicBool,
) -> Result<PreparedSourceRegistration, LiveError> {
    let (original, bytes) = verified_input(&handle, record, cancelled)?;
    let video = match streams {
        SourceStreams::VideoOnly {} | SourceStreams::VideoAndAudio { .. } => Some(open_video(
            &bytes,
            registration.new_asset_id.clone(),
            cancelled,
        )?),
        SourceStreams::AudioOnly { .. } => None,
    };
    let audio = match streams {
        SourceStreams::VideoAndAudio {
            audio_stream: stream,
        }
        | SourceStreams::AudioOnly { stream, .. } => Some(open_audio(bytes, stream, cancelled)?),
        SourceStreams::VideoOnly {} => None,
    };
    finish_qualification(
        original,
        video.as_ref(),
        audio.as_ref(),
        streams.interpretation(),
        cancelled,
    )
}

/// Qualify an Original the way native creation chooses its streams: the
/// primary picture plus the container's first audio track when one exists.
pub(crate) fn qualify_primary(
    handle: &OriginalImportHandle,
    record: &OriginalMediaRecord,
    asset: AssetId,
    cancelled: &AtomicBool,
) -> Result<PreparedSourceRegistration, LiveError> {
    let (original, bytes) = verified_input(handle, record, cancelled)?;
    let video = open_video(&bytes, asset, cancelled)?;
    let audio = video
        .info()
        .audio_streams
        .first()
        .map(|stream| open_audio(bytes, stream.stream_index, cancelled))
        .transpose()?;
    finish_qualification(original, Some(&video), audio.as_ref(), None, cancelled)
}

fn verified_input(
    handle: &OriginalImportHandle,
    record: &OriginalMediaRecord,
    cancelled: &AtomicBool,
) -> Result<(PreparedOriginalSnapshot, VerifiedSourceInput), LiveError> {
    let mut original = handle
        .snapshot_original(record, original_limits(), cancelled)
        .map_err(LiveError::store)?;
    let video_limits = SourceSessionLimits::default();
    let audio_limits = AudioSessionLimits::default();
    let identity = SourceContentIdentity::new(record.sha256(), record.object().byte_length())
        .map_err(|error| LiveError::store(StoreError::SourceQualification(error.into())))?;
    let bytes = VerifiedSourceInput::copy_verified(
        &mut original,
        identity,
        video_limits
            .decode
            .max_input_bytes
            .min(audio_limits.decode.max_input_bytes),
        video_limits.opening_timeout,
        cancelled,
    )
    .map_err(|error| LiveError::new("SourceSnapshotFailed", error))?;
    Ok((original, bytes))
}

fn open_video(
    bytes: &VerifiedSourceInput,
    asset: AssetId,
    cancelled: &AtomicBool,
) -> Result<SourceSession, LiveError> {
    SourceSession::open_input(
        bytes.clone(),
        asset,
        SourceSessionLimits::default(),
        cancelled,
    )
    .map_err(|error| LiveError::new("SourceVideoDecodeFailed", error))
}

fn open_audio(
    bytes: VerifiedSourceInput,
    stream: u32,
    cancelled: &AtomicBool,
) -> Result<AudioSession, LiveError> {
    AudioSession::open_input(bytes, stream, AudioSessionLimits::default(), cancelled)
        .map_err(|error| LiveError::new("SourceAudioDecodeFailed", error))
}

fn finish_qualification(
    original: PreparedOriginalSnapshot,
    video: Option<&SourceSession>,
    audio: Option<&AudioSession>,
    interpretation: Option<AudioLayoutInterpretation>,
    cancelled: &AtomicBool,
) -> Result<PreparedSourceRegistration, LiveError> {
    let decoded = DecodedSourceQualification::for_registration(video, audio, interpretation)
        .map_err(|error| LiveError::store(error.into()))?;
    PreparedSourceRegistration::from_decoded(original, &decoded, cancelled)
        .map_err(LiveError::store)
}

/// Publish only the exact admitted command with the owning store's transaction.
/// Return the operational receipt separately from any authored commit receipt.
pub fn commit(
    store: &mut ProjectStore,
    command: &PreparationCommand,
    prepared: PreparedOperation,
    cancelled: &AtomicBool,
) -> Result<PreparationCommit, LiveError> {
    if &prepared.command != command {
        return Err(LiveError::new(
            "HostPreparationChanged",
            "Prepared result belongs to another command",
        ));
    }
    let result = match (command, prepared.prepared) {
        (PreparationCommand::Retain { .. }, Prepared::Retained(prepared)) => {
            let outcome = store
                .retain_prepared_original(&prepared, cancelled)
                .map_err(LiveError::store)?;
            PreparationCommit {
                receipt: PreparationReceipt::Retained {
                    content: outcome.record.object().content().clone(),
                    location_version: outcome.record.version(),
                },
                output: json!({"protocol":1,"retained_original":outcome,"authored_asset_registered":false}),
                committed_revision: None,
                inventory_changed: true,
                completion_error: None,
            }
        }
        (PreparationCommand::Relink { .. }, Prepared::Relinked(prepared)) => {
            let record = store
                .relink_prepared_original(&prepared, cancelled)
                .map_err(LiveError::store)?;
            PreparationCommit {
                receipt: PreparationReceipt::Relinked {
                    content: record.object().content().clone(),
                    location_version: record.version(),
                },
                output: json!({"protocol":1,"relinked_original":record}),
                committed_revision: None,
                inventory_changed: true,
                completion_error: None,
            }
        }
        (PreparationCommand::Register { registration, .. }, Prepared::Registered(prepared)) => {
            let outcome = store
                .register_prepared_source(registration, &prepared, None, cancelled)
                .map_err(LiveError::store)?;
            PreparationCommit {
                receipt: PreparationReceipt::Registered {
                    asset: outcome.asset_id.clone(),
                    qualification: outcome.qualification.clone(),
                },
                committed_revision: outcome
                    .commit
                    .as_ref()
                    .map(|commit| commit.revision_id.clone()),
                inventory_changed: false,
                completion_error: None,
                output: json!({"protocol":1,"committed":outcome.commit.is_some(),"outcome":outcome}),
            }
        }
        (PreparationCommand::Checkpoint {}, Prepared::Checkpoint(prepared)) => {
            checkpoint_result(store.publish_prepared_checkpoint(prepared, cancelled))?
        }
        _ => {
            return Err(LiveError::new(
                "HostPreparationChanged",
                "Prepared result kind does not match the command",
            ));
        }
    };
    Ok(result)
}

fn checkpoint_result(
    result: Result<
        deadpan_store::checkpoint::CheckpointReceipt,
        deadpan_store::checkpoint::CheckpointError,
    >,
) -> Result<PreparationCommit, LiveError> {
    let (receipt, completion_error) = match result {
        Ok(receipt) => (receipt, None),
        Err(error) => match error.published_receipt() {
            Some(receipt) => (receipt.clone(), Some(LiveError::new(error.code(), error))),
            None => return Err(LiveError::new(error.code(), error)),
        },
    };
    Ok(PreparationCommit {
        output: json!({"protocol":1,"database_checkpoint":receipt.path}),
        receipt: PreparationReceipt::Checkpoint {
            path: receipt.path,
            project_id: receipt.project_id,
            revision_id: receipt.revision_id,
        },
        committed_revision: None,
        inventory_changed: false,
        completion_error,
    })
}
