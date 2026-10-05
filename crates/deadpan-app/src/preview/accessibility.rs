//! Native accessibility support that egui's widgets do not supply on their own:
//! a valid AccessKit focus, spoken status changes, painted-pane summaries and
//! the macOS Reduce motion and Increase contrast display preferences.

use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, Stroke, accesskit};

use super::style;
use crate::navigation::Pane;

const DRAWN_PANES: &str = "deadpan-drawn-panes";
const DISPLAY: &str = "deadpan-display-preferences";
/// The system preference is cheap to read. It is re-read on frames the app
/// paints anyway (at most this often) and whenever the window regains focus,
/// which is how a user returns from System Settings; it never schedules a
/// wakeup of its own.
const POLL: Duration = Duration::from_secs(1);

/// Which panes registered their focus target, per egui pass.
#[derive(Clone, Copy, Debug, Default)]
struct DrawnPanes {
    /// The pass `current` describes.
    pass: u64,
    current: u8,
    /// The mask of the most recent earlier pass that drew any pane. A pass
    /// that drew no pane at all (a sheet replacing the workspace, or shutdown)
    /// never replaces it.
    latest: u8,
}

fn bit(pane: Pane) -> u8 {
    match pane {
        Pane::Sources => 1,
        Pane::Viewer => 2,
        Pane::Sequence => 4,
        Pane::Inspector => 8,
        Pane::Sounds => 16,
    }
}

/// Record that `pane` drew its focus target in this pass.
pub(super) fn record_drawn_pane(context: &egui::Context, pane: Pane) {
    let pass = context.cumulative_pass_nr();
    context.data_mut(|data| {
        let drawn = data.get_temp_mut_or_default::<DrawnPanes>(egui::Id::new(DRAWN_PANES));
        if drawn.pass != pass {
            if drawn.current != 0 {
                drawn.latest = drawn.current;
            }
            drawn.current = 0;
            drawn.pass = pass;
        }
        drawn.current |= bit(pane);
    });
}

/// Whether `pane` drew its focus target in the latest earlier pass that drew
/// any pane. Keyboard routing runs before this pass's panes, so it observes the
/// previous layout. Only before the first drawn pass is every pane available.
///
/// A layout that omits a pane's focus target makes it unreachable by Tab on
/// purpose: an open Gain draft replaces the Beats outline with its owner line,
/// and the start screen has no Inspector or Placed sounds.
pub(super) fn pane_drawn(context: &egui::Context, pane: Pane) -> bool {
    let pass = context.cumulative_pass_nr();
    context.data(|data| {
        data.get_temp::<DrawnPanes>(egui::Id::new(DRAWN_PANES))
            .is_none_or(|drawn| {
                let mask = if drawn.pass != pass && drawn.current != 0 {
                    drawn.current
                } else {
                    drawn.latest
                };
                mask == 0 || mask & bit(pane) != 0
            })
    })
}

/// Whether `pane` drew its focus target earlier in the current pass.
pub(super) fn pane_drawn_this_pass(context: &egui::Context, pane: Pane) -> bool {
    let pass = context.cumulative_pass_nr();
    context.data(|data| {
        data.get_temp::<DrawnPanes>(egui::Id::new(DRAWN_PANES))
            .is_some_and(|drawn| drawn.pass == pass && drawn.current & bit(pane) != 0)
    })
}

/// Whether the focused control registered a widget in this pass.
fn drawn_this_pass(context: &egui::Context, id: egui::Id) -> bool {
    context.viewport(|viewport| viewport.this_pass.widgets.get(id).is_some())
}

/// AccessKit requires the focused node to exist in every tree update; a
/// focused widget that was not drawn this pass would otherwise abort the
/// macOS adapter. This last resort gives a remaining stale target a node; the
/// app first moves such focus to a drawn pane (`repair_focus`).
pub(super) fn guard_focus(context: &egui::Context) {
    if let Some(focused) = context.memory(|memory| memory.focused()) {
        context.accesskit_node_builder(focused, |_| ());
    }
}

/// Speak changes to a visible status. Assistive technology announces the new
/// value when it appears or changes, without moving keyboard focus.
pub(super) fn live(response: &egui::Response, urgent: bool) {
    response.ctx.accesskit_node_builder(response.id, |node| {
        node.set_live(if urgent {
            accesskit::Live::Assertive
        } else {
            accesskit::Live::Polite
        });
    });
}

