//! The same idle worker/cache ownership as committed definition analysis.

use super::*;
use crate::waveform::Target;

pub(super) fn measure(
    shared: &Shared,
    job: &crate::waveform::Job,
    memory: &WaveformMemory,
    retained: &mut Option<Retained>,
) {
    let mut update = job.edit_update(WaveformStatus::Measuring);
    shared.publish_waveform(job, update.clone(), false);
    let result = measure_inner(shared, job, memory, retained);
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
    #[cfg(test)]
    shared.observe_preparation(PreparationEvent::EditWaveformPublished, &job.cancelled);
}

fn measure_inner(
    shared: &Shared,
    job: &crate::waveform::Job,
    memory: &WaveformMemory,
    retained: &mut Option<Retained>,
) -> Result<deadpan_audio::EditWaveformMeasurement, String> {
    let Target::Edit {
        base,
        samples,
        limits,
    } = &job.target
    else {
        unreachable!("Edit waveform job")
    };
    if job.cancelled.load(Ordering::Acquire) {
        return Err("Edit waveform measurement was interrupted".into());
    }
    crate::EditWaveformRequest {
        base: base.clone(),
        snapshot: job.snapshot.clone(),
        samples: samples.clone(),
        limits: *limits,
    }
    .validate()
    .map_err(|error| error.to_string())?;
    // Identity alone does not retain edited-slice authority after its store
    // closes. Silent and empty windows must pass the same live admission even
    // though they never ask Sources for PCM.
    job.snapshot
        .check_media_live(&job.cancelled)
        .map_err(|error| format!("Edit waveform media admission expired: {error}"))?;
    // Release the prior exclusive cache before compiling/admitting this context.
    *retained = None;
    let plan =
        Arc::new(RenderPlan::compile(&job.snapshot.document).map_err(|error| error.to_string())?);
    if samples.end > plan.audio_duration().map_err(|error| error.to_string())? {
        return Err("Edit waveform range is outside its captured project".into());
    }
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
        return Err("Edit waveform cache is absent".into());
    };
    prepared
        .audio
        .measure_edit_window(
            &mut prepared.sources,
            samples.clone(),
            WaveformControl {
                limits: *limits,
                cancelled: &job.cancelled,
                memory,
            },
            |waveform| {
                let mut update = job.edit_update(WaveformStatus::Measuring);
                update.examined_samples =
                    u64::try_from(waveform.measured_end().0 - samples.start.0)
                        .expect("validated absolute waveform coverage");
                update.waveform = Some(waveform);
                #[cfg(test)]
                shared.observe_preparation(PreparationEvent::EditWaveformProgress, &job.cancelled);
                shared.publish_waveform(job, update, false);
            },
        )
        .map_err(|error| error.to_string())
}
