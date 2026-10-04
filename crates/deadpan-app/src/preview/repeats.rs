use super::*;
use deadpan_core::{ProjectDocument, SemanticContext, SemanticInstruction, SemanticSelector};
use std::num::NonZeroU32;

/// Only an explicit whole-beat terminal may follow our own checked wrap onto
/// its new wrapper. Motion terminals retain the original captured context.
pub(super) struct Capture {
    original: Result<macros::Capture, String>,
    beat: Result<macros::Capture, String>,
}

impl DeadpanApp {
    pub(super) fn begin_repeat(&mut self) {
        let target = self.capture_macro_target();
        self.repeat_prefix_target = Some(Capture {
            original: target.clone(),
            beat: target,
        });
    }

    pub(super) fn reconcile_repeat_prefix(&mut self) {
        let Some(capture) = self.repeat_prefix_target.as_ref() else {
            return;
        };
        let original_stale = capture
            .original
            .as_ref()
            .is_ok_and(|target| !target.matches(self));
        let beat_stale = capture
            .beat
            .as_ref()
            .is_ok_and(|target| !target.matches(self));
        let capture = self.repeat_prefix_target.as_mut().expect("capture exists");
        let error = || {
            "The captured Repeat context changed. Enter the binding again; no edit was made."
                .to_owned()
        };
        if original_stale {
            capture.original = Err(error());
        }
        if beat_stale {
            capture.beat = Err(error());
        }
    }

    pub(super) fn repeat_prefix_can_continue(&self) -> bool {
        matches!(
            self.bindings.repeat_pending_scope(),
            Some(
                navigation::RepeatPendingScope::SelectedBeat
                    | navigation::RepeatPendingScope::Mixed
            )
        ) && self.repeat_prefix_target.as_ref().is_some_and(|capture| {
            capture
                .beat
                .as_ref()
                .is_ok_and(|target| target.matches(self))
        }) && self
            .repeat_queue
            .context_matches(self.repeat_target().as_ref())
    }

    pub(super) fn advance_repeat_prefix(&mut self) {
        let next = self.capture_macro_target();
        if let Some(capture) = &mut self.repeat_prefix_target {
            capture.beat = next;
        }
    }

    /// `,e`: three plays, each 3 dB louder and 0.08 closer than the last,
    /// over the Visual range or the selected beat, as one Undo.
    pub(super) fn escalating_repeat(&mut self) {
        self.bindings.clear();
        self.cancel_repeats("an escalating Repeat was requested");
        let selector = if self
            .capture_visual_selection()
            .is_ok_and(|visual| visual.is_some())
        {
            SemanticSelector::VisualSelection
        } else {
            SemanticSelector::SelectedBeat
        };
        let escalation = deadpan_core::RepeatEscalation {
            gain_step: deadpan_core::GainDb::new(3_000).expect("constant gain"),
            zoom: Some(deadpan_core::ZoomStep {
                step: deadpan_core::quantize_zoom_step(
                    deadpan_core::ExactRatio::new(8, 100).expect("constant ratio"),
                )
                .expect("constant step"),
                progression: deadpan_core::ZoomProgression::Add,
            }),
        };
        let target = self.capture_macro_target();
        self.apply_recorded_instruction(
            target,
            Ok(SemanticInstruction::Repeat {
                selector,
                plays: NonZeroU32::new(3).expect("constant plays"),
                escalation: Some(escalation),
            }),
        );
    }

    /// `:gag NAME`: one recipe instruction, applied and recorded as one Undo.
    pub(super) fn apply_gag(&mut self, input: crate::navigation::gag::GagInput) {
        self.bindings.clear();
        self.cancel_repeats("a gag was requested");
        let Some(rate) = self
            .workspace
            .as_ref()
            .map(|workspace| workspace.document.presentation_basis().frame_rate)
        else {
            self.error = Some("Open a project first.".into());
            return;
        };
        let target = self.capture_macro_target();
        self.apply_recorded_instruction(
            target,
            input
                .recipe(rate)
                .map(|recipe| SemanticInstruction::Gag { recipe }),
        );
    }