/// Describe a painted pane's current content as its accessible value. When the
/// pane has focus the description is spoken as it changes, so keyboard
/// navigation inside a painted pane is heard as well as seen.
pub(super) fn describe_pane(context: &egui::Context, id: egui::Id, value: &str, speak: bool) {
    context.accesskit_node_builder(id, |node| {
        node.set_value(value);
        if speak {
            node.set_live(accesskit::Live::Polite);
        } else {
            node.clear_live();
        }
    });
}

/// Expose a truncated label's complete text. The painted galley is elided to
/// fit; the accessible text must not be.
pub(super) fn full_text(response: egui::Response, text: &str) -> egui::Response {
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, text));
    response
}

/// Name a control whose visible text is only a placeholder or nearby heading.
pub(super) fn name(response: &egui::Response, label: &str) {
    response
        .ctx
        .accesskit_node_builder(response.id, |node| node.set_label(label));
}

/// Give a slider a spoken value in its own units, in place of the raw number.
pub(super) fn spoken_value(response: &egui::Response, value: &str) {
    response
        .ctx
        .accesskit_node_builder(response.id, |node| node.set_value(value));
}

/// Expose a modal sheet as a labelled dialog, so focus entering it is heard
/// with its purpose rather than as a bare button.
pub(super) fn dialog(ui: &egui::Ui, label: &str) {
    // The sheet's measured area from the previous frame; its contents are not
    // laid out yet. Without bounds macOS reports a zero-sized dialog.
    let rect = ui
        .ctx()
        .memory(|memory| memory.area_rect(ui.layer_id().id))
        .unwrap_or_else(|| ui.max_rect());
    ui.ctx().accesskit_node_builder(ui.unique_id(), |node| {
        node.set_role(accesskit::Role::Dialog);
        node.set_modal();
        node.set_label(label);
        if rect.is_finite() {
            node.set_bounds(accesskit::Rect {
                x0: rect.min.x.into(),
                y0: rect.min.y.into(),
                x1: rect.max.x.into(),
                y1: rect.max.y.into(),
            });
        }
    });
}

/// Group the widgets of `ui` under one labelled container, so repeated control
/// names ("Install from folder…") are heard with their owner.
pub(super) fn group(ui: &egui::Ui, label: &str) {
    ui.ctx().accesskit_node_builder(ui.unique_id(), |node| {
        node.set_role(accesskit::Role::Group);
        node.set_label(label);
    });
}

/// Spoken duration for an exact frame count at a rational project rate.
pub(super) fn spoken_time(frames: u64, rate: deadpan_core::FrameRate) -> String {
    let numerator = u128::from(rate.numerator().max(1));
    let millis = u128::from(frames) * u128::from(rate.denominator()) * 1000 / numerator;
    let minutes = millis / 60_000;
    let seconds = (millis % 60_000) / 1000;
    let fraction = millis % 1000;
    if minutes > 0 {
        format!("{minutes} min {seconds}.{fraction:03} s")
    } else {
        format!("{seconds}.{fraction:03} s")
    }
}

/// The macOS display accommodations Deadpan follows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Preferences {
    pub reduce_motion: bool,
    pub increase_contrast: bool,
}

/// Current display preferences, as last applied to the egui style.
pub(super) fn preferences(context: &egui::Context) -> Preferences {
    context.data(|data| {
        data.get_temp::<Preferences>(egui::Id::new(DISPLAY))
            .unwrap_or_default()
    })
}

/// Apply display preferences on top of the workspace style.
pub(super) fn apply(context: &egui::Context, preferences: Preferences) {
    style::apply(context);
    context.data_mut(|data| data.insert_temp(egui::Id::new(DISPLAY), preferences));
    let defaults = egui::Style::default();
    context.all_styles_mut(|style| {
        style.animation_time = defaults.animation_time;
        style.scroll_animation = defaults.scroll_animation;
        style.visuals.text_cursor.blink = defaults.visuals.text_cursor.blink;
        if preferences.reduce_motion {
            // Collapsing headers, windows and scroll reveals jump instead of
            // animating; the text cursor stops blinking.
            style.animation_time = 0.0;
            style.scroll_animation = egui::style::ScrollAnimation::none();
            style.visuals.text_cursor.blink = false;
        }
        if preferences.increase_contrast {
            let border = Stroke::new(1.5, style::HIGH_CONTRAST_BORDER);
            style.visuals.weak_text_color = Some(style::TEXT);
            style.visuals.window_stroke = border;
            style.visuals.selection.stroke = Stroke::new(2.5, style::LAVENDER);
            for widget in [
                &mut style.visuals.widgets.noninteractive,
                &mut style.visuals.widgets.inactive,
                &mut style.visuals.widgets.open,
            ] {
                widget.bg_stroke = border;
            }
            // Focus and keyboard activation use the active visuals; make that
            // indicator thicker and distinct from hover.
            style.visuals.widgets.hovered.bg_stroke = Stroke::new(2.0, style::LAVENDER);
            style.visuals.widgets.active.bg_stroke = Stroke::new(3.0, style::CURSOR);
        }
    });
}

