//! Immutable, revision-scoped source admission. Only the preparation worker
//! opens media. Private PCM survives original-path changes; every cache hit
//! still checks the captured receipt and complete authored asset contract.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use deadpan_audio::{AudioSourceProvider, PreparationError, PreparedSource};
use deadpan_core::{AssetId, AssetRecord, ProjectDocument, ProjectId, RevisionId};
use deadpan_media::audio_session::{AudioSession, AudioSessionLimits};
use deadpan_store::original_media::{
    OriginalImportHandle, OriginalMediaLimits, OriginalMediaRecord,
};
use deadpan_store::source_registration::SourceQualificationReceipt;

#[derive(Clone)]
pub struct SourceEntry {
    pub receipt: Arc<SourceQualificationReceipt>,
    pub original: OriginalMediaRecord,
}

/// A capability issued by the live project service, with receipts resolved for
/// this exact document revision. It contains no SQLite connection or writer.
pub struct Snapshot {
    pub session: u64,
    pub document: Arc<ProjectDocument>,
    pub sources: BTreeMap<AssetId, SourceEntry>,
    pub originals: OriginalImportHandle,
}

/// Aggregate physical PCM on disk, separate from the DSP residency limit.
const MAX_CACHE_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_CACHED_SOURCES: usize = 16;
const MAX_CACHE_INDEX_FRAMES: u64 = 1_000_000;

struct CachedSource {
    prepared: PreparedSource,
    bytes: u64,
    index_frames: u64,
}

pub(crate) struct Sources {
    snapshot: Arc<Snapshot>,
    cache: BTreeMap<AssetId, CachedSource>,
    recency: VecDeque<AssetId>,
    cache_bytes: u64,
    cache_index_frames: u64,
}

impl Sources {
    pub(crate) fn new(snapshot: Arc<Snapshot>) -> Self {
        Self {
            snapshot,
            cache: BTreeMap::new(),
            recency: VecDeque::new(),
            cache_bytes: 0,
            cache_index_frames: 0,
        }
    }
    pub(crate) fn matches(&self, snapshot: &Snapshot) -> bool {
        self.snapshot.session == snapshot.session
            && Arc::ptr_eq(&self.snapshot.document, &snapshot.document)
            && self.snapshot.sources.len() == snapshot.sources.len()
            && self.snapshot.sources.iter().all(|(asset, before)| {
                snapshot.sources.get(asset).is_some_and(|after| {
                    Arc::ptr_eq(&before.receipt, &after.receipt)
                        && before.original == after.original
                })
            })
    }
}

impl AudioSourceProvider for Sources {
    fn source_for_context(
        &mut self,
        project: &ProjectId,
        revision: &RevisionId,
        asset: &AssetId,
        expected: &AssetRecord,
        cancelled: &AtomicBool,
    ) -> Result<&PreparedSource, PreparationError> {
        if self.snapshot.document.assets().get(asset) != Some(expected) {
            return Err(PreparationError::IndexMismatch);
        }
        self.source(project, revision, asset, cancelled)
    }

    fn source(
        &mut self,
        project: &ProjectId,
        revision: &RevisionId,
        asset: &AssetId,
        cancelled: &AtomicBool,
    ) -> Result<&PreparedSource, PreparationError> {
        check_cancel(cancelled)?;
        let document = &self.snapshot.document;
        if project != document.project_id() || revision != document.revision_id() {
            return Err(unavailable(
                "audio request differs from the captured project revision",
            ));
        }
        let entry =
            self.snapshot.sources.get(asset).ok_or_else(|| {
                unavailable("source receipt is absent from the captured revision")
            })?;
        let authored = document
            .assets()
            .get(asset)
            .ok_or_else(|| unavailable("audio asset is absent from the captured revision"))?;
        let receipt = &entry.receipt;
        if authored.source_qualification.as_ref() != Some(receipt.id())
            || authored.content_hash != receipt.original().content().to_string()
            || entry.original.object() != receipt.original()
        {
            return Err(PreparationError::IndexMismatch);
        }
        let expected = receipt
            .snapshot()
            .audio()
            .ok_or_else(|| unavailable("source has no qualified audio index"))?;
        if entry.original.sha256() != expected.content().sha256()
            || entry.original.object().byte_length() != expected.content().byte_length()
        {
            return Err(PreparationError::IndexMismatch);
        }
        if self.cache.contains_key(asset) {
            check_cancel(cancelled)?;
            mark_recent(&mut self.recency, asset);
        } else {
            // Decoded physical samples include priming and padding. The opener
            // receives this exact reservation and must reproduce the receipt.
            let bytes = expected
                .decoded_samples()
                .checked_mul(u64::from(expected.stream().channel_layout.channels()))
                .and_then(|samples| samples.checked_mul(4))
                .filter(|bytes| *bytes > 0)
                .ok_or_else(|| unavailable("invalid physical PCM cache size"))?;
            let index_frames = u64::try_from(expected.frames().len())
                .map_err(|_| unavailable("invalid audio index cache size"))?;
            validate_source_capacity(bytes, index_frames)?;
            let audio_limits = AudioSessionLimits {
                maximum_cache_bytes: bytes,
                maximum_index_frames: expected.frames().len(),
                opening_timeout: Duration::from_secs(30),
                ..AudioSessionLimits::default()
            };
            let original_limits = OriginalMediaLimits::new(
                audio_limits.decode.max_input_bytes,
                Duration::from_secs(15),
            )
            .map_err(unavailable)?;
            let mut original = self
                .snapshot
                .originals
                .snapshot_original(&entry.original, original_limits, cancelled)
                .map_err(unavailable)?;
            if original.record() != &entry.original {
                return Err(PreparationError::IndexMismatch);
            }
            check_cancel(cancelled)?;
            // Validate and snapshot originals before evicting a usable entry.
            // A bad cold source must not disturb the current resident set.
            make_room(
                &mut self.cache,
                &mut self.recency,
                &mut self.cache_bytes,
                &mut self.cache_index_frames,
                bytes,
                index_frames,
            )?;
            let reserved_bytes = self
                .cache_bytes
                .checked_add(bytes)
                .ok_or_else(|| unavailable("source cache byte accounting overflow"))?;
            let reserved_frames = self
                .cache_index_frames
                .checked_add(index_frames)
                .ok_or_else(|| unavailable("source cache index accounting overflow"))?;
            let session = AudioSession::open_verified(
                &mut original,
                expected.content(),
                expected.stream().stream_index,
                audio_limits,
                cancelled,
            )?;
            let prepared = PreparedSource::new(session, expected, cancelled)?;
            check_cancel(cancelled)?;
            self.cache.insert(
                asset.clone(),
                CachedSource {
                    prepared,
                    bytes,
                    index_frames,
                },
            );
            self.recency.push_back(asset.clone());
            self.cache_bytes = reserved_bytes;
            self.cache_index_frames = reserved_frames;
        }
        self.cache
            .get(asset)
            .map(|source| &source.prepared)
            .ok_or_else(|| unavailable("source cache is absent"))
    }
}

