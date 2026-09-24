//! All plan compilation, original verification, decoding and canonical DSP are
//! owned here. One completed mailbox batch and one controller/producer batch
//! bound the handoff to at most 16,384 stereo frames, plus the native queue.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use deadpan_audio::{LimitedAudio, StageAudio, StageLimits};
use deadpan_core::AudioSample;
use deadpan_plan::RenderPlan;

use crate::controller::{Job, Shared};
use crate::sources::Sources;

pub(crate) const BATCH_FRAMES: usize = 8192;
pub(crate) struct Batch {
    pub epoch: u64,
    pub start: AudioSample,
    pub end: AudioSample,
    pub samples: Vec<[f32; 2]>,
    pub eos: bool,
}
pub(crate) enum Reply {
    Batch(Batch),
    Failed { epoch: u64, error: String },
}

pub(crate) fn run(shared: Arc<Shared>) {
    let mut retained = None;
    loop {
        let job = {
            let mut state = shared.lock();
            loop {
                if state.shutdown() {
                    return;
                }
                if let Some(job) = state.prep.take() {
                    break job;
                }
                state = shared
                    .wake
                    .wait(state)
                    .unwrap_or_else(|error| error.into_inner());
            }
        };
        if let Err(error) = prepare(&shared, &job, &mut retained)
            && !job.cancelled.load(Ordering::Acquire)
        {
            publish(
                &shared,
                &job,
                Reply::Failed {
                    epoch: job.epoch,
                    error,
                },
            );
        }
    }
}

fn wait_slot(shared: &Shared, job: &Job) -> bool {
    let mut state = shared.lock();
    while state.reply.is_some() && !job.cancelled.load(Ordering::Acquire) && !state.shutdown() {
        state = shared
            .wake
            .wait(state)
            .unwrap_or_else(|error| error.into_inner());
    }
    !job.cancelled.load(Ordering::Acquire) && !state.shutdown()
}

fn publish(shared: &Shared, job: &Job, reply: Reply) {
    let mut state = shared.lock();
    if !job.cancelled.load(Ordering::Acquire) && !state.shutdown() {
        state.reply = Some(reply);
    }
    drop(state);
    shared.wake.notify_all();
}

struct Prepared {
    sources: Sources,
    audio: LimitedAudio,
    end: AudioSample,
}

fn prepare(shared: &Shared, job: &Job, retained: &mut Option<Prepared>) -> Result<(), String> {
    if job.cancelled.load(Ordering::Acquire) {
        return Ok(());
    }
    if !retained
        .as_ref()
        .is_some_and(|prepared| prepared.sources.matches(&job.snapshot))
    {
        // Drop the old cache before any new media admission, preserving the
        // aggregate limit across revision/session changes as well as seeks.
        *retained = None;
        let plan =
            Arc::new(RenderPlan::compile(&job.snapshot.document).map_err(|e| e.to_string())?);
        let end = plan.audio_duration().map_err(|e| e.to_string())?;
        let sources = Sources::new(job.snapshot.clone());
        // Canonical Preserve has an explicit full-input limit (~21.8 s at 48 kHz),
        // 128 MiB stereo stage residency and 64 stages. Exceeding it is an error.
        let audio = LimitedAudio::from_stages(
            StageAudio::with_limits(plan, StageLimits::default()).map_err(|e| e.to_string())?,
        );
        *retained = Some(Prepared {
            sources,
            audio,
            end,
        });
    }
    let prepared = retained.as_mut().ok_or("preparation cache is absent")?;
    let end = prepared.end;
    if job.start.0 > end.0 {
        return Err("playback start is past the sequence end".into());
    }
    let mut cursor = job.start;
    loop {
        if !wait_slot(shared, job) {
            return Ok(());
        }
        let start = cursor;
        let remaining = usize::try_from((end.0 - cursor.0).min(BATCH_FRAMES as i64))
            .map_err(|e| e.to_string())?;
        let mut samples = if remaining > 0 {
            let count = u32::try_from(remaining).map_err(|e| e.to_string())?;
            let block = prepared
                .audio
                .read(
                    &mut prepared.sources,
                    cursor,
                    count,
                    Duration::from_secs(60),
                    &job.cancelled,
                )
                .map_err(|e| e.to_string())?;
            if block.project_id != *job.snapshot.document.project_id()
                || block.revision_id != *job.snapshot.document.revision_id()
                || block.start != cursor
                || block.samples.len() != count as usize
            {
                return Err("canonical audio returned a foreign or incomplete block".into());
            }
            cursor.0 = cursor
                .0
                .checked_add(i64::from(count))
                .ok_or("playback sample overflow")?;
            block.samples
        } else {
            Vec::new()
        };
        for sample in &mut samples {
            for value in sample {
                *value *= job.gain;
                if !value.is_finite() || value.abs() > 1.0 {
                    return Err(
                        "limited audition exceeds the device range at this monitor level".into(),
                    );
                }
            }
        }
        let eos = cursor == end;
        publish(
            shared,
            job,
            Reply::Batch(Batch {
                epoch: job.epoch,
                start,
                end,
                samples,
                eos,
            }),
        );
        if eos {
            return Ok(());
        }
    }
}
