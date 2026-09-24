//! Canonical limited audition tiles on the absolute project sample grid.
//! The current upstream is the edge-faded bus. Voice effects, sends and the
//! full group mix remain separate work; this is not a completed export master.
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use deadpan_core::{AudioSample, ProjectId, RevisionId};
use deadpan_plan::RenderPlan;
use serde::Serialize;

use crate::stages::{Dependencies, PreparationBudget, PreparedBus};
use crate::{
    AudioSourceProvider, LIMITER_ID, LIMITER_MAX_OUTPUT_FRAMES, LimitedTile, LimiterContext,
    LimiterError, StageAudio, StageAudioError,
};

/// The cache holds at most this many complete, verified canonical tiles.
/// Source/stage caches and the bounded active limiter context are separate.
pub const MAX_CACHED_LIMITED_TILES: usize = 4;

/// Exact bus intervals, each containing at most 8192 stereo frames. They retain
/// the real context boundaries requested by the limiter, without grid padding.
pub const MAX_CACHED_LIMITER_BUS_BLOCKS: usize = 12;

#[derive(Debug, thiserror::Error)]
pub enum LimitedAudioError {
    #[error("limited audio requires 1..8192 samples inside the sequence")]
    Range,
    #[error(transparent)]
    Stage(#[from] StageAudioError),
    #[error(transparent)]
    Limiter(#[from] LimiterError),
    #[error(transparent)]
    Plan(#[from] deadpan_plan::PlanError),
}

/// Verification covers complete canonical tiles and their owned reconstruction
/// anchors, rather than pretending a cropped PCM block has zero-valued context.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VerifiedLimitedTile {
    pub samples: Range<AudioSample>,
    pub peak: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LimitedAudioBlock {
    pub schema_version: u32,
    pub stage: &'static str,
    pub engine: &'static str,
    pub processing_order: [&'static str; 3],
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub start: AudioSample,
    pub samples: Vec<[f32; 2]>,
    /// Linked stereo gain applied to the original bus, without normalization.
    pub gain: Vec<f64>,
    pub suppressed: Vec<Range<AudioSample>>,
    pub verified_tiles: Vec<VerifiedLimitedTile>,
}

impl LimitedAudioBlock {
    pub fn maximum_reduction_db(&self) -> f64 {
        -20.0 * self.gain.iter().copied().fold(1.0, f64::min).log10()
    }
}

struct CachedTile {
    limited: LimitedTile,
    dependencies: Dependencies,
    suppressed: Vec<Range<AudioSample>>,
}

struct CachedBusBlock {
    samples: Range<AudioSample>,
    bus: PreparedBus,
}

/// Worker-owned preparation against one immutable plan. Every cache hit checks
/// all original dependencies, including context hidden by Preserve stages.
/// Consumers share samples regardless of request order, seek or batch size.
pub struct LimitedAudio {
    stages: StageAudio,
    cache: Vec<Arc<CachedTile>>,
    bus_cache: Vec<Arc<CachedBusBlock>>,
}

impl LimitedAudio {
    pub fn new(plan: Arc<RenderPlan>) -> Self {
        Self::from_stages(StageAudio::new(plan))
    }

    pub fn from_stages(stages: StageAudio) -> Self {
        Self {
            stages,
            cache: Vec::new(),
            bus_cache: Vec::new(),
        }
    }

    pub fn plan(&self) -> &RenderPlan {
        self.stages.plan()
    }

    pub fn cached_tile_count(&self) -> usize {
        self.cache.len()
    }

    pub fn cached_bus_block_count(&self) -> usize {
        self.bus_cache.len()
    }

    pub fn read(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        start: AudioSample,
        frames: u32,
        timeout: Duration,
        cancelled: &AtomicBool,
    ) -> Result<LimitedAudioBlock, LimitedAudioError> {
        let budget = PreparationBudget::new(timeout, cancelled)?;
        let end = start
            .0
            .checked_add(i64::from(frames))
            .ok_or(LimitedAudioError::Range)?;
        let duration = self.plan().audio_duration()?;
        if start.0 < 0
            || frames == 0
            || frames as usize > LIMITER_MAX_OUTPUT_FRAMES
            || end > duration.0
        {
            return Err(LimitedAudioError::Range);
        }
        let requested = start..AudioSample(end);
        let mut samples = Vec::with_capacity(frames as usize);
        let mut gain = Vec::with_capacity(frames as usize);
        let mut suppressed = Vec::new();
        let mut verified_tiles = Vec::new();
        let width =
            i64::try_from(LIMITER_MAX_OUTPUT_FRAMES).map_err(|_| LimitedAudioError::Range)?;
        let mut cursor = start.0;
        // At most two canonical tiles for one admitted output interval. The
        // same budget covers both cache validation and cold preparation.
        while cursor < end {
            let origin = AudioSample(cursor / width * width);
            let tile_end = AudioSample(
                i64::try_from(
                    (i128::from(origin.0) + i128::from(width)).min(i128::from(duration.0)),
                )
                .map_err(|_| LimitedAudioError::Range)?,
            );
            let tile = self.tile(provider, origin..tile_end, duration, &budget, cancelled)?;
            let offset =
                usize::try_from(cursor - origin.0).map_err(|_| LimitedAudioError::Range)?;
            let count = usize::try_from((tile_end.0 - cursor).min(end - cursor))
                .map_err(|_| LimitedAudioError::Range)?;
            samples.extend_from_slice(&tile.limited.samples[offset..offset + count]);
            gain.extend_from_slice(&tile.limited.gain[offset..offset + count]);
            for range in &tile.suppressed {
                append_suppression(&mut suppressed, range, &requested);
            }
            verified_tiles.push(VerifiedLimitedTile {
                samples: origin..tile_end,
                peak: tile.limited.peak,
            });
            cursor += i64::try_from(count).map_err(|_| LimitedAudioError::Range)?;
        }
        budget.check()?;
        Ok(LimitedAudioBlock {
            schema_version: 1,
            stage: "limited_edge_faded_pcm",
            engine: LIMITER_ID,
            processing_order: ["time_pitch_mapping", "edge_fades", "stereo_limiter"],
            project_id: self.plan().metadata().project_id.clone(),
            revision_id: self.plan().metadata().revision_id.clone(),
            start,
            samples,
            gain,
            suppressed,
            verified_tiles,
        })
    }

    fn tile(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        requested: Range<AudioSample>,
        duration: AudioSample,
        budget: &PreparationBudget<'_>,
        cancelled: &AtomicBool,
    ) -> Result<Arc<CachedTile>, LimitedAudioError> {
        budget.check()?;
        if let Some(index) = self
            .cache
            .iter()
            .position(|entry| entry.limited.start == requested.start)
        {
            // Remove before revalidation: failures cannot leave an apparently
            // reusable entry behind. A changed valid source may be recomputed.
            let entry = self.cache.remove(index);
            if self
                .stages
                .revalidate_dependencies(provider, &entry.dependencies, budget)?
            {
                self.cache.push(Arc::clone(&entry));
                return Ok(entry);
            }
        }
        let project = AudioSample(0)..duration;
        let context = LimitedTile::required_context(&project, &requested)?;
        let frames = usize::try_from(context.end.0 - context.start.0)
            .map_err(|_| LimitedAudioError::Range)?;
        let width =
            i64::try_from(LIMITER_MAX_OUTPUT_FRAMES).map_err(|_| LimitedAudioError::Range)?;
        let mut samples = Vec::with_capacity(frames);
        let mut dependencies = Dependencies::new();
        let mut suppressed = Vec::new();
        let mut cursor = context.start;
        // Start at the exact required support. Rounding outward could admit
        // unrelated sources or effects that the requested limiter tile avoids.
        while cursor < context.end {
            let count = (context.end.0 - cursor.0).min(width);
            let end = AudioSample(
                cursor
                    .0
                    .checked_add(count)
                    .ok_or(LimitedAudioError::Range)?,
            );
            let block = self.bus_block(provider, cursor..end, budget)?;
            samples.extend_from_slice(&block.bus.block.samples);
            dependencies.extend(
                block
                    .bus
                    .dependencies
                    .iter()
                    .map(|(asset, fingerprint)| (asset.clone(), *fingerprint)),
            );
            for range in &block.bus.block.suppressed {
                append_suppression(&mut suppressed, range, &requested);
            }
            cursor = end;
        }
        let limited = LimitedTile::prepare(
            LimiterContext {
                project_samples: project,
                start: context.start,
                samples,
            },
            requested,
            budget.deadline(),
            cancelled,
        )?;
        budget.check()?;
        let entry = Arc::new(CachedTile {
            limited,
            dependencies,
            suppressed,
        });
        if self.cache.len() == MAX_CACHED_LIMITED_TILES {
            self.cache.remove(0);
        }
        self.cache.push(Arc::clone(&entry));
        Ok(entry)
    }

    fn bus_block(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        requested: Range<AudioSample>,
        budget: &PreparationBudget<'_>,
    ) -> Result<Arc<CachedBusBlock>, LimitedAudioError> {
        budget.check()?;
        if let Some(index) = self
            .bus_cache
            .iter()
            .position(|entry| entry.samples == requested)
        {
            let entry = self.bus_cache.remove(index);
            if self
                .stages
                .revalidate_dependencies(provider, &entry.bus.dependencies, budget)?
            {
                self.bus_cache.push(Arc::clone(&entry));
                return Ok(entry);
            }
        }
        let frames = u32::try_from(requested.end.0 - requested.start.0)
            .map_err(|_| LimitedAudioError::Range)?;
        let bus = self
            .stages
            .prepare_bus(provider, requested.start, frames, budget)?;
        budget.check()?;
        let entry = Arc::new(CachedBusBlock {
            samples: requested,
            bus,
        });
        if self.bus_cache.len() == MAX_CACHED_LIMITER_BUS_BLOCKS {
            self.bus_cache.remove(0);
        }
        self.bus_cache.push(Arc::clone(&entry));
        Ok(entry)
    }
}

fn append_suppression(
    output: &mut Vec<Range<AudioSample>>,
    range: &Range<AudioSample>,
    requested: &Range<AudioSample>,
) {
    let clipped = range.start.max(requested.start)..range.end.min(requested.end);
    if clipped.start >= clipped.end {
        return;
    }
    if let Some(last) = output.last_mut()
        && last.end >= clipped.start
    {
        last.end = last.end.max(clipped.end);
    } else {
        output.push(clipped);
    }
}
