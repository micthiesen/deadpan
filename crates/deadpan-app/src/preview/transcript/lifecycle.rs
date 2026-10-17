//! A recognition run retains its sleep epoch through worker drain and publication.

use super::*;

#[cfg(test)]
mod real;
#[cfg(test)]
mod real_activity;
use std::sync::Mutex;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Stop {
    Cancelled,
    Interrupted,
}

#[derive(Clone)]
pub(super) struct RunCapture {
    lifecycle: crate::jobs::Lifecycle,
    pub epoch: u64,
    cancel: Arc<AtomicBool>,
    cause: Arc<Mutex<Option<Stop>>>,
}

impl RunCapture {
    pub fn new(lifecycle: crate::jobs::Lifecycle, cancel: Arc<AtomicBool>) -> Self {
        let epoch = lifecycle.snapshot();
        Self {
            lifecycle,
            epoch,
            cancel,
            cause: Arc::new(Mutex::new(None)),
        }
    }

    /// Cancellation already requested when interruption is observed takes
    /// precedence. This does not claim chronological ordering of racing events.
    /// Retain our own cause so the flag we set is never read as user cancellation.
    pub fn stop(&self) -> Option<Stop> {
        let mut cause = self.cause.lock().unwrap_or_else(|error| error.into_inner());
        if cause.is_none() {
            if self.cancel.load(Ordering::Acquire) {
                *cause = Some(Stop::Cancelled);
            } else if self.lifecycle.interrupted_since(self.epoch) {
                *cause = Some(Stop::Interrupted);
                self.cancel.store(true, Ordering::Release);
            }
        }
        *cause
    }

