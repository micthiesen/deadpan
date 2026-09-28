//! Native fields edit an exact recipe only when their row button is activated.
//! The graph is derived from that recipe; it contains no synthetic waveform.

use deadpan_core::{
    ExactRatio, GainClock, GainCurve, GainDb, GainEnvelope, GainRange, GainSegment,
    MAX_GAIN_ENVELOPES, MAX_GAIN_MUTE_RANGES, MAX_GAIN_SEGMENTS,
};
use eframe::egui::{self, Color32, Id, Pos2, Rect, Sense, Stroke, Vec2};

use crate::gain::{GainEdit, format_db, format_frames, parse_db, parse_frames};

const FIELD_NAMES: [&str; 9] = [
    "trim",
    "range-start",
    "range-end",
    "key-time",
    "key-value",
    "control-one",
    "control-two",
    "mute-start",
    "mute-end",
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum CurveChoice {
    Step,
    Linear,
    Smoothstep,
    Cubic,
}

impl CurveChoice {
    fn label(self) -> &'static str {
        match self {
            Self::Step => "Step",
            Self::Linear => "Linear",
            Self::Smoothstep => "Smoothstep",
            Self::Cubic => "Cubic",
        }
    }
    fn from_curve(curve: GainCurve) -> Self {
        match curve {
            GainCurve::Step => Self::Step,
            GainCurve::Linear => Self::Linear,
            GainCurve::Smoothstep => Self::Smoothstep,
            GainCurve::Cubic { .. } => Self::Cubic,
        }
    }
}

pub(super) struct Controls {
    pub(super) error: Option<String>,
    owner_frames: i64,
    trim: String,
    envelope: usize,
    key: usize,
    adding_key: bool,
    range_start: String,
    range_end: String,
    key_time: String,
    key_value: String,
    curve: CurveChoice,
    control_one: String,
    control_two: String,
    mute: usize,
    adding_mute: bool,
    mute_start: String,
    mute_end: String,
}

impl Controls {
    pub(super) fn new(edit: &GainEdit, owner_frames: i64) -> Self {
        let mut controls = Self {
            error: None,
            owner_frames,
            trim: format_db(edit.trim()),
            envelope: 0,
            key: 0,
            adding_key: false,
            range_start: String::new(),
            range_end: String::new(),
            key_time: String::new(),
            key_value: String::new(),
            curve: CurveChoice::Linear,
            control_one: "0".into(),
            control_two: "0".into(),
            mute: 0,
            adding_mute: false,
            mute_start: String::new(),
            mute_end: String::new(),
        };
        controls.load_envelope(edit);
        controls.load_mute(edit);
        controls
    }

    /// The parent may apply only the recipe the fields currently describe.
    /// Untouched fields do not create configured unity or otherwise edit intent.
    pub(super) fn ready(&self, edit: &GainEdit) -> bool {
        self.error.is_none()
            && !self.trim_pending(edit)
            && !self.envelope_pending(edit)
            && !self.mute_pending(edit)
    }

    pub(super) fn text_focused(context: &egui::Context) -> bool {
        context.memory(|memory| {
            memory
                .focused()
                .is_some_and(|focused| FIELD_NAMES.iter().any(|name| focused == field_id(name)))
        })
    }

    pub(super) fn show(
        &mut self,
        ui: &mut egui::Ui,
        edit: &mut GainEdit,
        mut waveform: impl FnMut(&mut egui::Ui, i64),
    ) -> bool {
        let before = edit.recipe().clone();
        ui.vertical(|ui| {
            self.trim_row(ui, edit);
            ui.separator();
            self.envelope_selector(ui, edit);
            // The exact time/value fields and native curve menu need a useful
            // column width. Stack them at the minimum window size so a menu's
            // measured width cannot push its hit target past the viewport.
            if ui.available_width() >= 1_100.0 && !edit.envelopes().is_empty() {
                ui.columns(2, |columns| {
                    waveform(&mut columns[0], self.owner_frames);
                });
                // Keep exact key fields beside their curve as the editor
                // scrolls. Waveform height belongs to the preceding row.
                ui.columns(2, |columns| {
                    self.graph(&mut columns[0], edit);
                    self.range_row(&mut columns[1], edit);
                    self.key_rows(&mut columns[1], edit);
                });
            } else {
                waveform(ui, self.owner_frames);
                self.graph(ui, edit);
                if !edit.envelopes().is_empty() {
                    self.range_row(ui, edit);
                    self.key_rows(ui, edit);
                }
            }
            ui.separator();
            self.mute_rows(ui, edit);
            let pending = !self.ready(edit);
            ui.horizontal_wrapped(|ui| {
                ui.small("Tab: fields and buttons · Enter: activate focused button");
                if pending {
                    ui.colored_label(Color32::from_rgb(220, 187, 110), "Unapplied fields");
                    if ui
                        .button("Reset fields")
                        .on_hover_text(
                            "Discard unapplied field text; retain the current draft recipe.",
                        )
                        .reveal_on_focus()
                        .clicked()
                    {
                        self.trim = format_db(edit.trim());
                        self.adding_key = false;
                        self.adding_mute = false;
                        self.load_envelope(edit);
                        self.load_mute(edit);
                        self.error = None;
                    }
                }
            });
            if let Some(error) = &self.error {
                ui.colored_label(Color32::from_rgb(240, 146, 146), error);
            }
        });
        edit.recipe() != &before
    }

