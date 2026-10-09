//! Attention targets in the native workspace: what saved targets show at the
//! current picture, the `:track` commands, the background tracking status and
//! the inspector's Targets list. Camera owns picking, following, keyboard
//! rectangles and corrections ([`super::camera`]).

use std::time::Duration;

use deadpan_core::{AttentionTarget, TargetId, TargetRegion, TargetSource, TrackState};

use super::*;
use crate::project::targets::{Job, Operation, Outcome, Saved, TrackMode, Update};

#[derive(Default)]
pub(super) struct State {
    update: Option<Update>,
    ticket: u64,
    /// The independent command whose refusal the editor should show.
    awaiting: Option<u64>,
    /// Context captured when command entry opened.
    pub command: Option<Result<Capture, String>>,
    /// `:zoom … target=face:N` waiting for its face proposals.
    pub face_zoom: Option<super::camera::FaceZoom>,
}

/// The session, revision and followed target a `:track` command was entered
/// with. Absence is a real result: a later selection cannot supply it.
#[derive(Clone, Debug)]
pub(super) struct Capture {
    session: u64,
    revision: RevisionId,
    followed: Option<TargetId>,
}

impl State {
    #[cfg(feature = "ui-harness")]
    pub(super) fn pending_for_check(&self) -> bool {
        self.awaiting.is_some()
    }

    fn next_ticket(&mut self) -> u64 {
        self.ticket = self.ticket.wrapping_add(1).max(1);
        self.ticket
    }

    pub(super) fn job(&self) -> Option<&Job> {
        self.update.as_ref()?.job.as_ref()
    }

    pub(super) fn running(&self) -> Option<&Job> {
        self.job().filter(|job| job.running())
    }

    pub(super) fn saved(&self) -> Option<&Saved> {
        self.update.as_ref()?.saved.as_ref()
    }

    /// The latest face detection of this session.
    pub(super) fn faces(&self) -> Option<&crate::project::targets::FaceJob> {
        self.update.as_ref()?.faces.as_ref()
    }

    /// The latest independent reply: its ticket and refusal.
    pub(super) fn reply(&self) -> Option<&(u64, Option<String>)> {
        self.update.as_ref()?.reply.as_ref()
    }
}

/// A saved target as it is at the current picture.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Shown {
    pub id: TargetId,
    pub label: String,
    pub region: TargetRegion,
    pub source: TargetSource,
}

/// How a target got its region at a picture, in a few words.
pub(super) fn source_label(source: TargetSource) -> &'static str {
    match source {
        TargetSource::Initial => "drawn",
        TargetSource::Manual => "corrected",
        TargetSource::Tracked(TrackState::Tracked) => "tracked",
        TargetSource::Tracked(TrackState::Interpolated) => "interpolated",
        TargetSource::Tracked(TrackState::Lost) => "lost · holding",
    }
}

/// The overlay color of a target's rectangle.
pub(super) fn source_color(source: TargetSource) -> egui::Color32 {
    match source {
        TargetSource::Tracked(TrackState::Lost) => style::WARNING,
        TargetSource::Tracked(_) => style::SAVED,
        TargetSource::Initial | TargetSource::Manual => style::CURSOR,
    }
}

/// One line about a target's whole span and tracking.
pub(super) fn summary(target: &AttentionTarget) -> String {
    if target.samples.is_empty() {
        if target.corrections.is_empty() {
            "Drawn · not tracked".into()
        } else {
            format!("Drawn · {}", corrections(target.corrections.len()))
        }
    } else {
        let lost = target
            .samples
            .iter()
            .filter(|sample| sample.state == TrackState::Lost)
            .count();
        let mut text = format!("Tracked · {} positions", target.samples.len());
        if lost > 0 {
            text.push_str(&format!(" · {lost} lost"));
        }
        if !target.corrections.is_empty() {
            text.push_str(&format!(" · {}", corrections(target.corrections.len())));
        }
        text
    }
}