/// A busy indicator. With Reduce motion it is a static ellipsis rather than a
/// continuously rotating spinner; the adjacent text names the work.
pub(super) fn busy(ui: &mut egui::Ui) {
    if preferences(ui.ctx()).reduce_motion {
        ui.label(egui::RichText::new("…").weak())
            .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, "Busy"));
    } else {
        ui.spinner();
    }
}

/// The border colour of painted cards and panes, raised under Increase contrast.
pub(super) fn border(context: &egui::Context) -> Color32 {
    context
        .global_style()
        .visuals
        .widgets
        .noninteractive
        .bg_stroke
        .color
}

/// Follows the system display preferences in the native app. Replays and
/// tests keep fixed preferences so personal settings never change a result.
#[derive(Debug)]
pub(super) struct Display {
    follow_system: bool,
    checked: Option<Instant>,
    applied: Option<Preferences>,
    focused: bool,
}

impl Display {
    pub(super) fn fixed() -> Self {
        Self {
            follow_system: false,
            checked: None,
            applied: None,
            focused: false,
        }
    }

    pub(super) fn follow_system(&mut self) {
        self.follow_system = true;
        self.checked = None;
    }

    /// Re-read the system preferences when the window gains focus or at most
    /// once per [`POLL`] on an ordinary frame, and restyle only when they
    /// change. Schedules no repaint of its own.
    pub(super) fn poll(&mut self, context: &egui::Context) {
        if !self.follow_system {
            return;
        }
        let now = Instant::now();
        let focused = context.input(|input| input.viewport().focused.unwrap_or(true));
        let regained = focused && !self.focused;
        self.focused = focused;
        if !regained
            && self
                .checked
                .is_some_and(|checked| now.duration_since(checked) < POLL)
        {
            return;
        }
        self.checked = Some(now);
        let current = system_preferences();
        if self.applied != Some(current) {
            self.applied = Some(current);
            apply(context, current);
            context.request_repaint();
        }
    }
}

#[cfg(target_os = "macos")]
fn system_preferences() -> Preferences {
    let workspace = objc2_app_kit::NSWorkspace::sharedWorkspace();
    Preferences {
        reduce_motion: workspace.accessibilityDisplayShouldReduceMotion(),
        increase_contrast: workspace.accessibilityDisplayShouldIncreaseContrast(),
    }
}

#[cfg(not(target_os = "macos"))]
fn system_preferences() -> Preferences {
    Preferences::default()
}

impl super::DeadpanApp {
    /// Move a focus that names an undrawn pane to the current pane, or the
    /// nearest drawn one, so focus never rests on a missing control. The new
    /// focus is announced like any focus change. Discarded layout passes are
    /// left alone: their successor draws the pane.
    pub(super) fn repair_focus(&mut self, context: &egui::Context) {
        let Some(focused) = context.memory(|memory| memory.focused()) else {
            return;
        };
        let panes = [
            Pane::Sources,
            Pane::Viewer,
            Pane::Sequence,
            Pane::Inspector,
            Pane::Sounds,
        ];
        if context.will_discard()
            || drawn_this_pass(context, focused)
            || !panes
                .into_iter()
                .any(|pane| super::pane_id(pane) == focused)
        {
            return;
        }
        let Some(pane) = [self.pane, Pane::Viewer, Pane::Sequence, Pane::Sources]
            .into_iter()
            .chain(panes)
            .find(|pane| pane_drawn_this_pass(context, *pane))
        else {
            return;
        };
        self.pane = pane;
        context.memory_mut(|memory| memory.request_focus(super::pane_id(pane)));
    }

