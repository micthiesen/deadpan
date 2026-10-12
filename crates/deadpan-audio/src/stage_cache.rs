//! Persistent stage keys use actual canonical input and output suppression.
//! This intentionally scans input again on a cold miss, retaining only bounded
//! windows. No serialized plan or cache manifest can mint source admission.
use super::*;
use crate::persistent_pcm::{CacheKey, ENGINE_PIN};
use crate::{SavedAudioEvent, SavedAudioStatus};

pub(super) struct SavedStage {
    pub key: CacheKey,
    pub dependencies: Dependencies,
    pub relative_depth: usize,
}

fn io_error(error: StageAudioError) -> std::io::Error {
    std::io::Error::other(error.to_string())
}
fn cache_failure(error: &std::io::Error) -> SavedAudioEvent {
    match error.kind() {
        std::io::ErrorKind::InvalidData | std::io::ErrorKind::UnexpectedEof => {
            SavedAudioEvent::Damaged
        }
        std::io::ErrorKind::WouldBlock => SavedAudioEvent::Busy,
        std::io::ErrorKind::StorageFull => SavedAudioEvent::Full,
        _ => SavedAudioEvent::Unavailable,
    }
}

impl StageAudio {
    fn saved_status(&self, key: CacheKey, frames: u32, event: SavedAudioEvent) {
        if let Some(monitor) = &self.preparation_monitor {
            monitor.report_saved_audio(SavedAudioStatus {
                key: Some(key.0),
                bytes: u64::from(frames) * 8,
                event,
                regenerated_from: None,
            });
        }
    }

    pub(super) fn saved_stage_key(
        &mut self,
        input: SignalInput<'_, '_>,
        output: SignalInput<'_, '_>,
        recipe: StreamRecipe,
        provider: &mut impl AudioSourceProvider,
        control: WorkControl<'_>,
        depth: usize,
    ) -> Result<Option<SavedStage>, StageAudioError> {
        if self.persistent_cache.is_none() {
            return Ok(None);
        }
        let scope = control.cache_key_scope();
        let result = self.scan_saved_stage_key(input, output, recipe, provider, control, depth);
        drop(scope);
        match result {
            Err(StageAudioError::Limit(CACHE_KEY_WORK_LIMIT)) => {
                if let Some(monitor) = &self.preparation_monitor {
                    monitor.report_saved_audio(SavedAudioStatus {
                        key: None,
                        bytes: 0,
                        event: SavedAudioEvent::BudgetSkipped,
                        regenerated_from: None,
                    });
                }
                Ok(None)
            }
            other => other,
        }
    }

    fn scan_saved_stage_key(
        &mut self,
        input: SignalInput<'_, '_>,
        output: SignalInput<'_, '_>,
        recipe: StreamRecipe,
        provider: &mut impl AudioSourceProvider,
        control: WorkControl<'_>,
        depth: usize,
    ) -> Result<Option<SavedStage>, StageAudioError> {
        let progress = self.preparation_monitor.as_ref().map(|m| {
            m.enter(PreparationActivity::CheckingSavedAudio {
                completed: 0,
                total: recipe.input_frames(),
            })
        });
        let mut hash = blake3::Hasher::new();
        hash.update(b"deadpan-canonical-pcm-input-key-1\0stereo-f32-le-48000\0");
        hash.update(ENGINE_PIN.as_bytes());
        hash.update(recipe.engine_id().as_bytes());
        hash.update(&recipe.input_frames().to_le_bytes());
        hash.update(&recipe.output_frames().to_le_bytes());
        hash.update(&recipe.rate().numerator().to_le_bytes());
        hash.update(&recipe.rate().denominator().to_le_bytes());
        hash.update(&recipe.pitch_semitones().to_le_bytes());
        let mut dependencies = Dependencies::new();
        let mut relative_depth = 0;
        let mut cursor = 0;
        while cursor < recipe.input_frames() {
            control.check()?;
            let end = (cursor + MAX_OUTPUT_FRAMES).min(recipe.input_frames());
            // Retain exact authored ranges and mappings as well as the actual
            // PCM. Equal-valued samples from different selections must not
            // erase their provenance from the key. Revision-bearing nested
            // descriptors intentionally cause conservative misses.
            hash_input_mapping(&mut hash, input, cursor..end, control)?;
            let block = self.read_stage_input(input, cursor..end, provider, control, depth)?;
            for sample in block.samples.iter().flatten() {
                if !sample.is_finite() {
                    return Err(deadpan_dsp::DspError::NonFiniteInput.into());
                }
                if sample.abs() > deadpan_dsp::MAX_INPUT_PEAK {
                    return Err(deadpan_dsp::DspError::InputPeak.into());
                }
                hash.update(&sample.to_le_bytes());
            }
            dependencies.extend(block.dependencies);
            relative_depth = relative_depth.max(block.relative_depth);
            cursor = end;
            if let Some(progress) = &progress {
                progress.report(PreparationActivity::CheckingSavedAudio {
                    completed: cursor,
                    total: recipe.input_frames(),
                });
            }
        }
        // Hash the exact mask applied after the canonical DSP, on its output
        // grid. Intermediate masks are already present in the input PCM.
        cursor = 0;
        while cursor < recipe.output_frames() {
            control.check()?;
            let count = (recipe.output_frames() - cursor).min(MAX_OUTPUT_FRAMES);
            let mut mask = vec![[1.0; 2]; count as usize];
            suppress_signal(
                output,
                SignalSample(i64::from(cursor)),
                &mut mask,
                &self.plan,
                control,
            )?;
            for sample in mask {
                hash.update(&[u8::from(sample[0] == 0.0)]);
            }
            cursor += count;
        }
        // Include every freshly observed physical source's complete index,
        // byte identity and interpretation, even if two sources decode alike.
        for (asset, provenance) in &dependencies {
            let name = asset.as_str();
            hash.update(&(name.len() as u64).to_le_bytes());
            hash.update(name.as_bytes());
            hash.update(provenance);
        }
        control.check()?;
        Ok(Some(SavedStage {
            key: CacheKey(*hash.finalize().as_bytes()),
            dependencies,
            relative_depth,
        }))
    }

