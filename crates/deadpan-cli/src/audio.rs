//! Read-only source-stage inspection against one immutable project revision.
//! Opening originals, decoding and preparation belong off the UI/device thread.

use std::collections::{BTreeMap, VecDeque};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use deadpan_audio::{
    AudioSourceProvider, DefinitionAudioBlock, DomainAudioBlock, EdgeFadedBlock, LimitedAudio,
    LimitedAudioBlock, LimitedAudioError, PreparationError, PreparedSource, SequenceAudio,
    SequenceAudioError, SourceStageBlock, StageAudio, StageAudioError, TimeMappedBlock,
};
use deadpan_core::{
    AssetId, AssetRecord, AudioSample, FrozenAudioContext, ProjectDocument, ProjectId, RevisionId,
    SourceQualificationId,
};
use deadpan_media::audio_session::{AudioSession, AudioSessionLimits};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_plan::{
    AudioDefinitionSelector, AudioRootPlacement, PlanError, RenderPlan, SignalSample,
};
use deadpan_store::original_media::{OriginalMediaLimits, OriginalMediaRecord};
use deadpan_store::{AccessMode, ProjectStore, StoreError};

mod offline;
pub use offline::{MAX_OFFLINE_AUDIO_FRAMES, OfflineAudioError, OfflineAudioSession};

