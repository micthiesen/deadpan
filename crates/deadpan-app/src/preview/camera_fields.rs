//! Keyboard-editable numeric controls for Camera's current pose.
//!
//! Buffers are local presentation state. Call [`CameraFields::sync`] only when
//! an external Camera operation changes the pose; direct edits keep untouched
//! exact ratio components intact.

use deadpan_core::{ExactRatio, FRAMING_NUMERIC_SCALE, FramingPose};
use eframe::egui::{self, Id, Key, TextEdit};

use crate::navigation::camera::Direction;
use deadpan_core::framing_zoom::ZoomCurve;

const FIELD_CHAR_LIMIT: usize = 64;
const FIELD_WIDTH: f32 = 152.0;

#[derive(Clone, Copy)]
enum Component {
    CenterX,
    CenterY,
    Scale,
    Arrival,
}

impl Component {
    const ALL: [Self; 3] = [Self::CenterX, Self::CenterY, Self::Scale];

    const fn label(self) -> &'static str {
        match self {
            Self::CenterX => "Center X (% canvas)",
            Self::CenterY => "Center Y (% canvas)",
            Self::Scale => "Scale (%)",
            Self::Arrival => "Arrival scale (%)",
        }
    }

    const fn accessible_label(self) -> &'static str {
        match self {
            Self::CenterX => "Framing center X, percent of canvas",
            Self::CenterY => "Framing center Y, percent of canvas",
            Self::Scale => "Framing scale, percent of input size",
            Self::Arrival => "Follow arrival scale, percent of input size",
        }
    }

    fn value(self, pose: FramingPose) -> ExactRatio {
        match self {
            Self::CenterX => pose.center_x,
            Self::CenterY => pose.center_y,
            Self::Scale | Self::Arrival => pose.scale,
        }
    }

    fn replace(self, pose: FramingPose, value: ExactRatio) -> FramingPose {
        match self {
            Self::CenterX => FramingPose {
                center_x: value,
                ..pose
            },
            Self::CenterY => FramingPose {
                center_y: value,
                ..pose
            },
            Self::Scale => FramingPose {
                scale: value,
                ..pose
            },
            Self::Arrival => pose,
        }
    }

    fn percent_bounds(self) -> (f64, f64) {
        match self {
            Self::CenterX | Self::CenterY => (-1600.0, 1700.0),
            Self::Scale | Self::Arrival => (100.0 / 64.0, 6400.0),
        }
    }
}

struct Field {
    component: Component,
    id: Id,
    text: String,
    dirty: bool,
    composing: bool,
    composition_base: Option<(String, bool)>,
}

impl Field {
    fn new(component: Component, pose: FramingPose) -> Self {
        let name = match component {
            Component::CenterX => "center-x",
            Component::CenterY => "center-y",
            Component::Scale => "scale",
            Component::Arrival => "arrival",
        };
        Self {
            component,
            id: Id::new(("deadpan-camera-field", name)),
            text: format_percent(component.value(pose)),
            dirty: false,
            composing: false,
            composition_base: None,
        }
    }
}

/// Bounded pose fields, plus arrival and curve controls for an animated Follow.
pub struct CameraFields {
    pose: FramingPose,
    fields: Vec<Field>,
    ids: Vec<Id>,
    validation: Result<FramingPose, String>,
    /// A followed target supplies the center; scale and animation stay editable.
    centers_locked: bool,
    /// The exact arrival survives untouched rounded display text.
    animation: Option<(ExactRatio, ZoomCurve)>,
    curve_changed: bool,
}

impl CameraFields {
    pub fn new(pose: FramingPose) -> Self {
        let validation = pose
            .validate()
            .map(|()| pose)
            .map_err(|error| error.to_string());
        let fields: Vec<_> = Component::ALL
            .map(|component| Field::new(component, pose))
            .into();
        let ids = fields.iter().map(|field| field.id).collect();
        Self {
            pose,
            fields,
            ids,
            validation,
            centers_locked: false,
            animation: None,
            curve_changed: false,
        }
    }

    /// Adopt an accepted Follow edit. This is called with external Camera
    /// actions, never while the user is still editing an invalid text buffer.
    pub fn sync_animation(&mut self, animation: Option<(ExactRatio, ZoomCurve)>) {
        let curve_id = self
            .curve_id()
            .unwrap_or_else(|| Id::new(("deadpan-camera-field", "follow-curve")));
        self.fields.truncate(3);
        self.ids.truncate(3);
        self.animation = animation;
        self.curve_changed = false;
        if let Some((arrival, _)) = animation {
            let field = Field::new(
                Component::Arrival,
                FramingPose {
                    scale: arrival,
                    ..self.pose
                },
            );
            self.ids.push(field.id);
            self.ids.push(curve_id);
            self.fields.push(field);
        }
    }

