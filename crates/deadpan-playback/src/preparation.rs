//! All plan compilation, original verification, decoding and canonical DSP are
//! owned here. A bounded FIFO lets future canonical work continue while the
//! native device queue is full. Neither that FIFO nor DSP progress is a clock.

mod queue;
pub(crate) use queue::Queue;

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use deadpan_audio::{
    LimitedAudio, StageAudio, StageLimits, WaveformCompletion, WaveformControl, WaveformLimits,
    WaveformMemory,
};
use deadpan_core::AudioSample;
use deadpan_plan::{AudioDefinitionSelector, RenderPlan};

use crate::controller::{Job, Shared};
use crate::sources::Sources;
use crate::{Target, WaveformStatus, Window};

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PreparationEvent {
    PlaybackAdmitted,
    PlaybackBatchPrepared,
    WarmingAdmitted,
    WarmingBatchPrepared,
    WaveformAdmitted,
    WaveformReleased,
    EditWaveformProgress,
    EditWaveformPublished,
}

#[cfg(test)]
pub(crate) type Observer =
    Arc<dyn Fn(PreparationEvent, &std::sync::atomic::AtomicBool) + Send + Sync>;

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

enum Task {
    Playback(Arc<Job>),
    Waveform(Arc<crate::waveform::Job>, WaveformMemory),
    Warm(Arc<crate::warming::Job>),
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
                    break Task::Playback(job);
                }
                if state.can_analyze()
                    && let Some(job) = state.waveform.take_pending()
                {
                    break Task::Waveform(job, state.waveform.memory.clone());
                }
                if state.can_analyze()
                    && let Some(job) = state.warming.take_pending()
                {
                    break Task::Warm(job);
                }
                state = shared
                    .wake
                    .wait(state)
                    .unwrap_or_else(|error| error.into_inner());
            }
        };
        match job {
            Task::Playback(job) => {
                if let Err(error) = prepare(&shared, &job, &mut retained)
                    && !job.cancelled.load(Ordering::Acquire)
                    && publish(
                        &shared,
                        &job,
                        Reply::Failed {
                            epoch: job.epoch,
                            error,
                        },
                    )
                    .is_err()
                {
                    shared.worker_failed("audio preparation handoff");
                }
            }
            Task::Waveform(job, memory) => {
                shared
                    .lock()
                    .warming
                    .interrupt("Audio cache released for waveform analysis");
                measure(&shared, &job, &memory, &mut retained);
            }
            Task::Warm(job) => warm(&shared, &job, &mut retained),
        }
    }
}

fn wait_slot(shared: &Shared, job: &Job) -> bool {
    let mut state = shared.lock();
    while state.reply.is_full() && !job.cancelled.load(Ordering::Acquire) && !state.shutdown() {
        state = shared
            .wake
            .wait(state)
            .unwrap_or_else(|error| error.into_inner());
    }
    !job.cancelled.load(Ordering::Acquire) && !state.shutdown()
}

fn publish(shared: &Shared, job: &Job, reply: Reply) -> Result<(), String> {
    let mut state = shared.lock();
    if !job.cancelled.load(Ordering::Acquire) && !state.shutdown() {
        state.reply.push(reply)?;
    }
    drop(state);
    shared.wake.notify_all();
    Ok(())
}

struct Prepared {
    target: Target,
    sources: Sources,
    audio: LimitedAudio,
    end: AudioSample,
}

struct AnalysisPrepared {
    plan: Arc<RenderPlan>,
    sources: Sources,
    audio: StageAudio,
}

enum Retained {
    Playback(Prepared),
    Waveform(AnalysisPrepared),
}

fn admit(
    snapshot: &Arc<crate::Snapshot>,
    target: &Target,
    cancelled: &std::sync::atomic::AtomicBool,
    retained: &mut Option<Retained>,
) -> Result<bool, String> {
    snapshot
        .validate_admission()
        .map_err(|error| error.to_string())?;
    snapshot
        .check_media_live(cancelled)
        .map_err(|error| error.to_string())?;
    if retained.as_ref().is_some_and(|retained| {
        matches!(retained, Retained::Playback(prepared)
            if prepared.target == *target && prepared.sources.matches(snapshot))
    }) {
        return Ok(false);
    }
    // Drop the old cache before new admission, preserving aggregate budgets.
    *retained = None;
    let document = target.document(snapshot)?;
    let plan = Arc::new(RenderPlan::compile(&document).map_err(|e| e.to_string())?);
    let end = target.effective_end(plan.audio_duration().map_err(|e| e.to_string())?)?;
    *retained = Some(Retained::Playback(Prepared {
        target: target.clone(),
        sources: Sources::new(Arc::clone(snapshot)),
        audio: LimitedAudio::from_stages(
            StageAudio::with_limits(plan, StageLimits::default()).map_err(|e| e.to_string())?,
        ),
        end,
    }));
    Ok(true)
}

