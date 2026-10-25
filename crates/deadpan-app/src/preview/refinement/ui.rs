use super::*;
use deadpan_analysis::refinement::{OfferUnavailableReason, UnavailableReason};
use deadpan_core::ExactRatio;

const FOCUS_ID: &str = "word-refinement-focus";
#[derive(Debug, PartialEq, Eq)]
enum Action {
    Accept,
    Cancel,
    Manual,
    Models,
    Hear,
}

fn route(
    key: egui::Key,
    modifiers: egui::Modifiers,
    heading_focused: bool,
    ime: bool,
    repeat: bool,
) -> Option<Action> {
    if ime || repeat || modifiers != egui::Modifiers::NONE {
        return None;
    }
    match key {
        egui::Key::Escape => Some(Action::Cancel),
        egui::Key::Space if heading_focused => Some(Action::Hear),
        egui::Key::Enter if heading_focused => Some(Action::Accept),
        _ => None,
    }
}

fn route_focused(
    context: &egui::Context,
    key: egui::Key,
    modifiers: egui::Modifiers,
    ime: bool,
    repeat: bool,
) -> Option<Action> {
    route(
        key,
        modifiers,
        context.memory(|memory| memory.focused()) == Some(egui::Id::new(FOCUS_ID)),
        ime,
        repeat,
    )
}

fn reason(reason: &OfferUnavailableReason) -> String {
    match reason {
        OfferUnavailableReason::ManuallyPinned => "This word already has manually pinned timing.".into(),
        OfferUnavailableReason::OutsideCapturedContext => "The raw candidate is outside the captured context.".into(),
        OfferUnavailableReason::CrossesPreviousWord => "The candidate crosses the previous word's retained edge.".into(),
        OfferUnavailableReason::CrossesNextWord => "The candidate crosses the next word's retained edge.".into(),
        OfferUnavailableReason::CrossesAnotherSelectedProposal => "Selected candidates cross each other.".into(),
        OfferUnavailableReason::Backend(reasons) => reasons.iter().map(|reason| match reason {
            UnavailableReason::WordSupportBelowThreshold => "Low word path support.",
            UnavailableReason::PhoneSupportBelowThreshold => "Low phone path support.",
            UnavailableReason::EmptyInterval => "The candidate interval is empty.",
            UnavailableReason::ContextIncompleteOrPlacementAmbiguous => "Context is incomplete or placement is ambiguous: unexplained supported phonetic content.",
        }).collect::<Vec<_>>().join(" "),
    }
}

