//! Observations of preparation work, never an audio clock or an admission token.
//! The worker owns nested scopes; readers copy one small, coherent snapshot.

use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreparationActivity {
    CheckingPlan,
    LoadingSource,
    DecodingSource,
    PreparingOutput,
    CheckingSavedAudio {
        completed: u32,
        total: u32,
    },
    RestoringSavedAudio {
        completed: u32,
        total: u32,
    },
    SavingAudioCache,
    CheckingStretch {
        depth: usize,
        completed: u32,
        total: u32,
    },
    Stretching {
        depth: usize,
        completed: u32,
        total: u32,
    },
    CheckingStretchInput {
        depth: usize,
        completed: u32,
        total: u32,
    },
    SavingStretch {
        depth: usize,
    },
}

/// One request's preparation state. Attach it only to that request's worker;
/// a replacement request gets a new monitor, so late work cannot retarget it.
/// No callback, media handle, authored state or completed PCM is retained here.
pub struct PreparationMonitor {
    activity: Mutex<PreparationActivity>,
    saved_audio: Mutex<Option<SavedAudioStatus>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SavedAudioEvent {
    Missing,
    Restored,
    Saved,
    Unavailable,
    Damaged,
    Busy,
    Full,
    BudgetSkipped,
}

impl SavedAudioEvent {
    pub fn label(self) -> &'static str {
        match self {
            Self::Missing => "not saved yet",
            Self::Restored => "verified and restored",
            Self::Saved => "saved",
            Self::Unavailable => "cache unavailable",
            Self::Damaged => "damaged cache",
            Self::Busy => "cache in use",
            Self::Full => "cache full",
            Self::BudgetSkipped => "cache lookup work limit; prepared directly",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SavedAudioStatus {
    pub key: Option<[u8; 32]>,
    pub bytes: u64,
    pub event: SavedAudioEvent,
    pub regenerated_from: Option<SavedAudioEvent>,
}

impl PreparationMonitor {
    pub fn new(activity: PreparationActivity) -> Self {
        Self {
            activity: Mutex::new(activity),
            saved_audio: Mutex::new(None),
        }
    }

    pub fn activity(&self) -> PreparationActivity {
        *self
            .activity
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    pub fn saved_audio_status(&self) -> Option<SavedAudioStatus> {
        *self
            .saved_audio
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    pub fn report_saved_audio(&self, mut status: SavedAudioStatus) {
        let mut current = self
            .saved_audio
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if status.event == SavedAudioEvent::Saved {
            status.regenerated_from = current
                .filter(|old| old.key == status.key)
                .map(|old| old.event);
        }
        *current = Some(status);
    }

    pub fn enter(self: &Arc<Self>, activity: PreparationActivity) -> PreparationScope {
        let mut current = self
            .activity
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let previous = std::mem::replace(&mut *current, activity);
        PreparationScope {
            monitor: Arc::clone(self),
            previous,
        }
    }
}

/// Nested source opens and child stretches restore their parent's activity on
/// both success and failure. A scope reports work, never that PCM is ready.
pub struct PreparationScope {
    monitor: Arc<PreparationMonitor>,
    previous: PreparationActivity,
}

impl PreparationScope {
    pub fn report(&self, activity: PreparationActivity) {
        *self
            .monitor
            .activity
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = activity;
    }
}

impl Drop for PreparationScope {
    fn drop(&mut self) {
        *self
            .monitor
            .activity
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = self.previous;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn child_scopes_restore_parent_work_on_failure() {
        let monitor = Arc::new(PreparationMonitor::new(
            PreparationActivity::PreparingOutput,
        ));
        let parent = PreparationActivity::Stretching {
            depth: 0,
            completed: 256,
            total: 4096,
        };
        let _scope = monitor.enter(parent);
        let failed = || -> Result<(), ()> {
            let child = monitor.enter(PreparationActivity::LoadingSource);
            child.report(PreparationActivity::DecodingSource);
            assert_eq!(monitor.activity(), PreparationActivity::DecodingSource);
            Err(())
        };
        assert!(failed().is_err());
        assert_eq!(monitor.activity(), parent);
    }
}