    pub(super) fn restore_saved_stage(
        &self,
        saved: &SavedStage,
        frames: u32,
        control: WorkControl<'_>,
    ) -> Result<Option<PreparedBlock>, StageAudioError> {
        let Some(cache) = &self.persistent_cache else {
            return Ok(None);
        };
        let progress = self.preparation_monitor.as_ref().map(|m| {
            m.enter(PreparationActivity::RestoringSavedAudio {
                completed: 0,
                total: frames,
            })
        });
        let mut writer = PcmWriter::new(frames, self.limits.maximum_memory_stage_frames)?;
        let result = cache.restore(saved.key, frames, |samples| {
            writer.push(samples, control).map_err(io_error)?;
            if let Some(progress) = &progress {
                progress.report(PreparationActivity::RestoringSavedAudio {
                    completed: writer.len() as u32,
                    total: frames,
                });
            }
            Ok(())
        });
        control.check()?;
        match result {
            Ok(true) => {
                self.saved_status(saved.key, frames, SavedAudioEvent::Restored);
                Ok(Some(PreparedBlock {
                    samples: writer.finish(control)?,
                    dependencies: saved.dependencies.clone(),
                    relative_depth: saved.relative_depth,
                }))
            }
            Ok(false) => {
                self.saved_status(saved.key, frames, SavedAudioEvent::Missing);
                Ok(None)
            }
            Err(error) => {
                self.saved_status(saved.key, frames, cache_failure(&error));
                // Rebuild with the same canonical processor. No failed restore
                // is published; partial private PCM drops with this writer.
                Ok(None)
            }
        }
    }

    pub(super) fn save_stage(
        &self,
        saved: &SavedStage,
        block: &PreparedBlock,
        control: WorkControl<'_>,
    ) -> Result<(), StageAudioError> {
        let Some(cache) = &self.persistent_cache else {
            return Ok(());
        };
        control.check()?;
        // The input scan and DSP pass must observe the same immutable sources.
        if saved.dependencies != block.dependencies {
            return Err(PreparationError::IndexMismatch.into());
        }
        let _progress = self
            .preparation_monitor
            .as_ref()
            .map(|m| m.enter(PreparationActivity::SavingAudioCache));
        let frames = u32::try_from(block.samples.len()).map_err(|_| StageAudioError::Range)?;
        let result = cache.store(saved.key, frames, |range| {
            control.check().map_err(io_error)?;
            block
                .samples
                .read_window(range, control.cancelled)
                .map_err(io_error)
        });
        control.check()?;
        self.saved_status(
            saved.key,
            frames,
            match result {
                Ok(()) => SavedAudioEvent::Saved,
                Err(error) => cache_failure(&error),
            },
        );
        Ok(())
    }
}

fn hash_input_mapping(
    hash: &mut blake3::Hasher,
    input: SignalInput<'_, '_>,
    range: Range<u32>,
    control: WorkControl<'_>,
) -> Result<(), StageAudioError> {
    struct HashWriter<'a>(&'a mut blake3::Hasher);
    impl std::io::Write for HashWriter<'_> {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let range = SignalSample(i64::from(range.start))..SignalSample(i64::from(range.end));
    match input {
        SignalInput::Mix(mix) => {
            let query = mix.query(range, control.query_limits()?)?;
            control.spend_plan_work(query.work)?;
            for voice in query.voices {
                serde_json::to_writer(
                    HashWriter(hash),
                    &(voice.index, voice.signal.spans, voice.gates),
                )
                .map_err(|_| StageAudioError::Range)?;
            }
        }
        _ => {
            let query = input.query(range, control.query_limits()?)?;
            control.spend_plan_work(query.work)?;
            serde_json::to_writer(HashWriter(hash), &query.spans)
                .map_err(|_| StageAudioError::Range)?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "persistent_stage_tests.rs"]
mod tests;