fn corrections(count: usize) -> String {
    if count == 1 {
        "1 correction".into()
    } else {
        format!("{count} corrections")
    }
}

fn elapsed(job: &Job) -> String {
    let seconds = job.started.elapsed().as_secs();
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// `region` as `x, y, w, h` percentages of the picture, center first.
pub(super) fn region_text(region: &TargetRegion) -> [String; 4] {
    let percent = |value: u32| format!("{:.1}%", f64::from(value) / 10_000.0);
    [
        percent(region.center[0]),
        percent(region.center[1]),
        percent(region.size[0]),
        percent(region.size[1]),
    ]
}

impl DeadpanApp {
    /// Saved targets covering the current picture, in id order.
    pub(super) fn targets_at_picture(&self) -> Vec<Shown> {
        let (Some(workspace), Some(picture)) = (&self.workspace, self.presentation.picture())
        else {
            return Vec::new();
        };
        let Some((asset, point)) = &picture.follow_point else {
            return Vec::new();
        };
        workspace
            .document
            .targets()
            .iter()
            .filter(|(_, target)| &target.asset == asset)
            .filter_map(|(id, target)| {
                let (region, source) = target.region_at(*point)?;
                Some(Shown {
                    id: id.clone(),
                    label: target.label.clone(),
                    region,
                    source,
                })
            })
            .collect()
    }

    pub(super) fn target_label(&self, id: &TargetId) -> String {
        self.workspace
            .as_ref()
            .and_then(|workspace| workspace.document.targets().get(id))
            .map_or_else(|| id.as_str().to_owned(), |target| target.label.clone())
    }

    /// Submit a target command. Unlike ordinary edits this keeps Camera, its
    /// draft and playback state: Camera continues on the saved revision.
    pub(super) fn submit_target(&mut self, operation: Operation) -> Option<u64> {
        let request = ProjectRequest::Target(operation.clone());
        if !self.macro_request_allowed(&request) {
            return None;
        }
        let ticket = self.targets.next_ticket();
        let operation = match operation {
            Operation::Save {
                session,
                revision,
                id,
                target,
                ..
            } => Operation::Save {
                ticket,
                session,
                revision,
                id,
                target,
            },
            Operation::Track {
                session,
                revision,
                id,
                mode,
                ..
            } => Operation::Track {
                ticket,
                session,
                revision,
                id,
                mode,
            },
            Operation::Cancel { session, job, .. } => Operation::Cancel {
                ticket,
                session,
                job,
            },
            Operation::DetectFaces {
                session,
                revision,
                asset,
                pts,
                ..
            } => Operation::DetectFaces {
                ticket,
                session,
                revision,
                asset,
                pts,
            },
            Operation::SaveFramed {
                session,
                revision,
                scope,
                cursor,
                node,
                id,
                target,
                framing,
                ..
            } => Operation::SaveFramed {
                ticket,
                session,
                revision,
                scope,
                cursor,
                node,
                id,
                target,
                framing,
            },
        };
        match self.service.submit(ProjectRequest::Target(operation)) {
            Ok(()) => {
                self.error = None;
                self.targets.awaiting = Some(ticket);
                Some(ticket)
            }
            Err(error) => {
                self.error = Some(error);
                None
            }
        }
    }

    /// Admit the service's target state; show a refusal of our command.
    pub(super) fn receive_targets(&mut self, update: Option<Update>) {
        let session = self.workspace.as_ref().map(|workspace| workspace.session);
        self.targets.update = update.filter(|update| Some(update.session) == session);
        let Some((answered, refusal)) = self.targets.reply().cloned() else {
            self.finish_face_zoom();
            return;
        };
        // One service mailbox serializes commands: a later reply answers
        // every earlier one.
        if self
            .targets
            .awaiting
            .is_some_and(|awaiting| answered >= awaiting)
        {
            if self.targets.awaiting == Some(answered)
                && let Some(refusal) = &refusal
            {
                self.error = Some(refusal.clone());
            }
            self.targets.awaiting = None;
            self.camera_target_reply(answered, refusal.as_deref());
        }
        self.finish_face_zoom();
    }

    /// The context `:track` acts on, captured at command entry.
    pub(super) fn capture_track(&self) -> Result<Capture, String> {
        let workspace = self
            .workspace
            .as_ref()
            .ok_or("Open a project before tracking a target.")?;
        let followed = (self.view == View::Sequence && self.scoped.is_none())
            .then_some(self.selected_beat.as_ref())
            .flatten()
            .and_then(|node| workspace.document.nodes().get(node))
            .and_then(|node| match &node.framing {
                Some(deadpan_core::Framing {
                    value: deadpan_core::FramingValue::Follow { target, .. },
                    ..
                }) => Some(target.clone()),
                _ => None,
            });
        Ok(Capture {
            session: workspace.session,
            revision: workspace.document.revision_id().clone(),
            followed,
        })
    }

    /// `:track [ID] [through-shots]`: track a saved target in the background.
    pub(super) fn track_command(
        &mut self,
        capture: Option<Result<Capture, String>>,
        target: Option<String>,
        through_shots: bool,
    ) {
        let result = (|| {
            let capture = capture.ok_or("Enter :track again to capture its target.")??;
            let id = match target {
                Some(text) => self.resolve_target(&text)?,
                None => capture.followed.ok_or(
                    "Name the target, for example :track target-1, or select a beat that follows one.",
                )?,
            };
            self.start_tracking(capture.session, capture.revision, id, through_shots)
        })();
        if let Err(error) = result {
            self.error = Some(error);
        }
    }

    /// A target by id, or by its label ignoring case.
    pub(super) fn resolve_target(&self, text: &str) -> Result<TargetId, String> {
        let workspace = self.workspace.as_ref().ok_or("Open a project first.")?;
        let targets = workspace.document.targets();
        if let Some((id, _)) = targets.iter().find(|(id, _)| id.as_str() == text) {
            return Ok(id.clone());
        }
        let matching: Vec<_> = targets
            .iter()
            .filter(|(_, target)| target.label.eq_ignore_ascii_case(text))
            .collect();
        match matching.as_slice() {
            [(id, _)] => Ok((*id).clone()),
            [] => Err(format!("There is no saved target {text}.")),
            _ => Err(format!("Several targets are labelled {text}; use its id.")),
        }
    }

    pub(super) fn start_tracking(
        &mut self,
        session: u64,
        revision: RevisionId,
        id: TargetId,
        through_shots: bool,
    ) -> Result<(), String> {
        if self.targets.running().is_some() {
            return Err("A target is already tracking. Cancel it with :track-cancel first.".into());
        }
        self.submit_target(Operation::Track {
            ticket: 0,
            session,
            revision,
            id,
            mode: TrackMode::Track { through_shots },
        });
        Ok(())
    }

    /// `:track-cancel`.
    pub(super) fn track_cancel(&mut self) {
        let Some(job) = self.targets.running().map(|job| job.ticket) else {
            self.error = Some("No target is tracking.".into());
            return;
        };
        let Some(session) = self.workspace.as_ref().map(|workspace| workspace.session) else {
            return;
        };
        self.submit_target(Operation::Cancel {
            ticket: 0,
            session,
            job,
        });
    }

    /// Tracking status for the inspector and footer.
    pub(super) fn tracking_line(job: &Job) -> String {
        format!(
            "{} {} · {} · {}",
            if job.correction {
                "Re-tracking"
            } else {
                "Tracking"
            },
            job.label,
            job.phase.label(),
            elapsed(job)
        )
    }

    /// A status row while a tracking job runs.
    pub(super) fn tracking_footer(&self, ui: &mut egui::Ui) {
        let Some(job) = self.targets.running() else {
            return;
        };
        ui.horizontal_wrapped(|ui| {
            crate::preview::accessibility::busy(ui);
            ui.colored_label(style::LAVENDER, Self::tracking_line(job));
            style::key_hint(ui, ":track-cancel", "cancel");
        });
        ui.ctx().request_repaint_after(Duration::from_millis(250));
    }

    /// The latest job's status lines: progress while running, else its
    /// outcome. Shared by the inspector and Camera.
    pub(super) fn tracking_status(&mut self, ui: &mut egui::Ui) {
        let Some(job) = self.targets.job().cloned() else {
            return;
        };
        if job.running() {
            // Wrap: a long label must never widen the fixed inspector.
            ui.horizontal_wrapped(|ui| {
                crate::preview::accessibility::busy(ui);
                ui.label(Self::tracking_line(&job));
            });
            if let crate::project::targets::Phase::Tracking(percent) = job.phase {
                ui.add(
                    egui::ProgressBar::new(f32::from(percent) / 100.0)
                        .desired_height(4.0)
                        .fill(style::LAVENDER),
                );
            }
            if ui
                .add_enabled(
                    !self.service.is_busy(),
                    style::row_action(ui, "Cancel tracking", ":track-cancel"),
                )
                .clicked()
            {
                self.track_cancel();
            }
            ui.ctx().request_repaint_after(Duration::from_millis(250));
            return;
        }
        let (color, text) = match job.outcome.as_ref() {
            Some(Outcome::Saved { samples, .. }) => (
                style::SAVED,
                format!("{} tracked · {samples} positions saved", job.label),
            ),
            Some(Outcome::Cancelled) => (
                style::muted(ui),
                format!("Tracking {} cancelled", job.label),
            ),
            Some(Outcome::Failed(reason)) => (
                style::ERROR,
                format!("Tracking {} failed: {reason}", job.label),
            ),
            Some(Outcome::Unavailable(reason)) => (style::WARNING, reason.clone()),
            None => return,
        };
        ui.label(egui::RichText::new(text).size(12.0).color(color));
    }

    /// The Targets list in the beat inspector.
    pub(super) fn targets_inspector(&mut self, ui: &mut egui::Ui) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let targets: Vec<(TargetId, String, String)> = workspace
            .document
            .targets()
            .iter()
            .map(|(id, target)| (id.clone(), target.label.clone(), summary(target)))
            .collect();
        let shown: std::collections::BTreeMap<TargetId, TargetSource> = self
            .targets_at_picture()
            .into_iter()
            .map(|shown| (shown.id, shown.source))
            .collect();
        ui.add_space(8.0);
        ui.label(style::section_title("TARGETS", false));
        if targets.is_empty() {
            ui.label(
                egui::RichText::new(format!(
                    "No saved targets. In Camera ({}), press n to draw one on the picture.",
                    self.editor_key(EditorKey::Camera)
                ))
                .size(12.0)
                .weak(),
            );
        }
        for (id, label, summary) in &targets {
            ui.horizontal_wrapped(|ui| {
                ui.label(style::semibold(label));
                ui.label(
                    egui::RichText::new(id.as_str())
                        .monospace()
                        .size(11.0)
                        .weak(),
                );
                if let Some(source) = shown.get(id) {
                    ui.label(
                        egui::RichText::new(source_label(*source))
                            .size(12.0)
                            .color(source_color(*source)),
                    );
                }
            });
            ui.label(egui::RichText::new(summary).size(12.0).weak());
        }
        if !targets.is_empty() {
            ui.horizontal_wrapped(|ui| {
                style::key_hint(ui, self.editor_key(EditorKey::Camera).as_str(), "Camera");
                style::key_hint(ui, "f", "pick");
                style::key_hint(ui, "t", "follow");
                style::key_hint(ui, ":track ID", "track");
            });
        }
        self.tracking_status(ui);
    }
}