    /// Give each drawn pane focus target a spoken summary of what it shows.
    /// The current pane speaks changes, except during playback or while a
    /// native control has focus. The pane, not egui focus, decides: a closed
    /// sheet can leave native focus empty while the keyboard drives the pane.
    pub(super) fn describe_panes(&self, context: &egui::Context) {
        // A focused field, button or sheet control speaks for itself. While a
        // key is held (repeating motion), values change silently; the final
        // position is announced once when the key is released.
        let speak = self.transport.is_none()
            && !self.command_open
            && !super::native_control_focused(context)
            && context.input(|input| input.keys_down.is_empty());
        for pane in [
            Pane::Sources,
            Pane::Viewer,
            Pane::Sequence,
            Pane::Inspector,
            Pane::Sounds,
        ] {
            if !pane_drawn_this_pass(context, pane) {
                continue;
            }
            describe_pane(
                context,
                super::pane_id(pane),
                &self.pane_summary(pane),
                speak && self.pane == pane,
            );
        }
    }

    fn selected_beat_summary(&self) -> Option<String> {
        let selected = self.selected_beat.as_ref()?;
        let index = self.beat_rows.iter().position(|row| &row.id == selected)?;
        let beat = &self.beat_rows[index];
        let time = self
            .workspace
            .as_ref()
            .map_or_else(String::new, |workspace| {
                let rate = workspace.document.presentation_basis().frame_rate;
                format!(" ({})", spoken_time(beat.frames, rate))
            });
        Some(format!(
            "Beat {} of {}: {}, {}, {} frames{time}, boundaries {} to {}",
            index + 1,
            self.beat_rows.len(),
            beat.label,
            beat.kind,
            beat.frames,
            beat.start,
            beat.start + beat.frames,
        ))
    }

    fn viewer_summary(&self) -> String {
        if self.workspace.is_none() && self.raw_source.is_none() {
            return "No project open. Command-N chooses a video, Command-Shift-N starts from a YouTube URL, Command-O opens a project.".into();
        }
        let mut parts = Vec::new();
        if self.camera.is_some() {
            parts.push("Camera draft, unsaved. Enter applies, Escape cancels".to_owned());
        }
        match self.view {
            super::View::Source => {
                let length = self.source_length();
                parts.push(format!(
                    "Original, frame {} of {length}",
                    (self.source_cursor + 1).min(length.max(1))
                ));
            }
            super::View::Sequence => {
                let length = self.sequence_length();
                if length == 0 {
                    parts.push("Your edit is empty".into());
                } else {
                    let mut position = format!(
                        "Your edit, frame {} of {length}",
                        (self.sequence_cursor + 1).min(length)
                    );
                    if let Some(workspace) = &self.workspace {
                        let rate = workspace.document.presentation_basis().frame_rate;
                        position.push_str(&format!(
                            ", {} of {}",
                            spoken_time(self.sequence_cursor, rate),
                            spoken_time(length, rate)
                        ));
                    }
                    parts.push(position);
                    if let Some(beat) = self.selected_beat_summary() {
                        parts.push(beat);
                    }
                }
            }
        }
        // Picture loading is transient; it is not part of the spoken summary,
        // which would otherwise be announced twice per step.
        parts.join(". ")
    }

