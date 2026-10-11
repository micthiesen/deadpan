use super::*;

#[test]
fn preparation_progress_is_live_without_device_activation_and_supersession_stays_responsive() {
    let permit = resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let store =
        ProjectStore::create(&directory.path().join("progress.deadpan"), &hold(60)).unwrap();
    let snapshot = snapshot(&store, 117);
    let (engine, devices) = engine(&permit);
    let entered = Arc::new(AtomicBool::new(false));
    let gate = Arc::clone(&entered);
    *engine.shared.preparation_observer.lock().unwrap() =
        Some(Arc::new(move |event, cancelled| {
            if event == crate::preparation::PreparationEvent::PlaybackAdmitted
                && !gate.swap(true, Ordering::AcqRel)
            {
                let deadline = Instant::now() + Duration::from_secs(10);
                while !cancelled.load(Ordering::Acquire) && Instant::now() < deadline {
                    thread::sleep(Duration::from_millis(1));
                }
                assert!(
                    cancelled.load(Ordering::Acquire),
                    "first request must be superseded"
                );
            }
        }));
    engine
        .play(1, Arc::clone(&snapshot), AudioSample(0), 0.25)
        .unwrap();
    assert!(wait(|| entered.load(Ordering::Acquire)));
    let mut progress = None;
    assert!(wait(|| {
        progress = engine.poll().or(progress.take());
        progress.as_ref().is_some_and(|update| {
            update.preparation.is_some_and(|preparation| {
                preparation.elapsed_millis >= 100
                    && preparation.activity == deadpan_audio::PreparationActivity::CheckingPlan
            })
        })
    }));
    let progress = progress.unwrap();
    assert_eq!(progress.phase, Phase::Preparing);
    assert_eq!(
        progress.sample, None,
        "work progress cannot become the heard clock"
    );
    assert_eq!(progress.ticket, 1);
    assert!(!devices.lock().unwrap()[0].started.load(Ordering::Acquire));
    engine.play(2, snapshot, AudioSample(1600), 0.25).unwrap();
    let replacement = playing_device(&engine, &devices, 1);
    replacement.render(256, 0, 10_000_000);
    replacement.now.store(10_100_000, Ordering::Release);
    let playing = update(&engine, Phase::Playing);
    assert_eq!(playing.ticket, 2);
    assert_eq!(playing.preparation, None);
    assert!(playing.sample.unwrap().0 >= 1600);
    assert_eq!(
        engine.diagnostics().generations,
        1,
        "cancelled preparation never activated"
    );
}
