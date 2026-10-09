//! One bounded native responsibility: ask before system termination discards
//! editor previews. The host publishes their names after each input frame.
//! The adapter never owns a project, worker or editor command.
//!
//! `macos` contains the unsafe boundary: a previously absent selector on the
//! checked, pinned winit delegate and a scoped alert-only keyboard monitor. It
//! never replaces the delegate, its ivars, or its existing termination callback.

#![cfg(any(target_os = "macos", test))]

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::QuitGuard;

#[derive(Default)]
struct PreviewState {
    names: Vec<&'static str>,
    generation: u64,
    invalid: bool,
    asking: bool,
}

impl PreviewState {
    fn update(&mut self, names: &[&'static str]) {
        if self.names == names {
            return;
        }
        // The app supplies a small fixed list, never arbitrary document text.
        // Refuse termination if a future caller violates that boundary.
        if names.len() > 32 || names.iter().any(|name| name.len() > 256) {
            self.invalid = true;
            return;
        }
        let Some(generation) = self.generation.checked_add(1) else {
            self.invalid = true;
            return;
        };
        self.names.clear();
        self.names.extend_from_slice(names);
        self.generation = generation;
        self.invalid = false;
    }

    fn begin(&mut self) -> Decision {
        if self.invalid || self.asking {
            return Decision::Cancel;
        }
        if self.names.is_empty() {
            return Decision::Quit;
        }
        self.asking = true;
        Decision::Ask {
            generation: self.generation,
            message: format!(
                "These previews have not been saved:\n\n{}\n\nYour saved edits will be kept. Background work will be cancelled before Deadpan closes.",
                self.names.join("\n")
            ),
        }
    }

    fn finish(&mut self, generation: u64, discard: bool) -> bool {
        let was_asking = std::mem::take(&mut self.asking);
        was_asking && !self.invalid && generation == self.generation && discard
    }
}

enum Decision {
    Cancel,
    Quit,
    Ask { generation: u64, message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_quit_and_changed_previews_cannot_reuse_a_discard_decision() {
        let mut state = PreviewState::default();
        assert!(matches!(state.begin(), Decision::Quit));
        state.update(&["Camera framing"]);
        let Decision::Ask {
            generation,
            message,
        } = state.begin()
        else {
            panic!()
        };
        assert!(message.contains("Camera framing"));
        assert!(matches!(state.begin(), Decision::Cancel));
        state.update(&["Gain"]);
        assert!(!state.finish(generation, true));
        let Decision::Ask { generation, .. } = state.begin() else {
            panic!()
        };
        assert!(!state.finish(generation, false));
        let Decision::Ask { generation, .. } = state.begin() else {
            panic!()
        };
        assert!(state.finish(generation, true));
        assert!(!state.finish(generation, true));
    }

    #[test]
    fn unchanged_frames_keep_the_capture_and_invalid_input_refuses_quit() {
        let mut state = PreviewState::default();
        state.update(&["Trim"]);
        let Decision::Ask { generation, .. } = state.begin() else {
            panic!()
        };
        state.update(&["Trim"]);
        assert!(state.finish(generation, true));
        state.update(&["x"; 33]);
        assert!(matches!(state.begin(), Decision::Cancel));
        state.update(&[]);
        assert!(matches!(state.begin(), Decision::Quit));
        state.generation = u64::MAX;
        state.update(&["Gain"]);
        assert!(matches!(state.begin(), Decision::Cancel));
    }
}