#[derive(Debug, thiserror::Error)]
pub enum ProjectAudioError {
    #[error("retained audio context differs from the captured project revision")]
    ContextMismatch,
    #[error("limited audio inspection requires 1..256 samples")]
    LimitedInspectionRange,
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Plan(#[from] PlanError),
    #[error(transparent)]
    Sequence(#[from] SequenceAudioError),
    #[error(transparent)]
    Stage(#[from] StageAudioError),
    #[error(transparent)]
    Limited(#[from] LimitedAudioError),
}

/// A fixed revision with bounded retained source PCM sessions. Subsequent writer
/// edits, undo and reuse of asset aliases cannot change this session's meaning.
/// Each inspection path declares its processing order. Limited audition still
/// omits the unimplemented voice effects, sends and full group mix.
pub struct ProjectAudioSession {
    sequence: SequenceAudio,
    stages: StageAudio,
    limited: LimitedAudio,
    sources: RegisteredSources,
}

impl ProjectAudioSession {
    pub fn open(path: &Path) -> Result<Self, ProjectAudioError> {
        Self::open_at(path, None)
    }

    /// Inspect a specific committed revision without moving the history cursor.
    /// The store validates history and the media host uses this revision's
    /// receipts and asset meanings, even if their aliases differ at HEAD.
    pub fn open_revision(path: &Path, revision: &RevisionId) -> Result<Self, ProjectAudioError> {
        Self::open_at(path, Some(revision))
    }

    fn open_at(path: &Path, revision: Option<&RevisionId>) -> Result<Self, ProjectAudioError> {
        let store = ProjectStore::open(path, AccessMode::ReadOnly)?;
        let document = match revision {
            Some(revision) => store.snapshot_at(revision)?,
            None => store.snapshot()?,
        };
        let plan = RenderPlan::compile(&document)?;
        Ok(Self::from_plan(store, document, plan))
    }

    /// Reopen a serialized raw audio context against its exact retained history.
    /// Compare the complete capture before preparing media; claimed revision or
    /// qualification names alone cannot redirect the host to different intent.
    /// Accepted original bytes and source receipts are still independently
    /// verified on demand. This does not author or select a live resume binding.
    pub fn open_context(
        path: &Path,
        context: &FrozenAudioContext,
    ) -> Result<Self, ProjectAudioError> {
        let store = ProjectStore::open(path, AccessMode::ReadOnly)?;
        let document = store.snapshot_at(context.revision_id())?;
        if !context
            .matches_document(&document)
            .map_err(PlanError::from)?
        {
            return Err(ProjectAudioError::ContextMismatch);
        }
        let plan = RenderPlan::compile_audio_context(context)?;
        Ok(Self::from_plan(store, document, plan))
    }

    fn from_plan(store: ProjectStore, document: ProjectDocument, plan: RenderPlan) -> Self {
        let plan = Arc::new(plan);
        let sequence = SequenceAudio::new(Arc::clone(&plan));
        Self {
            sequence,
            stages: StageAudio::new(Arc::clone(&plan)),
            limited: LimitedAudio::new(plan),
            sources: RegisteredSources {
                store,
                document,
                retained: BTreeMap::new(),
                recency: VecDeque::new(),
                cache_bytes: 0,
                cache_index_frames: 0,
                deadline: None,
            },
        }
    }

    pub fn plan(&self) -> &RenderPlan {
        self.sequence.plan()
    }

    pub fn revision(&self) -> &RevisionId {
        self.sources.document.revision_id()
    }

    pub fn read(
        &mut self,
        start: AudioSample,
        frames: u32,
        cancelled: &AtomicBool,
    ) -> Result<SourceStageBlock, ProjectAudioError> {
        Ok(self.sequence.read_sources(
            &mut self.sources,
            start,
            frames,
            Duration::from_secs(10),
            cancelled,
        )?)
    }

    pub fn read_time_mapped(
        &mut self,
        start: AudioSample,
        frames: u32,
        cancelled: &AtomicBool,
    ) -> Result<TimeMappedBlock, ProjectAudioError> {
        Ok(self.stages.read(
            &mut self.sources,
            start,
            frames,
            Duration::from_secs(10),
            cancelled,
        )?)
    }

    /// Inspect one physical audio context on its captured signed root grid.
    /// The probe selects a currently allocated sample; the requested interval
    /// can include hidden context outside that allocation or before root zero.
    pub fn read_domain(
        &mut self,
        probe: AudioSample,
        start: AudioSample,
        frames: u32,
        cancelled: &AtomicBool,
    ) -> Result<DomainAudioBlock, ProjectAudioError> {
        check_cancel(cancelled).map_err(StageAudioError::from)?;
        let domain = self
            .sequence
            .plan()
            .audio_domain_at(probe, Default::default())?;
        Ok(self.stages.read_domain(
            &mut self.sources,
            &domain,
            start,
            frames,
            Duration::from_secs(10),
            cancelled,
        )?)
    }

    /// Read an authored Node or Repeat default directly in its canonical
    /// definition-output clock. Historical sessions retain the same admission.
    pub fn read_definition(
        &mut self,
        selector: AudioDefinitionSelector,
        start: SignalSample,
        frames: u32,
        cancelled: &AtomicBool,
    ) -> Result<DefinitionAudioBlock, ProjectAudioError> {
        check_cancel(cancelled).map_err(StageAudioError::from)?;
        let definition = self.sequence.plan().audio_definition(selector)?;
        Ok(self.stages.read_definition(
            &mut self.sources,
            &definition,
            start,
            frames,
            Duration::from_secs(10),
            cancelled,
        )?)
    }

    /// Evaluate this revision's owned recipe in an explicit root placement.
    /// Placement supplies coordinates only; the selected live/historical
    /// revision still supplies the raw recipe and qualified media contracts.
    pub fn read_placement(
        &mut self,
        selector: AudioDefinitionSelector,
        placement: AudioRootPlacement,
        start: AudioSample,
        frames: u32,
        cancelled: &AtomicBool,
    ) -> Result<DomainAudioBlock, ProjectAudioError> {
        check_cancel(cancelled).map_err(StageAudioError::from)?;
        let domain = self
            .sequence
            .plan()
            .audio_definition(selector)?
            .in_root_clock(placement)?;
        Ok(self.stages.read_domain(
            &mut self.sources,
            &domain,
            start,
            frames,
            Duration::from_secs(10),
            cancelled,
        )?)
    }

    pub fn read_edge_faded(
        &mut self,
        start: AudioSample,
        frames: u32,
        cancelled: &AtomicBool,
    ) -> Result<EdgeFadedBlock, ProjectAudioError> {
        Ok(self.stages.read_edge_faded(
            &mut self.sources,
            start,
            frames,
            Duration::from_secs(10),
            cancelled,
        )?)
    }

    /// Inspect the canonical bus after authored node and sound gain, before
    /// limiting. Raw, time-mapped and edge-only inspection retain their order.
    pub fn read_authored_bus(
        &mut self,
        start: AudioSample,
        frames: u32,
        cancelled: &AtomicBool,
    ) -> Result<EdgeFadedBlock, ProjectAudioError> {
        let end = start
            .0
            .checked_add(i64::from(frames))
            .ok_or(StageAudioError::Range)?;
        if start.0 < 0
            || frames == 0
            || frames > deadpan_audio::MAX_OUTPUT_FRAMES
            || end > self.plan().audio_duration()?.0
        {
            return Err(StageAudioError::Range.into());
        }
        Ok(self.stages.prepare_authored_bus(
            &mut self.sources,
            start,
            frames,
            Duration::from_secs(10),
            cancelled,
        )?)
    }

    /// The same canonical limited bus used by audition, with informational gain.
    /// Inspection keeps its 256-frame response limit; preparation includes the
    /// full real halo and shares one deadline across any crossed cache tiles.
    pub fn read_limited(
        &mut self,
        start: AudioSample,
        frames: u32,
        cancelled: &AtomicBool,
    ) -> Result<LimitedAudioBlock, ProjectAudioError> {
        if frames == 0 || frames > deadpan_audio::MAX_OUTPUT_FRAMES {
            return Err(ProjectAudioError::LimitedInspectionRange);
        }
        Ok(self.limited.read(
            &mut self.sources,
            start,
            frames,
            Duration::from_secs(60),
            cancelled,
        )?)
    }
}

// Unlike playback's lifetime cache, sequential CLI inspection can visit any
// number of sources. Evict least-recently-used entries at these residency bounds;
// callers cannot retain a source borrow across the next mutable provider call.
// AudioSession retains only PCM and its index. Original-byte snapshots exist
// during one serialized cold open, bounded by the decoder's input-byte limit.
const MAX_CACHED_SOURCES: usize = 16;
const MAX_CACHE_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_CACHE_INDEX_FRAMES: u64 = 1_000_000;

struct CachedSource {
    prepared: PreparedSource,
    authored: AssetRecord,
    qualification: SourceQualificationId,
    content: SourceContentIdentity,
    original: OriginalMediaRecord,
    bytes: u64,
    index_frames: u64,
    _resident: deadpan_diagnostics::Resident,
}

impl CachedSource {
    fn validate(&self, authored: &AssetRecord) -> Result<(), PreparationError> {
        if authored != &self.authored
            || authored.source_qualification.as_ref() != Some(&self.qualification)
            || authored.content_hash != self.original.object().content().to_string()
            || self.original.sha256() != self.content.sha256()
            || self.original.object().byte_length() != self.content.byte_length()
            || self.prepared.index().content() != self.content
        {
            return Err(PreparationError::IndexMismatch);
        }
        Ok(())
    }
}

struct RegisteredSources {
    store: ProjectStore,
    document: ProjectDocument,
    retained: BTreeMap<AssetId, CachedSource>,
    recency: VecDeque<AssetId>,
    cache_bytes: u64,
    cache_index_frames: u64,
    // Offline reads retain the caller's absolute job deadline. Inspection keeps
    // its existing per-call limits by leaving this absent.
    deadline: Option<Instant>,
}

impl RegisteredSources {
    fn check_control(&self, cancelled: &AtomicBool) -> Result<(), PreparationError> {
        check_cancel(cancelled)?;
        if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err(unavailable("offline audio preparation deadline expired"));
        }
        Ok(())
    }

    fn remaining_timeout(
        &self,
        maximum: Duration,
        cancelled: &AtomicBool,
    ) -> Result<Duration, PreparationError> {
        check_cancel(cancelled)?;
        match self.deadline {
            Some(deadline) => deadline
                .checked_duration_since(Instant::now())
                .filter(|remaining| !remaining.is_zero())
                .map(|remaining| remaining.min(maximum))
                .ok_or_else(|| unavailable("offline audio preparation deadline expired")),
            None => Ok(maximum),
        }
    }

    /// Called only after qualified original bytes have been snapshotted. Make
    /// room before native PCM allocation. A later decode failure leaves evicted
    /// entries absent, all other entries intact, and no reservation charged to
    /// the cache.
    fn make_room(&mut self, bytes: u64, index_frames: u64) -> Result<(u64, u64), PreparationError> {
        loop {
            let reservation = reserve_source_capacity(
                self.cache_bytes,
                self.cache_index_frames,
                bytes,
                index_frames,
            )?;
            if self.retained.len() < MAX_CACHED_SOURCES
                && let Some(reserved) = reservation
            {
                return Ok(reserved);
            }
            let oldest = self
                .recency
                .front()
                .ok_or_else(|| unavailable("source cache recency is inconsistent"))?;
            let entry = self
                .retained
                .get(oldest)
                .ok_or_else(|| unavailable("source cache entry is absent"))?;
            let remaining_bytes = self
                .cache_bytes
                .checked_sub(entry.bytes)
                .ok_or_else(|| unavailable("source cache byte accounting is inconsistent"))?;
            let remaining_index_frames = self
                .cache_index_frames
                .checked_sub(entry.index_frames)
                .ok_or_else(|| unavailable("source cache index accounting is inconsistent"))?;
            let oldest = oldest.clone();
            self.recency.pop_front();
            self.retained.remove(&oldest);
            deadpan_diagnostics::CACHES
                .decoded_pcm
                .evictions
                .increment();
            self.cache_bytes = remaining_bytes;
            self.cache_index_frames = remaining_index_frames;
        }
    }

    fn mark_recent(&mut self, asset: &AssetId) {
        self.recency.retain(|cached| cached != asset);
        self.recency.push_back(asset.clone());
    }
}

/// None means existing entries must be evicted. A source whose own physical
/// PCM or audio index cannot fit is rejected before snapshots, eviction or
/// decoding. A successful reservation returns prospective (bytes, index frames)
/// totals; neither counter is charged until preparation succeeds.
fn reserve_source_capacity(
    cached_bytes: u64,
    cached_index_frames: u64,
    bytes: u64,
    index_frames: u64,
) -> Result<Option<(u64, u64)>, PreparationError> {
    if bytes == 0 {
        return Err(unavailable("invalid physical PCM cache size"));
    }
    if bytes > MAX_CACHE_BYTES {
        return Err(unavailable("source PCM exceeds the 1 GiB inspection cache"));
    }
    if index_frames == 0 {
        return Err(unavailable("invalid audio index cache size"));
    }
    if index_frames > MAX_CACHE_INDEX_FRAMES {
        return Err(unavailable(
            "source audio index exceeds the 1,000,000-frame inspection cache",
        ));
    }
    let reserved_bytes = cached_bytes
        .checked_add(bytes)
        .filter(|sum| *sum <= MAX_CACHE_BYTES);
    let reserved_index_frames = cached_index_frames
        .checked_add(index_frames)
        .filter(|sum| *sum <= MAX_CACHE_INDEX_FRAMES);
    Ok(reserved_bytes.zip(reserved_index_frames))
}

impl AudioSourceProvider for RegisteredSources {
    fn source_for_context(
        &mut self,
        project: &ProjectId,
        revision: &RevisionId,
        asset: &AssetId,
        expected: &AssetRecord,
        cancelled: &AtomicBool,
    ) -> Result<&PreparedSource, PreparationError> {
        self.check_control(cancelled)?;
        if self.document.assets().get(asset) != Some(expected) {
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
        self.check_control(cancelled)?;
        if project != self.document.project_id() || revision != self.document.revision_id() {
            return Err(PreparationError::SourceUnavailable(
                "audio request differs from the fixed project revision".into(),
            ));
        }
        if self.retained.contains_key(asset) {
            deadpan_diagnostics::CACHES.decoded_pcm.hits.increment();
        } else {
            deadpan_diagnostics::CACHES.decoded_pcm.misses.increment();
            let authored = self
                .document
                .assets()
                .get(asset)
                .ok_or_else(|| unavailable("source is absent from the fixed project revision"))?
                .clone();
            let receipt = self
                .store
                .registered_source(revision, asset)
                .map_err(unavailable)?;
            if authored.source_qualification.as_ref() != Some(receipt.id())
                || authored.content_hash != receipt.original().content().to_string()
            {
                return Err(PreparationError::IndexMismatch);
            }
            let expected = receipt.snapshot().audio().ok_or_else(|| {
                PreparationError::SourceUnavailable(
                    "selected source has no qualified audio index".into(),
                )
            })?;
            self.check_control(cancelled)?;
            // Account for all physical samples, including priming and padding,
            // before taking any snapshots or opening a native decoder. Publish
            // the reservation only after complete preparation succeeds.
            let bytes = expected
                .decoded_samples()
                .checked_mul(u64::from(expected.stream().channel_layout.channels()))
                .and_then(|samples| samples.checked_mul(4))
                .ok_or_else(|| unavailable("invalid physical PCM cache size"))?;
            let index_frames = u64::try_from(expected.frames().len())
                .map_err(|_| unavailable("invalid audio index cache size"))?;
            let _ = reserve_source_capacity(
                self.cache_bytes,
                self.cache_index_frames,
                bytes,
                index_frames,
            )?;
            let mut audio_limits = AudioSessionLimits {
                maximum_cache_bytes: bytes,
                maximum_index_frames: expected.frames().len(),
                ..AudioSessionLimits::default()
            };
            let original_limits = OriginalMediaLimits::new(
                audio_limits.decode.max_input_bytes,
                self.remaining_timeout(audio_limits.opening_timeout, cancelled)?,
            )
            .map_err(unavailable)?;
            let original = self.store.snapshot_original(
                receipt.original().content(),
                original_limits,
                cancelled,
            );
            self.check_control(cancelled)?;
            let mut original = original.map_err(unavailable)?;
            if original.record().object() != receipt.original()
                || original.record().sha256() != expected.content().sha256()
                || original.record().object().byte_length() != expected.content().byte_length()
            {
                return Err(PreparationError::IndexMismatch);
            }
            self.check_control(cancelled)?;
            let reserved = self.make_room(bytes, index_frames)?;
            // Snapshotting already consumed part of the same job budget. A
            // fresh native opening limit must use only what remains now.
            audio_limits.opening_timeout =
                self.remaining_timeout(audio_limits.opening_timeout, cancelled)?;
            let session = AudioSession::open_verified(
                &mut original,
                expected.content(),
                expected.stream().stream_index,
                audio_limits,
                cancelled,
            );
            self.check_control(cancelled)?;
            let session = session?;
            // The receipt's persisted explicit interpretation, when the
            // stream declares no speaker layout, is the only layout choice.
            let layout = receipt
                .snapshot()
                .audio_layout()
                .ok_or_else(|| unavailable("source has no qualified audio index"))?;
            let prepared =
                PreparedSource::with_layout_controlled(session, expected, layout, || {
                    self.check_control(cancelled)
                })?;
            self.check_control(cancelled)?;
            self.retained.insert(
                asset.clone(),
                CachedSource {
                    prepared,
                    authored,
                    qualification: receipt.id().clone(),
                    content: expected.content(),
                    original: original.record().clone(),
                    bytes,
                    index_frames,
                    _resident: deadpan_diagnostics::Resident::new(
                        &deadpan_diagnostics::CACHES.decoded_pcm,
                        bytes,
                    ),
                },
            );
            (self.cache_bytes, self.cache_index_frames) = reserved;
            self.mark_recent(asset);
        }
        self.check_control(cancelled)?;
        // The complete index was compared when PreparedSource was constructed.
        // Re-admit the captured contracts on every hit, while continuing to use
        // private PCM if a linked external path has moved or disappeared.
        self.retained
            .get(asset)
            .ok_or_else(|| unavailable("source cache is absent"))?
            .validate(self.document.assets().get(asset).ok_or_else(|| {
                unavailable("source is absent from the fixed project revision")
            })?)?;
        self.mark_recent(asset);
        self.check_control(cancelled)?;
        self.retained
            .get(asset)
            .map(|entry| &entry.prepared)
            .ok_or_else(|| unavailable("source cache is absent"))
    }
}

fn check_cancel(cancelled: &AtomicBool) -> Result<(), PreparationError> {
    if cancelled.load(Ordering::Relaxed) {
        Err(PreparationError::Cancelled)
    } else {
        Ok(())
    }
}

fn unavailable(error: impl std::fmt::Display) -> PreparationError {
    PreparationError::SourceUnavailable(error.to_string())
}

#[cfg(test)]
#[path = "audio/cache_tests.rs"]
mod cache_tests;
