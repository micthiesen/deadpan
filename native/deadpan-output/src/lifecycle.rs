//! Narrow macOS workspace sleep/wake observation. Registration and removal
//! stay on the main thread; callbacks capture only owned Send + Sync state.
//! The unsafe calls below are the typed Objective-C notification boundary.
#![allow(unsafe_code)]

use std::{ptr::NonNull, sync::Arc};

use block2::RcBlock;
use objc2::{MainThreadMarker, rc::Retained, runtime::ProtocolObject};
use objc2_app_kit::{
    NSWorkspace, NSWorkspaceDidWakeNotification, NSWorkspaceWillSleepNotification,
};
use objc2_foundation::{NSNotification, NSNotificationCenter, NSObjectProtocol};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum LifecycleError {
    #[error("workspace lifecycle observation must be created on the main thread")]
    MainThreadRequired,
}

/// The workspace transition delivered by the native notification center.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleEvent {
    WillSleep,
    DidWake,
}

/// Observes workspace sleep and wake transitions. The action
/// should revoke playback and set a flag; it must not block, perform media or
/// device I/O, or panic. A callback already in flight may finish during removal,
/// so its state is owned and Send + Sync. This guard is !Send and !Sync, ensuring
/// observer removal and workspace ownership stay on the main thread.
pub struct LifecycleObserver {
    _main_thread: MainThreadMarker,
    _observers: ObserverTokens,
}

impl LifecycleObserver {
    /// Calls the same interruption action for both sleep and wake, preserving
    /// the playback observer's original contract.
    pub fn new(callback: Arc<dyn Fn() + Send + Sync>) -> Result<Self, LifecycleError> {
        Self::with_events(Arc::new(move |_| callback()))
    }

    /// Reports the exact transition. The callback follows the nonblocking,
    /// owned-state contract documented on `LifecycleObserver`.
    pub fn with_events(
        callback: Arc<dyn Fn(LifecycleEvent) + Send + Sync>,
    ) -> Result<Self, LifecycleError> {
        let main_thread = MainThreadMarker::new().ok_or(LifecycleError::MainThreadRequired)?;
        let center = NSWorkspace::sharedWorkspace().notificationCenter();
        Ok(Self {
            _main_thread: main_thread,
            _observers: ObserverTokens::new(center, callback),
        })
    }
}

struct ObserverTokens {
    center: Retained<NSNotificationCenter>,
    tokens: [Retained<ProtocolObject<dyn NSObjectProtocol>>; 2],
}

impl ObserverTokens {
    fn new(
        center: Retained<NSNotificationCenter>,
        callback: Arc<dyn Fn(LifecycleEvent) + Send + Sync>,
    ) -> Self {
        // SAFETY: These immutable AppKit notification-name globals are present
        // throughout supported macOS versions and remain valid for the process.
        let names = unsafe {
            [
                (NSWorkspaceWillSleepNotification, LifecycleEvent::WillSleep),
                (NSWorkspaceDidWakeNotification, LifecycleEvent::DidWake),
            ]
        };
        let tokens = names.map(|(name, event)| {
            let callback = Arc::clone(&callback);
            let block = RcBlock::new(move |_notification: NonNull<NSNotification>| callback(event));
            // SAFETY: No object filter or operation queue is supplied. The
            // copied block owns only the immutable event and an
            // Arc<dyn Fn(LifecycleEvent) + Send + Sync + 'static>,
            // so synchronous delivery on any posting thread is valid. The
            // notification pointer is never dereferenced or retained. The
            // center copies/retains the block; we retain its observer token.
            unsafe {
                center.addObserverForName_object_queue_usingBlock(Some(name), None, None, &block)
            }
        });
        Self { center, tokens }
    }
}

impl Drop for ObserverTokens {
    fn drop(&mut self) {
        for token in &self.tokens {
            let observer: &ProtocolObject<dyn NSObjectProtocol> = token;
            // SAFETY: Each token is the retained object returned by this exact
            // center's block-registration method. Remove before releasing its
            // last retained reference. In-flight callbacks own their Arc state.
            unsafe { self.center.removeObserver(observer.as_ref()) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn construction_off_main_thread_fails_before_workspace_access() {
        std::thread::spawn(|| {
            assert!(matches!(
                LifecycleObserver::new(Arc::new(|| {})),
                Err(LifecycleError::MainThreadRequired)
            ));
            assert!(matches!(
                LifecycleObserver::with_events(Arc::new(|_| {})),
                Err(LifecycleError::MainThreadRequired)
            ));
        })
        .join()
        .unwrap();
    }

    #[test]
    fn private_notification_center_delivers_both_events_and_removes_owned_tokens() {
        // A private Foundation center exercises actual block registration and
        // removal without publishing sleep/wake to NSWorkspace or other apps.
        let center = NSNotificationCenter::new();
        // Base-three digits retain delivery order without locking, allocating,
        // or panicking in the callback: sleep=1 and wake=2.
        let events = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&events);
        let observer = ObserverTokens::new(
            center.clone(),
            Arc::new(move |event| {
                let digit = match event {
                    LifecycleEvent::WillSleep => 1,
                    LifecycleEvent::DidWake => 2,
                };
                let _ = observed.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                    value
                        .checked_mul(3)
                        .and_then(|value| value.checked_add(digit))
                });
            }),
        );
        // SAFETY: Immutable AppKit names, no object argument, and a private
        // center. This does not request or emulate an OS sleep transition.
        unsafe {
            center.postNotificationName_object(NSWorkspaceWillSleepNotification, None);
            center.postNotificationName_object(NSWorkspaceDidWakeNotification, None);
            center.postNotificationName_object(NSWorkspaceDidWakeNotification, None);
            center.postNotificationName_object(NSWorkspaceWillSleepNotification, None);
        }
        assert_eq!(events.load(Ordering::Relaxed), 52); // digits 1, 2, 2, 1
        drop(observer);
        // SAFETY: Same private-center/name contract; observers have been removed.
        unsafe {
            center.postNotificationName_object(NSWorkspaceWillSleepNotification, None);
            center.postNotificationName_object(NSWorkspaceDidWakeNotification, None);
        }
        assert_eq!(events.load(Ordering::Relaxed), 52);
        assert_eq!(Arc::strong_count(&events), 1);
    }
}