    pub fn animation_edit(&self) -> Option<(Option<ExactRatio>, ZoomCurve)> {
        let (arrival, curve) = self.animation?;
        let edited = self.fields[3].dirty;
        (edited || self.curve_changed).then_some((edited.then_some(arrival), curve))
    }

    fn curve_id(&self) -> Option<Id> {
        self.animation.map(|_| self.ids[4])
    }

    /// Lock the center fields while a target supplies the center.
    pub fn set_centers_locked(&mut self, locked: bool) {
        self.centers_locked = locked;
    }

    /// The fields Tab may focus.
    pub fn focusable_ids(&self) -> &[Id] {
        if self.centers_locked {
            &self.ids[2..]
        } else {
            &self.ids
        }
    }

    /// Adopt a Camera action, resetting local text to its resulting pose.
    pub fn sync(&mut self, pose: FramingPose) {
        self.pose = pose;
        for field in &mut self.fields {
            let value = if matches!(field.component, Component::Arrival) {
                self.animation.expect("arrival field has animation").0
            } else {
                field.component.value(pose)
            };
            field.text = format_percent(value);
            field.dirty = false;
            field.composing = false;
            field.composition_base = None;
        }
        self.validation = pose
            .validate()
            .map(|()| pose)
            .map_err(|error| error.to_string());
    }

    /// Accept a valid local preview without rewriting a field still being
    /// typed. In particular, `150.` must retain its decimal separator for the
    /// next native text event. Other fields may show a rebased live scale.
    pub fn adopt(
        &mut self,
        pose: FramingPose,
        animation: Option<(ExactRatio, ZoomCurve)>,
        context: &egui::Context,
    ) {
        if self.animation.is_some() != animation.is_some() {
            self.sync_animation(animation);
        }
        self.pose = pose;
        self.animation = animation;
        self.curve_changed = false;
        for field in &mut self.fields {
            if !context.memory(|memory| memory.has_focus(field.id)) {
                let value = if matches!(field.component, Component::Arrival) {
                    animation.expect("arrival has animation").0
                } else {
                    field.component.value(pose)
                };
                field.text = format_percent(value);
            }
            field.dirty = false;
        }
        self.validation = Ok(pose);
    }