    fn pane_summary(&self, pane: Pane) -> String {
        match pane {
            Pane::Viewer => self.viewer_summary(),
            Pane::Sequence => self.selected_beat_summary().unwrap_or_else(|| {
                if self.beat_rows.is_empty() {
                    "No beats".into()
                } else {
                    format!("{} beats, none selected", self.beat_rows.len())
                }
            }),
            Pane::Inspector => self.selected_beat_summary().map_or_else(
                || "Nothing selected".into(),
                |beat| format!("Inspecting {beat}"),
            ),
            Pane::Sources if self.workspace.is_none() && self.raw_source.is_none() => {
                "No Original yet. Choose one video to begin".into()
            }
            Pane::Sources => match self.view {
                super::View::Source => {
                    let length = self.source_length();
                    format!(
                        "Browsing the unchanged Original, frame {} of {length}",
                        (self.source_cursor + 1).min(length.max(1))
                    )
                }
                super::View::Sequence => "Original, reuse, sound effects and transcript".into(),
            },
            Pane::Sounds => {
                let count = self
                    .workspace
                    .as_ref()
                    .map_or(0, |workspace| workspace.document.sounds().len());
                match count {
                    0 => "No placed sounds".into(),
                    1 => "1 placed sound".into(),
                    count => format!("{count} placed sounds"),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(context: &egui::Context, draw: impl FnMut(&mut egui::Ui)) -> egui::FullOutput {
        let mut draw = draw;
        let mut output = context.run_ui(egui::RawInput::default(), |ui| draw(ui));
        output.textures_delta.clear();
        output
    }

    fn tree(output: &egui::FullOutput) -> &accesskit::TreeUpdate {
        output.platform_output.accesskit_update.as_ref().unwrap()
    }

    #[test]
    fn a_focused_widget_that_was_not_drawn_still_has_a_node() {
        let context = egui::Context::default();
        context.enable_accesskit();
        let hidden = egui::Id::new("hidden-pane");
        // Keyboard routing moves focus to a pane before panes are drawn; this
        // layout never draws it.
        let output = run(&context, |ui| {
            ui.memory_mut(|memory| memory.request_focus(hidden));
            ui.label("visible");
            guard_focus(ui.ctx());
        });
        let update = tree(&output);
        assert_eq!(update.focus, hidden.accesskit_id());
        assert!(
            update
                .nodes
                .iter()
                .any(|(id, _)| *id == hidden.accesskit_id()),
            "the focused id must be in the node list, or the platform adapter panics"
        );
    }

    #[test]
    fn statuses_are_live_regions_and_pane_values_speak_only_when_asked() {
        let context = egui::Context::default();
        context.enable_accesskit();
        let pane = egui::Id::new("pane");
        let mut label = None;
        let output = run(&context, |ui| {
            let response = ui.label("Saved the edit");
            live(&response, false);
            label = Some(response.id);
            let error = ui.label("Could not complete action");
            live(&error, true);
            ui.interact(ui.max_rect(), pane, egui::Sense::click());
            describe_pane(ui.ctx(), pane, "Beat 2 of 3", true);
        });
        let update = tree(&output);
        let node = |id: egui::Id| {
            update
                .nodes
                .iter()
                .find(|(node, _)| *node == id.accesskit_id())
                .map(|(_, node)| node.clone())
                .unwrap()
        };
        let status = node(label.unwrap());
        assert_eq!(status.live(), Some(accesskit::Live::Polite));
        assert_eq!(status.value(), Some("Saved the edit"));
        assert!(update.nodes.iter().any(|(_, node)| {
            node.live() == Some(accesskit::Live::Assertive)
                && node.value() == Some("Could not complete action")
        }));
        let described = node(pane);
        assert_eq!(described.value(), Some("Beat 2 of 3"));
        assert_eq!(described.live(), Some(accesskit::Live::Polite));
        let output = run(&context, |ui| {
            ui.interact(ui.max_rect(), pane, egui::Sense::click());
            describe_pane(ui.ctx(), pane, "Beat 3 of 3", false);
        });
        let silent = tree(&output)
            .nodes
            .iter()
            .find(|(id, _)| *id == pane.accesskit_id())
            .map(|(_, node)| node.live())
            .unwrap();
        assert_eq!(silent, None);
    }

    #[test]
    fn truncated_labels_expose_their_complete_text() {
        let context = egui::Context::default();
        context.enable_accesskit();
        let text = "A long beat name that cannot fit in a narrow inspector column at all";
        let mut id = None;
        let output = run(&context, |ui| {
            ui.set_max_width(60.0);
            let response = ui.add(egui::Label::new(text).truncate());
            id = Some(full_text(response, text).id);
        });
        let node = tree(&output)
            .nodes
            .iter()
            .find(|(node, _)| *node == id.unwrap().accesskit_id())
            .map(|(_, node)| node.clone())
            .unwrap();
        assert_eq!(node.value(), Some(text));
    }

    #[test]
    fn drawn_panes_are_reported_for_the_following_pass() {
        let context = egui::Context::default();
        assert!(
            pane_drawn(&context, Pane::Sounds),
            "unknown layout allows every pane"
        );
        run(&context, |ui| {
            record_drawn_pane(ui.ctx(), Pane::Viewer);
            record_drawn_pane(ui.ctx(), Pane::Sequence);
            assert!(pane_drawn_this_pass(ui.ctx(), Pane::Viewer));
            assert!(!pane_drawn_this_pass(ui.ctx(), Pane::Sounds));
        });
        run(&context, |ui| {
            // Routing precedes drawing: the previous pass decides.
            assert!(pane_drawn(ui.ctx(), Pane::Sequence));
            assert!(!pane_drawn(ui.ctx(), Pane::Sounds));
            // An open Gain draft draws its owner line instead of the Beats
            // outline: Beats is deliberately unreachable by Tab next pass.
            record_drawn_pane(ui.ctx(), Pane::Viewer);
            assert!(pane_drawn(ui.ctx(), Pane::Sequence));
            assert!(!pane_drawn_this_pass(ui.ctx(), Pane::Sequence));
        });
        run(&context, |ui| {
            assert!(!pane_drawn(ui.ctx(), Pane::Sequence));
            assert!(pane_drawn(ui.ctx(), Pane::Viewer));
        });
        // A pass that draws no pane at all keeps the latest real layout; it
        // must not make every pane available again.
        run(&context, |_| {});
        run(&context, |ui| {
            assert!(!pane_drawn(ui.ctx(), Pane::Sequence));
            assert!(!pane_drawn(ui.ctx(), Pane::Sounds));
            assert!(pane_drawn(ui.ctx(), Pane::Viewer));
        });
    }

    #[test]
    fn reduce_motion_and_increase_contrast_change_the_style() {
        let context = egui::Context::default();
        apply(&context, Preferences::default());
        let standard = context.global_style();
        assert!(standard.animation_time > 0.0);
        assert!(!preferences(&context).reduce_motion);
        apply(
            &context,
            Preferences {
                reduce_motion: true,
                increase_contrast: true,
            },
        );
        let accommodated = context.global_style();
        assert_eq!(accommodated.animation_time, 0.0);
        assert_eq!(
            accommodated.scroll_animation,
            egui::style::ScrollAnimation::none()
        );
        assert!(!accommodated.visuals.text_cursor.blink);
        assert_eq!(
            accommodated.visuals.widgets.noninteractive.bg_stroke.color,
            style::HIGH_CONTRAST_BORDER
        );
        assert!(
            accommodated.visuals.widgets.active.bg_stroke.width
                > standard.visuals.widgets.active.bg_stroke.width
        );
        assert_eq!(accommodated.visuals.weak_text_color, Some(style::TEXT));
        assert!(preferences(&context).reduce_motion);
        // Turning the preferences off restores the workspace style.
        apply(&context, Preferences::default());
        let restored = context.global_style();
        assert_eq!(restored.animation_time, standard.animation_time);
        assert_eq!(
            restored.visuals.widgets.noninteractive.bg_stroke,
            standard.visuals.widgets.noninteractive.bg_stroke
        );
    }

    #[test]
    fn reduce_motion_replaces_the_spinner_with_a_static_busy_label() {
        let context = egui::Context::default();
        context.enable_accesskit();
        for reduce_motion in [false, true] {
            apply(
                &context,
                Preferences {
                    reduce_motion,
                    increase_contrast: false,
                },
            );
            let output = run(&context, busy);
            let labelled = tree(&output)
                .nodes
                .iter()
                .any(|(_, node)| node.value() == Some("Busy"));
            assert_eq!(labelled, reduce_motion);
        }
    }

    #[test]
    fn high_contrast_border_meets_the_non_text_contrast_minimum() {
        fn luminance(color: Color32) -> f64 {
            let channel = |value: u8| {
                let value = f64::from(value) / 255.0;
                if value <= 0.04045 {
                    value / 12.92
                } else {
                    ((value + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * channel(color.r()) + 0.7152 * channel(color.g()) + 0.0722 * channel(color.b())
        }
        let ratio = |a: Color32, b: Color32| {
            let (a, b) = (luminance(a), luminance(b));
            (a.max(b) + 0.05) / (a.min(b) + 0.05)
        };
        assert!(ratio(style::HIGH_CONTRAST_BORDER, style::CANVAS) >= 3.0);
        assert!(ratio(style::HIGH_CONTRAST_BORDER, style::PANEL) >= 3.0);
        assert!(ratio(style::MUTED, style::CANVAS) >= 4.5);
        assert!(ratio(style::LAVENDER, style::CANVAS) >= 3.0);
    }

    #[test]
    fn spoken_time_is_exact_at_rational_rates() {
        let ntsc = deadpan_core::FrameRate::new(30_000, 1001).unwrap();
        assert_eq!(spoken_time(0, ntsc), "0.000 s");
        assert_eq!(spoken_time(30, ntsc), "1.001 s");
        let film = deadpan_core::FrameRate::new(24, 1).unwrap();
        assert_eq!(spoken_time(24 * 62 + 12, film), "1 min 2.500 s");
    }
}