    fn trim_row(&mut self, ui: &mut egui::Ui, edit: &mut GainEdit) {
        ui.horizontal_wrapped(|ui| {
            field(ui, "Whole beat trim · dB", "trim", &mut self.trim, 78.0);
            if ui.button("Set trim").reveal_on_focus().clicked() {
                let result = parse_db(&self.trim).and_then(|trim| edit.set_trim(trim));
                if self.accept(result) {
                    self.trim = format_db(edit.trim());
                }
            }
            for (label, delta) in [("−3 dB", -3000), ("+3 dB", 3000)] {
                if ui
                    .add_enabled(!self.trim_pending(edit), egui::Button::new(label))
                    .reveal_on_focus()
                    .clicked()
                    && self.accept(edit.adjust_trim(delta))
                {
                    self.trim = format_db(edit.trim());
                }
            }
            let mut muted = edit.muted();
            if ui
                .checkbox(&mut muted, "Mute entire beat")
                .reveal_on_focus()
                .changed()
            {
                self.accept(edit.set_muted(muted));
            }
            if edit.clip().is_none() {
                ui.small("No authored gain stage");
            }
        });
    }

    fn envelope_selector(&mut self, ui: &mut egui::Ui, edit: &mut GainEdit) {
        let pending = self.envelope_pending(edit);
        ui.horizontal_wrapped(|ui| {
            ui.strong("Gain envelopes");
            let old = self.envelope;
            ui.add_enabled_ui(!pending && !edit.envelopes().is_empty(), |ui| {
                egui::ComboBox::from_id_salt("gain-envelope-selection")
                    .selected_text(if edit.envelopes().is_empty() {
                        "None".into()
                    } else {
                        format!("Envelope {}", self.envelope + 1)
                    })
                    .show_ui(ui, |ui| {
                        for index in 0..edit.envelopes().len() {
                            ui.selectable_value(
                                &mut self.envelope,
                                index,
                                format!("Envelope {}", index + 1),
                            )
                            .reveal_on_focus();
                        }
                    })
                    .response
                    .reveal_on_focus();
            });
            if old != self.envelope {
                self.key = 0;
                self.load_envelope(edit);
            }
            if ui
                .add_enabled(
                    !pending && edit.envelopes().len() < MAX_GAIN_ENVELOPES,
                    egui::Button::new("Add envelope"),
                )
                .on_hover_text("Add an explicit unity envelope over the current owner allocation.")
                .reveal_on_focus()
                .clicked()
            {
                let range = default_range(self.owner_frames);
                let result = GainSegment::new(range.end(), GainDb::UNITY, GainCurve::Linear)
                    .and_then(|end| {
                        GainEnvelope::new(GainClock::OwnerOutput, range, GainDb::UNITY, vec![end])
                    })
                    .map_err(|error| error.to_string())
                    .and_then(|envelope| edit.add_envelope(envelope));
                match result {
                    Ok(index) => {
                        self.envelope = index;
                        self.key = 0;
                        self.error = None;
                        self.load_envelope(edit);
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            if ui
                .add_enabled(
                    !pending && !edit.envelopes().is_empty(),
                    egui::Button::new("Remove envelope"),
                )
                .reveal_on_focus()
                .clicked()
                && self.accept(edit.remove_envelope(self.envelope))
            {
                self.envelope = self.envelope.min(edit.envelopes().len().saturating_sub(1));
                self.key = 0;
                self.load_envelope(edit);
            }
        });
    }

    fn range_row(&mut self, ui: &mut egui::Ui, edit: &mut GainEdit) {
        ui.horizontal_wrapped(|ui| {
            field(
                ui,
                "In · owner frames",
                "range-start",
                &mut self.range_start,
                90.0,
            );
            field(
                ui,
                "Out · exclusive",
                "range-end",
                &mut self.range_end,
                90.0,
            );
            if ui
                .button("Update range")
                .on_hover_text(
                    "Move boundary keys; retain every interior key at its exact position.",
                )
                .reveal_on_focus()
                .clicked()
            {
                let old_time = edit.key(self.envelope, self.key).ok().map(|key| key.0);
                let time_was_current = parse_frames(&self.key_time).ok() == old_time;
                let result = parsed_range(&self.range_start, &self.range_end)
                    .and_then(|range| edit.set_envelope_range(self.envelope, range));
                if self.accept(result) {
                    let range = edit.envelopes()[self.envelope].range();
                    self.range_start = format_frames(range.start());
                    self.range_end = format_frames(range.end());
                    if !self.adding_key
                        && time_was_current
                        && let Ok((time, _, _)) = edit.key(self.envelope, self.key)
                    {
                        self.key_time = format_frames(time);
                    }
                }
            }
        });
    }

    fn key_rows(&mut self, ui: &mut egui::Ui, edit: &mut GainEdit) {
        let last = edit.envelopes()[self.envelope].segments().len();
        ui.horizontal_wrapped(|ui| {
            if self.adding_key {
                ui.strong("New interior key");
            } else {
                let old = self.key;
                ui.add_enabled_ui(!self.key_pending(edit), |ui| {
                    egui::ComboBox::from_id_salt("gain-key-selection")
                        .selected_text(format!(
                            "Key {}{}",
                            self.key,
                            if self.key == 0 {
                                " · initial"
                            } else if self.key == last {
                                " · final"
                            } else {
                                ""
                            }
                        ))
                        .show_ui(ui, |ui| {
                            for index in 0..=last {
                                if let Ok((time, _, _)) = edit.key(self.envelope, index) {
                                    ui.selectable_value(
                                        &mut self.key,
                                        index,
                                        format!("Key {index} · {} f", format_frames(time)),
                                    )
                                    .reveal_on_focus();
                                }
                            }
                        })
                        .response
                        .reveal_on_focus();
                });
                if old != self.key {
                    self.load_key(edit);
                }
                let clean = !self.key_pending(edit);
                if ui
                    .add_enabled(clean && self.key > 0, egui::Button::new("Previous point"))
                    .reveal_on_focus()
                    .clicked()
                {
                    self.key -= 1;
                    self.load_key(edit);
                }
                if ui
                    .add_enabled(clean && self.key < last, egui::Button::new("Next point"))
                    .reveal_on_focus()
                    .clicked()
                {
                    self.key += 1;
                    self.load_key(edit);
                }
                if ui
                    .add_enabled(
                        clean && last < MAX_GAIN_SEGMENTS,
                        egui::Button::new("Add point"),
                    )
                    .reveal_on_focus()
                    .clicked()
                {
                    self.begin_key(edit);
                }
                if ui
                    .add_enabled(
                        clean && self.key > 0 && self.key < last,
                        egui::Button::new("Remove point"),
                    )
                    .reveal_on_focus()
                    .clicked()
                    && self.accept(edit.remove_key(self.envelope, self.key))
                {
                    self.key = self
                        .key
                        .min(edit.envelopes()[self.envelope].segments().len());
                    self.load_key(edit);
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            field(
                ui,
                "Time · owner frames",
                "key-time",
                &mut self.key_time,
                110.0,
            );
            field(ui, "Value · dB", "key-value", &mut self.key_value, 76.0);
            if self.adding_key || self.key > 0 {
                egui::ComboBox::from_id_salt("gain-key-curve")
                    .selected_text(self.curve.label())
                    .show_ui(ui, |ui| {
                        for curve in [
                            CurveChoice::Step,
                            CurveChoice::Linear,
                            CurveChoice::Smoothstep,
                            CurveChoice::Cubic,
                        ] {
                            ui.selectable_value(&mut self.curve, curve, curve.label())
                                .reveal_on_focus();
                        }
                    })
                    .response
                    .reveal_on_focus();
                ui.small("incoming curve");
            } else {
                ui.small("Initial key has no incoming curve");
            }
        });
        if (self.adding_key || self.key > 0) && self.curve == CurveChoice::Cubic {
            ui.horizontal_wrapped(|ui| {
                field(
                    ui,
                    "Cubic control 1 · dB",
                    "control-one",
                    &mut self.control_one,
                    76.0,
                );
                field(
                    ui,
                    "Control 2 · dB",
                    "control-two",
                    &mut self.control_two,
                    76.0,
                );
            });
        }
        ui.horizontal_wrapped(|ui| {
            if ui
                .button(if self.adding_key {
                    "Insert key"
                } else {
                    "Update key"
                })
                .reveal_on_focus()
                .clicked()
            {
                self.apply_key(edit);
            }
            if self.adding_key && ui.button("Cancel new point").reveal_on_focus().clicked() {
                self.adding_key = false;
                self.load_key(edit);
                self.error = None;
            }
            ui.small("Outside [In, Out): 0 dB");
        });
    }

    fn mute_rows(&mut self, ui: &mut egui::Ui, edit: &mut GainEdit) {
        ui.horizontal_wrapped(|ui| {
            ui.strong("Mute ranges");
            if self.adding_mute {
                ui.label("New range");
            } else {
                let old = self.mute;
                ui.add_enabled_ui(
                    !self.mute_pending(edit) && !edit.mute_ranges().is_empty(),
                    |ui| {
                        egui::ComboBox::from_id_salt("gain-mute-selection")
                            .selected_text(if edit.mute_ranges().is_empty() {
                                "None".into()
                            } else {
                                format!("Range {}", self.mute + 1)
                            })
                            .show_ui(ui, |ui| {
                                for (index, range) in edit.mute_ranges().iter().enumerate() {
                                    ui.selectable_value(
                                        &mut self.mute,
                                        index,
                                        format!(
                                            "{} · [{}, {}) f",
                                            index + 1,
                                            format_frames(range.start()),
                                            format_frames(range.end())
                                        ),
                                    )
                                    .reveal_on_focus();
                                }
                            })
                            .response
                            .reveal_on_focus();
                    },
                );
                if old != self.mute {
                    self.load_mute(edit);
                }
                if ui
                    .add_enabled(
                        !self.mute_pending(edit) && edit.mute_ranges().len() < MAX_GAIN_MUTE_RANGES,
                        egui::Button::new("Add mute range"),
                    )
                    .reveal_on_focus()
                    .clicked()
                {
                    self.adding_mute = true;
                    let range = default_range(self.owner_frames);
                    self.mute_start = format_frames(range.start());
                    self.mute_end = format_frames(range.end());
                }
                if ui
                    .add_enabled(
                        !self.mute_pending(edit) && !edit.mute_ranges().is_empty(),
                        egui::Button::new("Remove mute range"),
                    )
                    .reveal_on_focus()
                    .clicked()
                    && self.accept(edit.remove_mute_range(self.mute))
                {
                    self.mute = self.mute.min(edit.mute_ranges().len().saturating_sub(1));
                    self.load_mute(edit);
                }
            }
        });
        if self.adding_mute || !edit.mute_ranges().is_empty() {
            ui.horizontal_wrapped(|ui| {
                field(
                    ui,
                    "Mute In · frames",
                    "mute-start",
                    &mut self.mute_start,
                    90.0,
                );
                field(
                    ui,
                    "Mute Out · exclusive",
                    "mute-end",
                    &mut self.mute_end,
                    90.0,
                );
                if ui
                    .button(if self.adding_mute {
                        "Insert mute range"
                    } else {
                        "Update mute range"
                    })
                    .reveal_on_focus()
                    .clicked()
                {
                    match parsed_range(&self.mute_start, &self.mute_end) {
                        Ok(range) => {
                            if self.adding_mute {
                                match edit.add_mute_range(range) {
                                    Ok(index) => {
                                        self.mute = index;
                                        self.adding_mute = false;
                                        self.error = None;
                                        self.load_mute(edit);
                                    }
                                    Err(error) => self.error = Some(error),
                                }
                            } else if self.accept(edit.set_mute_range(self.mute, range)) {
                                self.load_mute(edit);
                            }
                        }
                        Err(error) => self.error = Some(error),
                    }
                }
                if self.adding_mute
                    && ui
                        .button("Cancel new mute range")
                        .reveal_on_focus()
                        .clicked()
                {
                    self.adding_mute = false;
                    self.load_mute(edit);
                    self.error = None;
                }
            });
        }
    }

    fn apply_key(&mut self, edit: &mut GainEdit) {
        let (time, value, curve) = match self.parsed_key() {
            Ok(proposed) => proposed,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let old_range = edit.envelopes()[self.envelope].range();
        let start_was_current = parse_frames(&self.range_start).ok() == Some(old_range.start());
        let end_was_current = parse_frames(&self.range_end).ok() == Some(old_range.end());
        if self.adding_key {
            let Some(curve) = curve else {
                self.error = Some("A new key needs an incoming curve.".into());
                return;
            };
            match edit.insert_key(self.envelope, time, value, curve) {
                Ok(key) => {
                    self.key = key;
                    self.adding_key = false;
                    self.error = None;
                    self.load_key(edit);
                }
                Err(error) => {
                    self.error = Some(error);
                    return;
                }
            }
        } else if self.accept(edit.set_key(self.envelope, self.key, time, value, curve)) {
            self.load_key(edit);
        } else {
            return;
        }
        let range = edit.envelopes()[self.envelope].range();
        if start_was_current {
            self.range_start = format_frames(range.start());
        }
        if end_was_current {
            self.range_end = format_frames(range.end());
        }
    }

    fn begin_key(&mut self, edit: &GainEdit) {
        let Some(envelope) = edit.envelopes().get(self.envelope) else {
            return;
        };
        let ending = self.key.min(envelope.segments().len().saturating_sub(1)) + 1;
        let Ok((start, value, _)) = edit.key(self.envelope, ending - 1) else {
            return;
        };
        let Ok((end, _, _)) = edit.key(self.envelope, ending) else {
            return;
        };
        let midpoint = start.checked_div(ExactRatio::integer(2)).and_then(|start| {
            end.checked_div(ExactRatio::integer(2))
                .and_then(|end| start.checked_add(end))
        });
        let overflow = midpoint.is_err();
        self.key_time = midpoint.map_or_else(|_| format_frames(start), format_frames);
        self.key_value = format_db(value);
        self.curve = CurveChoice::Linear;
        self.adding_key = true;
        self.error = overflow.then(|| {
            "Enter a distinct interior time; the exact midpoint exceeds the numeric bounds."
                .to_owned()
        });
    }

    fn parsed_key(&self) -> Result<(ExactRatio, GainDb, Option<GainCurve>), String> {
        let time = parse_frames(&self.key_time)?;
        let value = parse_db(&self.key_value)?;
        let curve = if !self.adding_key && self.key == 0 {
            None
        } else {
            Some(match self.curve {
                CurveChoice::Step => GainCurve::Step,
                CurveChoice::Linear => GainCurve::Linear,
                CurveChoice::Smoothstep => GainCurve::Smoothstep,
                CurveChoice::Cubic => GainCurve::Cubic {
                    control1: parse_db(&self.control_one)?,
                    control2: parse_db(&self.control_two)?,
                },
            })
        };
        Ok((time, value, curve))
    }

    fn trim_pending(&self, edit: &GainEdit) -> bool {
        parse_db(&self.trim).ok() != Some(edit.trim())
    }

    fn key_pending(&self, edit: &GainEdit) -> bool {
        self.adding_key
            || (!edit.envelopes().is_empty()
                && self.parsed_key().ok() != edit.key(self.envelope, self.key).ok())
    }

    fn envelope_pending(&self, edit: &GainEdit) -> bool {
        self.key_pending(edit)
            || edit.envelopes().get(self.envelope).is_some_and(|envelope| {
                parsed_range(&self.range_start, &self.range_end).ok() != Some(envelope.range())
            })
    }

    fn mute_pending(&self, edit: &GainEdit) -> bool {
        self.adding_mute
            || edit.mute_ranges().get(self.mute).is_some_and(|range| {
                parsed_range(&self.mute_start, &self.mute_end).ok() != Some(*range)
            })
    }

    fn load_envelope(&mut self, edit: &GainEdit) {
        if let Some(envelope) = edit.envelopes().get(self.envelope) {
            self.range_start = format_frames(envelope.range().start());
            self.range_end = format_frames(envelope.range().end());
            self.key = self.key.min(envelope.segments().len());
        }
        self.load_key(edit);
    }

    fn load_key(&mut self, edit: &GainEdit) {
        if let Ok((time, value, curve)) = edit.key(self.envelope, self.key) {
            self.key_time = format_frames(time);
            self.key_value = format_db(value);
            if let Some(curve) = curve {
                self.curve = CurveChoice::from_curve(curve);
                if let GainCurve::Cubic { control1, control2 } = curve {
                    self.control_one = format_db(control1);
                    self.control_two = format_db(control2);
                }
            }
        }
    }

    fn load_mute(&mut self, edit: &GainEdit) {
        if let Some(range) = edit.mute_ranges().get(self.mute) {
            self.mute_start = format_frames(range.start());
            self.mute_end = format_frames(range.end());
        }
    }

    fn accept(&mut self, result: Result<(), String>) -> bool {
        match result {
            Ok(()) => {
                self.error = None;
                true
            }
            Err(error) => {
                self.error = Some(error);
                false
            }
        }
    }

    fn graph(&mut self, ui: &mut egui::Ui, edit: &GainEdit) {
        let Some(envelope) = edit.envelopes().get(self.envelope) else {
            ui.small("Add an envelope to edit exact owner-output keys.");
            return;
        };
        if self.owner_frames <= 0 {
            ui.small("This owner has no allocated frames; its keys remain retained.");
            return;
        }
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(ui.available_width().max(1.0), 106.0),
            Sense::hover(),
        );
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Image,
                true,
                "Gain envelope graph · owner-output frames",
            )
        });
        let plot = plot_rect(rect);
        if plot.width() <= 1.0 {
            return;
        }
        let mut minimum = envelope.initial().millidecibels().min(0);
        let mut maximum = envelope.initial().millidecibels().max(0);
        for segment in envelope.segments() {
            minimum = minimum.min(segment.value().millidecibels());
            maximum = maximum.max(segment.value().millidecibels());
            if let GainCurve::Cubic { control1, control2 } = segment.curve() {
                minimum = minimum
                    .min(control1.millidecibels())
                    .min(control2.millidecibels());
                maximum = maximum
                    .max(control1.millidecibels())
                    .max(control2.millidecibels());
            }
        }
        let minimum = f64::from(minimum) - 3000.0;
        let maximum = f64::from(maximum) + 3000.0;
        let point = |time: ExactRatio, value: f64| {
            Pos2::new(
                plot.left() + (ratio_float(time) / self.owner_frames as f64) as f32 * plot.width(),
                plot.bottom() - ((value - minimum) / (maximum - minimum)) as f32 * plot.height(),
            )
        };
        let purple = Color32::from_rgb(185, 147, 250);
        let grid = Color32::from_gray(70);
        let painter = ui.painter_at(rect);
        for value in [minimum, 0.0, maximum] {
            let y = point(ExactRatio::ZERO, value).y;
            painter.line_segment(
                [Pos2::new(plot.left(), y), Pos2::new(plot.right(), y)],
                Stroke::new(1.0, grid),
            );
            painter.text(
                Pos2::new(rect.left(), y),
                egui::Align2::LEFT_CENTER,
                format!("{:.0} dB", value / 1000.0),
                egui::FontId::proportional(10.0),
                Color32::GRAY,
            );
        }
        painter.text(
            Pos2::new(plot.left(), rect.bottom()),
            egui::Align2::LEFT_BOTTOM,
            "0 f",
            egui::FontId::proportional(10.0),
            Color32::GRAY,
        );
        painter.text(
            Pos2::new(plot.right(), rect.bottom()),
            egui::Align2::RIGHT_BOTTOM,
            format!("{} owner frames", self.owner_frames),
            egui::FontId::proportional(10.0),
            Color32::GRAY,
        );
        let axis_font = egui::FontId::proportional(10.0);
        let end_width = painter
            .layout_no_wrap(
                format!("{} owner frames", self.owner_frames),
                axis_font.clone(),
                Color32::GRAY,
            )
            .size()
            .x;
        for index in 1..=3 {
            let Ok(time) = ExactRatio::new(i128::from(self.owner_frames) * index, 4) else {
                continue;
            };
            let x = point(time, 0.0).x;
            painter.line_segment(
                [Pos2::new(x, plot.top()), Pos2::new(x, plot.bottom())],
                Stroke::new(1.0, Color32::from_gray(48)),
            );
            let label = format!("{} f", format_frames(time));
            let width = painter
                .layout_no_wrap(label.clone(), axis_font.clone(), Color32::GRAY)
                .size()
                .x;
            if width + 8.0 < plot.width() / 4.0 && x + width / 2.0 + 8.0 < plot.right() - end_width
            {
                painter.text(
                    Pos2::new(x, rect.bottom()),
                    egui::Align2::CENTER_BOTTOM,
                    label,
                    axis_font.clone(),
                    Color32::GRAY,
                );
            }
        }
        let mut samples = Vec::with_capacity(193);
        for index in 0..=192i128 {
            let time = i128::from(self.owner_frames)
                .checked_mul(index)
                .and_then(|value| ExactRatio::new(value, 192).ok());
            if let Some(time) = time
                && let Ok(value) = envelope.evaluate(time)
            {
                samples.push((time, ratio_float(value)));
            }
        }
        let visible = |time: ExactRatio| {
            !time.compare_integer(0).is_lt() && !time.compare_integer(self.owner_frames).is_gt()
        };
        let draw_vertical = |time: ExactRatio, from: f64, to: f64| {
            if visible(time) {
                painter.line_segment(
                    [point(time, from), point(time, to)],
                    Stroke::new(1.5, purple),
                );
            }
        };
        draw_vertical(
            envelope.range().start(),
            0.0,
            f64::from(envelope.initial().millidecibels()),
        );
        let mut start = envelope.range().start();
        let mut from = envelope.initial();
        for segment in envelope.segments() {
            let Ok(range) = GainRange::new(start, segment.end()) else {
                continue;
            };
            let mut points = Vec::new();
            if visible(start) {
                points.push(point(start, f64::from(from.millidecibels())));
            }
            for &(time, value) in &samples {
                if time != start && range.contains(time) {
                    points.push(point(time, value));
                }
            }
            let end_value = if matches!(segment.curve(), GainCurve::Step) {
                from
            } else {
                segment.value()
            };
            if visible(segment.end()) {
                points.push(point(segment.end(), f64::from(end_value.millidecibels())));
            }
            if points.len() > 1 {
                painter.add(egui::Shape::line(points, Stroke::new(1.5, purple)));
            }
            let following = if segment.end() == envelope.range().end() {
                0.0
            } else {
                f64::from(segment.value().millidecibels())
            };
            draw_vertical(
                segment.end(),
                f64::from(end_value.millidecibels()),
                following,
            );
            start = segment.end();
            from = segment.value();
        }
        let mut selected = None;
        let clean = !self.key_pending(edit);
        let mut hidden = 0usize;
        for index in 0..=envelope.segments().len() {
            let Ok((time, value, _)) = edit.key(self.envelope, index) else {
                continue;
            };
            if !visible(time) {
                hidden += 1;
                continue;
            }
            let center = point(time, f64::from(value.millidecibels()));
            let response = ui
                .add_enabled_ui(clean, |ui| {
                    ui.spacing_mut().button_padding = Vec2::new(3.0, 1.0);
                    ui.put(
                        Rect::from_center_size(center, Vec2::new(28.0, 18.0)),
                        egui::Button::new(format!("K{index}"))
                            .small()
                            .wrap_mode(egui::TextWrapMode::Extend)
                            .selected(!self.adding_key && index == self.key),
                    )
                })
                .inner;
            if response
                .on_hover_text(format!(
                    "Select key {index}: {} owner frames, {} dB",
                    format_frames(time),
                    format_db(value)
                ))
                .reveal_on_focus()
                .clicked()
            {
                selected = Some(index);
            }
        }
        if let Some(index) = selected {
            self.key = index;
            self.load_key(edit);
        }
        if hidden > 0 {
            ui.small(format!(
                "{hidden} keys outside the current owner allocation remain retained."
            ));
        }
    }
}

pub(super) fn plot_rect(rect: Rect) -> Rect {
    Rect::from_min_max(
        rect.min + Vec2::new(47.0, 8.0),
        rect.max - Vec2::new(12.0, 20.0),
    )
}

fn field_id(name: &str) -> Id {
    Id::new(("deadpan-gain-field", name))
}

fn field(ui: &mut egui::Ui, label: &str, name: &str, text: &mut String, width: f32) {
    let label = ui.label(label);
    let response = ui
        .add_sized(
            [width, 22.0],
            egui::TextEdit::singleline(text).id(field_id(name)),
        )
        .labelled_by(label.id);
    if response.gained_focus() {
        ui.scroll_to_rect_animation(
            label.rect.union(response.rect),
            None,
            egui::style::ScrollAnimation::none(),
        );
    }
}

/// Reveal a newly focused control without fighting later pointer scrolling.
pub(super) trait FocusReveal {
    fn reveal_on_focus(self) -> Self;
}

impl FocusReveal for egui::Response {
    fn reveal_on_focus(self) -> Self {
        if self.gained_focus() {
            self.scroll_to_me_animation(None, egui::style::ScrollAnimation::none());
        }
        self
    }
}

fn parsed_range(start: &str, end: &str) -> Result<GainRange, String> {
    GainRange::new(parse_frames(start)?, parse_frames(end)?).map_err(|error| error.to_string())
}

fn default_range(owner_frames: i64) -> GainRange {
    // A positive exact default even when an empty owner retains hidden intent.
    GainRange::new(ExactRatio::ZERO, ExactRatio::integer(owner_frames.max(1)))
        .expect("positive owner range")
}

fn ratio_float(value: ExactRatio) -> f64 {
    value.numerator() as f64 / value.denominator() as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadpan_core::{AudioTreatments, ClipGain};

    fn fixture() -> GainEdit {
        let mut edit = GainEdit::new(AudioTreatments::from_clip_gain(ClipGain::default()));
        edit.add_envelope(
            GainEnvelope::new(
                GainClock::OwnerOutput,
                default_range(100),
                GainDb::UNITY,
                vec![
                    GainSegment::new(
                        ExactRatio::integer(100),
                        GainDb::new(3000).unwrap(),
                        GainCurve::Cubic {
                            control1: GainDb::new(-6000).unwrap(),
                            control2: GainDb::new(9000).unwrap(),
                        },
                    )
                    .unwrap(),
                ],
            )
            .unwrap(),
        )
        .unwrap();
        edit.add_mute_range(default_range(8)).unwrap();
        edit
    }

    #[test]
    fn opening_and_equivalent_text_preserve_the_complete_entry_recipe() {
        let empty = GainEdit::new(AudioTreatments::default());
        assert!(Controls::new(&empty, 8).ready(&empty));
        assert!(empty.recipe().is_empty());
        let edit = fixture();
        let before = edit.clone();
        let mut controls = Controls::new(&edit, 8);
        assert!(controls.ready(&edit));
        controls.trim = "+0.000".into();
        controls.range_end = "200/2".into();
        assert!(controls.ready(&edit));
        assert_eq!(edit, before);
    }

    #[test]
    fn unapplied_or_invalid_fields_and_new_items_block_parent_apply() {
        let edit = fixture();
        let mut controls = Controls::new(&edit, 8);
        controls.trim = "3".into();
        assert!(!controls.ready(&edit));
        controls.trim = "invalid".into();
        assert!(!controls.ready(&edit));
        controls.trim = "0".into();
        controls.range_end = "10".into();
        assert!(!controls.ready(&edit));
        controls.load_envelope(&edit);
        controls.key_value = "1".into();
        assert!(!controls.ready(&edit));
        controls.load_key(&edit);
        controls.mute_end = "0".into();
        assert!(!controls.ready(&edit));
        controls.load_mute(&edit);
        controls.adding_mute = true;
        assert!(!controls.ready(&edit));
        controls.adding_mute = false;
        controls.begin_key(&edit);
        assert!(!controls.ready(&edit));
    }

    #[test]
    fn failed_key_update_is_atomic_and_keeps_visible_pending_text() {
        let mut edit = fixture();
        let before = edit.clone();
        let mut controls = Controls::new(&edit, 8);
        controls.key = 1;
        controls.load_key(&edit);
        assert_eq!(controls.control_one, "-6");
        assert_eq!(controls.control_two, "9");
        controls.key_time = "0".into();
        controls.apply_key(&mut edit);
        assert_eq!(edit, before);
        assert!(controls.error.is_some());
        assert_eq!(controls.key_time, "0");
        assert!(!controls.ready(&edit));
        controls.key_time = "120".into();
        controls.apply_key(&mut edit);
        assert!(controls.ready(&edit));
        assert_eq!(controls.range_end, "120");
        assert_eq!(edit.key(0, 1).unwrap().2, before.key(0, 1).unwrap().2);
    }

    #[test]
    fn updates_do_not_discard_unrelated_pending_rows() {
        let mut edit = fixture();
        let mut controls = Controls::new(&edit, 8);
        controls.trim = "3".into();
        controls.range_end = "150".into();
        controls.key = 1;
        controls.load_key(&edit);
        controls.key_value = "6".into();
        controls.apply_key(&mut edit);
        assert_eq!(controls.trim, "3");
        assert_eq!(controls.range_end, "150");
        assert!(!controls.ready(&edit));
    }
}
