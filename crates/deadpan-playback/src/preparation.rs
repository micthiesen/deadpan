//! All plan compilation, original verification, decoding and canonical DSP are
//! owned here. One completed mailbox batch and one controller/producer batch
//! bound the handoff to at most 16,384 stereo frames, plus the native queue.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use deadpan_audio::{LimitedAudio, StageAudio, StageLimits};
use deadpan_core::AudioSample;
use deadpan_plan::RenderPlan;

use crate::controller::{Job, Shared};
use crate::sources::Sources;
use crate::{Target, Window};

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
    target: Target,
    sources: Sources,
    audio: LimitedAudio,
    end: AudioSample,
}

fn prepare(shared: &Shared, job: &Job, retained: &mut Option<Prepared>) -> Result<(), String> {
    if job.cancelled.load(Ordering::Acquire) {
        return Ok(());
    }
    if !retained.as_ref().is_some_and(|prepared| {
        prepared.target == job.target && prepared.sources.matches(&job.snapshot)
    }) {
        // Drop the old cache before any new media admission, preserving the
        // aggregate limit across revision/session changes as well as seeks.
        *retained = None;
        let document = job.target.document(&job.snapshot)?;
        let plan = Arc::new(RenderPlan::compile(&document).map_err(|e| e.to_string())?);
        let end = job
            .target
            .effective_end(plan.audio_duration().map_err(|e| e.to_string())?)?;
        let sources = Sources::new(job.snapshot.clone());
        // Canonical Preserve has an explicit full-input limit (~21.8 s at 48 kHz),
        // 128 MiB stereo stage residency and 64 stages. Exceeding it is an error.
        let audio = LimitedAudio::from_stages(
            StageAudio::with_limits(plan, StageLimits::default()).map_err(|e| e.to_string())?,
        );
        *retained = Some(Prepared {
            target: job.target.clone(),
            sources,
            audio,
            end,
        });
    }
    let prepared = retained.as_mut().ok_or("preparation cache is absent")?;
    let window = job
        .window
        .unwrap_or(Window::new(AudioSample(0), prepared.end, false).map_err(|e| e.to_string())?);
    window.validate(prepared.end, job.start)?;
    let mut cursor = job.start;
    loop {
        if !wait_slot(shared, job) {
            return Ok(());
        }
        let start = cursor;
        let deadline = Instant::now() + Duration::from_secs(60);
        let mut samples = Vec::new();
        while samples.len() < BATCH_FRAMES {
            if job.cancelled.load(Ordering::Acquire) {
                return Ok(());
            }
            let period = window.end().0 - window.start().0;
            if window.looping() && period <= samples.len() as i64 {
                // A complete, verified lap is already in this batch. Reuse it
                // within the bounded output buffer, including a partial first
                // lap after resume. Tiny loops must not turn one batch into
                // thousands of repeated provenance queries and allocations.
                let period = usize::try_from(period).map_err(|e| e.to_string())?;
                let from = samples.len() % period;
                let count = (BATCH_FRAMES - samples.len()).min(samples.len() - from);
                cursor.0 = cursor
                    .0
                    .checked_add(count as i64)
                    .ok_or("playback sample overflow")?;
                samples.reserve_exact(count);
                samples.extend_from_within(from..from + count);
                continue;
            }
            let canonical = window
                .sample(cursor)
                .ok_or("playback delivery sample is out of range")?;
            let remaining = usize::try_from(
                (window.end().0 - canonical.0).min((BATCH_FRAMES - samples.len()) as i64),
            )
            .map_err(|e| e.to_string())?;
            if remaining == 0 {
                break;
            }
            let count = u32::try_from(remaining).map_err(|e| e.to_string())?;
            // Each read ends at a content seam. The shared reader still sees
            // the full plan and retains limiter/stage context across every lap.
            let block = prepared
                .audio
                .read(
                    &mut prepared.sources,
                    canonical,
                    count,
                    deadline
                        .checked_duration_since(Instant::now())
                        .ok_or("audition batch preparation timed out")?,
                    &job.cancelled,
                )
                .map_err(|e| e.to_string())?;
            if block.project_id != *job.snapshot.document.project_id()
                || block.revision_id != *job.snapshot.document.revision_id()
                || block.start != canonical
                || block.samples.len() != count as usize
            {
                return Err("canonical audio returned a foreign or incomplete block".into());
            }
            cursor.0 = cursor
                .0
                .checked_add(i64::from(count))
                .ok_or("playback sample overflow")?;
            if samples.is_empty() {
                samples = block.samples;
            } else {
                samples.reserve_exact(block.samples.len());
                samples.extend(block.samples);
            }
        }
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
        let eos = !window.looping() && cursor == window.end();
        publish(
            shared,
            job,
            Reply::Batch(Batch {
                epoch: job.epoch,
                start,
                end: window.delivery_end(),
                samples,
                eos,
            }),
        );
        if eos {
            return Ok(());
        }
    }
}