    pub(super) fn repeat_action(&mut self, selector: SemanticSelector, plays: NonZeroU32) {
        self.reconcile_repeat_prefix();
        let target = self.repeat_prefix_target.take().map_or_else(
            || Err("Enter the Repeat binding again to capture its target.".into()),
            |capture| {
                if selector == SemanticSelector::SelectedBeat {
                    capture.beat
                } else {
                    capture.original
                }
            },
        );
        self.repeat_captured(target, selector, plays);
    }

    fn repeat_captured(
        &mut self,
        target: Result<macros::Capture, String>,
        selector: SemanticSelector,
        plays: NonZeroU32,
    ) {
        self.bindings.clear();
        if selector == SemanticSelector::SelectedBeat && !self.macros.recording() {
            let target = target.and_then(|capture| {
                if !capture.matches(self) {
                    return Err("The captured Repeat context changed. Start the command again; no edit was made.".into());
                }
                capture.repeat_target()
            });
            match target {
                Ok(target) => self.wrap_repeat(target, plays.get()),
                Err(error) => self.error = Some(error),
            }
        } else {
            self.cancel_repeats("a Repeat selection was requested");
            self.apply_recorded_instruction(
                target,
                Ok(SemanticInstruction::Repeat {
                    selector,
                    plays,
                    escalation: None,
                }),
            );
        }
    }

    pub(super) fn repeat_command(
        &mut self,
        target: Option<Result<macros::Capture, String>>,
        plays: u32,
        set: bool,
    ) {
        let target = target
            .unwrap_or_else(|| Err("Open the Repeat command again to capture its target.".into()));
        let Some(plays) = NonZeroU32::new(plays) else {
            self.error = Some("Repeat needs at least one total play.".into());
            return;
        };
        if !set {
            let selector = target.as_ref().map_or(
                SemanticSelector::SelectedBeat,
                macros::Capture::repeat_selector,
            );
            self.repeat_captured(target, selector, plays);
            return;
        }
        let target = target.and_then(|capture| {
            if !capture.matches(self) {
                return Err("The captured Repeat context changed. Start the command again; no edit was made.".into());
            }
            let instruction = capture.repeat_count_instruction(plays)?;
            Ok((capture, instruction))
        });
        match target {
            Ok((capture, instruction)) => {
                self.cancel_repeats("the Repeat count was changed");
                self.apply_recorded_instruction(Ok(capture), Ok(instruction));
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub(super) fn repeat_target(&self) -> Option<repeat_queue::Target> {
        if self.view != View::Sequence
            || self.sound_focused()
            || self.event_focused()
            || matches!(self.pane, Pane::Sources | Pane::Sounds)
            || self.edit_selection() != navigation::EditSelection::None
        {
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

    fn wrap_repeat(&mut self, target: repeat_queue::Target, plays: u32) {
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

/// Resolve command intent once from an already captured ordinary Sequence.
/// Explicit absence and Visual state cannot become a cursor-selected beat.
pub(super) fn repeat_count_instruction(
    document: &ProjectDocument,
    context: &SemanticContext,
    plays: NonZeroU32,
) -> Result<SemanticInstruction, String> {
    if context.visual_selection.is_some() {
        return Err("Clear the Visual range before changing an existing Repeat count.".into());
    }
    let selected = context
        .selected_child
        .as_ref()
        .ok_or("Select a beat before repeating it.")?;
    let Some(NodeKind::Sequence { children }) =
        document.nodes().get(&context.parent).map(|node| &node.kind)
    else {
        return Err("Repeat commands need an ordinary Sequence scope.".into());
    };
    if !children.contains(selected) {
        return Err("The captured Repeat target is not a direct child of this group.".into());
    }
    match document.nodes().get(selected).map(|node| &node.kind) {
        Some(NodeKind::Repeat { .. }) => Ok(SemanticInstruction::SetRepeatPlays { plays }),
        Some(_) => Ok(SemanticInstruction::Repeat {
            selector: SemanticSelector::SelectedBeat,
            plays,
            escalation: None,
        }),
        None => Err("The captured Repeat target is no longer available.".into()),
    }
}

#[cfg(test)]
mod tests;