    /// Draw all fields, then validate the final buffers after egui has applied
    /// this frame's text, IME, and focus events. This lets a following Apply
    /// button observe same-frame edits instead of an older parsed value.
    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<Result<FramingPose, String>> {
        let context = ui.ctx().clone();
        let focus_before = self
            .fields
            .iter()
            .map(|field| context.memory(|memory| memory.has_focus(field.id)))
            .collect::<Vec<_>>();
        let enter_pressed = ui.input(|input| input.key_pressed(Key::Enter));
        let (ime_update, ime_started) = ui.input(|input| {
            input
                .events
                .iter()
                .fold((None, false), |(state, started), event| match event {
                    egui::Event::Ime(egui::ImeEvent::Preedit { text, .. }) => (
                        Some(ImeUpdate::Preedit {
                            active: !text.is_empty(),
                        }),
                        started || !text.is_empty(),
                    ),
                    egui::Event::Ime(egui::ImeEvent::Commit(_)) => {
                        (Some(ImeUpdate::Commit), started)
                    }
                    _ => (state, started),
                })
        });
        let pre_draw_state = self
            .fields
            .iter()
            .map(|field| (field.text.clone(), field.dirty))
            .collect::<Vec<_>>();

        let mut changed = false;
        let mut lost_focus = false;
        let mut focus_after = vec![false; self.fields.len()];
        ui.vertical(|ui| {
            if self.centers_locked {
                ui.label("Target center (% canvas)");
                ui.label(format!("X {}% · Y {}%", format_percent(self.pose.center_x), format_percent(self.pose.center_y)));
            }
            for (index, field) in self.fields.iter_mut().enumerate() {
                if self.centers_locked && matches!(field.component, Component::CenterX | Component::CenterY) {
                    continue;
                }
                ui.label(field.component.label());
                let width = ui.available_width().min(FIELD_WIDTH);
                let response = ui.add(
                    TextEdit::singleline(&mut field.text)
                        .id(field.id)
                        .desired_width(width)
                        .horizontal_align(egui::Align::RIGHT)
                        .char_limit(FIELD_CHAR_LIMIT),
                );
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::TextEdit,
                        true,
                        field.component.accessible_label(),
                    )
                });
                let response = response.on_hover_text(field.component.accessible_label());
                if response.changed() {
                    field.dirty = true;
                    changed = true;
                }
                lost_focus |= response.lost_focus();
                focus_after[index] = response.has_focus();
            }
            if let Some((_, curve)) = &mut self.animation {
                ui.label("Arrival curve · Enter / Space");
                let name = match curve {
                    ZoomCurve::Step => "Step",
                    ZoomCurve::Linear => "Linear",
                    ZoomCurve::Smoothstep => "Smoothstep",
                };
                let response = ui.push_id("deadpan-camera-follow-curve", |ui| {
                    ui.add_sized([ui.available_width().min(FIELD_WIDTH), 24.0], egui::Button::new(name))
                }).inner;
                self.ids[4] = response.id;
                // The button's stable ID participates in Camera's focus cycle.
                let response = response.on_hover_text("Follow arrival curve. Enter or Space switches between Linear and Smoothstep; Up/Down also switches.");
                if response.clicked() {
                    // egui's mouse activation does not retain button focus.
                    // Subsequent arrows and Space belong to this control.
                    if !ui.memory(|memory| memory.has_focus(response.id)) {
                        response.request_focus();
                    }
                    *curve = next_curve(*curve);
                    self.curve_changed = true;
                    changed = true;
                }
                if ui.memory(|memory| memory.has_focus(response.id)) {
                    ui.memory_mut(|memory| memory.set_focus_lock_filter(response.id,
                        egui::EventFilter { vertical_arrows: true, ..Default::default() }));
                }
            }
        });

        let focus_index = if ime_update.is_some() {
            focus_before
                .iter()
                .position(|focused| *focused)
                .or_else(|| focus_after.iter().position(|focused| *focused))
        } else {
            focus_after
                .iter()
                .position(|focused| *focused)
                .or_else(|| focus_before.iter().position(|focused| *focused))
        };
        let ime_for_field = ime_update.is_some() && focus_index.is_some();
        if let (Some(index), Some(update)) = (focus_index, ime_update) {
            let field = &mut self.fields[index];
            if ime_started && field.composition_base.is_none() {
                field.composition_base = Some(pre_draw_state[index].clone());
            }
            match update {
                ImeUpdate::Preedit { active: true } => {
                    field.composing = true;
                }
                ImeUpdate::Preedit { active: false } => {
                    field.composing = false;
                    if let Some((text, dirty)) = field.composition_base.take() {
                        field.text = text;
                        field.dirty = dirty;
                    }
                }
                ImeUpdate::Commit => {
                    if let Some((text, dirty)) = field.composition_base.take() {
                        if field.text != text {
                            field.dirty = true;
                        } else {
                            field.dirty = dirty;
                        }
                    }
                    field.composing = false;
                }
            }
        }

        if self.fields.iter().any(|field| field.composing) {
            self.validation = Err("Finish text composition before applying framing.".into());
        } else {
            self.validation = self.build_pose().and_then(|pose| {
                if let Some(field) = self.fields.get(3)
                    && field.dirty
                {
                    let arrival = parse_percent(&field.text, Component::Arrival)?;
                    self.animation.as_mut().expect("arrival has animation").0 = arrival;
                }
                Ok(pose)
            });
            if let Ok(pose) = self.validation {
                self.pose = pose;
            }
        }
        let error = self.error_message().unwrap_or("");
        let color = if error.is_empty() {
            egui::Color32::TRANSPARENT
        } else {
            egui::Color32::LIGHT_RED
        };
        let error_height = ui.text_style_height(&egui::TextStyle::Body);
        ui.add_sized(
            egui::vec2(ui.available_width(), error_height),
            egui::Label::new(egui::RichText::new(error).color(color)).truncate(),
        );

        let field_had_focus = focus_before.iter().any(|focused| *focused);
        (changed || lost_focus || (enter_pressed && field_had_focus) || ime_for_field)
            .then(|| self.validation.clone())
    }

    /// Whether the newest text is valid and no IME composition is in flight.
    pub fn is_valid(&self) -> bool {
        self.validation.is_ok()
    }

    pub fn error_message(&self) -> Option<&str> {
        self.validation.as_ref().err().map(String::as_str)
    }

    /// IDs are stable for this Camera component and can be captured before
    /// drawing when keyboard routing precedes widget processing.
    #[cfg(test)]
    pub fn ids(&self) -> &[Id] {
        &self.ids
    }

    pub fn owns_focus(&self, context: &egui::Context) -> bool {
        self.ids
            .iter()
            .any(|id| context.memory(|memory| memory.has_focus(*id)))
    }

    /// Apply a one-percentage-point field step after `show` has consumed this
    /// frame's text and IME events. Returns `None` when no field owns focus.
    pub fn adjust_focused(
        &mut self,
        context: &egui::Context,
        direction: Direction,
        count: u32,
    ) -> Option<Result<FramingPose, String>> {
        if self
            .curve_id()
            .is_some_and(|id| context.memory(|memory| memory.has_focus(id)))
            && let Some(animation) = self.animation.as_mut()
        {
            // egui computes directional navigation at frame entry, before
            // Camera consumes the arrow. Cancel that pending move too, which
            // covers the first arrow after this button just gained focus.
            context.memory_mut(|memory| memory.move_focus(egui::FocusDirection::None));
            if self.validation.is_err() {
                return Some(self.validation.clone());
            }
            if matches!(direction, Direction::Up | Direction::Down) && count > 0 {
                animation.1 = next_curve(animation.1);
                self.curve_changed = true;
                return Some(Ok(self.pose));
            }
            return Some(Err("Use Up or Down to change the arrival curve.".into()));
        }
        let index = self
            .fields
            .iter()
            .position(|field| context.memory(|memory| memory.has_focus(field.id)))?;
        if let Err(error) = &self.validation {
            return Some(Err(error.clone()));
        }
        if self.fields[index].composing {
            return Some(Err(
                "Finish text composition before adjusting framing.".into()
            ));
        }
        if count == 0 {
            return Some(Err("Camera field step count must be positive.".into()));
        }
        if !matches!(direction, Direction::Up | Direction::Down) {
            return Some(Err(
                "Use Up or Down to adjust a focused Camera field.".into()
            ));
        }

        let component = self.fields[index].component;
        let value = if matches!(component, Component::Arrival) {
            self.animation.expect("arrival has animation").0
        } else {
            component.value(self.pose)
        };
        let delta = ExactRatio::new(i128::from(count), 100).and_then(|delta| {
            if direction == Direction::Up {
                Ok(delta)
            } else {
                ExactRatio::ZERO.checked_sub(delta)
            }
        });
        let candidate = delta
            .and_then(|delta| value.checked_add(delta))
            .and_then(quantize_q32);
        match candidate {
            Ok(value) => {
                if matches!(component, Component::Arrival) {
                    if let Err(error) = (FramingPose {
                        scale: value,
                        ..self.pose
                    })
                    .validate()
                    {
                        return Some(Err(error.to_string()));
                    }
                    self.animation.as_mut().expect("arrival has animation").0 = value;
                    self.fields[index].text = format_percent(value);
                    self.fields[index].dirty = true;
                    return Some(Ok(self.pose));
                }
                let pose = component.replace(self.pose, value);
                if let Err(error) = pose.validate() {
                    return Some(Err(error.to_string()));
                }
                self.pose = pose;
                self.fields[index].text = format_percent(component.value(pose));
                self.fields[index].dirty = true;
                self.validation = Ok(pose);
                Some(Ok(pose))
            }
            Err(error) => Some(Err(error.to_string())),
        }
    }

    fn build_pose(&self) -> Result<FramingPose, String> {
        let mut pose = self.pose;
        for field in &self.fields {
            if field.dirty && !matches!(field.component, Component::Arrival) {
                let value = parse_percent(&field.text, field.component)?;
                pose = field.component.replace(pose, value);
            }
        }
        pose.validate().map_err(|error| error.to_string())?;
        Ok(pose)
    }
}

