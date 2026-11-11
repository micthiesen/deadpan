//! What the viewer shows while a new project's Original is prepared: the
//! file, each step with its state, how long the current step and the whole
//! preparation have taken, and how to cancel. The import worker reports
//! stages, not fractions, so the bar runs without inventing a percentage.

use std::time::Instant;

use super::*;

/// When the current import and its current stage began, by import path.
#[derive(Default)]
pub(super) struct Timing {
    started: Option<(PathBuf, Instant)>,
    /// The file's size, read once when its import starts.
    size: Option<u64>,
    stage: Option<(ImportStage, Instant)>,
}

const STEPS: [(ImportStage, &str, &str); 4] = [
    (
        ImportStage::Retaining,
        "Keep a copy",
        "The video is copied into the project, so the Original never changes.",
    ),
    (
        ImportStage::Decoding,
        "Measure every picture and sound",
        "Each picture and audio packet is timed exactly. A long video takes a minute or more.",
    ),
    (
        ImportStage::PreparingInsertion,
        "Place the whole video",
        "The full video becomes Your edit's starting point.",
    ),
    (
        ImportStage::Registering,
        "Save",
        "The measured Original is saved with the project.",
    ),
];

fn clock(seconds: u64) -> String {
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

impl DeadpanApp {
    /// An import that is still working, if any.
    pub(super) fn preparing(&self) -> Option<&ImportStatus> {
        self.import.as_ref().filter(|import| {
            import.error.is_none()
                && !matches!(
                    import.stage,
                    ImportStage::Complete | ImportStage::Cancelled | ImportStage::Failed
                )
        })
    }

    /// Paint the preparation card over the viewer.
    pub(super) fn preparation_card(&mut self, ui: &mut egui::Ui, rect: egui::Rect) {
        let Some(import) = self.preparing().cloned() else {
            self.preparation = Timing::default();
            return;
        };
        let now = Instant::now();
        if self
            .preparation
            .started
            .as_ref()
            .is_none_or(|(path, _)| path != &import.path)
        {
            self.preparation.started = Some((import.path.clone(), now));
            self.preparation.size = std::fs::metadata(&import.path)
                .ok()
                .map(|metadata| metadata.len());
            self.preparation.stage = None;
        }
        if self
            .preparation
            .stage
            .is_none_or(|(stage, _)| stage != import.stage)
        {
            self.preparation.stage = Some((import.stage, now));
        }
        let total = self
            .preparation
            .started
            .as_ref()
            .map_or(0, |(_, at)| at.elapsed().as_secs());
        let step = self
            .preparation
            .stage
            .map_or(0, |(_, at)| at.elapsed().as_secs());
        let current = STEPS
            .iter()
            .position(|(stage, ..)| *stage == import.stage)
            .unwrap_or(0);
        let name = import.path.file_name().map_or_else(
            || import.path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        let size = self.preparation.size;
        let width = (rect.width() - 32.0).clamp(240.0, 520.0);
        let card = egui::Rect::from_center_size(
            rect.center(),
            egui::vec2(width, rect.height().min(330.0) - 16.0),
        );
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(card)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        egui::Frame::new()
            .fill(style::PANEL)
            .stroke(egui::Stroke::new(1.0, accessibility::border(ui.ctx())))
            .corner_radius(6)
            .inner_margin(egui::Margin::same(20))
            .show(&mut child, |ui| {
                ui.set_width(width - 42.0);
                ui.spacing_mut().item_spacing.y = 6.0;
                ui.label(style::section_title("PREPARING THE ORIGINAL", true));
                ui.add(egui::Label::new(style::semibold(&name).size(17.0)).truncate());
                ui.label(
                    egui::RichText::new(format!(
                        "{}Elapsed {}",
                        size.map_or_else(String::new, |bytes| format!(
                            "{} · ",
                            crate::model_packs::format_bytes(bytes)
                        )),
                        clock(total)
                    ))
                    .monospace()
                    .size(12.0)
                    .weak(),
                );
                ui.add_space(4.0);
                for (index, (_, title, detail)) in STEPS.iter().enumerate() {
                    ui.horizontal(|ui| {
                        let (mark, color) = match index.cmp(&current) {
                            std::cmp::Ordering::Less => ("✓", style::SAVED),
                            std::cmp::Ordering::Equal => ("●", style::CURSOR),
                            std::cmp::Ordering::Greater => ("○", style::muted(ui)),
                        };
                        ui.label(egui::RichText::new(mark).color(color));
                        let title = egui::RichText::new(*title);
                        ui.label(if index == current {
                            title.strong()
                        } else if index < current {
                            title
                        } else {
                            title.weak()
                        });
                        if index == current {
                            ui.label(
                                egui::RichText::new(clock(step))
                                    .monospace()
                                    .size(12.0)
                                    .color(style::CURSOR),
                            );
                            accessibility::busy(ui);
                        }
                    });
                    if index == current {
                        ui.add(
                            egui::Label::new(egui::RichText::new(*detail).size(12.0).weak()).wrap(),
                        );
                    }
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Cancel import").clicked() {
                        self.submit(ProjectRequest::CancelImport);
                    }
                    ui.label(
                        egui::RichText::new("The app stays usable while this runs.")
                            .size(12.0)
                            .weak(),
                    );
                });
            });
        // Elapsed time advances once a second without input.
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(500));
    }
}
