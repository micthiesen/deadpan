//! Read-only source-stage inspection against one immutable project revision.
//! Opening originals, decoding and preparation belong off the UI/device thread.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use deadpan_audio::{
    AudioSourceProvider, EdgeFadedBlock, PreparationError, PreparedSource, SequenceAudio,
    SequenceAudioError, SourceStageBlock, StageAudio, StageAudioError, TimeMappedBlock,
};
use deadpan_core::{
    AssetId, AssetRecord, AudioSample, FrozenAudioContext, ProjectDocument, ProjectId, RevisionId,
};
use deadpan_media::audio_session::{AudioSession, AudioSessionLimits};
use deadpan_plan::{PlanError, RenderPlan};
use deadpan_store::original_media::OriginalMediaLimits;
use deadpan_store::{AccessMode, ProjectStore, StoreError};

#[derive(Debug, thiserror::Error)]
pub enum ProjectAudioError {
    #[error("retained audio context differs from the captured project revision")]
    ContextMismatch,
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Plan(#[from] PlanError),
    #[error(transparent)]
    Sequence(#[from] SequenceAudioError),
    #[error(transparent)]
    Stage(#[from] StageAudioError),
}

/// A fixed revision with one retained source PCM session. Subsequent writer
/// edits, undo and reuse of asset aliases cannot change this session's meaning.
/// Returned PCM is explicitly before voice effects and the master pipeline.
pub struct ProjectAudioSession {
    sequence: SequenceAudio,
    stages: StageAudio,
    sources: RegisteredSources,
}

impl ProjectAudioSession {
    pub fn open(path: &Path) -> Result<Self, ProjectAudioError> {
        let store = ProjectStore::open(path, AccessMode::ReadOnly)?;
        let document = store.snapshot()?;
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
        if document.project_id() != context.project_id()
            || FrozenAudioContext::capture(&document).map_err(PlanError::from)? != *context
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
            stages: StageAudio::new(plan),
            sources: RegisteredSources {
                store,
                document,
                retained: None,
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
}

struct RegisteredSources {
    store: ProjectStore,
    document: ProjectDocument,
    retained: Option<(AssetId, PreparedSource)>,
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
        check_cancel(cancelled)?;
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
        check_cancel(cancelled)?;
        if project != self.document.project_id() || revision != self.document.revision_id() {
            return Err(PreparationError::SourceUnavailable(
                "audio request differs from the fixed project revision".into(),
            ));
        }
        if !self
            .retained
            .as_ref()
            .is_some_and(|(retained_asset, _)| retained_asset == asset)
        {
            // Drop first so even switching sources retains at most one bounded
            // private PCM cache. A failed load never exposes an earlier source.
            self.retained = None;
            let receipt = self
                .store
                .registered_source(revision, asset)
                .map_err(unavailable)?;
            let authored = self.document.assets().get(asset).ok_or_else(|| {
                PreparationError::SourceUnavailable(
                    "source is absent from the fixed project revision".into(),
                )
            })?;
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
            check_cancel(cancelled)?;
            let audio_limits = AudioSessionLimits::default();
            let original_limits = OriginalMediaLimits::new(
                audio_limits.decode.max_input_bytes,
                audio_limits.opening_timeout,
            )
            .map_err(unavailable)?;
            let mut original = self
                .store
                .snapshot_original(receipt.original().content(), original_limits, cancelled)
                .map_err(unavailable)?;
            if original.record().object() != receipt.original()
                || original.record().sha256() != expected.content().sha256()
                || original.record().object().byte_length() != expected.content().byte_length()
            {
                return Err(PreparationError::IndexMismatch);
            }
            check_cancel(cancelled)?;
            let session = AudioSession::open_verified(
                &mut original,
                expected.content(),
                expected.stream().stream_index,
                audio_limits,
                cancelled,
            )?;
            let prepared = PreparedSource::new(session, expected, cancelled)?;
            self.retained = Some((asset.clone(), prepared));
        }
        check_cancel(cancelled)?;
        self.retained
            .as_ref()
            .map(|(_, source)| source)
            .ok_or_else(|| PreparationError::SourceUnavailable("source cache is absent".into()))
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