fn next_curve(curve: ZoomCurve) -> ZoomCurve {
    match curve {
        ZoomCurve::Linear => ZoomCurve::Smoothstep,
        ZoomCurve::Step | ZoomCurve::Smoothstep => ZoomCurve::Linear,
    }
}

#[derive(Clone, Copy)]
enum ImeUpdate {
    Preedit { active: bool },
    Commit,
}

fn parse_percent(text: &str, component: Component) -> Result<ExactRatio, String> {
    let trimmed = text.trim();
    let value_text = trimmed.strip_suffix('%').unwrap_or(trimmed).trim();
    if value_text.is_empty() {
        return Err("Enter a finite percentage.".into());
    }
    let percent = value_text
        .parse::<f64>()
        .map_err(|_| "Enter a finite percentage.".to_owned())?;
    if !percent.is_finite() {
        return Err("Enter a finite percentage.".into());
    }
    let (minimum, maximum) = component.percent_bounds();
    if percent < minimum || percent > maximum {
        return Err(format!(
            "{} must be between {minimum}% and {maximum}%.",
            component.label()
        ));
    }
    let value = percent / 100.0;
    let scale = FRAMING_NUMERIC_SCALE as f64;
    let grid = (value * scale).round_ties_even();
    if !grid.is_finite() || grid < i64::MIN as f64 || grid > i64::MAX as f64 {
        return Err("Percentage exceeds Camera's numeric range.".into());
    }
    ExactRatio::new(grid as i128, i128::from(FRAMING_NUMERIC_SCALE))
        .map_err(|error| error.to_string())
}

fn quantize_q32(value: ExactRatio) -> Result<ExactRatio, deadpan_core::TimeError> {
    let scaled = value.checked_mul(ExactRatio::integer(
        i64::try_from(FRAMING_NUMERIC_SCALE).map_err(|_| deadpan_core::TimeError::Overflow)?,
    ))?;
    ExactRatio::new(scaled.round_even()?, i128::from(FRAMING_NUMERIC_SCALE))
}

