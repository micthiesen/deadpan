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

    /// `:gag-set key=value …`: the gag selected when the command was entered,
    /// with the given parameters changed. Only those parameters are recorded,
    /// so `.` changes just them on another gag of the same recipe.
    pub(super) fn set_gag(
        &mut self,
        target: Option<Result<super::macros::Capture, String>>,
        parameters: &[String],
    ) {
        self.bindings.clear();
        self.cancel_repeats("a gag change was requested");
        let target = target
            .unwrap_or_else(|| Err("Open :gag-set again to capture the selected gag.".into()));
        let instruction = target.as_ref().map_err(Clone::clone).and_then(|captured| {
            let node = captured
                .selected_node()
                .ok_or("Select an inserted gag's group first.")?;
            let current = deadpan_core::GagRecipe::from_label(&node.label).ok_or(
                "The selected beat is not an inserted gag: its label does not pin a recipe. Edit its parts directly.",
            )?;
            let parameters: Vec<&str> = parameters.iter().map(String::as_str).collect();
            crate::navigation::gag::merge(&current, &parameters, captured.frame_rate()).map(
                |(recipe, parameters)| SemanticInstruction::SetGag { recipe, parameters },
            )
        });
        self.apply_recorded_instruction(target, instruction);
    }

    /// `:gag NAME` for a saved preset: its recipe, with any given parameters
    /// changed, applied at the context captured on command entry and
    /// recorded as the concrete recipe it names.
    pub(super) fn apply_gag_preset(
        &mut self,
        target: Option<Result<super::macros::Capture, String>>,
        name: &str,
        parameters: &[String],
    ) {
        self.bindings.clear();
        self.cancel_repeats("a gag was requested");
        let target = target.unwrap_or_else(|| Err("Open :gag again to capture the cursor.".into()));
        let instruction = target.as_ref().map_err(Clone::clone).and_then(|captured| {
            let preset = self.gag_presets.get(name)?;
            let recipe = if parameters.is_empty() {
                preset
            } else {
                let parameters: Vec<&str> = parameters.iter().map(String::as_str).collect();
                crate::navigation::gag::merge(&preset, &parameters, captured.frame_rate())?.0
            };
            Ok(SemanticInstruction::Gag { recipe })
        });
        self.apply_recorded_instruction(target, instruction);
    }

    /// `:gag-save NAME`: keep the inserted gag selected on command entry, with
    /// its recipe and exact parameters, as a preset every project can insert
    /// with `:gag NAME`.
    pub(super) fn save_gag_preset(
        &mut self,
        target: Option<Result<super::macros::Capture, String>>,
        name: &str,
    ) {
        let result = (|| {
            let captured = target.ok_or("Open :gag-save again to capture the selected gag.")??;
            if !captured.matches(self) {
                return Err(
                    "The selection changed while typing. Start :gag-save again; nothing was saved."
                        .to_owned(),
                );
            }
            let node = captured
                .selected_node()
                .ok_or("Select an inserted gag's group first.")?;
            let recipe = deadpan_core::GagRecipe::from_label(&node.label).ok_or(
                "The selected beat is not an inserted gag: its label does not pin a recipe. A changed group can be kept in this project with :recipe-save.",
            )?;
            let replaced = self.gag_presets.save(name, recipe)?;
            Ok::<_, String>(format!(
                "{} gag preset {name}: {}. :gag {name} inserts it in any project.",
                if replaced { "Replaced" } else { "Saved" },
                recipe.label()
            ))
        })();
        match result {
            Ok(message) => {
                self.error = None;
                self.message = Some(message);
            }
            Err(error) => self.error = Some(error),
        }
    }

    /// `:gag-presets`: the saved presets, one row each, in Help, naming any
    /// entry this Deadpan cannot read.
    pub(super) fn list_gag_presets(&mut self) {
        match self.gag_presets.load() {
            Ok(loaded) => {
                let mut rows: Vec<String> = loaded
                    .presets
                    .iter()
                    .map(|(name, recipe)| format!(":gag {name} · {}", recipe.label()))
                    .collect();
                if rows.is_empty() {
                    rows.push(
                        "No gag presets are saved yet. Select an inserted gag and :gag-save NAME."
                            .into(),
                    );
                }
                for name in &loaded.skipped {
                    rows.push(format!(
                        "{name}: this entry cannot be read by this Deadpan and was skipped; it stays in the file."
                    ));
                }
                let count = loaded.presets.len();
                self.message = Some(format!(
                    "{count} saved gag preset{}{}; see Help.",
                    if count == 1 { "" } else { "s" },
                    if loaded.skipped.is_empty() {
                        String::new()
                    } else {
                        format!(", {} unreadable entry skipped", loaded.skipped.len())
                    }
                ));
                self.help_expansion = Some(("Saved gag presets".into(), rows));
                self.help_registers_first = false;
                self.help_scroll = Default::default();
                self.help_open = true;
            }
            Err(error) => self.error = Some(error),
        }
    }

    /// `:gag-inspect`: show the recipe's exact expansion in Help, with the
    /// current Visual range deciding its content as `:gag` would. Nothing is
    /// applied and history is unchanged.
    pub(super) fn inspect_gag(&mut self, input: crate::navigation::gag::GagInput) {
        let Some(rate) = self
            .workspace
            .as_ref()
            .map(|workspace| workspace.document.presentation_basis().frame_rate)
        else {
            self.error = Some("Open a project first.".into());
            return;
        };
        let visual = self.edit_selection() != navigation::EditSelection::None;
        let unseeded = matches!(
            input,
            crate::navigation::gag::GagInput::OneMoreTime {
                variation: Some((_, None)),
                ..
            }
        );
        let result = input.recipe(rate).and_then(|recipe| {
            let mut rows = crate::navigation::gag::expansion_rows(&recipe, visual, rate)?;
            if let (true, deadpan_core::GagRecipe::OneMoreTime {
                variation: Some(variation),
                ..
            }) = (unseeded, &recipe)
            {
                rows.push(format!(
                    "No seed was given, so seed {} was drawn for this preview. Apply with seed={} to get exactly these gaps; without it a new seed is drawn.",
                    variation.seed, variation.seed
                ));
            }
            Ok((recipe.name().to_owned(), rows))
        });
        match result {
            Ok((name, rows)) => {
                self.message = Some(format!(
                    "{name} expands to {} ordinary steps; see Help. Nothing was applied.",
                    rows.len()
                ));
                self.help_expansion = Some((name, rows));
                self.help_registers_first = false;
                self.help_scroll = Default::default();
                self.help_open = true;
            }
            Err(error) => self.error = Some(error),
        }
    }

    /// `:recipe-save a`: the selected group as one whole-group copy in
    /// project register a, recorded like `"ayag`.
    pub(super) fn save_recipe(
        &mut self,
        name: char,
        target: Option<Result<macros::Capture, String>>,
    ) {
        let group = self
            .workspace
            .as_ref()
            .zip(self.selected_beat.as_ref())
            .is_some_and(|(workspace, node)| {
                matches!(
                    workspace.document.nodes().get(node).map(|node| &node.kind),
                    Some(deadpan_core::NodeKind::Sequence { .. })
                )
            });
        if self.view != View::Sequence || !group {
            self.error = Some(
                "Select a group in Your edit, such as a gag, then save it with :recipe-save a."
                    .into(),
            );
            return;
        }
        self.cancel_repeats("a recipe was saved");
        let instruction = deadpan_core::RegisterName::new(name)
            .map(|register| SemanticInstruction::Yank {
                selector: SemanticSelector::TextObject {
                    object: deadpan_core::SemanticTextObject::AroundGroup,
                },
                register,
            })
            .map_err(|error| error.message);
        self.copied.begin_write();
        self.apply_recorded_instruction(
            target.unwrap_or_else(|| Err("Open :recipe-save again to capture its group.".into())),
            instruction,
        );
    }

    /// `:recipe a`: a fresh copy of local recipe a at the cursor.
    pub(super) fn insert_recipe(
        &mut self,
        name: char,
        target: Option<Result<macros::Capture, String>>,
    ) {
        if !self
            .copied
            .entries()
            .any(|(slot, content)| slot == name && matches!(content, copied::Content::Edited(_)))
        {
            self.error = Some(format!(
                "Local recipe {name} is empty. Select a group and save it with :recipe-save {name}."
            ));
            return;
        }
        self.cancel_repeats("a recipe was inserted");
        let instruction = deadpan_core::RegisterName::new(name)
            .map(|register| SemanticInstruction::Paste {
                register,
                before: false,
            })
            .map_err(|error| error.message);
        self.apply_recorded_instruction(
            target.unwrap_or_else(|| Err("Open :recipe again to capture its destination.".into())),
            instruction,
        );
    }

    /// `:recipe-inspect a`: the saved group's outline in Help. Nothing is
    /// applied.
    pub(super) fn inspect_recipe(&mut self, name: char) {
        let rows = self
            .copied
            .entries()
            .find_map(|(slot, content)| match content {
                copied::Content::Edited(captured) if slot == name => {
                    Some(captured.slice().outline())
                }
                _ => None,
            });
        match rows {
            Some(rows) => {
                self.message = Some(format!(
                    "Local recipe {name} inserts {} parts; see Help. Nothing was applied.",
                    rows.iter().filter(|row| !row.starts_with(' ')).count()
                ));
                self.help_expansion = Some((format!("Local recipe {name}"), rows));
                self.help_registers_first = false;
                self.help_scroll = Default::default();
                self.help_open = true;
            }
            None => {
                self.error = Some(format!(
                    "Register {name} holds no saved group. Select a group and use :recipe-save {name}."
                ));
            }
        }
    }

    /// `:repeat [N] gap=… gain-step=… zoom-step=…`: change the selected
    /// Repeat's plays, gaps and escalation, or wrap a plain beat first, as
    /// one recorded instruction and one Undo.
    pub(super) fn repeat_change(&mut self, input: crate::navigation::escalation::EscalationInput) {
        self.cancel_repeats("a Repeat change was requested");
        let target = self.capture_macro_target();
        let instruction = (|| {
            let workspace = self.workspace.as_ref().ok_or("Open a project first.")?;
            let rate = workspace.document.presentation_basis().frame_rate;
            let selected = self
                .selected_beat
                .as_ref()
                .ok_or("Select a beat or Repeat in the current group first.")?;
            let (plays_now, escalation_now) = match workspace
                .document
                .nodes()
                .get(selected)
                .map(|node| &node.kind)
            {
                Some(deadpan_core::NodeKind::Repeat {
                    iterations,
                    escalation,
                    ..
                }) => (Some(iterations.len()), *escalation),
                _ => (None, None),
            };
            let plays = input.plays.or(plays_now).ok_or(
                "Give a total play count to wrap this beat, for example :repeat 3 gap=120ms.",
            )?;
            let gaps = input.gap.map(|gap| gap.lengths(plays, rate)).transpose()?;
            let escalation = if input.changes_escalation() {
                Some(
                    input
                        .apply(escalation_now)?
                        .unwrap_or(deadpan_core::RepeatEscalation {
                            gain_step: deadpan_core::GainDb::UNITY,
                            zoom: None,
                        }),
                )
            } else {
                None
            };
            Ok(SemanticInstruction::SetRepeat {
                plays: input.plays.and_then(NonZeroU32::new),
                gaps,
                escalation,
            })
        })();
        self.apply_recorded_instruction(target, instruction);
    }

    pub(super) fn repeat_action(&mut self, selector: SemanticSelector, plays: NonZeroU32) {
        if selector == SemanticSelector::VisualSelection
            && self.edit_role != deadpan_core::MediaRole::Linked
        {
            // Visual r under a chosen role repeats that role only.
            self.repeat_prefix_target = None;
            self.role_repeat(self.edit_role, plays, false, self.capture_macro_target());
            return;
        }
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