fn prepare(shared: &Shared, job: &Job, retained: &mut Option<Retained>) -> Result<(), String> {
    if job.cancelled.load(Ordering::Acquire) {
        return Ok(());
    }
    let admission = job
        .monitor
        .enter(deadpan_audio::PreparationActivity::CheckingPlan);
    if admit(&job.snapshot, &job.target, &job.cancelled, retained)? {
        #[cfg(test)]
        shared.observe_preparation(PreparationEvent::PlaybackAdmitted, &job.cancelled);
    }
    let Some(Retained::Playback(prepared)) = retained.as_mut() else {
        return Err("playback preparation cache is absent".into());
    };
    prepared
        .audio
        .set_preparation_monitor(Some(Arc::clone(&job.monitor)));
    prepared
        .sources
        .set_preparation_monitor(Some(Arc::clone(&job.monitor)));
    drop(admission);
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
        #[cfg(test)]
        shared.observe_preparation(PreparationEvent::PlaybackBatchPrepared, &job.cancelled);
        // Cached canonical blocks and short-loop replication can bypass source
        // lookups. Recheck the owning session before publishing their PCM too.
        job.snapshot
            .check_media_live(&job.cancelled)
            .map_err(|error| error.to_string())?;
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
        )?;
        if eos {
            return Ok(());
        }
    }
}

fn warm(shared: &Shared, job: &crate::warming::Job, retained: &mut Option<Retained>) {
    let mut cursor = job.start;
    let mut paused = false;
    let result = (|| {
        let admission = job
            .monitor
            .enter(deadpan_audio::PreparationActivity::CheckingPlan);
        if job.cancelled.load(Ordering::Acquire) {
            return Ok(());
        }
        if job.promoted.load(Ordering::Acquire) {
            return Ok(());
        }
        if let Some(reason) = (shared.warming_guard)() {
            paused = true;
            return Err(reason);
        }
        let mut policy_checked = Instant::now();
        admit(&job.snapshot, &job.target, &job.cancelled, retained)?;
        #[cfg(test)]
        shared.observe_preparation(PreparationEvent::WarmingAdmitted, &job.cancelled);
        let Some(Retained::Playback(prepared)) = retained.as_mut() else {
            return Err("audio warming cache is absent".to_owned());
        };
        prepared
            .audio
            .set_preparation_monitor(Some(Arc::clone(&job.monitor)));
        prepared
            .sources
            .set_preparation_monitor(Some(Arc::clone(&job.monitor)));
        drop(admission);
        if cursor.0 < 0 || cursor >= prepared.end {
            return Err("audio warming starts outside the captured audio".to_owned());
        }
        let end = AudioSample(
            cursor
                .0
                .saturating_add(crate::WARM_HORIZON_FRAMES)
                .min(prepared.end.0),
        );
        let deadline = Instant::now() + Duration::from_secs(60);
        while cursor < end
            && !job.cancelled.load(Ordering::Acquire)
            && !job.promoted.load(Ordering::Acquire)
        {
            if policy_checked.elapsed() >= Duration::from_secs(2) {
                if let Some(reason) = (shared.warming_guard)() {
                    paused = true;
                    return Err(reason);
                }
                policy_checked = Instant::now();
            }
            let count = u32::try_from((end.0 - cursor.0).min(BATCH_FRAMES as i64))
                .map_err(|e| e.to_string())?;
            let block = prepared
                .audio
                .read(
                    &mut prepared.sources,
                    cursor,
                    count,
                    deadline
                        .checked_duration_since(Instant::now())
                        .ok_or("audio warming timed out")?,
                    &job.cancelled,
                )
                .map_err(|e| e.to_string())?;
            if block.project_id != *job.snapshot.document.project_id()
                || block.revision_id != *job.snapshot.document.revision_id()
                || block.start != cursor
                || block.samples.len() != count as usize
            {
                return Err("audio warming returned a foreign or incomplete block".into());
            }
            cursor.0 += i64::from(count);
            job.snapshot
                .check_media_live(&job.cancelled)
                .map_err(|e| e.to_string())?;
            #[cfg(test)]
            shared.observe_preparation(PreparationEvent::WarmingBatchPrepared, &job.cancelled);
            shared.publish_warming(job, crate::WarmStatus::Preparing, cursor, None);
        }
        Ok(())
    })();
    let (status, error) = match result {
        Err(error) if paused => (crate::WarmStatus::Interrupted, Some(error)),
        Err(error) => (crate::WarmStatus::Unavailable, Some(error)),
        Ok(()) if job.promoted.load(Ordering::Acquire) => (
            crate::WarmStatus::Interrupted,
            Some("Prepared audio handed to audition".into()),
        ),
        Ok(()) => (crate::WarmStatus::Complete, None),
    };
    shared.publish_warming(job, status, cursor, error);
}