impl DeadpanApp {
    pub(in crate::preview) fn refinement_keyboard(&mut self, context: &egui::Context) {
        let events = context.input(|input| input.events.clone());
        help_scroll::observe_composition(&events, &mut self.ime_composing);
        self.bindings.clear();
        let ime = self.ime_composing
            || events
                .iter()
                .any(|event| matches!(event, egui::Event::Ime(_)));
        // egui activates focused buttons directly from Enter/Space. Retain
        // plain native activation while excluding composition, held repeats
        // and modified global shortcuts before widgets paint.
        retain_native_activation(context, ime);
        if ime {
            return;
        }
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
            let action = route_focused(context, key, modifiers, self.ime_composing, repeat);
            if let Some(action) = action {
                context.input_mut(|input| {
                    input.consume_key(modifiers, key);
                });
                self.refinement_action(action, context);
                break;
            }
        }
    }

    pub(in crate::preview) fn refinement_sheet(&mut self, context: &egui::Context) {
        let mut action = None;
        egui::Modal::new(egui::Id::new("word-refinement-sheet"))
            .frame(egui::Frame::popup(&context.style_of(egui::Theme::Dark)).inner_margin(24).corner_radius(10))
            .show(context, |ui| {
                super::super::accessibility::dialog(ui, "Review approximate local word timing");
                ui.set_width((context.content_rect().width() - 64.0).clamp(300.0, 680.0));
                let heading = ui.heading("Review local word timing");
                let focus = ui.interact(heading.rect, egui::Id::new(FOCUS_ID), egui::Sense::focusable_noninteractive());
                if std::mem::take(&mut self.refinement.focus_pending) { focus.request_focus(); }
                ui.colored_label(style::LAVENDER, "APPROXIMATE · UNSAVED");
                ui.label("The local English phoneme model proposes edges. Audition and inspect them before accepting. Path support is uncalibrated and is not a correctness probability.");
                egui::ScrollArea::vertical().max_height((context.content_rect().height() - 360.0).max(100.0)).show(ui, |ui| {
                    match &self.refinement.phase {
                        Phase::Running => { ui.spinner(); ui.label("Reading the captured Original context and running local alignment…"); }
                        Phase::NeedsModel => {
                            ui.label("The local word-timing model is not installed. Existing words and cuts are unchanged.");
                            if ui.button("Open Models to install local word timing").clicked() { action = Some(Action::Models); }
                        }
                        Phase::Failed(error) => { ui.colored_label(style::WARNING, error); }
                        Phase::Saving(_) => { ui.spinner(); ui.label("Saving one correction. Waiting for the transaction receipt…"); }
                        Phase::Ready(prepared) => {
                            if let Some(capture) = &self.refinement.capture {
                                let word = &capture.transcript.transcript.words()[capture.word];
                                let audio = capture.transcript.transcript.audio();
                                let origin = audio.origin as f64 / f64::from(audio.sample_rate);
                                ui.label(egui::RichText::new(format!("Original word “{}”", word.text)).strong());
                                ui.label(format!("Original-clock current edges: {:.2}–{:.2} s", origin + f64::from(word.start_cs) / 100.0, origin + f64::from(word.end_cs) / 100.0));
                                for offer in prepared.review.offers() {
                                    if let Some(raw) = offer.raw_global_interval {
                                        ui.label(format!("Original-clock raw candidate: {:.5}–{:.5} s", origin + raw.start.0 as f64 / 16_000.0, origin + raw.end.0 as f64 / 16_000.0));
                                    }
                                    if let Some(bounds) = offer.centisecond_bounds {
                                        ui.label(format!("Original-clock edges proposed on acceptance: {:.2}–{:.2} s · start floors and end ceils to 10 ms", origin + f64::from(bounds.start_cs) / 100.0, origin + f64::from(bounds.end_cs) / 100.0));
                                    }
                                    if let RefinementOfferAvailability::Unavailable(reasons) = &offer.availability {
                                        ui.colored_label(style::WARNING, "UNAVAILABLE · diagnostic raw candidate only");
                                        for why in reasons { ui.label(reason(why)); }
                                    }
                                }
                                let local = capture.word - prepared.review.capture().context_words().start;
                                let raw = &prepared.review.diagnostics().words[local];
                                ui.weak(format!("Uncalibrated path support: word mean {:.4}, weakest phone mean {:.4}", raw.mean_phone_frame_posterior, raw.minimum_phone_mean_posterior));
                                waveform(ui, capture, prepared);
                            }
                        }
                    }
                });
                ui.separator();
                let available = matches!(&self.refinement.phase, Phase::Ready(prepared)
                    if prepared.review.offers().iter().all(|offer| offer.availability == RefinementOfferAvailability::Approximate));
                let saving = matches!(self.refinement.phase, Phase::Saving(_));
                let can_hear = matches!(self.refinement.phase, Phase::Ready(_));
                let manual = self.refinement.capture.as_ref().is_some_and(|capture| self.workspace.as_ref().is_some_and(|workspace| capture.matches(workspace)));
                ui.horizontal_wrapped(|ui| {
                    if ui.add_enabled(can_hear, style::action("Hear comparison context", "Space")).clicked() { action = Some(Action::Hear); }
                    if ui.add_enabled(available && !saving, style::action("Accept approximate timing", "Enter")).clicked() { action = Some(Action::Accept); }
                    if ui.add_enabled(!saving && manual, egui::Button::new("Adjust existing edges manually")).clicked() { action = Some(Action::Manual); }
                    if ui.add_enabled(!saving, style::action("Discard / close", "Esc")).clicked() { action = Some(Action::Cancel); }
                });
                ui.weak("Acceptance creates one correction Undo step. Existing cuts and all other word bounds stay unchanged. Esc discards this proposal.");
            });
        if let Some(action) = action {
            self.refinement_action(action, context);
        }
    }

    fn refinement_action(&mut self, action: Action, context: &egui::Context) {
        match action {
            Action::Accept => self.accept_refinement(),
            Action::Cancel => self.close_refinement(context),
            Action::Manual => {
                let word = self
                    .refinement
                    .capture
                    .as_ref()
                    .filter(|capture| {
                        self.workspace
                            .as_ref()
                            .is_some_and(|workspace| capture.matches(workspace))
                    })
                    .map(|capture| capture.word);
                self.close_refinement(context);
                if let Some(word) = word {
                    self.begin_refinement_manual(word);
                }
            }
            Action::Models => {
                self.close_refinement(context);
                self.close_corrections(context);
                let pack = deadpan_models::packs::approved_packs()
                    .into_iter()
                    .find(|pack| pack.supports(deadpan_models::packs::Operation::RefineWords))
                    .map(|pack| pack.pack_id);
                self.open_models(pack.as_deref(), context);
            }
            Action::Hear => self.audition_refinement(),
        }
    }

    fn audition_refinement(&mut self) {
        if self.transport.is_some() {
            self.receive_playback();
            self.pause_playback();
            return;
        }
        let (Some(capture), Phase::Ready(prepared)) =
            (&self.refinement.capture, &self.refinement.phase)
        else {
            return;
        };
        if self
            .workspace
            .as_ref()
            .is_none_or(|workspace| !capture.matches(workspace))
            || capture.interrupted()
        {
            return;
        }
        let word = &capture.transcript.transcript.words()[capture.word];
        // Timing annotations do not change the audio. Hear one fixed comparison
        // window enclosing both sets of edges, with identical lead/follow.
        let old = (u64::from(word.start_cs) * 160, u64::from(word.end_cs) * 160);
        let (start, end) = prepared
            .review
            .offers()
            .first()
            .and_then(|offer| offer.raw_global_interval)
            .map_or(old, |raw| (old.0.min(raw.start.0), old.1.max(raw.end.0)));
        let samples = |global: u64| -> Result<i64, deadpan_core::TimeError> {
            ExactRatio::new(
                i128::from(prepared.analysis_origin) - i128::from(prepared.source_start),
                i128::from(prepared.source_rate),
            )?
            .checked_add(ExactRatio::new(i128::from(global), 16_000)?)?
            .checked_mul(ExactRatio::integer(48_000))?
            .round_even()
            .and_then(|sample| i64::try_from(sample).map_err(|_| deadpan_core::TimeError::Overflow))
        };
        let (Ok(start), Ok(end)) = (samples(start), samples(end)) else {
            return;
        };
        let end_limit = prepared.audition.duration_samples().0;
        let start = start.clamp(0, end_limit).saturating_sub(24_000).max(0);
        let end = end
            .clamp(0, end_limit)
            .saturating_add(36_000)
            .min(end_limit);
        let Ok(window) = deadpan_playback::Window::new(AudioSample(start), AudioSample(end), false)
        else {
            return;
        };
        let domain = crate::transport::Domain::AudioRange(prepared.audition.clone());
        let resumed = self
            .resume
            .as_ref()
            .and_then(|resume| {
                resume.sample_for_domain(
                    crate::transport::ContentRef {
                        session: capture.workspace.session,
                        project: capture.workspace.document.project_id(),
                        revision: capture.workspace.document.revision_id(),
                        content: &deadpan_playback::ContentIdentity::Committed,
                    },
                    &domain,
                    &window,
                    self.refinement.audition_cursor,
                )
            })
            .filter(|sample| *sample < window.end());
        self.start_domain_playback(domain, window, resumed.unwrap_or(window.start()));
    }
}