fn validate_source_capacity(bytes: u64, index_frames: u64) -> Result<(), PreparationError> {
    if bytes == 0 {
        return Err(unavailable("invalid physical PCM cache size"));
    }
    if bytes > MAX_CACHE_BYTES {
        return Err(unavailable("source PCM exceeds the 1 GiB source cache"));
    }
    if index_frames == 0 {
        return Err(unavailable("invalid audio index cache size"));
    }
    if index_frames > MAX_CACHE_INDEX_FRAMES {
        return Err(unavailable(
            "source audio index exceeds the 1,000,000-frame source cache",
        ));
    }
    Ok(())
}

fn make_room(
    cache: &mut BTreeMap<AssetId, CachedSource>,
    recency: &mut VecDeque<AssetId>,
    cache_bytes: &mut u64,
    cache_index_frames: &mut u64,
    bytes: u64,
    index_frames: u64,
) -> Result<(), PreparationError> {
    validate_source_capacity(bytes, index_frames)?;
    loop {
        if cache.len() < MAX_CACHED_SOURCES
            && source_capacity_fits(*cache_bytes, *cache_index_frames, bytes, index_frames)
        {
            return Ok(());
        }
        let oldest = recency
            .front()
            .ok_or_else(|| unavailable("source cache recency is inconsistent"))?;
        let entry = cache
            .get(oldest)
            .ok_or_else(|| unavailable("source cache entry is absent"))?;
        let remaining_bytes = cache_bytes
            .checked_sub(entry.bytes)
            .ok_or_else(|| unavailable("source cache byte accounting is inconsistent"))?;
        let remaining_frames = cache_index_frames
            .checked_sub(entry.index_frames)
            .ok_or_else(|| unavailable("source cache index accounting is inconsistent"))?;
        let oldest = oldest.clone();
        recency.pop_front();
        cache.remove(&oldest);
        *cache_bytes = remaining_bytes;
        *cache_index_frames = remaining_frames;
    }
}

fn source_capacity_fits(
    cached_bytes: u64,
    cached_index_frames: u64,
    bytes: u64,
    index_frames: u64,
) -> bool {
    cached_bytes
        .checked_add(bytes)
        .is_some_and(|total| total <= MAX_CACHE_BYTES)
        && cached_index_frames
            .checked_add(index_frames)
            .is_some_and(|total| total <= MAX_CACHE_INDEX_FRAMES)
}

fn mark_recent(recency: &mut VecDeque<AssetId>, asset: &AssetId) {
    recency.retain(|cached| cached != asset);
    recency.push_back(asset.clone());
}

fn check_cancel(cancelled: &AtomicBool) -> Result<(), PreparationError> {
    if cancelled.load(Ordering::Acquire) {
        Err(PreparationError::Cancelled)
    } else {
        Ok(())
    }
}
fn unavailable(error: impl std::fmt::Display) -> PreparationError {
    PreparationError::SourceUnavailable(error.to_string())
}

#[cfg(test)]
#[path = "sources/cache_tests.rs"]
mod cache_tests;
