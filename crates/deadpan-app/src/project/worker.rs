use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread::JoinHandle;
use std::time::Duration;

use deadpan_core::AssetId;
use deadpan_media::audio_session::{AudioSession, AudioSessionLimits};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_media::source_input::VerifiedSourceInput;
use deadpan_media::source_qualification::DecodedSourceQualification;
use deadpan_media::source_session::{SourceSession, SourceSessionLimits};
use deadpan_store::original_media::{
    OriginalImportHandle, OriginalMediaLimits, OriginalMediaRecord, OriginalOwnership,
    PreparedOriginalRetention,
};
use deadpan_store::source_registration::PreparedSourceRegistration;

use super::ImportMedia;

/// Default decoder admission allows at most 64 GiB. Reject larger originals before copying.
pub(super) fn original_limits() -> OriginalMediaLimits {
    let bytes = SourceSessionLimits::default()
        .decode
        .max_input_bytes
        .min(AudioSessionLimits::default().decode.max_input_bytes);
    OriginalMediaLimits::new(bytes, Duration::from_secs(300)).expect("bounded decoder limits")
}

#[derive(Clone, Copy)]
pub(super) enum Streams {
    Import(ImportMedia),
    Exact { video: bool, audio: Option<u32> },
}

pub(super) enum Work {
    Retain {
        path: PathBuf,
        ownership: OriginalOwnership,
    },
    Qualify {
        record: OriginalMediaRecord,
        streams: Streams,
    },
}

pub(super) struct Job {
    pub id: u64,
    pub handle: OriginalImportHandle,
    pub cancelled: Arc<AtomicBool>,
    pub work: Work,
}

pub(super) enum Prepared {
    Retained(Box<PreparedOriginalRetention>),
    Qualified(Box<PreparedSourceRegistration>),
}

pub(super) struct Reply {
    pub id: u64,
    pub result: Result<Prepared, String>,
}

type Worker = (SyncSender<Job>, Receiver<Reply>, JoinHandle<()>);

pub(super) fn spawn() -> io::Result<Worker> {
    let (sender, receiver) = mpsc::sync_channel::<Job>(1);
    let (replies, results) = mpsc::sync_channel(1);
    let worker = std::thread::Builder::new()
        .name("deadpan-import".into())
        .spawn(move || {
            while let Ok(job) = receiver.recv() {
                let result = prepare(&job);
                if replies.send(Reply { id: job.id, result }).is_err() {
                    break;
                }
            }
        })?;
    Ok((sender, results, worker))
}

pub(super) fn prepare(job: &Job) -> Result<Prepared, String> {
    if job.cancelled.load(Ordering::Acquire) {
        return Err("Import cancelled".into());
    }
    match &job.work {
        Work::Retain { path, ownership } => job
            .handle
            .prepare_retention(path, ownership.clone(), original_limits(), &job.cancelled)
            .map(Box::new)
            .map(Prepared::Retained)
            .map_err(|error| error.to_string()),
        Work::Qualify { record, streams } => qualify(job, record, *streams)
            .map(Box::new)
            .map(Prepared::Qualified)
            .map_err(|error| error.to_string()),
    }
}

fn qualify(
    job: &Job,
    record: &OriginalMediaRecord,
    streams: Streams,
) -> Result<PreparedSourceRegistration, Box<dyn std::error::Error>> {
    let mut original = job
        .handle
        .snapshot_original(record, original_limits(), &job.cancelled)?;
    let video_limits = SourceSessionLimits::default();
    let audio_limits = AudioSessionLimits::default();
    let input = VerifiedSourceInput::copy_verified(
        &mut original,
        SourceContentIdentity::new(record.sha256(), record.object().byte_length())?,
        video_limits
            .decode
            .max_input_bytes
            .min(audio_limits.decode.max_input_bytes),
        video_limits.opening_timeout,
        &job.cancelled,
    )?;
    let needs_video = matches!(
        streams,
        Streams::Import(ImportMedia::Video) | Streams::Exact { video: true, .. }
    );
    let video = needs_video
        .then(|| {
            SourceSession::open_input(
                input.clone(),
                AssetId::new("native-import").expect("static source identity"),
                video_limits,
                &job.cancelled,
            )
        })
        .transpose()?;
    let audio_stream = match streams {
        Streams::Import(ImportMedia::Video) => video.as_ref().and_then(|video| {
            video
                .info()
                .audio_streams
                .first()
                .map(|stream| stream.stream_index)
        }),
        Streams::Import(ImportMedia::Audio { stream }) => Some(stream),
        Streams::Import(ImportMedia::FirstAudio) => None,
        Streams::Exact { audio, .. } => audio,
    };
    let audio = if matches!(streams, Streams::Import(ImportMedia::FirstAudio)) {
        Some(AudioSession::open_first_input(
            input,
            audio_limits,
            &job.cancelled,
        )?)
    } else {
        audio_stream
            .map(|stream| AudioSession::open_input(input, stream, audio_limits, &job.cancelled))
            .transpose()?
    };
    let decoded = DecodedSourceQualification::from_sessions(video.as_ref(), audio.as_ref())?;
    Ok(PreparedSourceRegistration::from_decoded(
        original,
        &decoded,
        &job.cancelled,
    )?)
}
