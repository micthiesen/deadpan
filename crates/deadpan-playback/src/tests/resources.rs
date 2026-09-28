//! Serialize this test binary's real-media work, including asynchronous teardown.
//! Transport deadlines measure one scenario, not competing debug DSP workloads.

use std::sync::{Arc, Condvar, LazyLock, Mutex};
use std::time::Duration;

#[derive(Default)]
struct PcmSlot {
    occupied: Mutex<bool>,
    available: Condvar,
}

pub(crate) struct PcmPermit {
    slot: Arc<PcmSlot>,
}

impl PcmSlot {
    fn acquire(self: &Arc<Self>) -> Arc<PcmPermit> {
        // This includes queueing behind whole test scenarios, not just one
        // worker wait. A leaked detached worker must still fail the suite
        // instead of leaving every later PCM scenario blocked indefinitely.
        self.acquire_with_timeout(Duration::from_secs(600))
            .expect("PCM test reservation stayed occupied for ten minutes; a test or detached worker did not finish")
    }

    fn acquire_with_timeout(self: &Arc<Self>, timeout: Duration) -> Option<Arc<PcmPermit>> {
        let (mut occupied, _) = self
            .available
            .wait_timeout_while(self.occupied.lock().unwrap(), timeout, |occupied| *occupied)
            .unwrap();
        if *occupied {
            return None;
        }
        *occupied = true;
        Some(Arc::new(PcmPermit { slot: self.clone() }))
    }
}

impl Drop for PcmPermit {
    fn drop(&mut self) {
        *self.slot.occupied.lock().unwrap() = false;
        self.slot.available.notify_one();
    }
}

pub(crate) fn pcm() -> Arc<PcmPermit> {
    static SLOT: LazyLock<Arc<PcmSlot>> = LazyLock::new(|| Arc::new(PcmSlot::default()));
    SLOT.acquire()
}

pub(super) fn repaint(permit: &Arc<PcmPermit>) -> Arc<dyn Fn() + Send + Sync> {
    let permit = permit.clone();
    // Shared owns this callback for both worker lifetimes. Engine::drop only
    // requests shutdown: releasing the test's permit there would admit another
    // PCM workload before the previous preparation worker had finished.
    Arc::new(move || {
        let _keep_alive = &permit;
    })
}

#[test]
fn pcm_slot_stays_owned_until_detached_engine_workers_exit() {
    use std::sync::mpsc;

    let slot = Arc::new(PcmSlot::default());
    let permit = slot.acquire();
    let (entered, opening) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let directory = tempfile::tempdir().unwrap();
    let store = deadpan_store::ProjectStore::create(
        &directory.path().join("project.deadpan"),
        &super::hold(1),
    )
    .unwrap();
    let engine = crate::Engine::with_factory(
        repaint(&permit),
        Box::new(move || {
            entered.send(()).unwrap();
            released.recv_timeout(Duration::from_secs(10)).unwrap();
            Err("injected open failure".into())
        }),
    )
    .unwrap();
    engine
        .play(
            1,
            super::snapshot(&store, 1),
            deadpan_core::AudioSample(0),
            0.1,
        )
        .unwrap();
    opening.recv_timeout(Duration::from_secs(10)).unwrap();
    drop(engine);
    drop(permit);
    assert!(*slot.occupied.lock().unwrap());
    // Exercise bounded admission while a real detached worker still owns the
    // slot. Timeout neither steals the reservation nor poisons its mutex.
    assert!(slot.acquire_with_timeout(Duration::ZERO).is_none());
    release.send(()).unwrap();
    assert!(super::wait(|| !*slot.occupied.lock().unwrap()));
    // A completed worker lifetime releases the reservation for the next test.
    drop(slot.acquire_with_timeout(Duration::ZERO).unwrap());
}

#[test]
fn waveform_shutdown_keeps_pcm_permit_until_its_preparation_owner_exits() {
    use crate::preparation::PreparationEvent;
    use std::sync::mpsc;

    let slot = Arc::new(PcmSlot::default());
    let permit = slot.acquire();
    let (entered, admitted) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let released = Mutex::new(released);
    let directory = tempfile::tempdir().unwrap();
    let store = deadpan_store::ProjectStore::create(
        &directory.path().join("waveform-resources.deadpan"),
        &super::hold(1),
    )
    .unwrap();
    let engine = crate::Engine::with_factory(
        repaint(&permit),
        Box::new(|| panic!("analysis must not open a device")),
    )
    .unwrap();
    *engine.shared.preparation_observer.lock().unwrap() =
        Some(Arc::new(move |event, cancelled| {
            if event == PreparationEvent::WaveformAdmitted {
                entered.send(()).unwrap();
                released
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(10))
                    .unwrap();
                assert!(cancelled.load(std::sync::atomic::Ordering::Acquire));
            }
        }));
    engine
        .request_waveform(crate::WaveformRequest {
            snapshot: super::snapshot(&store, 109),
            owner: super::node("pause"),
        })
        .unwrap();
    admitted.recv_timeout(Duration::from_secs(10)).unwrap();
    drop(engine);
    drop(permit);
    assert!(*slot.occupied.lock().unwrap());
    assert!(slot.acquire_with_timeout(Duration::ZERO).is_none());
    release.send(()).unwrap();
    assert!(super::wait(|| !*slot.occupied.lock().unwrap()));
    drop(slot.acquire_with_timeout(Duration::ZERO).unwrap());
}
