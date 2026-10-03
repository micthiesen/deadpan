//! An operator captures its context when its first key is pressed. Completing
//! the path resolves typed intent through the same planner as a saved macro.

use deadpan_core::{RegisterName, SemanticInstruction, SemanticSelector};

use super::*;

pub(super) struct Capture {
    target: Result<macros::Capture, String>,
    register: Option<char>,
    register_override: Option<Option<char>>,
}

impl DeadpanApp {
    pub(super) fn begin_operator(&mut self) {
        self.operator_target = Some(Capture {
            target: self.capture_macro_target(),
            register: self.copied.selected(),
            register_override: self.copied.selected_override(),
        });
    }

    pub(super) fn reconcile_operator(&mut self) {
        let stale = self.operator_target.as_ref().is_some_and(|capture| {
            capture
                .target
                .as_ref()
                .is_ok_and(|target| !target.matches(self))
                || capture.register_override != self.copied.selected_override()
        });
        if stale && let Some(capture) = &mut self.operator_target {
            capture.target = Err("The pending operator's editing context changed. Start it again; no content was copied or cut.".into());
        }
    }

    pub(super) fn operator_action(&mut self, cut: bool, selector: SemanticSelector) {
        self.reconcile_operator();
        let captured = self.operator_target.take();
        // A failed attempt still consumes its one-shot name, while preserving
        // every durable register and any already queued write.
        self.copied.begin_write();
        let Some(captured) = captured else {
            self.error = Some("Start the operator again to capture its context.".into());
            return;
        };
        let instruction = captured
            .register
            .map_or(Ok(RegisterName::unnamed()), RegisterName::new)
            .map_err(|error| error.to_string())
            .map(|register| {
                if cut {
                    SemanticInstruction::Cut { selector, register }
                } else {
                    SemanticInstruction::Yank { selector, register }
                }
            });
        self.apply_recorded_instruction(captured.target, instruction);
    }
}
