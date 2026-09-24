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

/// Calls one interruption action on both WillSleep and DidWake. The action
/// should revoke playback and set a flag; it must not block, perform media or
/// device I/O, or panic. A callback already in flight may finish during removal,
/// so its state is owned and Send + Sync. This guard is !Send and !Sync, ensuring
/// observer removal and workspace ownership stay on the main thread.
pub struct LifecycleObserver {
    _main_thread: MainThreadMarker,
    _observers: ObserverTokens,
}

impl LifecycleObserver {
    pub fn new(callback: Arc<dyn Fn() + Send + Sync>) -> Result<Self, LifecycleError> {
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
    fn new(center: Retained<NSNotificationCenter>, callback: Arc<dyn Fn() + Send + Sync>) -> Self {
        // SAFETY: These immutable AppKit notification-name globals are present
        // throughout supported macOS versions and remain valid for the process.
        let names = unsafe {
            [
                NSWorkspaceWillSleepNotification,
                NSWorkspaceDidWakeNotification,
            ]
        };
        let tokens = names.map(|name| {
            let callback = Arc::clone(&callback);
            let block = RcBlock::new(move |_notification: NonNull<NSNotification>| callback());
            // SAFETY: No object filter or operation queue is supplied. The
            // copied block owns only an Arc<dyn Fn() + Send + Sync + 'static>,
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
        })
        .join()
        .unwrap();
    }

    #[test]
    fn private_notification_center_delivers_both_events_and_removes_owned_tokens() {
        // A private Foundation center exercises actual block registration and
        // removal without publishing sleep/wake to NSWorkspace or other apps.
        let center = NSNotificationCenter::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&calls);
        let observer = ObserverTokens::new(
            center.clone(),
            Arc::new(move || {
                counter.fetch_add(1, Ordering::Relaxed);
            }),
        );
        // SAFETY: Immutable AppKit names, no object argument, and a private
        // center. This does not request or emulate an OS sleep transition.
        unsafe {
            center.postNotificationName_object(NSWorkspaceWillSleepNotification, None);
            center.postNotificationName_object(NSWorkspaceDidWakeNotification, None);
        }
        assert_eq!(calls.load(Ordering::Relaxed), 2);
        drop(observer);
        // SAFETY: Same private-center/name contract; observers have been removed.
        unsafe {
            center.postNotificationName_object(NSWorkspaceWillSleepNotification, None);
            center.postNotificationName_object(NSWorkspaceDidWakeNotification, None);
        }
        assert_eq!(calls.load(Ordering::Relaxed), 2);
        assert_eq!(Arc::strong_count(&calls), 1);
    }
}