fn measure(
    shared: &Shared,
    job: &crate::waveform::Job,
    memory: &WaveformMemory,
    retained: &mut Option<Retained>,
) {
    match &job.target {
        crate::waveform::Target::Definition(_) => measure_definition(shared, job, memory, retained),
        crate::waveform::Target::Edit { .. } => {
            edit_waveform::measure(shared, job, memory, retained)
        }
    }
}

mod edit_waveform;

fn measure_definition(
    shared: &Shared,
    job: &crate::waveform::Job,
    memory: &WaveformMemory,
    retained: &mut Option<Retained>,
) {
    let mut update = job.definition_update(WaveformStatus::Measuring);
    shared.publish_waveform(job, update.clone(), false);
    let result = measure_inner(shared, job, memory, retained);
    // Peaks retain their own bounded allocation; media and DSP are never kept
    // alive by a completed/cancelled overview or its UI-held result.
    *retained = None;
    #[cfg(test)]
    shared.observe_preparation(PreparationEvent::WaveformReleased, &job.cancelled);
    match result {
        Ok(measurement) => {
            update.waveform = Some(measurement.waveform);
            update.examined_samples = measurement.examined_samples;
            match measurement.completion {
                WaveformCompletion::Complete => update.status = WaveformStatus::Complete,
                WaveformCompletion::Partial(reason) => {
                    update.status = WaveformStatus::Partial;
                    update.error = Some(reason.to_string());
                }
            }
        }
        Err(error) => {
            update.status = WaveformStatus::Unavailable;
            update.error = Some(error);
        }
    }
    shared.publish_waveform(job, update, true);
}

fn measure_inner(
    shared: &Shared,
    job: &crate::waveform::Job,
    memory: &WaveformMemory,
    retained: &mut Option<Retained>,
) -> Result<deadpan_audio::WaveformMeasurement, String> {
    if job.cancelled.load(Ordering::Acquire) {
        return Err("Waveform analysis was interrupted".into());
    }
    job.snapshot
        .validate_admission()
        .map_err(|error| error.to_string())?;
    if job.snapshot.content != crate::ContentIdentity::Committed {
        return Err("waveform analysis requires a committed base snapshot".into());
    }
    // A new request admits its captured evidence anew. No playback and analysis
    // source/stage owners may coexist, even while a cold replacement opens.
    *retained = None;
    let plan =
        Arc::new(RenderPlan::compile(&job.snapshot.document).map_err(|error| error.to_string())?);
    let crate::waveform::Target::Definition(owner) = &job.target else {
        unreachable!("definition job")
    };
    let selector = AudioDefinitionSelector::Node {
        node: owner.clone(),
    };
    plan.audio_definition(selector.clone())
        .map_err(|error| error.to_string())?;
    let audio = StageAudio::with_limits(plan.clone(), StageLimits::default())
        .map_err(|error| error.to_string())?;
    *retained = Some(Retained::Waveform(AnalysisPrepared {
        plan,
        sources: Sources::new(job.snapshot.clone()),
        audio,
    }));
    #[cfg(test)]
    shared.observe_preparation(PreparationEvent::WaveformAdmitted, &job.cancelled);
    let Some(Retained::Waveform(prepared)) = retained.as_mut() else {
        return Err("waveform preparation cache is absent".into());
    };
    let definition = prepared
        .plan
        .audio_definition(selector)
        .map_err(|error| error.to_string())?;
    prepared
        .audio
        .measure_definition(
            &mut prepared.sources,
            &definition,
            WaveformControl {
                limits: WaveformLimits::default(),
                cancelled: &job.cancelled,
                memory,
            },
            |waveform| {
                let mut update = job.definition_update(WaveformStatus::Measuring);
                update.examined_samples = u64::try_from(waveform.measured_end().0)
                    .expect("validated waveform coverage starts at sample zero");
                update.waveform = Some(waveform);
                shared.publish_waveform(job, update, false);
            },
        )
        .map_err(|error| error.to_string())
}