    /// The scoped watcher can cancel native inference without waiting for a
    /// UI repaint or a progress callback. It ends when this worker returns.
    pub fn run<T>(&self, work: impl FnOnce() -> T) -> T {
        let done = AtomicBool::new(false);
        std::thread::scope(|scope| {
            scope.spawn(|| {
                while !done.load(Ordering::Acquire) {
                    if self.stop().is_some() {
                        break;
                    }
                    std::thread::park_timeout(Duration::from_millis(20));
                }
            });
            struct Finish<'a>(&'a AtomicBool);
            impl Drop for Finish<'_> {
                fn drop(&mut self) {
                    self.0.store(true, Ordering::Release);
                }
            }
            let _finish = Finish(&done);
            work()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sleeping_native_work_is_cancelled_without_ui_or_progress_callbacks() {
        let lifecycle = crate::jobs::Lifecycle::default();
        let cancel = Arc::new(AtomicBool::new(false));
        let run = RunCapture::new(lifecycle.clone(), cancel.clone());
        run.run(|| {
            lifecycle.will_sleep();
            lifecycle.did_wake();
            let deadline = std::time::Instant::now() + Duration::from_secs(2);
            while !cancel.load(Ordering::Acquire) {
                assert!(
                    std::time::Instant::now() < deadline,
                    "native worker cancellation was not signalled"
                );
                std::thread::park_timeout(Duration::from_millis(5));
            }
        });
        assert_eq!(run.stop(), Some(Stop::Interrupted));
        assert_eq!(
            run.stop(),
            Some(Stop::Interrupted),
            "own cancellation flag cannot change the cause"
        );
    }

    #[test]
    fn cancellation_already_requested_takes_precedence_over_coalesced_sleep() {
        let lifecycle = crate::jobs::Lifecycle::default();
        let cancel = Arc::new(AtomicBool::new(false));
        let run = RunCapture::new(lifecycle.clone(), cancel.clone());
        cancel.store(true, Ordering::Release);
        lifecycle.will_sleep();
        lifecycle.did_wake();
        assert_eq!(run.stop(), Some(Stop::Cancelled));
    }

    fn result() -> (TranscriptKey, Transcript) {
        (
            TranscriptKey {
                content: "a".repeat(64),
                audio_stream: 0,
                model_sha256: "b".repeat(64),
                language: "en".into(),
                engine: "test".into(),
            },
            Transcript::new(
                deadpan_analysis::AnalysedAudio {
                    origin: 0,
                    sample_rate: 48_000,
                    duration_cs: 100,
                },
                vec![],
            )
            .unwrap(),
        )
    }

    #[test]
    fn queued_and_completed_recognition_stay_interrupted_until_explicit_retry() {
        for buffered in [false, true] {
            let lifecycle = crate::jobs::Lifecycle::default();
            let cancel = Arc::new(AtomicBool::new(false));
            let run = RunCapture::new(lifecycle.clone(), cancel.clone());
            let (sender, receiver) = mpsc::channel();
            let mut state = Transcription {
                status: Status::Preparing,
                cancel,
                run: Some(run.clone()),
                events: Some(receiver),
                ..Transcription::default()
            };
            if buffered {
                let (key, transcript) = result();
                state.unsaved = Some((key, Arc::new(transcript), run.epoch));
            }
            lifecycle.will_sleep();
            lifecycle.did_wake();
            state.observe_interruption();
            assert_eq!(state.status, Status::Interrupted);
            assert!(state.unsaved.is_none());
            assert!(
                !state.retry_interrupted(&lifecycle),
                "cannot retry while the old worker drains"
            );
            sender.send(Event::Progress(99)).unwrap();
            let event = state.events.as_ref().unwrap().recv().unwrap();
            assert!(!state.accepts_event(&event));
            assert!(!state.accepts_event(&Event::Transcribed(Ok(result()))));
            assert!(state.events.is_none());
            assert!(state.unsaved.is_none());
            state.observe_interruption();
            assert_eq!(
                state.status,
                Status::Interrupted,
                "wake does not restart recognition"
            );
            assert!(state.retry_interrupted(&lifecycle));
            assert_eq!(state.status, Status::Unchecked);
            let retry = RunCapture::new(lifecycle, Arc::new(AtomicBool::new(false)));
            assert_ne!(retry.epoch, run.epoch);
            assert_eq!(retry.stop(), None);
        }
    }

    #[test]
    fn recognize_again_interruption_keeps_ready_state_and_rejects_old_completion() {
        let lifecycle = crate::jobs::Lifecycle::default();
        let cancel = Arc::new(AtomicBool::new(false));
        let run = RunCapture::new(lifecycle.clone(), cancel.clone());
        let mut state = Transcription {
            status: Status::Ready,
            cancel,
            run: Some(run),
            again: Some(recognition::Again::Queued),
            ..Transcription::default()
        };
        // Existing words/corrections live in Workspace; this state machine must
        // neither mutate them nor admit the late replacement into unsaved.
        lifecycle.will_sleep();
        state.observe_interruption();
        assert!(matches!(state.again, Some(recognition::Again::Interrupted)));
        assert_eq!(state.status, Status::Ready);
        assert!(!state.accepts_event(&Event::Transcribed(Ok(result()))));
        assert!(state.unsaved.is_none());
    }
    #[test]
    fn completed_recognition_does_not_interrupt_independent_activity() {
        let lifecycle = crate::jobs::Lifecycle::default();
        let mut state = Transcription {
            status: Status::Ready,
            run: Some(RunCapture::new(
                lifecycle.clone(),
                Arc::new(AtomicBool::new(false)),
            )),
            ..Transcription::default()
        };
        lifecycle.will_sleep();
        lifecycle.did_wake();
        let activity_cancel = Arc::new(AtomicBool::new(false));
        state
            .activity
            .start(RunCapture::new(lifecycle.clone(), activity_cancel.clone()));
        state.observe_interruption();
        assert_eq!(state.status, Status::Ready);
        assert!(!activity_cancel.load(Ordering::Acquire));
        assert!(state.activity.not_ready().unwrap().contains("while"));
        lifecycle.will_sleep();
        state.observe_interruption();
        assert!(activity_cancel.load(Ordering::Acquire));
        assert!(state.activity.not_ready().unwrap().contains("sleep"));
    }
    #[test]
    fn save_receipt_before_ui_sleep_observation_preserves_publication_outcome() {
        for again in [false, true] {
            // A successful admitted commit wins; only a typed guard rejection
            // is Interrupted. A real storage error remains a failure even if
            // sleep happened before the UI received its receipt.
            for (interrupted, error) in [
                (false, None),
                (false, Some("disk failure")),
                (true, Some("sleep guard")),
            ] {
                let lifecycle = crate::jobs::Lifecycle::default();
                let cancel = Arc::new(AtomicBool::new(false));
                let mut state = Transcription {
                    session: Some(7),
                    status: if again {
                        Status::Ready
                    } else {
                        Status::Saving(4)
                    },
                    run: Some(RunCapture::new(lifecycle.clone(), cancel)),
                    again: again.then_some(recognition::Again::Saving { attempt: 4 }),
                    ..Transcription::default()
                };
                lifecycle.will_sleep();
                lifecycle.did_wake();
                // UI may see sleep before the receipt as well as after it;
                // pending saves must retain their attempt identity either way.
                state.observe_interruption();
                state.receive_save(Some(crate::project::TranscriptSave {
                    session: 7,
                    attempt: 4,
                    interrupted,
                    error: error.map(str::to_owned),
                }));
                state.observe_interruption();
                assert!(state.run.is_none());
                if again {
                    match (&state.again, interrupted, error) {
                        (None, false, None)
                        | (Some(recognition::Again::Interrupted), true, Some(_)) => {}
                        (Some(recognition::Again::Failed { error }), false, Some(_)) => {
                            assert!(error.contains("disk failure"))
                        }
                        _ => panic!("receipt lost its publication outcome"),
                    }
                } else if interrupted {
                    assert_eq!(state.status, Status::Interrupted);
                } else if error.is_some() {
                    assert!(
                        matches!(state.status, Status::Failed(ref error) if error.contains("disk failure"))
                    );
                } else {
                    // Published Workspace will move Saving to Ready.
                    assert_eq!(state.status, Status::Saving(4));
                }
            }
        }
    }

    #[test]
    fn guard_receipt_preserves_cancellation_already_requested() {
        let lifecycle = crate::jobs::Lifecycle::default();
        let cancel = Arc::new(AtomicBool::new(false));
        let mut state = Transcription {
            session: Some(7),
            status: Status::Saving(4),
            run: Some(RunCapture::new(lifecycle.clone(), cancel.clone())),
            ..Transcription::default()
        };
        cancel.store(true, Ordering::Release);
        lifecycle.will_sleep();
        state.receive_save(Some(crate::project::TranscriptSave {
            session: 7,
            attempt: 4,
            interrupted: true,
            error: Some("sleep guard".into()),
        }));
        assert!(
            matches!(state.status, Status::Failed(ref error) if error == "transcription was cancelled")
        );
    }
}
