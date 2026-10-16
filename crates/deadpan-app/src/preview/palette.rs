//! The Keys sheet's search as an action palette. Results have one
//! highlighted row; Enter or a click chooses an action, and the next frame
//! runs it through the same command and key dispatch that typing `:verb` or
//! pressing its key uses, before any later input. Actions that cannot run here
//! keep Help open and explain where and how they work.

use super::*;
use crate::navigation::PaletteBinding;
use crate::navigation::palette::{self, Outcome};

impl DeadpanApp {
    /// Run the action Enter or a click chose in the Keys sheet. Runs on the
    /// first pass of the frame after the choice, ahead of the input that
    /// followed the key, so that suffix reaches whatever the action opened.
    pub(super) fn run_palette_choice(&mut self, context: &egui::Context) {
        if !self.help_open || context.current_pass_index() != 0 {
            return;
        }
        let Some(id) = self.help_scroll.take_run() else {
            return;
        };
        let Some(spec) = registry::by_id(id) else {
            return;
        };
        self.bindings.set_routing_domain(self.routed_domain());
        let selection = self.routed_edit_selection();
        let probe = self.bindings.clone();
        let outcome = palette::resolve(
            spec,
            self.help_context(),
            |binding| probe.palette_binding(binding, selection).is_some(),
            &probe,
        );
        if let Outcome::Explain(message) = outcome {
            self.help_scroll.palette.explain(message);
            context.request_repaint();
            return;
        }
        // Close Help as Escape does and return the keyboard to the pane.
        self.help_open = false;
        self.ime_composing = false;
        self.bindings.clear();
        context.memory_mut(|memory| memory.request_focus(pane_id(self.pane)));
        match outcome {
            Outcome::Run(verb) => self.palette_command(verb, false, context),
            Outcome::Prefill(verb) => self.palette_command(verb, true, context),
            Outcome::Key(binding) => self.palette_binding(binding, selection, context),
            Outcome::Explain(_) => unreachable!("handled above"),
        }
        context.request_repaint();
    }

    /// Open the command line exactly as `:` does, capturing its targets at
    /// entry, then type the verb: run it, or leave it for its arguments.
    fn palette_command(&mut self, verb: &str, arguments: bool, context: &egui::Context) {
        self.action(Action::Command, context);
        if !self.command_open {
            // The editor refused command entry and said why.
            return;
        }
        self.command = if arguments {
            format!("{verb} ")
        } else {
            verb.to_owned()
        };
        if arguments {
            let mut state =
                egui::text_edit::TextEditState::load(context, egui::Id::new(COMMAND_ID))
                    .unwrap_or_default();
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::one(
                    egui::text::CCursor::new(self.command.chars().count()),
                )));
            state.store(context, egui::Id::new(COMMAND_ID));
        } else {
            self.run_command(context);
        }
    }

    /// Perform a configured binding as its key press would: the same captures
    /// before dispatch, then the action, or the prefix family's pending path.
    fn palette_binding(
        &mut self,
        binding: navigation::BindingId,
        selection: navigation::EditSelection,
        context: &egui::Context,
    ) {
        match self.bindings.palette_binding(binding, selection) {
            Some(PaletteBinding::Action(action)) => {
                if matches!(action, Action::Operator { .. }) {
                    self.begin_operator();
                }
                if matches!(action, Action::Repeat { .. }) {
                    self.begin_repeat();
                }
                if action == Action::Trim {
                    self.trim_prefix_target = Some(self.capture_trim_target());
                }
                if matches!(action, Action::Ai(_)) {
                    self.ai.prefix = Some(self.ai_capture());
                }
                self.action(action, context);
                self.trim_prefix_target = None;
                self.ai.prefix = None;
                self.operator_target = None;
                self.repeat_prefix_target = None;
                if self.trim.is_some() {
                    context.memory_mut(|memory| memory.request_focus(pane_id(self.pane)));
                }
            }
            Some(PaletteBinding::Prefix) => {
                if !self.bindings.enter_prefix(binding, selection) {
                    return;
                }
                if self.bindings.operator_pending() {
                    self.begin_operator();
                }
                if self.bindings.repeat_pending() {
                    self.begin_repeat();
                }
                if self.bindings.macro_pending() {
                    self.macro_prefix_target = Some(self.capture_macro_target());
                }
                if self.bindings.mark_prefix().is_some() {
                    self.begin_mark_prefix();
                }
            }
            None => {
                self.error = Some(format!(
                    "{} no longer acts here; no action was taken.",
                    binding.as_str()
                ));
            }
        }
    }

    /// Hand input that followed a palette key to the next outer frame.
    pub(super) fn defer_help_input(&mut self, context: &egui::Context) {
        let deferred = self.help_scroll.take_deferred();
        if !deferred.is_empty() {
            self.deferred_text_input.extend(deferred);
            context.request_repaint();
        }
    }
}

/// The search results, best first, with the palette's highlighted row.
pub(super) fn search_results(
    ui: &mut egui::Ui,
    bindings: &Bindings,
    query: &str,
    here: registry::Contexts,
    state: &mut help_scroll::Palette,
) {
    let matches = palette::matches(bindings, query);
    let ids: Vec<_> = matches.iter().map(|found| found.spec.id).collect();
    state.settle(&ids);
    let query = query.trim();
    let highlighted = state.highlighted();
    // One live region with a fixed id: each changed count or highlight is
    // announced. A held key moves silently.
    let (shown, spoken) = if matches.is_empty() {
        let text = format!(
            "No action matches “{query}”. Try a key such as dd, a command such as :hold, or a word such as pause."
        );
        (egui::RichText::new(text.clone()), text)
    } else {
        let title = format!(
            "{} {} FOR “{query}”",
            matches.len(),
            if matches.len() == 1 {
                "MATCH"
            } else {
                "MATCHES"
            }
        );
        let spoken = format!(
            "{title}. Highlighted {} of {}: {}. Enter runs it.",
            highlighted + 1,
            matches.len(),
            matches[highlighted].spec.name
        );
        (style::section_title(&title, true), spoken)
    };
    let held = state.held();
    let count = ui.push_id("keys-search-count", |ui| ui.label(shown)).inner;
    let count = accessibility::full_text(count, &spoken);
    if !held {
        accessibility::live(&count, false);
    }
    let reveal = state.take_reveal();
    for (index, found) in matches.iter().enumerate() {
        let selected = index == highlighted;
        let fill = if selected {
            ui.visuals().selection.bg_fill.gamma_multiply(0.45)
        } else {
            egui::Color32::TRANSPARENT
        };
        let row = egui::Frame::new()
            .fill(fill)
            .corner_radius(4.0)
            .inner_margin(egui::Margin::symmetric(4, 2))
            .show(ui, |ui| {
                ui.push_id(("keys-result", found.spec.id), |ui| {
                    // A press chooses the row; it does not start a text selection.
                    ui.style_mut().interaction.selectable_labels = false;
                    help_spec_row_with(
                        ui,
                        &found.keys,
                        &format!(
                            "{} ({})",
                            found.help,
                            found.spec.section.title().to_lowercase()
                        ),
                        found.spec,
                        here,
                    )
                })
                .inner
            });
        // The whole row is one choice; its label keeps the spoken text.
        let response = row.inner.interact(egui::Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::SelectableLabel,
                true,
                selected,
                format!("{}. {}", found.spec.name, found.help),
            )
        });
        if response.clicked() {
            state.choose(index, found.spec.id);
        }
        if selected && reveal {
            row.response.scroll_to_me(None);
        }
    }
}
