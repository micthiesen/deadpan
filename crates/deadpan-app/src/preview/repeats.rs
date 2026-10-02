use super::*;

impl DeadpanApp {
    pub(super) fn repeat_target(&self) -> Option<repeat_queue::Target> {
        if self.view != View::Sequence || self.sound_focused() {
            return None;
        }
        let workspace = self.workspace.as_ref()?;
        Some(repeat_queue::Target {
            session: workspace.session,
            revision: workspace.document.revision_id().clone(),
            scope: self.sequence_scope.clone(),
            node: self.selected_beat.clone()?,
            cursor: ProjectFrame(i64::try_from(self.sequence_cursor).ok()?),
            pane: self.pane,
        })
    }

    pub(super) fn cancel_repeats(&mut self, reason: &str) {
        let cancelled = self.repeat_queue.cancel(reason);
        #[cfg(feature = "ui-harness")]
        for _ in 0..cancelled {
            self.feedback.record("repeat_cancelled");
        }
        #[cfg(not(feature = "ui-harness"))]
        let _ = cancelled;
    }

    pub(super) fn reconcile_repeats(&mut self, context: &egui::Context) {
        if !self.repeat_queue.active() {
            return;
        }
        let interrupted = self.close_pending
            || self.command_open
            || self.help_open
            || self.camera.is_some()
            || self.camera_pending.is_some()
            || self.dialogs.is_open()
            || self.transport.is_some()
            || egui::Popup::is_any_open(context)
            || context.any_popup_open()
            || !context.input(|input| input.focused);
        if interrupted
            || !self
                .repeat_queue
                .context_matches(self.repeat_target().as_ref())
        {
            self.cancel_repeats("editing context changed");
        }
    }

    pub(super) fn wrap_repeat(&mut self, plays: u32) {
        let Some(target) = self.repeat_target() else {
            self.error = Some("Select a beat in Your edit before wrapping a Repeat.".into());
            return;
        };
        if !self.repeat_queue.context_matches(Some(&target)) {
            self.cancel_repeats("editing context changed");
        }
        match self.repeat_queue.offer(&target, plays) {
            Ok(true) => {
                #[cfg(feature = "ui-harness")]
                self.feedback.record("repeat_queued");
            }
            Ok(false) => {
                if self.submit_now(target.request(plays)) {
                    self.repeat_queue.started(target, plays);
                }
            }
            Err(error) => {
                #[cfg(feature = "ui-harness")]
                self.feedback.record("command_rejected");
                self.error = Some(error);
            }
        }
    }

    pub(super) fn dispatch_waiting_repeat(&mut self, context: &egui::Context) {
        self.reconcile_repeats(context);
        let Some((target, plays)) = self.repeat_queue.next() else {
            return;
        };
        #[cfg(feature = "ui-harness")]
        self.feedback.record("repeat_dequeued");
        // This continuation is not new input. Keep an unfinished operator/count
        // typed while its preceding wrap committed; it resolves when completed.
        let bindings = self.bindings.clone();
        if self.submit_now(target.request(plays)) {
            self.repeat_queue.started(target, plays);
        } else {
            self.cancel_repeats("the next Repeat could not be submitted");
        }
        self.bindings = bindings;
    }
}
