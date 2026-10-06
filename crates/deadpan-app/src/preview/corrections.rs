//! Correcting the Original's transcript words and pauses.
//!
//! `:correct` (or Correct… in the TRANSCRIPT section) opens a modal sheet over
//! the words and pauses around the current word, in time order. Every change
//! is computed against the corrected analysis on screen and stored by the
//! project service as one correction with its own Undo step, apart from the
//! rebuildable recognizer and detector proposals and from edit history.
//! Edge moves are an unsaved draft until Enter, so a run of nudges is one
//! correction.

use deadpan_analysis::{
    CENTISECOND_SAMPLES, CorrectedPauses, CorrectionClock, Corrections, Pause,
    edges_in_centiseconds, measured_edges, next_edge,
};
use deadpan_store::CorrectionChange;

use super::*;
use navigation::corrections::CorrectionKey;

const TEXT_ID: &str = "correction-text";
const FOCUS_ID: &str = "correction-focus";
/// Items shown on each side of the selected one.
const CONTEXT_ITEMS: usize = 40;
/// One edge nudge: one energy frame.
const NUDGE: u64 = CENTISECOND_SAMPLES;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Item {
    Word(usize),
    Pause(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Edge {
    Start,
    End,
}

pub(super) struct Draft {
    session: u64,
    item: Option<Item>,
    /// After a save: select the word (true) or pause starting nearest this
    /// analysis sample.
    reselect: Option<(bool, u64)>,
    edge: Option<Edge>,
    /// Moved edges, unsaved, in analysis samples.
    bounds: Option<(u64, u64)>,
    text: Option<String>,
    text_focus_pending: bool,
    focus_pending: bool,
    /// The attempt the service is saving and its label.
    pending: Option<(u64, String)>,
    key: Option<CorrectionKey>,
    message: Option<String>,
    error: Option<String>,
}

impl Draft {
    fn unsaved(&self) -> bool {
        self.text.is_some() || self.bounds.is_some()
    }
}

/// Words and pauses in time order with their analysis-sample bounds, and the
/// corrected counts, built once per published transcript and activity.
pub(super) struct Stream {
    items: Vec<(Item, u64, u64)>,
    corrected_words: usize,
    corrected_pauses: usize,
}

/// The published analyses a stream was built from, held for identity.
pub(super) type StreamCache = (
    Option<Arc<crate::project::OriginalTranscript>>,
    Option<Arc<crate::project::OriginalActivity>>,
    Arc<Stream>,
);

fn build_stream(workspace: &Workspace) -> Stream {
    let mut items: Vec<(Item, u64, u64)> = Vec::new();
    let mut corrected_words = 0;
    if let Some(transcript) = &workspace.transcript {
        corrected_words = (0..transcript.transcript.words().len())
            .filter(|word| transcript.corrected(*word))
            .count();
        items.extend(
            transcript
                .transcript
                .words()
                .iter()
                .enumerate()
                .map(|(index, word)| {
                    (
                        Item::Word(index),
                        u64::from(word.start_cs) * CENTISECOND_SAMPLES,
                        u64::from(word.end_cs) * CENTISECOND_SAMPLES,
                    )
                }),
        );
    }
    let mut corrected_pauses = 0;
    if let Some(activity) = &workspace.speech_activity {
        corrected_pauses = activity.pauses.corrected.iter().filter(|c| **c).count();
        items.extend(
            activity
                .pauses
                .pauses
                .iter()
                .enumerate()
                .map(|(index, pause)| (Item::Pause(index), pause.start, pause.end)),
        );
    }
    items.sort_by_key(|(item, start, _)| (*start, matches!(item, Item::Pause(_))));
    Stream {
        items,
        corrected_words,
        corrected_pauses,
    }
}

fn same<T>(left: Option<&Arc<T>>, right: Option<&Arc<T>>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => Arc::ptr_eq(left, right),
        (None, None) => true,
        _ => false,
    }
}

impl DeadpanApp {
    fn correction_stream(&mut self, workspace: &Workspace) -> Arc<Stream> {
        if let Some((transcript, activity, stream)) = &self.transcription.correction_stream
            && same(transcript.as_ref(), workspace.transcript.as_ref())
            && same(activity.as_ref(), workspace.speech_activity.as_ref())
        {
            return Arc::clone(stream);
        }
        let stream = Arc::new(build_stream(workspace));
        self.transcription.correction_stream = Some((
            workspace.transcript.clone(),
            workspace.speech_activity.clone(),
            Arc::clone(&stream),
        ));
        stream
    }
}

/// Why the corrections on screen are not what operators use, if they are not.
fn correction_problem(workspace: &Workspace) -> Option<String> {
    workspace
        .corrections
        .as_ref()
        .and_then(|corrections| corrections.error.clone())
        .map(|error| format!("Stored corrections are unreadable: {error}"))
        .or_else(|| workspace.transcript.as_ref().and_then(|t| t.problem()))
        .or_else(|| workspace.speech_activity.as_ref().and_then(|a| a.problem()))
}

/// Seconds of an analysis sample on the Original's clock, for display.
fn seconds(clock: CorrectionClock, sample: u64) -> f64 {
    clock.origin as f64 / f64::from(clock.sample_rate.max(1))
        + sample as f64 / f64::from(deadpan_analysis::ANALYSIS_SAMPLE_RATE)
}

fn clock(workspace: &Workspace) -> Option<CorrectionClock> {
    workspace
        .transcript
        .as_ref()
        .map(|transcript| Corrections::clock_of(&transcript.transcript))
        .or_else(|| {
            workspace
                .speech_activity
                .as_ref()
                .map(|activity| Corrections::clock_of_activity(&activity.activity))
        })
}

impl DeadpanApp {
    /// `:correct`: open the sheet at the current word.
    pub(super) fn open_corrections(&mut self, context: &egui::Context) {
        let Some(workspace) = self.workspace.as_ref() else {
            self.error = Some("Open a project first.".into());
            return;
        };
        if workspace.corrections.is_none()
            || (workspace.transcript.is_none() && workspace.speech_activity.is_none())
        {
            self.error = Some(
                "There is nothing to correct yet: the Original needs a transcript or detected pauses."
                    .into(),
            );
            return;
        }
        let item = self.current_word().map(Item::Word).or_else(|| {
            build_stream(workspace)
                .items
                .first()
                .map(|(item, _, _)| *item)
        });
        self.correction = Some(Draft {
            session: workspace.session,
            item,
            reselect: None,
            edge: None,
            bounds: None,
            text: None,
            text_focus_pending: false,
            focus_pending: true,
            pending: None,
            key: None,
            message: None,
            error: workspace
                .corrections
                .as_ref()
                .and_then(|corrections| corrections.error.clone())
                .map(|error| format!("Stored corrections are unreadable: {error}")),
        });
        self.bindings.clear();
        context.request_repaint();
    }

    pub(super) fn close_corrections(&mut self, context: &egui::Context) {
        self.correction = None;
        context.memory_mut(|memory| memory.request_focus(pane_id(self.pane)));
    }

    /// A save reply: settle the pending attempt and follow the changed item.
    pub(super) fn receive_correction_save(&mut self, save: Option<crate::project::TranscriptSave>) {
        let Some(save) = save else {
            return;
        };
        let Some(draft) = self.correction.as_mut() else {
            return;
        };
        let Some((attempt, label)) = draft.pending.clone() else {
            return;
        };
        if draft.session != save.session || attempt != save.attempt {
            return;
        }
        draft.pending = None;
        match save.error {
            Some(error) => {
                draft.error = Some(error);
                draft.reselect = None;
            }
            None => {
                draft.error = None;
                draft.message = Some(format!("Saved: {label}."));
                draft.text = None;
                draft.bounds = None;
                draft.edge = None;
                self.after_correction();
                self.reselect_correction();
            }
        }
    }

    fn reselect_correction(&mut self) {
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        let workspace = Arc::clone(workspace);
        let stream = self.correction_stream(&workspace);
        let items = &stream.items;
        let Some(draft) = self.correction.as_mut() else {
            return;
        };
        let Some((word, at)) = draft.reselect.take() else {
            return;
        };
        draft.item = items
            .iter()
            .filter(|(item, _, _)| matches!(item, Item::Word(_)) == word)
            .min_by_key(|(_, start, _)| start.abs_diff(at))
            .or_else(|| items.iter().min_by_key(|(_, start, _)| start.abs_diff(at)))
            .map(|(item, _, _)| *item);
    }

    /// Modal routing precedes the editor's keys; the word field keeps native
    /// text editing and composition.
    pub(super) fn corrections_keyboard(&mut self, context: &egui::Context) {
        let events = context.input(|input| input.events.clone());
        help_scroll::observe_composition(&events, &mut self.ime_composing);
        self.bindings.clear();
        if pointer_focus_transition(&events) {
            return;
        }
        let ime = self.ime_composing
            || events
                .iter()
                .any(|event| matches!(event, egui::Event::Ime(_)));
        // An open text draft owns Enter and Escape even after focus moved.
        let field = context.memory(|memory| memory.has_focus(egui::Id::new(TEXT_ID)))
            || self
                .correction
                .as_ref()
                .is_some_and(|draft| draft.text.is_some());
        let editing_edge = self
            .correction
            .as_ref()
            .is_some_and(|draft| draft.edge.is_some());
        for event in events {
            let egui::Event::Key {
                key,
                modifiers,
                pressed: true,
                repeat,
                ..
            } = event
            else {
                continue;
            };
            if let Some(action) =
                navigation::corrections::route_key(key, modifiers, field, editing_edge, ime, repeat)
            {
                if let Some(draft) = &mut self.correction {
                    draft.key = Some(action);
                }
                context.input_mut(|input| {
                    input.consume_key(modifiers, key);
                });
                break;
            }
        }
    }

    pub(super) fn corrections_sheet(&mut self, context: &egui::Context) {
        let Some(workspace) = self.workspace.clone() else {
            self.correction = None;
            return;
        };
        if self
            .correction
            .as_ref()
            .is_some_and(|draft| draft.session != workspace.session)
        {
            self.correction = None;
            return;
        }
        let Some(key) = self.correction.as_mut().and_then(|draft| draft.key.take()) else {
            self.draw_corrections(context, &workspace);
            return;
        };
        self.correction_key(key, context, &workspace);
        if let Some(workspace) = self.workspace.clone()
            && self.correction.is_some()
        {
            self.draw_corrections(context, &workspace);
        }
    }

    fn correction_key(
        &mut self,
        key: CorrectionKey,
        context: &egui::Context,
        workspace: &Workspace,
    ) {
        let stream = self.correction_stream(workspace);
        let items = &stream.items;
        let Some(draft) = self.correction.as_mut() else {
            return;
        };
        draft.message = None;
        let position = draft
            .item
            .and_then(|item| items.iter().position(|(entry, _, _)| *entry == item));
        let selected = position.map(|position| items[position]);
        match key {
            CorrectionKey::Cancel => {
                if draft.text.is_some() {
                    draft.text = None;
                    draft.focus_pending = true;
                } else if draft.edge.is_some() || draft.bounds.is_some() {
                    draft.edge = None;
                    draft.bounds = None;
                    draft.message = Some("Edge change discarded.".into());
                } else {
                    self.close_corrections(context);
                }
            }
            CorrectionKey::Previous | CorrectionKey::Next => {
                let next = match (position, key == CorrectionKey::Next) {
                    (None, _) => 0,
                    (Some(position), true) => (position + 1).min(items.len().saturating_sub(1)),
                    (Some(position), false) => position.saturating_sub(1),
                };
                draft.item = items.get(next).map(|(item, _, _)| *item);
                draft.edge = None;
                draft.bounds = None;
            }
            CorrectionKey::StartEdge | CorrectionKey::EndEdge => {
                let Some((_, start, end)) = selected else {
                    draft.message = Some("Select a word or pause first.".into());
                    return;
                };
                draft.edge = Some(if key == CorrectionKey::StartEdge {
                    Edge::Start
                } else {
                    Edge::End
                });
                draft.bounds.get_or_insert((start, end));
            }
            CorrectionKey::Nudge { later } | CorrectionKey::Snap { later } => {
                let (Some(edge), Some((start, end))) = (draft.edge, draft.bounds) else {
                    return;
                };
                let limit = analysis_samples(workspace);
                let from = if edge == Edge::Start { start } else { end };
                let to = if matches!(key, CorrectionKey::Nudge { .. }) {
                    if later {
                        from.saturating_add(NUDGE).min(limit)
                    } else {
                        from.saturating_sub(NUDGE)
                    }
                } else {
                    match workspace
                        .speech_activity
                        .as_ref()
                        .map(|activity| measured_edges(&activity.activity, &activity.pauses.pauses))
                        .and_then(|edges| next_edge(&edges, from, later))
                    {
                        Some(edge) => edge.min(limit),
                        None => {
                            draft.message = Some(if workspace.speech_activity.is_none() {
                                "Measured edges need detected speech activity.".into()
                            } else {
                                format!(
                                    "No measured edge {} this one.",
                                    if later { "after" } else { "before" }
                                )
                            });
                            return;
                        }
                    }
                };
                draft.bounds = Some(match edge {
                    Edge::Start => (to.min(end), end),
                    Edge::End => (start, to.max(start)),
                });
            }
            CorrectionKey::EditText => match selected {
                Some((Item::Word(index), _, _)) => {
                    let text = workspace
                        .transcript
                        .as_ref()
                        .and_then(|transcript| transcript.transcript.words().get(index))
                        .map(|word| word.text.clone())
                        .unwrap_or_default();
                    draft.text = Some(text);
                    draft.text_focus_pending = true;
                    draft.edge = None;
                    draft.bounds = None;
                }
                Some((Item::Pause(_), _, _)) => {
                    draft.message =
                        Some("Pauses have no text: b or e chooses an edge, x removes it.".into());
                }
                None => draft.message = Some("Select a word first.".into()),
            },
            CorrectionKey::Apply => {
                if let Some(text) = draft.text.clone() {
                    if let Some((Item::Word(index), _, _)) = selected {
                        self.correct_word_text(workspace, index, &text);
                    }
                } else if let (Some((item, _, _)), Some((start, end))) = (selected, draft.bounds) {
                    self.correct_bounds(workspace, item, start, end);
                }
            }
            CorrectionKey::Join => match selected {
                Some((Item::Word(index), _, _)) => self.correct_join(workspace, index),
                _ => draft.message = Some("Select a word to join with the next one.".into()),
            },
            // Removing needs a selected item and no open draft, so a key
            // meant for a field or an edge never deletes.
            CorrectionKey::Remove if draft.unsaved() => {
                draft.message =
                    Some("Apply (Enter) or discard (Esc) the open change before removing.".into());
            }
            CorrectionKey::Remove => match selected {
                Some((Item::Word(index), _, _)) => self.correct_word_text(workspace, index, ""),
                Some((Item::Pause(index), _, _)) => self.correct_remove_pause(workspace, index),
                None => draft.message = Some("Select a word or pause to remove.".into()),
            },
            CorrectionKey::Discard => self.discard_corrections(workspace),
            CorrectionKey::AddPause => match selected {
                Some((Item::Word(index), _, _)) => self.correct_add_pause(workspace, index),
                _ => {
                    draft.message = Some("Select the word the pause follows, then press p.".into());
                }
            },
            CorrectionKey::Undo | CorrectionKey::Redo => {
                let undo = key == CorrectionKey::Undo;
                let stored = workspace
                    .corrections
                    .as_ref()
                    .and_then(|corrections| corrections.stored.as_ref());
                let label = stored.and_then(|stored| {
                    if undo {
                        stored.undo.clone()
                    } else {
                        stored.redo.clone()
                    }
                });
                let Some(label) = label else {
                    draft.message = Some(format!(
                        "There is no correction to {}.",
                        if undo { "undo" } else { "redo" }
                    ));
                    return;
                };
                let at = selected.map(|(item, start, _)| (matches!(item, Item::Word(_)), start));
                self.submit_correction(
                    workspace,
                    if undo {
                        CorrectionChange::Undo
                    } else {
                        CorrectionChange::Redo
                    },
                    format!("{} {label}", if undo { "undid" } else { "redid" }),
                    at,
                );
            }
        }
    }

    fn correct_word_text(&mut self, workspace: &Workspace, index: usize, text: &str) {
        let Some(transcript) = workspace.transcript.as_ref() else {
            return;
        };
        let Some(word) = transcript.transcript.words().get(index) else {
            return;
        };
        let edges = workspace
            .speech_activity
            .as_ref()
            .map(|activity| {
                edges_in_centiseconds(
                    &measured_edges(&activity.activity, &activity.pauses.pauses),
                    &transcript.transcript,
                    &activity.activity,
                )
            })
            .unwrap_or_default();
        let label = if text.trim().is_empty() {
            format!("remove “{}”", word.text)
        } else {
            format!(
                "“{}” → “{}”",
                word.text,
                text.split_whitespace().collect::<Vec<_>>().join(" ")
            )
        };
        let at = (true, u64::from(word.start_cs) * CENTISECOND_SAMPLES);
        let result = self.base_corrections(workspace).and_then(|base| {
            base.edit_word_text(&transcript.current(), index, text, &edges)
                .map_err(|error| error.to_string())
        });
        self.submit_corrected(workspace, result, label, Some(at));
    }

    fn correct_join(&mut self, workspace: &Workspace, index: usize) {
        let Some(transcript) = workspace.transcript.as_ref() else {
            return;
        };
        let words = transcript.transcript.words();
        let (Some(first), Some(second)) = (words.get(index), words.get(index + 1)) else {
            self.correction_message("There is no next word to join.");
            return;
        };
        let label = format!("join “{}” and “{}”", first.text, second.text);
        let at = (true, u64::from(first.start_cs) * CENTISECOND_SAMPLES);
        let result = self.base_corrections(workspace).and_then(|base| {
            base.merge_words(&transcript.current(), index)
                .map_err(|error| error.to_string())
        });
        self.submit_corrected(workspace, result, label, Some(at));
    }

    fn correct_bounds(&mut self, workspace: &Workspace, item: Item, start: u64, end: u64) {
        let clock = clock(workspace);
        let result = match item {
            Item::Word(index) => {
                let Some(transcript) = workspace.transcript.as_ref() else {
                    return;
                };
                let name = transcript
                    .transcript
                    .words()
                    .get(index)
                    .map(|word| word.text.clone())
                    .unwrap_or_default();
                let to_cs =
                    |sample: u64| u32::try_from(sample / CENTISECOND_SAMPLES).unwrap_or(u32::MAX);
                self.base_corrections(workspace)
                    .and_then(|base| {
                        base.set_word_bounds(&transcript.current(), index, to_cs(start), to_cs(end))
                            .map_err(|error| error.to_string())
                    })
                    .map(|(corrections, start, end)| {
                        (
                            corrections,
                            format!(
                                "move “{name}” to {}",
                                span(
                                    clock,
                                    u64::from(start) * CENTISECOND_SAMPLES,
                                    u64::from(end) * CENTISECOND_SAMPLES
                                )
                            ),
                            (true, u64::from(start) * CENTISECOND_SAMPLES),
                        )
                    })
            }
            Item::Pause(index) => {
                let Some(activity) = workspace.speech_activity.as_ref() else {
                    return;
                };
                self.base_corrections(workspace)
                    .and_then(|base| {
                        base.set_pause_bounds(
                            &activity.activity,
                            &activity.pauses,
                            index,
                            start,
                            end,
                        )
                        .map_err(|error| error.to_string())
                    })
                    .map(|(corrections, start, end)| {
                        (
                            corrections,
                            format!("move pause to {}", span(clock, start, end)),
                            (false, start),
                        )
                    })
            }
        };
        match result {
            Ok((corrections, label, at)) => {
                self.submit_corrected(workspace, Ok(corrections), label, Some(at));
            }
            Err(error) => self.submit_corrected(workspace, Err(error), String::new(), None),
        }
    }

    fn correct_remove_pause(&mut self, workspace: &Workspace, index: usize) {
        let Some(activity) = workspace.speech_activity.as_ref() else {
            return;
        };
        let Some(pause) = activity.pauses.pauses.get(index).copied() else {
            return;
        };
        let label = format!(
            "remove pause at {:.2} s",
            seconds_of(workspace, pause.start)
        );
        let result = self.base_corrections(workspace).and_then(|base| {
            base.replace_pauses(
                &activity.activity,
                &activity.pauses,
                index..index + 1,
                vec![],
            )
            .map_err(|error| error.to_string())
        });
        self.submit_corrected(workspace, result, label, Some((false, pause.start)));
    }

    /// A pause after a word: its gap to the next word, or 200 ms.
    fn correct_add_pause(&mut self, workspace: &Workspace, index: usize) {
        let (Some(transcript), Some(activity)) = (
            workspace.transcript.as_ref(),
            workspace.speech_activity.as_ref(),
        ) else {
            self.correction_message("Adding a pause needs detected speech activity.");
            return;
        };
        let words = transcript.transcript.words();
        let Some(word) = words.get(index) else {
            return;
        };
        let samples = activity.activity.audio().samples;
        let start = u64::from(word.end_cs) * CENTISECOND_SAMPLES;
        let gap_end = words
            .get(index + 1)
            .map(|next| u64::from(next.start_cs) * CENTISECOND_SAMPLES)
            .filter(|end| *end >= start + 5 * CENTISECOND_SAMPLES);
        let end = gap_end
            .unwrap_or(start + 20 * CENTISECOND_SAMPLES)
            .min(samples);
        if start >= end {
            self.correction_message("There is no audio after this word for a pause.");
            return;
        }
        let label = format!("add pause after “{}”", word.text);
        let result = self.base_corrections(workspace).and_then(|base| {
            base.add_pause(&activity.activity, &activity.pauses, Pause { start, end })
                .map_err(|error| error.to_string())
        });
        self.submit_corrected(workspace, result, label, Some((false, start)));
    }

    /// Shift+D: discard unreadable stored corrections, or drop the regions
    /// that no longer apply to the current analyses. Readable earlier states
    /// stay reachable with Undo; dropping is itself one undoable correction.
    fn discard_corrections(&mut self, workspace: &Workspace) {
        let Some(corrections) = workspace.corrections.as_ref() else {
            return;
        };
        if corrections.error.is_some() {
            let Some(clock) = clock(workspace) else {
                return;
            };
            self.submit_correction(
                workspace,
                CorrectionChange::DiscardUnreadable { clock },
                "discarded unreadable corrections".into(),
                None,
            );
            return;
        }
        let skipped = workspace.transcript.as_ref().map_or(0, |t| t.skipped)
            + workspace
                .speech_activity
                .as_ref()
                .map_or(0, |a| a.pauses.skipped);
        let Some(stored) = corrections.stored.as_ref().filter(|_| skipped > 0) else {
            self.correction_message("Every stored correction is readable and applies.");
            return;
        };
        let kept = stored.corrections.applicable_to(
            workspace.transcript.as_ref().map(|t| &*t.proposal),
            workspace.speech_activity.as_ref().map(|a| &a.activity),
        );
        let label = format!(
            "drop {skipped} correction{} that no longer appl{}",
            if skipped == 1 { "" } else { "s" },
            if skipped == 1 { "ies" } else { "y" }
        );
        self.submit_corrected(workspace, Ok(kept), label, None);
    }

    /// The stored corrections, or none yet on the analysis clock.
    fn base_corrections(&self, workspace: &Workspace) -> Result<Corrections, String> {
        let corrections = workspace
            .corrections
            .as_ref()
            .ok_or("The Original's corrections are not loaded.")?;
        if let Some(error) = &corrections.error {
            return Err(format!("Stored corrections are unreadable: {error}"));
        }
        match &corrections.stored {
            Some(stored) => Ok(stored.corrections.clone()),
            None => clock(workspace)
                .map(Corrections::empty)
                .ok_or_else(|| "There is nothing to correct yet.".into()),
        }
    }

    fn correction_message(&mut self, message: &str) {
        if let Some(draft) = &mut self.correction {
            draft.message = Some(message.into());
        }
    }

    fn submit_corrected(
        &mut self,
        workspace: &Workspace,
        result: Result<Corrections, String>,
        label: String,
        at: Option<(bool, u64)>,
    ) {
        match result {
            Ok(corrections) => self.submit_correction(
                workspace,
                CorrectionChange::Apply {
                    corrections,
                    label: label.clone(),
                },
                label,
                at,
            ),
            Err(error) => {
                if let Some(draft) = &mut self.correction {
                    draft.error = Some(capitalized(&error));
                }
            }
        }
    }

    fn submit_correction(
        &mut self,
        workspace: &Workspace,
        change: CorrectionChange,
        label: String,
        at: Option<(bool, u64)>,
    ) {
        let Some(corrections) = workspace.corrections.as_ref() else {
            return;
        };
        if self
            .correction
            .as_ref()
            .is_some_and(|draft| draft.pending.is_some())
        {
            self.correction_message("The previous correction is still saving.");
            return;
        }
        self.transcription.correction_attempts += 1;
        let attempt = self.transcription.correction_attempts;
        let request = ProjectRequest::ChangeCorrections(crate::project::CorrectionRequest {
            expected_session: workspace.session,
            attempt,
            key: corrections.key.clone(),
            expected_version: corrections.version(),
            change,
            transcript: workspace.transcript.clone(),
            activity: workspace.speech_activity.clone(),
        });
        if self.submit(request) {
            if let Some(draft) = &mut self.correction {
                draft.pending = Some((attempt, label));
                draft.reselect = at;
                draft.error = None;
            }
        } else if let Some(draft) = &mut self.correction {
            draft.error = self.error.take();
        }
    }

    fn draw_corrections(&mut self, context: &egui::Context, workspace: &Workspace) {
        let Some(mut draft) = self.correction.take() else {
            return;
        };
        let stream = self.correction_stream(workspace);
        let items = &stream.items;
        let problem = correction_problem(workspace);
        if draft
            .item
            .is_none_or(|item| !items.iter().any(|(entry, _, _)| *entry == item))
        {
            draft.item = items.first().map(|(item, _, _)| *item);
        }
        let clock = clock(workspace);
        let selected = draft
            .item
            .and_then(|item| items.iter().position(|(entry, _, _)| *entry == item));
        let mut clicked: Option<Item> = None;
        let mut button: Option<CorrectionKey> = None;
        let composing = self.ime_composing;
        let width = (context.content_rect().width() - 64.0).clamp(300.0, 620.0);
        let words = workspace.transcript.as_ref();
        let pauses = workspace
            .speech_activity
            .as_ref()
            .map(|activity| &activity.pauses);
        egui::Modal::new(egui::Id::new("corrections-sheet"))
            .frame(egui::Frame::popup(&context.style_of(egui::Theme::Dark)).inner_margin(24).corner_radius(10))
            .show(context, |ui| {
                super::accessibility::dialog(ui, "Correct transcript and pauses");
                ui.set_width(width);
                let heading = ui
                    .horizontal(|ui| {
                        ui.heading("Correct transcript and pauses");
                        if draft.unsaved() {
                            ui.colored_label(style::LAVENDER, "UNSAVED");
                        }
                    })
                    .response;
                let focus = ui.interact(heading.rect, egui::Id::new(FOCUS_ID), egui::Sense::focusable_noninteractive());
                if std::mem::take(&mut draft.focus_pending) {
                    focus.request_focus();
                }
                ui.weak("Corrections are kept apart from the recognized words and detected pauses, survive transcribing again, and are not edits: u and Shift+U undo and redo corrections only.");
                if let Some(problem) = &problem {
                    ui.colored_label(style::WARNING, format!("{problem}. Word and pause operators refuse until this is resolved."));
                    if ui.add(style::action("Discard what cannot be used", "Shift+D")).clicked() {
                        button = Some(CorrectionKey::Discard);
                    }
                }
                ui.separator();
                let first = selected.map_or(0, |at| at.saturating_sub(CONTEXT_ITEMS));
                let last = selected.map_or(items.len(), |at| (at + CONTEXT_ITEMS + 1).min(items.len()));
                if items.is_empty() {
                    ui.weak("No words or pauses to correct.");
                } else if let Some(item) = stream_view(ui, &items[first..last], draft.item, words, pauses) {
                    clicked = Some(item);
                }
                ui.add_space(6.0);
                // The selected item and any unsaved change.
                if let Some(at) = selected {
                    let (item, start, end) = items[at];
                    let detail = match item {
                        Item::Word(index) => words.and_then(|words| {
                            let word = words.transcript.words().get(index)?;
                            Some(format!(
                                "Word “{}” · {} · {}",
                                word.text,
                                span(clock, start, end),
                                if words.corrected(index) {
                                    let heard: Vec<&str> = words
                                        .proposal
                                        .words()
                                        .iter()
                                        .filter(|recognized| {
                                            recognized.start_cs < word.end_cs.max(word.start_cs + 1)
                                                && word.start_cs < recognized.end_cs.max(recognized.start_cs + 1)
                                        })
                                        .map(|recognized| recognized.text.as_str())
                                        .collect();
                                    if heard.is_empty() {
                                        "corrected".to_owned()
                                    } else {
                                        format!("corrected; recognized “{}”", heard.join(" "))
                                    }
                                } else {
                                    format!("recognized, {:.0}% sure", word.probability * 100.0)
                                }
                            ))
                        }),
                        Item::Pause(index) => pauses.map(|pauses| {
                            format!(
                                "Pause · {} · {}",
                                span(clock, start, end),
                                if pauses.corrected.get(index).copied().unwrap_or(false) {
                                    "corrected"
                                } else {
                                    "detected"
                                }
                            )
                        }),
                    };
                    if let Some(detail) = detail {
                        ui.label(egui::RichText::new(detail).strong());
                    }
                    if let (Some(edge), Some((new_start, new_end))) = (draft.edge, draft.bounds) {
                        let (from, to) = match edge {
                            Edge::Start => (start, new_start),
                            Edge::End => (end, new_end),
                        };
                        ui.colored_label(
                            style::LAVENDER,
                            format!(
                                "Moving the {} edge: {:.2} s → {:.2} s · h/l 10 ms · Shift+H/L measured edge · Enter applies · Esc discards",
                                if edge == Edge::Start { "start" } else { "end" },
                                at_seconds(clock, from),
                                at_seconds(clock, to),
                            ),
                        );
                    }
                }
                if let Some(text) = draft.text.as_mut() {
                    let label = ui.label("Word text (a space splits it; empty removes it)");
                    let field = ui
                        .add(
                            egui::TextEdit::singleline(text)
                                .id(egui::Id::new(TEXT_ID))
                                .event_filter(super::editor_input::field_filter(composing))
                                .return_key(None)
                                .desired_width(f32::INFINITY),
                        )
                        .labelled_by(label.id);
                    if std::mem::take(&mut draft.text_focus_pending) {
                        field.request_focus();
                    }
                    ui.weak("Enter applies · Esc discards");
                }
                if draft.pending.is_some() {
                    ui.weak("Saving correction…");
                }
                if let Some(error) = &draft.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
                if let Some(message) = &draft.message {
                    ui.weak(message);
                }
                ui.separator();
                let stored = workspace.corrections.as_ref().and_then(|corrections| corrections.stored.as_ref());
                let busy = draft.pending.is_some();
                ui.horizontal_wrapped(|ui| {
                    let word = matches!(draft.item, Some(Item::Word(_)));
                    for (label, key, enabled, action) in [
                        ("Previous / next", "h l", true, CorrectionKey::Next),
                        ("Edit text", "c", word && !busy, CorrectionKey::EditText),
                        ("Join with next", "Shift+J", word && !busy, CorrectionKey::Join),
                        ("Remove", "x", draft.item.is_some() && !busy, CorrectionKey::Remove),
                        ("Start edge", "b", draft.item.is_some(), CorrectionKey::StartEdge),
                        ("End edge", "e", draft.item.is_some(), CorrectionKey::EndEdge),
                        ("Pause after word", "p", word && pauses.is_some() && !busy, CorrectionKey::AddPause),
                    ] {
                        if ui.add_enabled(enabled, style::action(label, key)).clicked() {
                            button = Some(action);
                        }
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    let undo = stored.and_then(|stored| stored.undo.as_deref());
                    let redo = stored.and_then(|stored| stored.redo.as_deref());
                    if ui
                        .add_enabled(undo.is_some() && !busy, style::action(format!("Undo {}", undo.unwrap_or("correction")), "u"))
                        .clicked()
                    {
                        button = Some(CorrectionKey::Undo);
                    }
                    if ui
                        .add_enabled(redo.is_some() && !busy, style::action(format!("Redo {}", redo.unwrap_or("correction")), "Shift+U"))
                        .clicked()
                    {
                        button = Some(CorrectionKey::Redo);
                    }
                    if ui.add(style::action("Close", "Esc")).clicked() {
                        button = Some(CorrectionKey::Cancel);
                    }
                });
                let (corrected_words, corrected_pauses) = (stream.corrected_words, stream.corrected_pauses);
                ui.label(
                    egui::RichText::new(format!(
                        "{} and {} corrected · {} replaced · {}",
                        count(corrected_words, "word"),
                        count(corrected_pauses, "pause"),
                        count(words.map_or(0, |words| words.replaced), "recognized word"),
                        deadpan_analysis::CORRECTION_RULE
                    ))
                    .size(11.0)
                    .weak(),
                );
            });
        self.correction = Some(draft);
        if let Some(item) = clicked
            && let Some(draft) = &mut self.correction
        {
            draft.item = Some(item);
            draft.edge = None;
            draft.bounds = None;
            draft.text = None;
        }
        if let Some(action) = button
            && let Some(draft) = &mut self.correction
        {
            draft.key = Some(action);
            context.request_repaint();
        }
    }
}

impl DeadpanApp {
    #[cfg(feature = "ui-harness")]
    pub(super) fn correction_open(&self) -> bool {
        self.correction.is_some()
    }

    #[cfg(feature = "ui-harness")]
    pub(super) fn correction_item(&self) -> Option<Item> {
        self.correction.as_ref().and_then(|draft| draft.item)
    }

    /// No correction is waiting for the service.
    #[cfg(feature = "ui-harness")]
    pub(super) fn correction_settled(&self) -> bool {
        self.correction
            .as_ref()
            .is_none_or(|draft| draft.pending.is_none())
    }

    #[cfg(feature = "ui-harness")]
    pub(super) fn correction_error(&self) -> Option<String> {
        self.correction
            .as_ref()
            .and_then(|draft| draft.error.clone())
    }
}

/// The words and pauses as one wrapped run; returns the clicked item.
fn stream_view(
    ui: &mut egui::Ui,
    items: &[(Item, u64, u64)],
    selected: Option<Item>,
    words: Option<&Arc<crate::project::OriginalTranscript>>,
    pauses: Option<&CorrectedPauses>,
) -> Option<Item> {
    let mut job = egui::text::LayoutJob::default();
    let mut spans = Vec::with_capacity(items.len());
    for (item, start, end) in items {
        let (text, mut format) = match item {
            Item::Word(index) => {
                let Some(word) = words.and_then(|words| words.transcript.words().get(*index))
                else {
                    continue;
                };
                let corrected = words.is_some_and(|words| words.corrected(*index));
                let mut format = egui::TextFormat {
                    font_id: egui::FontId::proportional(13.0),
                    color: if word.approximate() {
                        style::muted(ui)
                    } else {
                        style::TEXT
                    },
                    italics: word.approximate(),
                    ..Default::default()
                };
                if corrected {
                    format.underline = egui::Stroke::new(1.0, style::LAVENDER);
                }
                (word.text.clone(), format)
            }
            Item::Pause(index) => {
                let corrected = pauses
                    .and_then(|pauses| pauses.corrected.get(*index))
                    .copied()
                    .unwrap_or(false);
                let duration =
                    (end - start) as f64 / f64::from(deadpan_analysis::ANALYSIS_SAMPLE_RATE);
                (
                    format!("‖ {duration:.2} s"),
                    egui::TextFormat {
                        font_id: egui::FontId::proportional(11.5),
                        color: if corrected {
                            style::LAVENDER
                        } else {
                            style::muted(ui)
                        },
                        ..Default::default()
                    },
                )
            }
        };
        if Some(*item) == selected {
            format.background = style::SELECTED;
            format.color = style::LAVENDER;
        }
        let begin = job.text.chars().count();
        job.append(&text, 0.0, format.clone());
        spans.push((begin..job.text.chars().count(), *item));
        job.append(
            " ",
            0.0,
            egui::TextFormat {
                background: egui::Color32::TRANSPARENT,
                underline: egui::Stroke::NONE,
                ..format
            },
        );
    }
    job.wrap.max_width = ui.available_width();
    let galley = ui.ctx().fonts_mut(|fonts| fonts.layout_job(job));
    let text = galley.text().to_owned();
    let response = egui::ScrollArea::vertical()
        .id_salt("correction-stream")
        .max_height(220.0)
        .show(ui, |ui| {
            ui.add(egui::Label::new(Arc::clone(&galley)).sense(egui::Sense::click()))
        })
        .inner;
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, &text));
    let position = response
        .interact_pointer_pos()
        .filter(|_| response.clicked())?;
    let offset = galley.cursor_from_pos(position - response.rect.min).index.0;
    spans
        .iter()
        .find(|(chars, _)| chars.contains(&offset))
        .map(|(_, item)| *item)
}

fn analysis_samples(workspace: &Workspace) -> u64 {
    workspace
        .speech_activity
        .as_ref()
        .map(|activity| activity.activity.audio().samples)
        .or_else(|| {
            workspace.transcript.as_ref().map(|transcript| {
                u64::from(transcript.transcript.audio().duration_cs) * CENTISECOND_SAMPLES
            })
        })
        .unwrap_or(0)
}

fn at_seconds(clock: Option<CorrectionClock>, sample: u64) -> f64 {
    clock.map_or(0.0, |clock| seconds(clock, sample))
}

fn seconds_of(workspace: &Workspace, sample: u64) -> f64 {
    at_seconds(clock(workspace), sample)
}

fn span(clock: Option<CorrectionClock>, start: u64, end: u64) -> String {
    format!(
        "{:.2}–{:.2} s",
        at_seconds(clock, start),
        at_seconds(clock, end)
    )
}

fn count(value: usize, noun: &str) -> String {
    format!("{value} {noun}{}", if value == 1 { "" } else { "s" })
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}
