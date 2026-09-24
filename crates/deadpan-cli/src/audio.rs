//! Read-only source-stage inspection against one immutable project revision.
//! Opening originals, decoding and preparation belong off the UI/device thread.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use deadpan_audio::{
    AudioSourceProvider, DefinitionAudioBlock, DomainAudioBlock, EdgeFadedBlock, LimitedAudio,
    LimitedAudioBlock, LimitedAudioError, PreparationError, PreparedSource, SequenceAudio,
    SequenceAudioError, SourceStageBlock, StageAudio, StageAudioError, TimeMappedBlock,
};
use deadpan_core::{
    AssetId, AssetRecord, AudioSample, FrozenAudioContext, ProjectDocument, ProjectId, RevisionId,
};
use deadpan_media::audio_session::{AudioSession, AudioSessionLimits};
use deadpan_plan::{
    AudioDefinitionSelector, AudioRootPlacement, PlanError, RenderPlan, SignalSample,
};
use deadpan_store::original_media::OriginalMediaLimits;
use deadpan_store::{AccessMode, ProjectStore, StoreError};

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

/// A fixed revision with one retained source PCM session. Subsequent writer
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
            stages: StageAudio::new(Arc::clone(&plan)),
            limited: LimitedAudio::new(plan),
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