fn waveform(ui: &mut egui::Ui, capture: &Capture, prepared: &Prepared) {
    ui.label("Measured Original waveform · current edges yellow · raw candidate lavender");
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 116.0),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, style::PANEL);
    let middle = rect.center().y;
    for (index, (low, high)) in prepared.waveform.iter().enumerate() {
        let x = rect.left() + rect.width() * index as f32 / prepared.waveform.len().max(1) as f32;
        painter.line_segment(
            [
                egui::pos2(x, middle - high.clamp(-1.0, 1.0) * rect.height() * 0.45),
                egui::pos2(x, middle - low.clamp(-1.0, 1.0) * rect.height() * 0.45),
            ],
            egui::Stroke::new(1.0, style::MUTED),
        );
    }
    let window = prepared.review.capture().window();
    let edge = |sample: u64, color| {
        let ratio = sample
            .saturating_sub(window.start.0)
            .min(window.end.0 - window.start.0) as f64
            / (window.end.0 - window.start.0) as f64;
        let x = rect.left() + ratio as f32 * rect.width();
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(2.0, color),
        );
    };
    let word = &capture.transcript.transcript.words()[capture.word];
    edge(u64::from(word.start_cs) * 160, style::CURSOR);
    edge(u64::from(word.end_cs) * 160, style::CURSOR);
    if let Some(raw) = prepared
        .review
        .offers()
        .first()
        .and_then(|offer| offer.raw_global_interval)
    {
        edge(raw.start.0, style::LAVENDER);
        edge(raw.end.0, style::LAVENDER);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(context: &egui::Context, input: egui::RawInput, draw: impl FnMut(&mut egui::Ui)) {
        let mut output = context.run_ui(input, draw);
        output.textures_delta.clear();
    }

    fn paint_button(ui: &mut egui::Ui, focus: bool, clicked: &std::cell::Cell<bool>) -> egui::Id {
        let response = ui.button("Accept approximate timing");
        if focus {
            response.request_focus();
        }
        clicked.set(clicked.get() || response.clicked());
        response.id
    }

    #[test]
    fn actual_button_focus_keeps_space_activation_out_of_the_audition_router() {
        let context = egui::Context::default();
        let button_id = std::cell::Cell::new(None);
        let clicked = std::cell::Cell::new(false);
        frame(&context, egui::RawInput::default(), |ui| {
            button_id.set(Some(paint_button(ui, true, &clicked)));
        });
        assert_eq!(context.memory(|memory| memory.focused()), button_id.get());
        let space = egui::Event::Key {
            key: egui::Key::Space,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        frame(
            &context,
            egui::RawInput {
                events: vec![space],
                ..Default::default()
            },
            |ui| {
                assert_eq!(
                    route_focused(
                        ui.ctx(),
                        egui::Key::Space,
                        egui::Modifiers::NONE,
                        false,
                        false
                    ),
                    None
                );
                paint_button(ui, false, &clicked);
            },
        );
        assert!(
            clicked.get(),
            "native focused button receives Space instead of audition"
        );
    }

    #[test]
    fn composed_held_and_modified_native_button_keys_cannot_accept() {
        for (key, modifiers, repeat, ime) in [
            (egui::Key::Enter, egui::Modifiers::COMMAND, false, false),
            (
                egui::Key::Space,
                egui::Modifiers::MAC_CMD | egui::Modifiers::COMMAND,
                false,
                false,
            ),
            (egui::Key::Enter, egui::Modifiers::NONE, true, false),
            (egui::Key::Space, egui::Modifiers::NONE, true, false),
            (egui::Key::Enter, egui::Modifiers::NONE, false, true),
            (egui::Key::Space, egui::Modifiers::NONE, false, true),
        ] {
            let context = egui::Context::default();
            let clicked = std::cell::Cell::new(false);
            // egui derives repeat from its retained keys_down set, overriding
            // the raw event flag. Hold the key before the button gains focus.
            let held = if repeat {
                vec![egui::Event::Key {
                    key,
                    physical_key: None,
                    modifiers: egui::Modifiers::NONE,
                    repeat: false,
                    pressed: true,
                }]
            } else {
                Vec::new()
            };
            frame(
                &context,
                egui::RawInput {
                    events: held,
                    ..Default::default()
                },
                |ui| {
                    // Composition may begin with the same key that remains
                    // held when the review gains native button focus.
                    retain_native_activation(ui.ctx(), true);
                    paint_button(ui, true, &clicked);
                },
            );
            assert!(!clicked.get(), "focusing the button cannot activate it");
            let event = egui::Event::Key {
                key,
                physical_key: None,
                modifiers,
                repeat: false,
                pressed: true,
            };
            frame(
                &context,
                egui::RawInput {
                    events: vec![event],
                    ..Default::default()
                },
                |ui| {
                    assert!(ui.input(|input| input.events.iter().any(|event| {
                        matches!(event, egui::Event::Key { key: actual, repeat: actual_repeat, pressed: true, .. }
                            if *actual == key && *actual_repeat == repeat)
                    })), "the witness must exercise egui's actual held-key state");
                    retain_native_activation(ui.ctx(), ime);
                    assert!(!ui.input(|input| input.key_pressed(key)));
                    paint_button(ui, false, &clicked);
                },
            );
            assert!(
                !clicked.get(),
                "{key:?} {modifiers:?}, repeat={repeat}, ime={ime}"
            );
            // Filtering an activation must preserve release bookkeeping and
            // allow the next deliberate unmodified press on this same button.
            frame(
                &context,
                egui::RawInput {
                    events: vec![egui::Event::Key {
                        key,
                        physical_key: None,
                        modifiers: egui::Modifiers::NONE,
                        repeat: false,
                        pressed: false,
                    }],
                    ..Default::default()
                },
                |ui| {
                    retain_native_activation(ui.ctx(), false);
                    paint_button(ui, false, &clicked);
                },
            );
            assert!(!clicked.get(), "release cannot activate the button");
            frame(
                &context,
                egui::RawInput {
                    events: vec![egui::Event::Key {
                        key,
                        physical_key: None,
                        modifiers: egui::Modifiers::NONE,
                        repeat: false,
                        pressed: true,
                    }],
                    ..Default::default()
                },
                |ui| {
                    retain_native_activation(ui.ctx(), false);
                    paint_button(ui, false, &clicked);
                },
            );
            assert!(clicked.get(), "fresh explicit activation remains available");
        }
    }

    #[test]
    fn refinement_modal_requires_explicit_acceptance_and_keeps_native_button_focus() {
        assert_eq!(
            route(egui::Key::Enter, egui::Modifiers::NONE, true, false, false),
            Some(Action::Accept)
        );
        assert_eq!(
            route(egui::Key::Enter, egui::Modifiers::NONE, false, false, false),
            None
        );
        assert_eq!(
            route(egui::Key::Space, egui::Modifiers::NONE, true, false, false),
            Some(Action::Hear)
        );
        assert_eq!(
            route(
                egui::Key::Escape,
                egui::Modifiers::NONE,
                false,
                false,
                false
            ),
            Some(Action::Cancel)
        );
        for key in [egui::Key::Enter, egui::Key::Space, egui::Key::Escape] {
            assert_eq!(route(key, egui::Modifiers::NONE, true, true, false), None);
            assert_eq!(route(key, egui::Modifiers::NONE, true, false, true), None);
            for modifiers in [
                egui::Modifiers::COMMAND,
                egui::Modifiers::MAC_CMD | egui::Modifiers::COMMAND,
                egui::Modifiers::CTRL,
                egui::Modifiers::ALT,
                egui::Modifiers::SHIFT,
            ] {
                assert_eq!(route(key, modifiers, true, false, false), None);
            }
        }
        assert_eq!(
            route(egui::Key::Tab, egui::Modifiers::NONE, false, false, false),
            None
        );
        assert_eq!(
            route(
                egui::Key::Backspace,
                egui::Modifiers::NONE,
                false,
                false,
                false
            ),
            None
        );
    }
}
