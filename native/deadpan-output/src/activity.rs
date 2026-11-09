//! Process activity held while audio plays.
//!
//! macOS coalesces timers of a process it considers idle (App Nap, or a
//! process launched outside an active app). Measured 2026-11-09 on this Mac: a
//! 2 ms sleep took 15-18 ms from an agent shell and about 120 ms under the
//! background policy, so the playback controller's 2 ms delivery checks became
//! 18 ms steps of the heard clock (1080p60 pictures skipped 80 of 1,800) and
//! queue refills risked running past the prefill. A user-interactive activity
//! keeps those wake-ups on time while playback is active and ends with it.
//!
//! The unsafe call below is the typed Objective-C activity boundary.
#![allow(unsafe_code)]

use objc2::rc::Retained;
use objc2::runtime::{NSObjectProtocol, ProtocolObject};
use objc2_foundation::{NSActivityOptions, NSProcessInfo, NSString};

/// Ends the activity when dropped. Not `Send`: begin and end on one thread.
pub struct PlaybackActivity {
    token: Retained<ProtocolObject<dyn NSObjectProtocol>>,
}

impl PlaybackActivity {
    /// Latency-critical and user-initiated, which also defers idle system
    /// sleep while sound plays, as media players do; display sleep is allowed.
    pub fn begin() -> Self {
        let info = NSProcessInfo::processInfo();
        let token = info.beginActivityWithOptions_reason(
            NSActivityOptions::UserInteractive,
            &NSString::from_str("Deadpan audio playback"),
        );
        Self { token }
    }
}

impl Drop for PlaybackActivity {
    fn drop(&mut self) {
        // SAFETY: the token is exactly the object `beginActivityWithOptions`
        // returned for this activity, and it is ended once.
        unsafe { NSProcessInfo::processInfo().endActivity(&self.token) };
    }
}
