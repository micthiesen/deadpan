use super::*;

/// The deterministic replacement for the model worker. It reports progress
/// and ends in a cancellation, a failure, or synthetic footage that the
/// production qualification and publication path makes Ready.
pub(super) fn scripted(
    script: &crate::project::generation::Script,
    allocated: &Allocated,
    mut progress: impl FnMut(AttemptProgress),
    mut records: impl FnMut(AttemptRecord) -> std::result::Result<(), String>,
    cancelled: &AtomicBool,
    handle: &crate::jobs::JobHandle,
) -> WorkerRun {
    use crate::project::generation::ScriptEnding;
    use deadpan_jobs::WorkerStage;
    let cancel = |records: &mut dyn FnMut(AttemptRecord) -> std::result::Result<(), String>| {
        let _ = records(AttemptRecord::CancelRequested);
        WorkerRun::without_workspace(RunResult::Cancelled, RunTimings::default())
    };
    progress(AttemptProgress::Preparing);
    if cancelled.load(Ordering::Acquire) {
        return cancel(&mut records);
    }
    progress(AttemptProgress::Stage(WorkerStage::ModelLoading));
    for completed in 1..=script.steps {
        if handle.checkpoint(Some(cancelled)).is_err() {
            return cancel(&mut records);
        }
        std::thread::sleep(script.step_interval);
        progress(AttemptProgress::Step {
            stage: WorkerStage::Inference,
            completed,
            total: script.steps,
        });
    }
    match &script.ending {
        ScriptEnding::WaitForCancel => {
            while !cancelled.load(Ordering::Acquire) {
                if handle.checkpoint(Some(cancelled)).is_err() {
                    return cancel(&mut records);
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            cancel(&mut records)
        }
        ScriptEnding::Process(pid) => {
            let failed = |reason: String| {
                WorkerRun::without_workspace(
                    RunResult::Failed(JobFailure::Host(attempt::host_failure(
                        HostFailureCode::WorkerExited,
                        &reason,
                    ))),
                    RunTimings::default(),
                )
            };
            let mut child = match deadpan_native_process::spawn(
                std::process::Command::new("/bin/sleep")
                    .arg("600")
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null()),
            ) {
                Ok(child) => child,
                Err(error) => return failed(format!("the scripted worker did not start: {error}")),
            };
            pid.store(child.id(), Ordering::Release);
            loop {
                if cancelled.load(Ordering::Acquire) {
                    let _ = child.kill();
                    let _ = child.wait();
                    return cancel(&mut records);
                }
                match child.try_wait() {
                    Ok(Some(status)) => {
                        return failed(format!(
                            "the AI worker process exited unexpectedly ({status})"
                        ));
                    }
                    Ok(None) => std::thread::sleep(Duration::from_millis(5)),
                    Err(error) => {
                        let _ = child.kill();
                        let _ = child.wait();
                        return failed(format!(
                            "the AI worker process could not be observed: {error}"
                        ));
                    }
                }
            }
        }
        ScriptEnding::Fail(reason) => WorkerRun::without_workspace(
            RunResult::Failed(JobFailure::Host(attempt::host_failure(
                HostFailureCode::WorkerExited,
                reason,
            ))),
            RunTimings::default(),
        ),
        ScriptEnding::Ready => match crate::project::generation::synthetic_tools() {
            Ok(worker) => attempt::synthetic::run(allocated, &worker, progress, records, cancelled),
            Err(reason) => WorkerRun::without_workspace(
                RunResult::Failed(JobFailure::Host(attempt::host_failure(
                    HostFailureCode::WorkerExited,
                    &reason,
                ))),
                RunTimings::default(),
            ),
        },
    }
}