fn format_percent(value: ExactRatio) -> String {
    let percent = (value.numerator() as f64 / value.denominator() as f64) * 100.0;
    let mut text = format!("{percent:.6}");
    while text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    if text == "-0" {
        text = "0".into();
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_ui(context: &egui::Context, input: egui::RawInput, draw: impl FnMut(&mut egui::Ui)) {
        context.enable_accesskit();
        let mut output = context.run_ui(input, draw);
        let tree = output.platform_output.accesskit_update.as_ref().unwrap();
        assert!(
            tree.nodes.iter().any(|(id, _)| *id == tree.focus),
            "Accessibility focus must name a node emitted in this frame"
        );
        output.textures_delta.clear();
    }

    fn key_event(key: Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    fn click_events(pos: egui::Pos2) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
        ]
    }

    fn exact_pose() -> FramingPose {
        FramingPose::new(
            ExactRatio::new(1, 3).unwrap(),
            ExactRatio::new(2, 5).unwrap(),
            ExactRatio::new(7, 4).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn follow_arrival_field_keeps_untouched_exact_value_and_keyboard_curve_focus() {
        let context = egui::Context::default();
        let original = exact_pose();
        let arrival = ExactRatio::new(11, 3).unwrap();
        let mut fields = CameraFields::new(original);
        fields.set_centers_locked(true);
        fields.sync_animation(Some((arrival, ZoomCurve::Step)));
        run_ui(&context, egui::RawInput::default(), |ui| {
            fields.show(ui);
        });
        assert_eq!(fields.animation, Some((arrival, ZoomCurve::Step)));
        assert_eq!(fields.animation_edit(), None);
        assert_eq!(fields.focusable_ids().len(), 3);
        let curve_id = fields.curve_id().unwrap();
        context.memory_mut(|memory| memory.request_focus(curve_id));
        assert!(fields.owns_focus(&context));
        assert_eq!(
            fields.adjust_focused(&context, Direction::Down, 1),
            Some(Ok(original))
        );
        assert_eq!(fields.animation_edit(), Some((None, ZoomCurve::Linear)));
        fields.sync_animation(Some((arrival, ZoomCurve::Linear)));
        assert_eq!(fields.curve_id(), Some(curve_id));
        run_ui(
            &context,
            egui::RawInput {
                events: vec![key_event(Key::Enter)],
                ..Default::default()
            },
            |ui| {
                fields.show(ui);
            },
        );
        assert_eq!(fields.animation_edit(), Some((None, ZoomCurve::Smoothstep)));
    }

    #[test]
    fn followed_centers_have_plain_accessible_summary_and_keep_exact_backing_values() {
        let context = egui::Context::default();
        context.enable_accesskit();
        let original = exact_pose();
        let mut fields = CameraFields::new(original);
        let ids = fields.ids().to_vec();
        fields.set_centers_locked(true);
        fields.sync_animation(Some((ExactRatio::new(11, 3).unwrap(), ZoomCurve::Linear)));
        let mut output = context.run_ui(egui::RawInput::default(), |ui| {
            fields.show(ui);
        });
        let tree = output.platform_output.accesskit_update.as_ref().unwrap();
        assert!(
            tree.nodes
                .iter()
                .any(|(_, node)| node.value() == Some("Target center (% canvas)"))
        );
        assert!(
            tree.nodes
                .iter()
                .any(|(_, node)| node.value() == Some("X 33.333333% · Y 40%"))
        );
        assert!(
            !tree
                .nodes
                .iter()
                .any(|(_, node)| node.role() == egui::accesskit::Role::TextInput
                    && node.label().is_some_and(|label| label.contains("center")))
        );
        assert_eq!(fields.pose, original);
        assert_eq!(&fields.ids()[..3], ids.as_slice());
        assert_eq!(fields.focusable_ids().len(), 3);
        output.textures_delta.clear();
    }

    #[test]
    fn clicking_follow_curve_retains_focus_for_subsequent_arrow_and_space() {
        let context = egui::Context::default();
        let original = exact_pose();
        let mut fields = CameraFields::new(original);
        fields.sync_animation(Some((ExactRatio::new(11, 3).unwrap(), ZoomCurve::Linear)));
        run_ui(&context, egui::RawInput::default(), |ui| {
            fields.show(ui);
            let _ = ui.button("Apply");
        });
        let id = fields.curve_id().unwrap();
        let rect = context
            .read_response(id)
            .expect("painted curve control")
            .rect;
        run_ui(
            &context,
            egui::RawInput {
                events: click_events(rect.center()),
                ..Default::default()
            },
            |ui| {
                fields.show(ui);
                let _ = ui.button("Apply");
            },
        );
        assert!(context.memory(|memory| memory.has_focus(id)));
        assert!(fields.owns_focus(&context));
        assert_eq!(fields.animation_edit(), Some((None, ZoomCurve::Smoothstep)));
        fields.adopt(original, fields.animation, &context);
        let mut adjustment = None;
        run_ui(
            &context,
            egui::RawInput {
                events: vec![key_event(Key::ArrowDown)],
                ..Default::default()
            },
            |ui| {
                // Production routing consumes the arrow before drawing, then
                // applies the deferred field adjustment after native widgets.
                ui.input_mut(|input| {
                    input.consume_key(egui::Modifiers::NONE, Key::ArrowDown);
                });
                fields.show(ui);
                adjustment = fields.adjust_focused(ui.ctx(), Direction::Down, 1);
                let _ = ui.button("Apply");
            },
        );
        assert_eq!(adjustment, Some(Ok(original)));
        assert!(
            context.memory(|memory| memory.has_focus(id)),
            "the consumed curve arrow must not transfer focus to Apply"
        );
        assert_eq!(fields.animation_edit(), Some((None, ZoomCurve::Linear)));
        fields.adopt(original, fields.animation, &context);
        run_ui(
            &context,
            egui::RawInput {
                events: vec![key_event(Key::Space)],
                ..Default::default()
            },
            |ui| {
                fields.show(ui);
                let _ = ui.button("Apply");
            },
        );
        assert!(context.memory(|memory| memory.has_focus(id)));
        assert_eq!(fields.animation_edit(), Some((None, ZoomCurve::Smoothstep)));
    }

    #[test]
    fn follow_arrival_same_batch_text_validates_without_changing_uniform_scale() {
        let context = egui::Context::default();
        let original = exact_pose();
        let mut fields = CameraFields::new(original);
        fields.sync_animation(Some((ExactRatio::new(11, 3).unwrap(), ZoomCurve::Linear)));
        run_ui(&context, egui::RawInput::default(), |ui| {
            fields.show(ui);
        });
        fields.fields[3].text.clear();
        context.memory_mut(|memory| memory.request_focus(fields.ids()[3]));
        let mut outcome = None;
        run_ui(
            &context,
            egui::RawInput {
                events: vec![egui::Event::Text("312.5".into()), key_event(Key::Enter)],
                ..Default::default()
            },
            |ui| {
                outcome = fields.show(ui);
            },
        );
        assert_eq!(outcome, Some(Ok(original)));
        assert_eq!(
            fields.animation_edit(),
            Some((Some(ExactRatio::new(25, 8).unwrap()), ZoomCurve::Linear))
        );
        fields.fields[3].text = "NaN".into();
        run_ui(&context, egui::RawInput::default(), |ui| {
            fields.show(ui);
        });
        assert!(!fields.is_valid());
        assert_eq!(fields.pose, original);
        // Enter finished the earlier native field. Re-focus the invalid
        // arrival buffer to exercise its arrow refusal explicitly.
        context.memory_mut(|memory| memory.request_focus(fields.ids()[3]));
        assert!(matches!(
            fields.adjust_focused(&context, Direction::Up, 1),
            Some(Err(_))
        ));
    }

    #[test]
    fn follow_arrival_composition_blocks_apply_and_cancellation_preserves_exact_value() {
        let context = egui::Context::default();
        let arrival = ExactRatio::new(11, 3).unwrap();
        let mut fields = CameraFields::new(exact_pose());
        fields.sync_animation(Some((arrival, ZoomCurve::Smoothstep)));
        run_ui(&context, egui::RawInput::default(), |ui| {
            fields.show(ui);
        });
        context.memory_mut(|memory| memory.request_focus(fields.ids()[3]));
        run_ui(
            &context,
            egui::RawInput {
                events: vec![egui::Event::Ime(egui::ImeEvent::Preedit {
                    text: "625".into(),
                    active_range_chars: None,
                })],
                ..Default::default()
            },
            |ui| {
                fields.show(ui);
            },
        );
        assert!(!fields.is_valid());
        run_ui(
            &context,
            egui::RawInput {
                events: vec![egui::Event::Ime(egui::ImeEvent::Preedit {
                    text: String::new(),
                    active_range_chars: None,
                })],
                ..Default::default()
            },
            |ui| {
                fields.show(ui);
            },
        );
        assert!(fields.is_valid());
        assert_eq!(fields.animation, Some((arrival, ZoomCurve::Smoothstep)));
        assert_eq!(fields.animation_edit(), None);
    }

    #[test]
    fn accepting_local_previews_preserves_incremental_native_decimal_typing() {
        for index in [2, 3] {
            let context = egui::Context::default();
            let mut fields = CameraFields::new(exact_pose());
            fields.sync_animation(Some((ExactRatio::new(27, 10).unwrap(), ZoomCurve::Linear)));
            run_ui(&context, egui::RawInput::default(), |ui| {
                fields.show(ui);
            });
            fields.fields[index].text.clear();
            context.memory_mut(|memory| memory.request_focus(fields.ids()[index]));
            for text in ["150.", "25"] {
                let mut result = None;
                run_ui(
                    &context,
                    egui::RawInput {
                        events: vec![egui::Event::Text(text.into())],
                        ..Default::default()
                    },
                    |ui| {
                        result = fields.show(ui);
                    },
                );
                let pose = result
                    .expect("text edit result")
                    .expect("valid numeric preview");
                fields.adopt(pose, fields.animation, &context);
                assert!(fields.owns_focus(&context));
            }
            assert_eq!(fields.fields[index].text, "150.25");
            let expected = quantize_q32(ExactRatio::new(601, 400).unwrap()).unwrap();
            if index == 2 {
                assert_eq!(fields.pose.scale, expected);
            } else {
                assert_eq!(fields.animation.unwrap().0, expected);
            }
            assert!(
                fields.animation_edit().is_none(),
                "accepted dirty buffers must not replay an older scale change"
            );
        }
    }

    #[test]
    fn same_frame_text_and_enter_validate_new_value_without_rounding_untouched_fields() {
        let context = egui::Context::default();
        let original = exact_pose();
        let mut fields = CameraFields::new(original);
        let mut apply_rect = egui::Rect::NOTHING;
        run_ui(&context, egui::RawInput::default(), |ui| {
            fields.show(ui);
            apply_rect = ui.button("Apply").rect;
        });

        fields.fields[0].text.clear();
        context.memory_mut(|memory| memory.request_focus(fields.ids()[0]));
        let mut outcome = None;
        let mut allowed = false;
        let mut clicked = false;
        let mut events = vec![egui::Event::Text("63.125".into()), key_event(Key::Enter)];
        events.extend(click_events(apply_rect.center()));
        run_ui(
            &context,
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ui| {
                outcome = fields.show(ui);
                let apply = ui.button("Apply");
                clicked = apply.clicked();
                if clicked {
                    allowed = fields.is_valid();
                }
            },
        );

        assert!(clicked, "the same-batch Apply click must be observed");
        assert!(allowed, "the final valid text must be available to Apply");
        let Some(Ok(result)) = outcome else {
            panic!("same-frame text plus Enter must return the newly parsed pose");
        };
        let expected_x = quantize_q32(ExactRatio::new(101, 160).unwrap()).unwrap();
        assert_eq!(result.center_x, expected_x);
        assert_eq!(result.center_y, original.center_y);
        assert_eq!(result.scale, original.scale);
    }

    #[test]
    fn nonfinite_text_in_same_batch_as_apply_is_rejected_without_clamping() {
        let context = egui::Context::default();
        let mut fields = CameraFields::new(exact_pose());
        let mut apply_rect = egui::Rect::NOTHING;
        run_ui(&context, egui::RawInput::default(), |ui| {
            fields.show(ui);
            apply_rect = ui.button("Apply").rect;
        });

        fields.fields[2].text.clear();
        context.memory_mut(|memory| memory.request_focus(fields.ids()[2]));
        let mut outcome = None;
        let mut allowed = true;
        let mut clicked = false;
        let mut events = vec![egui::Event::Text("NaN".into())];
        events.extend(click_events(apply_rect.center()));
        run_ui(
            &context,
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ui| {
                outcome = fields.show(ui);
                let apply = ui.button("Apply");
                clicked = apply.clicked();
                if clicked {
                    allowed = fields.is_valid();
                }
            },
        );

        assert!(clicked, "the pointer activation should reach Apply");
        assert!(!allowed, "invalid same-frame text must block Apply");
        assert!(matches!(outcome, Some(Err(_))));
        assert!(fields.validation.is_err());
        assert!(parse_percent("inf", Component::CenterX).is_err());
        assert!(parse_percent("1700.01", Component::CenterX).is_err());
        assert!(parse_percent("1", Component::Scale).is_err());
    }

    #[test]
    fn focused_arrow_steps_only_the_selected_exact_component() {
        let context = egui::Context::default();
        let original = exact_pose();
        let mut fields = CameraFields::new(original);
        context.memory_mut(|memory| memory.request_focus(fields.ids()[1]));
        run_ui(&context, egui::RawInput::default(), |ui| {
            fields.show(ui);
        });
        assert!(fields.owns_focus(&context));

        let Some(Ok(updated)) = fields.adjust_focused(&context, Direction::Up, 2) else {
            panic!("a valid focused field should accept a bounded arrow step");
        };
        assert_eq!(updated.center_x, original.center_x);
        assert_eq!(
            updated.center_y,
            quantize_q32(ExactRatio::new(42, 100).unwrap()).unwrap()
        );
        assert_eq!(updated.scale, original.scale);
        assert_eq!(fields.validation, Ok(updated));
    }

    #[test]
    fn fields_fit_the_minimum_inspector_content_width() {
        let context = egui::Context::default();
        let mut fields = CameraFields::new(exact_pose());
        fields.sync_animation(Some((
            ExactRatio::new(11, 3).unwrap(),
            ZoomCurve::Smoothstep,
        )));
        fields.set_centers_locked(true);
        let mut content_rect = egui::Rect::NOTHING;
        let mut clip_rect = egui::Rect::NOTHING;
        run_ui(
            &context,
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(168.0, 400.0),
                )),
                ..Default::default()
            },
            |ui| {
                fields.show(ui);
                content_rect = ui.min_rect();
                clip_rect = ui.clip_rect();
            },
        );
        assert!(
            content_rect.right() <= clip_rect.right() + 0.5,
            "Camera fields overflow the 168-point inspector: {content_rect:?} vs {clip_rect:?}"
        );
        assert!(
            content_rect.bottom() <= clip_rect.bottom() + 0.5,
            "Follow controls overflow the compact field area vertically: {content_rect:?} vs {clip_rect:?}"
        );
    }

    #[test]
    fn active_ime_preedit_blocks_apply_until_its_commit_is_consumed() {
        let context = egui::Context::default();
        let mut fields = CameraFields::new(exact_pose());
        run_ui(&context, egui::RawInput::default(), |ui| {
            fields.show(ui);
        });
        fields.fields[0].text.clear();
        context.memory_mut(|memory| memory.request_focus(fields.ids()[0]));

        let mut outcome = None;
        run_ui(
            &context,
            egui::RawInput {
                events: vec![egui::Event::Ime(egui::ImeEvent::Preedit {
                    text: "62.5".into(),
                    active_range_chars: None,
                })],
                ..Default::default()
            },
            |ui| outcome = fields.show(ui),
        );
        assert!(matches!(outcome, Some(Err(_))));
        assert!(!fields.is_valid(), "preedit text is not an applied value");

        run_ui(
            &context,
            egui::RawInput {
                events: vec![egui::Event::Ime(egui::ImeEvent::Commit("62.5".into()))],
                ..Default::default()
            },
            |ui| outcome = fields.show(ui),
        );
        let Some(Ok(committed)) = outcome else {
            panic!("the committed IME text should parse after composition ends");
        };
        assert_eq!(
            committed.center_x,
            quantize_q32(ExactRatio::new(5, 8).unwrap()).unwrap()
        );
        assert_eq!(committed.center_y, exact_pose().center_y);
        assert_eq!(committed.scale, exact_pose().scale);
    }

    #[test]
    fn canceled_ime_preedit_restores_the_exact_precomposition_buffer() {
        let context = egui::Context::default();
        let original = exact_pose();
        let mut fields = CameraFields::new(original);
        run_ui(&context, egui::RawInput::default(), |ui| {
            fields.show(ui);
        });
        fields.fields[0].text.clear();
        context.memory_mut(|memory| memory.request_focus(fields.ids()[0]));

        run_ui(
            &context,
            egui::RawInput {
                events: vec![egui::Event::Ime(egui::ImeEvent::Preedit {
                    text: "62.5".into(),
                    active_range_chars: None,
                })],
                ..Default::default()
            },
            |ui| {
                fields.show(ui);
            },
        );
        assert!(fields.fields[0].composing);

        let mut outcome = None;
        run_ui(
            &context,
            egui::RawInput {
                events: vec![egui::Event::Ime(egui::ImeEvent::Preedit {
                    text: String::new(),
                    active_range_chars: None,
                })],
                ..Default::default()
            },
            |ui| outcome = fields.show(ui),
        );

        assert_eq!(fields.fields[0].text, "");
        assert!(!fields.fields[0].dirty);
        assert!(!fields.fields[0].composing);
        assert_eq!(fields.validation, Ok(original));
        assert!(matches!(outcome, Some(Ok(pose)) if pose == original));
    }

    #[test]
    fn sync_resets_text_and_nonfinite_or_out_of_range_values_never_parse() {
        let mut fields = CameraFields::new(exact_pose());
        fields.fields[0].text = "NaN".into();
        fields.fields[0].dirty = true;
        let updated = FramingPose::identity();
        fields.sync(updated);
        assert_eq!(fields.validation, Ok(updated));
        assert!(!fields.fields[0].dirty);
        assert!(fields.fields[0].text.starts_with("50"));
        assert!(parse_percent("-Infinity", Component::CenterY).is_err());
        assert!(parse_percent("6400.01", Component::Scale).is_err());
    }
}
